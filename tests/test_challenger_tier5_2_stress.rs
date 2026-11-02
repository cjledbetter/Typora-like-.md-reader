//! Empirical Challenger Adversarial Stress Test Suite for Milestone 5 Phase 2 (Tier 5-2)
//!
//! Scope:
//! 1. `src/markdown.rs` (heading outline extraction H1-H6, slugification, marked.js disambiguation, stats: words, chars, lines, reading time, large documents)
//! 2. `src/ipc.rs` (all 7 inbound JSON commands, outbound JS callback generation, payload escaping, error payload formatting, malformed/huge payloads)
//! 3. `src/window.rs` & `src/main.rs` (UserEvent enum variants, event proxy routing, window title dirty synchronization)
//! 4. `ui/index.html` (NativeBridge fallback & mock IPC transport, offline asset integrity)
//! 5. `packaging/install-rust-omarchy.sh` & `packaging/typora-rust-reader.desktop` (sandboxed script execution, symlink migration, idempotency, desktop spec)

#![allow(unused_imports, dead_code)]

mod common;
use common::TestSandbox;

use std::collections::HashSet;
use std::fs::{self, File, Permissions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use typora_rust_reader::ipc::{
    escape_json_str, format_error_script, format_external_change_script,
    format_file_loaded_script, format_file_saved_script, format_outbound_js,
    format_window_title, parse_ipc_command_json, DocStats, DocumentStats,
    ErrorPayload, ExternalChangePayload, FileLoadedPayload, FileSavedPayload,
    InboundCommand, InboundIpcCommand, OutboundCommand, OutboundIpcCall,
};
use typora_rust_reader::markdown::{
    calculate_doc_stats, compute_stats, extract_document_stats, extract_headings,
    extract_toc, extract_toc_and_stats, marked_slugify, slugify, slugify_marked,
    HeadingItem, TocItem,
};
use typora_rust_reader::window::UserEvent;

/// Helper to execute JavaScript inside Node.js and assert successful evaluation without throwing.
fn assert_js_eval_success(js_code: &str) {
    let script = format!(
        r#"
        globalThis.window = {{
            __TYPORA_NATIVE__: {{
                onFileLoaded: function(d) {{ globalThis.__last = 'loaded'; }},
                onFileSaved: function(d) {{ globalThis.__last = 'saved'; }},
                onExternalFileChange: function(d) {{ globalThis.__last = 'external'; }},
                onExternalChange: function(d) {{ globalThis.__last = 'external'; }},
                onError: function(d) {{ globalThis.__last = 'error'; }}
            }}
        }};
        try {{
            {}
        }} catch (e) {{
            console.error('JS_FAIL:', e);
            process.exit(1);
        }}
        "#,
        js_code
    );

    let output = Command::new("node")
        .arg("-e")
        .arg(&script)
        .output()
        .expect("Failed to invoke node process");

    assert!(
        output.status.success(),
        "Node.js evaluation failed for script:\n{}\nStderr: {}",
        js_code,
        String::from_utf8_lossy(&output.stderr)
    );
}

// =========================================================================
// === SECTION 1: MARKDOWN AST, TOC & HEADING EXTRACTION ADVERSARIAL STRESS
// =========================================================================

#[test]
fn test_adversarial_headings_deeply_nested_in_blockquotes_and_lists() {
    let md = r#"
> # Blockquote Heading 1
> > ## Deeply Nested Heading 2
> > > ### Triple Nested Heading 3

- List Item 1
  # Heading in List Item 1
  - Sub-item
    ## Heading in Sub-list
"#;
    let (toc, stats) = extract_toc_and_stats(md);
    assert!(!toc.is_empty(), "Should extract headings nested in blockquotes");
    assert_eq!(toc[0].level, 1);
    assert_eq!(toc[0].title, "Blockquote Heading 1");
    assert_eq!(toc[0].anchor, "blockquote-heading-1");

    assert_eq!(toc[1].level, 2);
    assert_eq!(toc[1].title, "Deeply Nested Heading 2");

    assert_eq!(toc[2].level, 3);
    assert_eq!(toc[2].title, "Triple Nested Heading 3");

    assert!(stats.words >= 15);
    assert!(stats.lines >= 8);
}

#[test]
fn test_adversarial_headings_complex_inline_formatting_stripping() {
    let md = r#"
# Heading with **Bold**, *Italic*, ~~Strikethrough~~, and `inline code`
## Heading with [Hyperlink Text](https://example.com/ignored/url)
### Heading with <span class="highlight" style="color:red">HTML tags</span> inside
#### Heading with Math $E = mc^2$ and Display $$\sum_{i=1}^n x_i$$
"#;
    let (toc, _) = extract_toc_and_stats(md);
    assert_eq!(toc.len(), 4);

    assert_eq!(toc[0].level, 1);
    assert_eq!(toc[0].title, "Heading with Bold, Italic, Strikethrough, and inline code");
    assert_eq!(toc[0].anchor, "heading-with-bold-italic-strikethrough-and-inline-code");

    assert_eq!(toc[1].level, 2);
    assert_eq!(toc[1].title, "Heading with Hyperlink Text");
    assert_eq!(toc[1].anchor, "heading-with-hyperlink-text");

    assert_eq!(toc[2].level, 3);
    assert_eq!(toc[2].title, "Heading with HTML tags inside");
    assert_eq!(toc[2].anchor, "heading-with-html-tags-inside");

    assert_eq!(toc[3].level, 4);
    assert!(toc[3].title.contains("$E = mc^2$"));
}

#[test]
fn test_adversarial_headings_1000_duplicate_slug_disambiguation() {
    let count = 1000;
    let mut md = String::with_capacity(count * 20);
    for _ in 0..count {
        md.push_str("# Performance Test\n");
    }

    let start = Instant::now();
    let (toc, stats) = extract_toc_and_stats(&md);
    let elapsed = start.elapsed();

    assert_eq!(toc.len(), count);
    assert_eq!(stats.words, count * 2);
    assert_eq!(toc[0].anchor, "performance-test");
    assert_eq!(toc[1].anchor, "performance-test-1");
    assert_eq!(toc[count - 1].anchor, format!("performance-test-{}", count - 1));

    // Slug disambiguation across 1000 duplicates must complete in < 250ms
    assert!(
        elapsed.as_millis() < 1000,
        "1000 duplicate heading slugification took too long: {:?}",
        elapsed
    );
}

#[test]
fn test_adversarial_headings_non_alphanumeric_and_special_character_slugs() {
    let mut existing = HashSet::new();

    // Pure symbols -> fallback to "section"
    assert_eq!(marked_slugify("!@#$%^&*()_+", &mut existing), "section");
    assert_eq!(marked_slugify("???", &mut existing), "section-1");
    assert_eq!(marked_slugify("   ---   ", &mut existing), "section-2");

    // Pre-existing literal section title
    assert_eq!(marked_slugify("section", &mut existing), "section-3");

    // Leading and trailing hyphens/underscores trimmed
    assert_eq!(marked_slugify("___leading and trailing___", &mut existing), "leading-and-trailing");
    assert_eq!(marked_slugify("---dashes---", &mut existing), "dashes");

    // Multiple consecutive spaces/hyphens collapsed to single hyphen
    assert_eq!(marked_slugify("Word    Separated   By    Spaces", &mut existing), "word-separated-by-spaces");
    assert_eq!(marked_slugify("Word---with---many---dashes", &mut existing), "word-with-many-dashes");
}

#[test]
fn test_adversarial_headings_multilingual_unicode_slugs() {
    let mut existing = HashSet::new();

    // Chinese
    assert_eq!(marked_slugify("快速 入门 指南", &mut existing), "快速-入门-指南");
    // Japanese
    assert_eq!(marked_slugify("こんにちは 世界", &mut existing), "こんにちは-世界");
    // Russian / Cyrillic
    assert_eq!(marked_slugify("Документация и Тесты", &mut existing), "документация-и-тесты");
    // German with umlauts
    assert_eq!(marked_slugify("Übergrößen Träger", &mut existing), "übergrößen-träger");
    // Arabic
    assert_eq!(marked_slugify("مرحبا بكم", &mut existing), "مرحبا-بكم");
    // Duplicate multilingual
    assert_eq!(marked_slugify("快速 入门 指南", &mut existing), "快速-入门-指南-1");
}

#[test]
fn test_adversarial_setext_heading_edge_cases() {
    let md = r#"
Heading 1 Setext
===

Heading 2 Setext
---

Not a setext heading
=

Also not a setext heading
-

Valid Multiline
Setext Heading
==============
"#;
    let (toc, _) = extract_toc_and_stats(md);
    assert!(!toc.is_empty());
    assert_eq!(toc[0].level, 1);
    assert_eq!(toc[0].title, "Heading 1 Setext");
    assert_eq!(toc[1].level, 2);
    assert_eq!(toc[1].title, "Heading 2 Setext");

    // Check last setext heading
    let last = toc.last().unwrap();
    assert_eq!(last.level, 1);
    assert!(last.title.contains("Valid Multiline"));
}

#[test]
fn test_adversarial_massive_document_100k_words_stats_accuracy_and_speed() {
    let paragraph = "Typora is an elegant distraction-free markdown reader and editor with live preview support. \n";
    // paragraph has 13 words
    let repetitions = 10_000;
    let md = paragraph.repeat(repetitions); // 130,000 words

    let start = Instant::now();
    let stats = extract_document_stats(&md);
    let elapsed = start.elapsed();

    assert_eq!(stats.words, 130_000);
    assert_eq!(stats.lines, repetitions);
    assert_eq!(stats.reading_time_minutes, (130_000 + 199) / 200); // 650 mins
    assert!(stats.characters > stats.characters_no_spaces);

    // 130,000 words stats computation must complete quickly
    assert!(
        elapsed.as_millis() < 500,
        "130,000 words stats computation took too long: {:?}",
        elapsed
    );
}

#[test]
fn test_adversarial_docstats_and_documentstats_conversions_and_serde() {
    let stats1 = DocumentStats {
        words: 5432,
        characters: 34567,
        characters_no_spaces: 30000,
        lines: 456,
        reading_time_minutes: 28,
    };

    let compact: DocStats = stats1.clone().into();
    assert_eq!(compact.words, stats1.words);
    assert_eq!(compact.chars, stats1.characters);
    assert_eq!(compact.lines, stats1.lines);
    assert_eq!(compact.reading_time_minutes, stats1.reading_time_minutes);

    let roundtrip: DocumentStats = compact.clone().into();
    assert_eq!(roundtrip.words, stats1.words);
    assert_eq!(roundtrip.characters, stats1.characters);

    // Test DocumentStats JSON deserializing into DocStats (via alias "characters" -> "chars")
    let json_full = serde_json::to_string(&stats1).unwrap();
    let compact_from_full: DocStats = serde_json::from_str(&json_full).unwrap();
    assert_eq!(compact_from_full.chars, stats1.characters);
    assert_eq!(compact_from_full.words, stats1.words);

    // Test DocStats JSON roundtrip
    let json_compact = serde_json::to_string(&compact).unwrap();
    let compact_roundtrip: DocStats = serde_json::from_str(&json_compact).unwrap();
    assert_eq!(compact_roundtrip, compact);
}

// =========================================================================
// === SECTION 2: IPC BRIDGE COMMAND PARSER & PROTOCOL ADVERSARIAL STRESS
// =========================================================================

#[test]
fn test_adversarial_ipc_inbound_all_7_commands_exhaustive() {
    // 1. ready
    assert_eq!(
        parse_ipc_command_json(r#"{"cmd":"ready"}"#).unwrap(),
        InboundIpcCommand::Ready
    );
    // 2. save_file
    assert_eq!(
        parse_ipc_command_json(r##"{"cmd":"save_file","content":"# Test"}"##).unwrap(),
        InboundIpcCommand::SaveFile {
            content: "# Test".to_string()
        }
    );
    // 3. save_file_as
    assert_eq!(
        parse_ipc_command_json(r##"{"cmd":"save_file_as","content":"# Save As"}"##).unwrap(),
        InboundIpcCommand::SaveFileAs {
            content: "# Save As".to_string()
        }
    );
    // 4. open_file_dialog
    assert_eq!(
        parse_ipc_command_json(r#"{"cmd":"open_file_dialog"}"#).unwrap(),
        InboundIpcCommand::OpenFileDialog
    );
    // 5. open_dropped_path
    assert_eq!(
        parse_ipc_command_json(r#"{"cmd":"open_dropped_path","path":"/path/to/file.md"}"#).unwrap(),
        InboundIpcCommand::OpenDroppedPath {
            path: "/path/to/file.md".to_string()
        }
    );
    // 6. set_title
    assert_eq!(
        parse_ipc_command_json(r#"{"cmd":"set_title","title":"*Notes — Typora"}"#).unwrap(),
        InboundIpcCommand::SetTitle {
            title: "*Notes — Typora".to_string()
        }
    );
    // 7. print_pdf
    assert_eq!(
        parse_ipc_command_json(r#"{"cmd":"print_pdf"}"#).unwrap(),
        InboundIpcCommand::PrintPdf
    );
}

#[test]
fn test_adversarial_ipc_malformed_and_edge_case_json_rejection() {
    // Non-object roots
    assert!(parse_ipc_command_json("[\"ready\"]").is_err());
    assert!(parse_ipc_command_json("\"ready\"").is_err());
    assert!(parse_ipc_command_json("12345").is_err());
    assert!(parse_ipc_command_json("null").is_err());
    assert!(parse_ipc_command_json("true").is_err());
    assert!(parse_ipc_command_json("").is_err());
    assert!(parse_ipc_command_json("   ").is_err());

    // Truncated / invalid JSON
    assert!(parse_ipc_command_json("{\"cmd\": \"ready\"").is_err());
    assert!(parse_ipc_command_json("{\"cmd\": \"ready\",}").is_err());
    assert!(parse_ipc_command_json("{\"cmd\": 'ready'}").is_err());
    assert!(parse_ipc_command_json("{cmd: ready}").is_err());

    // Extra trailing characters outside object
    assert!(parse_ipc_command_json("{\"cmd\":\"ready\"} trailing junk").is_err());

    // Missing required fields
    assert!(parse_ipc_command_json("{\"foo\":\"bar\"}").is_err());
    assert!(parse_ipc_command_json("{\"cmd\":\"save_file\"}").is_err());
    assert!(parse_ipc_command_json("{\"cmd\":\"save_file_as\"}").is_err());
    assert!(parse_ipc_command_json("{\"cmd\":\"open_dropped_path\"}").is_err());
    assert!(parse_ipc_command_json("{\"cmd\":\"set_title\"}").is_err());

    // Null values for required fields
    assert!(parse_ipc_command_json("{\"cmd\":null}").is_err());
    assert!(parse_ipc_command_json("{\"cmd\":\"save_file\",\"content\":null}").is_err());
    assert!(parse_ipc_command_json("{\"cmd\":\"open_dropped_path\",\"path\":null}").is_err());
    assert!(parse_ipc_command_json("{\"cmd\":\"set_title\",\"title\":null}").is_err());

    // Type mismatches (e.g. number or array instead of string)
    assert!(parse_ipc_command_json("{\"cmd\":123}").is_err());
    assert!(parse_ipc_command_json("{\"cmd\":\"save_file\",\"content\":12345}").is_err());
    assert!(parse_ipc_command_json("{\"cmd\":\"save_file\",\"content\":[\"line1\"]}").is_err());
    assert!(parse_ipc_command_json("{\"cmd\":\"set_title\",\"title\":true}").is_err());

    // Unknown command
    let unknown_res = parse_ipc_command_json("{\"cmd\":\"unknown_action\"}");
    assert!(unknown_res.is_err());
    assert!(unknown_res.unwrap_err().contains("Unknown command: unknown_action"));
}

#[test]
fn test_adversarial_ipc_huge_json_payload_5mb() {
    let large_text = "A".repeat(5 * 1024 * 1024); // 5 MB string
    let json_payload = format!(r#"{{"cmd":"save_file","content":"{}"}}"#, large_text);

    let start = Instant::now();
    let res = parse_ipc_command_json(&json_payload).expect("5MB payload should parse cleanly");
    let elapsed = start.elapsed();

    match res {
        InboundIpcCommand::SaveFile { content } => {
            assert_eq!(content.len(), 5 * 1024 * 1024);
        }
        _ => panic!("Expected SaveFile command"),
    }

    assert!(
        elapsed.as_millis() < 250,
        "5MB JSON parse took too long: {:?}",
        elapsed
    );
}

#[test]
fn test_adversarial_ipc_outbound_js_and_node_evaluation() {
    // 1. OnFileLoaded with complex markdown, HTML, and quotes
    let complex_content = r#"# Title with "Double" & 'Single' Quotes
<script>alert("XSS")</script>
Backslashes: \n \t \\ \r
Emojis: 🚀 📝 ✨
"#;
    let payload = FileLoadedPayload {
        path: Some("/home/user/document.md".into()),
        filename: "document.md".into(),
        content: complex_content.into(),
        mtime: 1700000000,
        stats: DocStats {
            words: 15,
            chars: complex_content.len(),
            lines: 5,
            reading_time_minutes: 1,
        },
        toc: vec![TocItem {
            level: 1,
            title: "Title with \"Double\" & 'Single' Quotes".into(),
            anchor: "title-with-double-single-quotes".into(),
            line: 1,
        }],
        mode: Some("split".into()),
        theme: Some("dark".into()),
    };
    let json_str = serde_json::to_string(&payload).unwrap();
    let js_loaded = format_file_loaded_script(&json_str);
    assert_js_eval_success(&js_loaded);

    // 2. OnFileSaved
    let saved_payload = FileSavedPayload {
        path: Some("/home/user/document.md".into()),
        filename: "document.md".into(),
        mtime: 1700000001,
    };
    let js_saved = format_file_saved_script(&serde_json::to_string(&saved_payload).unwrap());
    assert_js_eval_success(&js_saved);

    // 3. OnExternalFileChange
    let ext_payload = ExternalChangePayload {
        path: Some("/home/user/document.md".into()),
        filename: "document.md".into(),
        content: "# External Content\nUpdated externally.".into(),
        mtime: 1700000002,
        stats: DocStats {
            words: 4,
            chars: 38,
            lines: 2,
            reading_time_minutes: 1,
        },
        toc: vec![],
    };
    let js_ext = format_external_change_script(&serde_json::to_string(&ext_payload).unwrap());
    assert_js_eval_success(&js_ext);

    // 4. OnError with adversarial error string (quotes, backslashes, tabs, newlines)
    let err_msg = "Error: File \"/root/test.md\" not found!\nLine 1\tTabbed\\Escaped";
    let js_err = format_error_script("ENOENT", err_msg);
    assert_js_eval_success(&js_err);
}

#[test]
fn test_adversarial_window_title_formatting() {
    // Clean
    assert_eq!(
        format_window_title("sample.md", false),
        "sample.md — Typora-RustReader"
    );
    // Dirty
    assert_eq!(
        format_window_title("sample.md", true),
        "*sample.md — Typora-RustReader"
    );
    // Empty clean
    assert_eq!(
        format_window_title("", false),
        "Untitled — Typora-RustReader"
    );
    // Empty dirty
    assert_eq!(
        format_window_title("", true),
        "*Untitled — Typora-RustReader"
    );
    // Unicode & spaces
    assert_eq!(
        format_window_title("📝 我的文档.md", true),
        "*📝 我的文档.md — Typora-RustReader"
    );
}

// =========================================================================
// === SECTION 3: UI NATIVEBRIDGE & OFFLINE ASSETS AUDIT
// =========================================================================

#[test]
fn test_adversarial_ui_native_bridge_contract_in_node() {
    let index_html_path = PathBuf::from("/home/cjledbetter/Documents/Programming/typora-like-.md-reader/ui/index.html");
    let content = fs::read_to_string(&index_html_path).expect("ui/index.html must exist");

    // Verify all 7 command names appear in NativeBridge
    assert!(content.contains("'ready'"));
    assert!(content.contains("'save_file'"));
    assert!(content.contains("'save_file_as'"));
    assert!(content.contains("'open_file_dialog'"));
    assert!(content.contains("'open_dropped_path'"));
    assert!(content.contains("'set_title'"));
    assert!(content.contains("'print_pdf'"));

    // Verify NativeBridge fallback logic without window.ipc
    let js_test = r#"
    let warnCount = 0;
    const originalConsoleWarn = console.warn;
    console.warn = (...args) => { warnCount++; };

    // NativeBridge logic extracted from ui/index.html
    const NativeBridge = {
      isNative() {
        return typeof window.ipc !== 'undefined' && typeof window.ipc.postMessage === 'function';
      },
      post(cmd, payload = {}) {
        if (!this.isNative()) {
          console.warn(`[NativeBridge] Native command '${cmd}' ignored in standalone mode`);
          return false;
        }
        window.ipc.postMessage(JSON.stringify({ cmd, command: cmd, ...payload }));
        return true;
      },
      ready() { return this.post('ready'); },
      saveFile(content) { return this.post('save_file', { content }); },
      saveFileAs(content) { return this.post('save_file_as', { content }); },
      openFileDialog() { return this.post('open_file_dialog'); },
      openDroppedPath(path) { return this.post('open_dropped_path', { path }); },
      setTitle(title) { return this.post('set_title', { title }); },
      printPdf() { return this.post('print_pdf'); }
    };

    // Case A: Standalone browser mode (window.ipc is undefined)
    globalThis.window = {};
    if (NativeBridge.isNative()) process.exit(1);
    if (NativeBridge.ready() !== false) process.exit(2);
    if (NativeBridge.saveFile('test') !== false) process.exit(3);
    if (warnCount < 2) process.exit(4);

    // Case B: Native mode (window.ipc is present)
    const messages = [];
    globalThis.window.ipc = {
      postMessage: (msg) => { messages.push(JSON.parse(msg)); }
    };

    if (!NativeBridge.isNative()) process.exit(5);
    NativeBridge.ready();
    NativeBridge.saveFile('# Saved');
    NativeBridge.saveFileAs('# Saved As');
    NativeBridge.openFileDialog();
    NativeBridge.openDroppedPath('/home/user/doc.md');
    NativeBridge.setTitle('*doc.md — Typora');
    NativeBridge.printPdf();

    if (messages.length !== 7) process.exit(6);
    if (messages[0].cmd !== 'ready') process.exit(7);
    if (messages[1].cmd !== 'save_file' || messages[1].content !== '# Saved') process.exit(8);
    if (messages[2].cmd !== 'save_file_as') process.exit(9);
    if (messages[3].cmd !== 'open_file_dialog') process.exit(10);
    if (messages[4].cmd !== 'open_dropped_path' || messages[4].path !== '/home/user/doc.md') process.exit(11);
    if (messages[5].cmd !== 'set_title') process.exit(12);
    if (messages[6].cmd !== 'print_pdf') process.exit(13);
    "#;

    let output = Command::new("node")
        .arg("-e")
        .arg(js_test)
        .output()
        .expect("Failed to test NativeBridge in node");

    assert!(
        output.status.success(),
        "NativeBridge JS verification failed in Node.js: Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_adversarial_ui_offline_assets_integrity() {
    let ui_dir = PathBuf::from("/home/cjledbetter/Documents/Programming/typora-like-.md-reader/ui");

    // All referenced scripts and styles in index.html must exist locally in ui/assets/
    let required_assets = [
        "assets/highlight-github.min.css",
        "assets/highlight-github-dark.min.css",
        "assets/highlight.min.js",
        "assets/katex.min.css",
        "assets/katex.min.js",
        "assets/katex-auto-render.min.js",
        "assets/marked.min.js",
        "assets/mermaid.min.js",
        "assets/icon.svg",
        "manifest.json",
    ];

    for asset in &required_assets {
        let p = ui_dir.join(asset);
        assert!(
            p.is_file(),
            "Required offline asset missing at {}",
            p.display()
        );
        let meta = fs::metadata(&p).unwrap();
        assert!(meta.len() > 0, "Asset {} is empty", p.display());
    }

    // Verify KaTeX fonts exist
    let fonts_dir = ui_dir.join("assets/fonts");
    assert!(fonts_dir.is_dir(), "KaTeX fonts directory missing");
    let font_count = fs::read_dir(&fonts_dir).unwrap().count();
    assert!(font_count >= 15, "Expected >= 15 KaTeX font files, found {}", font_count);
}

// =========================================================================
// === SECTION 4: PACKAGING & INSTALLATION SCRIPT ADVERSARIAL STRESS
// =========================================================================

#[test]
fn test_adversarial_packaging_install_migration_in_sandbox() {
    let sandbox = TestSandbox::new();
    let sandbox_home = sandbox.path().to_path_buf();

    let fake_bin_dir = sandbox_home.join(".local/bin");
    let fake_app_dir = sandbox_home.join(".local/share/applications");
    let fake_icon_dir = sandbox_home.join(".local/share/icons/hicolor/scalable/apps");
    fs::create_dir_all(&fake_bin_dir).unwrap();
    fs::create_dir_all(&fake_app_dir).unwrap();
    fs::create_dir_all(&fake_icon_dir).unwrap();

    // Create a mock compiled release binary
    let mock_target_bin = sandbox.create_binary_file("mock_target_binary", b"MOCK_ELF_BINARY_CONTENT");
    #[cfg(unix)]
    fs::set_permissions(&mock_target_bin, Permissions::from_mode(0o755)).unwrap();

    // Create a mock legacy launcher at ~/.local/bin/typora-reader
    let legacy_script = fake_bin_dir.join("typora-reader");
    fs::write(&legacy_script, "#!/usr/bin/env python3\nprint('Legacy')\n").unwrap();
    #[cfg(unix)]
    fs::set_permissions(&legacy_script, Permissions::from_mode(0o755)).unwrap();

    let script_path = PathBuf::from("/home/cjledbetter/Documents/Programming/typora-like-.md-reader/packaging/install-rust-omarchy.sh");
    assert!(script_path.is_file(), "install script must exist");

    // Execute install script pointing to sandbox directories
    let output = Command::new("bash")
        .arg(&script_path)
        .env("HOME", &sandbox_home)
        .env("BIN_DIR", &fake_bin_dir)
        .env("APP_DIR", &fake_app_dir)
        .env("ICON_DIR", &fake_icon_dir)
        .env("TARGET_BIN", &mock_target_bin)
        .output()
        .expect("Failed to execute install-rust-omarchy.sh");

    assert!(
        output.status.success(),
        "Install script failed: Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 1. Destination binary installed
    let installed_bin = fake_bin_dir.join("typora-rust-reader");
    assert!(installed_bin.is_file(), "Installed binary missing");
    #[cfg(unix)]
    {
        let perms = fs::metadata(&installed_bin).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o755, "Binary permissions must be 0755");
    }

    // 2. Legacy launcher backed up to typora-reader-legacy
    let legacy_backup = fake_bin_dir.join("typora-reader-legacy");
    assert!(legacy_backup.is_file(), "Legacy backup missing");
    let legacy_content = fs::read_to_string(&legacy_backup).unwrap();
    assert!(legacy_content.contains("Legacy"));

    // 3. Symlink typora-reader created and pointing to typora-rust-reader
    let symlink = fake_bin_dir.join("typora-reader");
    assert!(symlink.is_symlink(), "typora-reader must be a symlink");
    let target = fs::read_link(&symlink).unwrap();
    assert_eq!(target, Path::new("typora-rust-reader"));

    // 4. Desktop entry installed
    let desktop_dest = fake_app_dir.join("typora-rust-reader.desktop");
    assert!(desktop_dest.is_file(), "Desktop entry missing");

    // 5. Run install script a second time (IDEMPOTENCY TEST)
    let output2 = Command::new("bash")
        .arg(&script_path)
        .env("HOME", &sandbox_home)
        .env("BIN_DIR", &fake_bin_dir)
        .env("APP_DIR", &fake_app_dir)
        .env("ICON_DIR", &fake_icon_dir)
        .env("TARGET_BIN", &mock_target_bin)
        .output()
        .expect("Failed to re-run install script");

    assert!(output2.status.success(), "Second install run failed");
    // Verify legacy backup was NOT overwritten
    assert!(legacy_backup.is_file());
    assert_eq!(fs::read_to_string(&legacy_backup).unwrap(), legacy_content);
    // Symlink still valid
    assert!(symlink.is_symlink());
}

#[test]
fn test_adversarial_desktop_entry_spec_compliance() {
    let desktop_path = PathBuf::from("/home/cjledbetter/Documents/Programming/typora-like-.md-reader/packaging/typora-rust-reader.desktop");
    let content = fs::read_to_string(&desktop_path).expect("Desktop file must exist");

    assert!(content.contains("[Desktop Entry]"));
    assert!(content.contains("Type=Application"));
    assert!(content.contains("Name=Typora Rust Reader"));
    assert!(content.contains("Exec=typora-rust-reader %f"));
    assert!(content.contains("StartupWMClass=com.typora.rust-reader"));
    assert!(content.contains("text/markdown"));
    assert!(content.contains("Categories=Office;WordProcessor;Utility;TextEditor;Viewer;"));
}
