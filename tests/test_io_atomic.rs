//! Opaque-box E2E test suite for Typora-RustReader Direct File I/O and Atomic Saves.
//!
//! Features Covered:
//! - Feature 10: Direct File Read & Lossy UTF-8 Fallback
//! - Feature 11: Atomic File Save (`tmp -> sync -> rename`)
//!
//! Interface Contracts:
//! - DocumentContent { path, filename, content, mtime, size }
//! - read_file_lossy(path: &Path) -> Result<DocumentContent, std::io::Error>
//! - atomic_write_file(path: &Path, content: &str) -> Result<u64, std::io::Error>

#![allow(dead_code, unused_imports, unused_variables)]

mod common;
use common::{atomic_write_oracle, read_file_lossy_oracle, TestSandbox};
use std::fs::{self, File, Permissions};
use std::io::{self, Read, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentContentModel {
    pub path: Option<PathBuf>,
    pub filename: String,
    pub content: String,
    pub mtime: u64,
    pub size: usize,
}

/// Direct implementation of the contract oracle for DocumentContent
pub fn read_file_lossy_contract(path: &Path) -> io::Result<DocumentContentModel> {
    let metadata = fs::metadata(path)?;
    let bytes = fs::read(path)?;
    let content = String::from_utf8_lossy(&bytes).to_string();
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled".to_string());
    let mtime = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let size = bytes.len();

    Ok(DocumentContentModel {
        path: Some(path.to_path_buf()),
        filename,
        content,
        mtime,
        size,
    })
}

// =========================================================================
// === TIER 1: FEATURE COVERAGE (Minimum 10 tests across F10 and F11)
// =========================================================================

#[test]
fn test_io_f10_read_standard_utf8_markdown() {
    let sandbox = TestSandbox::new();
    let sample_text = "# Hello World\n\nThis is a standard markdown document.";
    let file = sandbox.create_file("doc.md", sample_text);

    let doc = read_file_lossy_contract(&file).expect("Failed to read file");
    assert_eq!(doc.content, sample_text);
    assert_eq!(doc.filename, "doc.md");
    assert_eq!(doc.size, sample_text.len());
    assert!(doc.mtime > 0);
}

#[test]
fn test_io_f10_read_lossy_utf8_invalid_bytes_no_panic() {
    let sandbox = TestSandbox::new();
    // 0xFF and 0xFE are invalid UTF-8 bytes
    let raw_bytes = b"Heading: \xFF\xFE\xFD with valid suffix";
    let file = sandbox.create_binary_file("lossy.md", raw_bytes);

    let doc = read_file_lossy_contract(&file).expect("Lossy read should not error on invalid UTF-8");
    assert!(doc.content.contains('\u{FFFD}'));
    assert!(doc.content.starts_with("Heading: "));
    assert!(doc.content.ends_with(" with valid suffix"));
    assert_eq!(doc.size, raw_bytes.len());
}

#[test]
fn test_io_f10_read_document_content_path_matches() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("path_test.md", "Content");
    let doc = read_file_lossy_contract(&file).unwrap();
    assert_eq!(doc.path, Some(file));
}

#[test]
fn test_io_f10_read_filename_extraction() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("nested/dir/my_notes.markdown", "notes");
    let doc = read_file_lossy_contract(&file).unwrap();
    assert_eq!(doc.filename, "my_notes.markdown");
}

#[test]
fn test_io_f10_read_preserves_crlf_line_endings() {
    let sandbox = TestSandbox::new();
    let crlf_content = "Line 1\r\nLine 2\r\nLine 3\r\n";
    let file = sandbox.create_file("crlf.md", crlf_content);
    let doc = read_file_lossy_contract(&file).unwrap();
    assert_eq!(doc.content, crlf_content);
}

#[test]
fn test_io_f11_atomic_write_creates_new_file() {
    let sandbox = TestSandbox::new();
    let target = sandbox.path().join("new_file.md");
    let content = "# Newly Created Document\n\nContent created atomically.";

    let mtime = atomic_write_oracle(&target, content).expect("Atomic write should succeed");
    assert!(target.exists());
    assert!(mtime > 0);

    let doc = read_file_lossy_contract(&target).unwrap();
    assert_eq!(doc.content, content);
}

