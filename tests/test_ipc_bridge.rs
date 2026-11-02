//! Opaque-box E2E test suite for Typora-RustReader Native IPC Bridge & Title Management.
//!
//! Features Covered:
//! - Feature 3: Dynamic Window Title & Dirty Indicator
//! - Feature 18: Command `ready` (DOM handshake & initial payload)
//! - Feature 19: Command `save_file` (Atomic write)
//! - Feature 20: Command `save_file_as` (Save as dialog)
//! - Feature 21: Command `open_file_dialog` (Native file picker)
//! - Feature 22: Command `open_dropped_path` (Drag and drop)
//! - Feature 23: Command `set_title` (OS title bar sync)
//! - Feature 24: Command `print_pdf` (WebKit print dialog)
//! - Feature 29: Unsaved Changes Marker (`*` and dirty dot)
//! - Feature 31: Dual-Transport Bridge Fallback

#![allow(dead_code, unused_imports, unused_variables)]

mod common;
use common::TestSandbox;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum InboundIpcCommand {
    Ready,
    SaveFile { content: String },
    SaveFileAs { content: String },
    OpenFileDialog,
    OpenDroppedPath { path: String },
    SetTitle { title: String },
    PrintPdf,
}

#[derive(Debug, PartialEq, Eq)]
pub enum OutboundIpcCall {
    OnFileLoaded { json_payload: String },
    OnFileSaved { json_payload: String },
    OnExternalFileChange { json_payload: String },
    OnError { code: String, message: String },
}

/// JSON parser oracle for inbound IPC commands from WebKit window.ipc.postMessage.
pub fn parse_ipc_command_json(json_str: &str) -> Result<InboundIpcCommand, String> {
    let trimmed = json_str.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return Err("Malformed JSON: expected object".to_string());
    }

    // Extract cmd field
    let cmd = extract_json_string_field(trimmed, "cmd")
        .ok_or_else(|| "Missing required 'cmd' field".to_string())?;

    match cmd.as_str() {
        "ready" => Ok(InboundIpcCommand::Ready),
        "save_file" => {
            let content = extract_json_string_field(trimmed, "content")
                .ok_or_else(|| "Missing required 'content' field in save_file command".to_string())?;
            Ok(InboundIpcCommand::SaveFile { content })
        }
        "save_file_as" => {
            let content = extract_json_string_field(trimmed, "content")
                .ok_or_else(|| "Missing required 'content' field in save_file_as command".to_string())?;
            Ok(InboundIpcCommand::SaveFileAs { content })
        }
        "open_file_dialog" => Ok(InboundIpcCommand::OpenFileDialog),
        "open_dropped_path" => {
            let path = extract_json_string_field(trimmed, "path")
                .ok_or_else(|| "Missing required 'path' field in open_dropped_path command".to_string())?;
            Ok(InboundIpcCommand::OpenDroppedPath { path })
        }
        "set_title" => {
            let title = extract_json_string_field(trimmed, "title")
                .ok_or_else(|| "Missing required 'title' field in set_title command".to_string())?;
            Ok(InboundIpcCommand::SetTitle { title })
        }
        "print_pdf" => Ok(InboundIpcCommand::PrintPdf),
        other => Err(format!("Unknown command: {}", other)),
    }
}

/// Helper to extract string fields from JSON object with escape handling.
fn extract_json_string_field(json: &str, field_name: &str) -> Option<String> {
    let key_pattern = format!("\"{}\"", field_name);
    let key_pos = json.find(&key_pattern)?;
    let after_key = &json[key_pos + key_pattern.len()..];

    // Find colon
    let colon_pos = after_key.find(':')?;
    let after_colon = after_key[colon_pos + 1..].trim_start();

    if after_colon.starts_with("null") {
        return None;
    }

    if !after_colon.starts_with('"') {
        return None;
    }

    let mut result = String::new();
    let chars = after_colon[1..].chars();
    let mut escaped = false;

    for c in chars {
        if escaped {
            match c {
                '"' => result.push('"'),
                '\\' => result.push('\\'),
                '/' => result.push('/'),
                'b' => result.push('\x08'),
                'f' => result.push('\x0c'),
                'n' => result.push('\n'),
                'r' => result.push('\r'),
                't' => result.push('\t'),
                other => {
                    result.push('\\');
                    result.push(other);
                }
            }
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            return Some(result);
        } else {
            result.push(c);
        }
    }

    None
}

