//! TensorBoard endpoint handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};

use super::run_dir;
use super::state::AppState;

/// Checks if `tensorboard` is available in the environment.
pub async fn available_tensorboard() -> impl IntoResponse {
    let available = super::tensorboard_service::TensorBoardManager::detect_tensorboard().await;
    Json(serde_json::json!({ "available": available }))
}

/// Checks if there are TensorBoard logs for a specific run.
pub async fn has_tensorboard_logs(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
) -> impl IntoResponse {
    let dir = run_dir(&state.base_dir, &exp, &run);
    let has_logs = super::tensorboard_service::TensorBoardManager::has_logs(&dir).await;
    Json(serde_json::json!({ "has_logs": has_logs }))
}

/// Spawn TensorBoard for a specific run.
pub async fn start_tensorboard(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
) -> impl IntoResponse {
    let dir = run_dir(&state.base_dir, &exp, &run);

    match state.tensorboard.spawn(&exp, &run, dir).await {
        Ok(port) => Json(serde_json::json!({ "port": port })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

/// Stop a running TensorBoard for a specific run.
pub async fn stop_tensorboard(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
) -> impl IntoResponse {
    match state.tensorboard.stop(&exp, &run).await {
        Ok(()) => Json(serde_json::json!({ "stopped": true })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

/// Get the status of a per-run TensorBoard.
pub async fn status_tensorboard(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Some(port) = state.tensorboard.status(&exp, &run) {
        Json(serde_json::json!({ "running": true, "port": port }))
    } else {
        Json(serde_json::json!({ "running": false, "port": null }))
    }
}
