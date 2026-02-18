//! Async logging engine: the heart of expman-rs.
//!
//! `LoggingEngine::new()` spawns a background tokio task that owns all file handles.
//! `log_metrics()` is a channel send — O(1), never blocks the experiment process.
//! The background task batches rows and flushes to Parquet periodically.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::runtime::Runtime;
use tokio::sync::{mpsc, oneshot};
use tokio::time::interval;
use tracing::{error, info};

use crate::error::{ExpmanError, Result};
use crate::models::{ExperimentConfig, MetricRow, MetricValue, RunMetadata, RunStatus};
use crate::storage;

/// Commands sent to the background logging task.
enum LogCommand {
    /// Log a row of metrics.
    Metric(MetricRow),
    /// Update the config/params YAML.
    Params(HashMap<String, serde_yaml::Value>),
    /// Copy an artifact file into the run's artifacts directory.
    Artifact(PathBuf),
    /// Log a message to the run log file.
    Log { level: LogLevel, message: String },
    /// Force flush the current buffer to disk.
    Flush(oneshot::Sender<Result<()>>),
    /// Gracefully shut down: flush everything, write final metadata.
    Shutdown {
        status: RunStatus,
        reply: oneshot::Sender<()>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

/// The non-blocking logging engine.
///
/// Internally holds a sender to a tokio mpsc channel. All heavy I/O
/// happens in a background task on a dedicated tokio runtime thread.
pub struct LoggingEngine {
    sender: mpsc::UnboundedSender<LogCommand>,
    /// Keep the runtime alive as long as the engine exists.
    _runtime: Arc<Runtime>,
    config: ExperimentConfig,
}

impl LoggingEngine {
    /// Create a new `LoggingEngine` for the given config.
    ///
    /// This initializes the run directory, writes initial metadata,
    /// and spawns the background I/O task.
    pub fn new(config: ExperimentConfig) -> Result<Self> {
        // Set up directories
        let run_dir = config.run_dir();
        storage::ensure_dir(&run_dir)?;
        storage::ensure_dir(&run_dir.join("artifacts"))?;

        // Write initial run metadata
        let meta = RunMetadata {
            name: config.run_name.clone(),
            experiment: config.name.clone(),
            status: RunStatus::Running,
            started_at: Utc::now(),
            ..Default::default()
        };
        storage::save_run_metadata(&run_dir, &meta)?;

        // Ensure experiment metadata exists
        let exp_dir = config.experiment_dir();
        storage::ensure_dir(&exp_dir)?;
        let exp_meta_path = exp_dir.join("experiment.yaml");
        if !exp_meta_path.exists() {
            storage::save_experiment_metadata(
                &exp_dir,
                &crate::models::ExperimentMetadata::default(),
            )?;
        }

        // Set up log file
        let log_path = run_dir.join("run.log");

        // Build dedicated tokio runtime for background I/O
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .thread_name("expman-io")
                .enable_all()
                .build()
                .map_err(|e| ExpmanError::Other(e.to_string()))?,
        );

        let (sender, receiver) = mpsc::unbounded_channel::<LogCommand>();

        // Spawn background task
        let flush_rows = config.flush_interval_rows;
        let flush_ms = config.flush_interval_ms;
        let run_dir_clone = run_dir.clone();
        runtime.spawn(background_task(
            receiver,
            run_dir_clone,
            log_path,
            flush_rows,
            flush_ms,
        ));

        info!(
            experiment = %config.name,
            run = %config.run_name,
            "LoggingEngine initialized"
        );

        Ok(Self {
            sender,
            _runtime: runtime,
            config,
        })
    }

    /// Log a row of metrics. Non-blocking — channel send only.
    pub fn log_metrics(&self, values: HashMap<String, MetricValue>, step: Option<u64>) {
        let row = MetricRow::new(values, step);
        // If channel is closed (engine shut down), silently drop.
        let _ = self.sender.send(LogCommand::Metric(row));
    }

    /// Log/update experiment parameters (config). Non-blocking.
    pub fn log_params(&self, params: HashMap<String, serde_yaml::Value>) {
        let _ = self.sender.send(LogCommand::Params(params));
    }

    /// Save an artifact file asynchronously. Non-blocking.
    /// The path is relative to the current working directory for the source,
    /// and will be preserved as a relative path within the run's artifacts directory.
    pub fn save_artifact(&self, path: PathBuf) {
        let _ = self.sender.send(LogCommand::Artifact(path));
    }

    /// Log a message to the run log. Non-blocking.
    pub fn log_message(&self, level: LogLevel, message: String) {
        let _ = self.sender.send(LogCommand::Log { level, message });
    }

    /// Force flush the metric buffer to disk. Async — awaits completion.
    pub async fn flush(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(LogCommand::Flush(tx))
            .map_err(|_| ExpmanError::ChannelClosed)?;
        rx.await.map_err(|_| ExpmanError::ChannelClosed)?
    }

    /// Gracefully shut down: flush all pending metrics, write final metadata.
    /// Blocks until complete. Should be called at experiment end.
    pub fn close(&self, status: RunStatus) {
        let (tx, rx) = oneshot::channel();
        if self
            .sender
            .send(LogCommand::Shutdown { status, reply: tx })
            .is_ok()
        {
            // Block current thread until background task confirms shutdown.
            // We use the runtime's block_on for this.
            let _ = self._runtime.block_on(rx);
        }
    }

    pub fn config(&self) -> &ExperimentConfig {
        &self.config
    }
}

impl Drop for LoggingEngine {
    fn drop(&mut self) {
        // Best-effort graceful shutdown on drop
        let (tx, rx) = oneshot::channel();
        if self
            .sender
            .send(LogCommand::Shutdown {
                status: RunStatus::Finished,
                reply: tx,
            })
            .is_ok()
        {
            let _ = self
                ._runtime
                .block_on(async { tokio::time::timeout(Duration::from_secs(5), rx).await });
        }
    }
}

// ─── Background I/O task ─────────────────────────────────────────────────────

async fn background_task(
    mut receiver: mpsc::UnboundedReceiver<LogCommand>,
    run_dir: PathBuf,
    log_path: PathBuf,
    flush_interval_rows: usize,
    flush_interval_ms: u64,
) {
    let metrics_path = run_dir.join("metrics.parquet");
    let config_path = run_dir.join("config.yaml");
    let _meta_path = run_dir.join("run.yaml");
    let artifacts_dir = run_dir.join("artifacts");

    let mut metric_buffer: Vec<MetricRow> = Vec::with_capacity(flush_interval_rows * 2);
    let mut log_lines: Vec<String> = Vec::new();
    let mut flush_ticker = interval(Duration::from_millis(flush_interval_ms));
    flush_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let started_at = Utc::now();

    loop {
        tokio::select! {
            // Prioritize incoming commands
            biased;

            cmd = receiver.recv() => {
                match cmd {
                    None => {
                        // Channel closed — flush and exit
                        flush_metrics(&metrics_path, &mut metric_buffer);
                        flush_logs(&log_path, &mut log_lines);
                        break;
                    }
                    Some(LogCommand::Metric(row)) => {
                        metric_buffer.push(row);
                        if metric_buffer.len() >= flush_interval_rows {
                            flush_metrics(&metrics_path, &mut metric_buffer);
                        }
                    }
                    Some(LogCommand::Params(params)) => {
                        handle_params(&config_path, params);
                    }
                    Some(LogCommand::Artifact(path)) => {
                        handle_artifact(&artifacts_dir, path);
                    }
                    Some(LogCommand::Log { level, message }) => {
                        let ts = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ");
                        let level_str = match level {
                            LogLevel::Info => "INFO",
                            LogLevel::Warn => "WARN",
                            LogLevel::Error => "ERROR",
                        };
                        log_lines.push(format!("[{ts}] [{level_str}] {message}"));
                        if log_lines.len() >= 20 {
                            flush_logs(&log_path, &mut log_lines);
                        }
                    }
                    Some(LogCommand::Flush(reply)) => {
                        flush_metrics(&metrics_path, &mut metric_buffer);
                        flush_logs(&log_path, &mut log_lines);
                        let _ = reply.send(Ok(()));
                    }
                    Some(LogCommand::Shutdown { status, reply }) => {
                        // Final flush
                        flush_metrics(&metrics_path, &mut metric_buffer);
                        flush_logs(&log_path, &mut log_lines);

                        // Update run metadata with final status
                        let finished_at = Utc::now();
                        let duration = (finished_at - started_at).num_milliseconds() as f64 / 1000.0;

                        if let Ok(mut meta) = storage::load_run_metadata(&run_dir) {
                            meta.status = status;
                            meta.finished_at = Some(finished_at);
                            meta.duration_secs = Some(duration);
                            let _ = storage::save_run_metadata(&run_dir, &meta);
                        }

                        let _ = reply.send(());
                        break;
                    }
                }
            }

            // Periodic flush
            _ = flush_ticker.tick() => {
                if !metric_buffer.is_empty() {
                    flush_metrics(&metrics_path, &mut metric_buffer);
                }
                if !log_lines.is_empty() {
                    flush_logs(&log_path, &mut log_lines);
                }
            }
        }
    }
}

fn flush_metrics(path: &std::path::Path, buffer: &mut Vec<MetricRow>) {
    if buffer.is_empty() {
        return;
    }
    if let Err(e) = storage::append_metrics(path, buffer) {
        error!("Failed to flush metrics: {}", e);
    }
    buffer.clear();
}

fn flush_logs(path: &std::path::Path, lines: &mut Vec<String>) {
    if lines.is_empty() {
        return;
    }
    use std::io::Write;
    match fs::OpenOptions::new().create(true).append(true).open(path) {
        Ok(mut f) => {
            for line in lines.iter() {
                let _ = writeln!(f, "{}", line);
            }
        }
        Err(e) => error!("Failed to write log: {}", e),
    }
    lines.clear();
}

fn handle_params(config_path: &std::path::Path, new_params: HashMap<String, serde_yaml::Value>) {
    // Load existing, merge, save
    let mut existing: HashMap<String, serde_yaml::Value> =
        storage::load_yaml(config_path).unwrap_or_default();
    existing.extend(new_params);
    if let Err(e) = storage::save_yaml(config_path, &existing) {
        error!("Failed to save params: {}", e);
    }
}

fn handle_artifact(artifacts_dir: &std::path::Path, path: PathBuf) {
    let dest = artifacts_dir.join(&path);
    if let Some(parent) = dest.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            error!("Failed to create artifact dir: {}", e);
            return;
        }
    }
    if let Err(e) = fs::copy(&path, &dest) {
        error!(
            "Failed to copy artifact {} -> {}: {}",
            path.display(),
            dest.display(),
            e
        );
    }
}
