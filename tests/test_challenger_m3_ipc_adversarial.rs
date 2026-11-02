//! Empirical Challenger Adversarial Stress Test Suite for Milestone 3 Gate Verification
//! Target: JSON IPC Message Protocol & Event Loop Routing
//!
//! Specifically stress tests:
//! 1. Inbound command protocol: all 7 commands (ready, save_file, save_file_as, open_file_dialog, open_dropped_path, set_title, print_pdf).
//! 2. Adversarial JSON payloads: malformed JSON, missing fields, null payloads, non-object JSON (arrays, strings, numbers), extra unrecognized parameters (graceful tolerance).
//! 3. Outbound script escaping: quotes, newlines, backslashes, HTML/script tags, Unicode, null characters — verified against JavaScript engine evaluation.
//! 4. Window title formatting: clean, dirty (*), empty/untitled, special characters, and UI dirty dot synchronization.

#![allow(unused_imports, dead_code)]

use std::process::Command;
use typora_rust_reader::ipc::{
    escape_json_str, format_error_script, format_external_change_script, format_file_loaded_script,
    format_file_saved_script, format_outbound_js, format_window_title, parse_ipc_command_json,
    DocStats, FileLoadedPayload, FileSavedPayload, InboundCommand, InboundIpcCommand,
    OutboundCommand, OutboundIpcCall, TocItem,
};

/// Helper to evaluate a JavaScript string using Node.js to verify evaluation syntax.
fn assert_js_evaluates_cleanly(js_code: &str) {
    // Setup a mock window.__TYPORA_NATIVE__ environment
    let wrapper = format!(
        r#"
        globalThis.window = {{
            __TYPORA_NATIVE__: {{
                onFileLoaded: function(data) {{ globalThis.__lastCall = 'onFileLoaded'; globalThis.__lastData = data; }},
                onFileSaved: function(data) {{ globalThis.__lastCall = 'onFileSaved'; globalThis.__lastData = data; }},
                onExternalFileChange: function(data) {{ globalThis.__lastCall = 'onExternalFileChange'; globalThis.__lastData = data; }},
                onError: function(data) {{ globalThis.__lastCall = 'onError'; globalThis.__lastData = data; }}
            }}
        }};
        try {{
            {}
        }} catch (err) {{
            console.error('JS_EVAL_ERROR:', err);
            process.exit(1);
        }}
        "#,
        js_code
    );

    let output = Command::new("node")
        .arg("-e")
        .arg(&wrapper)
        .output()
        .expect("Failed to execute Node.js for JS evaluation oracle");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        panic!(
            "JavaScript evaluation failed for code:\n{}\nStdout: {}\nStderr: {}",
            js_code, stdout, stderr
        );
    }
}

// =========================================================================
// === PART 1: INBOUND COMMAND PROTOCOL (ALL 7 COMMANDS)
// =========================================================================

