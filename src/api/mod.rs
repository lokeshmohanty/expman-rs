#![doc = include_str!("./README.md")]
//! API module: Axum router, helpers, and handler submodules.

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::{
    extract::State,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

pub use self::state::AppState;

pub use self::state::ServerConfig;

mod artifacts;
pub mod experiments;
pub mod frontend;
pub mod jupyter_handlers;
pub mod jupyter_service;
pub mod metrics;
pub mod projects;
pub mod runs;
pub mod state;
pub mod stats;
pub mod tensorboard_handlers;
pub mod tensorboard_service;

// ─── Helpers ─────────────────────────────────────────────────────────────

fn run_dir(base: &std::path::Path, exp: &str, run: &str) -> PathBuf {
    base.join(exp).join(run)
}

/// Refuse every request that is not a read, when the server is read-only.
///
/// Enforced as middleware rather than per handler: read-only has to hold for
/// routes added later too, and a guard that must be remembered in each new
/// handler is a guard that will eventually be forgotten. Jupyter and TensorBoard
/// start/stop are POSTs, so they are covered by the same rule — which is right,
/// since both spawn processes on the host.
async fn enforce_read_only(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::Method;
    let is_read = matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    if state.read_only && !is_read {
        return (
            axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "error": "read_only",
                "message": "This server is running in read-only mode (exp serve --read-only).",
            })),
        )
            .into_response();
    }
    next.run(request).await
}

fn exp_dir(base: &std::path::Path, exp: &str) -> PathBuf {
    base.join(exp)
}

// ─── Router ──────────────────────────────────────────────────────────────

fn api_router() -> Router<AppState> {
    Router::new()
        .route(
            "/projects",
            get(projects::list_projects).post(projects::create_project),
        )
        .route(
            "/projects/{project}",
            get(projects::get_project)
                .patch(projects::update_project)
                .delete(projects::delete_project),
        )
        .route(
            "/projects/{project}/readme",
            get(projects::get_project_readme).put(projects::update_project_readme),
        )
        .route("/projects/{project}/runs", get(projects::get_project_runs))
        .route("/experiments", get(experiments::list_experiments))
        .route("/experiments/{exp}/runs", get(runs::list_runs))
        .route(
            "/experiments/{exp}/metadata",
            get(experiments::get_experiment_metadata)
                .patch(experiments::update_experiment_metadata),
        )
        .route(
            "/experiments/{exp}/runs/{run}/metrics",
            get(metrics::get_metrics),
        )
        .route(
            "/run/{exp}/{run}/stream/vectors",
            get(metrics::stream_vectors),
        )
        .route(
            "/experiments/{exp}/runs/{run}/log/stream",
            get(metrics::stream_log),
        )
        .route(
            "/experiments/{exp}/runs/{run}/config",
            get(metrics::get_config),
        )
        .route(
            "/experiments/{exp}/runs/{run}/metadata",
            get(runs::get_run_metadata).patch(runs::update_run_metadata),
        )
        .route(
            "/experiments/{exp}/runs/{run}/artifacts",
            get(artifacts::list_artifacts),
        )
        .route(
            "/experiments/{exp}/runs/{run}/artifacts/content",
            get(artifacts::get_artifact_content),
        )
        .route("/experiments/{exp}/stats", get(stats::get_experiment_stats))
        .route("/config", get(stats::get_server_config))
        .route("/stats", get(stats::get_global_stats))
        .route(
            "/jupyter/available",
            get(jupyter_handlers::available_jupyter),
        )
        .route(
            "/experiments/{exp}/runs/{run}/jupyter/start",
            post(jupyter_handlers::start_jupyter),
        )
        .route(
            "/experiments/{exp}/runs/{run}/jupyter/stop",
            post(jupyter_handlers::stop_jupyter),
        )
        .route(
            "/experiments/{exp}/runs/{run}/jupyter/status",
            get(jupyter_handlers::status_jupyter),
        )
        .route(
            "/experiments/{exp}/runs/{run}/jupyter/notebook",
            get(jupyter_handlers::get_jupyter_notebook)
                .post(jupyter_handlers::create_jupyter_notebook),
        )
        .route(
            "/experiments/{exp}/jupyter/start",
            post(jupyter_handlers::start_multi_jupyter),
        )
        .route(
            "/experiments/{exp}/jupyter/stop",
            post(jupyter_handlers::stop_multi_jupyter),
        )
        .route(
            "/experiments/{exp}/jupyter/status",
            get(jupyter_handlers::status_multi_jupyter),
        )
        .route(
            "/experiments/{exp}/jupyter/notebook",
            get(jupyter_handlers::get_multi_jupyter_notebook)
                .post(jupyter_handlers::create_multi_jupyter_notebook),
        )
        .route(
            "/experiments/{exp}/runs/{run}/tensorboard/start",
            post(tensorboard_handlers::start_tensorboard),
        )
        .route(
            "/experiments/{exp}/runs/{run}/tensorboard/stop",
            post(tensorboard_handlers::stop_tensorboard),
        )
        .route(
            "/experiments/{exp}/runs/{run}/tensorboard/status",
            get(tensorboard_handlers::status_tensorboard),
        )
        .route(
            "/experiments/{exp}/runs/{run}/tensorboard/has_logs",
            get(tensorboard_handlers::has_tensorboard_logs),
        )
        .route(
            "/tensorboard/available",
            get(tensorboard_handlers::available_tensorboard),
        )
}

// ─── Public API ──────────────────────────────────────────────────────────

/// Build the Axum router with all routes.
pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // API routes
        .nest("/api", api_router())
        // Frontend: serve embedded static files
        .fallback(frontend::serve_frontend)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            enforce_read_only,
        ))
        .with_state(state)
        .layer(cors)
}

/// Start the server on the given address.
pub async fn serve(config: ServerConfig) -> anyhow::Result<()> {
    let state = AppState::from_config(&config);

    let cancel_token = state.shutdown_token.clone();
    let jupyter_manager = state.jupyter.clone();
    let tensorboard_manager = state.tensorboard.clone();

    let app = build_router(state);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    info!("ExpMan dashboard at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install CTRL+C handler");
        info!("Shutting down ExpMan server...");
        cancel_token.cancel();
    });

    let _ = server.await;
    info!("Server shutting down, cleaning up processes...");
    jupyter_manager.shutdown_all().await;
    tensorboard_manager.shutdown_all().await;

    Ok(())
}
