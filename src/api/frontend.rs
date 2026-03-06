//! Embedded frontend serving (HTML/JS/CSS/WASM).

use axum::{
    body::Body,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

#[derive(rust_embed::Embed)]
#[folder = "dist"]
#[include = "*.html"]
#[include = "*.js"]
#[include = "*.css"]
#[include = "*.wasm"]
struct Assets;

/// Serve the embedded frontend HTML/JS/CSS.
pub async fn serve_frontend(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    let (actual_path, content) = match Assets::get(path) {
        Some(content) => (path, content),
        None => match Assets::get("index.html") {
            Some(content) => ("index.html", content),
            None => return StatusCode::NOT_FOUND.into_response(),
        },
    };

    let mime = mime_guess::from_path(actual_path).first_or_octet_stream();

    Response::builder()
        .header(header::CONTENT_TYPE, mime.as_ref())
        .body(Body::from(content.data.into_owned()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
