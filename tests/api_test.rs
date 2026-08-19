use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use expman::api::{build_router, AppState};
use serde_json::Value;
// Removed unused PathBuf
use http_body_util::BodyExt; // for `collect`
use tempfile::TempDir;
use tower::ServiceExt; // for `oneshot`

fn setup_test_env() -> (TempDir, AppState) {
    let tmp = TempDir::new().unwrap();
    let base_dir = tmp.path().to_path_buf();

    // Create a dummy experiment and run
    let exp_name = "test_exp";
    let run_name = "run1";
    let run_dir = base_dir.join(exp_name).join(run_name);
    std::fs::create_dir_all(&run_dir).unwrap();

    // Write run.yaml
    let run_meta = serde_json::json!({
        "name": run_name,
        "experiment": exp_name,
        "status": "FINISHED",
        "started_at": "2024-01-01T00:00:00Z"
    });
    std::fs::write(
        run_dir.join("run.yaml"),
        serde_yaml::to_string(&run_meta).unwrap(),
    )
    .unwrap();

    // Write experiment.yaml
    let exp_meta = serde_json::json!({
        "display_name": "Test Experiment",
        "description": "A test experiment",
        "tags": ["test", "api"]
    });
    std::fs::write(
        base_dir.join(exp_name).join("experiment.yaml"),
        serde_yaml::to_string(&exp_meta).unwrap(),
    )
    .unwrap();

    let state = AppState::new(base_dir);
    (tmp, state)
}

#[tokio::test]
async fn test_list_experiments() {
    let (_tmp, state) = setup_test_env();
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/experiments")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let exps = json.as_array().unwrap();
    assert_eq!(exps.len(), 1);
    assert_eq!(exps[0]["id"], "test_exp");
    assert_eq!(exps[0]["display_name"], "Test Experiment");
}

#[tokio::test]
async fn test_get_experiment_metadata() {
    let (_tmp, state) = setup_test_env();
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/experiments/test_exp/metadata")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["display_name"], "Test Experiment");
}

#[tokio::test]
async fn test_update_experiment_metadata() {
    let (_tmp, state) = setup_test_env();
    let app = build_router(state.clone());

    let update = serde_json::json!({
        "display_name": "Updated Name",
        "description": "Updated Desc"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/experiments/test_exp/metadata")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&update).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify persistence
    let exp_yaml =
        std::fs::read_to_string(state.base_dir.join("test_exp").join("experiment.yaml")).unwrap();
    assert!(exp_yaml.contains("Updated Name"));
}

#[tokio::test]
async fn test_list_runs() {
    let (_tmp, state) = setup_test_env();
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/experiments/test_exp/runs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let runs = json.as_array().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["name"], "run1");
}

#[tokio::test]
async fn test_get_run_metadata() {
    let (_tmp, state) = setup_test_env();
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/experiments/test_exp/runs/run1/metadata")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "FINISHED");
}

#[tokio::test]
async fn test_get_server_config() {
    let (_tmp, state) = setup_test_env();
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["live_mode"], true);
}

#[tokio::test]
async fn test_get_metrics() {
    let (_tmp, state) = setup_test_env();
    let app = build_router(state.clone());

    // Write some fake metrics using expman storage
    let run_dir = state.base_dir.join("test_exp").join("run1");
    let parquet_path = run_dir.join("vectors.parquet");

    use expman::core::models::{MetricValue, VectorRow};
    use expman::core::storage::append_vectors;
    use std::collections::HashMap;

    let mut values = HashMap::new();
    values.insert("accuracy".to_string(), MetricValue::Float(0.85));
    values.insert("loss".to_string(), MetricValue::Float(0.15));

    let rows = vec![
        VectorRow::new(values.clone(), Some(1)),
        VectorRow::new(values, Some(2)),
    ];
    append_vectors(&parquet_path, &rows).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/experiments/test_exp/runs/run1/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let metrics = json.as_array().unwrap();
    assert_eq!(metrics.len(), 2);
    assert_eq!(metrics[0]["step"], 1);
    assert_eq!(metrics[0]["accuracy"], 0.85);
}