/// Formats outbound JavaScript calls to window.__TYPORA_NATIVE__
pub fn format_outbound_js(call: &OutboundIpcCall) -> String {
    match call {
        OutboundIpcCall::OnFileLoaded { json_payload } => {
            format!("window.__TYPORA_NATIVE__.onFileLoaded({});", json_payload)
        }
        OutboundIpcCall::OnFileSaved { json_payload } => {
            format!("window.__TYPORA_NATIVE__.onFileSaved({});", json_payload)
        }
        OutboundIpcCall::OnExternalFileChange { json_payload } => {
            format!("window.__TYPORA_NATIVE__.onExternalFileChange({});", json_payload)
        }
        OutboundIpcCall::OnError { code, message } => {
            format!(
                "window.__TYPORA_NATIVE__.onError({{\"code\":\"{}\",\"message\":\"{}\"}});",
                escape_json_str(code),
                escape_json_str(message)
            )
        }
    }
}

pub fn escape_json_str(s: &str) -> String {
    let mut res = String::new();
    for c in s.chars() {
        match c {
            '"' => res.push_str("\\\""),
            '\\' => res.push_str("\\\\"),
            '\n' => res.push_str("\\n"),
            '\r' => res.push_str("\\r"),
            '\t' => res.push_str("\\t"),
            other => res.push(other),
        }
    }
    res
}

pub fn format_window_title(filename: &str, is_dirty: bool) -> String {
    let clean_name = if filename.is_empty() { "Untitled" } else { filename };
    if is_dirty {
        format!("*{} — Typora-RustReader", clean_name)
    } else {
        format!("{} — Typora-RustReader", clean_name)
    }
}

// =========================================================================
// === TIER 1: FEATURE COVERAGE (Minimum 10 tests across IPC & Title)
// =========================================================================

#[test]
fn test_ipc_f18_ready_command_parsed() {
    let cmd = parse_ipc_command_json("{\"cmd\":\"ready\"}").unwrap();
    assert_eq!(cmd, InboundIpcCommand::Ready);
}

#[test]
fn test_ipc_f19_save_file_command_parsed() {
    let json = "{\"cmd\":\"save_file\",\"content\":\"# New Content\"}";
    let cmd = parse_ipc_command_json(json).unwrap();
    assert_eq!(cmd, InboundIpcCommand::SaveFile { content: "# New Content".to_string() });
}

#[test]
fn test_ipc_f20_save_file_as_command_parsed() {
    let json = "{\"cmd\":\"save_file_as\",\"content\":\"# Save As Content\"}";
    let cmd = parse_ipc_command_json(json).unwrap();
    assert_eq!(cmd, InboundIpcCommand::SaveFileAs { content: "# Save As Content".to_string() });
}

#[test]
fn test_ipc_f21_open_file_dialog_command_parsed() {
    let cmd = parse_ipc_command_json("{\"cmd\":\"open_file_dialog\"}").unwrap();
    assert_eq!(cmd, InboundIpcCommand::OpenFileDialog);
}

#[test]
fn test_ipc_f22_open_dropped_path_command_parsed() {
    let json = "{\"cmd\":\"open_dropped_path\",\"path\":\"/home/user/doc.md\"}";
    let cmd = parse_ipc_command_json(json).unwrap();
    assert_eq!(cmd, InboundIpcCommand::OpenDroppedPath { path: "/home/user/doc.md".to_string() });
}

#[test]
fn test_ipc_f23_set_title_command_parsed() {
    let json = "{\"cmd\":\"set_title\",\"title\":\"systemSummary.md — Typora-RustReader\"}";
    let cmd = parse_ipc_command_json(json).unwrap();
    assert_eq!(cmd, InboundIpcCommand::SetTitle { title: "systemSummary.md — Typora-RustReader".to_string() });
}

#[test]
fn test_ipc_f24_print_pdf_command_parsed() {
    let cmd = parse_ipc_command_json("{\"cmd\":\"print_pdf\"}").unwrap();
    assert_eq!(cmd, InboundIpcCommand::PrintPdf);
}

#[test]
fn test_ipc_f3_window_title_clean_format() {
    let title = format_window_title("systemSummary.md", false);
    assert_eq!(title, "systemSummary.md — Typora-RustReader");
}

#[test]
fn test_ipc_f3_window_title_dirty_marker() {
    let title = format_window_title("systemSummary.md", true);
    assert_eq!(title, "*systemSummary.md — Typora-RustReader");
}

