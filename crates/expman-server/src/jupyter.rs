use std::collections::HashMap;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::process::Child;
use tracing::{error, info};

/// Tracks an active Jupyter notebook instance.
pub struct JupyterInstance {
    pub port: u16,
    pub process: Child,
}

/// The interactive backend detected in the user's environment.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InteractiveBackend {
    Jupyter,
    Python,
    None,
}

/// Generate the full `.ipynb` JSON content for a default interactive notebook.
///
/// For Python runs, produces 2 cells:
///   1. Install dependencies (`pip install polars matplotlib`)
///   2. Load and display metrics
///
/// For Rust runs, produces a single cell with a `polars` snippet.
pub fn generate_notebook_content(is_python: bool) -> String {
    let cells = if is_python {
        r##"{
   "cell_type": "code",
   "execution_count": null,
   "metadata": {},
   "outputs": [],
   "source": [
    "# Install required dependencies into this environment\n",
    "import sys\n",
    "!pip --python {sys.executable} install polars matplotlib"
   ]
  },
  {
   "cell_type": "code",
   "execution_count": null,
   "metadata": {},
   "outputs": [],
   "source": [
    "import polars as pl\n",
    "import matplotlib.pyplot as plt\n",
    "\n",
    "# Load run vectors\n",
    "vectors_path = 'vectors.parquet'\n",
    "df = pl.read_parquet(vectors_path)\n",
    "\n",
    "# Display the latest metrics\n",
    "df.tail()"
   ]
  }"##
        .to_string()
    } else {
        let snippet = "use polars::prelude::*;\n\nfn main() -> Result<(), PolarsError> {\n    // Load run vectors\n    let mut file = std::fs::File::open(\"vectors.parquet\").unwrap();\n    let df = ParquetReader::new(&mut file).finish()?;\n\n    println!(\"{:?}\", df.tail(Some(5)));\n    Ok(())\n}";
        format!(
            r#"{{
   "cell_type": "code",
   "execution_count": null,
   "metadata": {{}},
   "outputs": [],
   "source": [
    "{}"
   ]
  }}"#,
            snippet.replace('\n', "\\n").replace('"', "\\\"")
        )
    };

    format!(
        r#"{{
 "cells": [
  {}
 ],
 "metadata": {{}},
 "nbformat": 4,
 "nbformat_minor": 5
}}"#,
        cells
    )
}

/// Write the default `interactive.ipynb` into `run_dir` if it does not already exist.
///
/// Returns `Ok(true)` if the notebook was created, `Ok(false)` if it already existed.
pub async fn generate_notebook(run_dir: &Path, is_python: bool) -> Result<bool, String> {
    let notebook_path = run_dir.join("interactive.ipynb");
    if notebook_path.exists() {
        return Ok(false);
    }

    let content = generate_notebook_content(is_python);
    if let Err(e) = tokio::fs::write(&notebook_path, content).await {
        error!("Failed to generate interactive.ipynb: {}", e);
        return Err(format!("Failed to generate interactive.ipynb: {}", e));
    }

    Ok(true)
}

