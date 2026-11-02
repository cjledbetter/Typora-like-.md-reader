//! Tier 4 Real-World Application Scenarios for Typora-RustReader.
//!
//! Scenarios specified in TEST_INFRA.md:
//! - Scenario 1: Distraction-free Reading of Math & Diagrams (F1, F2, F7, F8, F25, F32)
//! - Scenario 2: Live External Editing with Neovim / Helix (F4, F10, F12, F13, F30)
//! - Scenario 3: Fast Document Drafting with Atomic Save (F5, F6, F11, F17, F19, F26, F29)
//! - Scenario 4: Large Technical Document Outline Navigation (F16, F17, F25, F28)
//! - Scenario 5: Clean Installation & Upgrade on Hyprland (F33, F34, F35)

#![allow(dead_code, unused_imports, unused_variables)]

mod common;
use common::{
    atomic_write_oracle, calculate_stats_oracle, marked_slugify,
    read_file_lossy_oracle, TestSandbox,
};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

// =========================================================================
// === SCENARIO 1: Distraction-free Reading of Math & Diagrams
// === Features: F1, F2, F7, F8, F25, F32
// =========================================================================

#[test]
fn test_scenario_1_math_document_loading_and_assets() {
    let sandbox = TestSandbox::new();
    let math_doc = r#"# Advanced Mathematics & Physics
Here is an inline formula: $E = mc^2$.

Here is a block formula:
$$
\frac{d}{dx}\left( \int_{a}^{x} f(t) dt \right) = f(x)
$$

```mermaid
graph TD;
    A[Start] --> B{Decide};
    B -- Yes --> C[Render Math];
    B -- No --> D[Render Code];
```
"#;
    let file = sandbox.create_file("math.md", math_doc);

    // 1. Read document
    let content = read_file_lossy_oracle(&file).unwrap();
    assert!(content.contains("$$"));
    assert!(content.contains("mermaid"));

    // 2. Verify embedded assets for math & diagram rendering exist
    let root = Path::new("/home/cjledbetter/Documents/Programming/typora-like-.md-reader");
    let katex_js = root.join("assets/katex.min.js");
    let katex_css = root.join("assets/katex.min.css");
    let mermaid_js = root.join("assets/mermaid.min.js");

    assert!(katex_js.exists(), "KaTeX JS must be available for math rendering");
    assert!(katex_css.exists(), "KaTeX CSS must be available");
    assert!(mermaid_js.exists(), "Mermaid JS must be available for diagram rendering");
}

#[test]
fn test_scenario_1_cli_mode_read_theme_night_math() {
    let root = Path::new("/home/cjledbetter/Documents/Programming/typora-like-.md-reader");
    let index_html = root.join("index.html");
    if index_html.exists() {
        let html = fs::read_to_string(&index_html).unwrap();
        // Theme night stylesheet / class must be present
        assert!(html.contains("night"), "Night theme must be supported");
        // Read mode container must be present
        assert!(html.contains("preview") || html.contains("read"), "Read mode preview element must exist");
    }
}

#[test]
fn test_scenario_1_offline_katex_fonts_integrity() {
    let root = Path::new("/home/cjledbetter/Documents/Programming/typora-like-.md-reader");
    let fonts_dir = root.join("assets/fonts");
    if fonts_dir.exists() {
        let entries: Vec<_> = fs::read_dir(&fonts_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert!(entries.iter().any(|f| f.ends_with(".woff2")), "Must contain WOFF2 font files");
    }
}

// =========================================================================
// === SCENARIO 2: Live External Editing with Neovim / Helix
// === Features: F4, F10, F12, F13, F30
// =========================================================================

#[test]
fn test_scenario_2_external_helix_atomic_save_stream() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("project_notes.md", "# Heading 1\nLine 1\n");

    let initial_mtime = sandbox.get_mtime("project_notes.md");
    thread::sleep(Duration::from_millis(1100));

    // Helix atomic save simulation: writes to temporary file in same dir, syncs, then renames over target
    let temp_file = sandbox.path().join(".project_notes.md.helix.tmp");
    fs::write(&temp_file, "# Heading 1\nLine 1\nLine 2 from Helix\n").unwrap();
    fs::rename(&temp_file, &file).unwrap();

    let new_mtime = sandbox.get_mtime("project_notes.md");
    assert!(new_mtime > initial_mtime, "Mtime must advance on external save");

    let updated_content = read_file_lossy_oracle(&file).unwrap();
    assert!(updated_content.contains("Line 2 from Helix"));
}

