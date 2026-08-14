#![doc = include_str!("../README.md")]
//! PyO3 Python extension module for expman-rs.
#![allow(clippy::useless_conversion)]
//!
//! Exposes `Experiment` class to Python. All I/O is non-blocking:
//! `log_vector()` is a channel send on the background tokio runtime,
//! never blocking the Python GIL or the experiment loop.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::core::{ExperimentConfig, LogLevel, LoggingEngine, MetricValue, RunStatus};

/// Python-facing Experiment class.
#[pyclass]
struct Experiment {
    engine: Arc<Mutex<Option<LoggingEngine>>>,
}

#[pymethods]
impl Experiment {
    /// Create a new experiment run.
    ///
    /// Args:
    ///     name: Experiment name (e.g. "resnet_cifar10")
    ///     run_name: Optional run name. Auto-generated from timestamp if None.
    ///     base_dir: Root directory for experiments. Default: "experiments"
    ///     flush_interval_rows: Flush metrics every N rows. Default: 50
    ///     flush_interval_ms: Flush metrics every N milliseconds. Default: 500
    ///     project: Project this experiment belongs to. Written to experiment.yaml
    ///              offline — no server required.
    ///     tags: Tags for this run, written to run.yaml at creation.
    ///     description: Description for this run, written to run.yaml at creation.
    ///     heartbeat_interval_secs: Seconds between liveness heartbeats. 0 disables.
    #[new]
    #[pyo3(signature = (name, run_name=None, base_dir="experiments", flush_interval_rows=50, flush_interval_ms=500, project=None, tags=None, description=None, heartbeat_interval_secs=30, group=None, rank=None, system_metrics_interval_secs=15, capture_provenance=true, capture_diff=false))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        name: &str,
        run_name: Option<&str>,
        base_dir: &str,
        flush_interval_rows: usize,
        flush_interval_ms: u64,
        project: Option<String>,
        tags: Option<Vec<String>>,
        description: Option<String>,
        heartbeat_interval_secs: u64,
        group: Option<String>,
        rank: Option<u32>,
        system_metrics_interval_secs: u64,
        capture_provenance: bool,
        capture_diff: bool,
    ) -> PyResult<Self> {
        let mut config = ExperimentConfig::new(name, base_dir);
        config.flush_interval_rows = flush_interval_rows;
        config.flush_interval_ms = flush_interval_ms;
        config.language = "python".to_string();
        config.project = project;
        config.tags = tags.unwrap_or_default();
        config.description = description;
        config.heartbeat_interval_secs = heartbeat_interval_secs;
        config.group = group;
        config.rank = rank;
        config.system_metrics_interval_secs = system_metrics_interval_secs;
        config.capture_provenance = capture_provenance;
        config.capture_diff = capture_diff;

        if let Ok(sys) = py.import("sys") {
            if let Ok(exec_obj) = sys.getattr("executable") {
                if let Ok(executable) = exec_obj.extract::<String>() {
                    config.env_path = Some(executable);
                }
            }
        }
        if let Some(rn) = run_name {
            config = config.with_run_name(rn);
        }

        let engine = LoggingEngine::new(config)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        Ok(Self {
            engine: Arc::new(Mutex::new(Some(engine))),
        })
    }

    /// Log hyperparameters/configuration. Non-blocking.
    ///
    /// Args:
    ///     params: Dict of parameter name → value (str, int, float, bool)
    fn log_params(&self, params: &Bound<'_, PyDict>) -> PyResult<()> {
        let converted = py_dict_to_yaml(params)?;
        if let Ok(guard) = self.engine.lock() {
            if let Some(engine) = guard.as_ref() {
                engine.log_params(converted);
            }
        }
        Ok(())
    }

    /// Log a dictionary of vector metrics. Non-blocking (~100ns).
    ///
    /// Args:
    ///     values: Dict of metric name → numeric value
    ///     step: Optional step/epoch number
    #[pyo3(signature = (values, step=None))]
    fn log_vector(&self, values: &Bound<'_, PyDict>, step: Option<u64>) -> PyResult<()> {
        let converted = py_dict_to_map(values)?;
        if let Ok(guard) = self.engine.lock() {
            if let Some(engine) = guard.as_ref() {
                engine.log_vector(converted, step);
            }
        }
        Ok(())
    }

    /// Log a single scalar value. Non-blocking.
    ///
    /// Args:
    ///    key: Metric name
    ///    value: Metric value (float, int, bool, str)
    fn log_scalar(&self, key: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let val = if let Ok(f) = value.extract::<f64>() {
            MetricValue::Float(f)
        } else if let Ok(i) = value.extract::<i64>() {
            MetricValue::Int(i)
        } else if let Ok(b) = value.extract::<bool>() {
            MetricValue::Bool(b)
        } else if let Ok(s) = value.extract::<String>() {
            MetricValue::Text(s)
        } else {
            MetricValue::Text(value.str()?.to_string())
        };

        if let Ok(guard) = self.engine.lock() {
            if let Some(engine) = guard.as_ref() {
                engine.log_scalar(key.to_string(), val);
            }
        }
        Ok(())
    }

    /// Save an artifact file asynchronously. Non-blocking.
    ///
    /// Args:
    ///     path: Path to the file to save. This path will be preserved relative to
    ///           the artifacts directory.
    #[pyo3(signature = (path))]
    fn save_artifact(&self, path: &str) -> PyResult<()> {
        let src = PathBuf::from(path);
        if let Ok(guard) = self.engine.lock() {
            if let Some(engine) = guard.as_ref() {
                engine.save_artifact(src);
            }
        }
        Ok(())
    }

    /// Save an image/audio/video blob. Non-blocking.
    ///
    /// Args:
    ///     tag: Logical name, e.g. "train/samples".
    ///     data: Raw encoded bytes (PNG, JPEG, WAV, MP4 …).
    ///     extension: File extension without the dot.
    ///     step: Optional step, so the dashboard can show a timeline.
    #[pyo3(signature = (tag, data, extension="png", step=None))]
    fn log_media(
        &self,
        tag: &str,
        data: Vec<u8>,
        extension: &str,
        step: Option<u64>,
    ) -> PyResult<()> {
        if let Ok(guard) = self.engine.lock() {
            if let Some(engine) = guard.as_ref() {
                engine.log_media(tag.to_string(), step, extension.to_string(), data);
            }
        }
        Ok(())
    }

    /// Record a pre-binned histogram. Non-blocking.
    ///
    /// `edges` must have exactly one more element than `counts`.
    #[pyo3(signature = (tag, edges, counts, step=None))]
    fn log_histogram_bins(
        &self,
        tag: &str,
        edges: Vec<f64>,
        counts: Vec<u64>,
        step: Option<u64>,
    ) -> PyResult<()> {
        if edges.len() != counts.len() + 1 {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "edges must have one more element than counts, got {} and {}",
                edges.len(),
                counts.len()
            )));
        }
        if let Ok(guard) = self.engine.lock() {
            if let Some(engine) = guard.as_ref() {
                engine.log_histogram(tag.to_string(), step, edges, counts);
            }
        }
        Ok(())
    }

    /// Log a message to the run log. Non-blocking.
    fn info(&self, message: &str) -> PyResult<()> {
        if let Ok(guard) = self.engine.lock() {
            if let Some(engine) = guard.as_ref() {
                engine.log_message(LogLevel::Info, message.to_string());
            }
        }
        Ok(())
    }

    fn warn(&self, message: &str) -> PyResult<()> {
        if let Ok(guard) = self.engine.lock() {
            if let Some(engine) = guard.as_ref() {
                engine.log_message(LogLevel::Warn, message.to_string());
            }
        }
        Ok(())
    }

    /// Get the run directory path.
    #[getter]
    fn run_dir(&self) -> PyResult<String> {
        if let Ok(guard) = self.engine.lock() {
            if let Some(engine) = guard.as_ref() {
                return Ok(engine.config().run_dir().to_string_lossy().to_string());
            }
        }
        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "Engine is closed",
        ))
    }

    /// Assign this experiment to a project, offline.
    ///
    /// Writes only the `project:` field of `experiment.yaml`, so it works on a
    /// compute node with no dashboard running. Pass None to unassign.
    #[pyo3(signature = (project))]
    fn set_project(&self, project: Option<&str>) -> PyResult<()> {
        let (base_dir, name) = self.with_config(|cfg| (cfg.base_dir.clone(), cfg.name.clone()))?;
        crate::core::storage::set_experiment_project(&base_dir, &name, project)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// The project this experiment is assigned to, read from `experiment.yaml`.
    #[getter]
    fn project(&self) -> PyResult<Option<String>> {
        let (base_dir, name) = self.with_config(|cfg| (cfg.base_dir.clone(), cfg.name.clone()))?;
        crate::core::storage::load_experiment_metadata(&base_dir.join(&name))
            .map(|m| m.project)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Replace this run's tags in `run.yaml`.
    fn set_tags(&self, tags: Vec<String>) -> PyResult<()> {
        self.patch_run_metadata(|meta| meta.tags = Some(tags.clone()))
    }

    /// Add tags to this run, preserving any already present.
    fn add_tags(&self, tags: Vec<String>) -> PyResult<()> {
        self.patch_run_metadata(|meta| {
            let existing = meta.tags.get_or_insert_with(Vec::new);
            for tag in &tags {
                if !existing.contains(tag) {
                    existing.push(tag.clone());
                }
            }
        })
    }

    /// Set this run's description in `run.yaml`.
    fn set_description(&self, description: &str) -> PyResult<()> {
        self.patch_run_metadata(|meta| meta.description = Some(description.to_string()))
    }

    /// The group this run belongs to, if any.
    #[getter]
    fn group(&self) -> PyResult<Option<String>> {
        self.with_config(|cfg| cfg.group.clone())
    }

    /// This run's rank within its group.
    #[getter]
    fn rank(&self) -> PyResult<Option<u32>> {
        self.with_config(|cfg| cfg.rank)
    }

    /// Get the run name.
    #[getter]
    fn run_name(&self) -> PyResult<String> {
        if let Ok(guard) = self.engine.lock() {
            if let Some(engine) = guard.as_ref() {
                return Ok(engine.config().run_name.clone());
            }
        }
        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "Engine is closed",
        ))
    }

    /// Gracefully close the experiment.
    ///
    /// Args:
    ///     status: Terminal status ("FINISHED", "FAILED", "CRASHED"),
    ///             case-insensitive. `None` means "FINISHED".
    ///
    /// Raises:
    ///     ValueError: on anything else.
    #[pyo3(signature = (status=None))]
    fn close(&self, status: Option<String>) -> PyResult<()> {
        // `Option<String>`, not `Option<&str>`: the borrowed form makes PyO3
        // reach for its PyBytes/PyType_HasFeature extraction path, which drags
        // symbols into the `--all-features` test binary that the
        // extension-module build never links (undefined PyBytes_Size,
        // PyType_GetFlags). Owning the string keeps `cargo test` linkable.
        let run_status = parse_terminal_status(status.as_deref())?;
        if let Ok(mut guard) = self.engine.lock() {
            if let Some(engine) = guard.take() {
                engine.close(run_status);
            }
        }
        Ok(())
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    #[pyo3(signature = (exc_type=None, _exc_val=None, _exc_tb=None))]
    fn __exit__(
        &self,
        exc_type: Option<&Bound<'_, PyAny>>,
        _exc_val: Option<&Bound<'_, PyAny>>,
        _exc_tb: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<bool> {
        let status = if exc_type.is_some() {
            RunStatus::Failed
        } else {
            RunStatus::Finished
        };
        if let Ok(mut guard) = self.engine.lock() {
            if let Some(engine) = guard.take() {
                engine.close(status);
            }
        }
        Ok(false) // Don't suppress exceptions
    }

    fn __del__(&self) {
        // Best-effort close on GC
        if let Ok(mut guard) = self.engine.lock() {
            if let Some(engine) = guard.take() {
                engine.close(RunStatus::Finished);
            }
        }
    }

    fn __repr__(&self) -> String {
        if let Ok(guard) = self.engine.lock() {
            if let Some(engine) = guard.as_ref() {
                let cfg = engine.config();
                return format!("Experiment(name={:?}, run={:?})", cfg.name, cfg.run_name);
            }
        }
        "Experiment(closed)".to_string()
    }
}

impl Experiment {
    /// Run `f` against the live engine config, or raise if the engine is closed.
    fn with_config<T>(&self, f: impl FnOnce(&ExperimentConfig) -> T) -> PyResult<T> {
        let guard = self
            .engine
            .lock()
            .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("Engine lock poisoned"))?;
        let engine = guard
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Engine is closed"))?;
        Ok(f(engine.config()))
    }

    /// Read-modify-write `run.yaml`.
    ///
    /// Safe to interleave with the background writer: that task also does a full
    /// load-modify-save, so fields it does not own survive its next tick.
    fn patch_run_metadata(
        &self,
        f: impl FnOnce(&mut crate::core::models::RunMetadata),
    ) -> PyResult<()> {
        let run_dir = self.with_config(|cfg| cfg.run_dir())?;
        let mut meta = crate::core::storage::load_run_metadata(&run_dir)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        f(&mut meta);
        crate::core::storage::save_run_metadata(&run_dir, &meta)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }
}

