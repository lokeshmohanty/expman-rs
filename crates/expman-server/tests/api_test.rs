use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use expman_server::{build_router, AppState};
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

    use expman::models::{MetricValue, VectorRow};
    use expman::storage::append_vectors;
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
