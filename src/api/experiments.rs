//! Experiment-level API handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use crate::core::storage;

use super::state::AppState;

use super::exp_dir;

pub async fn list_experiments(State(state): State<AppState>) -> impl IntoResponse {
    match storage::list_experiments(&state.base_dir) {
        Ok(names) => {
            let mut result = vec![];
            for name in &names {
                let exp_dir = exp_dir(&state.base_dir, name);
                let runs = storage::list_runs(&exp_dir).unwrap_or_else(|_| vec![]);
                let meta = storage::load_experiment_metadata(&exp_dir).unwrap_or_default();
                result.push(serde_json::json!({
                    "id": name,
                    "display_name": meta.display_name.unwrap_or_else(|| name.to_string()),
                    "description": meta.description,
                    "tags": meta.tags,
                    "runs_count": runs.len(),
                }));
            }
            Json(result).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn get_experiment_metadata(
    State(state): State<AppState>,
    Path(exp): Path<String>,
) -> impl IntoResponse {
    let dir = exp_dir(&state.base_dir, &exp);
    match storage::load_experiment_metadata(&dir) {
        Ok(meta) => Json::<crate::core::models::ExperimentMetadata>(meta).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
pub struct MetadataUpdate {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
}

pub async fn update_experiment_metadata(
    State(state): State<AppState>,
    Path(exp): Path<String>,
    Json(update): Json<MetadataUpdate>,
) -> impl IntoResponse {
    let dir = exp_dir(&state.base_dir, &exp);
    let mut meta = storage::load_experiment_metadata(&dir).unwrap_or_default();
    if let Some(dn) = update.display_name {
        meta.display_name = Some(dn);
    }
    if let Some(desc) = update.description {
        meta.description = Some(desc);
    }
    if let Some(tags) = update.tags {
        meta.tags = tags;
    }
    match storage::save_experiment_metadata(&dir, &meta) {
        Ok(_) => Json(meta).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
