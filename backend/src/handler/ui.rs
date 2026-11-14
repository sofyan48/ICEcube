//! Serves the web UI from files embedded in the binary.
//!
//! Release builds embed `frontend/dist` at compile time, so the binary is the
//! whole application. Debug builds read the same folder from disk on each
//! request (rust-embed's default), so `bun run build` shows up without
//! recompiling; for UI work use the Vite dev server instead.

use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../frontend/dist"]
// A missing folder is fine while developing (the UI comes from Vite);
// build.rs refuses release builds without it.
#[allow_missing = true]
struct Assets;

/// Fallback handler: static files, with `index.html` for client-side routes.
pub async fn serve(method: Method, uri: Uri, headers: HeaderMap) -> Response {
    if method != Method::GET && method != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let path = uri.path().trim_start_matches('/');
    let if_none_match = headers.get(header::IF_NONE_MATCH);

    if let Some(res) = file(path, if_none_match) {
        return res;
    }
    // Missing files under these prefixes are real 404s, and so is anything
    // that looks like a file. Everything else is a Vue Router route.
    let last = path.rsplit('/').next().unwrap_or_default();
    if path.starts_with("assets/") || path.starts_with("api/") || last.contains('.') {
        return StatusCode::NOT_FOUND.into_response();
    }
    file("index.html", if_none_match).unwrap_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            "UI not built. Run `bun run build` in frontend/, or use the Vite dev server on :5173.",
        )
            .into_response()
    })
}

fn file(path: &str, if_none_match: Option<&HeaderValue>) -> Option<Response> {
    let path = if path.is_empty() { "index.html" } else { path };
    let asset = Assets::get(path)?;

    let cache = cache_control(path);
    let etag = format!("\"{}\"", hex(&asset.metadata.sha256_hash()[..16]));
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    if let Ok(v) = HeaderValue::from_str(&etag) {
        headers.insert(header::ETAG, v);
    }

    // Revalidation (mainly index.html, which is `no-cache`) costs a 304, not a download.
    if if_none_match.is_some_and(|v| etag_matches(v, &etag)) {
        return Some((StatusCode::NOT_MODIFIED, headers).into_response());
    }

    if let Ok(v) = HeaderValue::from_str(asset.metadata.mimetype()) {
        headers.insert(header::CONTENT_TYPE, v);
    }
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if path == "index.html" {
        // The UI never needs to be framed; blocks clickjacking of the admin UI.
        headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
        headers.insert(
            header::REFERRER_POLICY,
            HeaderValue::from_static("same-origin"),
        );
    }
    Some((headers, Body::from(asset.data.into_owned())).into_response())
}

/// Vite fingerprints everything under assets/, so it can be cached forever.
/// index.html must be revalidated so new releases are picked up immediately.
fn cache_control(path: &str) -> &'static str {
    if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else if path == "index.html" {
        "no-cache"
    } else {
        "public, max-age=3600"
    }
}

fn etag_matches(header: &HeaderValue, etag: &str) -> bool {
    header.to_str().is_ok_and(|v| {
        v.split(',')
            .map(|t| t.trim().trim_start_matches("W/"))
            .any(|t| t == "*" || t == etag)
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Number of embedded files, logged at startup.
pub fn file_count() -> usize {
    Assets::iter().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::to_bytes, http::Request};
    use tower::ServiceExt;

    fn app() -> Router {
        Router::new().fallback(serve)
    }

    async fn get(path: &str) -> Response {
        app()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    /// Asset tests need a built UI; skip (pass) when frontend/dist is missing.
    fn ui_built() -> bool {
        Assets::get("index.html").is_some()
    }

    #[test]
    fn cache_rules() {
        assert_eq!(
            cache_control("assets/index-abc.js"),
            "public, max-age=31536000, immutable"
        );
        assert_eq!(cache_control("index.html"), "no-cache");
        assert_eq!(cache_control("favicon.svg"), "public, max-age=3600");
    }

    #[test]
    fn etag_matching() {
        let tag = "\"abc\"";
        assert!(etag_matches(&HeaderValue::from_static("\"abc\""), tag));
        assert!(etag_matches(&HeaderValue::from_static("W/\"abc\""), tag));
        assert!(etag_matches(
            &HeaderValue::from_static("\"x\", \"abc\""),
            tag
        ));
        assert!(etag_matches(&HeaderValue::from_static("*"), tag));
        assert!(!etag_matches(&HeaderValue::from_static("\"other\""), tag));
    }

    #[tokio::test]
    async fn rejects_writes_and_unknown_files() {
        let res = app()
            .oneshot(Request::post("/users").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(
            get("/assets/missing-123.js").await.status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(get("/robots.txt").await.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn serves_index_for_spa_routes() {
        if !ui_built() {
            return;
        }
        for path in ["/", "/users", "/groups/default"] {
            let res = get(path).await;
            assert_eq!(res.status(), StatusCode::OK, "{path}");
            assert_eq!(res.headers()[header::CACHE_CONTROL], "no-cache");
            assert_eq!(res.headers()[header::X_FRAME_OPTIONS], "DENY");
            let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
            assert!(
                std::str::from_utf8(&body)
                    .unwrap()
                    .contains("<div id=\"app\">")
            );
        }
    }

    #[tokio::test]
    async fn serves_hashed_assets_and_revalidates() {
        if !ui_built() {
            return;
        }
        let asset = Assets::iter()
            .find(|p| p.starts_with("assets/") && p.ends_with(".js"))
            .expect("built UI has a JS asset");
        let res = get(&format!("/{asset}")).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert!(
            res.headers()[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .contains("javascript")
        );
        assert!(
            res.headers()[header::CACHE_CONTROL]
                .to_str()
                .unwrap()
                .contains("immutable")
        );

        let etag = get("/").await.headers()[header::ETAG].clone();
        let res = app()
            .oneshot(
                Request::get("/")
                    .header(header::IF_NONE_MATCH, etag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_MODIFIED);
        assert!(
            to_bytes(res.into_body(), usize::MAX)
                .await
                .unwrap()
                .is_empty()
        );
    }
}
