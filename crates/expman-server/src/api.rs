//! REST API handlers and SSE streaming for expman-server.

use std::convert::Infallible;
use std::path::PathBuf;
use std::time::Duration;

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response, Sse},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use tokio_stream::wrappers::IntervalStream;
use tokio_stream::StreamExt;

use expman_core::storage;

use crate::state::AppState;

// ─── Router ──────────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/experiments", get(list_experiments))
        .route("/experiments/:exp/runs", get(list_runs))
        .route("/experiments/:exp/metadata", get(get_experiment_metadata).patch(update_experiment_metadata))
        .route("/experiments/:exp/runs/:run/metrics", get(get_metrics))
        .route("/experiments/:exp/runs/:run/metrics/stream", get(stream_metrics))
        .route("/experiments/:exp/runs/:run/config", get(get_config))
        .route("/experiments/:exp/runs/:run/metadata", get(get_run_metadata).patch(update_run_metadata))
        .route("/experiments/:exp/runs/:run/artifacts", get(list_artifacts))
        .route("/experiments/:exp/runs/:run/artifacts/content", get(get_artifact_content))
        .route("/experiments/:exp/runs/:run/log/stream", get(stream_log))
        .route("/experiments/:exp/stats", get(get_experiment_stats))
        .route("/config", get(get_server_config))
        .route("/stats", get(get_global_stats))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn run_dir(base: &std::path::Path, exp: &str, run: &str) -> PathBuf {
    base.join(exp).join(run)
}

fn exp_dir(base: &std::path::Path, exp: &str) -> PathBuf {
    base.join(exp)
}

// ─── Handlers ────────────────────────────────────────────────────────────────

