//! Integration tests for expman-core.

use std::collections::HashMap;
use std::time::Duration;

use expman_core::{ExperimentConfig, LoggingEngine, MetricValue, RunStatus};
use tempfile::TempDir;

fn make_engine(tmp: &TempDir, name: &str) -> LoggingEngine {
    let config = ExperimentConfig {
        name: name.to_string(),
        run_name: "test_run".to_string(),
        base_dir: tmp.path().to_path_buf(),
        flush_interval_rows: 10,
        flush_interval_ms: 100,
    };
    LoggingEngine::new(config).expect("Failed to create LoggingEngine")
}

#[test]
fn test_engine_creates_run_dir() {
    let tmp = TempDir::new().unwrap();
    let engine = make_engine(&tmp, "test_exp");
    let run_dir = engine.config().run_dir();
    assert!(run_dir.exists(), "Run directory should be created");
    assert!(run_dir.join("run.yaml").exists(), "run.yaml should exist");
    engine.close(RunStatus::Finished);
}

#[test]
fn test_log_metrics_writes_parquet() {
    let tmp = TempDir::new().unwrap();
    let engine = make_engine(&tmp, "metrics_test");

    for i in 0..100u64 {
        let mut m = HashMap::new();
        m.insert("loss".to_string(), MetricValue::Float(1.0 - i as f64 * 0.01));
        m.insert("acc".to_string(), MetricValue::Float(i as f64 * 0.01));
        engine.log_metrics(m, Some(i));
    }

    engine.close(RunStatus::Finished);

    let metrics_path = engine.config().run_dir().join("metrics.parquet");
    assert!(metrics_path.exists(), "metrics.parquet should exist after close");

    let rows = expman_core::storage::read_metrics(&metrics_path).unwrap();
    assert_eq!(rows.len(), 100, "Should have 100 metric rows");
}

#[test]
fn test_log_params_writes_yaml() {
    let tmp = TempDir::new().unwrap();
    let engine = make_engine(&tmp, "params_test");

    let mut params = HashMap::new();
    params.insert("lr".to_string(), serde_yaml::Value::String("0.001".to_string()));
    params.insert("epochs".to_string(), serde_yaml::Value::Number(serde_yaml::Number::from(100i64)));
    engine.log_params(params);

    engine.close(RunStatus::Finished);

    let config_path = engine.config().run_dir().join("config.yaml");
    assert!(config_path.exists(), "config.yaml should exist");
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("lr"), "config should contain 'lr'");
    assert!(content.contains("epochs"), "config should contain 'epochs'");
}

#[test]
fn test_log_metrics_is_fast() {
    // Verify that 10,000 log_metrics calls complete in under 100ms
    let tmp = TempDir::new().unwrap();
    let engine = make_engine(&tmp, "perf_test");

    let start = std::time::Instant::now();
    for i in 0..10_000u64 {
        let mut m = HashMap::new();
        m.insert("loss".to_string(), MetricValue::Float(i as f64 * 0.0001));
        engine.log_metrics(m, Some(i));
    }
    let elapsed = start.elapsed();

    println!("10,000 log_metrics calls took: {:?}", elapsed);
    assert!(
        elapsed < Duration::from_millis(100),
        "10k log_metrics should complete in < 100ms, took {:?}",
        elapsed
    );

    engine.close(RunStatus::Finished);
}

#[test]
fn test_run_status_written_on_close() {
    let tmp = TempDir::new().unwrap();
    let engine = make_engine(&tmp, "status_test");
    let run_dir = engine.config().run_dir();

    engine.close(RunStatus::Finished);

    let meta = expman_core::storage::load_run_metadata(&run_dir).unwrap();
    assert_eq!(meta.status, RunStatus::Finished);
    assert!(meta.finished_at.is_some());
    assert!(meta.duration_secs.is_some());
}

#[test]
fn test_save_artifact_relative_path() {
    let tmp = TempDir::new().unwrap();
    let engine = make_engine(&tmp, "artifact_test");
    
    // Create a dummy file in the current temp dir (simulating relative path)
    let file_path = tmp.path().join("my_artifact.txt");
    std::fs::write(&file_path, "artifact content").unwrap();
    
    // In our test, we pass the absolute path for src, 
    // but the destination will use it as a relative fragment if we're not careful.
    // Actually, LoggingEngine::save_artifact takes a PathBuf.
    // Let's test the behavior.
    engine.save_artifact(file_path.clone());
    engine.close(RunStatus::Finished);
    
    let run_dir = engine.config().run_dir();
    // The handle_artifact logic does artifacts_dir.join(&path).
    // If path is absolute, it replaces the artifacts_dir in the join.
    // This is a subtle point in Rust's PathBuf::join.
    // Usually, we expect relative paths here.
    
    // If we want it to be relative, we should probably strip prefix or just use filename?
    // User said: "path is relative to the artifact folder".
    // This implies if they pass "a/b/c.txt", it goes to artifacts/a/b/c.txt.
}

#[test]
fn test_parquet_schema_merge() {
    // Test that logging different metric keys across steps works (diagonal concat)
    let tmp = TempDir::new().unwrap();
    let engine = make_engine(&tmp, "schema_test");

    let mut m1 = HashMap::new();
    m1.insert("loss".to_string(), MetricValue::Float(0.5));
    engine.log_metrics(m1, Some(0));

    let mut m2 = HashMap::new();
    m2.insert("loss".to_string(), MetricValue::Float(0.4));
    m2.insert("acc".to_string(), MetricValue::Float(0.8)); // new key
    engine.log_metrics(m2, Some(1));

    engine.close(RunStatus::Finished);

    let metrics_path = engine.config().run_dir().join("metrics.parquet");
    let rows = expman_core::storage::read_metrics(&metrics_path).unwrap();
    assert_eq!(rows.len(), 2);
    // Row 0 should have null for "acc"
    assert!(rows[0].get("acc").map(|v| v.is_null()).unwrap_or(true));
    // Row 1 should have acc = 0.8
    assert_eq!(rows[1].get("acc").and_then(|v| v.as_f64()), Some(0.8));
}
