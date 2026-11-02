//! Tier 2 Boundary, Adversarial & Resource Stress Test Suite for Typora-RustReader.
//!
//! Focus Areas:
//! 1. Encoding & Escaping Integrity (Control characters, Unicode normalization, UTF-8 sequences)
//! 2. Resource & Size Stress (Extreme document sizes, deep directories, massive outlines)
//! 3. Filesystem Fault Injection (Read-only trees, symlink loops, dangling links, race conditions)
//! 4. Malformed Input Fuzzing (Corrupted JSON, illegal CLI flags, non-ASCII command tokens)

#![allow(dead_code, unused_imports, unused_variables)]

mod common;
use common::{atomic_write_oracle, calculate_stats_oracle, marked_slugify, read_file_lossy_oracle, TestSandbox};
use std::collections::HashSet;
use std::fs::{self, File, Permissions};
use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// =========================================================================
// === 1. ENCODING & ESCAPING INTEGRITY (Adversarial Edge Cases)
// =========================================================================

#[test]
fn test_stress_encoding_all_ascii_control_characters() {
    let sandbox = TestSandbox::new();
    let mut control_bytes = Vec::new();
    for b in 0u8..=31u8 {
        control_bytes.push(b);
    }
    let file = sandbox.create_binary_file("controls.md", &control_bytes);
    let doc = read_file_lossy_oracle(&file).unwrap();
    assert_eq!(doc.len(), 32);
}

#[test]
fn test_stress_encoding_overlong_utf8_sequence_rejected_losslessly() {
    let sandbox = TestSandbox::new();
    // Overlong encoding of ASCII 'A' (0x41) as [0xC1, 0x81]
    let overlong = &[0xC1, 0x81];
    let file = sandbox.create_binary_file("overlong.md", overlong);
    let doc = read_file_lossy_oracle(&file).unwrap();
    // Overlong sequences must be replaced with U+FFFD per Unicode Security Standard
    assert!(doc.contains('\u{FFFD}'));
}

#[test]
fn test_stress_encoding_continuation_bytes_without_start() {
    let sandbox = TestSandbox::new();
    let broken = &[0x80, 0xBF, 0x80];
    let file = sandbox.create_binary_file("continuation.md", broken);
    let doc = read_file_lossy_oracle(&file).unwrap();
    assert!(doc.contains('\u{FFFD}'));
}

#[test]
fn test_stress_encoding_truncated_4byte_sequence() {
    let sandbox = TestSandbox::new();
    // 4-byte UTF-8 start byte (0xF0, 0x90, 0x80) without fourth byte
    let truncated = &[0xF0, 0x90, 0x80];
    let file = sandbox.create_binary_file("truncated.md", truncated);
    let doc = read_file_lossy_oracle(&file).unwrap();
    assert!(doc.contains('\u{FFFD}'));
}

#[test]
fn test_stress_encoding_bidirectional_unicode_override_text() {
    let sandbox = TestSandbox::new();
    // RLO (Right-to-Left Override) U+202E and LRO U+202D
    let bidi_text = "# Title \u{202E}dlrow olleh\u{202C} normal";
    let file = sandbox.create_file("bidi.md", bidi_text);
    let read_back = read_file_lossy_oracle(&file).unwrap();
    assert_eq!(read_back, bidi_text);
}

#[test]
fn test_stress_encoding_zero_width_spaces_and_joiners() {
    let sandbox = TestSandbox::new();
    // ZWSP U+200B, ZWJ U+200D, ZWNJ U+200C
    let zw_text = "Word\u{200B}With\u{200C}Zero\u{200D}Width";
    let file = sandbox.create_file("zw.md", zw_text);
    let read_back = read_file_lossy_oracle(&file).unwrap();
    assert_eq!(read_back, zw_text);
}

#[test]
fn test_stress_encoding_unicode_combining_diacritics_normalization() {
    let sandbox = TestSandbox::new();
    // "e" + combining acute accent U+0301 vs precomposed "é" U+00E9
    let decomposed = "e\u{0301}";
    let precomposed = "\u{00E9}";
    let file = sandbox.create_file("accents.md", &format!("{} {}", decomposed, precomposed));
    let read_back = read_file_lossy_oracle(&file).unwrap();
    assert!(read_back.contains(decomposed));
    assert!(read_back.contains(precomposed));
}

#[test]
fn test_stress_encoding_all_printable_latin1_bytes() {
    let sandbox = TestSandbox::new();
    let mut latin1 = Vec::new();
    for b in 160u8..=255u8 {
        latin1.push(b);
    }
    let file = sandbox.create_binary_file("latin1.md", &latin1);
    let read_back = read_file_lossy_oracle(&file).unwrap();
    assert!(!read_back.is_empty());
}

