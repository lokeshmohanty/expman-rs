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

// ─── CSV export correctness ──────────────────────────────────────────────────

/// A run where a metric appears only partway through, and a string metric
/// contains a comma and a quote — the two shapes that used to corrupt the CSV.
fn setup_run_with_late_and_awkward_metrics() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let base_dir = tmp.path().to_path_buf();
    let exp_name = "csv_exp";
    let run_name = "20240101_120000";

    let config = expman::core::ExperimentConfig::new(exp_name, base_dir.to_str().unwrap())
        .with_run_name(run_name);
    let engine = expman::core::LoggingEngine::new(config).unwrap();

    for step in 0..4u64 {
        let mut row: std::collections::HashMap<String, expman::core::MetricValue> =
            [("loss".to_string(), (1.0 / (step as f64 + 1.0)).into())]
                .into_iter()
                .collect();
        if step >= 2 {
            // Only ever present on later rows.
            row.insert("late_metric".to_string(), (step as f64).into());
            row.insert(
                "note".to_string(),
                expman::core::MetricValue::Text("a,b \"quoted\"".to_string()),
            );
        }
        engine.log_vector(row, Some(step));
    }
    engine.close(expman::core::RunStatus::Finished);
    tmp
}

#[test]
fn test_cli_export_csv_includes_metrics_first_logged_mid_run() {
    let tmp = setup_run_with_late_and_awkward_metrics();
    let run_dir = tmp.path().join("csv_exp").join("20240101_120000");
    let out_file = tmp.path().join("out.csv");

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("exp");
    cmd.arg("export")
        .arg(&run_dir)
        .arg("--format")
        .arg("csv")
        .arg("--output")
        .arg(&out_file)
        .assert()
        .success();

    let content = std::fs::read_to_string(&out_file).unwrap();
    let header = content.lines().next().unwrap();

    // The header must be the union of every row's keys. Taking it from rows[0]
    // dropped these columns from the file entirely.
    assert!(
        header.contains("late_metric"),
        "header must include a metric first logged mid-run; got: {header}"
    );
    assert!(
        header.contains("note"),
        "header must include a metric first logged mid-run; got: {header}"
    );
    assert!(header.starts_with("step,timestamp"), "got: {header}");

    // Every row must have the same number of fields as the header. A value
    // containing a comma used to split into an extra column here.
    let expected_fields = header.matches(',').count() + 1;
    for (i, line) in content.lines().enumerate().skip(1) {
        let fields = split_csv_line(line);
        assert_eq!(
            fields.len(),
            expected_fields,
            "row {i} has {} fields, expected {expected_fields}: {line}",
            fields.len()
        );
    }

    // The string value round-trips with its comma and quote intact.
    let note_idx = header.split(',').position(|h| h == "note").unwrap();
    let last_row = split_csv_line(content.lines().last().unwrap());
    assert_eq!(last_row[note_idx], r#"a,b "quoted""#);
}

/// Minimal RFC 4180 field splitter, enough to verify the writer's output.
fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = vec![];
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => fields.push(std::mem::take(&mut current)),
            other => current.push(other),
        }
    }
    fields.push(current);
    fields
}

#[test]
fn test_cli_import_writes_readable_metadata() {
    // An imported run had no run.yaml, so every reader fell back to
    // minimal_run_metadata and the import showed up as CRASHED.
    let tmp = TempDir::new().unwrap();
    let tb_dir = tmp.path().join("tb_logs");
    std::fs::create_dir_all(&tb_dir).unwrap();
    {
        let mut writer =
            tensorboard_rs::summary_writer::SummaryWriter::new(tb_dir.to_str().unwrap());
        for step in 0..5usize {
            writer.add_scalar("loss", 1.0 - step as f32 * 0.1, step);
        }
        writer.flush();
    }

    let store = tmp.path().join("experiments");
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("exp");
    cmd.arg("import")
        .arg("--dir")
        .arg(&store)
        .arg(&tb_dir)
        .assert()
        .success();

    let exp_dir = store.join("tb_logs");
    assert!(
        exp_dir.join("experiment.yaml").exists(),
        "import must write experiment.yaml"
    );
    let run_name = std::fs::read_dir(&exp_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().is_dir())
        .expect("imported run directory")
        .file_name()
        .to_string_lossy()
        .to_string();

    let meta = expman::core::storage::load_run_metadata(&exp_dir.join(run_name)).unwrap();
    assert_eq!(
        meta.status,
        expman::core::RunStatus::Finished,
        "an imported run is complete and must not read back as CRASHED"
    );
    assert!(meta.finished_at.is_some());
}
