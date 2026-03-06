//! API fetch functions for the frontend (WASM).

use super::models::*;

pub(crate) async fn fetch_experiments() -> Result<Vec<Experiment>, String> {
    let resp = gloo_net::http::Request::get("/api/experiments")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!("Error fetching experiments: {}", resp.status()));
    }

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

pub(crate) async fn fetch_runs(exp_id: String) -> Result<Vec<Run>, String> {
    let resp = gloo_net::http::Request::get(&format!("/api/experiments/{}/runs", exp_id))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!("Error fetching runs: {}", resp.status()));
    }

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

pub(crate) async fn update_experiment_metadata(
    exp_id: String,
    display_name: Option<String>,
    description: Option<String>,
    tags: Option<Vec<String>>,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "display_name": display_name,
        "description": description,
        "tags": tags,
    });
    let resp = gloo_net::http::Request::patch(&format!("/api/experiments/{}/metadata", exp_id))
        .json(&payload)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!("Error updating metadata: {}", resp.status()));
    }
    Ok(())
}

#[allow(dead_code)]
pub(crate) async fn update_run_metadata(
    exp_id: String,
    run_id: String,
    name: Option<String>,
    description: Option<String>,
    tags: Option<Vec<String>>,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "name": name,
        "description": description,
        "tags": tags,
    });
    let resp = gloo_net::http::Request::patch(&format!(
        "/api/experiments/{}/runs/{}/metadata",
        exp_id, run_id
    ))
    .json(&payload)
    .map_err(|e| e.to_string())?
    .send()
    .await
    .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!("Error updating run metadata: {}", resp.status()));
    }
    Ok(())
}

pub(crate) async fn fetch_global_stats() -> Result<GlobalStats, String> {
    let resp = gloo_net::http::Request::get("/api/stats")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!("Error fetching stats: {}", resp.status()));
    }

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

pub(crate) async fn fetch_artifacts(
    exp_id: String,
    run_id: String,
) -> Result<Vec<Artifact>, String> {
    let resp = gloo_net::http::Request::get(&format!(
        "/api/experiments/{}/runs/{}/artifacts",
        exp_id, run_id
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!("Error fetching artifacts: {}", resp.status()));
    }

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

pub(crate) async fn fetch_run_metrics(
    exp_id: String,
    run_id: String,
) -> Result<Vec<serde_json::Value>, String> {
    let resp = gloo_net::http::Request::get(&format!(
        "/api/experiments/{}/runs/{}/metrics",
        exp_id, run_id
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!("Error fetching run metrics: {}", resp.status()));
    }

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

pub(crate) async fn fetch_artifact_content(
    exp_id: String,
    run_id: String,
    path: String,
) -> Result<String, String> {
    let resp = gloo_net::http::Request::get(&format!(
        "/api/experiments/{}/runs/{}/artifacts/content?path={}",
        exp_id, run_id, path
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!(
            "Error fetching artifact content: {}",
            resp.status()
        ));
    }

    resp.text().await.map_err(|e| e.to_string())
}

pub(crate) async fn fetch_run_metadata(exp_id: String, run_id: String) -> Result<Run, String> {
    let resp = gloo_net::http::Request::get(&format!(
        "/api/experiments/{}/runs/{}/metadata",
        exp_id, run_id
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!("Error fetching run metadata: {}", resp.status()));
    }

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

pub(crate) async fn check_backend() -> Result<BackendInfo, String> {
    let resp = gloo_net::http::Request::get("/api/jupyter/available")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

pub(crate) async fn fetch_jupyter_status(
    exp: String,
    run: String,
) -> Result<JupyterStatus, String> {
    let resp = gloo_net::http::Request::get(&format!(
        "/api/experiments/{}/runs/{}/jupyter/status",
        exp, run
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

pub(crate) async fn start_jupyter(exp: String, run: String) -> Result<u16, String> {
    let resp = gloo_net::http::Request::post(&format!(
        "/api/experiments/{}/runs/{}/jupyter/start",
        exp, run
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;

    let text = resp.text().await.map_err(|e| e.to_string())?;
    let res: JupyterStartResponse = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(res.port)
}

pub(crate) async fn stop_jupyter(exp: String, run: String) -> Result<(), String> {
    gloo_net::http::Request::post(&format!(
        "/api/experiments/{}/runs/{}/jupyter/stop",
        exp, run
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) async fn fetch_notebook_content(
    exp: String,
    run: String,
) -> Result<NotebookInfo, String> {
    let resp = gloo_net::http::Request::get(&format!(
        "/api/experiments/{}/runs/{}/jupyter/notebook",
        exp, run
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

pub(crate) async fn create_default_notebook(exp: String, run: String) -> Result<String, String> {
    let resp = gloo_net::http::Request::post(&format!(
        "/api/experiments/{}/runs/{}/jupyter/notebook",
        exp, run
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;

    let text = resp.text().await.map_err(|e| e.to_string())?;
    let parsed: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(parsed["content"].as_str().unwrap_or("").to_string())
}

pub(crate) async fn fetch_multi_jupyter_status(exp: String) -> Result<JupyterStatus, String> {
    let resp = gloo_net::http::Request::get(&format!("/api/experiments/{}/jupyter/status", exp))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

pub(crate) async fn start_multi_jupyter(exp: String, runs: Vec<String>) -> Result<u16, String> {
    let resp = gloo_net::http::Request::post(&format!("/api/experiments/{}/jupyter/start", exp))
        .json(&serde_json::json!({ "runs": runs }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let text = resp.text().await.map_err(|e| e.to_string())?;
    let res: JupyterStartResponse = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(res.port)
}

pub(crate) async fn stop_multi_jupyter(exp: String) -> Result<(), String> {
    gloo_net::http::Request::post(&format!("/api/experiments/{}/jupyter/stop", exp))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) async fn fetch_multi_notebook_content(exp: String) -> Result<NotebookInfo, String> {
    let resp = gloo_net::http::Request::get(&format!("/api/experiments/{}/jupyter/notebook", exp))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

pub(crate) async fn create_multi_notebook(
    exp: String,
    runs: Vec<String>,
) -> Result<String, String> {
    let resp = gloo_net::http::Request::post(&format!("/api/experiments/{}/jupyter/notebook", exp))
        .json(&serde_json::json!({ "runs": runs }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let text = resp.text().await.map_err(|e| e.to_string())?;
    let parsed: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(parsed["content"].as_str().unwrap_or("").to_string())
}

/// Extract human-readable source code from ipynb JSON cells.
pub(crate) fn extract_cell_sources(ipynb_content: &str) -> Vec<String> {
    let parsed: serde_json::Value = match serde_json::from_str(ipynb_content) {
        Ok(v) => v,
        Err(_) => return vec![ipynb_content.to_string()],
    };
    let cells = match parsed["cells"].as_array() {
        Some(c) => c,
        None => return vec![],
    };
    cells
        .iter()
        .filter_map(|cell| {
            let source = &cell["source"];
            if let Some(arr) = source.as_array() {
                Some(
                    arr.iter()
                        .filter_map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(""),
                )
            } else {
                source.as_str().map(|s| s.to_string())
            }
        })
        .collect()
}
