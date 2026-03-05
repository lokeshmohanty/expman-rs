//! Shared application state for the Axum server.

use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct AppState {
    pub base_dir: Arc<PathBuf>,
    pub jupyter: super::jupyter_service::JupyterManager,
    pub shutdown_token: CancellationToken,
}

impl AppState {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir: Arc::new(base_dir),
            jupyter: super::jupyter_service::JupyterManager::new(),
            shutdown_token: CancellationToken::new(),
        }
    }
}

/// Configuration for the web server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub base_dir: PathBuf,
    pub host: String,
    pub port: u16,
    pub live_mode: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("experiments"),
            host: "127.0.0.1".to_string(),
            port: 8000,
            live_mode: true,
        }
    }
}