// ─── Type conversion helpers ──────────────────────────────────────────────────

/// Parse the status a caller wants a run closed with.
///
/// `None` means `FINISHED` — that is what a bare `close()` has always meant.
/// Anything unrecognised is an **error**, not a fallback: this crate's write
/// path swallows failures by design, so a typo that quietly resolved to
/// `FINISHED` would relabel a dead run as a successful one, and nothing
/// downstream could tell. `RUNNING` is rejected for the same reason — closing
/// is the act that makes a status terminal.
fn parse_terminal_status(status: Option<&str>) -> PyResult<RunStatus> {
    let Some(raw) = status else {
        return Ok(RunStatus::Finished);
    };
    match raw.trim().to_ascii_uppercase().as_str() {
        "FINISHED" => Ok(RunStatus::Finished),
        "FAILED" => Ok(RunStatus::Failed),
        "CRASHED" => Ok(RunStatus::Crashed),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "invalid terminal run status {raw:?}; \
             expected one of FINISHED, FAILED, CRASHED (case-insensitive)"
        ))),
    }
}

fn py_dict_to_map(dict: &Bound<'_, PyDict>) -> PyResult<HashMap<String, MetricValue>> {
    let mut map = HashMap::new();
    for (k, v) in dict.iter() {
        let key: String = k.extract()?;
        let val = if let Ok(f) = v.extract::<f64>() {
            MetricValue::Float(f)
        } else if let Ok(i) = v.extract::<i64>() {
            MetricValue::Int(i)
        } else if let Ok(b) = v.extract::<bool>() {
            MetricValue::Bool(b)
        } else if let Ok(s) = v.extract::<String>() {
            MetricValue::Text(s)
        } else {
            MetricValue::Text(v.str()?.to_string())
        };
        map.insert(key, val);
    }
    Ok(map)
}

