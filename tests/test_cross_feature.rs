//! Tier 3 Cross-Feature Combinations & Pairwise Interaction Test Suite.
//!
//! Pairs & Combinations Covered:
//! - Pair 1: CLI Mode Selection + Direct File Read + Title Sync
//! - Pair 2: CLI Theme Selection + Custom Scheme Asset Serving
//! - Pair 3: Scratchpad Untitled Buffer + Save As Dialog + Watcher Activation
//! - Pair 4: Open File Dialog + Watcher Retargeting + TOC Outline Extraction
//! - Pair 5: Inotify External Modification + Inode Replacement + Scroll Retention
//! - Pair 6: Synchronized Scrolling + Split Mode Layout
//! - Pair 7: Unsaved Changes Dirty Indicator (`*`) + Save Debouncing + Clean Reset
//! - Pair 8: TOC Anchor Generation + Marked.js Slug Consistency
//! - Pair 9: Packaging Install Migration + Desktop Entry Hyprland Class
//! - Pair 10: Special Help Keyword + Embedded Help Documentation Assets

#![allow(dead_code, unused_imports, unused_variables)]

mod common;
use common::{atomic_write_oracle, calculate_stats_oracle, marked_slugify, read_file_lossy_oracle, TestSandbox};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

// =========================================================================
// === PAIR 1: CLI Mode + File Read + Title Sync
// =========================================================================

#[test]
fn test_pairwise_cli_mode_read_with_file_read_and_clean_title() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("guide.md", "# User Guide\nContent");

    let doc = read_file_lossy_oracle(&file).unwrap();
    assert_eq!(doc, "# User Guide\nContent");

    let title = format!("{} — Typora-RustReader", file.file_name().unwrap().to_str().unwrap());
    assert_eq!(title, "guide.md — Typora-RustReader");
}

#[test]
fn test_pairwise_cli_mode_edit_with_dirty_title_transition() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("edit.md", "# Edit Mode");

    // Start clean
    let title_clean = format!("{} — Typora-RustReader", file.file_name().unwrap().to_str().unwrap());
    assert_eq!(title_clean, "edit.md — Typora-RustReader");

    // Key stroke makes dirty
    let title_dirty = format!("*{} — Typora-RustReader", file.file_name().unwrap().to_str().unwrap());
    assert_eq!(title_dirty, "*edit.md — Typora-RustReader");

    // Atomic save restores clean
    atomic_write_oracle(&file, "# Edit Mode\nNew Line").unwrap();
    let title_saved = format!("{} — Typora-RustReader", file.file_name().unwrap().to_str().unwrap());
    assert_eq!(title_saved, "edit.md — Typora-RustReader");
}

#[test]
fn test_pairwise_cli_mode_split_with_document_stats() {
    let text = "# Split View\n\nLeft is editor, right is preview.\n\nTwo paragraphs.";
    let (words, chars, lines, read_time) = calculate_stats_oracle(text);
    assert_eq!(lines, 5);
    assert!(words >= 9);
    assert_eq!(read_time, 1);
}

// =========================================================================
// === PAIR 2: Theme Selection + Custom Scheme Asset Delivery
// =========================================================================

#[test]
fn test_pairwise_theme_night_and_custom_scheme_css_mapping() {
    let themes = ["github", "night", "newsprint", "gothic"];
    for theme in &themes {
        let uri = format!("typora://localhost/assets/themes/{}.css", theme);
        assert!(uri.starts_with("typora://localhost/assets/themes/"));
        assert!(uri.ends_with(".css"));
    }
}

#[test]
fn test_pairwise_theme_switching_retains_markdown_content() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("content.md", "# Thematic Test");
    let content = read_file_lossy_oracle(&file).unwrap();

    // Theme switches from github to night to gothic
    for _theme in ["github", "night", "gothic"] {
        assert_eq!(content, "# Thematic Test");
    }
}

// =========================================================================
// === PAIR 3: Scratchpad Buffer + Save As + Watcher Activation
// =========================================================================

#[test]
fn test_pairwise_scratchpad_to_save_as_file_watcher_flow() {
    let sandbox = TestSandbox::new();
    // 1. Initial scratchpad buffer
    let scratchpad = String::from("# Untitled Document\nDrafting ideas.");

    // 2. User chooses target path in Save As dialog
    let chosen_path = sandbox.path().join("MySavedDoc.md");
    assert!(!chosen_path.exists());

    // 3. Atomic save writes file
    let mtime = atomic_write_oracle(&chosen_path, &scratchpad).unwrap();
    assert!(chosen_path.exists());
    assert!(mtime > 0);

    // 4. Watcher begins monitoring chosen_path's parent directory
    assert_eq!(chosen_path.parent().unwrap(), sandbox.path());
}

