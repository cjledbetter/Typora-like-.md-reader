//! Direct file reading and atomic writing engine for Typora-RustReader.
//!
//! Features:
//! - Direct file reading with lossless/lossy UTF-8 decoding (`read_file_lossy`)
//! - Metadata extraction: filename, path, byte size, modification timestamp (`DocumentContent`)
//! - Crash-safe atomic file writing (`atomic_write_file`) via:
//!   1. Unique temporary file creation in same parent directory (`.<filename>.tmp.<pid>.<nanos>.<counter>`)
//!   2. Permissions propagation from original file if existing (Unix)
//!   3. `write_all()`, `flush()`, and `sync_all()` for durability
//!   4. Closed file handle prior to atomic rename
//!   5. Atomic filesystem replace `rename()`
//!   6. Parent directory sync for durability on journaled filesystems
//!   7. Returns new `mtime` in seconds since UNIX epoch
//!   8. RAII cleanup guard guaranteeing no orphan temp files on error

use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Represents a loaded Markdown document and its filesystem metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentContent {
    /// Canonical or relative path on disk (None for untitled scratchpad).
    pub path: Option<PathBuf>,
    /// Display filename (e.g. "meeting.md" or "Untitled").
    pub filename: String,
    /// Document text content decoded losslessly or with UTF-8 replacement.
    pub content: String,
    /// Last modification timestamp in seconds since Unix epoch.
    pub mtime: u64,
    /// Raw file size in bytes.
    pub size: usize,
}

impl DocumentContent {
    /// Constructs a new `DocumentContent` for an untitled scratchpad document.
    pub fn untitled() -> Self {
        Self {
            path: None,
            filename: "Untitled".to_string(),
            content: String::new(),
            mtime: 0,
            size: 0,
        }
    }

    /// Checks if this is an untitled scratchpad (not yet saved to disk).
    pub fn is_untitled(&self) -> bool {
        self.path.is_none()
    }
}

impl Default for DocumentContent {
    fn default() -> Self {
        Self::untitled()
    }
}

/// Reads a document from disk with clean lossy UTF-8 decoding and metadata extraction.
///
/// # Errors
/// Returns `std::io::Error` if the file cannot be found, cannot be accessed,
/// or is a directory.
pub fn read_file_lossy(path: &Path) -> io::Result<DocumentContent> {
    let metadata = fs::metadata(path)?;
    if metadata.is_dir() {
        return Err(io::Error::other(format!(
            "Path is a directory, not a regular file: {}",
            path.display()
        )));
    }

    let bytes = fs::read(path)?;
    let size = bytes.len();
    let content = String::from_utf8_lossy(&bytes).into_owned();

    let filename = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".to_string());

    let mtime = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Ok(DocumentContent {
        path: Some(path.to_path_buf()),
        filename,
        content,
        mtime,
        size,
    })
}

/// RAII guard to delete temporary file if any failure occurs before atomic rename.
struct TempFileGuard<'a> {
    path: &'a Path,
    active: bool,
}

impl<'a> Drop for TempFileGuard<'a> {
    fn drop(&mut self) {
        if self.active {
            let _ = fs::remove_file(self.path);
        }
    }
}