fn py_dict_to_yaml(dict: &Bound<'_, PyDict>) -> PyResult<HashMap<String, serde_yaml::Value>> {
    let mut map = HashMap::new();
    for (k, v) in dict.iter() {
        let key: String = k.extract()?;
        let val = if let Ok(b) = v.extract::<bool>() {
            serde_yaml::Value::Bool(b)
        } else if let Ok(i) = v.extract::<i64>() {
            serde_yaml::Value::Number(serde_yaml::Number::from(i))
        } else if let Ok(f) = v.extract::<f64>() {
            serde_yaml::Value::Number(serde_yaml::Number::from(f))
        } else if let Ok(s) = v.extract::<String>() {
            serde_yaml::Value::String(s)
        } else {
            serde_yaml::Value::String(v.str()?.to_string())
        };
        map.insert(key, val);
    }
    Ok(map)
}

/// Convert a `serde_json::Value` into the equivalent native Python object.
fn json_to_py<'py>(py: Python<'py>, value: &serde_json::Value) -> PyResult<Bound<'py, PyAny>> {
    use pyo3::types::{PyList, PyNone};
    Ok(match value {
        serde_json::Value::Null => PyNone::get(py).to_owned().into_any(),
        serde_json::Value::Bool(b) => b.into_pyobject(py)?.to_owned().into_any(),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_pyobject(py)?.into_any()
            } else {
                n.as_f64().unwrap_or(f64::NAN).into_pyobject(py)?.into_any()
            }
        }
        serde_json::Value::String(s) => s.into_pyobject(py)?.into_any(),
        serde_json::Value::Array(items) => {
            let list = PyList::empty(py);
            for item in items {
                list.append(json_to_py(py, item)?)?;
            }
            list.into_any()
        }
        serde_json::Value::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                dict.set_item(k, json_to_py(py, v)?)?;
            }
            dict.into_any()
        }
    })
}