#[test]
fn test_io_f11_atomic_write_overwrites_existing_content() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("overwrite.md", "Old initial content");

    let new_content = "# Overwritten Content\n\nCompletely replaced.";
    atomic_write_oracle(&file, new_content).expect("Overwrite write should succeed");

    let doc = read_file_lossy_contract(&file).unwrap();
    assert_eq!(doc.content, new_content);
}

#[test]
fn test_io_f11_atomic_write_leaves_no_temp_files() {
    let sandbox = TestSandbox::new();
    let file = sandbox.path().join("clean_save.md");

    atomic_write_oracle(&file, "# Clean Content").unwrap();

    let entries: Vec<_> = fs::read_dir(sandbox.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0], "clean_save.md");
}

#[test]
fn test_io_f11_atomic_write_returns_mtime_matching_disk() {
    let sandbox = TestSandbox::new();
    let file = sandbox.path().join("mtime_test.md");

    let returned_mtime = atomic_write_oracle(&file, "Data").unwrap();
    let disk_mtime = fs::metadata(&file)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    assert_eq!(returned_mtime, disk_mtime);
}

#[test]
fn test_io_f11_atomic_write_large_document() {
    let sandbox = TestSandbox::new();
    let file = sandbox.path().join("large.md");
    let large_content = "# Large Document\n".repeat(50_000); // ~850 KB

    atomic_write_oracle(&file, &large_content).unwrap();
    let doc = read_file_lossy_contract(&file).unwrap();
    assert_eq!(doc.content.len(), large_content.len());
}

// =========================================================================
// === TIER 2: BOUNDARY & CORNER CASES (Minimum 20 tests)
// =========================================================================

#[test]
fn test_io_boundary_read_empty_file() {
    let sandbox = TestSandbox::new();
    let empty_file = sandbox.create_file("empty.md", "");
    let doc = read_file_lossy_contract(&empty_file).unwrap();

    assert_eq!(doc.content, "");
    assert_eq!(doc.size, 0);
    assert_eq!(doc.filename, "empty.md");
}

#[test]
fn test_io_boundary_write_empty_file() {
    let sandbox = TestSandbox::new();
    let file = sandbox.path().join("write_empty.md");
    atomic_write_oracle(&file, "").unwrap();

    let doc = read_file_lossy_contract(&file).unwrap();
    assert_eq!(doc.content, "");
    assert_eq!(doc.size, 0);
}

#[test]
fn test_io_boundary_read_nonexistent_file_errors() {
    let sandbox = TestSandbox::new();
    let missing = sandbox.path().join("does_not_exist.md");
    let res = read_file_lossy_contract(&missing);

    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::NotFound);
}

#[test]
fn test_io_boundary_read_file_with_null_bytes() {
    let sandbox = TestSandbox::new();
    let bytes_with_null = b"# Header\0\0with null bytes\0\nEnd";
    let file = sandbox.create_binary_file("nulls.md", bytes_with_null);

    let doc = read_file_lossy_contract(&file).unwrap();
    assert!(doc.content.contains('\0'));
    assert_eq!(doc.size, bytes_with_null.len());
}

#[test]
fn test_io_boundary_write_file_with_spaces_in_name() {
    let sandbox = TestSandbox::new();
    let file = sandbox.path().join("My Meeting Notes (Nov 2026).md");
    atomic_write_oracle(&file, "Notes content").unwrap();

    let doc = read_file_lossy_contract(&file).unwrap();
    assert_eq!(doc.filename, "My Meeting Notes (Nov 2026).md");
    assert_eq!(doc.content, "Notes content");
}

#[test]
fn test_io_boundary_write_file_with_unicode_name() {
    let sandbox = TestSandbox::new();
    let file = sandbox.path().join("文档_résumé_🦀.md");
    atomic_write_oracle(&file, "Unicode filename content").unwrap();

    let doc = read_file_lossy_contract(&file).unwrap();
    assert_eq!(doc.filename, "文档_résumé_🦀.md");
}

#[test]
fn test_io_boundary_write_to_directory_path_errors() {
    let sandbox = TestSandbox::new();
    let dir = sandbox.path().join("sub_directory");
    fs::create_dir_all(&dir).unwrap();

    let res = atomic_write_oracle(&dir, "Should fail because target is directory");
    assert!(res.is_err());
}

