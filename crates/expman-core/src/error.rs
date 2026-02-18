//! Error types for expman-core.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExpmanError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),

    #[error("Parquet error: {0}")]
    Parquet(#[from] parquet::errors::ParquetError),

    #[error("YAML serialization error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Logging engine channel closed")]
    ChannelClosed,

    #[error("Run directory not found: {0}")]
    RunNotFound(String),

    #[error("Experiment not found: {0}")]
    ExperimentNotFound(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, ExpmanError>;
