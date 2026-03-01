import os
import time

import yaml

import expman


def test_experiment_basic(tmp_path):
    """Test basic experiment creation and logging."""
    exp_dir = tmp_path / "experiments"
    exp_name = "test_exp"

    with expman.Experiment(exp_name, base_dir=str(exp_dir)) as exp:
        exp.log_params({"lr": 0.01})
        exp.log_vector({"loss": 0.5}, step=0)
        run_dir = exp.run_dir

    # Assertions outside 'with' to ensure background task has flushed
    assert os.path.exists(run_dir)
    assert os.path.exists(os.path.join(run_dir, "config.yaml"))
    assert os.path.exists(os.path.join(run_dir, "vectors.parquet"))


def test_singleton_init(tmp_path):
    """Test the global singleton API."""
    exp_dir = tmp_path / "experiments"

    expman.init("singleton_exp", base_dir=str(exp_dir))
    expman.log_params({"batch_size": 32})
    expman.log_vector({"acc": 0.9}, step=1)
    expman.close()

    # Check if files were created
    assert os.path.exists(exp_dir / "singleton_exp")


def test_artifact_save(tmp_path):
    """Test saving artifacts."""
    exp_dir = tmp_path / "experiments"
    artifact_file = tmp_path / "model.txt"
    artifact_file.write_text("hello world")

    with expman.Experiment("artifact_exp", base_dir=str(exp_dir)) as exp:
        exp.save_artifact(str(artifact_file))
        run_dir = exp.run_dir

    artifact_path = os.path.join(run_dir, "artifacts", "model.txt")
    assert os.path.exists(artifact_path)
    with open(artifact_path) as f:
        assert f.read() == "hello world"


def test_complex_types(tmp_path):
    """Test logging complex types (converted to strings or handled)."""
    exp_dir = tmp_path / "experiments"
    
    with expman.Experiment("complex_exp", base_dir=str(exp_dir)) as exp:
        exp.log_params({
            "list": [1, 2, 3],
            "dict": {"a": 1},
            "tuple": (1, 2)
        })
        run_dir = exp.run_dir
        
    config_path = os.path.join(run_dir, "config.yaml")
    with open(config_path) as f:
        cfg = yaml.safe_load(f)
        # Verify they are logged (likely as strings)
        assert "list" in cfg
        assert "dict" in cfg


def test_experiment_crash(tmp_path):
    """Test that crash (exception) sets status to FAILED."""
    exp_dir = tmp_path / "experiments"
    
    run_dir = None
    try:
        with expman.Experiment("crash_exp", base_dir=str(exp_dir)) as exp:
            run_dir = exp.run_dir
            raise RuntimeError("Intentional crash")
    except RuntimeError:
        pass
        
    assert run_dir is not None
    run_yaml_path = os.path.join(run_dir, "run.yaml")
    assert os.path.exists(run_yaml_path)
    with open(run_yaml_path) as f:
        meta = yaml.safe_load(f)
        assert meta["status"] == "FAILED"


def test_vectors_vs_scalar(tmp_path):
    """Verify log_vector (append) vs log_scalar (replace) behavior."""
    exp_dir = tmp_path / "experiments"
    exp_name = "test_vectors_scalar"

    with expman.Experiment(exp_name, base_dir=str(exp_dir)) as exp:
        run_dir = exp.run_dir
        
        # log_vector (append)
        exp.log_vector({"acc": 0.8}, step=0)
        exp.log_vector({"acc": 0.9}, step=1)
        
        # log_scalar (replace)
        exp.log_scalar("status", "starting")
        exp.log_scalar("status", "running")
        exp.log_scalar("lr", 0.1)
        exp.log_scalar("lr", 0.01)

    # Allow some time for background flush
    time.sleep(0.5)

    # 1. Verify vectors.parquet exists
    vectors_path = os.path.join(run_dir, "vectors.parquet")
    assert os.path.exists(vectors_path)
    assert os.path.getsize(vectors_path) > 0

    # 2. Verify run.yaml (separate storage)
    run_yaml_path = os.path.join(run_dir, "run.yaml")
    assert os.path.exists(run_yaml_path)
    with open(run_yaml_path) as f:
        meta = yaml.safe_load(f)
        
        # scalars should be in its own section
        scalars = meta["scalars"]
        assert scalars["status"] == "running"
        assert scalars["lr"] == 0.01
        
        # vectors should be in its own section (latest values)
        vectors = meta["vectors"]
        assert vectors["acc"] == 0.9
        
        # ensure no leakage
        assert "acc" not in scalars
        assert "status" not in vectors
