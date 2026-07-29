//! Metrics, vector streaming, and log streaming handlers.

use std::collections::HashMap;
use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Sse},
    Json,
};
use futures_util::StreamExt as _;
use serde::Deserialize;
use tokio_stream::wrappers::IntervalStream;

use crate::core::storage;

use super::state::AppState;

use super::run_dir;

/// How many points a chart gets by default.
///
/// Comfortably more than a wide chart has pixels, so downsampling is invisible,
/// while bounding the JSON payload regardless of run length.
const DEFAULT_MAX_POINTS: usize = 2000;

#[derive(Deserialize)]
pub struct MetricsQuery {
    pub since_step: Option<u64>,
    /// Cap on returned rows. Defaults to 2000; `0` means no cap.
    pub max_points: Option<usize>,
    /// Return every row. The escape hatch for export and analysis, where the
    /// exact series matters and the caller is not a browser.
    #[serde(default)]
    pub full: bool,
}

pub async fn get_metrics(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
    Query(q): Query<MetricsQuery>,
) -> impl IntoResponse {
    let dir = run_dir(&state.base_dir, &exp, &run);
    match storage::read_run_vectors_since(&dir, q.since_step) {
        Ok(rows) => {
            // A million-step run serialises to hundreds of MB of JSON and hangs
            // the tab before it draws. Bound it unless the caller says otherwise.
            let limit = if q.full {
                0
            } else {
                q.max_points.unwrap_or(DEFAULT_MAX_POINTS)
            };
            let rows = if limit == 0 {
                rows
            } else {
                storage::downsample_rows(rows, limit)
            };
            Json::<Vec<HashMap<String, serde_json::Value>>>(rows).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// SSE endpoint: streams new vector metrics from vectors.parquet every 500ms.
pub async fn stream_vectors(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
) -> Sse<impl tokio_stream::Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let dir = run_dir(&state.base_dir, &exp, &run);
    let mut last_step: Option<u64> = None;

    let interval = tokio::time::interval(Duration::from_millis(500));
    let shutdown = state.shutdown_token.clone();
    let stream = IntervalStream::new(interval)
        .take_until(async move { shutdown.cancelled().await })
        .map(move |_| {
            let rows: Vec<HashMap<String, serde_json::Value>> =
                storage::read_run_vectors_since(&dir, last_step).unwrap_or_default();
            if let Some(row) = rows.last() {
                let row: &HashMap<String, serde_json::Value> = row;
                if let Some(step) = row.get("step").and_then(|v: &serde_json::Value| v.as_u64()) {
                    last_step = Some(last_step.map_or(step, |ls| ls.max(step)));
                }
            }
            let data: String = serde_json::to_string(&rows).unwrap_or_default();
            Ok(axum::response::sse::Event::default().data(data))
        });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

#[derive(Deserialize)]
pub struct LogQuery {
    pub file: Option<String>,
}

/// SSE endpoint: streams new lines from a log file (default: run.log) every 500ms.
pub async fn stream_log(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
    Query(q): Query<LogQuery>,
) -> Sse<impl tokio_stream::Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let filename = q.file.unwrap_or_else(|| "run.log".to_string());
    let path = run_dir(&state.base_dir, &exp, &run).join(filename);
    let mut last_pos: u64 = 0;

    let interval = tokio::time::interval(Duration::from_millis(500));
    let shutdown = state.shutdown_token.clone();
    let stream = IntervalStream::new(interval)
        .take_until(async move { shutdown.cancelled().await })
        .map(move |_| {
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

pub async fn get_config(
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