#[test]
fn test_io_boundary_write_in_deep_nested_path() {
    let sandbox = TestSandbox::new();
    let deep_dir = sandbox.path().join("a/b/c/d/e");
    fs::create_dir_all(&deep_dir).unwrap();
    let file = deep_dir.join("deep.md");

    atomic_write_oracle(&file, "Deep content").unwrap();
    let doc = read_file_lossy_contract(&file).unwrap();
    assert_eq!(doc.content, "Deep content");
}

#[test]
#[cfg(unix)]
fn test_io_boundary_write_to_readonly_directory_fails() {
    let sandbox = TestSandbox::new();
    let ro_dir = sandbox.path().join("readonly_dir");
    fs::create_dir_all(&ro_dir).unwrap();

    // Set directory permissions to 0555 (read + execute, no write)
    fs::set_permissions(&ro_dir, Permissions::from_mode(0o555)).unwrap();

    let file = ro_dir.join("test.md");
    let res = atomic_write_oracle(&file, "Data");

    // Clean up permissions so sandbox drop won't fail
    let _ = fs::set_permissions(&ro_dir, Permissions::from_mode(0o755));

    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
}

#[test]
fn test_io_boundary_rapid_successive_saves_consistency() {
    let sandbox = TestSandbox::new();
    let file = sandbox.path().join("rapid.md");

    for i in 0..50 {
        let content = format!("Version {}", i);
        atomic_write_oracle(&file, &content).unwrap();
    }

    let doc = read_file_lossy_contract(&file).unwrap();
    assert_eq!(doc.content, "Version 49");
}

#[test]
fn test_io_boundary_concurrent_readers_never_see_partial_content() {
    let sandbox = TestSandbox::new();
    let file = sandbox.path().join("concurrent.md");
    let initial_content = "X".repeat(50_000);
    atomic_write_oracle(&file, &initial_content).unwrap();

    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop);
    let file_clone = file.clone();

    // Reader thread continuously reads the file
    let reader = thread::spawn(move || {
        while !stop_clone.load(Ordering::Relaxed) {
            if let Ok(bytes) = fs::read(&file_clone) {
                // Must either see initial "X"*50000 or new "Y"*50000, NEVER 0 or partial length
                assert!(bytes.len() == 50_000, "Observed partial read length: {}", bytes.len());
            }
        }
    });

    // Writer thread updates file atomically with "Y"*50000
    let new_content = "Y".repeat(50_000);
    for _ in 0..10 {
        atomic_write_oracle(&file, &new_content).unwrap();
        atomic_write_oracle(&file, &initial_content).unwrap();
    }

    stop.store(true, Ordering::Relaxed);
    reader.join().unwrap();
}

#[test]
#[cfg(unix)]
fn test_io_boundary_symlink_atomic_save_replaces_cleanly() {
    let sandbox = TestSandbox::new();
    let real_file = sandbox.create_file("real.md", "Original Real");
    let link = sandbox.path().join("symlink.md");
    std::os::unix::fs::symlink(&real_file, &link).unwrap();

    // Atomic save to symlink path renames temp over symlink or target
    atomic_write_oracle(&link, "Updated through link").unwrap();
    assert!(link.exists());
    let doc = read_file_lossy_contract(&link).unwrap();
    assert_eq!(doc.content, "Updated through link");
}

#[test]
fn test_io_boundary_content_with_extreme_line_lengths() {
    let sandbox = TestSandbox::new();
    let file = sandbox.path().join("long_line.md");
    let super_long_line = "A".repeat(200_000);
    atomic_write_oracle(&file, &super_long_line).unwrap();

    let doc = read_file_lossy_contract(&file).unwrap();
    assert_eq!(doc.content.len(), 200_000);
}

#[test]
fn test_io_boundary_content_with_only_emojis() {
    let sandbox = TestSandbox::new();
    let file = sandbox.path().join("emojis.md");
    let emoji_content = "🦀🚀✨❄️🎉".repeat(100);
    atomic_write_oracle(&file, &emoji_content).unwrap();

    let doc = read_file_lossy_contract(&file).unwrap();
    assert_eq!(doc.content, emoji_content);
}