#[tokio::test]
async fn test_artifact_content_types() {
    let (_tmp, state) = setup_test_env();
    let app = build_router(state.clone());

    let artifact_dir = state
        .base_dir
        .join("test_exp")
        .join("run1")
        .join("artifacts");
    std::fs::create_dir_all(&artifact_dir).unwrap();

    // Every extension the artifacts panel is willing to render must come back
    // with a matching content-type; anything else falls back to octet-stream.
    let cases = [
        ("frame.png", "image/png"),
        ("photo.jpg", "image/jpeg"),
        ("rollout.gif", "image/gif"),
        ("shot.webp", "image/webp"),
        ("plot.svg", "image/svg+xml"),
        ("clip.mp4", "video/mp4"),
        ("clip.webm", "video/webm"),
        ("sample.mp3", "audio/mpeg"),
        ("sample.wav", "audio/wav"),
        ("sample.flac", "audio/flac"),
        ("checkpoint.bin", "application/octet-stream"),
    ];

    for (name, expected) in cases {
        std::fs::write(artifact_dir.join(name), b"fake").unwrap();

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/experiments/test_exp/runs/run1/artifacts/content?path={name}"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK, "{name}");
        assert_eq!(
            response.headers()[axum::http::header::CONTENT_TYPE],
            expected,
            "{name}"
        );
    }
}

#[tokio::test]
async fn test_projects_crud() {
    let (_tmp, state) = setup_test_env();
    let app = build_router(state.clone());

    // 1. Create a project
    let create_payload = serde_json::json!({
        "name": "proj1",
        "display_name": "Project One",
        "description": "First test project",
        "tags": ["ml", "nlp"]
    });

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/projects")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&create_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);

    // 2. List projects
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/projects")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let projects = json.as_array().unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["id"], "proj1");
    assert_eq!(projects[0]["display_name"], "Project One");

    // 3. Assign experiment to project
    let exp_update = serde_json::json!({
        "project": "proj1"
    });

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/experiments/test_exp/metadata")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&exp_update).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    // 4. Get project detail (should include assigned experiment)
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/projects/proj1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["display_name"], "Project One");
    let exps = json["experiments"].as_array().unwrap();
    assert_eq!(exps.len(), 1);
    assert_eq!(exps[0]["id"], "test_exp");

    // 5. Update README
    let readme_payload = serde_json::json!({
        "content": "# Project One\n\nWelcome to project one."
    });

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/projects/proj1/readme")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&readme_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    // 6. Get README
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/projects/proj1/readme")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["content"]
        .as_str()
        .unwrap()
        .contains("Welcome to project one"));

    // 7. Delete project
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/projects/proj1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // 8. Verify project deleted and experiment unassigned
    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/projects")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 0);
}

/// The notebook POST route, end to end, with a project-supplied template.
///
/// Closes a gap `expman-build/memory/verification.md` calls out: the Jupyter
/// routes had no integration coverage at all, so nothing checked that the
/// template reached the handler through `ServerConfig` → `AppState`.
#[tokio::test]
async fn test_notebook_route_renders_the_configured_template() {
    let (tmp, _) = setup_test_env();
    let base_dir = tmp.path().to_path_buf();

    // The store carries its own template, at the conventional path.
    let template_dir = base_dir.join(".expman");
    std::fs::create_dir_all(&template_dir).unwrap();
    std::fs::write(
        template_dir.join("notebook.ipynb"),
        r#"{"cells": [{"cell_type": "code", "execution_count": null,
 "metadata": {}, "outputs": [], "source": ["DIR = '{{run_dir}}'\n", "EXP = '{{experiment}}'"]}],
 "metadata": {}, "nbformat": 4, "nbformat_minor": 5}"#,
    )
    .unwrap();

    let state = AppState::from_config(&expman::api::ServerConfig {
        base_dir: base_dir.clone(),
        ..Default::default()
    });
    let app = build_router(state);

    let notebook_route = "/api/experiments/test_exp/runs/run1/jupyter/notebook";
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(notebook_route)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let content = json["content"].as_str().unwrap();
    let notebook: Value = serde_json::from_str(content).unwrap();

    let source: String = notebook["cells"][0]["source"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line.as_str().unwrap())
        .collect();
    assert!(
        source.contains("EXP = 'test_exp'"),
        "the template should have been used: {source}"
    );
    assert!(
        source.contains(&format!(
            "DIR = '{}'",
            base_dir.join("test_exp").join("run1").display()
        )),
        "run_dir should be substituted absolutely: {source}"
    );
    assert!(
        notebook["metadata"]["expman"]["content_hash"].is_string(),
        "expman should stamp its provenance"
    );

    // Second POST: the file on disk stands.
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(notebook_route)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
}
