//! TensorBoard service logic (process management).
use std::collections::HashMap;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::process::Child;
use tracing::info;

/// Tracks an active TensorBoard instance.
pub struct TensorBoardInstance {
    pub port: u16,
    pub process: Child,
}

/// Thread-safe manager for spawning and stopping TensorBoard.
#[derive(Clone, Default)]
pub struct TensorBoardManager {
    instances: Arc<Mutex<HashMap<String, TensorBoardInstance>>>,
}

impl TensorBoardManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Finds an available TCP port (6006..7999).
    fn get_available_port() -> Option<u16> {
        (6006..7999).find(|port| TcpListener::bind(("127.0.0.1", *port)).is_ok())
    }

    /// Detect if tensorboard is available in the environment.
    pub async fn detect_tensorboard() -> bool {
        match tokio::process::Command::new("tensorboard")
            .arg("--version")
            .output()
            .await
        {
            Ok(output) => {
                info!(
                    "tensorboard --version: status={}, stdout={}, stderr={}",
                    output.status,
                    String::from_utf8_lossy(&output.stdout).trim(),
                    String::from_utf8_lossy(&output.stderr).trim()
                );
                output.status.success()
            }
            Err(e) => {
                info!("tensorboard not found: {}", e);
                false
            }
        }
    }

    /// Check if the run directory has a tensorboard subdirectory with logs.
    pub async fn has_logs(run_dir: &std::path::Path) -> bool {
        let tb_dir = run_dir.join("tensorboard");
        if !tb_dir.exists() {
            return false;
        }

        // Just check if there's any file in the tensorboard directory
        if let Ok(mut entries) = tokio::fs::read_dir(tb_dir).await {
            if let Ok(Some(_)) = entries.next_entry().await {
                return true; // found at least one file/dir
            }
        }
        false
    }

    /// Spawns TensorBoard for a given run directory.
    pub async fn spawn(&self, exp: &str, run: &str, run_dir: PathBuf) -> Result<u16, String> {
        let key = format!("{}:{}", exp, run);

        // Already running?
        {
            let instances = self.instances.lock().unwrap();
            if let Some(instance) = instances.get(&key) {
                return Ok(instance.port);
            }
        }

        let port = Self::get_available_port()
            .ok_or_else(|| "No available ports for TensorBoard".to_string())?;

        let logdir = run_dir.join("tensorboard");
        if !logdir.exists() {
            return Err("TensorBoard log directory not found".to_string());
        }

        info!("Spawning TensorBoard for {} on port {}", key, port);

        let mut child = tokio::process::Command::new("tensorboard")
            .arg(format!("--logdir={}", logdir.display()))
            .arg(format!("--port={}", port))
            .arg("--bind_all")
            .arg("--load_fast=false")
            .env("TENSORBOARD_CSP", "frame-ancestors *")
            .current_dir(&run_dir)
            .spawn()
            .map_err(|e| format!("Failed to spawn tensorboard: {}", e))?;

        // Small wait to ensure it hasn't instantly crashed
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!(
                "TensorBoard process crashed immediately with status {}",
                status
            ));
        }

        let mut instances = self.instances.lock().unwrap();
        instances.insert(
            key,
            TensorBoardInstance {
                port,
                process: child,
            },
        );

        Ok(port)
    }

    /// Returns the port if TensorBoard is running.
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

    /// Stops a running TensorBoard instance.
    pub async fn stop(&self, exp: &str, run: &str) -> Result<(), String> {
        let key = format!("{}:{}", exp, run);
        let mut instance = {
            let mut instances = self.instances.lock().unwrap();
            instances.remove(&key)
        };

        if let Some(mut inst) = instance.take() {
            info!("Shutting down TensorBoard for {}", key);
            let _ = inst.process.kill().await;
            let _ = inst.process.wait().await;
        }

        Ok(())
    }

    /// Kill all TensorBoards (e.g., on server shutdown).
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
