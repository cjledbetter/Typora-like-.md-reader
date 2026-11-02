//! Empirical Adversarial Stress Test Suite for Tier 5 Core Engine
//!
//! Scope:
//! 1. `src/cli.rs` (argument parsing, path normalization, resolution, discovery)
//! 2. `src/io.rs` (atomic file write, error conditions, lossy reading, temp file cleanup, permissions)
//! 3. `src/watcher.rs` (inotify parent watching, debouncing window, self-save suppression, file retargeting)
//! 4. `src/dialogs.rs` (file picking, Wayland threading, mock harness)
//! 5. `src/assets.rs` (custom protocol `typora://`, percent-decoding, directory traversal defense, mime types)

mod common;
use common::TestSandbox;
use std::fs::{self, Permissions};
use std::io;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use typora_rust_reader::assets::*;
use typora_rust_reader::cli::*;
use typora_rust_reader::dialogs::*;
use typora_rust_reader::io::*;
use typora_rust_reader::watcher::*;

// =========================================================================
// === 1. CLI ENGINE ADVERSARIAL STRESS
// =========================================================================

#[test]
fn test_tier5_cli_normalize_path_extreme_edge_cases() {
    // 1. Root and root traversal
    assert_eq!(normalize_path(Path::new("/")), PathBuf::from("/"));
    assert_eq!(normalize_path(Path::new("/..")), PathBuf::from("/"));
    assert_eq!(normalize_path(Path::new("/../../..")), PathBuf::from("/"));
    assert_eq!(normalize_path(Path::new("/a/../..")), PathBuf::from("/"));
    assert_eq!(normalize_path(Path::new("/a/b/../../c")), PathBuf::from("/c"));

    // 2. Relative paths and excessive ParentDir
    assert_eq!(normalize_path(Path::new(".")), PathBuf::from("."));
    assert_eq!(normalize_path(Path::new("./.")), PathBuf::from("."));
    assert_eq!(normalize_path(Path::new("..")), PathBuf::from(".."));
    assert_eq!(normalize_path(Path::new("../..")), PathBuf::from("../../..".split('/').take(2).collect::<PathBuf>()));
    assert_eq!(normalize_path(Path::new("a/..")), PathBuf::from("."));
    assert_eq!(normalize_path(Path::new("a/b/../..")), PathBuf::from("."));
    assert_eq!(normalize_path(Path::new("a/b/../../..")), PathBuf::from(".."));

    // 3. Redundant slashes and current dir dots
    assert_eq!(normalize_path(Path::new("a/./b/./c")), PathBuf::from("a/b/c"));
    assert_eq!(normalize_path(Path::new("///a//b///c")), PathBuf::from("/a/b/c"));

    // 4. Empty path
    assert_eq!(normalize_path(Path::new("")), PathBuf::new());
}

#[test]
fn test_tier5_cli_tilde_expansion_without_home_env() {
    // Save original HOME
    let original_home = std::env::var_os("HOME");
    std::env::remove_var("HOME");

    // Without HOME, ~ or ~/ should fall back safely to literal path without panic
    let res_tilde = expand_tilde("~");
    assert_eq!(res_tilde, PathBuf::from("~"));

    let res_sub = expand_tilde("~/documents/notes.md");
    assert_eq!(res_sub, PathBuf::from("~/documents/notes.md"));

    // Restore HOME
    if let Some(h) = original_home {
        std::env::set_var("HOME", h);
    }
}

#[test]
fn test_tier5_cli_dangling_symlink_returns_not_found_cleanly() {
    let sandbox = TestSandbox::new();
    let dangling_target = sandbox.path().join("does_not_exist_target.md");
    let link_path = sandbox.path().join("dangling_link.md");

    #[cfg(unix)]
    std::os::unix::fs::symlink(&dangling_target, &link_path).unwrap();

    let res = resolve_initial_file(Some("dangling_link.md"), sandbox.path());
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("File not found"));
}