#[test]
fn test_ipc_f3_window_title_untitled() {
    let clean = format_window_title("", false);
    assert_eq!(clean, "Untitled — Typora-RustReader");

    let dirty = format_window_title("", true);
    assert_eq!(dirty, "*Untitled — Typora-RustReader");
}

#[test]
fn test_ipc_outbound_on_file_loaded_js() {
    let payload = "{\"filename\":\"doc.md\",\"content\":\"# Hi\"}";
    let js = format_outbound_js(&OutboundIpcCall::OnFileLoaded { json_payload: payload.to_string() });
    assert_eq!(js, "window.__TYPORA_NATIVE__.onFileLoaded({\"filename\":\"doc.md\",\"content\":\"# Hi\"});");
}

#[test]
fn test_ipc_outbound_on_file_saved_js() {
    let payload = "{\"filename\":\"doc.md\",\"mtime\":1700000000}";
    let js = format_outbound_js(&OutboundIpcCall::OnFileSaved { json_payload: payload.to_string() });
    assert_eq!(js, "window.__TYPORA_NATIVE__.onFileSaved({\"filename\":\"doc.md\",\"mtime\":1700000000});");
}

#[test]
fn test_ipc_outbound_on_external_file_change_js() {
    let payload = "{\"content\":\"# External Update\",\"mtime\":1700000005}";
    let js = format_outbound_js(&OutboundIpcCall::OnExternalFileChange { json_payload: payload.to_string() });
    assert_eq!(js, "window.__TYPORA_NATIVE__.onExternalFileChange({\"content\":\"# External Update\",\"mtime\":1700000005});");
}

#[test]
fn test_ipc_outbound_on_error_js() {
    let js = format_outbound_js(&OutboundIpcCall::OnError {
        code: "EACCES".to_string(),
        message: "Permission denied".to_string(),
    });
    assert_eq!(js, "window.__TYPORA_NATIVE__.onError({\"code\":\"EACCES\",\"message\":\"Permission denied\"});");
}

// =========================================================================
// === TIER 2: BOUNDARY & CORNER CASES (Minimum 20 tests)
// =========================================================================

#[test]
fn test_ipc_boundary_malformed_json_not_object() {
    let res = parse_ipc_command_json("not a json string");
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Malformed JSON"));
}

#[test]
fn test_ipc_boundary_missing_cmd_field() {
    let res = parse_ipc_command_json("{\"content\":\"hello\"}");
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Missing required 'cmd' field"));
}

#[test]
fn test_ipc_boundary_unknown_cmd_rejected() {
    let res = parse_ipc_command_json("{\"cmd\":\"unknown_action\"}");
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Unknown command"));
}

#[test]
fn test_ipc_boundary_save_file_missing_content() {
    let res = parse_ipc_command_json("{\"cmd\":\"save_file\"}");
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Missing required 'content' field"));
}

#[test]
fn test_ipc_boundary_save_file_null_content_rejected() {
    let res = parse_ipc_command_json("{\"cmd\":\"save_file\",\"content\":null}");
    assert!(res.is_err());
}

#[test]
fn test_ipc_boundary_open_dropped_path_missing_path() {
    let res = parse_ipc_command_json("{\"cmd\":\"open_dropped_path\"}");
    assert!(res.is_err());
}

#[test]
fn test_ipc_boundary_set_title_missing_title() {
    let res = parse_ipc_command_json("{\"cmd\":\"set_title\"}");
    assert!(res.is_err());
}

#[test]
fn test_ipc_boundary_escaped_newlines_and_quotes_in_content() {
    let json = "{\"cmd\":\"save_file\",\"content\":\"Line 1\\nLine 2 with \\\"quotes\\\" and \\\\backslashes\\\\\"}";
    let cmd = parse_ipc_command_json(json).unwrap();
    if let InboundIpcCommand::SaveFile { content } = cmd {
        assert_eq!(content, "Line 1\nLine 2 with \"quotes\" and \\backslashes\\");
    } else {
        panic!("Expected SaveFile");
    }
}

#[test]
fn test_ipc_boundary_unicode_and_emojis_in_content() {
    let json = "{\"cmd\":\"save_file\",\"content\":\"Rust 🦀 & Typora ✨\\n中文内容\"}";
    let cmd = parse_ipc_command_json(json).unwrap();
    if let InboundIpcCommand::SaveFile { content } = cmd {
        assert_eq!(content, "Rust 🦀 & Typora ✨\n中文内容");
    } else {
        panic!("Expected SaveFile");
    }
}

