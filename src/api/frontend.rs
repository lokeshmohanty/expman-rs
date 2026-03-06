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
    let path_str = uri.path();
    let path = path_str.trim_start_matches('/');

    // Handle direct hits
    if let Some(content) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return Response::builder()
            .header(header::CONTENT_TYPE, mime.as_ref())
            .body(Body::from(content.data.into_owned()))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
            .into_response();
    }

    // Special case for root
    if path.is_empty() || path == "index.html" {
        if let Some(content) = Assets::get("index.html") {
            return Response::builder()
                .header(header::CONTENT_TYPE, "text/html")
                .body(Body::from(content.data.into_owned()))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
                .into_response();
        }
    }

    // Fallback to index.html ONLY for non-asset paths (SPA routing)
    let is_asset = path_str.contains('.') && !path_str.ends_with(".html");
    if !is_asset {
        if let Some(content) = Assets::get("index.html") {
            return Response::builder()
                .header(header::CONTENT_TYPE, "text/html")
                .body(Body::from(content.data.into_owned()))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
                .into_response();
        }
    }

    if is_asset {
        tracing::warn!("Frontend asset not found: {}", path_str);
    }
    StatusCode::NOT_FOUND.into_response()
}