/// Generate the full `.ipynb` JSON content for a multi-run interactive notebook.
pub fn generate_multi_run_notebook_content(is_python: bool, runs: &[String]) -> String {
    let cells = if is_python {
        let load_snippets = runs.iter().map(|run| {
            format!("df_{} = pl.read_parquet('{}/vectors.parquet').with_columns(pl.lit('{}').alias('run'))", run.replace('-', "_"), run, run)
        }).collect::<Vec<_>>().join("\n");
        let tail_snippets = runs
            .iter()
            .map(|run| format!("df_{}.tail()", run.replace('-', "_")))
            .collect::<Vec<_>>()
            .join(", ");

        format!(
            r##"{{
   "cell_type": "code",
   "execution_count": null,
   "metadata": {{}},
   "outputs": [],
   "source": [
    "# Install required dependencies into this environment\n",
    "import sys\n",
    "!pip --python {{sys.executable}} install polars matplotlib"
   ]
  }},
  {{
   "cell_type": "code",
   "execution_count": null,
   "metadata": {{}},
   "outputs": [],
   "source": [
    "import polars as pl\n",
    "import matplotlib.pyplot as plt\n",
    "\n",
    "# Load run vectors\n",
    "{}\n",
    "\n",
    "# Display the latest metrics\n",
    "{}"
   ]
  }}"##,
            load_snippets.replace('\n', "\\n"),
            tail_snippets.replace('\n', "\\n")
        )
    } else {
        let load_snippets = runs.iter().map(|run| {
            format!("    let df_{} = ParquetReader::new(&mut std::fs::File::open(\"{}/vectors.parquet\").unwrap()).finish()?;\n    // Note: To add a 'run' column in rust polars you would typically use lit(\"{}\") in a select/with_columns, \n    // but for simplicity here we just load them.", run.replace('-', "_"), run, run)
        }).collect::<Vec<_>>().join("\n");

        let snippet = format!("use polars::prelude::*;\n\nfn main() -> Result<(), PolarsError> {{\n    // Load run vectors\n{}\n    Ok(())\n}}", load_snippets);
        format!(
            r#"{{
   "cell_type": "code",
   "execution_count": null,
   "metadata": {{}},
   "outputs": [],
   "source": [
    "{}"
   ]
  }}"#,
            snippet.replace('\n', "\\n").replace('"', "\\\"")
        )
    };

    format!(
        r#"{{
 "cells": [
  {}
 ],
 "metadata": {{}},
 "nbformat": 4,
 "nbformat_minor": 5
}}"#,
        cells
    )
}

/// Write the default `interactive.ipynb` into `exp_dir` if it does not already exist.
///
/// Returns `Ok(true)` if the notebook was created, `Ok(false)` if it already existed.
pub async fn generate_multi_run_notebook(
    exp_dir: &Path,
    is_python: bool,
    runs: &[String],
) -> Result<bool, String> {
    let notebook_path = exp_dir.join("interactive.ipynb");
    if notebook_path.exists() {
        return Ok(false);
    }

    let content = generate_multi_run_notebook_content(is_python, runs);
    if let Err(e) = tokio::fs::write(&notebook_path, content).await {
        error!("Failed to generate interactive.ipynb: {}", e);
        return Err(format!("Failed to generate interactive.ipynb: {}", e));
    }

    Ok(true)
}

