//! Bidirectional JSON IPC Bridge Protocol between Rust backend and Wry WebView.
//!
//! Features:
//! - Inbound command parsing from `window.ipc.postMessage`:
//!   - `ready`: DOM handshake; Rust replies with initial document payload
//!   - `save_file`: Atomic save or native Save As if untitled
//!   - `save_file_as`: Save As dialog triggered, updates active file & watcher
//!   - `open_file_dialog`: Native file picker, loads file, switches active watcher
//!   - `open_dropped_path`: Loads markdown file dragged and dropped onto window
//!   - `set_title`: Updates OS window title bar from UI state
//!   - `print_pdf`: Triggers clean printing/export via WebKit
//! - Outbound JavaScript evaluation formatters:
//!   - `window.__TYPORA_NATIVE__.onFileLoaded(payload)`
//!   - `window.__TYPORA_NATIVE__.onFileSaved(payload)`
//!   - `window.__TYPORA_NATIVE__.onExternalFileChange(payload)`
//!   - `window.__TYPORA_NATIVE__.onError(payload)`
//! - Window title formatting with dirty indicator (`*`) and Untitled fallback.

use serde::{Deserialize, Serialize};

pub use crate::markdown::{DocStats, DocumentStats, HeadingItem, TocItem};

/// Commands received from the frontend WebView over `window.ipc.postMessage`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboundIpcCommand {
    /// DOM handshake indicating webview is ready to receive initial document.
    Ready,
    /// Request to save current document with given content.
    SaveFile { content: String },
    /// Request to open Save As dialog and save document with given content.
    SaveFileAs { content: String },
    /// Request to open native Open File dialog.
    OpenFileDialog,
    /// Request to open a file path dropped onto the window.
    OpenDroppedPath { path: String },
    /// Request to update the native OS window title bar.
    SetTitle { title: String },
    /// Request to invoke native print / PDF export dialog.
    PrintPdf,
}

pub type InboundCommand = InboundIpcCommand;

/// Outbound IPC calls evaluated in the WebView via `webview.evaluate_script`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutboundIpcCall {
    OnFileLoaded { json_payload: String },
    OnFileSaved { json_payload: String },
    OnExternalFileChange { json_payload: String },
    OnError { code: String, message: String },
}

pub type OutboundCommand = OutboundIpcCall;

/// Outbound JSON payload for `onFileLoaded`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileLoadedPayload {
    pub path: Option<String>,
    pub filename: String,
    pub content: String,
    pub mtime: u64,
    pub stats: DocStats,
    pub toc: Vec<TocItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}

/// Outbound JSON payload for `onFileSaved`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSavedPayload {
    pub path: Option<String>,
    pub filename: String,
    pub mtime: u64,
}

/// Outbound JSON payload for `onExternalFileChange`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalChangePayload {
    pub path: Option<String>,
    pub filename: String,
    pub content: String,
    pub mtime: u64,
    pub stats: DocStats,
    pub toc: Vec<TocItem>,
}

/// Outbound JSON payload for `onError`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
}

