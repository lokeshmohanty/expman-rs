//! Storage layer: Parquet/Arrow IPC metrics, YAML config, file system management.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use arrow::array::{ArrayRef, Float64Array, Int64Array, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Utc};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::arrow_writer::ArrowWriter;
use parquet::file::properties::WriterProperties;
use serde_yaml;

use crate::core::error::Result;
use crate::core::models::{
    ExperimentMetadata, MetricValue, ProjectMetadata, RunMetadata, RunStatus, VectorRow,
};

// ─── Directory helpers ────────────────────────────────────────────────────────

pub fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    Ok(())
}

pub fn list_experiments(base_dir: &Path) -> Result<Vec<String>> {
    if !base_dir.exists() {
        return Ok(vec![]);
    }
    let mut names = vec![];
    for entry in fs::read_dir(base_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                if !name.starts_with('.') {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort();
    Ok(names)
}

pub fn list_runs(experiment_dir: &Path) -> Result<Vec<String>> {
    if !experiment_dir.exists() {
        return Ok(vec![]);
    }
    let mut names = vec![];
    for entry in fs::read_dir(experiment_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                // Avoid "artifacts" if we are inside a run, but here we are at experiment level.
                // However, an experiment folder contains runs (directories).
                // We should probably filter for directories that contain a run.yaml or metrics.parquet
                // But for now, just listing all dirs except maybe some reserved ones.
                if name != "artifacts" && name != ".ipynb_checkpoints" {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort_by(|a, b| b.cmp(a)); // newest first
    Ok(names)
}

pub fn list_artifacts(run_dir: &Path) -> Result<Vec<ArtifactInfo>> {
    let mut files = vec![];

    // 1. List default artifacts from run_dir root
    if run_dir.exists() {
        for entry in fs::read_dir(run_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                // Include specific default files
                if name == "vectors.parquet"
                    || name == "config.yaml"
                    || name == "run.yaml"
                    || name == "run.log"
                    || name == "console.log"
                {
                    let size = path.metadata()?.len();
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    files.push(ArtifactInfo {
                        path: name.to_string(),
                        name: name.to_string(),
                        size,
                        ext,
                        is_default: true,
                    });
                }
            }
        }
    }

    // 2. List user artifacts from artifacts/ subdir
    let artifacts_dir = run_dir.join("artifacts");
    if artifacts_dir.exists() {
        collect_files(&artifacts_dir, &artifacts_dir, &mut files)?;
    }

    Ok(files)
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<ArtifactInfo>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let size = path.metadata()?.len();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            out.push(ArtifactInfo {
                path: rel.to_string_lossy().to_string(),
                name: path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string(),
                size,
                ext,
                is_default: false,
            });
        }
    }
    Ok(())
}

/// The artifact listing shape is part of the HTTP contract, so it lives in
/// `core::dto` where the frontend can share it. Kept as an alias because the
/// storage layer is where it is produced.
pub use crate::core::dto::Artifact as ArtifactInfo;

// ─── YAML config I/O ─────────────────────────────────────────────────────────

/// Write YAML atomically: temp file in the same directory, then rename.
///
/// A plain `fs::write` truncates first, so any reader landing in that window
/// sees an empty or half-written file and falls back to `minimal_run_metadata`
/// — which reports the run as CRASHED. With the dashboard polling while the
/// engine writes every 500ms, that window gets hit.
pub fn save_yaml<T: serde::Serialize>(path: &Path, data: &T) -> Result<()> {
    let content = serde_yaml::to_string(data)?;
    let tmp = path.with_extension(format!(
        "{}.tmp{}",
        path.extension().and_then(|e| e.to_str()).unwrap_or("yaml"),
        std::process::id()
    ));
    fs::write(&tmp, content)?;
    // rename is atomic within a filesystem, which is guaranteed here since the
    // temp file is a sibling.
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

pub fn load_yaml<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T> {
    if !path.exists() {
        return Ok(T::default());
    }
    let content = fs::read_to_string(path)?;
    let val: T = serde_yaml::from_str(&content)?;
    Ok(val)
}

pub fn load_yaml_value(path: &Path) -> Result<serde_yaml::Value> {
    if !path.exists() {
        return Ok(serde_yaml::Value::Mapping(Default::default()));
    }
    let content = fs::read_to_string(path)?;
    Ok(serde_yaml::from_str::<serde_yaml::Value>(&content)?)
}

pub fn save_run_metadata(run_dir: &Path, meta: &RunMetadata) -> Result<()> {
    save_yaml(&run_dir.join("run.yaml"), meta)
}

pub fn load_run_metadata(run_dir: &Path) -> Result<RunMetadata> {
    let path = run_dir.join("run.yaml");
    if !path.exists() {
        return Ok(minimal_run_metadata(run_dir));
    }
    let content = std::fs::read_to_string(&path)?;
    match serde_yaml::from_str(&content) {
        Ok(meta) => Ok(meta),
        Err(_) => Ok(minimal_run_metadata(run_dir)),
    }
}

/// Memoised `run.yaml` reads, keyed on the file's identity rather than its path.
///
/// Profiled on 800 runs: `query_runs` took 149ms, of which **136ms was YAML
/// parsing** and 3ms was `stat`. Re-parsing a file that has not changed is
/// therefore ~90% of the cost of every dashboard poll.
///
/// This is deliberately *not* a SQLite index. An index is a second copy of the
/// truth that can go stale, needs a migration story, and adds a C dependency —
/// to solve a problem that is really just repeated parsing inside one
/// long-lived process. A memo keyed on (mtime, len) has none of that: it is
/// semantically invisible, and a cold process simply parses as before.
struct MetadataCache {
    entries: HashMap<std::path::PathBuf, (std::time::SystemTime, u64, RunMetadata)>,
}

/// Cap so a server watching a huge store cannot grow the cache without bound.
/// On overflow the whole map is dropped rather than evicted cleverly — this is
/// a memo, and rebuilding it costs exactly one uncached pass.
const METADATA_CACHE_CAPACITY: usize = 20_000;

fn metadata_cache() -> &'static std::sync::Mutex<MetadataCache> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<MetadataCache>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        std::sync::Mutex::new(MetadataCache {
            entries: HashMap::new(),
        })
    })
}

