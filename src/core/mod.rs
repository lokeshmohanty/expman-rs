#![doc = include_str!("./README.md")]
//! expman: Core storage and logging engine for expman-rs.
//!
//! The central design principle: `log_vector()` is a channel send (~100ns),
//! never blocking the experiment process. A background tokio task handles
//! all I/O asynchronously.
//!

#[cfg(not(target_arch = "wasm32"))]
pub mod engine;
pub mod error;
pub mod models;
#[cfg(not(target_arch = "wasm32"))]
pub mod storage;

#[cfg(not(target_arch = "wasm32"))]
pub use engine::{LogLevel, LoggingEngine};
pub use error::ExpmanError;
pub use models::{ExperimentConfig, MetricValue, RunMetadata, RunStatus, VectorRow};

/// 📚 **Guide**: Interactive Jupyter Notebooks in ExpMan
#[doc = include_str!("../app/README.md")]
pub mod jupyter_integration {}
