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
// Self-hosted webfonts. Without this the @font-face URLs 404 and the dashboard
// silently falls back to system faces — which looks like a CSS bug, not a
// missing asset.
#[include = "*.woff2"]
struct Assets;

/// Extensions that mean "a file from the bundle", not "a route in the SPA".
///
/// The list exists because a dot in the path proves nothing: an experiment is
/// named after its environment, and dm_control task ids carry a dot in the
/// middle of the name — `/experiments/dmc-cartpole.swingup` is a route, not a
/// request for a `swingup` file.
///
/// It is deliberately wider than what [`Assets`] embeds. Getting this wrong in
/// the asset direction is the expensive mistake: answering a `<script>` with
/// `index.html` gives the browser HTML where it expected JavaScript, and the
/// dashboard dies on a syntax error that names the wrong file. Answering a
/// missing route with a 404 only costs a reload.
///
/// `html` is *not* here on purpose. Serving the shell to an HTML request is
/// exactly what SPA fallback is for, and it cannot mislead a parser.
const ASSET_EXTENSIONS: &[&str] = &[
    "js", "mjs", "css", "wasm", "woff2", "woff", "ttf", "map", "ico", "png", "svg", "json",
];

/// Whether this path is asking for a bundled file rather than an SPA route.
///
/// Only the final segment can carry an extension. A dot anywhere earlier is
/// part of a directory name — `/experiments/dmc-cartpole.swingup/runs/latest`
/// is a route, and the old whole-path test called it an asset and 404'd it.
fn is_asset_request(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .and_then(|segment| segment.rsplit_once('.'))
        .is_some_and(|(_, extension)| {
            ASSET_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

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
    let is_asset = is_asset_request(path_str);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_experiment_named_after_a_dm_control_task_is_a_route() {
        // The bug this replaced: every dm_control id carries a dot, so every
        // one of these 404'd and the dashboard could not be deep-linked.
        assert!(!is_asset_request("/experiments/dmc-cartpole.swingup"));
        assert!(!is_asset_request("/experiments/dmc-walker.walk"));
        assert!(!is_asset_request("/experiments/Craftax-Symbolic-v1"));
    }

    #[test]
    fn a_dot_in_a_parent_segment_does_not_make_a_route_an_asset() {
        assert!(!is_asset_request(
            "/experiments/dmc-cheetah.run/runs/latest"
        ));
    }

    #[test]
    fn bundled_files_are_still_assets() {
        for path in [
            "/index-abc123.js",
            "/style.css",
            "/expman_bg.wasm",
            "/fonts/inter.woff2",
            "/favicon.ico",
        ] {
            assert!(
                is_asset_request(path),
                "{path} should be served as an asset"
            );
        }
    }

    #[test]
    fn a_missing_script_404s_rather_than_returning_the_shell() {
        // Answering JS with HTML is the failure that hides its own cause: the
        // browser reports a syntax error in a file that was never served.
        assert!(is_asset_request("/assets/deleted.JS"));
    }

    #[test]
    fn html_falls_through_to_the_shell() {
        assert!(!is_asset_request("/some/page.html"));
    }

    #[test]
    fn the_root_and_extensionless_paths_are_routes() {
        assert!(!is_asset_request("/"));
        assert!(!is_asset_request(""));
        assert!(!is_asset_request("/experiments"));
    }
}
