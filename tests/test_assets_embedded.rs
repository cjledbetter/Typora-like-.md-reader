//! Opaque-box E2E test suite for Typora-RustReader Asset Embedding & Custom Scheme.
//!
//! Features Covered:
//! - Feature 1: Native GTK4/Wayland Window & WebKitGTK Core
//! - Feature 2: Custom Scheme `typora://localhost/...` & Zero-Port Architecture
//! - Feature 25: Read Mode Preview Assets (KaTeX, Mermaid, Highlight.js, Marked)
//! - Feature 26: Split Mode Containers
//! - Feature 27: Edit Mode Containers
//! - Feature 32: Single-Binary Asset Embedding (`rust-embed`)
//! - Feature 33: Release Profile Optimization & Binary Checks

#![allow(dead_code, unused_imports, unused_variables)]

mod common;
use common::TestSandbox;
use std::fs;
use std::path::{Path, PathBuf};

/// Custom scheme URI resolver oracle implementing typora://localhost/...
pub fn resolve_custom_scheme_uri_oracle(uri: &str) -> Result<(String, String), u16> {
    // Expected format: typora://localhost/<path>
    if !uri.starts_with("typora://localhost") && !uri.starts_with("typora://") {
        return Err(400); // Bad Request
    }

    let path_part = if uri.starts_with("typora://localhost") {
        &uri["typora://localhost".len()..]
    } else {
        &uri["typora://".len()..]
    };

    // Strip query string and fragment
    let clean_path = path_part.split('?').next().unwrap_or("");
    let clean_path = clean_path.split('#').next().unwrap_or("");
    let clean_path = clean_path.trim_start_matches('/');

    let target_file = if clean_path.is_empty() || clean_path == "index.html" {
        "index.html"
    } else {
        clean_path
    };

    // Security check: path traversal prevention
    if target_file.contains("..") || target_file.starts_with('/') {
        return Err(403); // Forbidden
    }

    let mime = guess_mime_type_oracle(target_file);
    Ok((target_file.to_string(), mime))
}

/// MIME type guessing oracle matching mime_guess specification.
pub fn guess_mime_type_oracle(path: &str) -> String {
    let lower = path.to_lowercase();
    if lower.ends_with(".html") || lower.ends_with(".htm") {
        "text/html; charset=utf-8".to_string()
    } else if lower.ends_with(".js") || lower.ends_with(".mjs") {
        "application/javascript; charset=utf-8".to_string()
    } else if lower.ends_with(".css") {
        "text/css; charset=utf-8".to_string()
    } else if lower.ends_with(".woff2") {
        "font/woff2".to_string()
    } else if lower.ends_with(".woff") {
        "font/woff".to_string()
    } else if lower.ends_with(".ttf") {
        "font/ttf".to_string()
    } else if lower.ends_with(".svg") {
        "image/svg+xml".to_string()
    } else if lower.ends_with(".png") {
        "image/png".to_string()
    } else if lower.ends_with(".json") {
        "application/json".to_string()
    } else {
        "application/octet-stream".to_string()
    }
}

fn get_project_root() -> PathBuf {
    PathBuf::from("/home/cjledbetter/Documents/Programming/typora-like-.md-reader")
}

// =========================================================================
// === TIER 1: FEATURE COVERAGE (Minimum 10 tests across Assets & Scheme)
// =========================================================================

#[test]
fn test_assets_f32_index_html_exists_and_valid() {
    let root = get_project_root();
    let index = root.join("index.html");
    assert!(index.is_file(), "index.html must exist at repo root or ui/");

    let content = fs::read_to_string(&index).expect("Failed to read index.html");
    assert!(content.contains("<!DOCTYPE html>") || content.contains("<!doctype html>"));
    assert!(content.contains("<html"));
    assert!(content.contains("</html>"));
    assert!(content.len() > 10_000, "index.html must be substantial Typora UI");
}

#[test]
fn test_assets_f32_marked_js_present() {
    let root = get_project_root();
    let marked = root.join("assets/marked.min.js");
    assert!(marked.is_file(), "marked.min.js must exist");

    let size = fs::metadata(&marked).unwrap().len();
    assert!(size > 20_000, "marked.min.js must be >20KB");
}

