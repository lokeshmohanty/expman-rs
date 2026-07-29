//! Data models for expman-rs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Configuration for a single experiment run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentConfig {
    /// Name of the experiment (e.g. "resnet_cifar10")
    pub name: String,
    /// Name of this specific run (auto-generated if None)
    pub run_name: String,
    /// Root directory for all experiments
    pub base_dir: PathBuf,
    /// Flush metrics to disk every N rows (default: 50)
    pub flush_interval_rows: usize,
    /// Flush metrics to disk every N milliseconds (default: 500)
    pub flush_interval_ms: u64,
    /// Language used for the run (e.g. "rust", "python")
    pub language: String,
    /// Environment path or executable (e.g. python executable path)
    pub env_path: Option<String>,
    /// Project this experiment belongs to. Written into `experiment.yaml` at run
    /// creation so the projects layer is reachable without a running server.
    pub project: Option<String>,
    /// Tags for this run, written into `run.yaml` at creation.
    pub tags: Vec<String>,
    /// Description for this run, written into `run.yaml` at creation.
    pub description: Option<String>,
    /// Interval between run heartbeats, in seconds. 0 disables the heartbeat.
    pub heartbeat_interval_secs: u64,
    /// Interval between system-metric samples, in seconds. 0 disables sampling.
    pub system_metrics_interval_secs: u64,
    /// Group this run belongs to — the unit a DDP job or a sweep is reasoned
    /// about as. All ranks of one job share a group.
    pub group: Option<String>,
    /// Rank within `group`. Rank 0 represents the group in rolled-up views.
    pub rank: Option<u32>,
    /// Capture git SHA/branch/dirty and scheduler ids at run creation.
    pub capture_provenance: bool,
    /// Also capture the working-tree diff. Off by default: a dirty tree can
    /// carry secrets into a store you may later share.
    pub capture_diff: bool,
}

impl ExperimentConfig {
    pub fn new(name: impl Into<String>, base_dir: impl Into<PathBuf>) -> Self {
        let now = chrono::Local::now();
        Self {
            name: name.into(),
            run_name: now.format("%Y%m%d_%H%M%S").to_string(),
            base_dir: base_dir.into(),
            flush_interval_rows: 50,
            flush_interval_ms: 500,
            language: "rust".to_string(),
            env_path: None,
            project: None,
            tags: Vec::new(),
            description: None,
            heartbeat_interval_secs: 30,
            system_metrics_interval_secs: 15,
            group: None,
            rank: None,
            capture_provenance: true,
            capture_diff: false,
        }
    }

    pub fn with_run_name(mut self, run_name: impl Into<String>) -> Self {
        self.run_name = run_name.into();
        self
    }

    pub fn with_project(mut self, project: impl Into<String>) -> Self {
        self.project = Some(project.into());
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_group(mut self, group: impl Into<String>, rank: u32) -> Self {
        self.group = Some(group.into());
        self.rank = Some(rank);
        self
    }

    pub fn run_dir(&self) -> PathBuf {
        self.base_dir.join(&self.name).join(&self.run_name)
    }

    pub fn experiment_dir(&self) -> PathBuf {
        self.base_dir.join(&self.name)
    }
}

/// A single metric value — supports float, int, or string.
///
/// `PartialEq` is needed by the frontend: Leptos memoises on it to decide
/// whether a signal actually changed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum MetricValue {
    Float(f64),
    Int(i64),
    Bool(bool),
    Text(String),
}

impl std::fmt::Display for MetricValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Float(v) => write!(f, "{}", v),
            Self::Int(v) => write!(f, "{}", v),
            Self::Bool(v) => write!(f, "{}", v),
            Self::Text(v) => write!(f, "{}", v),
        }
    }
}