#[test]
fn test_tier5_cli_circular_symlink_handled_gracefully() {
    let sandbox = TestSandbox::new();
    let link_a = sandbox.path().join("link_a.md");
    let link_b = sandbox.path().join("link_b.md");

    #[cfg(unix)]
    {
        let _ = std::os::unix::fs::symlink(&link_b, &link_a);
        let _ = std::os::unix::fs::symlink(&link_a, &link_b);

        // Circular symlinks fail exists() check on Linux (ELOOP: Too many levels of symbolic links)
        let res = resolve_initial_file(Some("link_a.md"), sandbox.path());
        assert!(res.is_err());
    }
}

#[test]
fn test_tier5_cli_resolve_deep_directory_100_levels() {
    let sandbox = TestSandbox::new();
    let mut deep_rel = PathBuf::new();
    for i in 0..100 {
        deep_rel.push(format!("sub_{i}"));
    }
    let target_dir = sandbox.path().join(&deep_rel);
    fs::create_dir_all(&target_dir).unwrap();

    let file = target_dir.join("deep.md");
    fs::write(&file, "# Deep 100").unwrap();

    let arg = deep_rel.join("deep.md");
    let res = resolve_initial_file(Some(arg.to_str().unwrap()), sandbox.path()).unwrap();
    assert_eq!(res, Some(file.canonicalize().unwrap()));
}

#[test]
fn test_tier5_cli_resolve_unicode_and_emojis() {
    let sandbox = TestSandbox::new();
    let unicode_names = [
        "🦀_rust_notes.md",
        "مرحبا_بالعالم.md",
        "中文文档_测试.md",
        "dîácrîtîcs_test.md",
        "spaced  name  with  tabs.md",
        "semi;colon&ampersand.md",
    ];

    for name in &unicode_names {
        let path = sandbox.create_file(name, &format!("# Content for {name}"));
        let res = resolve_initial_file(Some(name), sandbox.path()).unwrap();
        assert_eq!(res, Some(path.canonicalize().unwrap()));
    }
}

#[test]
fn test_tier5_cli_normalized_mode_and_theme_matrix() {
    let test_cases = [
        // Mode variations
        (Some("read"), "read", "github"),
        (Some("READ"), "read", "github"),
        (Some("  read  "), "read", "github"),
        (Some("preview"), "read", "github"),
        (Some("PREVIEW"), "read", "github"),
        (Some("edit"), "edit", "github"),
        (Some("EDIT"), "edit", "github"),
        (Some("editor"), "edit", "github"),
        (Some("split"), "split", "github"),
        (Some("SPLIT"), "split", "github"),
        (Some("unknown_mode"), "split", "github"),
        (None, "split", "github"),
    ];

    for (mode_in, expected_mode, _) in test_cases {
        let args = CliArgs {
            file: None,
            mode: mode_in.map(|s| s.to_string()),
            theme: None,
        };
        assert_eq!(args.normalized_mode(), expected_mode);
    }

    let theme_cases = [
        (Some("github"), "github"),
        (Some("GITHUB"), "github"),
        (Some("  github  "), "github"),
        (Some("night"), "night"),
        (Some("NIGHT"), "night"),
        (Some("newsprint"), "newsprint"),
        (Some("gothic"), "gothic"),
        (Some("solarized"), "github"), // fallback to github
        (None, "github"),
    ];

    for (theme_in, expected_theme) in theme_cases {
        let args = CliArgs {
            file: None,
            mode: None,
            theme: theme_in.map(|s| s.to_string()),
        };
        assert_eq!(args.normalized_theme(), expected_theme);
    }
}

// =========================================================================
// === 2. DIRECT FILE I/O & ATOMIC WRITE ADVERSARIAL STRESS
// =========================================================================

#[test]
fn test_tier5_io_atomic_write_deep_directory_creation() {
    let sandbox = TestSandbox::new();
    let mut deep_rel = PathBuf::new();
    for i in 0..40 {
        deep_rel.push(format!("nest_{i}"));
    }
    let target = sandbox.path().join(&deep_rel).join("deep_created.md");
    assert!(!target.parent().unwrap().exists());

    let mtime = atomic_write_file(&target, "Deep parent created successfully").unwrap();
    assert!(mtime > 0);
    assert!(target.exists());

    let read_back = read_file_lossy(&target).unwrap();
    assert_eq!(read_back.content, "Deep parent created successfully");
}

