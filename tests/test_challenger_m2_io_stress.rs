//! Empirical Challenger M2 Stress Test Suite
//! Target: Direct File I/O and Atomic Writing (`src/io.rs`)
//!
//! Directly tests `typora_rust_reader::io::{atomic_write_file, read_file_lossy, DocumentContent}`

use std::fs::{self, Permissions};
use std::io;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use typora_rust_reader::io::{atomic_write_file, read_file_lossy};

static SANDBOX_COUNTER: AtomicUsize = AtomicUsize::new(1000);

struct StressSandbox {
    root: PathBuf,
}

impl StressSandbox {
    fn new(prefix: &str) -> Self {
        let counter = SANDBOX_COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir_name = format!("typora_challenger_{prefix}_{pid}_{counter}_{nanos}");
        let root = std::env::temp_dir().join(dir_name);
        fs::create_dir_all(&root).expect("Failed to create stress sandbox dir");
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn list_all_entries(&self) -> Vec<String> {
        let mut entries = Vec::new();
        if let Ok(read_dir) = fs::read_dir(&self.root) {
            for entry in read_dir.flatten() {
                entries.push(entry.file_name().to_string_lossy().to_string());
            }
        }
        entries
    }
}

impl Drop for StressSandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

// =========================================================================
// 1. Rapid successive atomic saves (100 writes) + temp cleanup verification
// =========================================================================

#[test]
fn test_challenger_rapid_successive_saves_100_and_no_tmp_leftover() {
    let sandbox = StressSandbox::new("rapid_100");
    let target = sandbox.path().join("rapid_doc.md");

    let mut last_mtime = 0;
    for i in 0..100 {
        let payload = format!("Payload version {i} with payload padding: {}", "A".repeat(i * 10));
        let mtime = atomic_write_file(&target, &payload).expect("Write failed during rapid loop");
        assert!(mtime >= last_mtime, "mtime decreased: {mtime} < {last_mtime}");
        last_mtime = mtime;

        // Verify intermediate read
        let doc = read_file_lossy(&target).expect("Read failed during rapid loop");
        assert_eq!(doc.content, payload);
        assert_eq!(doc.size, payload.len());
    }

    // Inspect the sandbox directory for any leftover .tmp.* files
    let entries = sandbox.list_all_entries();
    for entry in &entries {
        assert!(
            !entry.contains(".tmp."),
            "Found orphan temp file in sandbox: {entry}"
        );
    }
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0], "rapid_doc.md");

    // Final read verification
    let final_doc = read_file_lossy(&target).unwrap();
    let expected_final = format!("Payload version 99 with payload padding: {}", "A".repeat(99 * 10));
    assert_eq!(final_doc.content, expected_final);
}

// =========================================================================
// 2. Concurrent reader threads reading while worker writes (NO 0-byte reads)
// =========================================================================

#[test]
fn test_challenger_concurrent_readers_no_zero_byte_reads() {
    let sandbox = StressSandbox::new("concurrent_readers");
    let target = sandbox.path().join("active_content.md");

    // Seed file with initial data
    let initial_content = "ORIGINAL_START: ".to_string() + &"X".repeat(60_000);
    atomic_write_file(&target, &initial_content).expect("Initial write failed");

    let stop = Arc::new(AtomicBool::new(false));
    let zero_byte_detected = Arc::new(AtomicBool::new(false));
    let invalid_content_detected = Arc::new(AtomicBool::new(false));
    let read_count = Arc::new(AtomicUsize::new(0));

    let num_readers = 4;
    let mut reader_handles = Vec::new();

    for reader_id in 0..num_readers {
        let stop_clone = Arc::clone(&stop);
        let zero_clone = Arc::clone(&zero_byte_detected);
        let invalid_clone = Arc::clone(&invalid_content_detected);
        let count_clone = Arc::clone(&read_count);
        let target_clone = target.clone();

        let handle = thread::spawn(move || {
            while !stop_clone.load(Ordering::Relaxed) {
                match read_file_lossy(&target_clone) {
                    Ok(doc) => {
                        count_clone.fetch_add(1, Ordering::Relaxed);
                        if doc.size == 0 || doc.content.is_empty() {
                            zero_clone.store(true, Ordering::SeqCst);
                            eprintln!("Reader {reader_id} observed 0-byte read!");
                            break;
                        }
                        // Content must either be from initial or one of the valid iterations
                        let is_valid = doc.content.starts_with("ORIGINAL_START: ")
                            || doc.content.starts_with("BURST_VERSION_");
                        if !is_valid {
                            invalid_clone.store(true, Ordering::SeqCst);
                            eprintln!("Reader {reader_id} observed corrupted content: len={}", doc.content.len());
                            break;
                        }
                    }
                    Err(e) => {
                        // On heavily concurrent rename, opening or metadata might briefly encounter NotFound or similar
                        // but it must not panic
                        eprintln!("Reader {reader_id} read error: {e}");
                    }
                }
            }
        });
        reader_handles.push(handle);
    }

    // Heavy burst writer
    for i in 0..50 {
        let payload = format!("BURST_VERSION_{i}: {}", "Y".repeat(50_000 + (i * 200)));
        atomic_write_file(&target, &payload).expect("Burst write failed");
        thread::yield_now();
    }

    stop.store(true, Ordering::Relaxed);
    for h in reader_handles {
        h.join().expect("Reader thread panicked");
    }

    assert!(
        !zero_byte_detected.load(Ordering::SeqCst),
        "CRITICAL BUG: Concurrent readers detected 0-byte reads!"
    );
    assert!(
        !invalid_content_detected.load(Ordering::SeqCst),
        "CRITICAL BUG: Readers detected corrupted partial content!"
    );
    assert!(
        read_count.load(Ordering::SeqCst) > 50,
        "Readers should have completed substantial reads (got {})",
        read_count.load(Ordering::SeqCst)
    );

    // Verify no leftover temp files
    let entries = sandbox.list_all_entries();
    for entry in &entries {
        assert!(!entry.contains(".tmp."), "Leftover temp file: {entry}");
    }
}