/// `load_run_metadata`, reusing a previous parse when the file is unchanged.
///
/// Used by the query and listing paths. **Not** used by `update_run_metadata`:
/// a read-modify-write must always see the current file, and mtime granularity
/// is too coarse to be trusted at the engine's 500ms write cadence.
pub fn load_run_metadata_cached(run_dir: &Path) -> Result<RunMetadata> {
    let path = run_dir.join("run.yaml");
    let Ok(meta) = fs::metadata(&path) else {
        // No file (or unreadable) — fall through to the uncached path, which
        // knows how to synthesise minimal metadata.
        return load_run_metadata(run_dir);
    };
    let Ok(mtime) = meta.modified() else {
        return load_run_metadata(run_dir);
    };
    let len = meta.len();

    if let Ok(cache) = metadata_cache().lock() {
        if let Some((cached_mtime, cached_len, cached)) = cache.entries.get(&path) {
            if *cached_mtime == mtime && *cached_len == len {
                return Ok(cached.clone());
            }
        }
    }

    let parsed = load_run_metadata(run_dir)?;
    if let Ok(mut cache) = metadata_cache().lock() {
        if cache.entries.len() >= METADATA_CACHE_CAPACITY {
            cache.entries.clear();
        }
        cache.entries.insert(path, (mtime, len, parsed.clone()));
    }
    Ok(parsed)
}

fn minimal_run_metadata(run_dir: &Path) -> RunMetadata {
    let name = run_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    let exp = run_dir
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    RunMetadata {
        name,
        experiment: exp,
        status: RunStatus::Crashed,
        started_at: Utc::now(),
        ..Default::default()
    }
}

/// Read-modify-write `run.yaml` under an exclusive advisory lock.
///
/// Every mutation of run metadata must go through this. Under DDP, N ranks share
/// one run directory and each ticks its own metadata update; a bare
/// load-mutate-save races, and the loser's fields — tags, scalars, the final
/// status — are silently dropped. The lock is advisory, so it only helps against
/// other expman processes, which is exactly the contention that occurs.
///
/// The closure should be short: it runs with the lock held.
pub fn update_run_metadata<F>(run_dir: &Path, mutate: F) -> Result<RunMetadata>
where
    F: FnOnce(&mut RunMetadata),
{
    use fs4::fs_std::FileExt;

    ensure_dir(run_dir)?;
    let lock_file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(run_dir.join(".run.lock"))?;
    lock_file.lock_exclusive()?;

    let result = (|| {
        let mut meta = load_run_metadata(run_dir)?;
        mutate(&mut meta);
        save_run_metadata(run_dir, &meta)?;
        Ok(meta)
    })();

    let _ = FileExt::unlock(&lock_file);
    result
}

pub fn save_experiment_metadata(exp_dir: &Path, meta: &ExperimentMetadata) -> Result<()> {
    save_yaml(&exp_dir.join("experiment.yaml"), meta)
}

pub fn load_experiment_metadata(exp_dir: &Path) -> Result<ExperimentMetadata> {
    load_yaml(&exp_dir.join("experiment.yaml"))
}

// ─── Project helpers ─────────────────────────────────────────────────────────

fn projects_dir(base_dir: &Path) -> std::path::PathBuf {
    base_dir.join(".projects")
}

pub fn list_projects(base_dir: &Path) -> Result<Vec<String>> {
    let dir = projects_dir(base_dir);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut names = vec![];
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}

pub fn load_project_metadata(base_dir: &Path, name: &str) -> Result<ProjectMetadata> {
    load_yaml(&projects_dir(base_dir).join(name).join("project.yaml"))
}

pub fn save_project_metadata(base_dir: &Path, name: &str, meta: &ProjectMetadata) -> Result<()> {
    let dir = projects_dir(base_dir).join(name);
    ensure_dir(&dir)?;
    save_yaml(&dir.join("project.yaml"), meta)
}

pub fn load_project_readme(base_dir: &Path, name: &str) -> Result<Option<String>> {
    let path = projects_dir(base_dir).join(name).join("README.md");
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(fs::read_to_string(&path)?))
}

pub fn save_project_readme(base_dir: &Path, name: &str, content: &str) -> Result<()> {
    let dir = projects_dir(base_dir).join(name);
    ensure_dir(&dir)?;
    fs::write(dir.join("README.md"), content)?;
    Ok(())
}

pub fn delete_project(base_dir: &Path, name: &str) -> Result<()> {
    let dir = projects_dir(base_dir).join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    // Unassign all experiments from this project
    let experiments = list_experiments(base_dir)?;
    for exp_name in &experiments {
        let exp_dir = base_dir.join(exp_name);
        let mut meta = load_experiment_metadata(&exp_dir)?;
        if meta.project.as_deref() == Some(name) {
            meta.project = None;
            save_experiment_metadata(&exp_dir, &meta)?;
        }
    }
    Ok(())
}

pub fn project_exists(base_dir: &Path, name: &str) -> bool {
    projects_dir(base_dir)
        .join(name)
        .join("project.yaml")
        .exists()
}