/// Detect the best available interactive Python backend in the user's environment.
///
/// Checks (in order): `jupyter notebook`, `ipython`, `python3`.
pub async fn detect_backend() -> InteractiveBackend {
    // Check jupyter
    match tokio::process::Command::new("jupyter")
        .args(["notebook", "--version"])
        .output()
        .await
    {
        Ok(output) => {
            info!(
                "jupyter notebook --version: status={}, stdout={}, stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stdout).trim(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
            if output.status.success() {
                return InteractiveBackend::Jupyter;
            }
        }
        Err(e) => {
            info!("jupyter not found: {}", e);
        }
    }

    // Check python3
    match tokio::process::Command::new("python3")
        .arg("--version")
        .output()
        .await
    {
        Ok(output) => {
            info!(
                "python3 --version: status={}, stdout={}",
                output.status,
                String::from_utf8_lossy(&output.stdout).trim()
            );
            if output.status.success() {
                return InteractiveBackend::Python;
            }
        }
        Err(e) => {
            info!("python3 not found: {}", e);
        }
    }

    InteractiveBackend::None
}

/// Thread-safe manager for spawning and stopping Jupyter Notebooks.
///
/// When `jupyter notebook` is available in the user's environment, this manager
/// spawns per-run Jupyter instances. When only ipython/python is available,
/// the frontend shows notebook content with copy-paste guidance instead.
#[derive(Clone, Default)]
pub struct JupyterManager {
    instances: Arc<Mutex<HashMap<String, JupyterInstance>>>,
}

impl JupyterManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Finds an available TCP port.
    fn get_available_port() -> Option<u16> {
        (8888..9999).find(|port| TcpListener::bind(("127.0.0.1", *port)).is_ok())
    }

    /// Spawns a Jupyter Notebook for a given run directory.
    ///
    /// Uses the `jupyter` binary from the user's PATH.
    pub async fn spawn(
        &self,
        exp: &str,
        run: &str,
        run_dir: PathBuf,
        is_python: bool,
    ) -> Result<u16, String> {
        let key = format!("{}:{}", exp, run);

        // Already running?
        {
            let instances = self.instances.lock().unwrap();
            if let Some(instance) = instances.get(&key) {
                return Ok(instance.port);
            }
        }

        let port = Self::get_available_port()
            .ok_or_else(|| "No available ports for Jupyter".to_string())?;

        // Generate notebook if it doesn't exist
        generate_notebook(&run_dir, is_python).await?;

        info!("Spawning Jupyter Notebook for {} on port {}", key, port);

        let mut child = tokio::process::Command::new("jupyter")
            .arg("notebook")
            .arg("--no-browser")
            .arg(format!("--port={}", port))
            .arg("--ServerApp.token=''")
            .arg("--ServerApp.password=''")
            .arg("--ServerApp.disable_check_xsrf=True")
            .arg("--ServerApp.tornado_settings={\"headers\":{\"Content-Security-Policy\":\"frame-ancestors *\"}}")
            .current_dir(&run_dir)
            .spawn()
            .map_err(|e| format!("Failed to spawn jupyter: {}", e))?;

        // Small wait to ensure it hasn't instantly crashed
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!(
                "Jupyter process crashed immediately with status {}",
                status
            ));
        }

        let mut instances = self.instances.lock().unwrap();
        instances.insert(
            key,
            JupyterInstance {
                port,
                process: child,
            },
        );

        Ok(port)
    }

    /// Spawns a multi-run Jupyter Notebook in the experiment directory.
    pub async fn spawn_multi(
        &self,
        exp: &str,
        exp_dir: PathBuf,
        is_python: bool,
        runs: &[String],
    ) -> Result<u16, String> {
        let key = format!("{}:__multi__", exp);

        // Already running?
        {
            let instances = self.instances.lock().unwrap();
            if let Some(instance) = instances.get(&key) {
                return Ok(instance.port);
            }
        }

        let port = Self::get_available_port()
            .ok_or_else(|| "No available ports for Jupyter".to_string())?;

        // Generate notebook if it doesn't exist
        generate_multi_run_notebook(&exp_dir, is_python, runs).await?;

        info!(
            "Spawning multi-run Jupyter Notebook for {} on port {}",
            exp, port
        );

        let mut child = tokio::process::Command::new("jupyter")
            .arg("notebook")
            .arg("--no-browser")
            .arg(format!("--port={}", port))
            .arg("--ServerApp.token=''")
            .arg("--ServerApp.password=''")
            .arg("--ServerApp.disable_check_xsrf=True")
            .arg("--ServerApp.tornado_settings={\"headers\":{\"Content-Security-Policy\":\"frame-ancestors *\"}}")
            .current_dir(&exp_dir)
            .spawn()
            .map_err(|e| format!("Failed to spawn jupyter: {}", e))?;

        // Small wait to ensure it hasn't instantly crashed
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!(
                "Jupyter process crashed immediately with status {}",
                status
            ));
        }

        let mut instances = self.instances.lock().unwrap();
        instances.insert(
            key,
            JupyterInstance {
                port,
                process: child,
            },
        );

        Ok(port)
    }

    /// Returns the port if the notebook is running.
    pub fn status(&self, exp: &str, run: &str) -> Option<u16> {
        let key = format!("{}:{}", exp, run);
        let mut instances = self.instances.lock().unwrap();

        if let Some(instance) = instances.get_mut(&key) {
            match instance.process.try_wait() {
                Ok(Some(_)) => { /* exited */ }
                Ok(None) => return Some(instance.port),
                Err(_) => { /* error polling */ }
            }
        }

        instances.remove(&key);
        None
    }

    /// Stops a running Jupyter instance.
    pub async fn stop(&self, exp: &str, run: &str) -> Result<(), String> {
        let key = format!("{}:{}", exp, run);
        let mut instance = {
            let mut instances = self.instances.lock().unwrap();
            instances.remove(&key)
        };

        if let Some(mut inst) = instance.take() {
            info!("Shutting down Jupyter Notebook for {}", key);
            let _ = inst.process.kill().await;
            let _ = inst.process.wait().await;
        }

        Ok(())
    }

    /// Kill all notebooks (e.g., on server shutdown).
    pub async fn shutdown_all(&self) {
        let all: Vec<_> = {
            let mut instances = self.instances.lock().unwrap();
            instances.drain().map(|(_, inst)| inst).collect()
        };

        for mut inst in all {
            let _ = inst.process.kill().await;
            let _ = tokio::time::timeout(tokio::time::Duration::from_secs(5), inst.process.wait())
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jupyter_manager_new() {
        let manager = JupyterManager::new();
        assert!(manager.status("eval", "run_1").is_none());
    }

    #[test]
    fn test_get_available_port() {
        let port = JupyterManager::get_available_port();
        assert!(port.is_some());
        let p = port.unwrap();
        assert!((8888..9999).contains(&p));
    }

    #[tokio::test]
    async fn test_stop_non_existent() {
        let manager = JupyterManager::new();
        let res = manager.stop("exp1", "run1").await;
        assert!(res.is_ok());
    }

    #[test]
    fn test_generate_notebook_content_python_has_two_cells() {
        let content = generate_notebook_content(true);
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let cells = parsed["cells"].as_array().unwrap();
        assert_eq!(
            cells.len(),
            2,
            "Python notebook should have exactly 2 cells"
        );
        assert_eq!(parsed["nbformat"], 4);
    }
    #[test]
    fn test_generate_notebook_content_rust_has_one_cell() {
        let content = generate_notebook_content(false);
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let cells = parsed["cells"].as_array().unwrap();
        assert_eq!(cells.len(), 1, "Rust notebook should have exactly 1 cell");
        assert_eq!(parsed["nbformat"], 4);
    }

    #[test]
    fn test_generate_multi_run_notebook_content_python_shows_tails() {
        let runs = vec!["run-1".to_string(), "run-2".to_string()];
        let content = generate_multi_run_notebook_content(true, &runs);
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let cells = parsed["cells"].as_array().unwrap();

        // Find the cell with the tails snippet
        let mut found = false;
        for cell in cells {
            if let Some(source) = cell["source"].as_array() {
                let full_source = source
                    .iter()
                    .map(|s| s.as_str().unwrap())
                    .collect::<String>();
                if full_source.contains("df_run_1.tail()")
                    && full_source.contains("df_run_2.tail()")
                {
                    found = true;
                }
            }
        }
        assert!(found, "Should have found a cell with individual tail calls");
    }

    #[tokio::test]
    async fn test_generate_notebook_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let created = generate_notebook(tmp.path(), true).await.unwrap();
        assert!(created);
        assert!(tmp.path().join("interactive.ipynb").exists());

        let created_again = generate_notebook(tmp.path(), true).await.unwrap();
        assert!(!created_again);
    }

    #[tokio::test]
    async fn test_detect_backend_returns_something() {
        let backend = detect_backend().await;
        // In CI/test environments, at least python3 should be available
        assert_ne!(backend, InteractiveBackend::None);
    }
}
