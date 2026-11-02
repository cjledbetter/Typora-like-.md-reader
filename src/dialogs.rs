//! Native GTK4/Wayland file chooser dialogs via `rfd`.
//!
//! Provides native file selection and save dialogs conforming to
//! Typora-RustReader specifications:
//! - `pick_open_file`: Configures filters for Markdown files (`md`, `markdown`, `mdown`, `txt`), title "Open Markdown Document".
//! - `pick_save_file`: Configures default file name (e.g. `Untitled.md`), filter for `.md`, title "Save Markdown Document".
//! - Non-blocking worker thread spawning to prevent Tao/WebKitGTK event loop freezes under Wayland.
//! - Thread-safe RAII mock harness and environment-variable hooks for 100% headless automated testing in CI.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::window;

/// Canonical window title for the native Open File dialog.
pub const OPEN_DIALOG_TITLE: &str = "Open Markdown Document";

/// Canonical window title for the native Save File dialog.
pub const SAVE_DIALOG_TITLE: &str = "Save Markdown Document";

/// Default file name suggested in the Save As dialog when none is specified.
pub const DEFAULT_SAVE_NAME: &str = "Untitled.md";

/// File extensions permitted in the Open File dialog filter.
pub const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdown", "txt"];

/// File extensions permitted in the Save File dialog filter.
pub const SAVE_EXTENSIONS: &[&str] = &["md"];

// =========================================================================
// === HEADLESS TEST MOCK HARNESS
// =========================================================================

static MOCK_ACTIVE: AtomicBool = AtomicBool::new(false);
static MOCK_OPEN_SELECTION: Mutex<Option<Option<PathBuf>>> = Mutex::new(None);
static MOCK_SAVE_SELECTION: Mutex<Option<Option<PathBuf>>> = Mutex::new(None);

/// Environment variable override for headless Open dialog simulation.
pub const ENV_MOCK_OPEN_FILE: &str = "TYPORA_MOCK_OPEN_FILE";

/// Environment variable override for headless Save dialog simulation.
pub const ENV_MOCK_SAVE_FILE: &str = "TYPORA_MOCK_SAVE_FILE";

static MOCK_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Scoped RAII guard that automatically serializes mock tests and disables the dialog mock upon drop.
pub struct DialogMockGuard {
    _lock: Option<std::sync::MutexGuard<'static, ()>>,
}

impl Drop for DialogMockGuard {
    fn drop(&mut self) {
        disable_mock();
        // MutexGuard drops here
    }
}

/// Sets the mock response for subsequent `pick_open_file` invocations.
///
/// Returns a `DialogMockGuard` that will restore normal behavior when dropped.
pub fn set_mock_open_file(result: Option<PathBuf>) -> DialogMockGuard {
    let guard = MOCK_TEST_LOCK.lock().unwrap();
    let mut lock = MOCK_OPEN_SELECTION.lock().unwrap();
    *lock = Some(result);
    MOCK_ACTIVE.store(true, Ordering::SeqCst);
    DialogMockGuard {
        _lock: Some(guard),
    }
}

/// Sets the mock response for subsequent `pick_save_file` invocations.
///
/// Returns a `DialogMockGuard` that will restore normal behavior when dropped.
pub fn set_mock_save_file(result: Option<PathBuf>) -> DialogMockGuard {
    let guard = MOCK_TEST_LOCK.lock().unwrap();
    let mut lock = MOCK_SAVE_SELECTION.lock().unwrap();
    *lock = Some(result);
    MOCK_ACTIVE.store(true, Ordering::SeqCst);
    DialogMockGuard {
        _lock: Some(guard),
    }
}

/// Disables all programmatic dialog mocks and resets state.
pub fn disable_mock() {
    if let Ok(mut lock) = MOCK_OPEN_SELECTION.lock() {
        *lock = None;
    }
    if let Ok(mut lock) = MOCK_SAVE_SELECTION.lock() {
        *lock = None;
    }
    MOCK_ACTIVE.store(false, Ordering::SeqCst);
}

/// Returns `true` if dialog mocking is currently active.
pub fn is_mock_enabled() -> bool {
    MOCK_ACTIVE.load(Ordering::SeqCst)
}

