//! Stats and server configuration handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};

use crate::core::{dto, storage};

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
            storage::load_run_metadata(&dir).unwrap_or_else(|_| crate::core::models::RunMetadata {
                name: run_name.to_string(),
                experiment: exp.to_string(),
                status: crate::core::models::RunStatus::Crashed,
                started_at: chrono::Utc::now(),
                ..Default::default()
            });

        let last_metrics = storage::read_run_latest_scalars(&dir).unwrap_or_default();

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
    let now = chrono::Utc::now();
    let mut total_runs = 0;
    let mut active_runs = 0;
    let mut stale_runs = 0;

    for exp in &experiments {
        let exp_dir = exp_dir(&state.base_dir, exp);
        let runs = storage::list_runs(&exp_dir).unwrap_or_else(|_| vec![]);
        total_runs += runs.len();

        for run in runs.iter() {
            let dir = run_dir(&state.base_dir, exp, run.as_str());
            if let Ok(meta) = storage::load_run_metadata_cached(&dir) {
                if meta.status == crate::core::models::RunStatus::Running {
                    // A hard-killed run stays RUNNING forever. Splitting the
                    // count keeps active_runs honest instead of letting dead
                    // runs accumulate in it silently.
                    if storage::looks_alive(&meta, now) {
                        active_runs += 1;
                    } else {
                        stale_runs += 1;
                    }
                }
            }
        }
    }

    Json(dto::GlobalStats {
        total_experiments: experiments.len(),
        total_projects: storage::list_projects(&state.base_dir)
            .unwrap_or_default()
            .len(),
        total_runs,
        active_runs,
        stale_runs,
        total_storage_bytes: 0,
    })
}

pub async fn get_server_config(State(state): State<AppState>) -> impl IntoResponse {
    Json(dto::ServerConfig {
        live_mode: state.live_mode,
        read_only: state.read_only,
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}
