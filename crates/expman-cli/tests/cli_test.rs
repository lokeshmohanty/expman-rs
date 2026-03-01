use assert_cmd::Command;
use predicates::prelude::*;
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
    let mut cmd = Command::cargo_bin("exp").unwrap();

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
    let mut cmd = Command::cargo_bin("exp").unwrap();

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
    let mut cmd = Command::cargo_bin("exp").unwrap();

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
    let mut cmd = Command::cargo_bin("exp").unwrap();

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
    let mut cmd = Command::cargo_bin("exp").unwrap();
    cmd.arg("export")
        .arg(&run_dir)
        .arg("--format")
        .arg("json")
        .assert()
        .failure()
        .stderr(predicates::str::contains("No metrics.parquet found"));
}