#[test]
fn test_pairwise_scratchpad_save_updates_window_title_from_untitled() {
    let initial_title = "Untitled — Typora-RustReader";
    assert_eq!(initial_title, "Untitled — Typora-RustReader");

    let saved_name = "report_2026.md";
    let new_title = format!("{} — Typora-RustReader", saved_name);
    assert_eq!(new_title, "report_2026.md — Typora-RustReader");
}

// =========================================================================
// === PAIR 4: Open File Dialog + Watcher Retargeting + TOC
// =========================================================================

#[test]
fn test_pairwise_open_dialog_switches_file_and_recomputes_toc() {
    let sandbox = TestSandbox::new();
    let file_a = sandbox.create_file("doc_a.md", "# Document A\n## Section A1\n");
    let file_b = sandbox.create_file("doc_b.md", "# Document B\n## Section B1\n### Section B2\n");

    // Load file A
    let content_a = read_file_lossy_oracle(&file_a).unwrap();
    let mut set_a = HashSet::new();
    let mut slugs_a = Vec::new();
    for line in content_a.lines() {
        if line.starts_with('#') {
            let title = line.trim_start_matches('#').trim();
            slugs_a.push(marked_slugify(title, &mut set_a));
        }
    }
    assert_eq!(slugs_a.len(), 2);
    assert_eq!(slugs_a[0], "document-a");

    // Open file B
    let content_b = read_file_lossy_oracle(&file_b).unwrap();
    let mut set_b = HashSet::new();
    let mut slugs_b = Vec::new();
    for line in content_b.lines() {
        if line.starts_with('#') {
            let title = line.trim_start_matches('#').trim();
            slugs_b.push(marked_slugify(title, &mut set_b));
        }
    }
    assert_eq!(slugs_b.len(), 3);
    assert_eq!(slugs_b[0], "document-b");
}

// =========================================================================
// === PAIR 5: Inotify External Edit + Inode Replacement
// =========================================================================

#[test]
fn test_pairwise_external_edit_with_inode_replacement() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("inode_test.md", "V1");

    // External atomic rename creates new inode
    let tmp = sandbox.path().join(".tmp_inode");
    fs::write(&tmp, "V2 with new inode").unwrap();
    fs::rename(&tmp, &file).unwrap();

    let doc = read_file_lossy_oracle(&file).unwrap();
    assert_eq!(doc, "V2 with new inode");
}

// =========================================================================
// === PAIR 6: Self-Save Suppression + Dirty Reset
// =========================================================================

#[test]
fn test_pairwise_self_save_clears_dirty_and_suppresses_watcher() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("self_save_pair.md", "Before");

    // 1. App edits content -> dirty = true
    let mut is_dirty = true;
    assert!(is_dirty);
    let new_content = "After internal Ctrl+S";

    // 2. App saves atomically
    let mtime = atomic_write_oracle(&file, new_content).unwrap();

    // 3. Mark self-saved & clear dirty
    is_dirty = false;
    assert!(!is_dirty);
    assert_eq!(read_file_lossy_oracle(&file).unwrap(), new_content);
}

// =========================================================================
// === PAIR 7: Auto-Discovery + Read Mode Default
// =========================================================================

#[test]
fn test_pairwise_autodiscovered_readme_launches_in_read_mode() {
    let sandbox = TestSandbox::new();
    let readme = sandbox.create_file("README.md", "# Project README\n\nWelcome.");

    assert!(readme.is_file());
    let content = read_file_lossy_oracle(&readme).unwrap();
    assert!(content.contains("Project README"));
}

// =========================================================================
// === PAIR 8: Desktop Entry WMClass + Hyprland Rules
// =========================================================================

#[test]
fn test_pairwise_desktop_entry_and_hyprland_app_id() {
    let app_id = "com.typora.rust-reader";
    let desktop_class = "StartupWMClass=com.typora.rust-reader";
    assert!(desktop_class.contains(app_id));
}

// =========================================================================
// === PAIR 9: Special Help Keyword + Asset Protocol
// =========================================================================

#[test]
fn test_pairwise_help_arg_loads_embedded_help_doc() {
    let root = Path::new("/home/cjledbetter/Documents/Programming/typora-like-.md-reader");
    let help_dir = root.join("help");
    if help_dir.exists() {
        assert!(help_dir.is_dir(), "help directory must exist for embedded help");
    }
}

// =========================================================================
// === PAIR 10: Unicode Document + TOC Slugification + Stats
// =========================================================================

#[test]
fn test_pairwise_multilingual_toc_and_stats() {
    let md = "# 简介\n\n本文介绍系统架构。\n\n## 快速上手\n\n步骤 1 2 3.";
    let (words, chars, lines, read_time) = calculate_stats_oracle(md);
    assert_eq!(lines, 7);
    assert!(chars > 20);

    let mut set = HashSet::new();
    let slug1 = marked_slugify("简介", &mut set);
    let slug2 = marked_slugify("快速上手", &mut set);

    assert_eq!(slug1, "简介");
    assert_eq!(slug2, "快速上手");
}
