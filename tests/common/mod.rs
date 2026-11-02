//! Common test helpers, fixtures, and oracles for Typora-RustReader E2E tests.
#![allow(dead_code)]

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SANDBOX_COUNTER: AtomicUsize = AtomicUsize::new(1);
static TEST_TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

/// An isolated temporary directory sandbox that automatically cleans up on drop.
pub struct TestSandbox {
    pub root: PathBuf,
}

impl TestSandbox {
    /// Creates a new isolated test sandbox in the system temp directory.
    pub fn new() -> Self {
        let counter = SANDBOX_COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir_name = format!("typora_test_{}_{}_{}", pid, counter, timestamp);
        let root = std::env::temp_dir().join(dir_name);
        fs::create_dir_all(&root).expect("Failed to create test sandbox directory");
        Self { root }
    }

    /// Path to the sandbox root.
    pub fn path(&self) -> &Path {
        &self.root
    }

    /// Creates a text file at the given relative path with the provided content.
    pub fn create_file<P: AsRef<Path>>(&self, rel_path: P, content: &str) -> PathBuf {
        let full_path = self.root.join(rel_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent directory in sandbox");
        }
        let mut file = File::create(&full_path).expect("Failed to create file in sandbox");
        file.write_all(content.as_bytes())
            .expect("Failed to write content in sandbox");
        file.sync_all().expect("Failed to sync file in sandbox");
        full_path
    }

    /// Creates a binary file at the given relative path with arbitrary bytes.
    pub fn create_binary_file<P: AsRef<Path>>(&self, rel_path: P, bytes: &[u8]) -> PathBuf {
        let full_path = self.root.join(rel_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent directory in sandbox");
        }
        let mut file = File::create(&full_path).expect("Failed to create binary file in sandbox");
        file.write_all(bytes).expect("Failed to write bytes in sandbox");
        file.sync_all().expect("Failed to sync binary file in sandbox");
        full_path
    }

    /// Reads text file with UTF-8 lossy replacement.
    pub fn read_file_lossy<P: AsRef<Path>>(&self, rel_path: P) -> io::Result<String> {
        let full_path = self.root.join(rel_path);
        let bytes = fs::read(&full_path)?;
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }

    /// Reads binary bytes from a file.
    pub fn read_bytes<P: AsRef<Path>>(&self, rel_path: P) -> io::Result<Vec<u8>> {
        let full_path = self.root.join(rel_path);
        fs::read(&full_path)
    }

    /// Checks if a file exists relative to sandbox root.
    pub fn file_exists<P: AsRef<Path>>(&self, rel_path: P) -> bool {
        self.root.join(rel_path).exists()
    }

    /// Gets modification time in seconds since Unix epoch.
    pub fn get_mtime<P: AsRef<Path>>(&self, rel_path: P) -> u64 {
        let full_path = self.root.join(rel_path);
        let metadata = fs::metadata(&full_path).expect("Failed to read metadata");
        let modified = metadata.modified().expect("Failed to read mtime");
        modified
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

impl Drop for TestSandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Locates the compiled binary for `typora-rust-reader`.
pub fn find_typora_binary() -> Option<PathBuf> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let candidates = [
        manifest_dir.join("target/debug/typora-rust-reader"),
        manifest_dir.join("target/release/typora-rust-reader"),
        PathBuf::from("/home/cjledbetter/.local/bin/typora-rust-reader"),
    ];

    for path in &candidates {
        if path.is_file() {
            return Some(path.clone());
        }
    }
    None
}

/// Slugify anchor oracle matching CommonMark / marked.js rules.
/// Lowercases, replaces spaces and punctuation with hyphens, handles duplicates with -1, -2.
pub fn marked_slugify(title: &str, existing: &mut HashSet<String>) -> String {
    let mut base = String::new();
    let mut prev_hyphen = false;

    for c in title.chars() {
        if c.is_alphanumeric() {
            base.extend(c.to_lowercase());
            prev_hyphen = false;
        } else if c == ' ' || c == '-' || c == '_' {
            if !prev_hyphen && !base.is_empty() {
                base.push('-');
                prev_hyphen = true;
            }
        }
    }

    if base.ends_with('-') {
        base.pop();
    }
    if base.is_empty() {
        base.push_str("section");
    }

    let mut slug = base.clone();
    let mut counter = 1;
    while existing.contains(&slug) {
        slug = format!("{}-{}", base, counter);
        counter += 1;
    }
    existing.insert(slug.clone());
    slug
}

/// Document statistics oracle: (words, chars, lines, reading_time_minutes).
pub fn calculate_stats_oracle(text: &str) -> (usize, usize, usize, usize) {
    let lines = if text.is_empty() { 0 } else { text.lines().count() };
    let chars = text.chars().count();
    let words = text
        .split_whitespace()
        .filter(|w| !w.is_empty())
        .count();
    let reading_time_minutes = if words == 0 {
        0
    } else {
        ((words as f64) / 200.0).ceil() as usize
    };
    (words, chars, lines, reading_time_minutes)
}

/// Direct atomic write oracle following: write to temp file -> sync -> atomic rename.
pub fn atomic_write_oracle(path: &Path, content: &str) -> io::Result<u64> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let pid = std::process::id();
    let counter = TEST_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_name = format!(".tmp_{pid}_{nanos}_{counter}");
    let temp_path = parent.join(temp_name);

    {
        let mut file = File::create(&temp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
    }

    fs::rename(&temp_path, path)?;
    let metadata = fs::metadata(path)?;
    let mtime = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    Ok(mtime)
}

/// Direct read oracle with UTF-8 lossy fallback.
pub fn read_file_lossy_oracle(path: &Path) -> io::Result<String> {
    let bytes = fs::read(path)?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}