// =========================================================================
// 3. Symlink handling: atomic save to symlink path
// =========================================================================

#[test]
#[cfg(unix)]
fn test_challenger_symlink_atomic_save_resolution_or_replacement() {
    let sandbox = StressSandbox::new("symlink_test");
    let real_file = sandbox.path().join("real_target.md");
    fs::write(&real_file, "Original Target Content").unwrap();

    let symlink_file = sandbox.path().join("symlink_pointer.md");
    std::os::unix::fs::symlink(&real_file, &symlink_file).expect("Failed to create symlink");

    // Atomic write targeting the symlink path
    let new_content = "# Updated via symlink path";
    let mtime = atomic_write_file(&symlink_file, new_content).expect("Atomic write to symlink failed");
    assert!(mtime > 0);

    // Verify reading symlink_file returns the new content
    let doc = read_file_lossy(&symlink_file).expect("Read from symlink failed");
    assert_eq!(doc.content, new_content);

    // Verify real file still exists and was not corrupted
    assert!(real_file.exists());

    // Verify no stray temp files left behind
    let entries = sandbox.list_all_entries();
    for entry in &entries {
        assert!(!entry.contains(".tmp."), "Leftover temp file: {entry}");
    }
}

#[test]
#[cfg(unix)]
fn test_challenger_broken_symlink_atomic_save_replaces_cleanly() {
    let sandbox = StressSandbox::new("broken_symlink");
    let non_existent = sandbox.path().join("does_not_exist.md");
    let broken_link = sandbox.path().join("broken_link.md");
    std::os::unix::fs::symlink(&non_existent, &broken_link).expect("Failed to create broken symlink");

    // Atomic write targeting the broken symlink should succeed by replacing the broken symlink
    let content = "# Content writing over broken symlink";
    let mtime = atomic_write_file(&broken_link, content).expect("Atomic write over broken symlink failed");
    assert!(mtime > 0);

    let doc = read_file_lossy(&broken_link).expect("Read failed");
    assert_eq!(doc.content, content);
}

// =========================================================================
// 4. Read lossy UTF-8: invalid bytes, truncated multi-byte, null bytes, binary
// =========================================================================

#[test]
fn test_challenger_read_lossy_invalid_byte_sequences() {
    let sandbox = StressSandbox::new("lossy_invalid");
    let target = sandbox.path().join("invalid_bytes.md");

    // Sequence of invalid UTF-8 bytes: lone continuations, out-of-range bytes 0xFF, 0xFE, 0xC0
    let invalid_bytes = vec![
        0x61, 0x62, 0x63, // "abc"
        0xFF, 0xFE,       // invalid bytes
        0x64, 0x65, 0x66, // "def"
        0x80, 0xBF,       // lone continuation bytes
        0x67, 0x68,       // "gh"
        0xC0, 0xAF,       // overlong ASCII slash
        0x69,             // "i"
    ];
    fs::write(&target, &invalid_bytes).unwrap();

    let doc = read_file_lossy(&target).expect("Lossy read should not fail on invalid bytes");
    assert_eq!(doc.size, invalid_bytes.len());
    assert!(doc.content.starts_with("abc"));
    assert!(doc.content.contains('\u{FFFD}'));
    assert!(doc.content.contains("def"));
    assert!(doc.content.contains("gh"));
    assert!(doc.content.ends_with('i'));
}