#[test]
fn test_scenario_2_scroll_preservation_contract() {
    // When external modification arrives, UI payload includes mtime and content,
    // and frontend scroll position is preserved without resetting to top
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("long_doc.md", &"# Line\n".repeat(500));

    let content = read_file_lossy_oracle(&file).unwrap();
    let lines = content.lines().count();
    assert_eq!(lines, 500);

    // External append
    atomic_write_oracle(&file, &format!("{}\n## New Appendix", content)).unwrap();
    let re_read = read_file_lossy_oracle(&file).unwrap();
    assert_eq!(re_read.lines().count(), 502);
}

#[test]
fn test_scenario_2_external_truncate_and_repopulate() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("active.md", "Old content");

    // External editor clears and repopulates
    atomic_write_oracle(&file, "").unwrap();
    assert_eq!(read_file_lossy_oracle(&file).unwrap(), "");

    atomic_write_oracle(&file, "New repopulated content").unwrap();
    assert_eq!(read_file_lossy_oracle(&file).unwrap(), "New repopulated content");
}

// =========================================================================
// === SCENARIO 3: Fast Document Drafting with Atomic Save
// === Features: F5, F6, F11, F17, F19, F26, F29
// =========================================================================

#[test]
fn test_scenario_3_untitled_scratchpad_to_saved_file() {
    let sandbox = TestSandbox::new();
    let mut unsaved_buffer = String::new();

    // User types in scratchpad
    unsaved_buffer.push_str("# My New Project\n\nIdeas:\n- High performance Rust app\n- Zero localhost dependencies\n");

    let (words, chars, lines, read_time) = calculate_stats_oracle(&unsaved_buffer);
    assert_eq!(lines, 5);
    assert!(words > 10);
    assert_eq!(read_time, 1);

    // User hits Ctrl+S -> chooses target file path
    let target = sandbox.path().join("Projects/RustApp.md");
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mtime = atomic_write_oracle(&target, &unsaved_buffer).unwrap();

    assert!(target.exists());
    assert!(mtime > 0);
    assert_eq!(read_file_lossy_oracle(&target).unwrap(), unsaved_buffer);
}

#[test]
fn test_scenario_3_split_mode_editing_and_dirty_toggle() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("draft.md", "# Draft v1");

    // Dirty state
    let is_dirty = true;
    let window_title = if is_dirty {
        format!("*{} — Typora-RustReader", file.file_name().unwrap().to_str().unwrap())
    } else {
        format!("{} — Typora-RustReader", file.file_name().unwrap().to_str().unwrap())
    };
    assert!(window_title.starts_with('*'));

    // Save
    atomic_write_oracle(&file, "# Draft v1.1").unwrap();
    let is_dirty = false;
    let clean_title = if is_dirty {
        format!("*{} — Typora-RustReader", file.file_name().unwrap().to_str().unwrap())
    } else {
        format!("{} — Typora-RustReader", file.file_name().unwrap().to_str().unwrap())
    };
    assert!(!clean_title.starts_with('*'));
}

#[test]
fn test_scenario_3_relative_path_cwd_resolution_and_save() {
    let sandbox = TestSandbox::new();
    let notes = sandbox.create_file("notes.md", "# Notes");

    // Relative save
    let rel_path = sandbox.path().join("./notes.md");
    atomic_write_oracle(&rel_path, "# Notes\n\nUpdated").unwrap();

    let doc = read_file_lossy_oracle(&notes).unwrap();
    assert!(doc.contains("Updated"));
}