/// Assign an experiment to a project by rewriting only the `project:` field of
/// its `experiment.yaml`.
///
/// This is the offline half of the projects layer: it needs no server, so a
/// SLURM batch job or a tmux session can reach it. Passing `None` unassigns.
/// The experiment directory is created if absent so a project can be populated
/// before its first run exists.
pub fn set_experiment_project(
    base_dir: &Path,
    experiment: &str,
    project: Option<&str>,
) -> Result<()> {
    let exp_dir = base_dir.join(experiment);
    ensure_dir(&exp_dir)?;
    let mut meta = load_experiment_metadata(&exp_dir)?;
    meta.project = project.map(|p| p.to_string());
    save_experiment_metadata(&exp_dir, &meta)
}

/// List the experiments assigned to `project`.
pub fn list_project_experiments(base_dir: &Path, project: &str) -> Result<Vec<String>> {
    let mut out = vec![];
    for exp_name in list_experiments(base_dir)? {
        let exp_dir = base_dir.join(&exp_name);
        if load_experiment_metadata(&exp_dir)?.project.as_deref() == Some(project) {
            out.push(exp_name);
        }
    }
    Ok(out)
}

// ─── Run index and querying ──────────────────────────────────────────────────

/// One row of the cross-experiment run index.
///
/// This is the shared shape behind `exp list`, `GET /projects/{p}/runs`, and the
/// Python `load_runs()` — so all three agree on what a run *is* rather than each
/// re-deriving it from the directory tree.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RunEntry {
    pub run: String,
    pub experiment: String,
    pub project: Option<String>,
    pub group: Option<String>,
    pub rank: Option<u32>,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub heartbeat_at: Option<DateTime<Utc>>,
    pub duration_secs: Option<f64>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub scalars: HashMap<String, MetricValue>,
    pub vectors: HashMap<String, MetricValue>,
    pub path: String,
}

/// Filters applied when building a run index. All present filters must match.
#[derive(Debug, Clone, Default)]
pub struct RunQuery {
    pub project: Option<String>,
    pub experiment: Option<String>,
    /// Only runs in this group (a DDP job or a sweep cohort).
    pub group: Option<String>,
    pub status: Option<RunStatus>,
    /// Tag expression: outer Vec is AND, inner Vec is OR.
    ///
    /// `[["arm:tiered"], ["study:1", "study:2"]]` means
    /// `arm:tiered AND (study:1 OR study:2)`.
    pub tags: Vec<Vec<String>>,
}

impl RunQuery {
    fn matches(&self, entry: &RunEntry) -> bool {
        if let Some(p) = &self.project {
            if entry.project.as_deref() != Some(p.as_str()) {
                return false;
            }
        }
        if let Some(e) = &self.experiment {
            if &entry.experiment != e {
                return false;
            }
        }
        if let Some(g) = &self.group {
            if entry.group.as_deref() != Some(g.as_str()) {
                return false;
            }
        }
        if let Some(s) = &self.status {
            if entry.status != s.to_string() {
                return false;
            }
        }
        self.tags
            .iter()
            .all(|clause| clause.iter().any(|t| entry.tags.iter().any(|et| et == t)))
    }
}

/// Parse a tag expression such as `arm:tiered AND (study:1 OR study:2)`.
///
/// Grammar kept deliberately small: clauses separated by `AND` (or `,`), each
/// clause a set of alternatives separated by `OR` (or `|`), optionally wrapped
/// in parentheses. Case-insensitive on the operators, never on the tags.
pub fn parse_tag_expr(expr: &str) -> Vec<Vec<String>> {
    let normalized = expr.replace(',', " AND ").replace('|', " OR ");
    let mut clauses = vec![];
    for raw_clause in split_keyword(&normalized, "AND") {
        let cleaned = raw_clause
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')');
        let alternatives: Vec<String> = split_keyword(cleaned, "OR")
            .into_iter()
            .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !alternatives.is_empty() {
            clauses.push(alternatives);
        }
    }
    clauses
}

/// Split on a bare keyword, case-insensitively, requiring whitespace around it
/// so a tag like `brand:ORACLE` is not torn in half by `OR`.
fn split_keyword(input: &str, keyword: &str) -> Vec<String> {
    let mut parts = vec![];
    let mut current = String::new();
    for token in input.split_whitespace() {
        if token.eq_ignore_ascii_case(keyword) {
            parts.push(std::mem::take(&mut current));
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(token);
        }
    }
    parts.push(current);
    parts.into_iter().filter(|p| !p.trim().is_empty()).collect()
}

/// Build the run index across the whole store, applying `query`.
///
/// Results are newest-first by `started_at`.
pub fn query_runs(base_dir: &Path, query: &RunQuery) -> Result<Vec<RunEntry>> {
    let mut out = vec![];
    // Resolve each experiment's project once rather than per run.
    let experiments = match &query.experiment {
        Some(e) => vec![e.clone()],
        None => list_experiments(base_dir)?,
    };

    for exp_name in experiments {
        let exp_dir = base_dir.join(&exp_name);
        if !exp_dir.is_dir() {
            continue;
        }
        let project = load_experiment_metadata(&exp_dir)
            .unwrap_or_default()
            .project;
        // Cheap rejection: skip the whole experiment when the project cannot match.
        if let Some(want) = &query.project {
            if project.as_deref() != Some(want.as_str()) {
                continue;
            }
        }

        for run_name in list_runs(&exp_dir)? {
            let dir = exp_dir.join(&run_name);
            let meta = load_run_metadata_cached(&dir)?;
            let entry = RunEntry {
                run: run_name,
                experiment: exp_name.clone(),
                project: project.clone(),
                group: meta.group.clone(),
                rank: meta.rank,
                status: meta.status.to_string(),
                started_at: meta.started_at,
                finished_at: meta.finished_at,
                heartbeat_at: meta.heartbeat_at,
                duration_secs: meta.duration_secs,
                description: meta.description,
                tags: meta.tags.unwrap_or_default(),
                scalars: meta.scalars.unwrap_or_default(),
                vectors: meta.vectors.unwrap_or_default(),
                path: dir.to_string_lossy().to_string(),
            };
            if query.matches(&entry) {
                out.push(entry);
            }
        }
    }

    // Newest first.
    out.sort_by_key(|entry| std::cmp::Reverse(entry.started_at));
    Ok(out)
}