async fn list_experiments(State(state): State<AppState>) -> impl IntoResponse {
    match storage::list_experiments(&state.base_dir) {
        Ok(names) => {
            let mut result = vec![];
            for name in names {
                let exp_dir = exp_dir(&state.base_dir, &name);
                let runs = storage::list_runs(&exp_dir).unwrap_or_default();
                let meta = storage::load_experiment_metadata(&exp_dir).unwrap_or_default();
                result.push(serde_json::json!({
                    "id": name,
                    "display_name": meta.display_name.unwrap_or_else(|| name.clone()),
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

/// Query params for `list_runs`.
#[derive(Deserialize, Default)]
struct ListRunsQuery {
    /// Comma-separated list of metric keys to include. If omitted, all scalars are returned.
    metrics: Option<String>,
}

async fn list_runs(
    State(state): State<AppState>,
    Path(exp): Path<String>,
    Query(q): Query<ListRunsQuery>,
) -> impl IntoResponse {
    // Parse optional metric filter
    let metric_filter: Option<std::collections::HashSet<String>> = q.metrics.map(|s| {
        s.split(',').map(|k| k.trim().to_string()).filter(|k| !k.is_empty()).collect()
    });

    let exp_dir = exp_dir(&state.base_dir, &exp);
    match storage::list_runs(&exp_dir) {
        Ok(run_names) => {
            let mut result = vec![];
            for name in run_names {
                let dir = run_dir(&state.base_dir, &exp, &name);
                let mut meta = storage::load_run_metadata(&dir).unwrap_or_else(|_| {
                    expman_core::models::RunMetadata {
                        name: name.clone(),
                        experiment: exp.clone(),
                        status: expman_core::models::RunStatus::Crashed,
                        started_at: chrono::Utc::now(),
                        ..Default::default()
                    }
                });

                // Attach latest scalar metrics, filtered if requested
                let metrics_path = dir.join("metrics.parquet");
                if let Ok(scalars) = storage::read_latest_scalar_metrics(&metrics_path) {
                    if !scalars.is_empty() {
                        let filtered = match &metric_filter {
                            Some(keys) => scalars.into_iter().filter(|(k, _)| keys.contains(k)).collect(),
                            None => scalars,
                        };
                        meta.metrics = Some(filtered);
                    }
                }

                result.push(meta);
            }
            Json(result).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn get_experiment_metadata(
    State(state): State<AppState>,
    Path(exp): Path<String>,
) -> impl IntoResponse {
    let dir = exp_dir(&state.base_dir, &exp);
    match storage::load_experiment_metadata(&dir) {
        Ok(meta) => Json(meta).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct MetadataUpdate {
    display_name: Option<String>,
    description: Option<String>,
    tags: Option<Vec<String>>,
}

async fn update_experiment_metadata(
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

#[derive(Deserialize)]
struct RunMetadataUpdate {
    name: Option<String>,
    description: Option<String>,
}

async fn update_run_metadata(
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
            match storage::save_run_metadata(&dir, &meta) {
                Ok(_) => Json(meta).into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct MetricsQuery {
    since_step: Option<u64>,
}

async fn get_metrics(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
    Query(q): Query<MetricsQuery>,
) -> impl IntoResponse {
    let dir = run_dir(&state.base_dir, &exp, &run);
    let path = dir.join("metrics.parquet");
    match storage::read_metrics_since(&path, q.since_step) {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// SSE endpoint: streams new metric rows every 500ms.
async fn stream_metrics(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
) -> Sse<impl tokio_stream::Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let path = run_dir(&state.base_dir, &exp, &run).join("metrics.parquet");
    let mut last_step: Option<u64> = None;

    let interval = tokio::time::interval(Duration::from_millis(500));
    let stream = IntervalStream::new(interval).map(move |_| {
        let rows = storage::read_metrics_since(&path, last_step).unwrap_or_default();
        for row in &rows {
            if let Some(step) = row.get("step").and_then(|v| v.as_u64()) {
                last_step = Some(last_step.map_or(step, |ls| ls.max(step)));
            }
        }
        let data = serde_json::to_string(&rows).unwrap_or_default();
        Ok(axum::response::sse::Event::default().data(data))
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// SSE endpoint: streams new lines from run.log every 500ms.
async fn stream_log(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
) -> Sse<impl tokio_stream::Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let path = run_dir(&state.base_dir, &exp, &run).join("run.log");
    let mut last_pos: u64 = 0;

    let interval = tokio::time::interval(Duration::from_millis(500));
    let stream = IntervalStream::new(interval).map(move |_| {
        let mut data = String::new();
        if let Ok(file) = std::fs::File::open(&path) {
            use std::io::{Read, Seek, SeekFrom};
            let mut reader = std::io::BufReader::new(file);
            let metadata = std::fs::metadata(&path).unwrap();
            let len = metadata.len();

            if len < last_pos {
                last_pos = 0;
            }
            if len > last_pos {
                let _ = reader.seek(SeekFrom::Start(last_pos));
                let _ = reader.read_to_string(&mut data);
                last_pos = len;
            }
        }
        Ok(axum::response::sse::Event::default().data(data))
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

async fn get_config(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
) -> impl IntoResponse {
    let dir = run_dir(&state.base_dir, &exp, &run);
    let path = dir.join("config.yaml");
    match storage::load_yaml_value(&path) {
        Ok(val) => match serde_json::to_value(&val) {
            Ok(json_val) => Json(json_val).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn get_run_metadata(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
) -> impl IntoResponse {
    let dir = run_dir(&state.base_dir, &exp, &run);
    match storage::load_run_metadata(&dir) {
        Ok(mut meta) => {
            let metrics_path = dir.join("metrics.parquet");
            if let Ok(scalars) = storage::read_latest_scalar_metrics(&metrics_path) {
                if !scalars.is_empty() {
                    meta.metrics = Some(scalars);
                }
            }
            Json(meta).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn list_artifacts(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
) -> impl IntoResponse {
    let dir = run_dir(&state.base_dir, &exp, &run);
    match storage::list_artifacts(&dir) {
        Ok(artifacts) => Json(artifacts).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct ArtifactQuery {
    path: String,
}

async fn get_artifact_content(
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
        let rows = storage::read_metrics(&canonical_file).unwrap_or_default();
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

async fn get_experiment_stats(
    State(state): State<AppState>,
    Path(exp): Path<String>,
) -> impl IntoResponse {
    let exp_dir = exp_dir(&state.base_dir, &exp);
    let runs = match storage::list_runs(&exp_dir) {
        Ok(r) => r,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let mut stats = vec![];
    for run_name in &runs {
        let dir = run_dir(&state.base_dir, &exp, run_name);
        let meta = storage::load_run_metadata(&dir).unwrap_or_else(|_| {
            expman_core::models::RunMetadata {
                name: run_name.clone(),
                experiment: exp.clone(),
                status: expman_core::models::RunStatus::Crashed,
                started_at: chrono::Utc::now(),
                ..Default::default()
            }
        });

        let last_metrics = storage::read_latest_scalar_metrics(&dir.join("metrics.parquet"))
            .unwrap_or_default();

        stats.push(serde_json::json!({
            "run": run_name,
            "status": meta.status.to_string(),
            "started_at": meta.started_at,
            "finished_at": meta.finished_at,
            "duration_secs": meta.duration_secs,
            "last_metrics": last_metrics,
        }));
    }

    Json(stats).into_response()
}

async fn get_global_stats(State(state): State<AppState>) -> impl IntoResponse {
    let experiments = storage::list_experiments(&state.base_dir).unwrap_or_default();
    let mut total_runs = 0;
    let mut active_runs = 0;

    for exp in &experiments {
        let exp_dir = exp_dir(&state.base_dir, exp);
        let runs = storage::list_runs(&exp_dir).unwrap_or_default();
        total_runs += runs.len();

        for run in runs {
            let dir = run_dir(&state.base_dir, exp, &run);
            if let Ok(meta) = storage::load_run_metadata(&dir) {
                if meta.status == expman_core::models::RunStatus::Running {
                    active_runs += 1;
                }
            }
        }
    }

    Json(serde_json::json!({
        "total_experiments": experiments.len(),
        "total_runs": total_runs,
        "active_runs": active_runs,
        "total_storage_bytes": 0,
    }))
}

async fn get_server_config() -> impl IntoResponse {
    Json(serde_json::json!({"live_mode": true, "version": env!("CARGO_PKG_VERSION")}))
}

// ─── Frontend (embedded) ─────────────────────────────────────────────────────

/// Serve the embedded frontend HTML/JS/CSS.
pub async fn serve_frontend(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    let (actual_path, content) = match Assets::get(path) {
        Some(content) => (path, content),
        None => match Assets::get("index.html") {
            Some(content) => ("index.html", content),
            None => return StatusCode::NOT_FOUND.into_response(),
        },
    };

    let mime = mime_guess::from_path(actual_path).first_or_octet_stream();

    Response::builder()
        .header(header::CONTENT_TYPE, mime.as_ref())
        .body(Body::from(content.data.into_owned()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[derive(rust_embed::Embed)]
#[folder = "../../frontend/dist"]
#[include = "*.html"]
#[include = "*.js"]
#[include = "*.css"]
#[include = "*.wasm"]
struct Assets;