#[test]
fn test_stress_encoding_mixed_line_endings_lf_cr_crlf() {
    let sandbox = TestSandbox::new();
    let mixed = "Line 1\nLine 2\rLine 3\r\nLine 4";
    let file = sandbox.create_file("mixed_endings.md", mixed);
    let read_back = read_file_lossy_oracle(&file).unwrap();
    assert_eq!(read_back, mixed);
}

#[test]
fn test_stress_encoding_slugify_mixed_alphabets() {
    let mut set = HashSet::new();
    // English + Russian + Arabic + Chinese + Japanese + Korean + Emoji
    let title = "Hello Мир سلام 世界 こんにちは 한국어 🚀";
    let slug = marked_slugify(title, &mut set);
    assert!(!slug.is_empty());
    assert!(!slug.contains(' '));
    assert!(!slug.contains("--"));
}

// =========================================================================
// === 2. RESOURCE & SIZE STRESS
// =========================================================================

#[test]
fn test_stress_resource_100_000_lines_document() {
    let sandbox = TestSandbox::new();
    let file = sandbox.path().join("massive_100k.md");

    let mut content = String::with_capacity(2_000_000);
    for i in 1..=100_000 {
        content.push_str("Item ");
        content.push_str(&i.to_string());
        content.push('\n');
    }

    atomic_write_oracle(&file, &content).unwrap();
    let read_back = read_file_lossy_oracle(&file).unwrap();
    assert_eq!(read_back.len(), content.len());
    let (words, _, lines, _) = calculate_stats_oracle(&read_back);
    assert_eq!(lines, 100_000);
    assert_eq!(words, 200_000);
}

#[test]
fn test_stress_resource_5_000_headings_outline() {
    let mut doc = String::new();
    for i in 1..=5_000 {
        doc.push_str(&format!("## Heading Number {}\n\nContent paragraph.\n\n", i));
    }

    let mut set = HashSet::new();
    let mut count = 0;
    for line in doc.lines() {
        if line.starts_with("## ") {
            let title = &line[3..];
            let _ = marked_slugify(title, &mut set);
            count += 1;
        }
    }
    assert_eq!(count, 5_000);
    assert_eq!(set.len(), 5_000);
}

#[test]
fn test_stress_resource_extremely_deep_directory_hierarchy() {
    let sandbox = TestSandbox::new();
    let mut deep_path = sandbox.path().to_path_buf();
    for i in 0..50 {
        deep_path = deep_path.join(format!("level_{}", i));
    }
    fs::create_dir_all(&deep_path).unwrap();
    let target = deep_path.join("deep_doc.md");

    atomic_write_oracle(&target, "# Deep File Content").unwrap();
    assert!(target.exists());
    let content = read_file_lossy_oracle(&target).unwrap();
    assert_eq!(content, "# Deep File Content");
}

#[test]
fn test_stress_resource_single_line_10_mb() {
    let sandbox = TestSandbox::new();
    let file = sandbox.path().join("huge_line.md");
    let huge_line = "x".repeat(10_000_000); // 10 MB single line

    atomic_write_oracle(&file, &huge_line).unwrap();
    let content = read_file_lossy_oracle(&file).unwrap();
    assert_eq!(content.len(), 10_000_000);
}

#[test]
fn test_stress_resource_10_000_words_reading_time() {
    let text = "word ".repeat(10_000);
    let (words, _, _, read_time) = calculate_stats_oracle(&text);
    assert_eq!(words, 10_000);
    assert_eq!(read_time, 50); // 10,000 / 200 = 50 min
}

// =========================================================================
// === 3. FILESYSTEM FAULT INJECTION & RACE CONDITIONS
// =========================================================================

#[test]
fn test_stress_fs_symlink_pointing_to_nonexistent_target() {
    let sandbox = TestSandbox::new();
    let link = sandbox.path().join("broken_link.md");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/nonexistent/file/path.md", &link).unwrap();
        assert!(link.is_symlink());
        assert!(!link.exists()); // Target does not exist

        // Reading broken link should return NotFound error cleanly
        let res = read_file_lossy_oracle(&link);
        assert!(res.is_err());
    }
}

#[test]
fn test_stress_fs_symlink_pointing_to_directory() {
    let sandbox = TestSandbox::new();
    let sub = sandbox.path().join("sub_folder");
    fs::create_dir_all(&sub).unwrap();
    let link = sandbox.path().join("folder_link");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&sub, &link).unwrap();
        assert!(link.is_dir());
    }
}