/// How long a heartbeat may go unheard before a run is presumed dead.
///
/// Ten times the 30s default interval, so an I/O stall or a loaded node does not
/// produce a false positive.
pub const HEARTBEAT_STALE_AFTER: i64 = 300;

/// How long a *heartbeat-less* run may run before it is presumed dead.
///
/// Runs written before heartbeats existed have nothing but `started_at`, which
/// says nothing about liveness. A day is deliberately generous: overcounting a
/// dead run is a cosmetic error, while calling a live multi-hour job dead is a
/// misleading one.
pub const NO_HEARTBEAT_STALE_AFTER: i64 = 86_400;

/// Whether a `RUNNING` run still looks alive, under the dashboard's policy.
///
/// This is a *presumption*, not a fact — it never mutates anything. `exp reap`
/// is what actually rewrites status, and it takes its threshold from the user.
pub fn looks_alive(meta: &RunMetadata, now: DateTime<Utc>) -> bool {
    if meta.status != RunStatus::Running {
        return false;
    }
    let (last_seen, max_age) = match meta.heartbeat_at {
        Some(hb) => (hb, HEARTBEAT_STALE_AFTER),
        None => (meta.started_at, NO_HEARTBEAT_STALE_AFTER),
    };
    now.signed_duration_since(last_seen).num_seconds() <= max_age
}

/// True when a `RUNNING` run has not been heard from within `max_age`.
///
/// Falls back to `started_at` for runs written before heartbeats existed, which
/// is the conservative choice: such a run is only reaped once it is older than
/// the threshold, never sooner.
pub fn is_run_stale(meta: &RunMetadata, max_age: chrono::Duration, now: DateTime<Utc>) -> bool {
    if meta.status != RunStatus::Running {
        return false;
    }
    let last_seen = meta.heartbeat_at.unwrap_or(meta.started_at);
    now.signed_duration_since(last_seen) > max_age
}

// ─── Parquet metrics I/O ─────────────────────────────────────────────────────

// ─── Append-only segment writing ──────────────────────────────────────────────
//
// A run's metrics are written as append-only Arrow IPC **segments** while it is
// live, then compacted into a single Parquet file when it closes.
//
// The obvious alternative — rewriting the Parquet on every flush — is what this
// replaced. That made total write volume grow with the *square* of the step
// count: a 10k-step run at the default 50-row flush rewrote the whole file 200
// times. Appending keeps each flush proportional to the rows in it.
//
// One IPC stream carries one schema, so a metric first logged mid-run rolls a
// new segment rather than invalidating the stream. Readers union the segments,
// and a segment truncated by a hard kill still yields every batch written
// before it — which is strictly better than the old scheme, where a kill
// mid-rewrite could leave the file short.

/// Metric family written by `log_vector`.
pub const VECTORS_STEM: &str = "vectors";
/// Metric family written by the system-metrics sampler.
pub const SYSTEM_STEM: &str = "system";
/// Metric family written by `log_histogram`.
pub const HISTOGRAM_STEM: &str = "histograms";

pub fn metrics_parquet_path(run_dir: &Path, stem: &str) -> std::path::PathBuf {
    run_dir.join(format!("{stem}.parquet"))
}

fn segment_path(run_dir: &Path, stem: &str, seq: usize) -> std::path::PathBuf {
    run_dir.join(format!("{stem}-{seq:04}.arrow"))
}

/// Segment files for `stem`, oldest first.
fn segment_files(run_dir: &Path, stem: &str) -> Result<Vec<(usize, std::path::PathBuf)>> {
    let mut out = vec![];
    if !run_dir.exists() {
        return Ok(out);
    }
    let prefix = format!("{stem}-");
    for entry in fs::read_dir(run_dir)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(rest) = name.strip_prefix(&prefix) else {
            continue;
        };
        let Some(seq) = rest.strip_suffix(".arrow").and_then(|s| s.parse().ok()) else {
            continue;
        };
        out.push((seq, path));
    }
    out.sort_by_key(|(seq, _)| *seq);
    Ok(out)
}

/// Append-only writer for one metric family within a run.
pub struct MetricWriter {
    run_dir: std::path::PathBuf,
    stem: String,
    seq: usize,
    schema: Option<Arc<Schema>>,
    writer: Option<arrow::ipc::writer::StreamWriter<std::io::BufWriter<fs::File>>>,
}

impl MetricWriter {
    pub fn new(run_dir: &Path, stem: &str) -> Self {
        // Resume numbering past any segments already on disk, so re-opening a
        // run directory appends rather than overwriting.
        let seq = segment_files(run_dir, stem)
            .ok()
            .and_then(|s| s.last().map(|(seq, _)| seq + 1))
            .unwrap_or(0);
        Self {
            run_dir: run_dir.to_path_buf(),
            stem: stem.to_string(),
            seq,
            schema: None,
            writer: None,
        }
    }