#[test]
fn test_tier5_io_atomic_write_preserves_custom_unix_permissions() {
    #[cfg(unix)]
    {
        let sandbox = TestSandbox::new();
        let target = sandbox.path().join("secure_file.md");

        // Write initial file and set 0600 (read/write by owner only)
        atomic_write_file(&target, "Initial secret content").unwrap();
        let perms = Permissions::from_mode(0o600);
        fs::set_permissions(&target, perms).unwrap();

        let initial_meta = fs::metadata(&target).unwrap();
        assert_eq!(initial_meta.permissions().mode() & 0o777, 0o600);

        // Atomic overwrite must preserve 0600 mode!
        atomic_write_file(&target, "Updated secret content").unwrap();
        let updated_meta = fs::metadata(&target).unwrap();
        assert_eq!(
            updated_meta.permissions().mode() & 0o777,
            0o600,
            "Permissions must be preserved across atomic writes"
        );

        let doc = read_file_lossy(&target).unwrap();
        assert_eq!(doc.content, "Updated secret content");
    }
}

#[test]
fn test_tier5_io_atomic_write_readonly_dir_fails_cleanly_no_temp_files() {
    #[cfg(unix)]
    {
        let sandbox = TestSandbox::new();
        let ro_dir = sandbox.path().join("readonly_dir");
        fs::create_dir_all(&ro_dir).unwrap();

        // Make ro_dir read-only
        fs::set_permissions(&ro_dir, Permissions::from_mode(0o555)).unwrap();

        let target = ro_dir.join("should_fail.md");
        let res = atomic_write_file(&target, "Cannot write here");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().kind(), io::ErrorKind::PermissionDenied);

        // Restore permissions so cleanup works
        fs::set_permissions(&ro_dir, Permissions::from_mode(0o755)).unwrap();

        // Verify zero temporary files left behind
        let entries: Vec<_> = fs::read_dir(&ro_dir).unwrap().collect();
        assert_eq!(entries.len(), 0, "No temp files should remain in directory");
    }
}