/// Inspects environment variable override for dialog mocking in test runners.
fn check_env_mock(var_name: &str) -> Option<Option<PathBuf>> {
    match std::env::var(var_name) {
        Ok(val) => {
            let trimmed = val.trim();
            if trimmed.is_empty()
                || trimmed.eq_ignore_ascii_case("cancel")
                || trimmed.eq_ignore_ascii_case("none")
            {
                Some(None)
            } else {
                Some(Some(PathBuf::from(trimmed)))
            }
        }
        Err(_) => None,
    }
}

// =========================================================================
// === UTILITY / NORMALIZATION HELPERS
// =========================================================================

/// Resolves the default file name for the Save dialog, ensuring proper `.md` extension.
pub fn resolve_save_filename(default_name: Option<&str>) -> String {
    match default_name {
        Some(name) => {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                DEFAULT_SAVE_NAME.to_string()
            } else if !trimmed.ends_with(".md") {
                format!("{trimmed}.md")
            } else {
                trimmed.to_string()
            }
        }
        None => DEFAULT_SAVE_NAME.to_string(),
    }
}

/// Checks whether a given file extension is a supported Markdown format.
pub fn is_supported_markdown_extension(ext: &str) -> bool {
    let clean = ext.trim_start_matches('.').to_lowercase();
    MARKDOWN_EXTENSIONS.iter().any(|&e| e == clean)
}

/// Appends `.md` extension if the path does not already have an extension.
pub fn ensure_markdown_extension(path: PathBuf) -> PathBuf {
    if path.extension().is_none() {
        path.with_extension("md")
    } else {
        path
    }
}

// =========================================================================
// === CORE SYNCHRONOUS DIALOG API
// =========================================================================

/// Prompts the user to pick an existing Markdown document.
///
/// Configures filters for Markdown files (`md`, `markdown`, `mdown`, `txt`),
/// sets title to `"Open Markdown Document"`, and opens at `starting_dir` if specified.
///
/// Returns `Some(PathBuf)` if a file was selected, or `None` if canceled.
pub fn pick_open_file(starting_dir: Option<&Path>) -> Option<PathBuf> {
    pick_open_file_with_parent(starting_dir, None)
}

/// Prompts the user to pick an existing Markdown document with an optional parent window.
pub fn pick_open_file_with_parent(
    starting_dir: Option<&Path>,
    parent: Option<&tao::window::Window>,
) -> Option<PathBuf> {
    // 1. Check programmatic mock
    if is_mock_enabled() {
        if let Ok(lock) = MOCK_OPEN_SELECTION.lock() {
            if let Some(ref mock_result) = *lock {
                return mock_result.clone();
            }
        }
    }

    // 2. Check environment variable override
    if let Some(env_result) = check_env_mock(ENV_MOCK_OPEN_FILE) {
        return env_result;
    }

    // 3. Build native rfd dialog
    let mut dialog = rfd::FileDialog::new()
        .set_title(OPEN_DIALOG_TITLE)
        .add_filter("Markdown Files", MARKDOWN_EXTENSIONS)
        .add_filter("All Files", &["*"]);

    if let Some(dir) = starting_dir {
        if dir.exists() {
            dialog = dialog.set_directory(dir);
        }
    }

    if let Some(window) = parent {
        dialog = dialog.set_parent(window);
    }

    dialog.pick_file()
}

/// Prompts the user to choose a destination path for saving a Markdown document.
///
/// Configures default file name (e.g. `Untitled.md`), filter for `.md`,
/// and title `"Save Markdown Document"`.
///
/// Returns `Some(PathBuf)` if chosen, or `None` if canceled.
pub fn pick_save_file(starting_dir: Option<&Path>, default_name: Option<&str>) -> Option<PathBuf> {
    pick_save_file_with_parent(starting_dir, default_name, None)
}

/// Prompts the user to choose a save path with an optional parent window.
pub fn pick_save_file_with_parent(
    starting_dir: Option<&Path>,
    default_name: Option<&str>,
    parent: Option<&tao::window::Window>,
) -> Option<PathBuf> {
    let resolved_name = resolve_save_filename(default_name);

    // 1. Check programmatic mock
    if is_mock_enabled() {
        if let Ok(lock) = MOCK_SAVE_SELECTION.lock() {
            if let Some(ref mock_result) = *lock {
                return mock_result.clone();
            }
        }
    }

    // 2. Check environment variable override
    if let Some(env_result) = check_env_mock(ENV_MOCK_SAVE_FILE) {
        return env_result;
    }

    // 3. Build native rfd dialog
    let mut dialog = rfd::FileDialog::new()
        .set_title(SAVE_DIALOG_TITLE)
        .set_file_name(&resolved_name)
        .add_filter("Markdown Files", SAVE_EXTENSIONS)
        .add_filter("All Files", &["*"]);

    if let Some(dir) = starting_dir {
        if dir.exists() {
            dialog = dialog.set_directory(dir);
        }
    }

    if let Some(window) = parent {
        dialog = dialog.set_parent(window);
    }

    dialog.save_file().map(ensure_markdown_extension)
}