#[test]
fn test_challenger_read_lossy_truncated_multibyte() {
    let sandbox = StressSandbox::new("truncated_multibyte");
    let target = sandbox.path().join("truncated.md");

    // Truncated multi-byte UTF-8:
    // 0xE2 0x82 0xAC is Euro sign (€), here truncated to [0xE2, 0x82]
    // 0xF0 0x9F 0x90 0x80 is 🦀, truncated to [0xF0, 0x9F]
    let truncated_bytes = vec![
        0x23, 0x20, // "# "
        0xE2, 0x82, // incomplete 3-byte
        0x20,       // " "
        0xF0, 0x9F, // incomplete 4-byte
        0x21,       // "!"
    ];
    fs::write(&target, &truncated_bytes).unwrap();

    let doc = read_file_lossy(&target).expect("Truncated UTF-8 should decode lossily");
    assert_eq!(doc.size, truncated_bytes.len());
    assert!(doc.content.starts_with("# "));
    assert!(doc.content.ends_with('!'));
    assert!(doc.content.contains('\u{FFFD}'));
}

#[test]
fn test_challenger_read_lossy_embedded_null_bytes() {
    let sandbox = StressSandbox::new("null_bytes");
    let target = sandbox.path().join("nulls.md");

    let raw = b"Line 1\0with\0null\0bytes\nLine 2\0embedded\0nulls\0";
    fs::write(&target, raw).unwrap();

    let doc = read_file_lossy(&target).expect("Null bytes should decode cleanly");
    assert_eq!(doc.size, raw.len());
    assert_eq!(doc.content.matches('\0').count(), 6);
    assert!(doc.content.contains("Line 1\0with\0null"));
}

#[test]
fn test_challenger_read_lossy_binary_file_png_header() {
    let sandbox = StressSandbox::new("binary_png");
    let target = sandbox.path().join("fake_image.png");

    let mut binary_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]; // PNG header
    for b in 0..=255 {
        binary_data.push(b as u8);
    }
    fs::write(&target, &binary_data).unwrap();

    let doc = read_file_lossy(&target).expect("Arbitrary binary should decode lossily");
    assert_eq!(doc.size, binary_data.len());
    assert!(doc.content.contains('\u{FFFD}'));
    assert_eq!(doc.filename, "fake_image.png");
}

// =========================================================================
// 5. Error cases: write to directory, read-only directory, empty path
// =========================================================================

#[test]
fn test_challenger_write_to_existing_directory_errors() {
    let sandbox = StressSandbox::new("write_dir");
    let dir = sandbox.path().join("subfolder");
    fs::create_dir_all(&dir).unwrap();

    let res = atomic_write_file(&dir, "Should fail on directory");
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
}

#[test]
fn test_challenger_write_to_directory_with_trailing_slash_errors() {
    let sandbox = StressSandbox::new("write_dir_slash");
    let dir = sandbox.path().join("folder");
    fs::create_dir_all(&dir).unwrap();

    let slash_path = dir.join("");
    let res = atomic_write_file(&slash_path, "Should fail on directory with trailing slash");
    assert!(res.is_err());
}

#[test]
fn test_challenger_write_empty_path_errors() {
    let res = atomic_write_file(Path::new(""), "Data");
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::InvalidInput);
}

#[test]
#[cfg(unix)]
fn test_challenger_write_to_readonly_directory_errors() {
    let sandbox = StressSandbox::new("ro_dir");
    let ro_dir = sandbox.path().join("readonly_sub");
    fs::create_dir_all(&ro_dir).unwrap();

    // Set read + execute only (0o555)
    fs::set_permissions(&ro_dir, Permissions::from_mode(0o555)).unwrap();

    let target = ro_dir.join("should_fail.md");
    let res = atomic_write_file(&target, "Data");

    // Restore permissions for clean sandbox drop
    let _ = fs::set_permissions(&ro_dir, Permissions::from_mode(0o755));

    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::PermissionDenied);

    // Verify no temp file remained
    let entries = sandbox.list_all_entries();
    for entry in entries {
        assert!(!entry.contains(".tmp."));
    }
}

#[test]
fn test_challenger_read_directory_path_errors() {
    let sandbox = StressSandbox::new("read_dir");
    let dir = sandbox.path().join("a_directory");
    fs::create_dir_all(&dir).unwrap();

    let res = read_file_lossy(&dir);
    assert!(res.is_err());
}

#[test]
fn test_challenger_read_nonexistent_file_errors() {
    let sandbox = StressSandbox::new("read_missing");
    let missing = sandbox.path().join("ghost_file.md");

    let res = read_file_lossy(&missing);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::NotFound);
}

#[test]
fn test_challenger_read_empty_path_errors() {
    let res = read_file_lossy(Path::new(""));
    assert!(res.is_err());
}

// =========================================================================
// 6. Permissions preservation on Unix
// =========================================================================

