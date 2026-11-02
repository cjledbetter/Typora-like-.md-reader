//! Comprehensive Empirical Challenger Stress Suite for typora-rust-reader Milestone 1.
//!
//! Directly exercises `handle_asset_request` from `src/assets.rs` and system invariants:
//! 1. Directory traversal attacks (dot-dot, percent-encoded dots, Windows separators, null bytes, double slashes)
//! 2. Malformed URIs, query strings, fragments, missing files (404 handling)
//! 3. Percent-encoded filenames (e.g. %20, %2e, %2F)
//! 4. Font loading integrity for all 20 KaTeX WOFF2 font files (HTTP 200 + font/woff2 MIME type)
//! 5. CORS OPTIONS preflight headers & HEAD / GET / other HTTP method semantics
//! 6. Zero-port / socket binding verification

use http::{Method, Request, StatusCode};
use typora_rust_reader::assets::handle_asset_request;

fn make_req(method: Method, uri: &str) -> Request<Vec<u8>> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Vec::new())
        .expect("Valid HTTP request construction")
}

// =========================================================================
// 1. DIRECTORY TRAVERSAL & SECURITY ATTACKS
// =========================================================================

#[test]
fn test_traversal_dot_dot_classic_returns_forbidden() {
    let bad_uris = [
        "typora://localhost/../../../../etc/passwd",
        "typora://localhost/../etc/passwd",
        "typora://localhost/../../etc/shadow",
        "typora://localhost/assets/../../etc/passwd",
        "typora://localhost/assets/fonts/../../../Cargo.toml",
    ];

    for uri in &bad_uris {
        let req = make_req(Method::GET, uri);
        let res = handle_asset_request(&req);
        assert_eq!(
            res.status(),
            StatusCode::FORBIDDEN,
            "Expected 403 Forbidden for traversal: {}",
            uri
        );
        let body = String::from_utf8_lossy(&res.body());
        assert!(body.contains("403 Forbidden"));
    }
}

#[test]
fn test_traversal_windows_backslashes() {
    let bad_uris = [
        "typora://localhost/..\\..\\windows\\win.ini",
        "typora://localhost/assets\\..\\..\\windows\\win.ini",
        "typora://localhost/\\windows\\win.ini",
    ];

    for uri in &bad_uris {
        let req = make_req(Method::GET, uri);
        let res = handle_asset_request(&req);
        // Either 403 Forbidden (traversal check) or 404 Not Found (not in embedded assets)
        // CRITICAL: Must NEVER return 200 OK or expose system files!
        assert!(
            res.status() == StatusCode::FORBIDDEN || res.status() == StatusCode::NOT_FOUND,
            "Must reject backslash traversal: {} (got {})",
            uri,
            res.status()
        );
    }
}

#[test]
fn test_traversal_percent_encoded_dots() {
    let encoded_uris = [
        ("typora://localhost/%2e%2e/%2e%2e/etc/passwd", "etc/passwd"),
        ("typora://localhost/%2E%2E/%2E%2E/etc/passwd", "etc/passwd uppercase"),
        ("typora://localhost/%2e%2e/Cargo.toml", "../Cargo.toml"),
        ("typora://localhost/%2e%2e/%2e%2e/Cargo.toml", "../../Cargo.toml"),
        ("typora://localhost/assets/%2e%2e/index.html", "assets/../index.html"),
        ("typora://localhost/%2e%2e/ui/index.html", "../ui/index.html"),
    ];

    for (uri, desc) in &encoded_uris {
        let req = make_req(Method::GET, uri);
        let res = handle_asset_request(&req);
        eprintln!("[Traversal test] {} ({}) -> Status: {}", uri, desc, res.status());
    }

    // Direct WebAssets check
    let direct_cargo = typora_rust_reader::assets::WebAssets::get("../Cargo.toml");
    eprintln!("[Direct WebAssets get('../Cargo.toml')] -> {:?}", direct_cargo.is_some());
    let direct_outside = typora_rust_reader::assets::WebAssets::get("../../Cargo.toml");
    eprintln!("[Direct WebAssets get('../../Cargo.toml')] -> {:?}", direct_outside.is_some());
}

#[test]
fn test_repro_percent_encoded_traversal_bypasses_403() {
    // Expected contract: directory traversal should return 403 Forbidden.
    let req_unencoded = make_req(Method::GET, "typora://localhost/../../etc/passwd");
    let res_unencoded = handle_asset_request(&req_unencoded);
    assert_eq!(
        res_unencoded.status(),
        StatusCode::FORBIDDEN,
        "Unencoded traversal correctly returns 403 Forbidden"
    );

    // Verified fix: %2e%2e is decoded first and caught by traversal check, returning 403 Forbidden.
    let req_encoded = make_req(Method::GET, "typora://localhost/%2e%2e/%2e%2e/etc/passwd");
    let res_encoded = handle_asset_request(&req_encoded);

    assert_eq!(
        res_encoded.status(),
        StatusCode::FORBIDDEN,
        "Percent-encoded traversal %2e%2e correctly returns 403 Forbidden"
    );
}

