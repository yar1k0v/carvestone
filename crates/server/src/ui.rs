//! The web UI at `/`: the built `web/dist` embedded in the binary with feature `ui`, otherwise a
//! page that says how to build it.

use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};

#[cfg(not(feature = "ui"))]
const PLACEHOLDER: &str = "<!doctype html>\n<html lang=\"en\">\n<head><meta charset=\"utf-8\"><title>stone</title></head>\n<body>\n<h1>stone</h1>\n<p>This build has no web UI. The JSON API is at <code>/api</code>.</p>\n<p>To get the UI, run <code>pnpm --dir web build</code>, then build the binary with <code>--features ui</code>.</p>\n</body>\n</html>\n";

/// The page for a GET outside `/api`.
#[cfg(not(feature = "ui"))]
pub(crate) fn page(_path: &str) -> Response {
    let mut response = PLACEHOLDER.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
}

#[cfg(feature = "ui")]
#[derive(rust_embed::RustEmbed)]
#[folder = "../../web/dist"]
struct Dist;

/// The embedded file for `path`, or `index.html` so the client router can take the route. A path
/// that names a file (it has an extension) and is missing stays a 404.
#[cfg(feature = "ui")]
pub(crate) fn page(path: &str) -> Response {
    use axum::http::StatusCode;

    let wanted = path.trim_start_matches('/');
    let exact = if wanted.is_empty() { None } else { Dist::get(wanted) };
    let (name, file) = match exact {
        Some(file) => (wanted, file),
        None if wanted.rsplit('/').next().is_some_and(|last| last.contains('.')) => {
            return StatusCode::NOT_FOUND.into_response();
        }
        None => match Dist::get("index.html") {
            Some(file) => ("index.html", file),
            None => return StatusCode::NOT_FOUND.into_response(),
        },
    };
    // Build output names carry a content hash, so they never change under the same name.
    let cache = if name.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    let mut response = file.data.into_owned().into_response();
    let headers = response.headers_mut();
    if let Ok(mime) = HeaderValue::from_str(file.metadata.mimetype()) {
        headers.insert(header::CONTENT_TYPE, mime);
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    response
}
