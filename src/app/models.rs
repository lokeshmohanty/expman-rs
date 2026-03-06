//! Data models used across the frontend.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Default)]
pub struct ExperimentMetadata {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Default)]
pub struct Experiment {
    pub id: String,
    pub display_name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub runs_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Run {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_secs: Option<f64>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub scalars: Option<std::collections::HashMap<String, MetricValue>>,
    pub vectors: Option<std::collections::HashMap<String, MetricValue>>,
    pub language: Option<String>,
    pub env_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Default)]
pub struct GlobalStats {
    pub total_experiments: usize,
    pub total_runs: usize,
    pub active_runs: usize,
    pub total_storage_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Artifact {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub ext: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct BackendInfo {
    pub(crate) backend: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct JupyterStatus {
    pub(crate) running: bool,
    pub(crate) port: Option<u16>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct JupyterStartResponse {
    pub(crate) port: u16,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct NotebookInfo {
    pub(crate) exists: bool,
    pub(crate) content: Option<String>,
}
