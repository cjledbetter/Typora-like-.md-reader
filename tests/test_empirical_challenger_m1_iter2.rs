//! Adversarial Empirical Test Suite for Milestone 1 Iteration 2 Gate Verification
//!
//! Specifically validates:
//! 1. All path traversal variants (%2e%2e, %2E%2E, ..%2f, %2e%2e%2f, %5c, //, etc.)
//!    strictly return 403 Forbidden and NEVER 404.
//! 2. Valid assets (index.html, KaTeX fonts, Highlight.js, Mermaid, CSS themes)
//!    return 200 OK with correct MIME types and non-empty content.
//! 3. GApplication ID invariants (APP_ID format, reverse-DNS rules).

use http::{Method, Request, StatusCode};
use typora_rust_reader::assets::handle_asset_request;
use typora_rust_reader::window::APP_ID;

fn make_req(uri: &str) -> Request<Vec<u8>> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(Vec::new())
        .expect("Valid HTTP request construction")
}

#[test]
fn test_all_traversal_variants_strictly_return_forbidden() {
    let traversal_vectors = [
        // Classic dot-dot
        "typora://localhost/..",
        "typora://localhost/../",
        "typora://localhost/../../etc/passwd",
        "typora://localhost/../../../etc/shadow",
        "typora://localhost/assets/../index.html",
        "typora://localhost/assets/../../Cargo.toml",
        "typora://localhost/assets/fonts/../../../Cargo.toml",

        // Percent-encoded lowercase %2e%2e
        "typora://localhost/%2e%2e",
        "typora://localhost/%2e%2e/",
        "typora://localhost/%2e%2e/%2e%2e",
        "typora://localhost/%2e%2e/%2e%2e/etc/passwd",
        "typora://localhost/%2e%2e/Cargo.toml",
        "typora://localhost/assets/%2e%2e/index.html",
        "typora://localhost/assets/%2e%2e/%2e%2e/Cargo.toml",

        // Percent-encoded uppercase %2E%2E
        "typora://localhost/%2E%2E",
        "typora://localhost/%2E%2E/",
        "typora://localhost/%2E%2E/%2E%2E",
        "typora://localhost/%2E%2E/%2E%2E/etc/passwd",
        "typora://localhost/%2E%2E/Cargo.toml",
        "typora://localhost/assets/%2E%2E/index.html",

        // Mixed case percent-encoding
        "typora://localhost/%2e%2E",
        "typora://localhost/%2E%2e",
        "typora://localhost/%2e%2E/etc/passwd",
        "typora://localhost/%2E%2e/etc/passwd",

        // Traversal with percent-encoded slashes (..%2f and %2e%2e%2f)
        "typora://localhost/..%2f",
        "typora://localhost/..%2F",
        "typora://localhost/..%2fetc%2fpasswd",
        "typora://localhost/%2e%2e%2f",
        "typora://localhost/%2e%2e%2F",
        "typora://localhost/%2E%2E%2f",
        "typora://localhost/%2E%2E%2F",
        "typora://localhost/%2e%2e%2fetc%2fpasswd",
        "typora://localhost/%2e%2e%2f%2e%2e%2fetc%2fpasswd",
        "typora://localhost/assets/..%2f..%2fCargo.toml",
        "typora://localhost/assets/%2e%2e%2f%2e%2e%2fCargo.toml",

        // Windows backslash traversal (%5c, %5C, ..\\, \\..)
        "typora://localhost/%5c",
        "typora://localhost/%5C",
        "typora://localhost/%5cetc%2fpasswd",
        "typora://localhost/%5Cwindows\\win.ini",
        "typora://localhost/..%5c",
        "typora://localhost/..%5C",
        "typora://localhost/%2e%2e%5c",
        "typora://localhost/%2E%2E%5C",
        "typora://localhost/..\\..\\windows\\win.ini",
        "typora://localhost/\\windows\\win.ini",
        "typora://localhost/assets\\..\\..\\win.ini",

        // Slashes and double slashes with traversal
        "typora://localhost//..",
        "typora://localhost///..",
        "typora://localhost//../",
        "typora://localhost//../../etc/passwd",
        "typora://localhost/assets//..//index.html",
        "typora://localhost/%2f..",
        "typora://localhost/%2f%2e%2e",
        "typora://localhost/%2F%2E%2E",

        // Partial percent encoding
        "typora://localhost/.%2e",
        "typora://localhost/%2e.",
        "typora://localhost/.%2e/",
        "typora://localhost/%2e./",
        "typora://localhost/.%2e/etc/passwd",
        "typora://localhost/%2e./etc/passwd",
    ];

    for uri in &traversal_vectors {
        let req = make_req(uri);
        let res = handle_asset_request(&req);
        assert_eq!(
            res.status(),
            StatusCode::FORBIDDEN,
            "Attack vector must return 403 FORBIDDEN and NEVER 404 or 200: {} (got {})",
            uri,
            res.status()
        );
        let body = String::from_utf8_lossy(&res.body());
        assert!(
            body.contains("403 Forbidden"),
            "Body must indicate 403 Forbidden for: {}",
            uri
        );
    }
}

#[test]
fn test_root_and_double_slash_behavior() {
    // typora://localhost/ -> index.html (200 OK)
    let res = handle_asset_request(&make_req("typora://localhost/"));
    assert_eq!(res.status(), StatusCode::OK);

    // typora://localhost// -> index.html (200 OK)
    let res = handle_asset_request(&make_req("typora://localhost//"));
    assert_eq!(res.status(), StatusCode::OK);

    // typora://localhost/// -> index.html (200 OK)
    let res = handle_asset_request(&make_req("typora://localhost///"));
    assert_eq!(res.status(), StatusCode::OK);

    // typora://localhost//index.html -> index.html (200 OK)
    let res = handle_asset_request(&make_req("typora://localhost//index.html"));
    assert_eq!(res.status(), StatusCode::OK);
}

