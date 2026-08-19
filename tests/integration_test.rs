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
        project: None,
        tags: Vec::new(),
        description: None,
        // Tests assert on file contents immediately; a heartbeat would race
        // with those reads for no benefit at this timescale.
        heartbeat_interval_secs: 0,
        // Likewise sampling: it would add a system.parquet these tests do not
        // expect, and shell out to nvidia-smi on every developer machine.
        system_metrics_interval_secs: 0,
        group: None,
        rank: None,
        // Provenance shells out to git; these tests do not assert on it and the
        // subprocess would dominate their runtime.
        capture_provenance: false,
        capture_diff: false,
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

// ─── Projects, tag queries, reaping ──────────────────────────────────────────

use expman::core::models::{ExperimentMetadata, RunMetadata, RunStatus as Status};
use expman::core::storage::{self, RunQuery};

/// Write a run directly to disk, bypassing the engine, so a test can pin
/// exactly the metadata it wants to query.
fn write_run(base: &std::path::Path, exp: &str, run: &str, tags: &[&str], status: Status) {
    let run_dir = base.join(exp).join(run);
    std::fs::create_dir_all(&run_dir).unwrap();
    storage::save_run_metadata(
        &run_dir,
        &RunMetadata {
            name: run.to_string(),
            experiment: exp.to_string(),
            status,
            started_at: chrono::Utc::now(),
            tags: Some(tags.iter().map(|t| t.to_string()).collect()),
            ..Default::default()
        },
    )
    .unwrap();
}

#[test]
fn test_set_experiment_project_is_offline_and_preserves_other_fields() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path();
    let exp_dir = base.join("e1");
    std::fs::create_dir_all(&exp_dir).unwrap();
    storage::save_experiment_metadata(
        &exp_dir,
        &ExperimentMetadata {
            display_name: Some("Drift regret".to_string()),
            description: Some("keep me".to_string()),
            tags: vec!["thesis".to_string()],
            project: None,
        },
    )
    .unwrap();

    storage::set_experiment_project(base, "e1", Some("study-1")).unwrap();

    let meta = storage::load_experiment_metadata(&exp_dir).unwrap();
    assert_eq!(meta.project.as_deref(), Some("study-1"));
    // The whole point is that only `project:` moves.
    assert_eq!(meta.display_name.as_deref(), Some("Drift regret"));
    assert_eq!(meta.description.as_deref(), Some("keep me"));
    assert_eq!(meta.tags, vec!["thesis".to_string()]);

    storage::set_experiment_project(base, "e1", None).unwrap();
    assert_eq!(
        storage::load_experiment_metadata(&exp_dir).unwrap().project,
        None
    );
}

#[test]
fn test_engine_writes_project_and_tags_at_creation() {
    let tmp = TempDir::new().unwrap();
    let mut config = ExperimentConfig::new("e1", tmp.path());
    config.project = Some("study-1".to_string());
    config.tags = vec!["arm:tiered".to_string(), "seed:1".to_string()];
    config.description = Some("first run".to_string());
    config.heartbeat_interval_secs = 0;
    config.system_metrics_interval_secs = 0;
    config.system_metrics_interval_secs = 0;

    let engine = LoggingEngine::new(config).unwrap();
    let run_dir = engine.config().run_dir();
    engine.close(RunStatus::Finished);

    let exp_meta = storage::load_experiment_metadata(&tmp.path().join("e1")).unwrap();
    assert_eq!(exp_meta.project.as_deref(), Some("study-1"));

    let run_meta = storage::load_run_metadata(&run_dir).unwrap();
    assert_eq!(
        run_meta.tags,
        Some(vec!["arm:tiered".to_string(), "seed:1".to_string()])
    );
    assert_eq!(run_meta.description.as_deref(), Some("first run"));
}

#[test]
fn test_engine_updates_project_when_experiment_yaml_already_exists() {
    // experiment.yaml is only written when absent, so an explicit project= on a
    // later run must still take effect rather than being silently ignored.
    let tmp = TempDir::new().unwrap();
    let mut first = ExperimentConfig::new("e1", tmp.path());
    first.heartbeat_interval_secs = 0;
    first.system_metrics_interval_secs = 0;
    LoggingEngine::new(first)
        .unwrap()
        .close(RunStatus::Finished);

    let mut second = ExperimentConfig::new("e1", tmp.path()).with_run_name("second");
    second.project = Some("study-2".to_string());
    second.heartbeat_interval_secs = 0;
    second.system_metrics_interval_secs = 0;
    LoggingEngine::new(second)
        .unwrap()
        .close(RunStatus::Finished);

    assert_eq!(
        storage::load_experiment_metadata(&tmp.path().join("e1"))
            .unwrap()
            .project
            .as_deref(),
        Some("study-2")
    );
}

