//! Integration tests for expman.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use expman::core::{ExperimentConfig, LoggingEngine, MetricValue, RunStatus};
use tempfile::TempDir;

fn make_engine(tmp: &TempDir, name: &str) -> LoggingEngine {
    let config = ExperimentConfig {
        name: name.to_string(),
        run_name: "test_run".to_string(),
        base_dir: tmp.path().to_path_buf(),
        flush_interval_rows: 10,
        flush_interval_ms: 100,
        language: "rust".to_string(),
        env_path: None,
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
fn test_log_vector_writes_parquet() {
    let tmp = TempDir::new().unwrap();
    let engine = make_engine(&tmp, "metrics_test");

    for i in 0..100u64 {
        let mut m = HashMap::new();
        m.insert(
            "loss".to_string(),
            MetricValue::Float(1.0 - i as f64 * 0.01),
        );
        m.insert("acc".to_string(), MetricValue::Float(i as f64 * 0.01));
        engine.log_vector(m, Some(i));
    }

    engine.close(RunStatus::Finished);

    let metrics_path = engine.config().run_dir().join("vectors.parquet");
    assert!(
        metrics_path.exists(),
        "vectors.parquet should exist after close"
    );

    let rows = expman::core::storage::read_vectors(&metrics_path).unwrap();
    assert_eq!(rows.len(), 100, "Should have 100 metric rows");
}

#[test]
fn test_log_params_writes_yaml() {
    let tmp = TempDir::new().unwrap();
    let engine = make_engine(&tmp, "params_test");

    let mut params = HashMap::new();
    params.insert(
        "lr".to_string(),
        serde_yaml::Value::String("0.001".to_string()),
    );
    params.insert(
        "epochs".to_string(),
        serde_yaml::Value::Number(serde_yaml::Number::from(100i64)),
    );
    engine.log_params(params);

    engine.close(RunStatus::Finished);

    let config_path = engine.config().run_dir().join("config.yaml");
    assert!(config_path.exists(), "config.yaml should exist");
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("lr"), "config should contain 'lr'");
    assert!(content.contains("epochs"), "config should contain 'epochs'");
}

#[test]
fn test_log_vector_is_fast() {
    // Verify that 10,000 log_vector calls complete in under 100ms
    let tmp = TempDir::new().unwrap();
    let engine = make_engine(&tmp, "perf_test");

    let start = std::time::Instant::now();
    for i in 0..10_000u64 {
        let mut m = HashMap::new();
        m.insert("loss".to_string(), MetricValue::Float(i as f64 * 0.0001));
        engine.log_vector(m, Some(i));
    }
    let elapsed = start.elapsed();

    println!("10,000 log_vector calls took: {:?}", elapsed);
    assert!(
        elapsed < Duration::from_millis(100),
        "10k log_vector should complete in < 100ms, took {:?}",
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

    let meta = expman::core::storage::load_run_metadata(&run_dir).unwrap();
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
}

#[test]
fn test_parquet_schema_merge() {
    // Test that logging different metric keys across steps works (diagonal concat)
    let tmp = TempDir::new().unwrap();
    let engine = make_engine(&tmp, "schema_test");

    let mut m1 = HashMap::new();
    m1.insert("loss".to_string(), MetricValue::Float(0.5));
    engine.log_vector(m1, Some(0));

    let mut m2 = HashMap::new();
    m2.insert("loss".to_string(), MetricValue::Float(0.4));
    m2.insert("acc".to_string(), MetricValue::Float(0.8)); // new key
    engine.log_vector(m2, Some(1));

    engine.close(RunStatus::Finished);

    let metrics_path = engine.config().run_dir().join("vectors.parquet");
    let rows = expman::core::storage::read_vectors(&metrics_path).unwrap();
    assert_eq!(rows.len(), 2);
    // Row 0 should have null for "acc"
    assert!(rows[0]
        .get("acc")
        .map(|v: &serde_json::Value| v.is_null())
        .unwrap_or(true));
    // Row 1 should have acc = 0.8
    assert_eq!(
        rows[1]
            .get("acc")
            .and_then(|v: &serde_json::Value| v.as_f64()),
        Some(0.8)
    );
}

#[test]
fn test_read_latest_scalar_metrics() {
    let tmp = TempDir::new().unwrap();
    let engine = make_engine(&tmp, "scalar_test");

    for i in 0..5u64 {
        let mut m = HashMap::new();
        m.insert("loss".to_string(), MetricValue::Float(1.0 - i as f64 * 0.1));
        m.insert("acc".to_string(), MetricValue::Float(i as f64 * 0.1));
        engine.log_vector(m, Some(i));
    }
    engine.close(RunStatus::Finished);

    let metrics_path = engine.config().run_dir().join("vectors.parquet");
    let scalars = expman::core::storage::read_latest_scalar_metrics(&metrics_path).unwrap();

    // Last row (step=4): loss = 0.6, acc = 0.4
    let loss = scalars.get("loss").copied().unwrap();
    let acc = scalars.get("acc").copied().unwrap();
    assert!((loss - 0.6).abs() < 1e-9, "expected loss≈0.6, got {}", loss);
    assert!((acc - 0.4).abs() < 1e-9, "expected acc≈0.4, got {}", acc);
    // "step" and "timestamp" should not appear
    assert!(!scalars.contains_key("step"));
    assert!(!scalars.contains_key("timestamp"));
}

#[test]
fn test_corrupt_yaml_metadata() {
    let tmp = TempDir::new().unwrap();
    let run_dir = tmp.path().join("corrupt_exp").join("run1");
    std::fs::create_dir_all(&run_dir).unwrap();

    // Write invalid YAML
    std::fs::write(run_dir.join("run.yaml"), "{ invalid: [ yaml }").unwrap();

    // Should fallback to default/crashed metadata instead of panicking
    let meta = expman::core::storage::load_run_metadata(&run_dir).unwrap();
    assert_eq!(meta.status, RunStatus::Crashed);
}

#[test]
fn test_concurrent_metrics_logging() {
    let tmp = TempDir::new().unwrap();
    let engine = Arc::new(make_engine(&tmp, "concurrent_test"));

    let mut handles = vec![];
    for t in 0..4 {
        let engine_clone = engine.clone();
        handles.push(std::thread::spawn(move || {
            for i in 0..100 {
                let mut m = HashMap::new();
                m.insert(format!("thread_{}", t), MetricValue::Int(i));
                engine_clone.log_vector(m, Some((t * 100 + i) as u64));
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    engine.close(RunStatus::Finished);

    let metrics_path = engine.config().run_dir().join("vectors.parquet");
    let rows = expman::core::storage::read_vectors(&metrics_path).unwrap();
    assert_eq!(rows.len(), 400);
}

#[test]
fn test_save_artifact_absolute_path() {
    let tmp = TempDir::new().unwrap();
    let engine = make_engine(&tmp, "abs_artifact_test");

    let external_dir = TempDir::new().unwrap();
    let abs_path = external_dir.path().join("external_file.txt");
    std::fs::write(&abs_path, "external content").unwrap();

    engine.save_artifact(abs_path.clone());
    engine.close(RunStatus::Finished);

    let artifact_dest = engine
        .config()
        .run_dir()
        .join("artifacts")
        .join("external_file.txt");
    assert!(artifact_dest.exists());
    assert_eq!(
        std::fs::read_to_string(artifact_dest).unwrap(),
        "external content"
    );
}

#[test]
fn test_log_vector_replaces_step() {
    let tmp = TempDir::new().unwrap();
    let config = ExperimentConfig::new("test_replace_exp", tmp.path().to_str().unwrap());

    let run_dir = {
        let engine = LoggingEngine::new(config).unwrap();
        engine.log_vector([("loss".to_string(), 0.5.into())].into(), Some(1));

        // At this point we log more vectors. engine.close() flushes everything.
        engine.log_vector([("acc".to_string(), 0.9.into())].into(), Some(1)); // same step
        engine.log_vector([("loss".to_string(), 0.2.into())].into(), Some(2)); // new step
        let dir = engine.config().run_dir().clone();
        engine.close(RunStatus::Finished);
        dir
    };

    let vectors = expman::core::storage::read_vectors(&run_dir.join("vectors.parquet")).unwrap();
    println!("VECTORS: {:?}", vectors);
    assert_eq!(vectors.len(), 2);

    let step_1 = vectors
        .iter()
        .find(|row| row.get("step").and_then(|v| v.as_i64()) == Some(1))
        .unwrap();
    let step_2 = vectors
        .iter()
        .find(|row| row.get("step").and_then(|v| v.as_i64()) == Some(2))
        .unwrap();

    assert_eq!(step_1.get("loss").and_then(|v| v.as_f64()), Some(0.5));
    assert_eq!(step_1.get("acc").and_then(|v| v.as_f64()), Some(0.9));

    assert_eq!(step_2.get("loss").and_then(|v| v.as_f64()), Some(0.2));
}