#[test]
fn test_missing_assets_return_404_not_403() {
    let missing_assets = [
        "typora://localhost/missing.html",
        "typora://localhost/assets/missing.js",
        "typora://localhost/assets/fonts/missing.woff2",
        "typora://localhost/assets/css/missing.css",
    ];

    for uri in &missing_assets {
        let req = make_req(uri);
        let res = handle_asset_request(&req);
        assert_eq!(
            res.status(),
            StatusCode::NOT_FOUND,
            "Legitimate missing asset must return 404 NOT FOUND: {} (got {})",
            uri,
            res.status()
        );
        let body = String::from_utf8_lossy(&res.body());
        assert!(body.contains("404 Not Found"));
    }
}

#[test]
fn test_valid_assets_integrity_and_mimes() {
    let valid_assets = [
        ("index.html", "text/html; charset=utf-8"),
        ("manifest.json", "application/json; charset=utf-8"),
        ("assets/marked.min.js", "application/javascript; charset=utf-8"),
        ("assets/katex.min.js", "application/javascript; charset=utf-8"),
        ("assets/katex.min.css", "text/css; charset=utf-8"),
        ("assets/mermaid.min.js", "application/javascript; charset=utf-8"),
        ("assets/highlight.min.js", "application/javascript; charset=utf-8"),
        ("assets/highlight-github.min.css", "text/css; charset=utf-8"),
        ("assets/highlight-github-dark.min.css", "text/css; charset=utf-8"),
        ("assets/icon.svg", "image/svg+xml"),
    ];

    for (path, expected_mime) in &valid_assets {
        let uri = format!("typora://localhost/{}", path);
        let req = make_req(&uri);
        let res = handle_asset_request(&req);

        assert_eq!(
            res.status(),
            StatusCode::OK,
            "Valid asset {} must return 200 OK",
            path
        );

        let ct = res
            .headers()
            .get("content-type")
            .expect("Content-Type header must be present")
            .to_str()
            .unwrap();

        assert_eq!(
            ct,
            *expected_mime,
            "Asset {} MIME type mismatch. Expected '{}', got '{}'",
            path,
            expected_mime,
            ct
        );

        assert!(
            !res.body().is_empty(),
            "Asset {} body must not be empty",
            path
        );
    }
}

#[test]
fn test_all_20_katex_fonts_integrity() {
    let katex_fonts = [
        "KaTeX_AMS-Regular.woff2",
        "KaTeX_Caligraphic-Bold.woff2",
        "KaTeX_Caligraphic-Regular.woff2",
        "KaTeX_Fraktur-Bold.woff2",
        "KaTeX_Fraktur-Regular.woff2",
        "KaTeX_Main-Bold.woff2",
        "KaTeX_Main-BoldItalic.woff2",
        "KaTeX_Main-Italic.woff2",
        "KaTeX_Main-Regular.woff2",
        "KaTeX_Math-BoldItalic.woff2",
        "KaTeX_Math-Italic.woff2",
        "KaTeX_SansSerif-Bold.woff2",
        "KaTeX_SansSerif-Italic.woff2",
        "KaTeX_SansSerif-Regular.woff2",
        "KaTeX_Script-Regular.woff2",
        "KaTeX_Size1-Regular.woff2",
        "KaTeX_Size2-Regular.woff2",
        "KaTeX_Size3-Regular.woff2",
        "KaTeX_Size4-Regular.woff2",
        "KaTeX_Typewriter-Regular.woff2",
    ];

    for font in &katex_fonts {
        let uri = format!("typora://localhost/assets/fonts/{}", font);
        let req = make_req(&uri);
        let res = handle_asset_request(&req);

        assert_eq!(
            res.status(),
            StatusCode::OK,
            "KaTeX font {} must return 200 OK",
            font
        );

        let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
        assert_eq!(ct, "font/woff2", "Font {} MIME must be font/woff2", font);

        let body = res.body();
        assert!(!body.is_empty(), "Font {} body must not be empty", font);
        assert!(body.starts_with(b"wOF2"), "Font {} must start with wOF2 magic", font);
    }
}

#[test]
fn test_gapplication_id_spec_compliance() {
    assert_eq!(APP_ID, "com.typora.rust-reader");

    // Specification compliance for GLib GApplication ID:
    // 1. Must be composed of elements separated by periods
    let elements: Vec<&str> = APP_ID.split('.').collect();
    assert!(elements.len() >= 2, "Must contain at least 2 elements");

    // 2. Each element must start with a letter or underscore
    for elem in &elements {
        assert!(!elem.is_empty(), "Elements cannot be empty");
        let first_char = elem.chars().next().unwrap();
        assert!(
            first_char.is_ascii_alphabetic() || first_char == '_',
            "Element '{}' must start with an ASCII letter or underscore",
            elem
        );
    }

    // 3. Characters in elements must only be alphanumeric, underscore, or hyphen
    for elem in &elements {
        for ch in elem.chars() {
            assert!(
                ch.is_ascii_alphanumeric() || ch == '_' || ch == '-',
                "Invalid character '{}' in element '{}'",
                ch,
                elem
            );
        }
    }

    // 4. Must not exceed 255 characters
    assert!(APP_ID.len() <= 255, "App ID must not exceed 255 chars");

    // 5. Must not start or end with a dot
    assert!(!APP_ID.starts_with('.'));
    assert!(!APP_ID.ends_with('.'));
}