#[test]
fn test_tier5_io_concurrent_multithreaded_atomic_writes() {
    let sandbox = TestSandbox::new();
    let num_threads = 8;
    let iterations_per_thread = 20;

    let handles: Vec<_> = (0..num_threads)
        .map(|tid| {
            let root = sandbox.path().to_path_buf();
            thread::spawn(move || {
                for iter in 0..iterations_per_thread {
                    let file_path = root.join(format!("thread_file_{tid}.md"));
                    let payload = format!("Thread {tid} Iteration {iter} Data: {}", "x".repeat(iter * 50));
                    atomic_write_file(&file_path, &payload).expect("Concurrent atomic write failed");

                    let doc = read_file_lossy(&file_path).expect("Read during concurrent test failed");
                    assert_eq!(doc.content, payload);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    // Inspect directory: must have exactly `num_threads` files, zero `.tmp.*` files
    let entries: Vec<String> = fs::read_dir(sandbox.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();

    assert_eq!(entries.len(), num_threads);
    for entry in &entries {
        assert!(!entry.contains(".tmp."));
    }
}

#[test]
fn test_tier5_io_lossy_read_utf8_bom_and_null_bytes() {
    let sandbox = TestSandbox::new();

    // 1. UTF-8 with BOM (\xEF\xBB\xBF)
    let bom_file = sandbox.path().join("bom.md");
    let mut bom_bytes = vec![0xEF, 0xBB, 0xBF];
    bom_bytes.extend_from_slice(b"# Header with BOM");
    fs::write(&bom_file, &bom_bytes).unwrap();

    let doc_bom = read_file_lossy(&bom_file).unwrap();
    assert!(doc_bom.content.contains("# Header with BOM"));

    // 2. File with embedded NULL bytes (\0)
    let null_file = sandbox.path().join("null.md");
    let null_bytes = b"Line 1\x00Line 2\x00\x00Line 3";
    fs::write(&null_file, null_bytes).unwrap();

    let doc_null = read_file_lossy(&null_file).unwrap();
    assert_eq!(doc_null.size, null_bytes.len());
    assert!(doc_null.content.contains("Line 1\0Line 2"));
}

#[test]
fn test_tier5_io_large_document_roundtrip_5mb() {
    let sandbox = TestSandbox::new();
    let large_file = sandbox.path().join("large_5mb.md");

    // Construct 5MB document
    let chunk = "# Chapter Section\nParagraph of text with math $x^2 + y^2 = z^2$ and code `print(1)`.\n\n";
    let count = 5 * 1024 * 1024 / chunk.len();
    let large_content = chunk.repeat(count);

    let mtime = atomic_write_file(&large_file, &large_content).unwrap();
    assert!(mtime > 0);

    let doc = read_file_lossy(&large_file).unwrap();
    assert_eq!(doc.content.len(), large_content.len());
    assert_eq!(doc.content, large_content);
    assert_eq!(doc.size, large_content.len());
}

// =========================================================================
// === 3. KERNEL WATCHER ADVERSARIAL STRESS
// =========================================================================

#[test]
fn test_tier5_watcher_rapid_dynamic_retargeting_stability() {
    let sandbox = TestSandbox::new();
    let file1 = sandbox.create_file("target1.md", "content 1");
    let file2 = sandbox.create_file("target2.md", "content 2");
    let file3 = sandbox.create_file("target3.md", "content 3");

    let (tx, rx) = channel();
    let mut watcher = DocumentWatcher::new(move |ev| {
        let _ = tx.send(ev);
    }).expect("Failed to create DocumentWatcher");

    // Rapidly switch target 10 times
    for _ in 0..5 {
        watcher.watch_file(&file1).unwrap();
        watcher.watch_file(&file2).unwrap();
        watcher.watch_file(&file3).unwrap();
    }

    // Finally settle on file2
    watcher.watch_file(&file2).unwrap();
    thread::sleep(Duration::from_millis(100));

    // Modify file1 (should be ignored!)
    fs::write(&file1, "Modified 1").unwrap();

    // Modify file2 (must be detected!)
    fs::write(&file2, "Modified 2").unwrap();

    let ev = rx.recv_timeout(Duration::from_millis(800)).expect("Did not receive event for file2");
    match ev {
        WatcherEvent::Modified(p) => {
            assert_eq!(p.file_name(), file2.file_name());
        }
        other => panic!("Expected Modified event, got {:?}", other),
    }

    // No lingering events for file1
    thread::sleep(Duration::from_millis(200));
    while let Ok(unexpected) = rx.try_recv() {
        if let WatcherEvent::Modified(ref p) = unexpected {
            assert_ne!(p.file_name(), file1.file_name(), "Should not receive events for inactive file1");
        }
    }
}

#[test]
fn test_tier5_watcher_burst_50_writes_debounces_safely() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("burst_50.md", "init");

    let event_count = Arc::new(AtomicU64::new(0));
    let count_clone = Arc::clone(&event_count);

    let (tx, _rx) = channel();
    let mut watcher = DocumentWatcher::new(move |ev| {
        count_clone.fetch_add(1, Ordering::SeqCst);
        let _ = tx.send(ev);
    }).expect("Failed to create DocumentWatcher");

    watcher.watch_file(&file).unwrap();
    thread::sleep(Duration::from_millis(100));

    // Rapid 50 writes in ~50ms
    for i in 1..=50 {
        fs::write(&file, format!("Burst data {i}")).unwrap();
        thread::sleep(Duration::from_millis(1));
    }

    // Allow debounce to settle
    thread::sleep(Duration::from_millis(300));

    let total = event_count.load(Ordering::SeqCst);
    assert!(
        total >= 1 && total <= 3,
        "Debouncer must collapse 50 rapid writes into <= 3 callbacks, got: {total}"
    );

    // Final state of file is readable
    let doc = read_file_lossy(&file).unwrap();
    assert_eq!(doc.content, "Burst data 50");
}

#[test]
fn test_tier5_watcher_clean_drop_without_deadlock() {
    let sandbox = TestSandbox::new();
    let file = sandbox.create_file("drop_test.md", "content");

    let mut watcher = DocumentWatcher::new(|_| {}).expect("Create watcher");
    watcher.watch_file(&file).unwrap();

    // Trigger rapid events right as drop occurs
    let file_clone = file.clone();
    let t = thread::spawn(move || {
        for i in 0..10 {
            let _ = fs::write(&file_clone, format!("{i}"));
            thread::sleep(Duration::from_millis(5));
        }
    });

    thread::sleep(Duration::from_millis(10));
    drop(watcher); // Must join cleanly and not deadlock
    t.join().unwrap();
}

// =========================================================================
// === 4. NATIVE DIALOGS & MOCK HARNESS ADVERSARIAL STRESS
// =========================================================================

#[test]
fn test_tier5_dialogs_resolve_save_filename_edge_cases() {
    assert_eq!(resolve_save_filename(None), "Untitled.md");
    assert_eq!(resolve_save_filename(Some("")), "Untitled.md");
    assert_eq!(resolve_save_filename(Some("   \t\n  ")), "Untitled.md");
    assert_eq!(resolve_save_filename(Some("notes")), "notes.md");
    assert_eq!(resolve_save_filename(Some("notes.md")), "notes.md");
    assert_eq!(resolve_save_filename(Some("archive.tar.gz")), "archive.tar.gz.md");
    assert_eq!(resolve_save_filename(Some("日本語")), "日本語.md");
    assert_eq!(resolve_save_filename(Some("emoji_🦀")), "emoji_🦀.md");
}

#[test]
fn test_tier5_dialogs_ensure_markdown_extension_edge_cases() {
    // No extension -> append .md
    assert_eq!(
        ensure_markdown_extension(PathBuf::from("/path/to/my_file")),
        PathBuf::from("/path/to/my_file.md")
    );

    // Existing .md -> keep as is
    assert_eq!(
        ensure_markdown_extension(PathBuf::from("/path/to/my_file.md")),
        PathBuf::from("/path/to/my_file.md")
    );

    // Existing .markdown -> keep as is
    assert_eq!(
        ensure_markdown_extension(PathBuf::from("/path/to/my_file.markdown")),
        PathBuf::from("/path/to/my_file.markdown")
    );

    // Dotted path in folder name, no extension on file: /folder.name/file -> /folder.name/file.md
    assert_eq!(
        ensure_markdown_extension(PathBuf::from("/dotted.folder/document")),
        PathBuf::from("/dotted.folder/document.md")
    );
}

#[test]
fn test_tier5_dialogs_mock_harness_thread_safety() {
    let mock_path = PathBuf::from("/tmp/concurrent_test.md");
    let _guard = set_mock_open_file(Some(mock_path.clone()));

    // Spawning multiple threads requesting dialog should all receive mock_path
    let handles: Vec<_> = (0..5)
        .map(|_| {
            let expected = mock_path.clone();
            thread::spawn(move || {
                let chosen = pick_open_file(None);
                assert_eq!(chosen, Some(expected));
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
}

// =========================================================================
// === 5. ASSET ENGINE & ZERO-PORT PROTOCOL ADVERSARIAL STRESS
// =========================================================================

#[test]
fn test_tier5_assets_head_request_returns_empty_body_with_headers() {
    let req = http::Request::builder()
        .method(http::Method::HEAD)
        .uri("typora://localhost/index.html")
        .body(Vec::new())
        .unwrap();

    let res = handle_asset_request(&req);
    assert_eq!(res.status(), http::StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap().to_str().unwrap(),
        "text/html; charset=utf-8"
    );
    assert!(res.body().is_empty(), "HEAD response body must be empty");
}

#[test]
fn test_tier5_assets_options_cors_preflight() {
    let req = http::Request::builder()
        .method(http::Method::OPTIONS)
        .uri("typora://localhost/assets/katex/katex.min.css")
        .body(Vec::new())
        .unwrap();

    let res = handle_asset_request(&req);
    assert_eq!(res.status(), http::StatusCode::NO_CONTENT);
    assert_eq!(
        res.headers().get("access-control-allow-origin").unwrap(),
        "*"
    );
    assert_eq!(
        res.headers().get("access-control-allow-methods").unwrap(),
        "GET, HEAD, OPTIONS"
    );
}

#[test]
fn test_tier5_assets_directory_traversal_adversarial_matrix() {
    let attack_payloads = [
        "typora://localhost/../Cargo.toml",
        "typora://localhost/../../../../etc/passwd",
        "typora://localhost/assets/../../Cargo.lock",
        "typora://localhost/%2e%2e/Cargo.toml",
        "typora://localhost/%2e%2e%2fCargo.toml",
        "typora://localhost/%2E%2E%2FCargo.toml",
        "typora://localhost/%2fetc%2fshadow",
        "typora://localhost/%5c..%5cCargo.toml",
        "typora://localhost/assets/%2e%2e%2f%2e%2e%2fsrc/main.rs",
        "typora://localhost/..%2f..%2f..%2f..%2fetc%2fhosts",
        "typora://localhost/.%2e/.%2e/etc/passwd",
    ];

    for attack in &attack_payloads {
        let req = http::Request::builder()
            .uri(*attack)
            .body(Vec::new())
            .unwrap();

        let res = handle_asset_request(&req);
        assert_eq!(
            res.status(),
            http::StatusCode::FORBIDDEN,
            "Traversal attack MUST receive 403 Forbidden: {attack}"
        );
        assert_eq!(
            res.body().as_ref(),
            b"403 Forbidden",
            "Must return 403 Forbidden payload"
        );
    }
}

#[test]
fn test_tier5_assets_url_host_routing_and_default_index() {
    // 1. typora://localhost -> index.html
    let req1 = http::Request::builder()
        .uri("typora://localhost")
        .body(Vec::new())
        .unwrap();
    let res1 = handle_asset_request(&req1);
    assert_eq!(res1.status(), http::StatusCode::OK);

    // 2. typora://localhost/ -> index.html
    let req2 = http::Request::builder()
        .uri("typora://localhost/")
        .body(Vec::new())
        .unwrap();
    let res2 = handle_asset_request(&req2);
    assert_eq!(res2.status(), http::StatusCode::OK);

    // 3. typora://index.html (host routing)
    let req3 = http::Request::builder()
        .uri("typora://index.html")
        .body(Vec::new())
        .unwrap();
    let res3 = handle_asset_request(&req3);
    assert_eq!(res3.status(), http::StatusCode::OK);
}

#[test]
fn test_tier5_assets_mime_types_for_all_asset_categories() {
    let mime_tests = [
        ("index.html", "text/html; charset=utf-8"),
        ("manifest.json", "application/json; charset=utf-8"),
        ("assets/icon.svg", "image/svg+xml"),
        ("assets/katex.min.css", "text/css; charset=utf-8"),
        ("assets/katex.min.js", "application/javascript; charset=utf-8"),
        ("assets/highlight.min.js", "application/javascript; charset=utf-8"),
        ("assets/mermaid.min.js", "application/javascript; charset=utf-8"),
        ("assets/fonts/KaTeX_Main-Regular.woff2", "font/woff2"),
    ];

    for (path, expected_mime) in mime_tests {
        let req = http::Request::builder()
            .uri(format!("typora://localhost/{path}"))
            .body(Vec::new())
            .unwrap();
        let res = handle_asset_request(&req);
        assert_eq!(res.status(), http::StatusCode::OK, "Asset should exist: {path}");
        let ctype = res.headers().get("content-type").unwrap().to_str().unwrap();
        assert_eq!(ctype, expected_mime, "MIME mismatch for {path}");
    }
}

#[test]
fn test_tier5_assets_missing_asset_returns_404_not_found() {
    let req = http::Request::builder()
        .uri("typora://localhost/non_existent_file_xyz.js")
        .body(Vec::new())
        .unwrap();

    let res = handle_asset_request(&req);
    assert_eq!(res.status(), http::StatusCode::NOT_FOUND);
    assert_eq!(res.body().as_ref(), b"404 Not Found");
}
