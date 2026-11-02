//! Standalone native Typora-like Markdown reader and editor.

use std::fs;
use std::path::{Path, PathBuf};
use tao::event::{Event, WindowEvent};
use tao::event_loop::ControlFlow;
use typora_rust_reader::cli;
use typora_rust_reader::dialogs;
use typora_rust_reader::io;
use typora_rust_reader::ipc::{self, InboundIpcCommand, OutboundIpcCall};
use typora_rust_reader::markdown;
use typora_rust_reader::watcher::{DocumentWatcher, WatcherEvent};
use typora_rust_reader::window::{self, UserEvent};

/// Runtime mutable state for the application event loop.
struct AppState {
    active_path: Option<PathBuf>,
    is_dirty: bool,
    mode: String,
    theme: String,
}

#[allow(unused_assignments)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Linux Graphics & WebKitGTK Compatibility Environment
    #[cfg(target_os = "linux")]
    {
        if std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    // 2. Parse CLI Arguments
    let args = cli::parse_cli();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let initial_file = cli::resolve_initial_file(args.file.as_deref(), &cwd)?;
    let initial_mode = args.normalized_mode().to_string();
    let initial_theme = args.normalized_theme().to_string();

    // 3. Build Event Loop & Thread Proxy
    let event_loop = window::build_event_loop()?;
    let proxy = event_loop.create_proxy();

    // 4. Determine Initial Window Title
    let initial_filename = match &initial_file {
        Some(path) => path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("Untitled"),
        None => "Untitled",
    };
    let initial_title = ipc::format_window_title(initial_filename, false);

    // 5. Construct Window & WebKitGTK WebView
    let app_window = window::build_window_and_webview(&event_loop, proxy.clone(), &initial_title)?;
    let window = app_window.window;
    let webview = app_window.webview;

    // 6. Initialize Kernel File Watcher
    let watcher_proxy = proxy.clone();
    let mut watcher = DocumentWatcher::new(move |ev| match ev {
        WatcherEvent::Modified(path) => {
            let _ = watcher_proxy.send_event(UserEvent::FileChanged(path));
        }
        WatcherEvent::Deleted(path) => {
            let _ = watcher_proxy.send_event(UserEvent::FileDeleted(path));
        }
    })?;

    // If an initial file exists, start monitoring its parent directory
    if let Some(ref path) = initial_file {
        if path.exists() {
            let _ = watcher.watch_file(path);
        }
    }

    // 7. Initialize Application State
    let mut state = AppState {
        active_path: initial_file,
        is_dirty: false,
        mode: initial_mode,
        theme: initial_theme,
    };

    // Helper closure to evaluate JS and log errors
    let eval_js = |wv: &wry::WebView, js: &str| {
        if let Err(e) = wv.evaluate_script(js) {
            eprintln!("[WebView] Failed to evaluate script: {e}");
        }
    };

    // 8. Run Tao Event Loop
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }

            Event::UserEvent(user_event) => match user_event {
                UserEvent::SetTitle(title) => {
                    window.set_title(&title);
                    state.is_dirty = title.starts_with('*');
                }

                UserEvent::OpenFile(path) => {
                    match io::read_file_lossy(&path) {
                        Ok(doc) => {
                            let (toc, stats) = markdown::extract_toc_and_stats(&doc.content);
                            let _ = watcher.watch_file(&path);
                            state.active_path = Some(path.clone());
                            state.is_dirty = false;

                            window.set_title(&ipc::format_window_title(&doc.filename, false));

                            let payload = ipc::FileLoadedPayload {
                                path: Some(path.to_string_lossy().to_string()),
                                filename: doc.filename,
                                content: doc.content,
                                mtime: doc.mtime,
                                stats,
                                toc,
                                mode: Some(state.mode.clone()),
                                theme: Some(state.theme.clone()),
                            };
                            if let Ok(json) = serde_json::to_string(&payload) {
                                let js = ipc::format_outbound_js(&OutboundIpcCall::OnFileLoaded {
                                    json_payload: json,
                                });
                                eval_js(&webview, &js);
                            }
                        }
                        Err(e) => {
                            eprintln!("[OpenFile] Error loading {}: {e}", path.display());
                            let js = ipc::format_outbound_js(&OutboundIpcCall::OnError {
                                code: "ENOENT".to_string(),
                                message: format!("Could not open file: {e}"),
                            });
                            eval_js(&webview, &js);
                        }
                    }
                }

                UserEvent::FileChanged(path) => {
                    // Hot-reload only if matches active file
                    let matches_active = if let Some(ref active) = state.active_path {
                        active == &path || fs::canonicalize(active).ok() == fs::canonicalize(&path).ok()
                    } else {
                        false
                    };

                    if matches_active {
                        if let Ok(doc) = io::read_file_lossy(&path) {
                            let (toc, stats) = markdown::extract_toc_and_stats(&doc.content);
                            state.is_dirty = false;
                            window.set_title(&ipc::format_window_title(&doc.filename, false));

                            let payload = ipc::ExternalChangePayload {
                                path: Some(path.to_string_lossy().to_string()),
                                filename: doc.filename,
                                content: doc.content,
                                mtime: doc.mtime,
                                stats,
                                toc,
                            };
                            if let Ok(json) = serde_json::to_string(&payload) {
                                let js = ipc::format_outbound_js(&OutboundIpcCall::OnExternalFileChange {
                                    json_payload: json,
                                });
                                eval_js(&webview, &js);
                            }
                        }
                    }
                }

                UserEvent::FileDeleted(path) => {
                    let matches_active = if let Some(ref active) = state.active_path {
                        active == &path || fs::canonicalize(active).ok() == fs::canonicalize(&path).ok()
                    } else {
                        false
                    };

                    if matches_active {
                        eprintln!("[Watcher] Active file was removed: {}", path.display());
                        let js = ipc::format_outbound_js(&OutboundIpcCall::OnError {
                            code: "ENOENT".to_string(),
                            message: format!("Active file was deleted externally: {}", path.display()),
                        });
                        eval_js(&webview, &js);
                    }
                }

                UserEvent::SaveFileAsChosen { path, content } => {
                    match io::atomic_write_file(&path, &content) {
                        Ok(mtime) => {
                            let filename = path
                                .file_name()
                                .and_then(|f| f.to_str())
                                .unwrap_or("Untitled")
                                .to_string();

                            let _ = watcher.watch_file(&path);
                            watcher.mark_self_saved(mtime);

                            state.active_path = Some(path.clone());
                            state.is_dirty = false;
                            window.set_title(&ipc::format_window_title(&filename, false));

                            let payload = ipc::FileSavedPayload {
                                path: Some(path.to_string_lossy().to_string()),
                                filename,
                                mtime,
                            };
                            if let Ok(json) = serde_json::to_string(&payload) {
                                let js = ipc::format_outbound_js(&OutboundIpcCall::OnFileSaved {
                                    json_payload: json,
                                });
                                eval_js(&webview, &js);
                            }
                        }
                        Err(e) => {
                            eprintln!("[SaveFileAs] Error saving {}: {e}", path.display());
                            let js = ipc::format_outbound_js(&OutboundIpcCall::OnError {
                                code: "EIO".to_string(),
                                message: format!("Failed to save file: {e}"),
                            });
                            eval_js(&webview, &js);
                        }
                    }
                }

                UserEvent::IpcMessage(payload_str) => {
                    match ipc::parse_ipc_command_json(&payload_str) {
                        Ok(InboundIpcCommand::Ready) => {
                            // DOM ready handshake
                            if let Some(ref path) = state.active_path {
                                if let Ok(doc) = io::read_file_lossy(path) {
                                    let (toc, stats) = markdown::extract_toc_and_stats(&doc.content);
                                    window.set_title(&ipc::format_window_title(&doc.filename, false));

                                    let payload = ipc::FileLoadedPayload {
                                        path: Some(path.to_string_lossy().to_string()),
                                        filename: doc.filename,
                                        content: doc.content,
                                        mtime: doc.mtime,
                                        stats,
                                        toc,
                                        mode: Some(state.mode.clone()),
                                        theme: Some(state.theme.clone()),
                                    };
                                    if let Ok(json) = serde_json::to_string(&payload) {
                                        let js = ipc::format_outbound_js(&OutboundIpcCall::OnFileLoaded {
                                            json_payload: json,
                                        });
                                        eval_js(&webview, &js);
                                    }
                                }
                            } else {
                                // Untitled buffer
                                window.set_title(&ipc::format_window_title("Untitled", false));
                                let payload = ipc::FileLoadedPayload {
                                    path: None,
                                    filename: "Untitled".to_string(),
                                    content: String::new(),
                                    mtime: 0,
                                    stats: markdown::DocStats::empty(),
                                    toc: Vec::new(),
                                    mode: Some(state.mode.clone()),
                                    theme: Some(state.theme.clone()),
                                };
                                if let Ok(json) = serde_json::to_string(&payload) {
                                    let js = ipc::format_outbound_js(&OutboundIpcCall::OnFileLoaded {
                                        json_payload: json,
                                    });
                                    eval_js(&webview, &js);
                                }
                            }
                        }

                        Ok(InboundIpcCommand::SaveFile { content }) => {
                            if let Some(ref path) = state.active_path {
                                match io::atomic_write_file(path, &content) {
                                    Ok(mtime) => {
                                        let filename = path
                                            .file_name()
                                            .and_then(|f| f.to_str())
                                            .unwrap_or("Untitled")
                                            .to_string();

                                        watcher.mark_self_saved(mtime);
                                        state.is_dirty = false;
                                        window.set_title(&ipc::format_window_title(&filename, false));

                                        let payload = ipc::FileSavedPayload {
                                            path: Some(path.to_string_lossy().to_string()),
                                            filename,
                                            mtime,
                                        };
                                        if let Ok(json) = serde_json::to_string(&payload) {
                                            let js = ipc::format_outbound_js(&OutboundIpcCall::OnFileSaved {
                                                json_payload: json,
                                            });
                                            eval_js(&webview, &js);
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("[SaveFile] Error saving {}: {e}", path.display());
                                        let js = ipc::format_outbound_js(&OutboundIpcCall::OnError {
                                            code: "EIO".to_string(),
                                            message: format!("Failed to save file: {e}"),
                                        });
                                        eval_js(&webview, &js);
                                    }
                                }
                            } else {
                                // Untitled document: trigger Save As dialog
                                let proxy_clone = proxy.clone();
                                dialogs::spawn_pick_save_file(None, Some("Untitled.md".to_string()), move |chosen| {
                                    if let Some(chosen_path) = chosen {
                                        let _ = proxy_clone.send_event(UserEvent::SaveFileAsChosen {
                                            path: chosen_path,
                                            content,
                                        });
                                    }
                                });
                            }
                        }

                        Ok(InboundIpcCommand::SaveFileAs { content }) => {
                            let starting_dir = state
                                .active_path
                                .as_ref()
                                .and_then(|p| p.parent().map(Path::to_path_buf));
                            let default_name = state
                                .active_path
                                .as_ref()
                                .and_then(|p| p.file_name().and_then(|n| n.to_str()))
                                .map(String::from)
                                .or_else(|| Some("Untitled.md".to_string()));

                            let proxy_clone = proxy.clone();
                            dialogs::spawn_pick_save_file(starting_dir, default_name, move |chosen| {
                                if let Some(chosen_path) = chosen {
                                    let _ = proxy_clone.send_event(UserEvent::SaveFileAsChosen {
                                        path: chosen_path,
                                        content,
                                    });
                                }
                            });
                        }

                        Ok(InboundIpcCommand::OpenFileDialog) => {
                            let starting_dir = state
                                .active_path
                                .as_ref()
                                .and_then(|p| p.parent().map(Path::to_path_buf));
                            dialogs::spawn_pick_open_file_to_proxy(starting_dir, proxy.clone());
                        }

                        Ok(InboundIpcCommand::OpenDroppedPath { path }) => {
                            let _ = proxy.send_event(UserEvent::OpenFile(PathBuf::from(path)));
                        }

                        Ok(InboundIpcCommand::SetTitle { title }) => {
                            window.set_title(&title);
                            state.is_dirty = title.starts_with('*');
                        }

                        Ok(InboundIpcCommand::PrintPdf) => {
                            eval_js(&webview, "window.print();");
                        }

                        Err(err_msg) => {
                            eprintln!("[IPC] Protocol error parsing command: {err_msg}");
                            let js = ipc::format_outbound_js(&OutboundIpcCall::OnError {
                                code: "EINVAL".to_string(),
                                message: err_msg,
                            });
                            eval_js(&webview, &js);
                        }
                    }
                }
            },

            _ => {}
        }
    });
}
