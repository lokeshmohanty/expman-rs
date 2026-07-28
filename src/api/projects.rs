//! Project-level API handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use crate::core::storage;

use super::state::AppState;

pub async fn list_projects(State(state): State<AppState>) -> impl IntoResponse {
    match storage::list_projects(&state.base_dir) {
        Ok(names) => {
            let mut result = vec![];
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
                result.push(serde_json::json!({
                    "id": name,
                    "display_name": meta.display_name.unwrap_or_else(|| name.to_string()),
                    "description": meta.description,
                    "tags": meta.tags,
                    "experiments_count": exp_count,
                    "created_at": meta.created_at,
                }));
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
    };
    match storage::save_project_metadata(&state.base_dir, &body.name, &meta) {
        Ok(_) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "id": body.name,
                "display_name": meta.display_name.unwrap_or_else(|| body.name.clone()),
                "description": meta.description,
                "tags": meta.tags,
                "experiments_count": 0,
                "created_at": meta.created_at,
            })),
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
            let project_experiments: Vec<serde_json::Value> = all_experiments.iter().filter_map(|exp_name| {
                let exp_dir = state.base_dir.join(exp_name);
                let exp_meta = storage::load_experiment_metadata(&exp_dir).ok()?;
                if exp_meta.project.as_deref() == Some(project.as_str()) {
                    let runs = storage::list_runs(&exp_dir).unwrap_or_default();
                    Some(serde_json::json!({
                        "id": exp_name,
                        "display_name": exp_meta.display_name.unwrap_or_else(|| exp_name.to_string()),
                        "description": exp_meta.description,
                        "tags": exp_meta.tags,
                        "runs_count": runs.len(),
                    }))
                } else {
                    None
                }
            }).collect();

            Json(serde_json::json!({
                "id": project,
                "display_name": meta.display_name.unwrap_or_else(|| project.clone()),
                "description": meta.description,
                "tags": meta.tags,
                "created_at": meta.created_at,
                "readme": readme,
                "experiments": project_experiments,
            }))
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
        Ok(content) => Json(serde_json::json!({
            "content": content.unwrap_or_default(),
        }))
        .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
pub struct ReadmeUpdate {
    pub content: String,
}

pub async fn update_project_readme(
    State(state): State<AppState>,
    Path(project): Path<String>,
    Json(body): Json<ReadmeUpdate>,
) -> impl IntoResponse {
    match storage::save_project_readme(&state.base_dir, &project, &body.content) {
        Ok(_) => Json(serde_json::json!({ "content": body.content })).into_response(),
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
    match storage::save_project_metadata(&state.base_dir, &project, &meta) {
        Ok(_) => Json(serde_json::json!({
            "id": project,
            "display_name": meta.display_name.unwrap_or_else(|| project.clone()),
            "description": meta.description,
            "tags": meta.tags,
            "created_at": meta.created_at,
        }))
        .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn delete_project(
    State(state): State<AppState>,
    Path(project): Path<String>,
) -> impl IntoResponse {
    match storage::delete_project(&state.base_dir, &project) {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
