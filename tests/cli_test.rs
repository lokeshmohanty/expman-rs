// Imports cleaned up

use tempfile::TempDir;

fn setup_test_env() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let base_dir = tmp.path().to_path_buf();

    // Create a dummy experiment and run
    let exp_name = "test_exp";
    let run_name = "20240101_120000";
    let run_dir = base_dir.join(exp_name).join(run_name);
    std::fs::create_dir_all(&run_dir).unwrap();

    // Write run.yaml
    let run_meta = serde_json::json!({
        "name": run_name,
        "experiment": exp_name,
        "status": "FINISHED",
        "started_at": "2024-01-01T12:00:00Z"
    });
    std::fs::write(
        run_dir.join("run.yaml"),
        serde_yaml::to_string(&run_meta).unwrap(),
    )
    .unwrap();

    // Write config.yaml
    let config = serde_json::json!({
        "lr": 0.01,
        "batch_size": 32
    });
    std::fs::write(
        run_dir.join("config.yaml"),
        serde_yaml::to_string(&config).unwrap(),
    )
    .unwrap();

    tmp
}

#[test]
fn test_cli_list_experiments() {
    let tmp = setup_test_env();
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("exp");

    cmd.arg("list")
        .arg(tmp.path()) // Positional DIR
        .assert()
        .success()
        .stdout(predicates::str::contains("test_exp"))
        .stdout(predicates::str::contains("Experiments in:"));
}

#[test]
fn test_cli_list_runs() {
    let tmp = setup_test_env();
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("exp");

    cmd.arg("list")
        .arg(tmp.path()) // Positional DIR
        .arg("--experiment")
        .arg("test_exp")
        .assert()
        .success()
        .stdout(predicates::str::contains("20240101_120000"))
        .stdout(predicates::str::contains("FINISHED"));
}

#[test]
fn test_cli_inspect() {
    let tmp = setup_test_env();
    let run_dir = tmp.path().join("test_exp").join("20240101_120000");
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("exp");

    cmd.arg("inspect")
        .arg(run_dir)
        .assert()
        .success()
        .stdout(predicates::str::contains("Run: 20240101_120000"))
        .stdout(predicates::str::contains("lr: 0.01"));
}

#[test]
fn test_cli_clean_dry_run() {
    let tmp = setup_test_env();
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("exp");

    // Create extra runs to trigger cleaning
    for i in 1..10 {
        let run_dir = tmp.path().join("test_exp").join(format!("old_run_{}", i));
        std::fs::create_dir_all(&run_dir).unwrap();
    }

    cmd.arg("clean")
        .arg("test_exp")
        .arg("--dir")
        .arg(tmp.path()) // --dir is long arg here
        .arg("--keep")
        .arg("5")
        .assert()
        .success()
        .stdout(predicates::str::contains("Will delete 5 run(s)"))
        .stdout(predicates::str::contains("Dry run"));

    // Verify no deletion
    let runs = std::fs::read_dir(tmp.path().join("test_exp"))
        .unwrap()
        .count();
    assert!(runs >= 10);
}

#[test]
fn test_cli_export_json() {
    let tmp = setup_test_env();
    let run_dir = tmp.path().join("test_exp").join("20240101_120000");

    // Export should fail if no metrics.parquet
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("exp");
    cmd.arg("export")
        .arg(&run_dir)
        .arg("--format")
        .arg("json")
        .assert()
        .failure()
        .stderr(predicates::str::contains("No vectors.parquet found"));
}

#[test]
fn test_cli_import_nonexistent_path() {
    let tmp = setup_test_env();
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("exp");

    cmd.arg("import")
        .arg("/nonexistent/tensorboard/logs")
        .arg("--dir")
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("Input path does not exist"));
}

#[test]
fn test_cli_import_no_tfevents_in_dir() {
    let tmp = setup_test_env();
    let empty_dir = tmp.path().join("empty_tb_dir");
    std::fs::create_dir_all(&empty_dir).unwrap();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("exp");
    cmd.arg("import")
        .arg(&empty_dir)
        .arg("--dir")
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("No tfevents file found"));
}

