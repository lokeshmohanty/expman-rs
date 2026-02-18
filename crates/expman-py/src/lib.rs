//! PyO3 Python extension module for expman-rs.
//!
//! Exposes `Experiment` class to Python. All I/O is non-blocking:
//! `log_metrics()` is a channel send on the background tokio runtime,
//! never blocking the Python GIL or the experiment loop.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pyo3::prelude::*;
use pyo3::types::PyDict;

use expman_core::{ExperimentConfig, LoggingEngine, LogLevel, MetricValue, RunStatus};

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
    #[new]
    #[pyo3(signature = (name, run_name=None, base_dir="experiments", flush_interval_rows=50, flush_interval_ms=500))]
    fn new(
        name: &str,
        run_name: Option<&str>,
        base_dir: &str,
        flush_interval_rows: usize,
        flush_interval_ms: u64,
    ) -> PyResult<Self> {
        let mut config = ExperimentConfig::new(name, base_dir);
        config.flush_interval_rows = flush_interval_rows;
        config.flush_interval_ms = flush_interval_ms;
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

    /// Log a dictionary of metrics. Non-blocking (~100ns).
    ///
    /// Args:
    ///     metrics: Dict of metric name → numeric value
    ///     step: Optional step/epoch number
    #[pyo3(signature = (metrics, step=None))]
    fn log_metrics(&self, metrics: &Bound<'_, PyDict>, step: Option<u64>) -> PyResult<()> {
        let converted = py_dict_to_metrics(metrics)?;
        if let Ok(guard) = self.engine.lock() {
            if let Some(engine) = guard.as_ref() {
                engine.log_metrics(converted, step);
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
        Err(pyo3::exceptions::PyRuntimeError::new_err("Engine is closed"))
    }

    /// Get the run name.
    #[getter]
    fn run_name(&self) -> PyResult<String> {
        if let Ok(guard) = self.engine.lock() {
            if let Some(engine) = guard.as_ref() {
                return Ok(engine.config().run_name.clone());
            }
        }
        Err(pyo3::exceptions::PyRuntimeError::new_err("Engine is closed"))
    }

    /// Gracefully close the experiment: flush all pending metrics and write final metadata.
    /// Called automatically by __del__ and context manager __exit__.
    fn close(&self) -> PyResult<()> {
        if let Ok(mut guard) = self.engine.lock() {
            if let Some(engine) = guard.take() {
                engine.close(RunStatus::Finished);
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

// ─── Type conversion helpers ──────────────────────────────────────────────────

fn py_dict_to_metrics(
    dict: &Bound<'_, PyDict>,
) -> PyResult<HashMap<String, MetricValue>> {
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

fn py_dict_to_yaml(
    dict: &Bound<'_, PyDict>,
) -> PyResult<HashMap<String, serde_yaml::Value>> {
    let mut map = HashMap::new();
    for (k, v) in dict.iter() {
        let key: String = k.extract()?;
        let val = if let Ok(b) = v.extract::<bool>() {
            serde_yaml::Value::Bool(b)
        } else if let Ok(i) = v.extract::<i64>() {
            serde_yaml::Value::Number(serde_yaml::Number::from(i))
        } else if let Ok(f) = v.extract::<f64>() {
            serde_yaml::Value::Number(
                serde_yaml::Number::from(f)
            )
        } else if let Ok(s) = v.extract::<String>() {
            serde_yaml::Value::String(s)
        } else {
            serde_yaml::Value::String(v.str()?.to_string())
        };
        map.insert(key, val);
    }
    Ok(map)
}

// ─── Module definition ────────────────────────────────────────────────────────

#[pymodule]
fn expman(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Experiment>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
