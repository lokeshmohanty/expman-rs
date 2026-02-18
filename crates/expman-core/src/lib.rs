//! expman-core: Core storage and logging engine for expman-rs.
//!
//! The central design principle: `log_metrics()` is a channel send (~100ns),
//! never blocking the experiment process. A background tokio task handles
//! all I/O asynchronously.

pub mod engine;
pub mod error;
pub mod models;
pub mod storage;

pub use engine::{LogLevel, LoggingEngine};
pub use error::ExpmanError;
pub use models::{ExperimentConfig, MetricRow, MetricValue, RunStatus};