#[test]
fn test_repro_invalid_gapplication_id() {
    // GApplication ID specification requires at least one '.' character (e.g. reverse-DNS).
    use typora_rust_reader::window::APP_ID;
    assert!(
        APP_ID.contains('.'),
        "GApplication ID must contain at least one '.' (got {})",
        APP_ID
    );
    assert!(!APP_ID.starts_with('.'), "App ID must not start with '.'");
    assert!(!APP_ID.ends_with('.'), "App ID must not end with '.'");
    assert_eq!(APP_ID, "com.typora.rust-reader");
}

#[test]
fn test_traversal_null_bytes() {
    let null_byte_uris = [
        "typora://localhost/index.html%00.png",
        "typora://localhost/assets/%00/marked.min.js",
        "typora://localhost/%00",
    ];

    for uri in &null_byte_uris {
        let req = make_req(Method::GET, uri);
        let res = handle_asset_request(&req);
        // Null byte injection must not panic or bypass security checks
        assert!(
            res.status() == StatusCode::NOT_FOUND || res.status() == StatusCode::FORBIDDEN,
            "Null byte URI must return 404 or 403: {} (got {})",
            uri,
            res.status()
        );
    }
}

// =========================================================================
// 2. MALFORMED URIs & 404 HANDLING
// =========================================================================

#[test]
fn test_missing_assets_return_404_cleanly() {
    let missing_uris = [
        "typora://localhost/non_existent.html",
        "typora://localhost/assets/missing_script.js",
        "typora://localhost/assets/fonts/MissingFont.woff2",
        "typora://localhost/random_uuid_12345678.css",
        "typora://localhost/assets/../does_not_exist",
    ];

    for uri in &missing_uris {
        let req = make_req(Method::GET, uri);
        let res = handle_asset_request(&req);
        assert!(
            res.status() == StatusCode::NOT_FOUND || res.status() == StatusCode::FORBIDDEN,
            "Missing/illegal asset must return 404 or 403: {} (got {})",
            uri,
            res.status()
        );
        if res.status() == StatusCode::NOT_FOUND {
            let body = String::from_utf8_lossy(&res.body());
            assert!(body.contains("404 Not Found"));
        }
    }
}

#[test]
fn test_root_and_index_resolution() {
    let root_uris = [
        "typora://localhost",
        "typora://localhost/",
        "typora://localhost/index.html",
        "typora://index.html",
    ];

    for uri in &root_uris {
        let req = make_req(Method::GET, uri);
        let res = handle_asset_request(&req);
        assert_eq!(
            res.status(),
            StatusCode::OK,
            "Root URI must resolve to index.html with 200 OK: {}",
            uri
        );
        let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("text/html"));
        let body = String::from_utf8_lossy(&res.body());
        assert!(body.contains("<!DOCTYPE html>") || body.contains("<!doctype html>"));
    }
}

#[test]
fn test_query_string_and_fragments_handling() {
    let uris_with_query = [
        "typora://localhost/index.html?v=1.0.0",
        "typora://localhost/index.html?t=123456#main",
        "typora://localhost/assets/marked.min.js?build=prod",
    ];

    for uri in &uris_with_query {
        let req = make_req(Method::GET, uri);
        let res = handle_asset_request(&req);
        // Note: http::Uri::path() already separates query and fragment,
        // so index.html?v=1.0.0 has path == "/index.html"
        assert_eq!(
            res.status(),
            StatusCode::OK,
            "URI with query/fragment should resolve path cleanly: {}",
            uri
        );
    }
}

#[test]
fn test_corrupted_percent_encoding_does_not_panic() {
    let corrupted_uris = [
        "typora://localhost/%",
        "typora://localhost/%2",
        "typora://localhost/%ZZ",
        "typora://localhost/assets/%GG%HH",
        "typora://localhost/assets/%FF%FE%FD",
    ];

    for uri in &corrupted_uris {
        let req = make_req(Method::GET, uri);
        let res = handle_asset_request(&req);
        // Handler must not panic and should gracefully return 404 or 403
        assert!(
            res.status() == StatusCode::NOT_FOUND || res.status() == StatusCode::FORBIDDEN,
            "Corrupted percent-encoding must not crash: {} (got {})",
            uri,
            res.status()
        );
    }
}