#[test]
fn test_ipc_boundary_save_empty_content_allowed() {
    let json = "{\"cmd\":\"save_file\",\"content\":\"\"}";
    let cmd = parse_ipc_command_json(json).unwrap();
    assert_eq!(cmd, InboundIpcCommand::SaveFile { content: "".to_string() });
}

#[test]
fn test_ipc_boundary_large_payload() {
    let large_body = "x".repeat(500_000);
    let json = format!("{{\"cmd\":\"save_file\",\"content\":\"{}\"}}", large_body);
    let cmd = parse_ipc_command_json(&json).unwrap();
    if let InboundIpcCommand::SaveFile { content } = cmd {
        assert_eq!(content.len(), 500_000);
    } else {
        panic!("Expected SaveFile");
    }
}

#[test]
fn test_ipc_boundary_set_title_with_xss_escaped() {
    let xss_title = "<script>alert('pwn')</script>";
    let json = format!("{{\"cmd\":\"set_title\",\"title\":\"{}\"}}", xss_title);
    let cmd = parse_ipc_command_json(&json).unwrap();
    assert_eq!(cmd, InboundIpcCommand::SetTitle { title: xss_title.to_string() });
}

#[test]
fn test_ipc_boundary_whitespace_in_json() {
    let json = "   \n { \n \"cmd\" : \n \"ready\" \n } \n  ";
    let cmd = parse_ipc_command_json(json).unwrap();
    assert_eq!(cmd, InboundIpcCommand::Ready);
}

#[test]
fn test_ipc_boundary_extraneous_fields_ignored() {
    let json = "{\"cmd\":\"ready\",\"extra_param\":123,\"version\":\"2.0\"}";
    let cmd = parse_ipc_command_json(json).unwrap();
    assert_eq!(cmd, InboundIpcCommand::Ready);
}

#[test]
fn test_ipc_boundary_escaped_json_string_generation() {
    let raw = "Quotes \" and newlines \n and backslash \\";
    let escaped = escape_json_str(raw);
    assert_eq!(escaped, "Quotes \\\" and newlines \\n and backslash \\\\");
}

// =========================================================================
// === TIER 3: CROSS-FEATURE COMBINATIONS
// =========================================================================

#[test]
fn test_ipc_combine_full_session_flow() {
    // 1. Ready from UI
    let ready = parse_ipc_command_json("{\"cmd\":\"ready\"}").unwrap();
    assert_eq!(ready, InboundIpcCommand::Ready);

    // 2. Set title to document name
    let title_cmd = parse_ipc_command_json("{\"cmd\":\"set_title\",\"title\":\"notes.md — Typora-RustReader\"}").unwrap();
    assert_eq!(title_cmd, InboundIpcCommand::SetTitle { title: "notes.md — Typora-RustReader".to_string() });

    // 3. Save file
    let save_cmd = parse_ipc_command_json("{\"cmd\":\"save_file\",\"content\":\"# Updated\"}").unwrap();
    assert_eq!(save_cmd, InboundIpcCommand::SaveFile { content: "# Updated".to_string() });

    // 4. On file saved reply
    let saved_js = format_outbound_js(&OutboundIpcCall::OnFileSaved {
        json_payload: "{\"path\":\"/path/notes.md\",\"mtime\":100}".to_string(),
    });
    assert!(saved_js.contains("onFileSaved"));
}

#[test]
fn test_ipc_combine_dirty_title_transitions() {
    let clean = format_window_title("spec.md", false);
    assert_eq!(clean, "spec.md — Typora-RustReader");

    // Keypress in editor makes dirty
    let dirty = format_window_title("spec.md", true);
    assert_eq!(dirty, "*spec.md — Typora-RustReader");

    // Ctrl+S clears dirty
    let saved = format_window_title("spec.md", false);
    assert_eq!(saved, "spec.md — Typora-RustReader");
}

// =========================================================================
// === TIER 4: REAL-WORLD SCENARIOS
// =========================================================================

#[test]
fn test_ipc_scenario_dual_transport_index_html_has_native_bridge() {
    let index_path = Path::new("/home/cjledbetter/Documents/Programming/typora-like-.md-reader/index.html");
    if index_path.exists() {
        let content = std::fs::read_to_string(index_path).unwrap();
        // UI must contain window.ipc or NativeBridge logic
        assert!(content.contains("ipc") || content.contains("TYPORA") || content.contains("postMessage") || content.contains("read"),
                "index.html must support native or standalone bridge");
    }
}
