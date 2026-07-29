//! Project-level API handlers.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;

use crate::core::{dto, storage};

use super::state::AppState;

/// Reject a write to a project that is a generated projection of an external
/// source, since the next sync would silently discard it.
///
/// Accepting such a write is worse than refusing it: the dashboard would report
/// success and the edit would vanish at the next sync with no trace. Read-only
/// mode is enforced separately, as middleware over every mutating verb.
fn reject_generated(state: &AppState, project: &str) -> Option<Response> {
    let meta = storage::load_project_metadata(&state.base_dir, project).ok()?;
    if !meta.generated {
        return None;
    }
    Some(
        (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "generated_project",
                "message": format!(
                    "Project '{project}' is generated from {} and is regenerated on each sync. \
                     Edit the source and re-run `exp project sync` instead.",
                    meta.generated_from.as_deref().unwrap_or("an external source")
                ),
                "generated_from": meta.generated_from,
            })),
        )
            .into_response(),
    )
}

pub async fn list_projects(State(state): State<AppState>) -> impl IntoResponse {
    match storage::list_projects(&state.base_dir) {
        Ok(names) => {
            let mut result: Vec<dto::Project> = vec![];
            // Also count experiments per project
            let all_experiments = storage::list_experiments(&state.base_dir).unwrap_or_default();
            for name in &names {
                let meta =
                    storage::load_project_metadata(&state.base_dir, name).unwrap_or_default();
                // Count experiments assigned to this project
                let exp_count = all_experiments
                    .iter()
                    .filter(|exp_name| {
                        let exp_dir = state.base_dir.join(exp_name);
                        storage::load_experiment_metadata(&exp_dir)
                            .map(|m| m.project.as_deref() == Some(name.as_str()))
                            .unwrap_or(false)
                    })
                    .count();
                result.push(dto::Project::new(name, meta, exp_count));
            }
            Json(result).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
pub struct CreateProject {
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
}

pub async fn create_project(
    State(state): State<AppState>,
    Json(body): Json<CreateProject>,
) -> impl IntoResponse {
    let meta = crate::core::models::ProjectMetadata {
        display_name: body.display_name,
        description: body.description,
        tags: body.tags.unwrap_or_default(),
        created_at: Some(chrono::Utc::now()),
        ..Default::default()
    };
    match storage::save_project_metadata(&state.base_dir, &body.name, &meta) {
        Ok(_) => (
            StatusCode::CREATED,
            Json(dto::Project::new(&body.name, meta, 0)),
        )
            .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn get_project(
    State(state): State<AppState>,
    Path(project): Path<String>,
) -> impl IntoResponse {
    match storage::load_project_metadata(&state.base_dir, &project) {
        Ok(meta) => {
            let readme = storage::load_project_readme(&state.base_dir, &project).unwrap_or(None);
            // List experiments in this project
            let all_experiments = storage::list_experiments(&state.base_dir).unwrap_or_default();
            let project_experiments: Vec<dto::Experiment> = all_experiments
                .iter()
                .filter_map(|exp_name| {
                    let exp_dir = state.base_dir.join(exp_name);
                    let exp_meta = storage::load_experiment_metadata(&exp_dir).ok()?;
                    if exp_meta.project.as_deref() != Some(project.as_str()) {
                        return None;
                    }
                    let runs = storage::list_runs(&exp_dir).unwrap_or_default();
                    Some(dto::Experiment::new(exp_name, exp_meta, runs.len()))
                })
                .collect();

            Json(dto::ProjectDetail::new(
                &project,
                meta,
                readme,
                project_experiments,
            ))
            .into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn get_project_readme(
    State(state): State<AppState>,
    Path(project): Path<String>,
) -> impl IntoResponse {
    match storage::load_project_readme(&state.base_dir, &project) {
        Ok(content) => Json(dto::ReadmeContent {
            content: content.unwrap_or_default(),
        })
        .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn update_project_readme(
    State(state): State<AppState>,
    Path(project): Path<String>,
    Json(body): Json<dto::ReadmeContent>,
) -> impl IntoResponse {
    if let Some(refusal) = reject_generated(&state, &project) {
        return refusal;
    }
    match storage::save_project_readme(&state.base_dir, &project, &body.content) {
        Ok(_) => Json(dto::ReadmeContent {
            content: body.content,
        })
        .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
pub struct ProjectUpdate {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
}

pub async fn update_project(
    State(state): State<AppState>,
    Path(project): Path<String>,
    Json(update): Json<ProjectUpdate>,
) -> impl IntoResponse {
    if let Some(refusal) = reject_generated(&state, &project) {
        return refusal;
    }
    let mut meta = storage::load_project_metadata(&state.base_dir, &project).unwrap_or_default();
    if let Some(dn) = update.display_name {
        meta.display_name = Some(dn);
    }
    if let Some(desc) = update.description {
        meta.description = Some(desc);
    }
    if let Some(tags) = update.tags {
        meta.tags = tags;
    }
    let experiments_count = storage::list_project_experiments(&state.base_dir, &project)
        .map(|e| e.len())
        .unwrap_or(0);
    match storage::save_project_metadata(&state.base_dir, &project, &meta) {
        Ok(_) => Json(dto::Project::new(&project, meta, experiments_count)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn delete_project(
    State(state): State<AppState>,
    Path(project): Path<String>,
) -> impl IntoResponse {
    if let Some(refusal) = reject_generated(&state, &project) {
        return refusal;
    }
    match storage::delete_project(&state.base_dir, &project) {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ─── Project-scoped aggregation ───────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RunsQuery {
    /// Tag expression, e.g. `arm:tiered AND (study:1 OR study:2)`.
    pub tags: Option<String>,
    pub status: Option<String>,
    pub experiment: Option<String>,
    /// Only runs in this group (a DDP job or a sweep cohort).
    pub group: Option<String>,
}

/// Every run in a project, across its experiments, with tag facets.
///
/// `GET /projects/{p}` returns only an experiment list, which is not a view you
/// can work in: the thesis hierarchy puts the interesting comparison *across*
/// experiments. This returns the flat runs table plus the facet counts needed to
/// build tag filters, and the union of metric names so a caller knows what is
/// comparable before fetching any series.
pub async fn get_project_runs(
    State(state): State<AppState>,
    Path(project): Path<String>,
    Query(params): Query<RunsQuery>,
) -> impl IntoResponse {
    let status = match params.status.as_deref() {
        None => None,
        Some(s) => match s.to_ascii_uppercase().as_str() {
            "RUNNING" => Some(crate::core::models::RunStatus::Running),
            "FINISHED" => Some(crate::core::models::RunStatus::Finished),
            "FAILED" => Some(crate::core::models::RunStatus::Failed),
            "CRASHED" => Some(crate::core::models::RunStatus::Crashed),
            other => {
                return (StatusCode::BAD_REQUEST, format!("Unknown status {other:?}"))
                    .into_response()
            }
        },
    };

    let query = storage::RunQuery {
        project: Some(project.clone()),
        experiment: params.experiment,
        group: params.group,
        status,
        tags: params
            .tags
            .as_deref()
            .map(storage::parse_tag_expr)
            .unwrap_or_default(),
    };

    let runs = match storage::query_runs(&state.base_dir, &query) {
        Ok(r) => r,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    // Facets are counted over the *returned* runs, so they narrow as filters are
    // applied — the behaviour a filter UI needs to avoid offering empty results.
    let mut facets = dto::RunFacets::default();
    let mut metrics: std::collections::BTreeSet<String> = Default::default();
    let mut typed: Vec<dto::Run> = Vec::with_capacity(runs.len());

    for run in runs {
        for tag in &run.tags {
            *facets.tags.entry(tag.clone()).or_default() += 1;
        }
        *facets.status.entry(run.status.clone()).or_default() += 1;
        *facets
            .experiments
            .entry(run.experiment.clone())
            .or_default() += 1;
        if let Some(group) = &run.group {
            *facets.groups.entry(group.clone()).or_default() += 1;
        }
        metrics.extend(run.scalars.keys().cloned());
        metrics.extend(run.vectors.keys().cloned());

        // RunEntry is the storage shape; dto::Run is the wire shape. Convert
        // explicitly rather than serializing the storage type by accident.
        typed.push(dto::Run {
            id: run.run.clone(),
            name: run.run,
            experiment: run.experiment,
            status: match run.status.as_str() {
                "RUNNING" => crate::core::models::RunStatus::Running,
                "FINISHED" => crate::core::models::RunStatus::Finished,
                "FAILED" => crate::core::models::RunStatus::Failed,
                _ => crate::core::models::RunStatus::Crashed,
            },
            group: run.group,
            rank: run.rank,
            started_at: run.started_at,
            finished_at: run.finished_at,
            heartbeat_at: run.heartbeat_at,
            duration_secs: run.duration_secs,
            description: run.description,
            tags: Some(run.tags),
            scalars: Some(run.scalars),
            vectors: Some(run.vectors),
            language: None,
            env_path: None,
        });
    }

    Json(dto::ProjectRuns {
        project,
        total: typed.len(),
        runs: typed,
        facets,
        metrics: metrics.into_iter().collect(),
    })
    .into_response()
}
