#![doc = include_str!("../../../docs/expman-server.md")]
//! expman-server: Axum web server with REST API and SSE live streaming.

pub mod api;
pub mod jupyter;
pub mod state;

use axum::Router;
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

use crate::state::AppState;

pub use state::ServerConfig;

/// Build the Axum router with all routes.
pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // API routes
        .nest("/api", api::router())
        // Frontend: serve embedded static files
        .fallback(api::serve_frontend)
        .with_state(state)
        .layer(cors)
}

/// Start the server on the given address.
pub async fn serve(config: ServerConfig) -> anyhow::Result<()> {
    let state = AppState::new(config.base_dir.clone());
    let app = build_router(state);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    info!("ExpMan dashboard at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
