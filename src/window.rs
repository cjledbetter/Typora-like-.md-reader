//! Window and WebView initialization for Typora-RustReader.
//!
//! Provides native Wayland / Hyprland windowing via `tao` and `wry` (WebKitGTK).

use std::path::PathBuf;
use tao::{
    dpi::LogicalSize,
    event_loop::{EventLoop, EventLoopBuilder, EventLoopProxy},
    window::{Window, WindowBuilder},
};
use wry::{DragDropEvent, WebView, WebViewBuilder};

#[cfg(target_os = "linux")]
use tao::platform::unix::{EventLoopBuilderExtUnix, WindowExtUnix};
#[cfg(target_os = "linux")]
use wry::WebViewBuilderExtUnix;

/// Application-wide custom user events routed through the Tao event loop.
#[derive(Debug, Clone)]
pub enum UserEvent {
    /// Active file modified on disk (from notify watcher)
    FileChanged(PathBuf),
    /// Active file deleted or moved on disk
    FileDeleted(PathBuf),
    /// Open a target file path (from CLI, native dialog, or Drag-and-Drop)
    OpenFile(PathBuf),
    /// Update the native OS window title bar
    SetTitle(String),
    /// Raw JSON IPC message received from frontend (window.ipc.postMessage)
    IpcMessage(String),
    /// Result from background non-blocking Save As dialog
    SaveFileAsChosen { path: PathBuf, content: String },
}

/// Container holding the native OS window and embedded WebView.
pub struct AppWindow {
    pub window: Window,
    pub webview: WebView,
}

/// Canonical application ID adhering to GApplication / D-Bus reverse-DNS specification.
pub const APP_ID: &str = "com.typora.rust-reader";

/// Builds the Tao `EventLoop<UserEvent>` configured with Wayland app_id.
pub fn build_event_loop() -> Result<EventLoop<UserEvent>, Box<dyn std::error::Error>> {
    let mut builder = EventLoopBuilder::<UserEvent>::with_user_event();

    #[cfg(target_os = "linux")]
    {
        builder.with_app_id(APP_ID);
    }

    Ok(builder.build())
}

/// Builds the native window (1100x800) and attaches the WebKitGTK WebView.
pub fn build_window_and_webview(
    event_loop: &EventLoop<UserEvent>,
    proxy: EventLoopProxy<UserEvent>,
    initial_title: &str,
) -> Result<AppWindow, Box<dyn std::error::Error>> {
    // 1. Build Native Tao Window with logical size
    let window = WindowBuilder::new()
        .with_title(initial_title)
        .with_inner_size(LogicalSize::new(1100.0, 800.0))
        .with_min_inner_size(LogicalSize::new(400.0, 300.0))
        .build(event_loop)
        .map_err(|e| format!("Failed to create Tao window: {e}"))?;

    // 2. Setup WebViewBuilder with Custom Protocol, IPC, and Drag-and-Drop Handlers
    let proxy_ipc = proxy.clone();
    let proxy_dnd = proxy.clone();

    let builder = WebViewBuilder::new()
        .with_custom_protocol(crate::assets::CUSTOM_SCHEME.to_string(), move |_webview_id, request| {
            crate::assets::handle_asset_request(&request)
        })
        .with_url(crate::assets::APP_URL)
        .with_devtools(cfg!(debug_assertions))
        .with_ipc_handler(move |request| {
            let body = request.body().clone();
            if let Err(e) = proxy_ipc.send_event(UserEvent::IpcMessage(body)) {
                eprintln!("[IPC] Failed to forward message to event loop: {e}");
            }
        })
        .with_drag_drop_handler(move |event| {
            if let DragDropEvent::Drop { paths, .. } = event {
                if let Some(path) = paths.into_iter().next() {
                    let _ = proxy_dnd.send_event(UserEvent::OpenFile(path));
                    return true;
                }
            }
            false
        });

    // 3. Attach WebView to Window (GTK default_vbox for Wayland / X11)
    #[cfg(target_os = "linux")]
    let webview = {
        let vbox = window
            .default_vbox()
            .ok_or_else(|| "Failed to retrieve default GTK vbox from tao window".to_string())?;

        builder
            .build_gtk(vbox)
            .map_err(|e| format!("Failed to build WebKitGTK webview via build_gtk: {e}"))?
    };

    #[cfg(not(target_os = "linux"))]
    let webview = builder
        .build(&window)
        .map_err(|e| format!("Failed to build webview: {e}"))?;

    Ok(AppWindow { window, webview })
}