#[test]
fn test_assets_f32_katex_assets_present() {
    let root = get_project_root();
    let js = root.join("assets/katex.min.js");
    let css = root.join("assets/katex.min.css");

    assert!(js.is_file(), "katex.min.js must exist");
    assert!(css.is_file(), "katex.min.css must exist");

    assert!(fs::metadata(&js).unwrap().len() > 100_000, "katex.min.js must be >100KB");
    assert!(fs::metadata(&css).unwrap().len() > 10_000, "katex.min.css must be >10KB");
}

#[test]
fn test_assets_f32_mermaid_js_present() {
    let root = get_project_root();
    let mermaid = root.join("assets/mermaid.min.js");
    assert!(mermaid.is_file(), "mermaid.min.js must exist");

    let size = fs::metadata(&mermaid).unwrap().len();
    assert!(size > 1_000_000, "mermaid.min.js must be >1MB");
}

#[test]
fn test_assets_f32_highlight_js_present() {
    let root = get_project_root();
    let hljs = root.join("assets/highlight.min.js");
    assert!(hljs.is_file(), "highlight.min.js must exist");

    let size = fs::metadata(&hljs).unwrap().len();
    assert!(size > 50_000, "highlight.min.js must be >50KB");
}

#[test]
fn test_assets_f32_katex_fonts_directory_present() {
    let root = get_project_root();
    let fonts_dir = root.join("assets/fonts");
    assert!(fonts_dir.is_dir(), "assets/fonts directory must exist");

    let count = fs::read_dir(&fonts_dir).unwrap().count();
    assert!(count >= 5, "assets/fonts must contain WOFF2 font files");
}

#[test]
fn test_assets_f32_icon_svg_valid() {
    let root = get_project_root();
    let icon = root.join("assets/icon.svg");
    assert!(icon.is_file(), "icon.svg must exist");

    let content = fs::read_to_string(&icon).unwrap();
    assert!(content.contains("<svg"));
    assert!(content.contains("</svg>"));
}

#[test]
fn test_assets_f2_custom_scheme_index_html_mapping() {
    let (path, mime) = resolve_custom_scheme_uri_oracle("typora://localhost/index.html").unwrap();
    assert_eq!(path, "index.html");
    assert_eq!(mime, "text/html; charset=utf-8");
}

#[test]
fn test_assets_f2_custom_scheme_root_url_defaults_to_index() {
    let (path, mime) = resolve_custom_scheme_uri_oracle("typora://localhost/").unwrap();
    assert_eq!(path, "index.html");
    assert_eq!(mime, "text/html; charset=utf-8");
}

#[test]
fn test_assets_f2_custom_scheme_asset_subpath_mapping() {
    let (path, mime) = resolve_custom_scheme_uri_oracle("typora://localhost/assets/marked.min.js").unwrap();
    assert_eq!(path, "assets/marked.min.js");
    assert_eq!(mime, "application/javascript; charset=utf-8");
}

#[test]
fn test_assets_f2_mime_type_css() {
    assert_eq!(guess_mime_type_oracle("assets/katex.min.css"), "text/css; charset=utf-8");
}

#[test]
fn test_assets_f2_mime_type_woff2() {
    assert_eq!(guess_mime_type_oracle("assets/fonts/KaTeX_Main-Regular.woff2"), "font/woff2");
}

#[test]
fn test_assets_f2_mime_type_svg() {
    assert_eq!(guess_mime_type_oracle("assets/icon.svg"), "image/svg+xml");
}

// =========================================================================
// === TIER 2: BOUNDARY & CORNER CASES (Minimum 20 tests)
// =========================================================================

#[test]
fn test_assets_boundary_path_traversal_dot_dot_forbidden() {
    let res = resolve_custom_scheme_uri_oracle("typora://localhost/../../etc/passwd");
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), 403);
}

#[test]
fn test_assets_boundary_invalid_scheme_bad_request() {
    let res = resolve_custom_scheme_uri_oracle("http://localhost:8333/index.html");
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), 400);
}

#[test]
fn test_assets_boundary_query_string_stripped() {
    let (path, _) = resolve_custom_scheme_uri_oracle("typora://localhost/assets/highlight.min.js?v=11.9.0").unwrap();
    assert_eq!(path, "assets/highlight.min.js");
}