/// Convert anything `Serialize` into a native Python object, via JSON.
fn serde_to_py<'py, T: serde::Serialize>(
    py: Python<'py>,
    value: &T,
) -> PyResult<Bound<'py, PyAny>> {
    let json = serde_json::to_value(value)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    json_to_py(py, &json)
}

/// Build a `RunQuery` from the loosely-typed keyword arguments Python passes.
///
/// `tags` accepts either an expression string (`"arm:tiered AND study:1"`) or a
/// list of tags, which is treated as a conjunction.
fn build_query(
    project: Option<String>,
    experiment: Option<String>,
    group: Option<String>,
    status: Option<String>,
    tags: Option<&Bound<'_, PyAny>>,
) -> PyResult<crate::core::storage::RunQuery> {
    let status = match status.as_deref() {
        None => None,
        Some(s) => Some(match s.to_ascii_uppercase().as_str() {
            "RUNNING" => RunStatus::Running,
            "FINISHED" => RunStatus::Finished,
            "FAILED" => RunStatus::Failed,
            "CRASHED" => RunStatus::Crashed,
            other => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "Unknown status {other:?}; expected RUNNING, FINISHED, FAILED or CRASHED"
                )))
            }
        }),
    };

    let tag_clauses = match tags {
        None => vec![],
        Some(obj) => {
            if let Ok(expr) = obj.extract::<String>() {
                crate::core::storage::parse_tag_expr(&expr)
            } else {
                obj.extract::<Vec<String>>()
                    .map_err(|_| {
                        pyo3::exceptions::PyTypeError::new_err(
                            "tags must be a str expression or a list of str",
                        )
                    })?
                    .into_iter()
                    .map(|t| vec![t])
                    .collect()
            }
        }
    };

    Ok(crate::core::storage::RunQuery {
        project,
        experiment,
        group,
        status,
        tags: tag_clauses,
    })
}

