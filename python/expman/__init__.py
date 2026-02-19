"""
expman: High-performance experiment manager with Rust backend.

Usage:
    from expman import Experiment

    exp = Experiment("my_experiment")
    exp.log_params({"lr": 0.001, "epochs": 100})

    for i in range(100):
        exp.log_metrics({"loss": 1.0 - i * 0.01, "acc": i * 0.01}, step=i)

    exp.close()  # or use as context manager

Context manager:
    with Experiment("my_experiment") as exp:
        exp.log_params({"lr": 0.001})
        for i in range(100):
            exp.log_metrics({"loss": 1.0 - i * 0.01}, step=i)
"""

from __future__ import annotations

import atexit
import os
import sys
from typing import Any

_current_exp: Experiment | None = None


class Tee:
    def __init__(self, primary_file, secondary_file):
        self.primary_file = primary_file
        self.secondary_file = secondary_file

    def write(self, data):
        self.primary_file.write(data)
        self.secondary_file.write(data)

    def flush(self):
        self.primary_file.flush()
        self.secondary_file.flush()

    def __getattr__(self, attr):
        return getattr(self.primary_file, attr)


# Import the compiled Rust extension
try:
    from .expman import Experiment as _RustExperiment
    from .expman import __version__
except ImportError as e:
    raise ImportError(
        "expman Rust extension not found. "
        "Build it with: maturin develop\n"
        f"Original error: {e}"
    ) from e


class Experiment:
    """
    Manages a single experiment run.

    All logging methods are non-blocking: metrics are sent to a background
    Rust/tokio task via a channel (~100ns per call), never blocking your
    training loop.

    Args:
        name: Experiment name (e.g. "resnet_cifar10")
        run_name: Optional run name. Auto-generated from timestamp if None.
        base_dir: Root directory for experiments. Default: "experiments"
        flush_interval_rows: Flush metrics every N rows. Default: 50
        flush_interval_ms: Flush metrics every N milliseconds. Default: 500
        redirect_console: Whether to redirect stdout/stderr to a console.log file. Default: True
    """

    def __init__(
        self,
        name: str,
        run_name: str | None = None,
        base_dir: str = "experiments",
        flush_interval_rows: int = 50,
        flush_interval_ms: int = 500,
        redirect_console: bool = True,
    ):
        self._exp = _RustExperiment(
            name=name,
            run_name=run_name,
            base_dir=base_dir,
            flush_interval_rows=flush_interval_rows,
            flush_interval_ms=flush_interval_ms,
        )
        self._closed = False
        self._old_stdout = None
        self._old_stderr = None
        self._console_file = None

        if redirect_console:
            try:
                log_path = os.path.join(self._exp.run_dir, "console.log")
                self._console_file = open(log_path, "a", buffering=1)
                self._old_stdout = sys.stdout
                self._old_stderr = sys.stderr
                sys.stdout = Tee(self._old_stdout, self._console_file)
                sys.stderr = Tee(self._old_stderr, self._console_file)
            except Exception as e:
                print(f"Warning: Failed to redirect console output: {e}")

        atexit.register(self.close)

    def log_params(self, params: dict[str, Any]) -> None:
        """Log hyperparameters/configuration. Non-blocking."""
        self._exp.log_params(params)

    def log_metrics(
        self,
        metrics: dict[str, float],
        step: int | None = None,
    ) -> None:
        """
        Log a dictionary of metrics. Non-blocking (~100ns).

        Args:
            metrics: Dict of metric name → numeric value
            step: Optional step/epoch number
        """
        self._exp.log_metrics(metrics, step)

    def save_artifact(self, path: str) -> None:
        """
        Save an artifact file asynchronously. Non-blocking.

        Args:
            path: Path to the file to save. This path will be preserved relative to
                  the run's artifacts directory.
        """
        self._exp.save_artifact(path)

    def info(self, message: str) -> None:
        """Log an info message to the run log. Non-blocking."""
        self._exp.info(message)

    def warn(self, message: str) -> None:
        """Log a warning message to the run log. Non-blocking."""
        self._exp.warn(message)

    @property
    def run_dir(self) -> str:
        """Path to the run directory."""
        return self._exp.run_dir

    @property
    def run_name(self) -> str:
        """Name of this run."""
        return self._exp.run_name

    def close(self) -> None:
        """
        Gracefully close the experiment.
        Flushes all pending metrics and writes final metadata.
        Called automatically via atexit and context manager __exit__.
        """
        if not self._closed:
            # Restore stdout/stderr
            if self._old_stdout:
                sys.stdout = self._old_stdout
            if self._old_stderr:
                sys.stderr = self._old_stderr
            if self._console_file:
                self._console_file.close()

            self._closed = True
            self._exp.close()

    def __enter__(self) -> Experiment:
        return self

    def __exit__(self, exc_type, exc_val, exc_tb) -> bool:
        self.close()  # Restore redirection first
        self._exp.__exit__(exc_type, exc_val, exc_tb)
        return False

    def __repr__(self) -> str:
        return repr(self._exp)


def init(
    name: str,
    run_name: str | None = None,
    base_dir: str = "experiments",
    flush_interval_rows: int = 50,
    flush_interval_ms: int = 500,
    redirect_console: bool = True,
) -> Experiment:
    """Initialize a global experiment. Returns the Experiment instance."""
    global _current_exp
    if _current_exp:
        _current_exp.close()

    _current_exp = Experiment(
        name=name,
        run_name=run_name,
        base_dir=base_dir,
        flush_interval_rows=flush_interval_rows,
        flush_interval_ms=flush_interval_ms,
        redirect_console=redirect_console,
    )
    return _current_exp


def log_params(params: dict[str, Any]) -> None:
    """Log parameters to the current global experiment."""
    if _current_exp:
        _current_exp.log_params(params)
    else:
        print("Warning: No active experiment. Call expman.init() first.")


def log_metrics(metrics: dict[str, float], step: int | None = None) -> None:
    """Log metrics to the current global experiment."""
    if _current_exp:
        _current_exp.log_metrics(metrics, step=step)
    else:
        print("Warning: No active experiment. Call expman.init() first.")


def save_artifact(path: str) -> None:
    """Save an artifact to the current global experiment."""
    if _current_exp:
        _current_exp.save_artifact(path)
    else:
        print("Warning: No active experiment. Call expman.init() first.")


def info(message: str) -> None:
    """Log an info message to the current global experiment."""
    if _current_exp:
        _current_exp.info(message)


def warn(message: str) -> None:
    """Log a warning message to the current global experiment."""
    if _current_exp:
        _current_exp.warn(message)


def close() -> None:
    """Flush and close the current global experiment."""
    global _current_exp
    if _current_exp:
        _current_exp.close()
        _current_exp = None


__all__ = [
    "Experiment",
    "init",
    "log_metrics",
    "log_params",
    "save_artifact",
    "info",
    "warn",
    "close",
    "__version__",
]
