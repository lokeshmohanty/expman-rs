//! expman-server: Axum web server with REST API, SSE live streaming, and embedded Leptos frontend.

// ── Frontend module (WASM only) ──────────────────────────────────────────────
#[cfg(target_arch = "wasm32")]
pub mod app;

// ── Server module (native only) ─────────────────────────────────────────────
#[cfg(not(target_arch = "wasm32"))]
mod api;

// Re-export public items for native builds
#[cfg(not(target_arch = "wasm32"))]
pub use api::{build_router, serve};

#[cfg(not(target_arch = "wasm32"))]
pub use api::state::{AppState, ServerConfig};