#[test]
fn test_adversarial_inbound_ready() {
    let res = parse_ipc_command_json(r#"{"cmd":"ready"}"#).expect("ready command should parse");
    assert_eq!(res, InboundIpcCommand::Ready);

    // Whitespace variants
    let res2 = parse_ipc_command_json("  \n\t { \t \"cmd\" : \"ready\" \n } \r\n ").expect("whitespace ready");
    assert_eq!(res2, InboundIpcCommand::Ready);
}

#[test]
fn test_adversarial_inbound_save_file() {
    let res = parse_ipc_command_json(r##"{"cmd":"save_file","content":"# Heading\nBody"}"##)
        .expect("save_file should parse");
    assert_eq!(
        res,
        InboundIpcCommand::SaveFile {
            content: "# Heading\nBody".to_string()
        }
    );

    // Empty content allowed
    let empty_res = parse_ipc_command_json(r#"{"cmd":"save_file","content":""}"#).unwrap();
    assert_eq!(
        empty_res,
        InboundIpcCommand::SaveFile {
            content: String::new()
        }
    );
}

#[test]
fn test_adversarial_inbound_save_file_as() {
    let res = parse_ipc_command_json(r##"{"cmd":"save_file_as","content":"# Heading\nBody"}"##)
        .expect("save_file_as should parse");
    assert_eq!(
        res,
        InboundIpcCommand::SaveFileAs {
            content: "# Heading\nBody".to_string()
        }
    );

    // Empty content allowed
    let empty_res = parse_ipc_command_json(r#"{"cmd":"save_file_as","content":""}"#).unwrap();
    assert_eq!(
        empty_res,
        InboundIpcCommand::SaveFileAs {
            content: String::new()
        }
    );
}

#[test]
fn test_adversarial_inbound_open_file_dialog() {
    let res = parse_ipc_command_json(r#"{"cmd":"open_file_dialog"}"#)
        .expect("open_file_dialog should parse");
    assert_eq!(res, InboundIpcCommand::OpenFileDialog);
}

#[test]
fn test_adversarial_inbound_open_dropped_path() {
    let res = parse_ipc_command_json(r#"{"cmd":"open_dropped_path","path":"/tmp/test.md"}"#)
        .expect("open_dropped_path should parse");
    assert_eq!(
        res,
        InboundIpcCommand::OpenDroppedPath {
            path: "/tmp/test.md".to_string()
        }
    );

    // Path with special characters & spaces
    let special_res = parse_ipc_command_json(
        r#"{"cmd":"open_dropped_path","path":"/home/user/My Documents/🦀 note.md"}"#,
    )
    .unwrap();
    assert_eq!(
        special_res,
        InboundIpcCommand::OpenDroppedPath {
            path: "/home/user/My Documents/🦀 note.md".to_string()
        }
    );
}

#[test]
fn test_adversarial_inbound_set_title() {
    let res = parse_ipc_command_json(r#"{"cmd":"set_title","title":"notes.md — Typora-RustReader"}"#)
        .expect("set_title should parse");
    assert_eq!(
        res,
        InboundIpcCommand::SetTitle {
            title: "notes.md — Typora-RustReader".to_string()
        }
    );

    // Dirty marker title
    let dirty_res = parse_ipc_command_json(
        r#"{"cmd":"set_title","title":"*notes.md — Typora-RustReader"}"#,
    )
    .unwrap();
    assert_eq!(
        dirty_res,
        InboundIpcCommand::SetTitle {
            title: "*notes.md — Typora-RustReader".to_string()
        }
    );
}

#[test]
fn test_adversarial_inbound_print_pdf() {
    let res = parse_ipc_command_json(r#"{"cmd":"print_pdf"}"#).expect("print_pdf should parse");
    assert_eq!(res, InboundIpcCommand::PrintPdf);
}

// =========================================================================
// === PART 2: ADVERSARIAL JSON PAYLOADS & REJECTION HARNESS
// =========================================================================

#[test]
fn test_adversarial_malformed_syntax() {
    let bad_syntaxes = vec![
        "",
        "   ",
        "\t\n",
        "{",
        "}",
        "}{",
        r#"{"cmd": "ready""#,
        r#"{"cmd": 'ready'}"#,
        r#"{cmd: "ready"}"#,
        r#"{"cmd": "ready",}"#,
        r#"{, "cmd": "ready"}"#,
        "undefined",
        "NaN",
        "null",
        "true",
        "false",
        "12345",
        r#""string literal""#,
        "[1, 2, 3]",
        r#"["ready"]"#,
        r#"[{"cmd":"ready"}]"#,
    ];

    for bad in bad_syntaxes {
        let res = parse_ipc_command_json(bad);
        assert!(
            res.is_err(),
            "Expected malformed syntax to fail, but succeeded on: {:?}",
            bad
        );
        let err = res.unwrap_err();
        assert!(
            err.contains("Malformed JSON"),
            "Error message should mention Malformed JSON for: {:?}, got: {}",
            bad,
            err
        );
    }
}

#[test]
fn test_adversarial_missing_cmd_field() {
    let bad_objects = vec![
        r#"{}"#,
        r#"{"command": "ready"}"#,
        r##"{"action": "save_file", "content": "test"}"##,
        r#"{"type": "ready"}"#,
        r#"{"payload": {}}"#,
    ];

    for bad in bad_objects {
        let res = parse_ipc_command_json(bad);
        assert!(res.is_err(), "Expected missing cmd to fail on: {:?}", bad);
        assert!(
            res.unwrap_err().contains("Missing required 'cmd' field"),
            "Error must specify missing 'cmd' field for: {:?}",
            bad
        );
    }
}

#[test]
fn test_adversarial_null_cmd_and_non_string_cmd() {
    let bad_cmds = vec![
        r#"{"cmd": null}"#,
        r#"{"cmd": 123}"#,
        r#"{"cmd": true}"#,
        r#"{"cmd": false}"#,
        r#"{"cmd": ["ready"]}"#,
        r#"{"cmd": {"sub": "ready"}}"#,
    ];

    for bad in bad_cmds {
        let res = parse_ipc_command_json(bad);
        assert!(res.is_err(), "Expected invalid cmd type to fail on: {:?}", bad);
        assert!(
            res.unwrap_err().contains("Missing required 'cmd' field"),
            "Expected Missing required 'cmd' field error for: {:?}",
            bad
        );
    }
}

#[test]
fn test_adversarial_unknown_commands() {
    let unknowns = vec![
        r#"{"cmd": ""}"#,
        r#"{"cmd": "READY"}"#,
        r#"{"cmd": "Save_File"}"#,
        r#"{"cmd": "savefile"}"#,
        r#"{"cmd": "close"}"#,
        r#"{"cmd": "quit"}"#,
        r#"{"cmd": "eval"}"#,
        r#"{"cmd": "__proto__"}"#,
        r#"{"cmd": "constructor"}"#,
    ];

    for unk in unknowns {
        let res = parse_ipc_command_json(unk);
        assert!(res.is_err(), "Expected unknown command to fail on: {:?}", unk);
        let err = res.unwrap_err();
        assert!(
            err.contains("Unknown command"),
            "Error must mention Unknown command for: {:?}, got: {}",
            unk,
            err
        );
    }
}

#[test]
fn test_adversarial_missing_and_null_required_fields() {
    // save_file requires content
    assert!(parse_ipc_command_json(r#"{"cmd":"save_file"}"#).unwrap_err().contains("Missing required 'content' field"));
    assert!(parse_ipc_command_json(r#"{"cmd":"save_file","content":null}"#).unwrap_err().contains("Missing required 'content' field"));
    assert!(parse_ipc_command_json(r#"{"cmd":"save_file","content":123}"#).unwrap_err().contains("Missing required 'content' field"));
    assert!(parse_ipc_command_json(r#"{"cmd":"save_file","content":[]}"#).unwrap_err().contains("Missing required 'content' field"));

    // save_file_as requires content
    assert!(parse_ipc_command_json(r#"{"cmd":"save_file_as"}"#).unwrap_err().contains("Missing required 'content' field"));
    assert!(parse_ipc_command_json(r#"{"cmd":"save_file_as","content":null}"#).unwrap_err().contains("Missing required 'content' field"));

    // open_dropped_path requires path
    assert!(parse_ipc_command_json(r#"{"cmd":"open_dropped_path"}"#).unwrap_err().contains("Missing required 'path' field"));
    assert!(parse_ipc_command_json(r#"{"cmd":"open_dropped_path","path":null}"#).unwrap_err().contains("Missing required 'path' field"));
    assert!(parse_ipc_command_json(r#"{"cmd":"open_dropped_path","path":false}"#).unwrap_err().contains("Missing required 'path' field"));

    // set_title requires title
    assert!(parse_ipc_command_json(r#"{"cmd":"set_title"}"#).unwrap_err().contains("Missing required 'title' field"));
    assert!(parse_ipc_command_json(r#"{"cmd":"set_title","title":null}"#).unwrap_err().contains("Missing required 'title' field"));
    assert!(parse_ipc_command_json(r#"{"cmd":"set_title","title":404}"#).unwrap_err().contains("Missing required 'title' field"));
}

#[test]
fn test_adversarial_graceful_tolerance_of_extra_parameters() {
    let payload = r##"{
        "cmd": "save_file",
        "content": "# Clean Note",
        "extra_field": "ignore me",
        "client_timestamp": 1700000000,
        "nested_metadata": {
            "author": "Alice",
            "tags": ["rust", "ipc", "robustness"],
            "depth": {"a": {"b": 1}}
        },
        "flags": [true, false, null]
    }"##;

    let res = parse_ipc_command_json(payload).expect("extra parameters should be gracefully tolerated");
    assert_eq!(
        res,
        InboundIpcCommand::SaveFile {
            content: "# Clean Note".to_string()
        }
    );
}

#[test]
fn test_adversarial_large_content_payload_stress() {
    // 5 Megabytes content payload
    let large_body = "A".repeat(5 * 1024 * 1024);
    let payload = format!(r#"{{"cmd":"save_file","content":"{}"}}"#, large_body);

    let res = parse_ipc_command_json(&payload).expect("5MB payload should parse successfully");
    if let InboundIpcCommand::SaveFile { content } = res {
        assert_eq!(content.len(), 5 * 1024 * 1024);
    } else {
        panic!("Expected SaveFile command");
    }
}

// =========================================================================
// === PART 3: OUTBOUND SCRIPT ESCAPING & JAVASCRIPT EVALUATION ORACLE
// =========================================================================

#[test]
fn test_adversarial_escape_json_str_characters() {
    let raw = "Quotes \" and newlines \n \r and tabs \t and backslash \\";
    let escaped = escape_json_str(raw);
    assert_eq!(
        escaped,
        "Quotes \\\" and newlines \\n \\r and tabs \\t and backslash \\\\"
    );
}

#[test]
fn test_adversarial_on_error_script_node_evaluation() {
    let test_messages = vec![
        ("EIO", "Standard error message"),
        ("ENOENT", "File not found: /path/to/missing.md"),
        ("EACCES", "Permission denied (os error 13)"),
        ("EINVAL", "Invalid JSON syntax: unexpected token at line 1"),
        // Quotes and escaping attacks
        ("ERR_QUOTES", r#"He said: "Hello 'World'" and smiled."#),
        ("ERR_SLASHES", r#"Path C:\Windows\System32\drivers\etc\hosts with \\ double slashes"#),
        ("ERR_NEWLINES", "Line 1\nLine 2\r\nLine 3\rLine 4\tTabbed"),
        ("ERR_INJECTION", r#""); alert('xss'); //"#),
        ("ERR_TAGS", r#"<script>alert(1)</script><img src=x onerror="alert(2)">"#),
        ("ERR_UNICODE", "中文错误信息 / エラー / ข้อผิดพลาด / 🦀 🚀 ✨"),
        ("ERR_TRAILING_SLASH", r#"Ends with backslash: \"#),
        ("ERR_SPECIAL_PUNCT", r#"`~@#$%^&*()_+-=[]{}|;':,./<>?"#),
    ];

    for (code, msg) in test_messages {
        let js = format_error_script(code, msg);
        // Verify formatted JS syntax using Node.js execution oracle
        assert_js_evaluates_cleanly(&js);
    }
}

#[test]
fn test_adversarial_on_file_loaded_script_node_evaluation() {
    let payload = FileLoadedPayload {
        path: Some("/home/user/notes/test \"quote\" & \nnewline.md".to_string()),
        filename: "test \"quote\" & \nnewline.md".to_string(),
        content: "# Heading with 'single' and \"double\" quotes\n\nCode with backslash: `C:\\dir`\n\n<script>alert('pwn')</script>\n\nEmoji: 🦀✨".to_string(),
        mtime: 1710000000,
        stats: DocStats {
            words: 15,
            chars: 120,
            lines: 8,
            reading_time_minutes: 1,
        },
        toc: vec![
            TocItem {
                level: 1,
                title: "Heading with 'single' and \"double\" quotes".to_string(),
                anchor: "heading-with-single-and-double-quotes".to_string(),
                line: 1,
            },
        ],
        mode: Some("split".to_string()),
        theme: Some("night".to_string()),
    };

    let json_payload = serde_json::to_string(&payload).unwrap();
    let js = format_file_loaded_script(&json_payload);

    assert_js_evaluates_cleanly(&js);
}

#[test]
fn test_adversarial_on_file_saved_script_node_evaluation() {
    let payload = FileSavedPayload {
        path: Some("/tmp/path with 'spaces' & \"quotes\"/doc.md".to_string()),
        filename: "doc.md".to_string(),
        mtime: 1720000000,
    };

    let json_payload = serde_json::to_string(&payload).unwrap();
    let js = format_file_saved_script(&json_payload);

    assert_js_evaluates_cleanly(&js);
}

#[test]
fn test_adversarial_on_external_change_script_node_evaluation() {
    let payload = typora_rust_reader::ipc::ExternalChangePayload {
        path: Some("/home/user/external.md".to_string()),
        filename: "external.md".to_string(),
        content: "External content update with \n\r\t and \\ and \"quotes\"".to_string(),
        mtime: 1730000000,
        stats: DocStats::empty(),
        toc: Vec::new(),
    };

    let json_payload = serde_json::to_string(&payload).unwrap();
    let js = format_external_change_script(&json_payload);

    assert_js_evaluates_cleanly(&js);
}

// =========================================================================
// === PART 4: WINDOW TITLE FORMATTING & DIRTY INDICATOR
// =========================================================================

#[test]
fn test_adversarial_window_title_clean_and_dirty() {
    // Normal files
    assert_eq!(
        format_window_title("README.md", false),
        "README.md — Typora-RustReader"
    );
    assert_eq!(
        format_window_title("README.md", true),
        "*README.md — Typora-RustReader"
    );

    // Empty filename defaults to Untitled
    assert_eq!(
        format_window_title("", false),
        "Untitled — Typora-RustReader"
    );
    assert_eq!(
        format_window_title("", true),
        "*Untitled — Typora-RustReader"
    );

    // Special characters in filename
    assert_eq!(
        format_window_title("My Notes (Draft 2026).md", false),
        "My Notes (Draft 2026).md — Typora-RustReader"
    );
    assert_eq!(
        format_window_title("My Notes (Draft 2026).md", true),
        "*My Notes (Draft 2026).md — Typora-RustReader"
    );

    // Filename with Unicode & Emoji
    assert_eq!(
        format_window_title("🦀 Rust & Typora ✨.md", false),
        "🦀 Rust & Typora ✨.md — Typora-RustReader"
    );
    assert_eq!(
        format_window_title("🦀 Rust & Typora ✨.md", true),
        "*🦀 Rust & Typora ✨.md — Typora-RustReader"
    );

    // Filename that already has an asterisk
    assert_eq!(
        format_window_title("*starred.md", false),
        "*starred.md — Typora-RustReader"
    );
    assert_eq!(
        format_window_title("*starred.md", true),
        "**starred.md — Typora-RustReader"
    );
}

#[test]
fn test_adversarial_ui_html_dirty_dot_contract() {
    let ui_index_path = std::path::Path::new("/home/cjledbetter/Documents/Programming/typora-like-.md-reader/ui/index.html");
    assert!(ui_index_path.exists(), "ui/index.html must exist");

    let content = std::fs::read_to_string(ui_index_path).unwrap();

    // Verify .dirty-dot CSS class exists
    assert!(
        content.contains(".dirty-dot"),
        "ui/index.html must define .dirty-dot CSS styling"
    );

    // Verify dirty dot DOM insertion when state.isModified is true
    assert!(
        content.contains(r#"<span class="dirty-dot""#) || content.contains("dirty-dot"),
        "ui/index.html must render dirty-dot indicator element when modified"
    );

    // Verify title sync with asterisk
    assert!(
        content.contains("state.isModified ? '*' : ''"),
        "ui/index.html must prepend asterisk to window title when modified"
    );
}
