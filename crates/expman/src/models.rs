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
        }
    }

    pub fn with_run_name(mut self, run_name: impl Into<String>) -> Self {
        self.run_name = run_name.into();
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MetricValue {
    Float(f64),
    Int(i64),
    Bool(bool),
    Text(String),
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

/// A row of metrics logged at a specific step/time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricRow {
    pub step: Option<u64>,
    pub timestamp: DateTime<Utc>,
    pub values: HashMap<String, MetricValue>,
}

impl MetricRow {
    pub fn new(values: HashMap<String, MetricValue>, step: Option<u64>) -> Self {
        Self {
            step,
            timestamp: Utc::now(),
            values,
        }
    }
}

/// Status of a run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum RunStatus {
    Running,
    Finished,
    Failed,
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
    /// Latest scalar metrics (numeric only) from the last logged row.
    #[serde(default)]
    pub metrics: Option<HashMap<String, f64>>,
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
            metrics: None,
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
}