/// Parses an inbound JSON command string received from `window.ipc.postMessage`.
///
/// Conforms strictly to test specification:
/// - Leading/trailing whitespace is trimmed.
/// - Root must be a valid JSON object starting with `{` and ending with `}`.
/// - Requires `cmd` field string.
/// - Requires corresponding fields for specific commands (`content` for `save_file`/`save_file_as`,
///   `path` for `open_dropped_path`, `title` for `set_title`).
/// - Rejects `null` values for required fields.
/// - Ignores unknown extra fields.
pub fn parse_ipc_command_json(json_str: &str) -> Result<InboundIpcCommand, String> {
    let trimmed = json_str.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return Err("Malformed JSON: expected object".to_string());
    }

    let val: serde_json::Value = serde_json::from_str(trimmed)
        .map_err(|e| format!("Malformed JSON: expected object ({e})"))?;

    let obj = val.as_object()
        .ok_or_else(|| "Malformed JSON: expected object".to_string())?;

    let cmd_val = obj.get("cmd")
        .ok_or_else(|| "Missing required 'cmd' field".to_string())?;

    let cmd = cmd_val.as_str()
        .ok_or_else(|| "Missing required 'cmd' field".to_string())?;

    match cmd {
        "ready" => Ok(InboundIpcCommand::Ready),
        "save_file" => {
            let content_val = obj.get("content")
                .ok_or_else(|| "Missing required 'content' field in save_file command".to_string())?;
            let content = content_val.as_str()
                .ok_or_else(|| "Missing required 'content' field in save_file command".to_string())?;
            Ok(InboundIpcCommand::SaveFile {
                content: content.to_string(),
            })
        }
        "save_file_as" => {
            let content_val = obj.get("content")
                .ok_or_else(|| "Missing required 'content' field in save_file_as command".to_string())?;
            let content = content_val.as_str()
                .ok_or_else(|| "Missing required 'content' field in save_file_as command".to_string())?;
            Ok(InboundIpcCommand::SaveFileAs {
                content: content.to_string(),
            })
        }
        "open_file_dialog" => Ok(InboundIpcCommand::OpenFileDialog),
        "open_dropped_path" => {
            let path_val = obj.get("path")
                .ok_or_else(|| "Missing required 'path' field in open_dropped_path command".to_string())?;
            let path = path_val.as_str()
                .ok_or_else(|| "Missing required 'path' field in open_dropped_path command".to_string())?;
            Ok(InboundIpcCommand::OpenDroppedPath {
                path: path.to_string(),
            })
        }
        "set_title" => {
            let title_val = obj.get("title")
                .ok_or_else(|| "Missing required 'title' field in set_title command".to_string())?;
            let title = title_val.as_str()
                .ok_or_else(|| "Missing required 'title' field in set_title command".to_string())?;
            Ok(InboundIpcCommand::SetTitle {
                title: title.to_string(),
            })
        }
        "print_pdf" => Ok(InboundIpcCommand::PrintPdf),
        other => Err(format!("Unknown command: {}", other)),
    }
}

/// Formats outbound JavaScript calls to `window.__TYPORA_NATIVE__`.
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

/// Formats outbound script for `onFileLoaded`.
pub fn format_file_loaded_script(json_payload: &str) -> String {
    format_outbound_js(&OutboundIpcCall::OnFileLoaded {
        json_payload: json_payload.to_string(),
    })
}

/// Formats outbound script for `onFileSaved`.
pub fn format_file_saved_script(json_payload: &str) -> String {
    format_outbound_js(&OutboundIpcCall::OnFileSaved {
        json_payload: json_payload.to_string(),
    })
}

/// Formats outbound script for `onExternalFileChange`.
pub fn format_external_change_script(json_payload: &str) -> String {
    format_outbound_js(&OutboundIpcCall::OnExternalFileChange {
        json_payload: json_payload.to_string(),
    })
}

/// Formats outbound script for `onError`.
pub fn format_error_script(code: &str, message: &str) -> String {
    format_outbound_js(&OutboundIpcCall::OnError {
        code: code.to_string(),
        message: message.to_string(),
    })
}

/// Escapes a string for safe inclusion in a JSON literal within JavaScript.
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

