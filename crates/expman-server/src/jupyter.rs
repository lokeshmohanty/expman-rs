use std::collections::HashMap;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::process::Child;
use tracing::{error, info};

/// Tracks an active Jupyter notebook instance.
pub struct JupyterInstance {
    pub port: u16,
    pub process: Child,
}

/// Thread-safe manager for spawning and stopping Jupyter Notebooks.
#[derive(Clone, Default)]
pub struct JupyterManager {
    // Maps a unique run identifier (e.g., "experiment:run") to a Jupyter instance.
    instances: Arc<Mutex<HashMap<String, JupyterInstance>>>,
}

impl JupyterManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Checks if `jupyter notebook` is available in the current environment.
    ///
    /// This is used by the frontend to determine whether to enable the
    /// "Launch Live Jupyter Notebook" button or show a warning.
    pub async fn is_available() -> bool {
        match tokio::process::Command::new("jupyter")
            .arg("notebook")
            .arg("--version")
            .output()
            .await
        {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    /// Finds an available TCP port starting from a base port.
    ///
    /// Scans ports from 8000 to 9000 to find the first one that can be bound to `127.0.0.1`.
    fn get_available_port() -> Option<u16> {
        (8000..9000).find(|port| TcpListener::bind(("127.0.0.1", *port)).is_ok())
    }

    /// Spawns a new Jupyter Notebook process for a given run and environment.
    ///
    /// If a process is already tracked for the given run, returns its port immediately.
    /// Generates `interactive.ipynb` if it does not exist in the run directory.
    pub async fn spawn(
        &self,
        exp: &str,
        run: &str,
        _env_path: &str,
        run_dir: PathBuf,
        is_python: bool,
    ) -> Result<u16, String> {
        let key = format!("{}:{}", exp, run);

        // Check if already running
        {
            let instances = self.instances.lock().unwrap();
            if let Some(instance) = instances.get(&key) {
                return Ok(instance.port);
            }
        }

        let port = Self::get_available_port()
            .ok_or_else(|| "No available ports for Jupyter".to_string())?;

        // 1. Generate notebook content if it doesn't exist.
        let notebook_path = run_dir.join("interactive.ipynb");
        if !notebook_path.exists() {
            let cells = if is_python {
                r##"{
   "cell_type": "code",
   "execution_count": null,
   "metadata": {},
   "outputs": [],
   "source": [
    "# Install required dependencies into this environment\n",
    "import sys\n",
    "!pip install polars matplotlib pyarrow fastparquet --python {sys.executable}"
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
    "# Load run metrics\n",
    "metrics_path = 'metrics.parquet'\n",
    "df = pl.read_parquet(metrics_path)\n",
    "\n",
    "# Display the latest metrics\n",
    "df.tail()"
   ]
  }"##
                .to_string()
            } else {
                let snippet = "use polars::prelude::*;\n\nfn main() -> Result<(), PolarsError> {\n    // Load run metrics\n    let mut file = std::fs::File::open(\"metrics.parquet\").unwrap();\n    let df = ParquetReader::new(&mut file).finish()?;\n\n    println!(\"{:?}\", df.tail(Some(5)));\n    Ok(())\n}";
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

            let ipynb_content = format!(
                r#"{{
 "cells": [
  {}
 ],
 "metadata": {{}},
 "nbformat": 4,
 "nbformat_minor": 5
}}"#,
                cells
            );

            if let Err(e) = tokio::fs::write(&notebook_path, ipynb_content).await {
                error!("Failed to generate interactive.ipynb: {}", e);
                return Err(format!("Failed to generate interactive.ipynb: {}", e));
            }
        }

        info!("Spawning Jupyter Notebook for {} on port {}", key, port);

        // We run the global `jupyter notebook` command available in the dashboard's environment
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
            .map_err(|e| format!("Failed to spawn global jupyter child process: {}", e))?;

        // Small wait to ensure it hasn't instantly crashed (e.g. module not found)
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

    /// Returns the port if the notebook is running, or None.
    pub fn status(&self, exp: &str, run: &str) -> Option<u16> {
        let key = format!("{}:{}", exp, run);
        let mut instances = self.instances.lock().unwrap();

        // Check if the process exited on its own, clean it up if it did:
        if let Some(instance) = instances.get_mut(&key) {
            match instance.process.try_wait() {
                Ok(Some(_)) => {
                    // Process exited
                }
                Ok(None) => {
                    // Still running
                    return Some(instance.port);
                }
                Err(_) => {
                    // Error polling
                }
            }
        }

        instances.remove(&key);
        None
    }

    /// Stops a running Jupyter instance, if any.
    ///
    /// Kills the underlying child process and removes it from the internal tracking map.
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
    ///
    /// Iterates through all tracked instances and sends a kill signal to their processes.
    pub async fn shutdown_all(&self) {
        let instances_to_kill: Vec<_> = {
            let mut instances = self.instances.lock().unwrap();
            instances.drain().map(|(_, inst)| inst).collect()
        };

        for mut inst in instances_to_kill {
            let _ = inst.process.kill().await;
            let _ = inst.process.wait().await;
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
    fn test_jupyter_manager_get_available_port() {
        let port = JupyterManager::get_available_port();
        assert!(port.is_some());
        let p = port.unwrap();
        assert!((8000..9000).contains(&p));
    }

    #[tokio::test]
    async fn test_jupyter_manager_stop_non_existent() {
        let manager = JupyterManager::new();
        // Stopping a non-existent notebook shouldn't error
        let res = manager.stop("exp1", "run1").await;
        assert!(res.is_ok());
    }
}