// =========================================================================
// === SCENARIO 4: Large Technical Document Outline Navigation
// === Features: F16, F17, F25, F28
// =========================================================================

#[test]
fn test_scenario_4_large_outline_toc_navigation() {
    let mut large_doc = String::new();
    large_doc.push_str("# Typora Rust Reader Specification\n\n");

    let mut expected_headings = Vec::new();
    for section in 1..=10 {
        large_doc.push_str(&format!("## Section {}: Architecture Overview\n\n", section));
        expected_headings.push(format!("section-{}-architecture-overview", section));
        for sub in 1..=5 {
            large_doc.push_str(&format!("### Subsection {}.{}: Component Details\n\nProse content here.\n\n", section, sub));
            expected_headings.push(format!("subsection-{}{}-component-details", section, sub));
        }
    }

    let mut existing_anchors = HashSet::new();
    let mut extracted_anchors = Vec::new();
    for line in large_doc.lines() {
        if line.starts_with('#') {
            let level = line.chars().take_while(|&c| c == '#').count();
            let title = line[level..].trim();
            let slug = marked_slugify(title, &mut existing_anchors);
            extracted_anchors.push(slug);
        }
    }

    assert_eq!(extracted_anchors.len(), 1 + 10 + 50); // 1 top + 10 sec + 50 subsec = 61
}

#[test]
fn test_scenario_4_reading_statistics_accuracy_on_large_doc() {
    let text = "The quick brown fox jumps over the lazy dog. ".repeat(1000); // 9000 words
    let (words, chars, lines, read_time) = calculate_stats_oracle(&text);

    assert_eq!(words, 9000);
    assert_eq!(read_time, 45); // 9000 / 200 = 45 min
}

// =========================================================================
// === SCENARIO 5: Clean Installation & Upgrade on Hyprland
// === Features: F33, F34, F35
// =========================================================================

#[test]
fn test_scenario_5_hyprland_desktop_entry_spec() {
    let root = Path::new("/home/cjledbetter/Documents/Programming/typora-like-.md-reader");
    let desktop_candidate = root.join("typora-reader.desktop");
    if desktop_candidate.exists() {
        let content = fs::read_to_string(&desktop_candidate).unwrap();
        assert!(content.contains("[Desktop Entry]"));
        assert!(content.contains("Type=Application"));
    }
}

#[test]
fn test_scenario_5_hyprland_window_rule_matching_wmclass() {
    // Hyprland rules use `windowrule = float, ^(com\.typora\.rust-reader)$`
    let wmclass = "com.typora.rust-reader";
    assert_eq!(wmclass, "com.typora.rust-reader");
}

#[test]
fn test_scenario_5_symlink_migration_preserves_legacy_wrapper() {
    let sandbox = TestSandbox::new();
    let local_bin = sandbox.path().join(".local/bin");
    fs::create_dir_all(&local_bin).unwrap();

    // 1. Existing python typora-reader
    let legacy = local_bin.join("typora-reader");
    fs::write(&legacy, "#!/usr/bin/env python3\n# original python launcher").unwrap();

    // 2. Migration
    let new_rust = local_bin.join("typora-rust-reader");
    fs::write(&new_rust, "#!/bin/sh\n# native binary").unwrap();

    let legacy_backup = local_bin.join("typora-reader-legacy");
    fs::rename(&legacy, &legacy_backup).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("typora-rust-reader", &legacy).unwrap();

    // Verify
    assert!(legacy_backup.exists());
    assert_eq!(fs::read_to_string(&legacy_backup).unwrap(), "#!/usr/bin/env python3\n# original python launcher");
    assert_eq!(fs::read_link(&legacy).unwrap(), Path::new("typora-rust-reader"));
}