// =========================================================================
// === ASYNCHRONOUS / NON-BLOCKING EVENT LOOP INTEGRATION
// =========================================================================

/// Spawns the Open File dialog on a background worker thread.
///
/// Prevents freezing the Tao/WebKitGTK event loop on Wayland/Linux while
/// the native file chooser dialog is open.
pub fn spawn_pick_open_file<F>(starting_dir: Option<PathBuf>, callback: F)
where
    F: FnOnce(Option<PathBuf>) + Send + 'static,
{
    let builder = std::thread::Builder::new().name("typora-open-dialog".to_string());
    let _ = builder.spawn(move || {
        let result = pick_open_file(starting_dir.as_deref());
        callback(result);
    });
}

/// Spawns the Save File dialog on a background worker thread.
pub fn spawn_pick_save_file<F>(
    starting_dir: Option<PathBuf>,
    default_name: Option<String>,
    callback: F,
) where
    F: FnOnce(Option<PathBuf>) + Send + 'static,
{
    let builder = std::thread::Builder::new().name("typora-save-dialog".to_string());
    let _ = builder.spawn(move || {
        let result = pick_save_file(starting_dir.as_deref(), default_name.as_deref());
        callback(result);
    });
}

/// Spawns the Open File dialog and dispatches `UserEvent::OpenFile` back to the Tao event loop.
pub fn spawn_pick_open_file_to_proxy(
    starting_dir: Option<PathBuf>,
    proxy: tao::event_loop::EventLoopProxy<window::UserEvent>,
) {
    spawn_pick_open_file(starting_dir, move |chosen| {
        if let Some(path) = chosen {
            if let Err(e) = proxy.send_event(window::UserEvent::OpenFile(path)) {
                eprintln!("[Dialogs] Failed to send OpenFile event to Tao event loop: {e}");
            }
        }
    });
}