    /// Append rows. Cost is proportional to `rows`, not to the run's history.
    pub fn append(&mut self, rows: &[VectorRow]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let batch = rows_to_record_batch(rows)?;
        let needs_roll = match &self.schema {
            Some(current) => current.fields() != batch.schema().fields(),
            None => true,
        };
        if needs_roll {
            self.roll(batch.schema())?;
        }
        if let Some(writer) = self.writer.as_mut() {
            writer.write(&batch)?;
            // Flush per batch: an unflushed buffer is data a `kill -9` loses,
            // and losing metrics is exactly what this file exists to prevent.
            writer.flush()?;
        }
        Ok(())
    }

    /// Close the current segment and open a new one for `schema`.
    fn roll(&mut self, schema: Arc<Schema>) -> Result<()> {
        self.finish()?;
        let path = segment_path(&self.run_dir, &self.stem, self.seq);
        self.seq += 1;
        let file = fs::File::create(&path)?;
        self.writer = Some(arrow::ipc::writer::StreamWriter::try_new(
            std::io::BufWriter::new(file),
            &schema,
        )?);
        self.schema = Some(schema);
        Ok(())
    }

    /// Finish the open segment, writing the end-of-stream marker.
    pub fn finish(&mut self) -> Result<()> {
        if let Some(mut writer) = self.writer.take() {
            writer.finish()?;
        }
        self.schema = None;
        Ok(())
    }
}

impl Drop for MetricWriter {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

/// Read every batch of every segment for `stem`.
///
/// A segment whose tail was truncated by a hard kill yields the batches that
/// were completely written and stops — partial data beats no data.
fn read_segment_batches(run_dir: &Path, stem: &str) -> Result<Vec<RecordBatch>> {
    let mut out = vec![];
    for (_, path) in segment_files(run_dir, stem)? {
        let Ok(file) = fs::File::open(&path) else {
            continue;
        };
        // A missing/short header means nothing was ever committed here.
        let Ok(reader) = arrow::ipc::reader::StreamReader::try_new(file, None) else {
            continue;
        };
        for batch in reader {
            match batch {
                Ok(batch) => out.push(batch),
                Err(_) => break,
            }
        }
    }
    Ok(out)
}

/// All rows for a metric family: the compacted Parquet plus any live segments.
pub fn read_metrics(run_dir: &Path, stem: &str) -> Result<Vec<HashMap<String, serde_json::Value>>> {
    let mut rows = vec![];
    let parquet = metrics_parquet_path(run_dir, stem);
    if parquet.exists() {
        rows.extend(record_batch_to_rows(&read_parquet(&parquet)?)?);
    }
    for batch in read_segment_batches(run_dir, stem)? {
        rows.extend(record_batch_to_rows(&batch)?);
    }
    Ok(merge_rows_by_step(rows))
}

/// Convenience for the common case.
pub fn read_run_vectors(run_dir: &Path) -> Result<Vec<HashMap<String, serde_json::Value>>> {
    read_metrics(run_dir, VECTORS_STEM)
}

/// A run's vectors past `since_step`, for live streaming.
pub fn read_run_vectors_since(
    run_dir: &Path,
    since_step: Option<u64>,
) -> Result<Vec<HashMap<String, serde_json::Value>>> {
    let all = read_run_vectors(run_dir)?;
    let Some(since) = since_step else {
        return Ok(all);
    };
    Ok(all
        .into_iter()
        .filter(|row| {
            row.get("step")
                .and_then(|v| v.as_u64())
                .map(|s| s > since)
                .unwrap_or(true)
        })
        .collect())
}

/// The latest numeric value of every metric in a run.
///
/// Reads segments as well as the compacted Parquet, so a **live** run reports
/// its current values rather than nothing until it closes.
pub fn read_run_latest_scalars(run_dir: &Path) -> Result<HashMap<String, f64>> {
    let Some(last) = read_run_vectors(run_dir)?.into_iter().last() else {
        return Ok(HashMap::new());
    };
    Ok(last
        .into_iter()
        .filter(|(k, _)| k != "step" && k != "timestamp")
        .filter_map(|(k, v)| v.as_f64().map(|f| (k, f)))
        .collect())
}

/// True when a run has any metrics at all, compacted or not.
pub fn has_metrics(run_dir: &Path, stem: &str) -> bool {
    metrics_parquet_path(run_dir, stem).exists()
        || segment_files(run_dir, stem)
            .map(|s| !s.is_empty())
            .unwrap_or(false)
}

/// Collapse rows sharing a step into one, later values winning.
///
/// `log_vector({"loss": ...}, step=1)` followed by
/// `log_vector({"acc": ...}, step=1)` is one row with both keys. Nulls never
/// overwrite a real value — a null here means "this metric was absent from that
/// batch", not "it became null".
fn merge_rows_by_step(
    rows: Vec<HashMap<String, serde_json::Value>>,
) -> Vec<HashMap<String, serde_json::Value>> {
    let mut out: Vec<HashMap<String, serde_json::Value>> = Vec::with_capacity(rows.len());
    let mut by_step: HashMap<i64, usize> = HashMap::new();

    for row in rows {
        let step = row.get("step").and_then(|v| v.as_i64());
        match step.and_then(|s| by_step.get(&s).copied()) {
            Some(idx) => {
                for (key, value) in row {
                    if value.is_null() {
                        continue;
                    }
                    out[idx].insert(key, value);
                }
            }
            None => {
                if let Some(s) = step {
                    by_step.insert(s, out.len());
                }
                out.push(row);
            }
        }
    }
    out
}

/// Fold a run's segments into its Parquet file and delete them.
///
/// Idempotent and crash-safe by construction: the Parquet is written before the
/// segments are removed, so an interruption leaves both, and readers union them
/// to the same result.
pub fn compact_metrics(run_dir: &Path, stem: &str) -> Result<()> {
    let segments = segment_files(run_dir, stem)?;
    if segments.is_empty() {
        return Ok(());
    }
    let rows = read_metrics(run_dir, stem)?;
    if !rows.is_empty() {
        let batch = rows_to_record_batch(&json_rows_to_vector_rows(rows))?;
        write_parquet(&metrics_parquet_path(run_dir, stem), &batch)?;
    }
    for (_, path) in segments {
        let _ = fs::remove_file(path);
    }
    Ok(())
}

/// Turn read-back JSON rows into `VectorRow`s so they can be re-serialized.
fn json_rows_to_vector_rows(rows: Vec<HashMap<String, serde_json::Value>>) -> Vec<VectorRow> {
    rows.into_iter()
        .map(|mut row| {
            let step = row.remove("step").and_then(|v| v.as_u64());
            let timestamp = row
                .remove("timestamp")
                .and_then(|v| {
                    v.as_str()
                        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                })
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now);
            let values = row
                .into_iter()
                .filter_map(|(k, v)| json_to_metric_value(v).map(|mv| (k, mv)))
                .collect();
            VectorRow {
                step,
                timestamp,
                values,
            }
        })
        .collect()
}

