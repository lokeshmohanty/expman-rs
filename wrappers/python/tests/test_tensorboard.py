"""Comprehensive tests for the expman TensorBoard SummaryWriter drop-in replacement."""

import os
import warnings
from pathlib import Path

import pytest

import expman
from expman.tensorboard import SummaryWriter

# ── Basic Creation & Cleanup ────────────────────────────────────────────────


def test_summary_writer_creates_expman_run(tmp_path):
    """SummaryWriter should create the expman directory structure."""
    log_dir = str(tmp_path / "test_tb_run")
    writer = SummaryWriter(log_dir=log_dir)

    writer.add_scalar("loss", 0.5, 0)
    writer.add_scalar("loss", 0.4, 1)
    writer.close()

    exp_dir = os.path.join(tmp_path, "test_tb_run")
    assert os.path.exists(exp_dir)
    runs = [d for d in os.listdir(exp_dir) if os.path.isdir(os.path.join(exp_dir, d))]
    assert len(runs) == 1

    run_dir = os.path.join(exp_dir, runs[0])
    assert os.path.exists(os.path.join(run_dir, "vectors.parquet"))


def test_context_manager(tmp_path):
    """SummaryWriter should work as a context manager and auto-close."""
    log_dir = str(tmp_path / "ctx_test")

    with SummaryWriter(log_dir=log_dir) as writer:
        writer.add_scalar("metric", 1.0, 0)
        writer.add_scalar("metric", 2.0, 1)

    # Should be closed — check that the run directory exists
    exp_dir = os.path.join(tmp_path, "ctx_test")
    assert os.path.exists(exp_dir)
    runs = [d for d in os.listdir(exp_dir) if os.path.isdir(os.path.join(exp_dir, d))]
    assert len(runs) == 1


def test_default_log_dir():
    """SummaryWriter with no log_dir should generate a default path."""
    writer = SummaryWriter()
    assert writer.log_dir is not None
    assert "runs/" in writer.log_dir
    writer.close()


# ── Scalar Logging ──────────────────────────────────────────────────────────


def test_add_scalar_various_types(tmp_path):
    """add_scalar should accept int and float values."""
    log_dir = str(tmp_path / "scalar_types")
    with SummaryWriter(log_dir=log_dir) as writer:
        writer.add_scalar("int_metric", 42, 0)
        writer.add_scalar("float_metric", 3.14, 1)
        writer.add_scalar("zero", 0, 2)
        writer.add_scalar("negative", -1.5, 3)

    exp_dir = os.path.join(tmp_path, "scalar_types")
    runs = [d for d in os.listdir(exp_dir) if os.path.isdir(os.path.join(exp_dir, d))]
    assert len(runs) == 1
    run_dir = os.path.join(exp_dir, runs[0])
    assert os.path.exists(os.path.join(run_dir, "vectors.parquet"))


def test_add_scalar_without_step(tmp_path):
    """add_scalar should work without a global_step argument."""
    log_dir = str(tmp_path / "no_step")
    with SummaryWriter(log_dir=log_dir) as writer:
        writer.add_scalar("loss", 0.5)
        writer.add_scalar("loss", 0.4)


# ── Grouped Scalars ─────────────────────────────────────────────────────────


def test_add_scalars_prefixing(tmp_path):
    """add_scalars should prefix each key with main_tag/."""
    log_dir = str(tmp_path / "scalars_prefix")
    with SummaryWriter(log_dir=log_dir) as writer:
        writer.add_scalars("metrics", {"accuracy": 0.9, "precision": 0.8}, 0)
        writer.add_scalars("metrics", {"accuracy": 0.95, "precision": 0.85}, 1)

    exp_dir = os.path.join(tmp_path, "scalars_prefix")
    runs = [d for d in os.listdir(exp_dir) if os.path.isdir(os.path.join(exp_dir, d))]
    assert len(runs) == 1


def test_add_scalars_empty_dict(tmp_path):
    """add_scalars with an empty dict should not crash."""
    log_dir = str(tmp_path / "empty_scalars")
    with SummaryWriter(log_dir=log_dir) as writer:
        writer.add_scalars("empty", {}, 0)


# ── Text Logging ────────────────────────────────────────────────────────────


def test_add_text(tmp_path):
    """add_text should not raise and should log via info."""
    log_dir = str(tmp_path / "text_test")
    with SummaryWriter(log_dir=log_dir) as writer:
        writer.add_text("notes", "hello world", 0)
        writer.add_text("notes", "second note", 1)


# ── Hyperparameters ─────────────────────────────────────────────────────────


def test_add_hparams(tmp_path):
    """add_hparams should log params and metrics without error."""
    log_dir = str(tmp_path / "hparams_test")
    with SummaryWriter(log_dir=log_dir) as writer:
        writer.add_hparams(
            {"lr": 0.001, "batch_size": 32},
            {"hparam/accuracy": 0.95, "hparam/loss": 0.05},
        )


def test_add_hparams_empty_metrics(tmp_path):
    """add_hparams with empty metric_dict should still log params."""
    log_dir = str(tmp_path / "hparams_empty")
    with SummaryWriter(log_dir=log_dir) as writer:
        writer.add_hparams({"lr": 0.001}, {})