/// Formats the OS window title bar string with optional unsaved changes indicator (`*`).
///
/// Rules:
/// - If `filename` is empty, defaults to `"Untitled"`.
/// - Clean: `"{filename} — Typora-RustReader"`
/// - Dirty: `"*{filename} — Typora-RustReader"`
pub fn format_window_title(filename: &str, is_dirty: bool) -> String {
    let clean_name = if filename.is_empty() {
        "Untitled"
    } else {
        filename
    };
    if is_dirty {
        format!("*{} — Typora-RustReader", clean_name)
    } else {
        format!("{} — Typora-RustReader", clean_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_commands_parsing() {
        assert_eq!(parse_ipc_command_json("{\"cmd\":\"ready\"}").unwrap(), InboundIpcCommand::Ready);
        assert_eq!(
            parse_ipc_command_json("{\"cmd\":\"save_file\",\"content\":\"# Header\"}").unwrap(),
            InboundIpcCommand::SaveFile { content: "# Header".to_string() }
        );
        assert_eq!(
            parse_ipc_command_json("{\"cmd\":\"save_file_as\",\"content\":\"# Save\"}").unwrap(),
            InboundIpcCommand::SaveFileAs { content: "# Save".to_string() }
        );
        assert_eq!(parse_ipc_command_json("{\"cmd\":\"open_file_dialog\"}").unwrap(), InboundIpcCommand::OpenFileDialog);
        assert_eq!(
            parse_ipc_command_json("{\"cmd\":\"open_dropped_path\",\"path\":\"/a.md\"}").unwrap(),
            InboundIpcCommand::OpenDroppedPath { path: "/a.md".to_string() }
        );
        assert_eq!(
            parse_ipc_command_json("{\"cmd\":\"set_title\",\"title\":\"Notes\"}").unwrap(),
            InboundIpcCommand::SetTitle { title: "Notes".to_string() }
        );
        assert_eq!(parse_ipc_command_json("{\"cmd\":\"print_pdf\"}").unwrap(), InboundIpcCommand::PrintPdf);
    }

    #[test]
    fn test_ipc_command_errors() {
        assert!(parse_ipc_command_json("not json").unwrap_err().contains("Malformed JSON"));
        assert!(parse_ipc_command_json("{\"unknown\":1}").unwrap_err().contains("Missing required 'cmd' field"));
        assert!(parse_ipc_command_json("{\"cmd\":\"bad\"}").unwrap_err().contains("Unknown command"));
        assert!(parse_ipc_command_json("{\"cmd\":\"save_file\"}").unwrap_err().contains("Missing required 'content' field"));
        assert!(parse_ipc_command_json("{\"cmd\":\"save_file\",\"content\":null}").is_err());
        assert!(parse_ipc_command_json("{\"cmd\":\"open_dropped_path\"}").unwrap_err().contains("Missing required 'path' field"));
        assert!(parse_ipc_command_json("{\"cmd\":\"set_title\"}").unwrap_err().contains("Missing required 'title' field"));
    }

    #[test]
    fn test_window_title_formatting() {
        assert_eq!(format_window_title("doc.md", false), "doc.md — Typora-RustReader");
        assert_eq!(format_window_title("doc.md", true), "*doc.md — Typora-RustReader");
        assert_eq!(format_window_title("", false), "Untitled — Typora-RustReader");
        assert_eq!(format_window_title("", true), "*Untitled — Typora-RustReader");
    }

    #[test]
    fn test_outbound_js_formatting() {
        let loaded = format_outbound_js(&OutboundIpcCall::OnFileLoaded { json_payload: "{\"test\":true}".to_string() });
        assert_eq!(loaded, "window.__TYPORA_NATIVE__.onFileLoaded({\"test\":true});");

        let saved = format_outbound_js(&OutboundIpcCall::OnFileSaved { json_payload: "{\"mtime\":1}".to_string() });
        assert_eq!(saved, "window.__TYPORA_NATIVE__.onFileSaved({\"mtime\":1});");

        let ext = format_outbound_js(&OutboundIpcCall::OnExternalFileChange { json_payload: "{\"mtime\":2}".to_string() });
        assert_eq!(ext, "window.__TYPORA_NATIVE__.onExternalFileChange({\"mtime\":2});");

        let err = format_outbound_js(&OutboundIpcCall::OnError { code: "EIO".into(), message: "Failed".into() });
        assert_eq!(err, "window.__TYPORA_NATIVE__.onError({\"code\":\"EIO\",\"message\":\"Failed\"});");
    }

    #[test]
    fn test_outbound_script_helpers() {
        let loaded = format_file_loaded_script("{\"test\":true}");
        assert_eq!(loaded, "window.__TYPORA_NATIVE__.onFileLoaded({\"test\":true});");

        let saved = format_file_saved_script("{\"mtime\":1}");
        assert_eq!(saved, "window.__TYPORA_NATIVE__.onFileSaved({\"mtime\":1});");

        let ext = format_external_change_script("{\"mtime\":2}");
        assert_eq!(ext, "window.__TYPORA_NATIVE__.onExternalFileChange({\"mtime\":2});");

        let err = format_error_script("EIO", "Failed");
        assert_eq!(err, "window.__TYPORA_NATIVE__.onError({\"code\":\"EIO\",\"message\":\"Failed\"});");
    }
}
