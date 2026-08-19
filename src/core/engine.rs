//! Async logging engine: the heart of expman-rs.
//!
//! `LoggingEngine::new()` spawns a background tokio task that owns all file handles.
//! `log_vector()` is a channel send — O(1), never blocks the experiment process.
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

use crate::core::error::{ExpmanError, Result};
use crate::core::models::{ExperimentConfig, MetricValue, RunMetadata, RunStatus, VectorRow};
use crate::core::storage;

/// Commands sent to the background logging task.
enum LogCommand {
    /// Log a row of vector metrics.
    Vector(VectorRow),
    /// Log a single scalar value (replaces if exists).
    Scalar(HashMap<String, MetricValue>),
    /// Update the config/params YAML.
    Params(HashMap<String, serde_yaml::Value>),
    /// Copy an artifact file into the run's artifacts directory.
    Artifact(PathBuf),
    /// Write raw bytes as a media file and record it in the media manifest.
    Media {
        tag: String,
        step: Option<u64>,
        extension: String,
        bytes: Vec<u8>,
    },
    /// Record a histogram: bin edges and counts for one tag at one step.
    Histogram {
        tag: String,
        step: Option<u64>,
        edges: Vec<f64>,
        counts: Vec<u64>,
    },
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
        let now = Utc::now();
        let meta = RunMetadata {
            name: config.run_name.clone(),
            experiment: config.name.clone(),
            status: RunStatus::Running,
            started_at: now,
            heartbeat_at: Some(now),
            language: Some(config.language.clone()),
            env_path: config.env_path.clone(),
            description: config.description.clone(),
            group: config.group.clone(),
            rank: config.rank,
            tags: if config.tags.is_empty() {
                None
            } else {
                Some(config.tags.clone())
            },
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
                &crate::core::models::ExperimentMetadata {
                    project: config.project.clone(),
                    ..Default::default()
                },
            )?;
        } else if let Some(project) = &config.project {
            // experiment.yaml is only written when absent, so an explicit
            // `project=` from a later run would otherwise be silently ignored.
            // Update just that field and leave the rest of the file alone.
            let mut existing = storage::load_experiment_metadata(&exp_dir)?;
            if existing.project.as_deref() != Some(project.as_str()) {
                existing.project = Some(project.clone());
                storage::save_experiment_metadata(&exp_dir, &existing)?;
            }
        }

        // Capture provenance once, at creation. Cheap enough to be default-on;
        // the diff stays opt-in (see core::provenance).
        if config.capture_provenance {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let provenance =
                crate::core::provenance::Provenance::capture(&cwd, config.capture_diff);
            if let Err(e) = storage::save_yaml(&run_dir.join("provenance.yaml"), &provenance) {
                error!("Failed to write provenance: {}", e);
            }
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
                .map_err(|e: std::io::Error| ExpmanError::Other(e.to_string()))?,
        );

        let (sender, receiver) = mpsc::unbounded_channel::<LogCommand>();

        // Spawn background task
        let flush_rows = config.flush_interval_rows;
        let flush_ms = config.flush_interval_ms;
        let heartbeat_secs = config.heartbeat_interval_secs;
        let system_secs = config.system_metrics_interval_secs;
        let run_dir_clone = run_dir.clone();
        runtime.spawn(background_task(
            receiver,
            run_dir_clone,
            log_path,
            flush_rows,
            flush_ms,
            heartbeat_secs,
            system_secs,
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

    /// Log a row of vector metrics. Non-blocking — channel send only.
    pub fn log_vector(&self, values: HashMap<String, MetricValue>, step: Option<u64>) {
        let row = VectorRow::new(values, step);
        // If channel is closed (engine shut down), silently drop.
        let _ = self.sender.send(LogCommand::Vector(row));
    }

    /// Log a single scalar value. Non-blocking — replaces existing value for the key.
    pub fn log_scalar(&self, key: String, value: MetricValue) {
        let mut map = HashMap::new();
        map.insert(key, value);
        let _ = self.sender.send(LogCommand::Scalar(map));
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

    /// Save an image/audio/video blob under `media/`, indexed by tag and step.
    pub fn log_media(&self, tag: String, step: Option<u64>, extension: String, bytes: Vec<u8>) {
        let _ = self.sender.send(LogCommand::Media {
            tag,
            step,
            extension,
            bytes,
        });
    }

    /// Record a pre-binned histogram.
    pub fn log_histogram(&self, tag: String, step: Option<u64>, edges: Vec<f64>, counts: Vec<u64>) {
        let _ = self.sender.send(LogCommand::Histogram {
            tag,
            step,
            edges,
            counts,
        });
    }

    /// Force flush the metric buffer to disk. Async — awaits completion.
    pub async fn flush(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(LogCommand::Flush(tx))
            .map_err(|_| ExpmanError::ChannelClosed)?;
        let res: Result<()> = rx.await.map_err(|_| ExpmanError::ChannelClosed)?;
        res
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
    heartbeat_interval_secs: u64,
    system_interval_secs: u64,
) {
    let mut vector_writer = storage::MetricWriter::new(&run_dir, storage::VECTORS_STEM);
    let config_path = run_dir.join("config.yaml");
    let _meta_path = run_dir.join("run.yaml");
    let artifacts_dir = run_dir.join("artifacts");

    let mut vector_buffer: Vec<VectorRow> = Vec::with_capacity(flush_interval_rows * 2);
    let mut current_scalars: HashMap<String, MetricValue> = HashMap::new();
    let mut current_vectors: HashMap<String, MetricValue> = HashMap::new();
    let mut log_lines: Vec<String> = Vec::new();
    let mut flush_ticker = interval(Duration::from_millis(flush_interval_ms));
    flush_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // `interval` panics on a zero duration, so clamp and gate the select branch
    // on the original value instead — 0 means "no heartbeat".
    let heartbeat_enabled = heartbeat_interval_secs > 0;
    let mut heartbeat_ticker = interval(Duration::from_secs(heartbeat_interval_secs.max(1)));
    heartbeat_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // System metrics go to their own file so a `gpu.0.util_pct` column never
    // shows up mixed into the user's own metric namespace.
    let system_enabled = system_interval_secs > 0;
    let mut system_sampler = system_enabled.then(|| {
        crate::core::sysmetrics::SystemSampler::new(crate::core::sysmetrics::ProbeSpec::defaults())
    });
    let mut system_writer = storage::MetricWriter::new(&run_dir, storage::SYSTEM_STEM);
    // Histograms are their own family: they are one row per (tag, step) with a
    // variable number of bins, which does not belong in the scalar schema.
    let mut histogram_writer = storage::MetricWriter::new(&run_dir, storage::HISTOGRAM_STEM);
    let mut system_step: u64 = 0;
    let mut system_ticker = interval(Duration::from_secs(system_interval_secs.max(1)));
    system_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let started_at = Utc::now();

    loop {
        tokio::select! {
            // Prioritize incoming commands
            biased;

            cmd = receiver.recv() => {
                match cmd {
                    None => {
                        // Channel closed — flush and exit
                        flush_vectors(&mut vector_writer, &mut vector_buffer);
                        flush_logs(&log_path, &mut log_lines);
                        break;
                    }
                    Some(LogCommand::Vector(row)) => {
                        // Update current vectors with latest values from this row
                        for (k, v) in row.values.iter() {
                            current_vectors.insert(k.clone(), v.clone());
                        }
                        vector_buffer.push(row);
                        if vector_buffer.len() >= flush_interval_rows {
                            flush_vectors(&mut vector_writer, &mut vector_buffer);
                        }
                    }
                    Some(LogCommand::Scalar(scalars)) => {
                        current_scalars.extend(scalars);
                    }
                    Some(LogCommand::Params(params)) => {
                        handle_params(&config_path, params);
                    }
                    Some(LogCommand::Artifact(path)) => {
                        handle_artifact(&artifacts_dir, path);
                    }
                    Some(LogCommand::Media { tag, step, extension, bytes }) => {
                        handle_media(&run_dir, &tag, step, &extension, &bytes);
                    }
                    Some(LogCommand::Histogram { tag, step, edges, counts }) => {
                        handle_histogram(&mut histogram_writer, &tag, step, &edges, &counts);
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
                        flush_vectors(&mut vector_writer, &mut vector_buffer);
                        flush_logs(&log_path, &mut log_lines);
                        let _ = reply.send(Ok(()));
                    }
                    Some(LogCommand::Shutdown { status, reply }) => {
                        // Final flush, then fold the append-only segments into
                        // a single Parquet so readers see one tidy file.
                        flush_vectors(&mut vector_writer, &mut vector_buffer);
                        flush_logs(&log_path, &mut log_lines);
                        let _ = vector_writer.finish();
                        let _ = system_writer.finish();
                        let _ = histogram_writer.finish();
                        storage::compact_run(&run_dir);

                        // Update run metadata with final status and latest scalars
                        let finished_at = Utc::now();
                        let duration = (finished_at - started_at).num_milliseconds() as f64 / 1000.0;

                        let _ = storage::update_run_metadata(&run_dir, |meta| {
                            meta.status = status;
                            meta.finished_at = Some(finished_at);
                            meta.duration_secs = Some(duration);
                            if !current_scalars.is_empty() {
                                meta.scalars = Some(current_scalars.clone());
                            }
                            if !current_vectors.is_empty() {
                                meta.vectors = Some(current_vectors.clone());
                            }
                        });

                        let _ = reply.send(());
                        break;
                    }
                }
            }

            // Periodic flush
            _ = flush_ticker.tick() => {
                if !vector_buffer.is_empty() {
                    flush_vectors(&mut vector_writer, &mut vector_buffer);
                }
                if !log_lines.is_empty() {
                    flush_logs(&log_path, &mut log_lines);
                }

                // Update metadata with current scalars periodically
                if !current_scalars.is_empty() || !current_vectors.is_empty() {
                    let _ = storage::update_run_metadata(&run_dir, |meta| {
                        meta.scalars = Some(current_scalars.clone());
                        meta.vectors = Some(current_vectors.clone());
                    });
                }
            }

            // Sample hardware utilisation. Subprocess probes take a few ms, so
            // this runs on the I/O task and never touches the training loop.
            _ = system_ticker.tick(), if system_enabled => {
                if let Some(sampler) = system_sampler.as_mut() {
                    let values = sampler.sample();
                    if !values.is_empty() {
                        let row = VectorRow::new(values, Some(system_step));
                        system_step += 1;
                        if let Err(e) = system_writer.append(std::slice::from_ref(&row)) {
                            error!("Failed to write system metrics: {}", e);
                        }
                    }
                }
            }

            // Heartbeat: prove the run is still alive so a hard kill is
            // distinguishable from a legitimately long job. `exp reap` reads this.
            _ = heartbeat_ticker.tick(), if heartbeat_enabled => {
                let _ = storage::update_run_metadata(&run_dir, |meta| {
                    if meta.status == RunStatus::Running {
                        meta.heartbeat_at = Some(Utc::now());
                    }
                });
            }
        }
    }
}

fn flush_vectors(writer: &mut storage::MetricWriter, buffer: &mut Vec<VectorRow>) {
    if buffer.is_empty() {
        return;
    }
    if let Err(e) = writer.append(buffer) {
        error!("Failed to flush vectors: {}", e);
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

/// Write a media blob and append a line to the media manifest.
///
/// Bytes go to a file rather than into Parquet: a Parquet column of image blobs
/// makes the metrics file unreadable for its actual purpose, and the dashboard
/// needs a URL it can hand to an `<img>` anyway.
fn handle_media(
    run_dir: &std::path::Path,
    tag: &str,
    step: Option<u64>,
    extension: &str,
    bytes: &[u8],
) {
    let media_dir = run_dir.join("media");
    if let Err(e) = fs::create_dir_all(&media_dir) {
        error!("Failed to create media dir: {}", e);
        return;
    }
    // Tags are user-supplied and routinely contain '/' ("train/samples"), which
    // would silently create directories or escape the run.
    let safe_tag: String = tag
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let filename = match step {
        Some(s) => format!("{safe_tag}-{s:08}.{extension}"),
        None => format!("{safe_tag}.{extension}"),
    };
    let path = media_dir.join(&filename);
    if let Err(e) = fs::write(&path, bytes) {
        error!("Failed to write media {}: {}", path.display(), e);
        return;
    }

    // A JSONL manifest, appended to: it survives a hard kill mid-run, which a
    // rewritten index would not.
    let entry = serde_json::json!({
        "tag": tag,
        "step": step,
        "file": format!("media/{filename}"),
        "bytes": bytes.len(),
        "logged_at": Utc::now().to_rfc3339(),
    });
    use std::io::Write;
    match fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(run_dir.join("media.jsonl"))
    {
        Ok(mut f) => {
            let _ = writeln!(f, "{entry}");
        }
        Err(e) => error!("Failed to update media manifest: {}", e),
    }
}

/// Store one histogram as a row: edges and counts as JSON-encoded strings.
///
/// Bin counts vary per tag and per step, so a column-per-bin schema would churn
/// the Parquet schema on every call.
fn handle_histogram(
    writer: &mut storage::MetricWriter,
    tag: &str,
    step: Option<u64>,
    edges: &[f64],
    counts: &[u64],
) {
    let mut values: HashMap<String, MetricValue> = HashMap::new();
    values.insert("tag".to_string(), MetricValue::Text(tag.to_string()));
    values.insert(
        "edges".to_string(),
        MetricValue::Text(serde_json::to_string(edges).unwrap_or_default()),
    );
    values.insert(
        "counts".to_string(),
        MetricValue::Text(serde_json::to_string(counts).unwrap_or_default()),
    );
    values.insert(
        "total".to_string(),
        MetricValue::Int(counts.iter().sum::<u64>() as i64),
    );
    let row = VectorRow::new(values, step);
    if let Err(e) = writer.append(std::slice::from_ref(&row)) {
        error!("Failed to write histogram: {}", e);
    }
}

fn handle_artifact(artifacts_dir: &std::path::Path, path: PathBuf) {
    // If path is absolute, join() will replace artifacts_dir.
    // We want to save the file into artifacts_dir, preserving its filename.
    let dest = if path.is_absolute() {
        if let Some(filename) = path.file_name() {
            artifacts_dir.join(filename)
        } else {
            error!("Invalid artifact path: {}", path.display());
            return;
        }
    } else {
        artifacts_dir.join(&path)
    };

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
