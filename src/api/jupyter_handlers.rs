//! Jupyter notebook endpoint handlers (the API layer).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};

use crate::core::storage;

use super::state::AppState;

use super::{exp_dir, run_dir};

/// Returns the best available interactive backend.
pub async fn available_jupyter() -> impl IntoResponse {
    let backend = super::jupyter_service::detect_backend().await;
    Json(serde_json::json!({ "backend": backend }))
}

/// Spawn a Jupyter Notebook for a specific run.
pub async fn start_jupyter(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
) -> impl IntoResponse {
    let dir = run_dir(&state.base_dir, &exp, &run);

    let is_python = match storage::load_run_metadata(&dir) {
        Ok(meta) => {
            meta.language
                .unwrap_or_else(|| "python".to_string())
                .to_lowercase()
                != "rust"
        }
        Err(_) => true,
    };

    match state.jupyter.spawn(&exp, &run, dir, is_python).await {
        Ok(port) => Json(serde_json::json!({ "port": port })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

/// Stop a running Jupyter Notebook for a specific run.
pub async fn stop_jupyter(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
) -> impl IntoResponse {
    match state.jupyter.stop(&exp, &run).await {
        Ok(()) => Json(serde_json::json!({ "stopped": true })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

/// Get the status of a per-run Jupyter Notebook.
pub async fn status_jupyter(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Some(port) = state.jupyter.status(&exp, &run) {
        Json(serde_json::json!({ "running": true, "port": port }))
    } else {
        Json(serde_json::json!({ "running": false, "port": null }))
    }
}

/// Endpoint to check if `interactive.ipynb` exists and return its content.
pub async fn get_jupyter_notebook(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
) -> impl IntoResponse {
    let notebook_path = run_dir(&state.base_dir, &exp, &run).join("interactive.ipynb");
    if notebook_path.exists() {
        match tokio::fs::read_to_string(&notebook_path).await {
            Ok(content) => {
                Json(serde_json::json!({ "exists": true, "content": content })).into_response()
            }
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        Json(serde_json::json!({ "exists": false, "content": null })).into_response()
    }
}

/// Endpoint to create the default `interactive.ipynb` for a run.
pub async fn create_jupyter_notebook(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
) -> impl IntoResponse {
    let dir = run_dir(&state.base_dir, &exp, &run);

    let is_python = match storage::load_run_metadata(&dir) {
        Ok(meta) => {
            meta.language
                .unwrap_or_else(|| "python".to_string())
                .to_lowercase()
                != "rust"
        }
        Err(_) => true, // Default to Python
    };

    match super::jupyter_service::generate_notebook(&dir, is_python).await {
        Ok(true) => {
            // Read back the created content
            let content = tokio::fs::read_to_string(dir.join("interactive.ipynb"))
                .await
                .unwrap_or_default();
            Json(serde_json::json!({ "created": true, "content": content })).into_response()
        }
        Ok(false) => (StatusCode::CONFLICT, "Notebook already exists").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct MultiRunJupyterPayload {
    pub runs: Vec<String>,
}

/// Spawn a multi-run Jupyter Notebook for the experiment.
pub async fn start_multi_jupyter(
    State(state): State<AppState>,
    Path(exp): Path<String>,
    Json(payload): Json<MultiRunJupyterPayload>,
) -> impl IntoResponse {
    let dir = exp_dir(&state.base_dir, &exp);

    let is_python = if let Some(first_run) = payload.runs.first() {
        let r_dir = run_dir(&state.base_dir, &exp, first_run);
        match storage::load_run_metadata(&r_dir) {
            Ok(meta) => {
                meta.language
                    .unwrap_or_else(|| "python".to_string())
                    .to_lowercase()
                    != "rust"
            }
            Err(_) => true,
        }
    } else {
        true
    };

    match state
        .jupyter
        .spawn_multi(&exp, dir, is_python, &payload.runs)
        .await
    {
        Ok(port) => Json(serde_json::json!({ "port": port })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

/// Stop a running multi-run Jupyter Notebook.
pub async fn stop_multi_jupyter(
    State(state): State<AppState>,
    Path(exp): Path<String>,
) -> impl IntoResponse {
    match state.jupyter.stop(&exp, "__multi__").await {
        Ok(()) => Json(serde_json::json!({ "stopped": true })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

/// Get the status of a multi-run Jupyter Notebook.
pub async fn status_multi_jupyter(
    State(state): State<AppState>,
    Path(exp): Path<String>,
) -> impl IntoResponse {
    if let Some(port) = state.jupyter.status(&exp, "__multi__") {
        Json(serde_json::json!({ "running": true, "port": port }))
    } else {
        Json(serde_json::json!({ "running": false, "port": null }))
    }
}

/// Endpoint to check if `interactive.ipynb` exists in the experiment directory and return its content.
pub async fn get_multi_jupyter_notebook(
    State(state): State<AppState>,
    Path(exp): Path<String>,
) -> impl IntoResponse {
    let notebook_path = exp_dir(&state.base_dir, &exp).join("interactive.ipynb");
    if notebook_path.exists() {
        match tokio::fs::read_to_string(&notebook_path).await {
            Ok(content) => {
                Json(serde_json::json!({ "exists": true, "content": content })).into_response()
            }
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        Json(serde_json::json!({ "exists": false, "content": null })).into_response()
    }
}

/// Endpoint to create the multi-run `interactive.ipynb`.
pub async fn create_multi_jupyter_notebook(
    State(state): State<AppState>,
    Path(exp): Path<String>,
    Json(payload): Json<MultiRunJupyterPayload>,
) -> impl IntoResponse {
    let dir = exp_dir(&state.base_dir, &exp);

    let is_python = if let Some(first_run) = payload.runs.first() {
        let r_dir = run_dir(&state.base_dir, &exp, first_run);
        match storage::load_run_metadata(&r_dir) {
            Ok(meta) => {
                meta.language
                    .unwrap_or_else(|| "python".to_string())
                    .to_lowercase()
                    != "rust"
            }
            Err(_) => true,
        }
    } else {
        true
    };

    match super::jupyter_service::generate_multi_run_notebook(&dir, is_python, &payload.runs).await
    {
        Ok(true) => {
            let content = tokio::fs::read_to_string(dir.join("interactive.ipynb"))
                .await
                .unwrap_or_default();
            Json(serde_json::json!({ "created": true, "content": content })).into_response()
        }
        Ok(false) => (StatusCode::CONFLICT, "Notebook already exists").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}