fn json_to_metric_value(value: serde_json::Value) -> Option<MetricValue> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::Bool(b) => Some(MetricValue::Bool(b)),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(MetricValue::Int)
            .or_else(|| n.as_f64().map(MetricValue::Float)),
        serde_json::Value::String(s) => Some(MetricValue::Text(s)),
        other => Some(MetricValue::Text(other.to_string())),
    }
}

/// Append vector rows to a Parquet file.
///
/// Read existing → concat → write back, so cost is proportional to the file, not
/// the appended rows. Retained for **one-shot** writes such as `exp import`,
/// where the whole run arrives at once. Live runs use [`MetricWriter`].
pub fn append_vectors(path: &Path, rows: &[VectorRow]) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }

    // Deduplicate and merge rows by step (if any have the same step)
    let mut merged_rows: Vec<VectorRow> = Vec::new();
    let mut step_to_idx: HashMap<u64, usize> = HashMap::new();
    for row in rows {
        if let Some(s) = row.step {
            if let Some(&idx) = step_to_idx.get(&s) {
                for (k, v) in &row.values {
                    let k: String = k.clone();
                    let v: MetricValue = v.clone();
                    merged_rows[idx].values.insert(k, v);
                }
                merged_rows[idx].timestamp = row.timestamp;
            } else {
                step_to_idx.insert(s, merged_rows.len());
                merged_rows.push(row.clone());
            }
        } else {
            merged_rows.push(row.clone());
        }
    }

    // Build new batch from merged rows
    let new_batch = rows_to_record_batch(&merged_rows)?;

    // If file exists, read and concat
    let final_batch = if path.exists() {
        let existing = read_parquet(path)?;

        let new_steps: std::collections::HashSet<i64> = merged_rows
            .iter()
            .filter_map(|r| r.step)
            .map(|s| s as i64)
            .collect();

        if new_steps.is_empty() || existing.num_rows() == 0 {
            concat_batches(&existing, &new_batch)?
        } else {
            let step_col = existing.column_by_name("step");
            let existing_filtered = if let Some(col) = step_col {
                if let Some(step_arr) = col.as_any().downcast_ref::<arrow::array::UInt64Array>() {
                    use arrow::array::Array;
                    let mut keep_vec = Vec::with_capacity(existing.num_rows());
                    for i in 0..existing.num_rows() {
                        if step_arr.is_null(i) {
                            keep_vec.push(Some(true));
                        } else {
                            let s: u64 = step_arr.value(i);
                            keep_vec.push(Some(!new_steps.contains(&(s as i64))));
                        }
                    }
                    use arrow::array::BooleanArray;
                    let keep_array = BooleanArray::from(keep_vec);
                    arrow::compute::filter_record_batch(&existing, &keep_array)?
                } else {
                    existing
                }
            } else {
                existing
            };
            concat_batches(&existing_filtered, &new_batch)?
        }
    } else {
        new_batch
    };

    write_parquet(path, &final_batch)?;
    Ok(())
}

/// Read all vectors from a Parquet file as a list of row maps.
pub fn read_vectors(path: &Path) -> Result<Vec<HashMap<String, serde_json::Value>>> {
    if !path.exists() {
        return Ok(vec![]);
    }
    let batch = read_parquet(path)?;
    record_batch_to_rows(&batch)
}

/// Read vectors since a given step (for live streaming).
pub fn read_vectors_since(
    path: &Path,
    since_step: Option<u64>,
) -> Result<Vec<HashMap<String, serde_json::Value>>> {
    let all = read_vectors(path)?;
    if let Some(since) = since_step {
        Ok(all
            .into_iter()
            .filter(|row: &HashMap<String, serde_json::Value>| {
                row.get("step")
                    .and_then(|v: &serde_json::Value| v.as_u64())
                    .map(|s| s > since)
                    .unwrap_or(true)
            })
            .collect())
    } else {
        Ok(all)
    }
}

/// Read the latest row of a parquet file and return only numeric (scalar) columns as f64.
/// Non-numeric columns (step, timestamp, strings) are silently skipped.
pub fn read_latest_scalar_metrics(path: &Path) -> Result<HashMap<String, f64>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let rows = read_vectors(path)?;
    let Some(last) = rows.into_iter().last() else {
        return Ok(HashMap::new());
    };
    let last: HashMap<String, serde_json::Value> = last;
    let scalars = last
        .into_iter()
        .filter(|(k, _)| k != "step" && k != "timestamp")
        .filter_map(|(k, v)| v.as_f64().map(|f| (k, f)))
        .collect();
    Ok(scalars)
}

