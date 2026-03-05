//! Run-level API handlers.

use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use expman::storage;

use super::state::AppState;

use super::{exp_dir, run_dir};

/// Query params for `list_runs`.
#[derive(Deserialize, Default)]
pub struct ListRunsQuery {
    /// Comma-separated list of metric keys to include. If omitted, all scalars are returned.
    pub metrics: Option<String>,
}

pub async fn list_runs(
    State(state): State<AppState>,
    Path(exp): Path<String>,
    Query(q): Query<ListRunsQuery>,
) -> impl IntoResponse {
    // Parse optional metric filter
    let metric_filter: Option<std::collections::HashSet<String>> = q.metrics.map(|s| {
        s.split(',')
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .collect()
    });

    let exp_dir = exp_dir(&state.base_dir, &exp);
    match storage::list_runs(&exp_dir) {
        Ok(run_names) => {
            let mut result = vec![];
            for name in run_names {
                let dir = run_dir(&state.base_dir, &exp, &name);
                let mut meta = storage::load_run_metadata(&dir).unwrap_or_else(|_| {
                    expman::models::RunMetadata {
                        name: name.clone(),
                        experiment: exp.clone(),
                        status: expman::models::RunStatus::Crashed,
                        started_at: chrono::Utc::now(),
                        ..Default::default()
                    }
                });

                // Attach latest vectors, backfilled from parquet if missing in metadata
                let vectors_path = dir.join("vectors.parquet");
                if meta.vectors.is_none() || meta.vectors.as_ref().unwrap().is_empty() {
                    if let Ok(scalars) = storage::read_latest_scalar_metrics(&vectors_path) {
                        if !scalars.is_empty() {
                            let converted: HashMap<String, expman::models::MetricValue> = scalars
                                .into_iter()
                                .map(|(k, v)| (k, expman::models::MetricValue::Float(v)))
                                .collect();
                            meta.vectors = Some(converted);
                        }
                    }
                }

                if let Some(scalars) = &mut meta.scalars {
                    if let Some(keys) = &metric_filter {
                        scalars.retain(|k, _| keys.contains(k));
                    }
                }

                let mut json = serde_json::to_value(meta).unwrap();
                json.as_object_mut()
                    .unwrap()
                    .insert("id".to_string(), serde_json::json!(name));
                result.push(json);
            }
            Json(result).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn get_run_metadata(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
) -> impl IntoResponse {
    let dir = run_dir(&state.base_dir, &exp, &run);
    match storage::load_run_metadata(&dir) {
        Ok(mut meta) => {
            let vectors_path = dir.join("vectors.parquet");
            if let Ok(scalars_map) = storage::read_latest_scalar_metrics(&vectors_path) {
                if !scalars_map.is_empty() {
                    let converted: HashMap<String, expman::models::MetricValue> = scalars_map
                        .into_iter()
                        .map(|(k, v)| (k, expman::models::MetricValue::Float(v)))
                        .collect();
                    meta.vectors = Some(converted);
                }
            }
            Json(meta).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
pub struct RunMetadataUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
}

pub async fn update_run_metadata(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
    Json(update): Json<RunMetadataUpdate>,
) -> impl IntoResponse {
    let dir = run_dir(&state.base_dir, &exp, &run);
    match storage::load_run_metadata(&dir) {
        Ok(mut meta) => {
            if let Some(n) = update.name {
                meta.name = n;
            }
            if let Some(desc) = update.description {
                meta.description = Some(desc);
            }
            if let Some(tags) = update.tags {
                meta.tags = Some(tags);
            }
            match storage::save_run_metadata(&dir, &meta) {
                Ok(_) => Json(meta).into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