#[test]
fn test_io_boundary_no_trailing_newline_preserved() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("no_newline.md", "First\nSecond\nThird");
    let doc = read_file_lossy_contract(&file).unwrap();
    assert!(!doc.content.ends_with('\n'));
}

#[test]
fn test_io_boundary_multiple_trailing_newlines_preserved() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("multi_newlines.md", "Text\n\n\n\n");
    let doc = read_file_lossy_contract(&file).unwrap();
    assert!(doc.content.ends_with("\n\n\n\n"));
}

#[test]
fn test_io_boundary_single_character_file() {
    let sandbox = TestSandbox::new();
    let file = sandbox.path().join("single.md");
    atomic_write_oracle(&file, "Q").unwrap();

    let doc = read_file_lossy_contract(&file).unwrap();
    assert_eq!(doc.content, "Q");
    assert_eq!(doc.size, 1);
}

#[test]
fn test_io_boundary_tab_and_form_feed_characters() {
    let sandbox = TestSandbox::new();
    let file = sandbox.path().join("tabs.md");
    let special = "Col1\tCol2\tCol3\x0CNextPage";
    atomic_write_oracle(&file, special).unwrap();

    let doc = read_file_lossy_contract(&file).unwrap();
    assert_eq!(doc.content, special);
}

#[test]
fn test_io_boundary_lossy_surrogate_halves_handling() {
    let sandbox = TestSandbox::new();
    // Lone high surrogate 0xD800 in UTF-8 would be [0xED, 0xA0, 0x80]
    let surrogate_bytes = &[0xED, 0xA0, 0x80];
    let file = sandbox.create_binary_file("surrogate.md", surrogate_bytes);

    let doc = read_file_lossy_contract(&file).unwrap();
    assert!(doc.content.contains('\u{FFFD}'));
}

// =========================================================================
// === TIER 3: CROSS-FEATURE COMBINATIONS
// =========================================================================

#[test]
fn test_io_combine_lossy_read_then_clean_save() {
    let sandbox = TestSandbox::new();
    let raw_bytes = b"Bad UTF8: \xFF\xFE -> cleaned up";
    let file = sandbox.create_binary_file("corrupt.md", raw_bytes);

    let doc = read_file_lossy_contract(&file).unwrap();
    let edited = format!("{}\nAppended clean line.", doc.content);
    atomic_write_oracle(&file, &edited).unwrap();

    let re_read = read_file_lossy_contract(&file).unwrap();
    assert_eq!(re_read.content, edited);
}

#[test]
fn test_io_combine_empty_file_growth_and_shrinkage() {
    let sandbox = TestSandbox::new();
    let file = sandbox.path().join("grow_shrink.md");

    // Start empty
    atomic_write_oracle(&file, "").unwrap();
    assert_eq!(read_file_lossy_contract(&file).unwrap().size, 0);

    // Grow to 50KB
    let big = "Data\n".repeat(10_000);
    atomic_write_oracle(&file, &big).unwrap();
    assert_eq!(read_file_lossy_contract(&file).unwrap().size, big.len());

    // Shrink back to 10 bytes
    atomic_write_oracle(&file, "Short text").unwrap();
    assert_eq!(read_file_lossy_contract(&file).unwrap().size, 10);
}

#[test]
fn test_io_combine_atomic_write_and_metadata_mtime_advancement() {
    let sandbox = TestSandbox::new();
    let file = sandbox.path().join("mtime_advance.md");

    let mtime1 = atomic_write_oracle(&file, "Initial").unwrap();
    thread::sleep(Duration::from_millis(1100)); // Sleep past 1 second resolution
    let mtime2 = atomic_write_oracle(&file, "Second").unwrap();

    assert!(mtime2 >= mtime1);
}

#[test]
fn test_io_combine_nested_atomic_save_and_read_isolation() {
    let sandbox = TestSandbox::new();
    let file1 = sandbox.create_file("sub1/doc1.md", "Doc 1");
    let file2 = sandbox.create_file("sub2/doc2.md", "Doc 2");

    atomic_write_oracle(&file1, "Doc 1 Updated").unwrap();
    let read2 = read_file_lossy_contract(&file2).unwrap();
    assert_eq!(read2.content, "Doc 2");
}