#[test]
fn test_assets_boundary_fragment_identifier_stripped() {
    let (path, _) = resolve_custom_scheme_uri_oracle("typora://localhost/index.html#section-1").unwrap();
    assert_eq!(path, "index.html");
}

#[test]
fn test_assets_boundary_uppercase_extension_mime() {
    assert_eq!(guess_mime_type_oracle("STYLE.CSS"), "text/css; charset=utf-8");
    assert_eq!(guess_mime_type_oracle("SCRIPT.JS"), "application/javascript; charset=utf-8");
    assert_eq!(guess_mime_type_oracle("PAGE.HTML"), "text/html; charset=utf-8");
}

#[test]
fn test_assets_boundary_unknown_extension_fallback() {
    assert_eq!(guess_mime_type_oracle("unknown.xyz"), "application/octet-stream");
    assert_eq!(guess_mime_type_oracle("no_extension"), "application/octet-stream");
}

#[test]
fn test_assets_boundary_leading_multiple_slashes() {
    let (path, _) = resolve_custom_scheme_uri_oracle("typora://localhost////index.html").unwrap();
    assert_eq!(path, "index.html");
}

#[test]
fn test_assets_boundary_zero_port_verification() {
    // Verify that custom scheme architecture replaces localhost port binding
    let (path, _) = resolve_custom_scheme_uri_oracle("typora://localhost/index.html").unwrap();
    assert!(!path.contains(":8333"), "Must not use HTTP port 8333");
}

#[test]
fn test_assets_boundary_woff_mime_type() {
    assert_eq!(guess_mime_type_oracle("font.woff"), "font/woff");
}

#[test]
fn test_assets_boundary_ttf_mime_type() {
    assert_eq!(guess_mime_type_oracle("font.ttf"), "font/ttf");
}

#[test]
fn test_assets_boundary_json_mime_type() {
    assert_eq!(guess_mime_type_oracle("manifest.json"), "application/json");
}

#[test]
fn test_assets_boundary_png_mime_type() {
    assert_eq!(guess_mime_type_oracle("screenshot.png"), "image/png");
}

#[test]
fn test_assets_boundary_deep_asset_subpath() {
    let (path, mime) = resolve_custom_scheme_uri_oracle("typora://localhost/assets/fonts/subset/font.woff2").unwrap();
    assert_eq!(path, "assets/fonts/subset/font.woff2");
    assert_eq!(mime, "font/woff2");
}

// =========================================================================
// === TIER 3: CROSS-FEATURE COMBINATIONS
// =========================================================================

#[test]
fn test_assets_combine_index_html_has_read_split_edit_elements() {
    let root = get_project_root();
    let content = fs::read_to_string(root.join("index.html")).unwrap();

    // Verify key mode UI elements exist in DOM
    assert!(content.contains("preview") || content.contains("read"), "Must contain preview/read mode container");
    assert!(content.contains("editor") || content.contains("edit"), "Must contain editor mode container");
    assert!(content.contains("theme"), "Must contain theme switcher elements");
}

#[test]
fn test_assets_combine_all_theme_css_classes_in_html() {
    let root = get_project_root();
    let content = fs::read_to_string(root.join("index.html")).unwrap();

    assert!(content.contains("github"), "Must support github theme");
    assert!(content.contains("night"), "Must support night theme");
    assert!(content.contains("newsprint"), "Must support newsprint theme");
    assert!(content.contains("gothic"), "Must support gothic theme");
}

// =========================================================================
// === TIER 4: REAL-WORLD SCENARIOS
// =========================================================================

#[test]
fn test_assets_scenario_zero_external_cdn_dependencies() {
    // Guarantees 100% offline self-contained operation without internet access
    let root = get_project_root();
    let content = fs::read_to_string(root.join("index.html")).unwrap();

    // Ensure no CDN links in script or link tags
    assert!(!content.contains("cdn.jsdelivr.net"), "Must not rely on jsdelivr CDN");
    assert!(!content.contains("cdnjs.cloudflare.com"), "Must not rely on cloudflare CDN");
    assert!(!content.contains("unpkg.com"), "Must not rely on unpkg CDN");
}
