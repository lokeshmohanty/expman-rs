//! Stats and server configuration handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};

use expman::storage;

use super::state::AppState;

use super::{exp_dir, run_dir};

pub async fn get_experiment_stats(
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
        let meta =
            storage::load_run_metadata(&dir).unwrap_or_else(|_| expman::models::RunMetadata {
                name: run_name.clone(),
                experiment: exp.clone(),
                status: expman::models::RunStatus::Crashed,
                started_at: chrono::Utc::now(),
                ..Default::default()
            });

        let last_metrics =
            storage::read_latest_scalar_metrics(&dir.join("vectors.parquet")).unwrap_or_default();

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

pub async fn get_global_stats(State(state): State<AppState>) -> impl IntoResponse {
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
                if meta.status == expman::models::RunStatus::Running {
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

pub async fn get_server_config() -> impl IntoResponse {
    Json(serde_json::json!({"live_mode": true, "version": env!("CARGO_PKG_VERSION")}))
}