#[test]
#[cfg(unix)]
fn test_challenger_preserves_custom_unix_permissions() {
    let sandbox = StressSandbox::new("permissions");
    let file = sandbox.path().join("secret.md");

    // Initial creation with 0o600 (owner read/write only)
    atomic_write_file(&file, "Confidential").unwrap();
    fs::set_permissions(&file, Permissions::from_mode(0o600)).unwrap();

    let mode_before = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode_before, 0o600);

    // Atomic overwrite
    atomic_write_file(&file, "Updated Confidential").unwrap();

    let mode_after = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode_after, 0o600, "Permissions not preserved across atomic save!");

    let doc = read_file_lossy(&file).unwrap();
    assert_eq!(doc.content, "Updated Confidential");
}

// =========================================================================
// 7. Auto parent directory creation on write
// =========================================================================

#[test]
fn test_challenger_creates_deep_parent_directories_automatically() {
    let sandbox = StressSandbox::new("deep_dirs");
    let deep_file = sandbox.path().join("level1/level2/level3/nested_doc.md");

    assert!(!deep_file.parent().unwrap().exists());

    let mtime = atomic_write_file(&deep_file, "# Deep Document").unwrap();
    assert!(mtime > 0);
    assert!(deep_file.exists());

    let doc = read_file_lossy(&deep_file).unwrap();
    assert_eq!(doc.content, "# Deep Document");
    assert_eq!(doc.filename, "nested_doc.md");
}

// =========================================================================
// 8. Large document roundtrip stress (5MB)
// =========================================================================

#[test]
fn test_challenger_large_document_roundtrip_5mb() {
    let sandbox = StressSandbox::new("large_doc");
    let target = sandbox.path().join("big_book.md");

    let chunk = "The quick brown fox jumps over the lazy dog. 1234567890! 🦀\n";
    let repeat_count = 5_000_000 / chunk.len();
    let big_content = chunk.repeat(repeat_count);

    let mtime = atomic_write_file(&target, &big_content).expect("Large write failed");
    assert!(mtime > 0);

    let doc = read_file_lossy(&target).expect("Large read failed");
    assert_eq!(doc.size, big_content.len());
    assert_eq!(doc.content.len(), big_content.len());
    assert_eq!(doc.content, big_content);
}

// =========================================================================
// 9. Multi-thread concurrent writers competing on the SAME file
// =========================================================================

#[test]
fn test_challenger_multi_thread_concurrent_writers_on_same_file() {
    let sandbox = StressSandbox::new("concurrent_writers");
    let target = sandbox.path().join("contested.md");

    // Initialize file
    atomic_write_file(&target, "INITIAL").unwrap();

    let num_writers = 8;
    let writes_per_thread = 25;
    let mut handles = Vec::new();

    for writer_id in 0..num_writers {
        let target_clone = target.clone();
        let handle = thread::spawn(move || {
            for i in 0..writes_per_thread {
                let payload = format!("Writer {writer_id} iteration {i}: {}", "Z".repeat(500));
                atomic_write_file(&target_clone, &payload).expect("Concurrent atomic_write_file failed");
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("Writer thread panicked");
    }

    // Verify final content is valid
    let final_doc = read_file_lossy(&target).expect("Read contested file failed");
    assert!(final_doc.content.starts_with("Writer "));
    assert!(final_doc.content.contains(": ZZZZZ"));

    // Verify ZERO orphan temp files
    let entries = sandbox.list_all_entries();
    for entry in &entries {
        assert!(!entry.contains(".tmp."), "Orphan temp file found: {entry}");
    }
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0], "contested.md");
}

// =========================================================================
// 10. Symlink across different directories
// =========================================================================

#[test]
#[cfg(unix)]
fn test_challenger_symlink_across_different_directories() {
    let sandbox = StressSandbox::new("cross_dir_symlink");
    let dir_a = sandbox.path().join("dir_a");
    let dir_b = sandbox.path().join("dir_b");
    fs::create_dir_all(&dir_a).unwrap();
    fs::create_dir_all(&dir_b).unwrap();

    let real_file = dir_a.join("source.md");
    fs::write(&real_file, "Original in dir A").unwrap();

    let link_file = dir_b.join("link_to_a.md");
    std::os::unix::fs::symlink(&real_file, &link_file).unwrap();

    // Write to link_file
    let new_content = "# Replaced link in dir B";
    atomic_write_file(&link_file, new_content).expect("Write to cross-dir symlink failed");

    // dir_b/link_to_a.md is now a real file with new content
    let link_doc = read_file_lossy(&link_file).unwrap();
    assert_eq!(link_doc.content, new_content);

    // dir_a/source.md is untouched
    let orig_doc = read_file_lossy(&real_file).unwrap();
    assert_eq!(orig_doc.content, "Original in dir A");

    // Check no temp files in either directory
    for dir in &[&dir_a, &dir_b] {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(!name.contains(".tmp."), "Temp file in {}: {}", dir.display(), name);
        }
    }
}

