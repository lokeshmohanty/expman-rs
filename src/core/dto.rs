//! Wire types: the contract between the HTTP API and the WASM frontend.
//!
//! This module is compiled for **both** native and `wasm32`, which is the whole
//! point. The server serializes these types and the frontend deserializes the
//! same definitions, so a field can no longer be added on one side and silently
//! missed on the other — which is exactly what happened when `src/app/models.rs`
//! was a hand-maintained mirror of `serde_json::json!` literals in the handlers.
//!
//! These are deliberately *not* the storage models. A wire type may expose less
//! than `RunMetadata` (or shape it differently) — but it does so in typed code,
//! via the constructors below, so removing a storage field breaks the build
//! rather than quietly changing an endpoint's output.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::core::models::{
    ExperimentMetadata, MetricValue, ProjectMetadata, RunMetadata, RunStatus,
};

// ─── Experiments ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct Experiment {
    pub id: String,
    /// Falls back to `id` when no display name is set, so the UI never has to.
    pub display_name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub project: Option<String>,
    pub runs_count: usize,
}

impl Experiment {
    pub fn new(id: impl Into<String>, meta: ExperimentMetadata, runs_count: usize) -> Self {
        let id = id.into();
        Self {
            display_name: meta.display_name.unwrap_or_else(|| id.clone()),
            id,
            description: meta.description,
            tags: meta.tags,
            project: meta.project,
            runs_count,
        }
    }
}

// ─── Projects ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct Project {
    pub id: String,
    pub display_name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub experiments_count: usize,
    pub created_at: Option<DateTime<Utc>>,
    /// Generated projection of an external source: read-only, regenerated on
    /// each sync. The dashboard hides its edit affordances rather than offering
    /// a write the next sync would discard.
    #[serde(default)]
    pub generated: bool,
    #[serde(default)]
    pub generated_from: Option<String>,
    #[serde(default)]
    pub generated_at: Option<DateTime<Utc>>,
}

impl Project {
    pub fn new(id: impl Into<String>, meta: ProjectMetadata, experiments_count: usize) -> Self {
        let id = id.into();
        Self {
            display_name: meta.display_name.unwrap_or_else(|| id.clone()),
            id,
            description: meta.description,
            tags: meta.tags,
            experiments_count,
            created_at: meta.created_at,
            generated: meta.generated,
            generated_from: meta.generated_from,
            generated_at: meta.generated_at,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct ProjectDetail {
    pub id: String,
    pub display_name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub generated: bool,
    #[serde(default)]
    pub generated_from: Option<String>,
    #[serde(default)]
    pub generated_at: Option<DateTime<Utc>>,
    pub readme: Option<String>,
    pub experiments: Vec<Experiment>,
}

impl ProjectDetail {
    pub fn new(
        id: impl Into<String>,
        meta: ProjectMetadata,
        readme: Option<String>,
        experiments: Vec<Experiment>,
    ) -> Self {
        let id = id.into();
        Self {
            display_name: meta.display_name.unwrap_or_else(|| id.clone()),
            id,
            description: meta.description,
            tags: meta.tags,
            created_at: meta.created_at,
            generated: meta.generated,
            generated_from: meta.generated_from,
            generated_at: meta.generated_at,
            readme,
            experiments,
        }
    }
}

// ─── Runs ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Run {
    /// The run directory name. Distinct from `name` only in principle, but the
    /// frontend routes on it, so it is explicit rather than implied.
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub experiment: String,
    pub status: RunStatus,
    /// Group this run belongs to; all ranks of a DDP job share one.
    #[serde(default)]
    pub group: Option<String>,
    /// Rank within the group. Rank 0 stands for the group when rolled up.
    #[serde(default)]
    pub rank: Option<u32>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub heartbeat_at: Option<DateTime<Utc>>,
    pub duration_secs: Option<f64>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub scalars: Option<HashMap<String, MetricValue>>,
    pub vectors: Option<HashMap<String, MetricValue>>,
    pub language: Option<String>,
    pub env_path: Option<String>,
}

impl Run {
    pub fn new(id: impl Into<String>, meta: RunMetadata) -> Self {
        Self {
            id: id.into(),
            name: meta.name,
            experiment: meta.experiment,
            status: meta.status,
            group: meta.group,
            rank: meta.rank,
            started_at: meta.started_at,
            finished_at: meta.finished_at,
            heartbeat_at: meta.heartbeat_at,
            duration_secs: meta.duration_secs,
            description: meta.description,
            tags: meta.tags,
            scalars: meta.scalars,
            vectors: meta.vectors,
            language: meta.language,
            env_path: meta.env_path,
        }
    }
}

/// Response of `GET /projects/{project}/runs`.
///
/// Carries the flat cross-experiment runs table plus the facets a filter UI
/// needs, so the frontend never has to derive them by scanning every run.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct ProjectRuns {
    pub project: String,
    pub runs: Vec<Run>,
    pub total: usize,
    pub facets: RunFacets,
    /// Union of metric names across the returned runs.
    pub metrics: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct RunFacets {
    pub tags: std::collections::BTreeMap<String, usize>,
    pub status: std::collections::BTreeMap<String, usize>,
    pub experiments: std::collections::BTreeMap<String, usize>,
    /// Group → number of runs in it. Empty when nothing is grouped.
    #[serde(default)]
    pub groups: std::collections::BTreeMap<String, usize>,
}

// ─── Artifacts ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Artifact {
    /// Path relative to the artifacts root.
    pub path: String,
    pub name: String,
    pub size: u64,
    /// Lowercased extension, used by the frontend to pick a viewer.
    pub ext: String,
    /// True for the run's own files (run.yaml, vectors.parquet, …) rather than
    /// something the user saved.
    pub is_default: bool,
}

// ─── Stats and server config ──────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct GlobalStats {
    pub total_experiments: usize,
    #[serde(default)]
    pub total_projects: usize,
    pub total_runs: usize,
    /// `RUNNING` runs whose heartbeat is still fresh.
    pub active_runs: usize,
    /// `RUNNING` runs that have gone silent — almost certainly hard-killed.
    /// Counted separately so `active_runs` cannot quietly accumulate corpses.
    #[serde(default)]
    pub stale_runs: usize,
    pub total_storage_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct ServerConfig {
    pub live_mode: bool,
    #[serde(default)]
    pub read_only: bool,
    pub version: String,
}

/// Body of both `GET` and `PUT /projects/{p}/readme`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct ReadmeContent {
    pub content: String,
}

// ─── Jupyter and TensorBoard ──────────────────────────────────────────────────

/// The interactive backend detected on the server.
///
/// Serialized lowercase, and `Display` is derived from the same match so the
/// wire value and the printed value cannot disagree.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum InteractiveBackend {
    Jupyter,
    Python,
    #[default]
    None,
}

impl std::fmt::Display for InteractiveBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Jupyter => "jupyter",
            Self::Python => "python",
            Self::None => "none",
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct BackendInfo {
    pub backend: InteractiveBackend,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct ServiceStatus {
    pub running: bool,
    pub port: Option<u16>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ServiceStartResponse {
    pub port: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct NotebookInfo {
    pub exists: bool,
    pub content: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct TensorBoardBackendInfo {
    pub available: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct TensorBoardLogsInfo {
    pub has_logs: bool,
}