fn read_parquet(path: &Path) -> Result<RecordBatch> {
    let file = fs::File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let mut reader = builder.build()?;
    let mut batches: Vec<RecordBatch> = vec![];
    for batch in &mut reader {
        batches.push(batch?);
    }
    if batches.is_empty() {
        // Return empty batch with default schema
        let schema = Arc::new(Schema::new(vec![
            Field::new("step", DataType::Int64, true),
            Field::new(
                "timestamp",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
        ]));
        return Ok(RecordBatch::new_empty(schema));
    }
    if batches.len() == 1 {
        return Ok(batches.remove(0));
    }
    // Concat multiple batches
    let schema = batches[0].schema();
    Ok(arrow::compute::concat_batches(&schema, &batches)?)
}

fn write_parquet(path: &Path, batch: &RecordBatch) -> Result<()> {
    let file = fs::File::create(path)?;
    let props = WriterProperties::builder()
        .set_compression(parquet::basic::Compression::SNAPPY)
        .build();
    let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(props))?;
    writer.write(batch)?;
    writer.close()?;
    Ok(())
}

fn concat_batches(existing: &RecordBatch, new: &RecordBatch) -> Result<RecordBatch> {
    // Merge schemas: new batch may have columns not in existing (diagonal concat)
    let merged_schema = merge_schemas(existing.schema_ref(), new.schema_ref());
    let merged_schema = Arc::new(merged_schema);

    let existing_aligned = align_batch(existing, &merged_schema)?;
    let new_aligned = align_batch(new, &merged_schema)?;

    Ok(arrow::compute::concat_batches(
        &merged_schema,
        &[existing_aligned, new_aligned],
    )?)
}

fn merge_schemas(a: &Schema, b: &Schema) -> Schema {
    let mut fields: Vec<Field> = a
        .fields()
        .iter()
        .map(|f: &Arc<Field>| f.as_ref().clone())
        .collect();
    for field in b.fields() {
        let name: &String = field.name();
        if a.field_with_name(name).is_err() {
            fields.push(field.as_ref().clone());
        }
    }
    Schema::new(fields)
}

fn align_batch(batch: &RecordBatch, target_schema: &Schema) -> Result<RecordBatch> {
    let n = batch.num_rows();
    let mut columns: Vec<ArrayRef> = vec![];

    for field in target_schema.fields() {
        let name: &String = field.name();
        if let Some(col) = batch.column_by_name(name) {
            columns.push(col.clone());
        } else {
            // Fill missing column with nulls
            let null_array: ArrayRef = match field.data_type() {
                DataType::Float64 => Arc::new(Float64Array::from(vec![None::<f64>; n])),
                DataType::Int64 => Arc::new(Int64Array::from(vec![None::<i64>; n])),
                DataType::Timestamp(TimeUnit::Microsecond, _) => Arc::new(
                    TimestampMicrosecondArray::from(vec![None::<i64>; n])
                        .with_timezone_opt(Some("UTC".to_string())),
                ),
                _ => Arc::new(StringArray::from(vec![None::<&str>; n])),
            };
            columns.push(null_array);
        }
    }

    Ok(RecordBatch::try_new(
        Arc::new(target_schema.clone()),
        columns,
    )?)
}

fn rows_to_record_batch(rows: &[VectorRow]) -> Result<RecordBatch> {
    if rows.is_empty() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("step", DataType::Int64, true),
            Field::new(
                "timestamp",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
        ]));
        return Ok(RecordBatch::new_empty(schema));
    }

    // Collect all unique metric keys across all rows
    let mut all_keys: Vec<String> = vec![];
    for row in rows {
        for key in row.values.keys() {
            if !all_keys.contains(key) {
                all_keys.push(key.clone());
            }
        }
    }

    let _n = rows.len();

    // Build columns
    let mut fields = vec![
        Field::new("step", DataType::Int64, true),
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
    ];
    let mut arrays: Vec<ArrayRef> = vec![];

    // step column
    let steps: Vec<Option<i64>> = rows.iter().map(|r| r.step.map(|s| s as i64)).collect();
    arrays.push(Arc::new(Int64Array::from(steps)));

    // timestamp column (microseconds since epoch UTC)
    let timestamps: Vec<Option<i64>> = rows
        .iter()
        .map(|r| Some(r.timestamp.timestamp_micros()))
        .collect();
    arrays.push(Arc::new(
        TimestampMicrosecondArray::from(timestamps).with_timezone_opt(Some("UTC".to_string())),
    ));

    // metric value columns
    for key in &all_keys {
        // Determine type from first non-null value
        let first_val = rows.iter().find_map(|r| r.values.get(key));
        match first_val {
            Some(MetricValue::Float(_)) | Some(MetricValue::Int(_)) => {
                // Store as Float64 for simplicity
                let vals: Vec<Option<f64>> = rows
                    .iter()
                    .map(|r| match r.values.get(key) {
                        Some(MetricValue::Float(f)) => Some(*f),
                        Some(MetricValue::Int(i)) => Some(*i as f64),
                        _ => None,
                    })
                    .collect();
                fields.push(Field::new(key, DataType::Float64, true));
                arrays.push(Arc::new(Float64Array::from(vals)));
            }
            _ => {
                // Store as Utf8
                let vals: Vec<Option<String>> = rows
                    .iter()
                    .map(|r| match r.values.get(key) {
                        Some(MetricValue::Text(s)) => Some(s.clone()),
                        Some(MetricValue::Bool(b)) => Some(b.to_string()),
                        Some(MetricValue::Float(f)) => Some(f.to_string()),
                        Some(MetricValue::Int(i)) => Some(i.to_string()),
                        None => None,
                    })
                    .collect();
                fields.push(Field::new(key, DataType::Utf8, true));
                arrays.push(Arc::new(StringArray::from(vals)));
            }
        }
    }

    let schema = Arc::new(Schema::new(fields));
    Ok(RecordBatch::try_new(schema, arrays)?)
}