impl From<f64> for MetricValue {
    fn from(v: f64) -> Self {
        MetricValue::Float(v)
    }
}
impl From<f32> for MetricValue {
    fn from(v: f32) -> Self {
        MetricValue::Float(v as f64)
    }
}
impl From<i64> for MetricValue {
    fn from(v: i64) -> Self {
        MetricValue::Int(v)
    }
}
impl From<i32> for MetricValue {
    fn from(v: i32) -> Self {
        MetricValue::Int(v as i64)
    }
}
impl From<usize> for MetricValue {
    fn from(v: usize) -> Self {
        MetricValue::Int(v as i64)
    }
}
impl From<bool> for MetricValue {
    fn from(v: bool) -> Self {
        MetricValue::Bool(v)
    }
}
impl From<String> for MetricValue {
    fn from(v: String) -> Self {
        MetricValue::Text(v)
    }
}
impl From<&str> for MetricValue {
    fn from(v: &str) -> Self {
        MetricValue::Text(v.to_string())
    }
}

/// A single vector row logged at a specific step/time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorRow {
    pub step: Option<u64>,
    pub timestamp: DateTime<Utc>,
    pub values: HashMap<String, MetricValue>,
}

impl VectorRow {
    pub fn new(values: HashMap<String, MetricValue>, step: Option<u64>) -> Self {
        Self {
            step,
            timestamp: Utc::now(),
            values,
        }
    }
}

/// Status of a run.
///
/// `Crashed` is the default for the same reason `RunMetadata::default()` uses
/// it: a run we cannot read anything about is one that died without saying so.
/// Defaulting to `Running` would inflate the active count with unknowns.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum RunStatus {
    Running,
    Finished,
    Failed,
    #[default]
    Crashed,
}

impl std::fmt::Display for RunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunStatus::Running => write!(f, "RUNNING"),
            RunStatus::Finished => write!(f, "FINISHED"),
            RunStatus::Failed => write!(f, "FAILED"),
            RunStatus::Crashed => write!(f, "CRASHED"),
        }
    }
}

/// Metadata stored alongside a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetadata {
    pub name: String,
    pub experiment: String,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_secs: Option<f64>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    /// Group this run belongs to; all ranks of a DDP job share one.
    #[serde(default)]
    pub group: Option<String>,
    /// Rank within the group. Rank 0 stands for the group when rolled up.
    #[serde(default)]
    pub rank: Option<u32>,
    /// Last time the logging engine confirmed this run was alive.
    ///
    /// A `RUNNING` run whose heartbeat has gone stale was killed without
    /// closing; `exp reap` uses this to distinguish it from a legitimately
    /// long-running job. `None` on runs written before heartbeats existed —
    /// those fall back to `started_at`.
    #[serde(default)]
    pub heartbeat_at: Option<DateTime<Utc>>,
    /// Latest scalar values (replaced on update).
    #[serde(default)]
    pub scalars: Option<HashMap<String, MetricValue>>,
    /// Latest vector values (latest row summary).
    #[serde(default)]
    pub vectors: Option<HashMap<String, MetricValue>>,
    /// Language of the run
    #[serde(default)]
    pub language: Option<String>,
    /// Environment path or executable used
    #[serde(default)]
    pub env_path: Option<String>,
}

impl Default for RunMetadata {
    fn default() -> Self {
        Self {
            name: String::new(),
            experiment: String::new(),
            status: RunStatus::Crashed,
            started_at: Utc::now(),
            finished_at: None,
            duration_secs: None,
            description: None,
            tags: None,
            group: None,
            rank: None,
            heartbeat_at: None,
            scalars: None,
            vectors: None,
            language: None,
            env_path: None,
        }
    }
}

/// Metadata stored for an experiment.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExperimentMetadata {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    #[serde(default)]
    pub project: Option<String>,
}

/// Metadata stored for a project.
///
/// A project may be a *generated projection* of an authoritative source that
/// lives outside expman (see `generated`). Such a project is overwritten
/// wholesale by the next sync, so the dashboard must present it read-only
/// rather than silently losing the user's edits.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectMetadata {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub created_at: Option<DateTime<Utc>>,
    /// True when this project is generated from an external source and will be
    /// clobbered by the next sync. Edits through the dashboard are refused.
    #[serde(default)]
    pub generated: bool,
    /// Human-readable pointer to whatever is authoritative, e.g.
    /// `"studies.yaml (thesis repo)"`. Shown next to the read-only marker.
    #[serde(default)]
    pub generated_from: Option<String>,
    /// When the last sync from `generated_from` ran.
    #[serde(default)]
    pub generated_at: Option<DateTime<Utc>>,
}
