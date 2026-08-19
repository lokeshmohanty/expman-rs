//! Shared application state for the Axum server.

use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::error;

use super::jupyter_service::{
    JupyterCommand, JupyterManager, NotebookTemplateConfig, DEFAULT_JUPYTER_COMMAND,
};

#[derive(Clone)]
pub struct AppState {
    pub base_dir: Arc<PathBuf>,
    pub jupyter: super::jupyter_service::JupyterManager,
    pub tensorboard: super::tensorboard_service::TensorBoardManager,
    pub shutdown_token: CancellationToken,
    /// Whether SSE streaming is enabled. Reported by `/api/config`.
    pub live_mode: bool,
    /// Whether every mutating request is refused. Reported by `/api/config` so
    /// the frontend can hide controls rather than offering writes that 403.
    pub read_only: bool,
}

impl AppState {
    pub fn new(base_dir: PathBuf) -> Self {
        Self::from_config(&ServerConfig {
            base_dir,
            ..Default::default()
        })
    }

    pub fn from_config(config: &ServerConfig) -> Self {
        // `exp serve` rejects an unparsable command up front, so this branch only
        // guards a library caller that built a `ServerConfig` by hand. Falling
        // back beats refusing to serve: every other tab still works.
        let jupyter_command = JupyterCommand::parse(&config.jupyter_command).unwrap_or_else(|e| {
            error!("{e}; falling back to `{DEFAULT_JUPYTER_COMMAND}`");
            JupyterCommand::default()
        });
        let template = NotebookTemplateConfig {
            explicit: config.notebook_template.clone(),
            base_dir: config.base_dir.clone(),
        };

        Self {
            base_dir: Arc::new(config.base_dir.clone()),
            jupyter: JupyterManager::with_config(jupyter_command, template),
            tensorboard: super::tensorboard_service::TensorBoardManager::new(),
            shutdown_token: CancellationToken::new(),
            live_mode: config.live_mode,
            read_only: config.read_only,
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
    /// Refuse every mutating request, so a store can be shared read-only.
    pub read_only: bool,
    /// A project-supplied `.ipynb` to generate interactive notebooks from
    /// (`--notebook-template`). `None` falls back to
    /// `<base_dir>/.expman/notebook.ipynb`, then to the built-in default.
    pub notebook_template: Option<PathBuf>,
    /// The command line that launches Jupyter (`--jupyter-command`), e.g.
    /// `uv run --extra nb jupyter` to get the project's virtualenv as the kernel.
    pub jupyter_command: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("experiments"),
            host: "127.0.0.1".to_string(),
            port: 8000,
            live_mode: true,
            read_only: false,
            notebook_template: None,
            jupyter_command: DEFAULT_JUPYTER_COMMAND.to_string(),
        }
    }
}