fn record_batch_to_rows(batch: &RecordBatch) -> Result<Vec<HashMap<String, serde_json::Value>>> {
    let schema = batch.schema();
    let n = batch.num_rows();
    let mut rows = vec![HashMap::new(); n];

    for (col_idx, field) in schema.fields().iter().enumerate() {
        let col = batch.column(col_idx);
        let name: String = field.name().clone();

        for (row_idx, row) in rows.iter_mut().enumerate().take(n) {
            use arrow::array::Array;
            if col.is_null(row_idx) {
                row.insert(name.clone(), serde_json::Value::Null);
                continue;
            }
            let val = match field.data_type() {
                DataType::Float64 => {
                    let arr = col.as_any().downcast_ref::<Float64Array>().unwrap();
                    let f = arr.value(row_idx);
                    if f.is_nan() || f.is_infinite() {
                        serde_json::Value::Null
                    } else {
                        serde_json::json!(f)
                    }
                }
                DataType::Int64 => {
                    let arr = col.as_any().downcast_ref::<Int64Array>().unwrap();
                    serde_json::json!(arr.value(row_idx))
                }
                DataType::Timestamp(TimeUnit::Microsecond, _) => {
                    let arr = col
                        .as_any()
                        .downcast_ref::<TimestampMicrosecondArray>()
                        .unwrap();
                    let micros = arr.value(row_idx);
                    let dt = DateTime::<Utc>::from_timestamp_micros(micros).unwrap_or_default();
                    serde_json::json!(dt.to_rfc3339())
                }
                DataType::Utf8 => {
                    let arr = col.as_any().downcast_ref::<StringArray>().unwrap();
                    serde_json::json!(arr.value(row_idx))
                }
                _ => serde_json::Value::Null,
            };
            let k_str: String = name.clone();
            let v_val: serde_json::Value = val.clone();
            row.insert(k_str, v_val);
        }
    }

    Ok(rows)
}

// ─── Downsampling ─────────────────────────────────────────────────────────────

/// Reduce `rows` to at most `max_points`, preserving shape.
///
/// A long run has more steps than a chart has pixels, and serialising a million
/// rows to JSON hangs the browser tab long before it draws anything. Every
/// plotting tool downsamples; doing it server-side means the payload is bounded
/// too.
///
/// The algorithm is **Largest-Triangle-Three-Buckets** on the first numeric
/// column, which is what makes this safe: naive stride sampling drops the very
/// points a user is looking for — the loss spike, the divergence — because they
/// are single rows between strides. LTTB keeps whichever point in each bucket
/// contributes most visible area, so extremes survive.
///
/// The first and last rows are always kept, so a chart's endpoints are exact.
pub fn downsample_rows(
    rows: Vec<HashMap<String, serde_json::Value>>,
    max_points: usize,
) -> Vec<HashMap<String, serde_json::Value>> {
    if max_points < 3 || rows.len() <= max_points {
        return rows;
    }

    // Pick the column to preserve the shape of: the first non-step, non-timestamp
    // numeric key, chosen by name so the result is deterministic across calls.
    let mut candidates: Vec<&String> = rows
        .first()
        .map(|r| {
            r.iter()
                .filter(|(k, v)| *k != "step" && *k != "timestamp" && v.is_number())
                .map(|(k, _)| k)
                .collect()
        })
        .unwrap_or_default();
    candidates.sort();
    let Some(key) = candidates.first().map(|k| (*k).clone()) else {
        // Nothing numeric to preserve the shape of — take a plain stride.
        let stride = rows.len().div_ceil(max_points);
        return rows.into_iter().step_by(stride).collect();
    };

    let value_at = |row: &HashMap<String, serde_json::Value>| -> f64 {
        row.get(&key).and_then(|v| v.as_f64()).unwrap_or(0.0)
    };

    let n = rows.len();
    let bucket_size = (n - 2) as f64 / (max_points - 2) as f64;
    let mut out: Vec<HashMap<String, serde_json::Value>> = Vec::with_capacity(max_points);
    out.push(rows[0].clone());

    let mut prev_idx = 0usize;
    for bucket in 0..(max_points - 2) {
        let start = ((bucket as f64) * bucket_size).floor() as usize + 1;
        let end = (((bucket + 1) as f64) * bucket_size).floor() as usize + 1;
        let end = end.min(n - 1);
        if start >= end {
            continue;
        }

        // Average of the *next* bucket forms the third triangle vertex.
        let next_start = end;
        let next_end = ((((bucket + 2) as f64) * bucket_size).floor() as usize + 1).min(n);
        let (mut avg_x, mut avg_y, mut count) = (0.0f64, 0.0f64, 0usize);
        for row in rows.iter().take(next_end).skip(next_start) {
            avg_x += count as f64;
            avg_y += value_at(row);
            count += 1;
        }
        if count == 0 {
            continue;
        }
        avg_x = next_start as f64 + avg_x / count as f64;
        avg_y /= count as f64;

        let prev_x = prev_idx as f64;
        let prev_y = value_at(&rows[prev_idx]);

        let mut best_idx = start;
        let mut best_area = -1.0f64;
        for (offset, row) in rows.iter().take(end).skip(start).enumerate() {
            let x = (start + offset) as f64;
            let area = ((prev_x - avg_x) * (value_at(row) - prev_y)
                - (prev_x - x) * (avg_y - prev_y))
                .abs();
            if area > best_area {
                best_area = area;
                best_idx = start + offset;
            }
        }
        out.push(rows[best_idx].clone());
        prev_idx = best_idx;
    }

    out.push(rows[n - 1].clone());
    out
}
