//! Embedded static asset engine and zero-port custom URI protocol handler.
//!
//! Uses `rust-embed` to compile `ui/` (`index.html`, KaTeX, Mermaid, fonts, themes)
//! directly into the binary, serving requests to `typora://localhost/...` without opening
//! any localhost TCP sockets.

use rust_embed::RustEmbed;
use std::borrow::Cow;

/// Embedded application assets compiled directly into the executable binary.
#[derive(RustEmbed)]
#[folder = "ui/"]
pub struct WebAssets;

/// Custom URI protocol scheme for Typora reader.
pub const CUSTOM_SCHEME: &str = "typora";

/// Entry point URL for loading the main reader interface.
pub const APP_URL: &str = "typora://localhost/index.html";

/// Compile-time embedded help markdown for standalone installations.
pub const EMBEDDED_HELP_MD: &str = include_str!("../help/README.md");

/// Creates the custom protocol closure to pass to `wry::WebViewBuilder::with_custom_protocol`.
pub fn create_custom_protocol_handler(
) -> impl Fn(wry::WebViewId, http::Request<Vec<u8>>) -> http::Response<Cow<'static, [u8]>>
       + Send
       + Sync
       + 'static {
    move |_webview_id, request| handle_asset_request(&request)
}

/// Processes an inbound HTTP request to `typora://...` and returns an HTTP response.
///
/// Designed as a pure function to enable 100% headless unit and integration testing.
pub fn handle_asset_request(
    request: &http::Request<Vec<u8>>,
) -> http::Response<Cow<'static, [u8]>> {
    // 1. Handle CORS preflight requests (OPTIONS)
    if request.method() == http::Method::OPTIONS {
        return http::Response::builder()
            .status(http::StatusCode::NO_CONTENT)
            .header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(http::header::ACCESS_CONTROL_ALLOW_METHODS, "GET, HEAD, OPTIONS")
            .header(http::header::ACCESS_CONTROL_ALLOW_HEADERS, "*")
            .body(Cow::Borrowed(b"" as &'static [u8]))
            .unwrap();
    }

    // 2. Extract and sanitize URI path
    let uri = request.uri();
    let path_str = uri.path();
    let mut clean_path = path_str.trim_start_matches('/');

    // Handle alternate host routing (e.g. typora://index.html)
    if clean_path.is_empty() {
        if let Some(host) = uri.host() {
            if host != "localhost" && !host.is_empty() {
                clean_path = host;
            }
        }
    }

    // Default to index.html for root requests
    if clean_path.is_empty() {
        clean_path = "index.html";
    }

    // 3. URL percent-decoding FIRST (Normalize Before Validation - CWE-20/CWE-23)
    let decoded_path = percent_decode(clean_path);

    // 4. Security: Prevent directory traversal on the DECODED path
    if decoded_path.contains("..") || decoded_path.starts_with('/') || decoded_path.starts_with('\\') {
        return http::Response::builder()
            .status(http::StatusCode::FORBIDDEN)
            .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(Cow::Borrowed(b"403 Forbidden" as &'static [u8]))
            .unwrap();
    }

    // 5. Query embedded assets
    match WebAssets::get(&decoded_path) {
        Some(file) => {
            let mime = mime_guess::from_path(decoded_path.as_ref()).first_or_octet_stream();
            let content_type = match mime.as_ref() {
                "text/html" => "text/html; charset=utf-8",
                "text/css" => "text/css; charset=utf-8",
                "application/javascript" | "text/javascript" => {
                    "application/javascript; charset=utf-8"
                }
                "application/json" => "application/json; charset=utf-8",
                "text/markdown" => "text/markdown; charset=utf-8",
                other => other,
            };

            let body = if request.method() == http::Method::HEAD {
                Cow::Borrowed(b"" as &'static [u8])
            } else {
                file.data
            };

            http::Response::builder()
                .status(http::StatusCode::OK)
                .header(http::header::CONTENT_TYPE, content_type)
                .header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                .header(http::header::CACHE_CONTROL, "no-cache")
                .body(body)
                .unwrap()
        }
        None => http::Response::builder()
            .status(http::StatusCode::NOT_FOUND)
            .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(Cow::Borrowed(b"404 Not Found" as &'static [u8]))
            .unwrap(),
    }
}

/// Simple, zero-dependency percent-decoding for URL path strings.
///
/// Returns a borrowed `Cow` slice when no `%` characters are present,
/// avoiding any memory allocation on the hot path for static asset loading.
fn percent_decode(input: &str) -> Cow<'_, str> {
    if !input.contains('%') {
        return Cow::Borrowed(input);
    }
    let mut result = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""),
                16,
            ) {
                result.push(byte);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }
    match String::from_utf8(result) {
        Ok(s) => Cow::Owned(s),
        Err(_) => Cow::Borrowed(input),
    }
}

/// Retrieve raw bytes of an embedded asset directly.
pub fn get_asset(path: &str) -> Option<Cow<'static, [u8]>> {
    WebAssets::get(path.trim_start_matches('/')).map(|f| f.data)
}

/// Retrieve an embedded asset as a UTF-8 string.
pub fn get_asset_str(path: &str) -> Option<String> {
    get_asset(path).and_then(|bytes| String::from_utf8(bytes.into_owned()).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_asset_request_options() {
        let req = http::Request::builder()
            .method(http::Method::OPTIONS)
            .uri("typora://localhost/index.html")
            .body(Vec::new())
            .unwrap();
        let res = handle_asset_request(&req);
        assert_eq!(res.status(), http::StatusCode::NO_CONTENT);
        assert_eq!(
            res.headers().get("access-control-allow-origin").unwrap(),
            "*"
        );
    }

    #[test]
    fn test_handle_asset_request_traversal_forbidden() {
        let req = http::Request::builder()
            .uri("typora://localhost/../../etc/passwd")
            .body(Vec::new())
            .unwrap();
        let res = handle_asset_request(&req);
        assert_eq!(res.status(), http::StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_handle_asset_request_percent_encoded_traversal_forbidden() {
        let bad_uris = [
            "typora://localhost/%2e%2e/%2e%2e/etc/passwd",
            "typora://localhost/%2E%2E/%2E%2E/etc/passwd",
            "typora://localhost/%2e%2e/Cargo.toml",
            "typora://localhost/assets/%2e%2e/index.html",
            "typora://localhost/%2fetc/passwd",
            "typora://localhost/%5cwindows\\win.ini",
        ];

        for uri in &bad_uris {
            let req = http::Request::builder()
                .uri(*uri)
                .body(Vec::new())
                .unwrap();
            let res = handle_asset_request(&req);
            assert_eq!(
                res.status(),
                http::StatusCode::FORBIDDEN,
                "Expected 403 Forbidden for traversal attempt: {}",
                uri
            );
        }
    }

    #[test]
    fn test_handle_asset_request_not_found() {
        let req = http::Request::builder()
            .uri("typora://localhost/assets/non_existent.xyz")
            .body(Vec::new())
            .unwrap();
        let res = handle_asset_request(&req);
        assert_eq!(res.status(), http::StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_handle_asset_request_index_html() {
        let req = http::Request::builder()
            .uri("typora://localhost/index.html")
            .body(Vec::new())
            .unwrap();
        let res = handle_asset_request(&req);
        assert_eq!(res.status(), http::StatusCode::OK);
        assert!(res
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("text/html"));
        assert!(!res.body().is_empty());
    }

    #[test]
    fn test_get_asset_str_embedded() {
        let manifest = get_asset_str("manifest.json");
        assert!(manifest.is_some());
        assert!(manifest.unwrap().contains("name"));
    }
}
