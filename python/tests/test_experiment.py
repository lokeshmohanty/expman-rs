import os

import expman


def test_experiment_basic(tmp_path):
    """Test basic experiment creation and logging."""
    exp_dir = tmp_path / "experiments"
    exp_name = "test_exp"

    with expman.Experiment(exp_name, base_dir=str(exp_dir)) as exp:
        exp.log_params({"lr": 0.01})
        exp.log_metrics({"loss": 0.5}, step=0)
        run_dir = exp.run_dir

    # Assertions outside 'with' to ensure background task has flushed
    assert os.path.exists(run_dir)
    assert os.path.exists(os.path.join(run_dir, "config.yaml"))
    assert os.path.exists(os.path.join(run_dir, "metrics.parquet"))


def test_singleton_init(tmp_path):
    """Test the global singleton API."""
    exp_dir = tmp_path / "experiments"

    expman.init("singleton_exp", base_dir=str(exp_dir))
    expman.log_params({"batch_size": 32})
    expman.log_metrics({"acc": 0.9}, step=1)
    expman.close()

    # Check if files were created
    assert os.path.exists(exp_dir / "singleton_exp")


def test_artifact_save(tmp_path):
    """Test saving artifacts."""
    exp_dir = tmp_path / "experiments"
    artifact_file = tmp_path / "model.txt"
    artifact_file.write_text("hello world")

    with expman.Experiment("artifact_exp", base_dir=str(exp_dir)) as exp:
        # Pass absolute path. Rust engine should handle it.
        exp.save_artifact(str(artifact_file))
        run_dir = exp.run_dir

    # Artifacts are saved asynchronously, but close() waits for background task
    artifact_path = os.path.join(run_dir, "artifacts", "model.txt")
    assert os.path.exists(artifact_path)
    with open(artifact_path) as f:
        assert f.read() == "hello world"