#[test]
fn test_cli_export_tensorboard_no_data() {
    let tmp = setup_test_env();
    let run_dir = tmp.path().join("test_exp").join("20240101_120000");

    // Export tensorboard format should fail if no vectors.parquet
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("exp");
    cmd.arg("export")
        .arg(&run_dir)
        .arg("--format")
        .arg("tensorboard")
        .assert()
        .failure()
        .stderr(predicates::str::contains("No vectors.parquet found"));
}

/// Helper to create a run with actual vector data for export tests.
fn setup_test_env_with_vectors() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let base_dir = tmp.path().to_path_buf();

    let exp_name = "export_exp";
    let run_name = "20240101_120000";
    let run_dir = base_dir.join(exp_name).join(run_name);
    std::fs::create_dir_all(&run_dir).unwrap();

    // Write run.yaml
    let run_meta = serde_json::json!({
        "name": run_name,
        "experiment": exp_name,
        "status": "FINISHED",
        "started_at": "2024-01-01T12:00:00Z"
    });
    std::fs::write(
        run_dir.join("run.yaml"),
        serde_yaml::to_string(&run_meta).unwrap(),
    )
    .unwrap();

    // Use expman's engine to write actual vector data
    let config = expman::core::ExperimentConfig::new(exp_name, base_dir.to_str().unwrap());
    let config = config.with_run_name(run_name);
    let engine = expman::core::LoggingEngine::new(config).unwrap();

    for step in 0..5 {
        engine.log_vector(
            [
                ("loss".to_string(), (1.0 / (step as f64 + 1.0)).into()),
                ("accuracy".to_string(), (step as f64 * 0.1).into()),
            ]
            .into(),
            Some(step),
        );
    }
    engine.close(expman::core::RunStatus::Finished);

    tmp
}

#[test]
fn test_cli_export_csv_with_data() {
    let tmp = setup_test_env_with_vectors();
    let run_dir = tmp.path().join("export_exp").join("20240101_120000");
    let out_file = tmp.path().join("output.csv");

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("exp");
    cmd.arg("export")
        .arg(&run_dir)
        .arg("--format")
        .arg("csv")
        .arg("--output")
        .arg(&out_file)
        .assert()
        .success()
        .stdout(predicates::str::contains("Exported"));

    assert!(out_file.exists());
    let content = std::fs::read_to_string(&out_file).unwrap();
    assert!(content.contains("loss"));
    assert!(content.contains("accuracy"));
}

#[test]
fn test_cli_export_json_with_data() {
    let tmp = setup_test_env_with_vectors();
    let run_dir = tmp.path().join("export_exp").join("20240101_120000");
    let out_file = tmp.path().join("output.json");

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("exp");
    cmd.arg("export")
        .arg(&run_dir)
        .arg("--format")
        .arg("json")
        .arg("--output")
        .arg(&out_file)
        .assert()
        .success()
        .stdout(predicates::str::contains("Exported"));

    assert!(out_file.exists());
    let content = std::fs::read_to_string(&out_file).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(parsed.is_array());
}

#[test]
fn test_cli_export_tensorboard_with_data() {
    let tmp = setup_test_env_with_vectors();
    let run_dir = tmp.path().join("export_exp").join("20240101_120000");
    let out_dir = tmp.path().join("tb_output");

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("exp");
    cmd.arg("export")
        .arg(&run_dir)
        .arg("--format")
        .arg("tensorboard")
        .arg("--output")
        .arg(&out_dir)
        .assert()
        .success();

    // Check that the output directory was created and contains event files
    assert!(out_dir.exists());
    let entries: Vec<_> = std::fs::read_dir(&out_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(
        !entries.is_empty(),
        "TensorBoard output directory should contain event files"
    );
}