#[test]
fn test_stress_fs_rapid_100_atomic_writes_with_concurrent_stat() {
    let sandbox = TestSandbox::new();
    let file = sandbox.path().join("race.md");
    atomic_write_oracle(&file, "0").unwrap();

    let stop = Arc::new(AtomicBool::new(false));
    let stop_c = Arc::clone(&stop);
    let file_c = file.clone();

    // Concurrent stat thread
    let statter = thread::spawn(move || {
        while !stop_c.load(Ordering::Relaxed) {
            let _ = fs::metadata(&file_c);
            let _ = fs::read(&file_c);
        }
    });

    for i in 1..=100 {
        atomic_write_oracle(&file, &format!("Iteration {}", i)).unwrap();
    }

    stop.store(true, Ordering::Relaxed);
    statter.join().unwrap();

    let final_content = read_file_lossy_oracle(&file).unwrap();
    assert_eq!(final_content, "Iteration 100");
}

#[test]
#[cfg(unix)]
fn test_stress_fs_write_to_directory_without_executable_bit_fails() {
    let sandbox = TestSandbox::new();
    let no_exec_dir = sandbox.path().join("no_exec");
    fs::create_dir_all(&no_exec_dir).unwrap();

    // Permissions 0666: read+write but NO search/exec
    fs::set_permissions(&no_exec_dir, Permissions::from_mode(0o666)).unwrap();
    let file = no_exec_dir.join("test.md");

    let res = atomic_write_oracle(&file, "Data");
    let _ = fs::set_permissions(&no_exec_dir, Permissions::from_mode(0o755));

    assert!(res.is_err());
}

#[test]
fn test_stress_fs_overwriting_symbolic_link_replaces_atomically() {
    let sandbox = TestSandbox::new();
    let target1 = sandbox.create_file("t1.md", "Target 1");
    let link = sandbox.path().join("active_link.md");

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target1, &link).unwrap();
        atomic_write_oracle(&link, "Replaced link content").unwrap();

        assert!(link.exists());
        let content = read_file_lossy_oracle(&link).unwrap();
        assert_eq!(content, "Replaced link content");
    }
}

// =========================================================================
// === 4. MALFORMED INPUT FUZZING
// =========================================================================

#[test]
fn test_stress_fuzzing_random_binary_in_stats_calculator() {
    let mut random_bytes = Vec::new();
    for i in 0..5000 {
        random_bytes.push((i * 37 % 256) as u8);
    }
    let lossy_str = String::from_utf8_lossy(&random_bytes);
    let (words, chars, lines, read_time) = calculate_stats_oracle(&lossy_str);
    assert!(chars > 0);
}

#[test]
fn test_stress_fuzzing_slugify_extreme_special_strings() {
    let mut set = HashSet::new();
    let inputs = [
        "",
        "    ",
        "---",
        "___",
        "!@#$%^&*()",
        "<script>alert(1)</script>",
        "1234567890",
        "emoji 🚀✨🎉❄️",
        "multi    space    tab\tnewline\n",
        "C++ #1 / C# .NET",
    ];

    for inp in &inputs {
        let slug = marked_slugify(inp, &mut set);
        assert!(!slug.is_empty(), "Slug must never be empty for: {}", inp);
        assert!(!slug.starts_with('-'), "Slug must not start with hyphen: {}", slug);
        assert!(!slug.ends_with('-'), "Slug must not end with hyphen: {}", slug);
    }
}

#[test]
fn test_stress_fuzzing_deeply_nested_brackets_and_quotes() {
    let mut nested = String::new();
    for _ in 0..50 {
        nested.push_str("[*`_");
    }
    nested.push_str("Deep Title");
    for _ in 0..50 {
        nested.push_str("_`*]");
    }

    let mut set = HashSet::new();
    let slug = marked_slugify(&nested, &mut set);
    assert!(!slug.is_empty());
}

#[test]
fn test_stress_fuzzing_zero_byte_files_roundtrip() {
    let sandbox = TestSandbox::new();
    let file = sandbox.path().join("zero.md");
    atomic_write_oracle(&file, "").unwrap();
    assert_eq!(read_file_lossy_oracle(&file).unwrap(), "");

    // Repeat 10 times
    for _ in 0..10 {
        atomic_write_oracle(&file, "").unwrap();
        assert_eq!(read_file_lossy_oracle(&file).unwrap(), "");
    }
}

#[test]
fn test_stress_fuzzing_consecutive_rename_replace_swaps() {
    let sandbox = TestSandbox::new();
    let file_a = sandbox.create_file("a.md", "Content A");
    let file_b = sandbox.create_file("b.md", "Content B");

    for i in 0..20 {
        atomic_write_oracle(&file_a, &format!("A {}", i)).unwrap();
        atomic_write_oracle(&file_b, &format!("B {}", i)).unwrap();
    }

    assert_eq!(read_file_lossy_oracle(&file_a).unwrap(), "A 19");
    assert_eq!(read_file_lossy_oracle(&file_b).unwrap(), "B 19");
}