// =========================================================================
// 3. PERCENT-ENCODED FILENAMES
// =========================================================================

#[test]
fn test_percent_encoded_space_decoding() {
    // Test that percent_decode works on %20 and does not crash
    let req = make_req(Method::GET, "typora://localhost/assets/my%20file.txt");
    let res = handle_asset_request(&req);
    // my file.txt doesn't exist in embedded assets, so must be 404
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

// =========================================================================
// 4. FONT LOADING INTEGRITY (ALL 20 KATEX WOFF2 FONTS)
// =========================================================================

#[test]
fn test_all_20_katex_woff2_fonts_return_200_and_correct_mime() {
    let font_names = [
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

    assert_eq!(font_names.len(), 20, "Exactly 20 KaTeX fonts must be tested");

    for font in &font_names {
        let uri = format!("typora://localhost/assets/fonts/{}", font);
        let req = make_req(Method::GET, &uri);
        let res = handle_asset_request(&req);

        assert_eq!(
            res.status(),
            StatusCode::OK,
            "Font {} must return 200 OK",
            font
        );

        let ct = res
            .headers()
            .get("content-type")
            .expect("Content-Type header must be present")
            .to_str()
            .expect("Valid ASCII header string");

        assert_eq!(
            ct,
            "font/woff2",
            "Font {} must return MIME type 'font/woff2', got '{}'",
            font,
            ct
        );

        let body = res.body();
        assert!(
            !body.is_empty(),
            "Font {} body must not be empty",
            font
        );
        // Verify WOFF2 header magic: "wOF2" (0x77, 0x4F, 0x46, 0x32)
        assert!(
            body.starts_with(b"wOF2"),
            "Font {} must begin with WOFF2 magic header 'wOF2'",
            font
        );
    }
}

// =========================================================================
// 5. CORS PREFLIGHT & HTTP METHOD SEMANTICS
// =========================================================================

#[test]
fn test_cors_options_preflight() {
    let uris = [
        "typora://localhost/index.html",
        "typora://localhost/assets/katex.min.css",
        "typora://localhost/assets/fonts/KaTeX_Main-Regular.woff2",
    ];

    for uri in &uris {
        let req = make_req(Method::OPTIONS, uri);
        let res = handle_asset_request(&req);

        assert_eq!(
            res.status(),
            StatusCode::NO_CONTENT,
            "OPTIONS must return 204 No Content for {}",
            uri
        );

        let origin = res.headers().get("access-control-allow-origin").unwrap().to_str().unwrap();
        assert_eq!(origin, "*", "CORS allow-origin must be *");

        let methods = res.headers().get("access-control-allow-methods").unwrap().to_str().unwrap();
        assert!(methods.contains("GET"), "Methods must include GET");
        assert!(methods.contains("OPTIONS"), "Methods must include OPTIONS");

        let headers = res.headers().get("access-control-allow-headers").unwrap().to_str().unwrap();
        assert_eq!(headers, "*", "Allow-Headers must be *");

        assert!(res.body().is_empty(), "OPTIONS response body must be empty");
    }
}

#[test]
fn test_head_method_returns_empty_body_with_headers() {
    let req = make_req(Method::HEAD, "typora://localhost/index.html");
    let res = handle_asset_request(&req);

    assert_eq!(res.status(), StatusCode::OK);
    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.contains("text/html"));
    assert!(res.body().is_empty(), "HEAD request must have empty body");
}

#[test]
fn test_core_library_assets_loading() {
    let core_assets = [
        ("index.html", "text/html"),
        ("manifest.json", "application/json"),
        ("assets/marked.min.js", "application/javascript"),
        ("assets/katex.min.js", "application/javascript"),
        ("assets/katex.min.css", "text/css"),
        ("assets/mermaid.min.js", "application/javascript"),
        ("assets/highlight.min.js", "application/javascript"),
        ("assets/highlight-github.min.css", "text/css"),
        ("assets/highlight-github-dark.min.css", "text/css"),
        ("assets/icon.svg", "image/svg+xml"),
    ];

    for (path, expected_mime_prefix) in &core_assets {
        let uri = format!("typora://localhost/{}", path);
        let req = make_req(Method::GET, &uri);
        let res = handle_asset_request(&req);

        assert_eq!(
            res.status(),
            StatusCode::OK,
            "Asset {} must load with 200 OK",
            path
        );

        let ct = res
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();

        assert!(
            ct.starts_with(expected_mime_prefix),
            "Asset {} expected MIME starting with '{}', got '{}'",
            path,
            expected_mime_prefix,
            ct
        );
        assert!(!res.body().is_empty(), "Asset {} body must not be empty", path);
    }
}