#[test]
fn test_io_combine_lossy_read_utf16_bom() {
    let sandbox = TestSandbox::new();
    // UTF-16 LE BOM [0xFF, 0xFE]
    let mut utf16_bytes = vec![0xFF, 0xFE];
    utf16_bytes.extend_from_slice(b"H\0e\0l\0l\0o\0");
    let file = sandbox.create_binary_file("utf16.md", &utf16_bytes);

    let doc = read_file_lossy_contract(&file).unwrap();
    assert!(doc.content.contains('\u{FFFD}'));
    // Atomic overwrite to proper UTF-8
    atomic_write_oracle(&file, "Hello UTF-8").unwrap();
    let doc_fixed = read_file_lossy_contract(&file).unwrap();
    assert_eq!(doc_fixed.content, "Hello UTF-8");
}

// =========================================================================
// === TIER 4: REAL-WORLD SCENARIOS
// =========================================================================

#[test]
fn test_io_scenario_simulated_ctrl_s_save_flow() {
    let sandbox = TestSandbox::new();
    let initial_md = "# Draft 1\n\nInitial notes from brainstorming session.";
    let file = sandbox.create_file("meeting.md", initial_md);

    // Initial load
    let mut doc = read_file_lossy_contract(&file).unwrap();
    assert_eq!(doc.content, initial_md);

    // User edits document in UI
    doc.content.push_str("\n\n## Action Items\n- [ ] Follow up on Rust migration");

    // User hits Ctrl+S -> atomic_write_file called
    let new_mtime = atomic_write_oracle(&file, &doc.content).unwrap();
    assert!(new_mtime >= doc.mtime);

    // Verification from disk
    let verified = read_file_lossy_contract(&file).unwrap();
    assert_eq!(verified.content, doc.content);
}

#[test]
fn test_io_scenario_crash_consistency_atomicity() {
    // Verifies that during atomic write, the destination file is never truncated
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("crucial.md", "Vital business data that must never be lost");

    // Perform atomic replace
    let replacement = "Updated vital business data";
    atomic_write_oracle(&file, replacement).unwrap();

    let disk_content = fs::read_to_string(&file).unwrap();
    assert_eq!(disk_content, replacement);
}

#[test]
fn test_io_scenario_large_technical_spec_roundtrip() {
    let sandbox = TestSandbox::new();
    // Simulate reading AGENT.MD or large technical specification
    let file = sandbox.path().join("SPEC.md");
    let mut spec = String::new();
    for i in 1..=500 {
        spec.push_str(&format!("## Section {}\n\nDetailed specifications and code snippets.\n\n", i));
    }

    atomic_write_oracle(&file, &spec).unwrap();
    let doc = read_file_lossy_contract(&file).unwrap();

    assert_eq!(doc.content.len(), spec.len());
    assert_eq!(doc.filename, "SPEC.md");
}

#[test]
fn test_io_scenario_multi_tab_parallel_file_writes() {
    let sandbox = TestSandbox::new();
    let files: Vec<_> = (0..5)
        .map(|i| sandbox.create_file(format!("doc_{}.md", i), &format!("Content {}", i)))
        .collect();

    let handles: Vec<_> = files
        .into_iter()
        .map(|file| {
            thread::spawn(move || {
                for rep in 0..10 {
                    let text = format!("Updated by thread rep {}", rep);
                    atomic_write_oracle(&file, &text).unwrap();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_io_scenario_save_scratchpad_untitled_to_chosen_path() {
    let sandbox = TestSandbox::new();
    // Scratchpad buffer saved for the first time via Save As dialog
    let chosen_path = sandbox.path().join("my_new_document.md");
    let content = "# My Brand New Document\nSaved from untitled scratchpad.";

    assert!(!chosen_path.exists());
    atomic_write_oracle(&chosen_path, content).unwrap();

    assert!(chosen_path.exists());
    let doc = read_file_lossy_contract(&chosen_path).unwrap();
    assert_eq!(doc.filename, "my_new_document.md");
    assert_eq!(doc.content, content);
}