// =========================================================================
// === UNIT TESTS
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants_and_filters() {
        assert_eq!(OPEN_DIALOG_TITLE, "Open Markdown Document");
        assert_eq!(SAVE_DIALOG_TITLE, "Save Markdown Document");
        assert_eq!(DEFAULT_SAVE_NAME, "Untitled.md");

        assert!(MARKDOWN_EXTENSIONS.contains(&"md"));
        assert!(MARKDOWN_EXTENSIONS.contains(&"markdown"));
        assert!(MARKDOWN_EXTENSIONS.contains(&"mdown"));
        assert!(MARKDOWN_EXTENSIONS.contains(&"txt"));
        assert_eq!(SAVE_EXTENSIONS, &["md"]);
    }

    #[test]
    fn test_is_supported_markdown_extension() {
        assert!(is_supported_markdown_extension("md"));
        assert!(is_supported_markdown_extension(".md"));
        assert!(is_supported_markdown_extension("markdown"));
        assert!(is_supported_markdown_extension(".markdown"));
        assert!(is_supported_markdown_extension("mdown"));
        assert!(is_supported_markdown_extension("txt"));
        assert!(!is_supported_markdown_extension("pdf"));
        assert!(!is_supported_markdown_extension("exe"));
    }

    #[test]
    fn test_resolve_save_filename() {
        assert_eq!(resolve_save_filename(None), "Untitled.md");
        assert_eq!(resolve_save_filename(Some("")), "Untitled.md");
        assert_eq!(resolve_save_filename(Some("   ")), "Untitled.md");
        assert_eq!(resolve_save_filename(Some("notes")), "notes.md");
        assert_eq!(resolve_save_filename(Some("notes.md")), "notes.md");
        assert_eq!(resolve_save_filename(Some("my_document")), "my_document.md");
    }

    #[test]
    fn test_ensure_markdown_extension() {
        let p1 = PathBuf::from("/home/user/draft");
        assert_eq!(ensure_markdown_extension(p1), PathBuf::from("/home/user/draft.md"));

        let p2 = PathBuf::from("/home/user/draft.md");
        assert_eq!(ensure_markdown_extension(p2), PathBuf::from("/home/user/draft.md"));

        let p3 = PathBuf::from("/home/user/draft.markdown");
        assert_eq!(ensure_markdown_extension(p3), PathBuf::from("/home/user/draft.markdown"));
    }

    #[test]
    fn test_mock_open_file_selection() {
        let expected = PathBuf::from("/tmp/test_doc.md");
        let _guard = set_mock_open_file(Some(expected.clone()));

        assert!(is_mock_enabled());
        let result = pick_open_file(None);
        assert_eq!(result, Some(expected));
    }

    #[test]
    fn test_mock_open_file_cancel() {
        let _guard = set_mock_open_file(None);
        assert!(is_mock_enabled());
        let result = pick_open_file(None);
        assert_eq!(result, None);
    }

    #[test]
    fn test_mock_save_file_selection() {
        let expected = PathBuf::from("/tmp/saved_notes.md");
        let _guard = set_mock_save_file(Some(expected.clone()));

        assert!(is_mock_enabled());
        let result = pick_save_file(None, Some("saved_notes.md"));
        assert_eq!(result, Some(expected));
    }

    #[test]
    fn test_mock_save_file_cancel() {
        let _guard = set_mock_save_file(None);
        assert!(is_mock_enabled());
        let result = pick_save_file(None, None);
        assert_eq!(result, None);
    }

    #[test]
    fn test_mock_guard_drop_restores_state() {
        {
            let _guard = set_mock_open_file(Some(PathBuf::from("/tmp/a.md")));
            assert!(is_mock_enabled());
        }
        assert!(!is_mock_enabled());
    }

    #[test]
    fn test_spawn_pick_open_file_mocked() {
        let expected = PathBuf::from("/tmp/threaded_open.md");
        let _guard = set_mock_open_file(Some(expected.clone()));

        let (tx, rx) = std::sync::mpsc::channel();
        spawn_pick_open_file(None, move |chosen| {
            tx.send(chosen).unwrap();
        });

        let received = rx.recv_timeout(std::time::Duration::from_millis(500)).unwrap();
        assert_eq!(received, Some(expected));
    }

    #[test]
    fn test_spawn_pick_save_file_mocked() {
        let expected = PathBuf::from("/tmp/threaded_save.md");
        let _guard = set_mock_save_file(Some(expected.clone()));

        let (tx, rx) = std::sync::mpsc::channel();
        spawn_pick_save_file(None, Some("threaded_save".to_string()), move |chosen| {
            tx.send(chosen).unwrap();
        });

        let received = rx.recv_timeout(std::time::Duration::from_millis(500)).unwrap();
        assert_eq!(received, Some(expected));
    }

    #[test]
    fn test_pick_with_parent_none_mocked() {
        let expected = PathBuf::from("/tmp/parent_test.md");
        let _guard = set_mock_open_file(Some(expected.clone()));

        let result = pick_open_file_with_parent(None, None);
        assert_eq!(result, Some(expected));
    }

    #[test]
    fn test_env_var_mock_open_file() {
        let _lock = MOCK_TEST_LOCK.lock().unwrap();
        disable_mock();

        std::env::set_var(ENV_MOCK_OPEN_FILE, "/tmp/env_open.md");
        let result = pick_open_file(None);
        assert_eq!(result, Some(PathBuf::from("/tmp/env_open.md")));

        std::env::set_var(ENV_MOCK_OPEN_FILE, "CANCEL");
        let cancel_result = pick_open_file(None);
        assert_eq!(cancel_result, None);

        std::env::remove_var(ENV_MOCK_OPEN_FILE);
    }

    #[test]
    fn test_env_var_mock_save_file() {
        let _lock = MOCK_TEST_LOCK.lock().unwrap();
        disable_mock();

        std::env::set_var(ENV_MOCK_SAVE_FILE, "/tmp/env_save.md");
        let result = pick_save_file(None, None);
        assert_eq!(result, Some(PathBuf::from("/tmp/env_save.md")));

        std::env::set_var(ENV_MOCK_SAVE_FILE, "CANCEL");
        let cancel_result = pick_save_file(None, None);
        assert_eq!(cancel_result, None);

        std::env::remove_var(ENV_MOCK_SAVE_FILE);
    }
}
