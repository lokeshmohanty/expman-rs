//! Data models used across the frontend.
//!
//! These are **not** defined here. They are the shared wire types in
//! [`crate::core::dto`], which the server serializes and this module simply
//! re-exports — so the two can no longer drift. This file was previously a
//! hand-maintained mirror of `serde_json::json!` literals in the handlers, and
//! adding a field to an endpoint meant remembering to add it here too.
//!
//! Add a field to `core::dto` and both sides get it.

pub use crate::core::dto::{
    Artifact, BackendInfo, Experiment, GlobalStats, InteractiveBackend, NotebookInfo, Project,
    ProjectDetail, ProjectRuns, Run, ServiceStartResponse, ServiceStatus, TensorBoardBackendInfo,
    TensorBoardLogsInfo,
};
pub use crate::core::models::{ExperimentMetadata, MetricValue, RunStatus};

// The frontend used to have distinct Jupyter/TensorBoard status types with
// identical shapes. They are one type now; these aliases keep the call sites
// reading as what they fetch.
pub(crate) type JupyterStatus = ServiceStatus;
pub(crate) type JupyterStartResponse = ServiceStartResponse;
pub(crate) type TensorBoardStatus = ServiceStatus;
pub(crate) type TensorBoardStartResponse = ServiceStartResponse;