/// Writes `content` to `path` atomically using a temporary file in the same directory.
///
/// Steps:
/// 1. Create a unique hidden temporary file in the same parent directory (`.<filename>.tmp.<pid>.<nanos>.<counter>`).
/// 2. Copy Unix file permissions from the target file if it already exists.
/// 3. Write data, flush buffers, and call `sync_all()`.
/// 4. Close file descriptor and atomically replace destination path via `fs::rename`.
/// 5. Durably sync parent directory.
/// 6. Return new file `mtime` in seconds since Unix epoch.
///
/// # Errors
/// Returns `std::io::Error` if the target path is an existing directory,
/// parent directory is unwritable, or disk write fails.
pub fn atomic_write_file(path: &Path, content: &str) -> io::Result<u64> {
    if path.as_os_str().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Target path cannot be empty",
        ));
    }

    if path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("Target path is an existing directory: {}", path.display()),
        ));
    }

    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };

    if !parent.exists() {
        fs::create_dir_all(parent)?;
    }

    let filename_str = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_else(|| "document".into());

    let pid = std::process::id();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let temp_filename = format!(".{filename_str}.tmp.{pid}.{nanos}.{counter}");
    let temp_path = parent.join(temp_filename);

    let mut guard = TempFileGuard {
        path: &temp_path,
        active: true,
    };

    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;

        #[cfg(unix)]
        if let Ok(meta) = fs::metadata(path) {
            let _ = file.set_permissions(meta.permissions());
        }

        file.write_all(content.as_bytes())?;
        file.flush()?;
        file.sync_all()?;
    }

    fs::rename(&temp_path, path)?;
    guard.active = false;

    #[cfg(unix)]
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }

    let metadata = fs::metadata(path)?;
    let mtime = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Ok(mtime)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestSandbox {
        root: PathBuf,
    }

    impl TestSandbox {
        fn new() -> Self {
            let pid = std::process::id();
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("typora_io_unit_{pid}_{nanos}"));
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn path(&self) -> &Path {
            &self.root
        }
    }

    impl Drop for TestSandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn test_read_standard_file() {
        let sandbox = TestSandbox::new();
        let file = sandbox.path().join("doc.md");
        fs::write(&file, "# Hello World\nContent.").unwrap();

        let doc = read_file_lossy(&file).unwrap();
        assert_eq!(doc.content, "# Hello World\nContent.");
        assert_eq!(doc.filename, "doc.md");
        assert_eq!(doc.size, 22);
        assert!(doc.mtime > 0);
        assert_eq!(doc.path, Some(file));
    }

    #[test]
    fn test_read_lossy_invalid_utf8() {
        let sandbox = TestSandbox::new();
        let file = sandbox.path().join("lossy.md");
        fs::write(&file, b"Header: \xFF\xFE tail").unwrap();

        let doc = read_file_lossy(&file).unwrap();
        assert!(doc.content.contains('\u{FFFD}'));
        assert!(doc.content.starts_with("Header: "));
        assert!(doc.content.ends_with(" tail"));
        assert_eq!(doc.size, 15);
    }

    #[test]
    fn test_read_nonexistent_file() {
        let sandbox = TestSandbox::new();
        let missing = sandbox.path().join("nonexistent.md");
        let res = read_file_lossy(&missing);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn test_read_empty_file() {
        let sandbox = TestSandbox::new();
        let file = sandbox.path().join("empty.md");
        fs::write(&file, b"").unwrap();

        let doc = read_file_lossy(&file).unwrap();
        assert_eq!(doc.content, "");
        assert_eq!(doc.size, 0);
    }

    #[test]
    fn test_read_preserves_crlf() {
        let sandbox = TestSandbox::new();
        let file = sandbox.path().join("crlf.md");
        let crlf = "A\r\nB\r\n";
        fs::write(&file, crlf).unwrap();

        let doc = read_file_lossy(&file).unwrap();
        assert_eq!(doc.content, crlf);
    }

    #[test]
    fn test_atomic_write_creates_file() {
        let sandbox = TestSandbox::new();
        let file = sandbox.path().join("new.md");
        let mtime = atomic_write_file(&file, "Atomic created").unwrap();
        assert!(mtime > 0);
        assert!(file.exists());

        let doc = read_file_lossy(&file).unwrap();
        assert_eq!(doc.content, "Atomic created");
    }

    #[test]
    fn test_atomic_write_overwrites_existing() {
        let sandbox = TestSandbox::new();
        let file = sandbox.path().join("overwrite.md");
        fs::write(&file, "Old content").unwrap();

        atomic_write_file(&file, "New content").unwrap();
        let doc = read_file_lossy(&file).unwrap();
        assert_eq!(doc.content, "New content");
    }

    #[test]
    fn test_atomic_write_leaves_no_temp_files() {
        let sandbox = TestSandbox::new();
        let file = sandbox.path().join("clean.md");
        atomic_write_file(&file, "Clean content").unwrap();

        let entries: Vec<_> = fs::read_dir(sandbox.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], "clean.md");
    }

    #[test]
    fn test_atomic_write_empty_file() {
        let sandbox = TestSandbox::new();
        let file = sandbox.path().join("empty_write.md");
        atomic_write_file(&file, "").unwrap();

        let doc = read_file_lossy(&file).unwrap();
        assert_eq!(doc.content, "");
        assert_eq!(doc.size, 0);
    }

    #[test]
    fn test_atomic_write_rejects_directory() {
        let sandbox = TestSandbox::new();
        let dir = sandbox.path().join("existing_dir");
        fs::create_dir_all(&dir).unwrap();

        let res = atomic_write_file(&dir, "Should fail");
        assert!(res.is_err());
    }

    #[test]
    fn test_atomic_write_rejects_empty_path() {
        let res = atomic_write_file(Path::new(""), "Data");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn test_atomic_write_rapid_successive() {
        let sandbox = TestSandbox::new();
        let file = sandbox.path().join("rapid.md");
        for i in 0..30 {
            atomic_write_file(&file, &format!("Ver {i}")).unwrap();
        }
        let doc = read_file_lossy(&file).unwrap();
        assert_eq!(doc.content, "Ver 29");
    }

    #[test]
    fn test_document_content_default_untitled() {
        let doc = DocumentContent::default();
        assert!(doc.is_untitled());
        assert_eq!(doc.path, None);
        assert_eq!(doc.filename, "Untitled");
        assert_eq!(doc.content, "");
        assert_eq!(doc.mtime, 0);
        assert_eq!(doc.size, 0);
    }

    #[test]
    fn test_document_content_serde() {
        let doc = DocumentContent {
            path: Some(PathBuf::from("/tmp/test.md")),
            filename: "test.md".to_string(),
            content: "# Header".to_string(),
            mtime: 1730500000,
            size: 8,
        };
        let json = serde_json::to_string(&doc).unwrap();
        let deserialized: DocumentContent = serde_json::from_str(&json).unwrap();
        assert_eq!(doc, deserialized);
    }
}