def test_add_hparams_none_metrics(tmp_path):
    """add_hparams with None metric_dict should not crash."""
    log_dir = str(tmp_path / "hparams_none")
    with SummaryWriter(log_dir=log_dir) as writer:
        writer.add_hparams({"lr": 0.001}, None)


# ── Stub Methods (No-ops) ──────────────────────────────────────────────────


@pytest.mark.parametrize(
    "method_name",
    [
        "add_histogram",
        "add_image",
        "add_images",
        "add_figure",
        "add_video",
        "add_audio",
        "add_graph",
        "add_embedding",
        "add_pr_curve",
        "add_custom_scalars",
        "add_mesh",
    ],
)
def test_compat_methods_never_break_a_training_run(tmp_path, method_name):
    """Odd arguments must not raise out of the compatibility layer.

    This wraps code written for TensorBoard, often mid-way through a long
    training run. Reporting a problem is right; killing the run over an
    unencodable image is not.
    """
    log_dir = str(tmp_path / f"compat_{method_name}")
    with SummaryWriter(log_dir=log_dir) as writer:
        method = getattr(writer, method_name)
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            method("tag", "data", 0)
            method(some_kwarg="value")
            method()


def test_unsupported_methods_warn_instead_of_dropping_silently(tmp_path):
    """The bug this replaced: these were no-ops, so data vanished unannounced."""
    log_dir = str(tmp_path / "warns")
    with SummaryWriter(log_dir=log_dir) as writer:
        with pytest.warns(UserWarning, match="add_graph"):
            writer.add_graph(object())
        with pytest.warns(UserWarning, match="add_embedding"):
            writer.add_embedding(object())


def test_unsupported_methods_warn_only_once(tmp_path):
    """A per-epoch call must not turn the log into a wall of warnings."""
    log_dir = str(tmp_path / "warn_once")
    with SummaryWriter(log_dir=log_dir) as writer:
        with pytest.warns(UserWarning):
            writer.add_graph(object())
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            for _ in range(5):
                writer.add_graph(object())
        assert caught == [], f"expected no repeat warnings, got {len(caught)}"


def test_images_and_histograms_are_actually_stored(tmp_path):
    """The other half of the fix: supported calls must persist, not just not-raise."""
    log_dir = str(tmp_path / "stored")
    png = _tiny_png()
    with SummaryWriter(log_dir=log_dir) as writer:
        writer.add_image("samples", png, 0)
        writer.add_histogram("weights", [0.1, 0.2, 0.3, 0.9, 1.0], 0)
        run_dir = writer._exp.run_dir

    media = expman.read_media(run_dir)
    assert [m["tag"] for m in media] == ["samples"]
    assert (Path(run_dir) / media[0]["file"]).exists()

    hists = expman.read_histograms(run_dir)
    assert [h["tag"] for h in hists] == ["weights"]
    assert hists[0]["total"] == 5


def _tiny_png() -> bytes:
    """A 1x1 PNG, so these tests need no image library."""
    import base64

    return base64.b64decode(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="
    )


# ── Flush ───────────────────────────────────────────────────────────────────


def test_flush_is_noop(tmp_path):
    """flush() should be a no-op and not raise."""
    log_dir = str(tmp_path / "flush_test")
    with SummaryWriter(log_dir=log_dir) as writer:
        writer.add_scalar("x", 1.0, 0)
        writer.flush()
        writer.flush()


# ── Multiple Writers ────────────────────────────────────────────────────────


def test_multiple_sequential_writers(tmp_path):
    """Multiple writers to different log_dirs should not interfere."""
    for i in range(3):
        log_dir = str(tmp_path / f"multi_writer_{i}")
        with SummaryWriter(log_dir=log_dir) as writer:
            writer.add_scalar("val", float(i), 0)

    for i in range(3):
        exp_dir = os.path.join(tmp_path, f"multi_writer_{i}")
        assert os.path.exists(exp_dir)
        runs = [d for d in os.listdir(exp_dir) if os.path.isdir(os.path.join(exp_dir, d))]
        assert len(runs) == 1


# ── Log Dir Mapping ─────────────────────────────────────────────────────────


def test_log_dir_with_path_separator(tmp_path):
    """log_dir='base/name' should map to base_dir='base', name='name'."""
    log_dir = str(tmp_path / "mybase" / "myexp")
    with SummaryWriter(log_dir=log_dir) as writer:
        writer.add_scalar("x", 1.0, 0)

    # The exp dir should be at mybase/myexp
    assert os.path.isdir(os.path.join(tmp_path, "mybase", "myexp"))


def test_log_dir_without_path_separator(tmp_path):
    """log_dir='name' (no slash) should use 'experiments' as base_dir."""
    # Use chdir to tmp_path so 'experiments' is created there
    old_cwd = os.getcwd()
    os.chdir(str(tmp_path))
    try:
        with SummaryWriter(log_dir="standalone_exp") as writer:
            writer.add_scalar("x", 1.0, 0)
        assert os.path.isdir(os.path.join(tmp_path, "experiments", "standalone_exp"))
    finally:
        os.chdir(old_cwd)