// ─── Read API ─────────────────────────────────────────────────────────────────

/// Query runs across the whole store.
///
/// Returns a list of dicts, newest first. Each carries `run`, `experiment`,
/// `project`, `status`, `started_at`, `tags`, latest `scalars`/`vectors`, and
/// `path` — pass that `path` to `read_metrics()` / `load_config()`.
#[pyfunction]
#[pyo3(signature = (base_dir="experiments", project=None, experiment=None, group=None, status=None, tags=None))]
fn load_runs<'py>(
    py: Python<'py>,
    base_dir: &str,
    project: Option<String>,
    experiment: Option<String>,
    group: Option<String>,
    status: Option<String>,
    tags: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let query = build_query(project, experiment, group, status, tags)?;
    let entries = crate::core::storage::query_runs(&PathBuf::from(base_dir), &query)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    serde_to_py(py, &entries)
}

/// Read a run's logged vector metrics as a list of row dicts.
///
/// `run_dir` is a run directory — what `load_runs()` puts in `path` and what
/// `Experiment.run_dir` returns. Each row carries `step`, `timestamp`, and one
/// key per logged metric; metrics absent from a row are `None`.
#[pyfunction]
#[pyo3(signature = (run_dir))]
fn read_metrics<'py>(py: Python<'py>, run_dir: &str) -> PyResult<Bound<'py, PyAny>> {
    let rows = crate::core::storage::read_run_vectors(&PathBuf::from(run_dir))
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    serde_to_py(py, &rows)
}

/// Read a run's logged parameters (`config.yaml`) as a dict.
#[pyfunction]
#[pyo3(signature = (run_dir))]
fn load_config<'py>(py: Python<'py>, run_dir: &str) -> PyResult<Bound<'py, PyAny>> {
    let path = PathBuf::from(run_dir).join("config.yaml");
    let value = crate::core::storage::load_yaml_value(&path)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    serde_to_py(py, &value)
}

/// Read a single run's metadata (`run.yaml`) as a dict.
#[pyfunction]
#[pyo3(signature = (run_dir))]
fn load_run<'py>(py: Python<'py>, run_dir: &str) -> PyResult<Bound<'py, PyAny>> {
    let meta = crate::core::storage::load_run_metadata(&PathBuf::from(run_dir))
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    serde_to_py(py, &meta)
}

/// Read a run's captured provenance (`provenance.yaml`) as a dict.
///
/// Returns an empty dict when the run predates provenance capture or had it
/// disabled — absence is normal, not an error.
#[pyfunction]
#[pyo3(signature = (run_dir))]
fn load_provenance<'py>(py: Python<'py>, run_dir: &str) -> PyResult<Bound<'py, PyAny>> {
    let path = PathBuf::from(run_dir).join("provenance.yaml");
    let value = crate::core::storage::load_yaml_value(&path)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    serde_to_py(py, &value)
}

