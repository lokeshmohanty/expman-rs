//! Artifact listing and content handlers.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;

use crate::core::storage;

use super::state::AppState;

use super::run_dir;

pub async fn list_artifacts(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
) -> impl IntoResponse {
    let dir = run_dir(&state.base_dir, &exp, &run);
    match storage::list_artifacts(&dir) {
        Ok(artifacts) => Json::<Vec<storage::ArtifactInfo>>(artifacts).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
pub struct ArtifactQuery {
    pub path: String,
}

pub async fn get_artifact_content(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
    Query(q): Query<ArtifactQuery>,
) -> impl IntoResponse {
    let dir = run_dir(&state.base_dir, &exp, &run);

    let file_path = if dir.join("artifacts").join(&q.path).exists() {
        dir.join("artifacts").join(&q.path)
    } else {
        dir.join(&q.path)
    };

    // Security: prevent path traversal
    let canonical_run_dir = match dir.canonicalize() {
        Ok(p) => p,
        Err(_) => return (StatusCode::NOT_FOUND, "Run directory not found").into_response(),
    };
    let canonical_file = match file_path.canonicalize() {
        Ok(p) => p,
        Err(_) => return (StatusCode::NOT_FOUND, "File not found").into_response(),
    };
    if !canonical_file.starts_with(&canonical_run_dir) {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    if !canonical_file.exists() {
        return (StatusCode::NOT_FOUND, "File not found").into_response();
    }

    let ext = canonical_file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext == "parquet" {
        let rows = storage::read_vectors(&canonical_file).unwrap_or_default();
        let preview: Vec<_> = rows.into_iter().take(100).collect();
        return Json(serde_json::json!({"type": "parquet", "data": preview})).into_response();
    }

    let content_type = match ext.as_str() {
        "png" | "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "mp4" => "video/mp4",
        "json" => "application/json",
        "yaml" | "yml" => "text/yaml",
        "txt" | "log" => "text/plain",
        "csv" => "text/csv",
        _ => "application/octet-stream",
    };

    match tokio::fs::read(&canonical_file).await {
        Ok(bytes) => Response::builder()
            .header(header::CONTENT_TYPE, content_type)
            .body(Body::from(bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