#[test]
fn test_parse_tag_expr() {
    assert_eq!(
        storage::parse_tag_expr("arm:tiered"),
        vec![vec!["arm:tiered".to_string()]]
    );
    assert_eq!(
        storage::parse_tag_expr("arm:tiered AND study:1"),
        vec![vec!["arm:tiered".to_string()], vec!["study:1".to_string()]]
    );
    assert_eq!(
        storage::parse_tag_expr("arm:tiered AND (study:1 OR study:2)"),
        vec![
            vec!["arm:tiered".to_string()],
            vec!["study:1".to_string(), "study:2".to_string()]
        ]
    );
    // Comma is AND, pipe is OR.
    assert_eq!(
        storage::parse_tag_expr("a,b|c"),
        vec![
            vec!["a".to_string()],
            vec!["b".to_string(), "c".to_string()]
        ]
    );
    // A tag that merely contains the operator letters must survive intact.
    assert_eq!(
        storage::parse_tag_expr("brand:ORACLE"),
        vec![vec!["brand:ORACLE".to_string()]]
    );
}

#[test]
fn test_query_runs_by_project_tag_and_status() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path();

    write_run(
        base,
        "e1",
        "r1",
        &["arm:tiered", "study:1"],
        Status::Finished,
    );
    write_run(base, "e1", "r2", &["arm:flat", "study:1"], Status::Finished);
    write_run(
        base,
        "e2",
        "r3",
        &["arm:tiered", "study:2"],
        Status::Running,
    );
    storage::set_experiment_project(base, "e1", Some("study-1")).unwrap();
    storage::set_experiment_project(base, "e2", Some("study-2")).unwrap();

    let all = storage::query_runs(base, &RunQuery::default()).unwrap();
    assert_eq!(all.len(), 3);

    let by_project = storage::query_runs(
        base,
        &RunQuery {
            project: Some("study-1".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(by_project.len(), 2);
    assert!(by_project.iter().all(|r| r.experiment == "e1"));

    // AND across clauses.
    let tiered_study1 = storage::query_runs(
        base,
        &RunQuery {
            tags: storage::parse_tag_expr("arm:tiered AND study:1"),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(tiered_study1.len(), 1);
    assert_eq!(tiered_study1[0].run, "r1");

    // OR within a clause.
    let either_study = storage::query_runs(
        base,
        &RunQuery {
            tags: storage::parse_tag_expr("arm:tiered AND (study:1 OR study:2)"),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(either_study.len(), 2);

    let running = storage::query_runs(
        base,
        &RunQuery {
            status: Some(Status::Running),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(running.len(), 1);
    assert_eq!(running[0].run, "r3");

    // The project of a run is resolved through its experiment.
    assert_eq!(running[0].project.as_deref(), Some("study-2"));
}

#[test]
fn test_stale_detection_uses_heartbeat_and_falls_back_to_started_at() {
    let now = chrono::Utc::now();
    let hour = chrono::Duration::hours(1);

    let mut meta = RunMetadata {
        status: Status::Running,
        started_at: now - chrono::Duration::hours(10),
        heartbeat_at: Some(now - chrono::Duration::minutes(1)),
        ..Default::default()
    };
    // A long job that is still beating is NOT stale, however old it is — this is
    // the case a started_at-only rule would wrongly kill.
    assert!(!storage::is_run_stale(&meta, hour, now));
    assert!(storage::looks_alive(&meta, now));

    meta.heartbeat_at = Some(now - chrono::Duration::hours(3));
    assert!(storage::is_run_stale(&meta, hour, now));
    assert!(!storage::looks_alive(&meta, now));

    // Pre-heartbeat runs fall back to started_at.
    meta.heartbeat_at = None;
    assert!(storage::is_run_stale(&meta, hour, now));
    // ...but the dashboard's own policy is deliberately more forgiving for them.
    assert!(storage::looks_alive(&meta, now));

    // A finished run is never stale, whatever the timestamps say.
    meta.status = Status::Finished;
    assert!(!storage::is_run_stale(&meta, hour, now));
}

#[test]
fn test_project_sync_is_one_way_and_reconciles_membership() {
    use expman::core::projects::{self, ProjectSpec};

    let tmp = TempDir::new().unwrap();
    let base = tmp.path();
    for exp in ["e1", "e2", "e3"] {
        std::fs::create_dir_all(base.join(exp)).unwrap();
    }
    storage::set_experiment_project(base, "e3", Some("study-1")).unwrap();

    let spec = ProjectSpec {
        name: "study-1".to_string(),
        display_name: Some("Study 1".to_string()),
        generated_from: Some("studies.yaml".to_string()),
        experiments: vec!["e1".to_string(), "e2".to_string()],
        ..Default::default()
    };
    let report = projects::sync_project(base, &spec).unwrap();

    assert_eq!(report.assigned, vec!["e1".to_string(), "e2".to_string()]);
    // e3 was in the project but has dropped out of the manifest.
    assert_eq!(report.unassigned, vec!["e3".to_string()]);

    let meta = storage::load_project_metadata(base, "study-1").unwrap();
    assert!(meta.generated, "sync must mark the project generated");
    assert_eq!(meta.generated_from.as_deref(), Some("studies.yaml"));

    let readme = storage::load_project_readme(base, "study-1")
        .unwrap()
        .unwrap();
    assert!(
        projects::is_generated_readme(&readme),
        "generated README must carry the do-not-edit marker"
    );

    assert_eq!(
        storage::list_project_experiments(base, "study-1").unwrap(),
        vec!["e1".to_string(), "e2".to_string()]
    );
}

// ─── Append-only write path ──────────────────────────────────────────────────

#[test]
fn test_metrics_are_readable_while_the_run_is_still_open() {
    // The whole point of the segment writer: a live run's data must be visible
    // before close(). Previously every flush rewrote vectors.parquet, so this
    // happened to work; with append-only segments a reader that only looks at
    // the Parquet would see nothing until the run ended.
    let tmp = TempDir::new().unwrap();
    let mut config = ExperimentConfig::new("live", tmp.path());
    config.flush_interval_rows = 5;
    config.heartbeat_interval_secs = 0;
    config.system_metrics_interval_secs = 0;

    let engine = LoggingEngine::new(config).unwrap();
    let run_dir = engine.config().run_dir();

    for step in 0..20u64 {
        let mut row = HashMap::new();
        row.insert(
            "loss".to_string(),
            MetricValue::Float(1.0 - step as f64 * 0.01),
        );
        engine.log_vector(row, Some(step));
    }
    // Force the buffer out without closing the run.
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(engine.flush())
        .unwrap();

    assert!(
        !run_dir.join("vectors.parquet").exists(),
        "nothing should be compacted yet — the run is still open"
    );
    let live = expman::core::storage::read_run_vectors(&run_dir).unwrap();
    assert_eq!(live.len(), 20, "a live run must expose its flushed metrics");

    engine.close(RunStatus::Finished);

    // After close the segments are folded away and the Parquet is authoritative.
    assert!(run_dir.join("vectors.parquet").exists());
    let files: Vec<String> = std::fs::read_dir(&run_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".arrow"))
        .collect();
    assert!(
        files.is_empty(),
        "segments should be compacted away: {files:?}"
    );

    let after = expman::core::storage::read_run_vectors(&run_dir).unwrap();
    assert_eq!(after.len(), 20);
}

#[test]
fn test_metric_appearing_mid_run_rolls_a_segment_and_still_reads_back() {
    // One IPC stream carries one schema, so a new metric key must roll a new
    // segment. The union across segments has to be lossless.
    let tmp = TempDir::new().unwrap();
    let mut config = ExperimentConfig::new("schema", tmp.path());
    config.flush_interval_rows = 2;
    config.heartbeat_interval_secs = 0;
    config.system_metrics_interval_secs = 0;

    let engine = LoggingEngine::new(config).unwrap();
    let run_dir = engine.config().run_dir();

    for step in 0..4u64 {
        engine.log_vector(
            [("loss".to_string(), MetricValue::Float(step as f64))].into(),
            Some(step),
        );
    }
    // `acc` shows up only from step 4 — the case that silently lost a column in
    // the CSV export, and that would break a single-schema IPC stream.
    for step in 4..8u64 {
        let mut row = HashMap::new();
        row.insert("loss".to_string(), MetricValue::Float(step as f64));
        row.insert("acc".to_string(), MetricValue::Float(step as f64 * 0.1));
        engine.log_vector(row, Some(step));
    }
    engine.close(RunStatus::Finished);

    let rows = expman::core::storage::read_run_vectors(&run_dir).unwrap();
    assert_eq!(rows.len(), 8);

    let early = rows.iter().find(|r| r["step"].as_i64() == Some(1)).unwrap();
    assert!(early.get("acc").map(|v| v.is_null()).unwrap_or(true));

    let late = rows.iter().find(|r| r["step"].as_i64() == Some(5)).unwrap();
    assert_eq!(late["acc"].as_f64(), Some(0.5));
    assert_eq!(late["loss"].as_f64(), Some(5.0));
}

#[test]
fn test_concurrent_metadata_updates_do_not_clobber_each_other() {
    // Under DDP, N ranks share a run directory and each ticks its own metadata
    // update. A bare load-mutate-save races and drops the loser's field.
    use std::sync::Arc as StdArc;

    let tmp = TempDir::new().unwrap();
    let run_dir = tmp.path().join("exp").join("run");
    std::fs::create_dir_all(&run_dir).unwrap();
    expman::core::storage::save_run_metadata(
        &run_dir,
        &expman::core::models::RunMetadata {
            name: "run".into(),
            experiment: "exp".into(),
            ..Default::default()
        },
    )
    .unwrap();

    let dir = StdArc::new(run_dir.clone());
    let handles: Vec<_> = (0..8)
        .map(|i| {
            let dir = StdArc::clone(&dir);
            std::thread::spawn(move || {
                for _ in 0..10 {
                    let _ = expman::core::storage::update_run_metadata(&dir, |meta| {
                        let tags = meta.tags.get_or_insert_with(Vec::new);
                        let tag = format!("rank:{i}");
                        if !tags.contains(&tag) {
                            tags.push(tag);
                        }
                    });
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }

    let meta = expman::core::storage::load_run_metadata(&run_dir).unwrap();
    let mut tags = meta.tags.unwrap_or_default();
    tags.sort();
    assert_eq!(tags.len(), 8, "every rank's tag must survive: {tags:?}");
}

// ─── Downsampling ────────────────────────────────────────────────────────────

fn row(step: u64, value: f64) -> std::collections::HashMap<String, serde_json::Value> {
    let mut r = std::collections::HashMap::new();
    r.insert("step".to_string(), serde_json::json!(step));
    r.insert("loss".to_string(), serde_json::json!(value));
    r
}

#[test]
fn downsampling_preserves_spikes_that_stride_sampling_would_drop() {
    // The whole point: a loss spike is one row among thousands. Naive stride
    // sampling loses it, and losing it is exactly the case a user is looking at
    // the chart to find.
    let mut rows: Vec<_> = (0..5000u64).map(|i| row(i, 1.0)).collect();
    rows[1234] = row(1234, 99.0);
    rows[4321] = row(4321, -42.0);

    let out = expman::core::storage::downsample_rows(rows, 200);
    assert!(out.len() <= 200, "must respect the cap, got {}", out.len());

    let values: Vec<f64> = out.iter().map(|r| r["loss"].as_f64().unwrap()).collect();
    assert!(values.contains(&99.0), "the positive spike must survive");
    assert!(values.contains(&-42.0), "the negative spike must survive");
}

#[test]
fn downsampling_keeps_the_endpoints_exact() {
    let rows: Vec<_> = (0..1000u64).map(|i| row(i, i as f64)).collect();
    let out = expman::core::storage::downsample_rows(rows, 50);
    assert_eq!(out.first().unwrap()["step"].as_u64(), Some(0));
    assert_eq!(out.last().unwrap()["step"].as_u64(), Some(999));
}

#[test]
fn downsampling_is_a_noop_below_the_cap() {
    let rows: Vec<_> = (0..100u64).map(|i| row(i, i as f64)).collect();
    let out = expman::core::storage::downsample_rows(rows.clone(), 2000);
    assert_eq!(out.len(), rows.len());
}

#[test]
fn downsampling_handles_rows_with_no_numeric_column() {
    // Falls back to a stride rather than panicking or returning nothing.
    let rows: Vec<_> = (0..1000u64)
        .map(|i| {
            let mut r = std::collections::HashMap::new();
            r.insert("step".to_string(), serde_json::json!(i));
            r.insert("note".to_string(), serde_json::json!("text"));
            r
        })
        .collect();
    let out = expman::core::storage::downsample_rows(rows, 100);
    assert!(!out.is_empty() && out.len() <= 100);
}

#[test]
#[ignore = "diagnostic, not an assertion"]
fn profile_query_cost() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path();
    for i in 0..800 {
        write_run(
            base,
            &format!("exp{}", i % 10),
            &format!("r{i:05}"),
            &["a:1"],
            Status::Finished,
        );
    }
    let t = std::time::Instant::now();
    let runs = expman::core::storage::query_runs(base, &RunQuery::default()).unwrap();
    let full = t.elapsed();

    let t = std::time::Instant::now();
    let _ = expman::core::storage::query_runs(base, &RunQuery::default()).unwrap();
    let warm = t.elapsed();
    println!("  cold: {full:?}  warm: {warm:?}");

    let t = std::time::Instant::now();
    let mut n = 0;
    for e in &runs {
        let _ = std::fs::metadata(std::path::Path::new(&e.path).join("run.yaml"));
        n += 1;
    }
    let stats = t.elapsed();

    let t = std::time::Instant::now();
    for e in &runs {
        let _ = expman::core::storage::load_run_metadata(std::path::Path::new(&e.path));
    }
    let parses = t.elapsed();

    println!("query_runs({n} runs): {full:?} | {n} stats: {stats:?} | {n} yaml parses: {parses:?}");
}

#[test]
fn cached_metadata_reads_see_writes() {
    // The memo is keyed on (mtime, len). A cache that served a stale run.yaml
    // after a write would silently freeze the dashboard on old values, which is
    // worse than the cost it saves.
    let tmp = TempDir::new().unwrap();
    let run_dir = tmp.path().join("exp").join("run");
    std::fs::create_dir_all(&run_dir).unwrap();

    let mut meta = expman::core::models::RunMetadata {
        name: "run".into(),
        experiment: "exp".into(),
        status: Status::Running,
        ..Default::default()
    };
    expman::core::storage::save_run_metadata(&run_dir, &meta).unwrap();
    assert_eq!(
        expman::core::storage::load_run_metadata_cached(&run_dir)
            .unwrap()
            .status,
        Status::Running
    );

    // Same length, different content: mtime alone must catch this.
    meta.status = Status::Crashed;
    std::thread::sleep(std::time::Duration::from_millis(20));
    expman::core::storage::save_run_metadata(&run_dir, &meta).unwrap();

    assert_eq!(
        expman::core::storage::load_run_metadata_cached(&run_dir)
            .unwrap()
            .status,
        Status::Crashed,
        "the memo must not serve a stale run.yaml"
    );
}

#[test]
#[cfg(feature = "cli")]
fn test_reap_compacts_the_run_it_marks_crashed() {
    // A hard-killed run never reaches the engine's close path, so its metrics
    // stay as live `.arrow` segments that every later read re-parses. Reaping
    // used to rewrite only the status, leaving the run terminal AND slow — one
    // real store accumulated 6589 orphaned segments across 14 such runs.
    let tmp = TempDir::new().unwrap();
    let engine = make_engine(&tmp, "reap_compacts");
    let run_dir = engine.config().run_dir();

    for step in 0..20u64 {
        let mut row = HashMap::new();
        row.insert("loss".to_string(), MetricValue::Float(step as f64));
        engine.log_vector(row, Some(step));
    }
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(engine.flush())
        .unwrap();

    // Abandon it exactly as a SIGKILL would: no close, no compaction, and the
    // metadata left saying RUNNING. `forget` is the point — dropping would run
    // the orderly shutdown this test exists to bypass.
    std::mem::forget(engine);

    let segments = |dir: &std::path::Path| -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".arrow"))
            .collect()
    };
    assert!(
        !segments(&run_dir).is_empty(),
        "the abandoned run should still hold live segments"
    );
    let before = expman::core::storage::read_run_vectors(&run_dir).unwrap();
    assert_eq!(before.len(), 20);

    std::thread::sleep(std::time::Duration::from_millis(20));
    expman::cli::cmd_reap(tmp.path().to_path_buf(), "0s", None, None, true).unwrap();

    let meta = expman::core::storage::load_run_metadata(&run_dir).unwrap();
    assert_eq!(meta.status, RunStatus::Crashed);
    assert!(
        segments(&run_dir).is_empty(),
        "reap must fold the segments away, not just relabel the run: {:?}",
        segments(&run_dir)
    );
    assert!(run_dir.join("vectors.parquet").exists());

    // Compaction is a storage change, never a data change.
    let after = expman::core::storage::read_run_vectors(&run_dir).unwrap();
    assert_eq!(after.len(), 20, "compaction must not lose a row");
}