/// Read a run's logged histograms as row dicts.
///
/// Each row carries `tag`, `step`, `edges`, `counts` and `total`. Edges and
/// counts arrive as JSON strings because bin counts vary per row.
#[pyfunction]
#[pyo3(signature = (run_dir))]
fn read_histograms<'py>(py: Python<'py>, run_dir: &str) -> PyResult<Bound<'py, PyAny>> {
    let rows = crate::core::storage::read_metrics(
        &PathBuf::from(run_dir),
        crate::core::storage::HISTOGRAM_STEM,
    )
    .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    serde_to_py(py, &rows)
}

/// List a run's logged media, newest last.
#[pyfunction]
#[pyo3(signature = (run_dir))]
fn read_media<'py>(py: Python<'py>, run_dir: &str) -> PyResult<Bound<'py, PyAny>> {
    let path = PathBuf::from(run_dir).join("media.jsonl");
    let mut out: Vec<serde_json::Value> = vec![];
    if let Ok(content) = std::fs::read_to_string(&path) {
        for line in content.lines().filter(|l| !l.trim().is_empty()) {
            // Skip a torn final line rather than failing the whole read: the
            // manifest is appended to while the run is live.
            if let Ok(value) = serde_json::from_str(line) {
                out.push(value);
            }
        }
    }
    serde_to_py(py, &out)
}

/// Read a run's sampled hardware metrics as row dicts.
///
/// Rows carry `step` (the sample index), `timestamp`, and one key per probe
/// reading, e.g. `gpu.0.util_pct`. Empty when no probe was available.
#[pyfunction]
#[pyo3(signature = (run_dir))]
fn read_system_metrics<'py>(py: Python<'py>, run_dir: &str) -> PyResult<Bound<'py, PyAny>> {
    let rows = crate::core::storage::read_metrics(
        &PathBuf::from(run_dir),
        crate::core::storage::SYSTEM_STEM,
    )
    .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    serde_to_py(py, &rows)
}

/// List every project in the store, with its experiment count.
#[pyfunction]
#[pyo3(signature = (base_dir="experiments"))]
fn load_projects<'py>(py: Python<'py>, base_dir: &str) -> PyResult<Bound<'py, PyAny>> {
    let base = PathBuf::from(base_dir);
    let names = crate::core::storage::list_projects(&base)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    let mut out = vec![];
    for name in names {
        let meta = crate::core::storage::load_project_metadata(&base, &name).unwrap_or_default();
        let experiments =
            crate::core::storage::list_project_experiments(&base, &name).unwrap_or_default();
        out.push(serde_json::json!({
            "id": name,
            "display_name": meta.display_name,
            "description": meta.description,
            "tags": meta.tags,
            "created_at": meta.created_at,
            "generated": meta.generated,
            "generated_from": meta.generated_from,
            "experiments": experiments,
        }));
    }
    serde_to_py(py, &out)
}

/// Assign an experiment to a project without a running server.
///
/// This is the same write `Experiment.set_project()` performs, callable without
/// an open run — e.g. from a sync script that projects an external manifest.
#[pyfunction]
#[pyo3(signature = (experiment, project, base_dir="experiments"))]
fn assign_project(experiment: &str, project: Option<&str>, base_dir: &str) -> PyResult<()> {
    crate::core::storage::set_experiment_project(&PathBuf::from(base_dir), experiment, project)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

// ─── Module definition ────────────────────────────────────────────────────────

#[pymodule(name = "expman")]
fn expman_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Experiment>()?;
    m.add_function(wrap_pyfunction!(load_runs, m)?)?;
    m.add_function(wrap_pyfunction!(read_metrics, m)?)?;
    m.add_function(wrap_pyfunction!(load_config, m)?)?;
    m.add_function(wrap_pyfunction!(load_run, m)?)?;
    m.add_function(wrap_pyfunction!(load_projects, m)?)?;
    m.add_function(wrap_pyfunction!(load_provenance, m)?)?;
    m.add_function(wrap_pyfunction!(read_system_metrics, m)?)?;
    m.add_function(wrap_pyfunction!(read_histograms, m)?)?;
    m.add_function(wrap_pyfunction!(read_media, m)?)?;
    m.add_function(wrap_pyfunction!(assign_project, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

// No `#[cfg(test)] mod tests` here on purpose. Anything in this module that
// touches PyO3 — including building a PyErr to assert on — forces the lib test
// binary to link libpython, which the `extension-module` build deliberately
// does not do, and `cargo test --all-features` fails with undefined
// PyGILState_Ensure / PyTuple_Type. The cdylib is fine: Python resolves those
// on load. `parse_terminal_status` is covered end to end from Python instead —
// see wrappers/python/tests/test_run_status.py.
