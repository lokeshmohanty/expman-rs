"""
Drop-in replacement for ``torch.utils.tensorboard.SummaryWriter``.

Instead of writing TensorBoard event files, all metrics are routed through
expman's high-performance Rust backend and stored in Parquet format.

Quick start::

    # Replace this:
    # from torch.utils.tensorboard import SummaryWriter

    # With this:
    from expman import SummaryWriter

    writer = SummaryWriter(log_dir="runs/my_experiment")

    for epoch in range(100):
        loss = 1.0 / (epoch + 1)
        writer.add_scalar("train/loss", loss, epoch)

    writer.close()

The ``SummaryWriter`` maps TensorBoard's directory-based ``log_dir`` convention
to expman's ``(base_dir, experiment_name)`` structure:

- ``SummaryWriter("runs/exp1")`` → expman ``Experiment("exp1", base_dir="runs")``
- ``SummaryWriter("my_exp")`` → expman ``Experiment("my_exp", base_dir="experiments")``

Fully supported methods: ``add_scalar``, ``add_scalars``, ``add_text``,
``add_hparams``. Other methods (images, histograms, etc.) are stubbed as
no-ops so existing code doesn't break.
"""

import os

import expman


class SummaryWriter:
    """
    Drop-in replacement for ``torch.utils.tensorboard.SummaryWriter``.

    Writes metrics to expman's Rust backend instead of TensorBoard event files.
    Supports context manager protocol and automatic cleanup.

    Args:
        log_dir: Directory for storing logs. Mapped to expman's
            ``(base_dir, experiment_name)`` pair. If ``None``, generates a
            default path similar to TensorBoard's behavior.
        comment: Appended to the auto-generated ``log_dir`` when
            ``log_dir`` is ``None``.
        purge_step: Ignored (TensorBoard compatibility).
        max_queue: Ignored (TensorBoard compatibility).
        flush_secs: Ignored (TensorBoard compatibility).
        filename_suffix: Ignored (TensorBoard compatibility).

    Example::

        with SummaryWriter("runs/mnist") as writer:
            for step in range(100):
                writer.add_scalar("loss", 1.0 / (step + 1), step)
    """

    def __init__(
        self,
        log_dir: str | None = None,
        comment: str = "",
        purge_step: int | None = None,
        max_queue: int = 10,
        flush_secs: int = 120,
        filename_suffix: str = "",
        **kwargs,
    ):
        if log_dir is None:
            import socket
            from datetime import datetime

            current_time = datetime.now().strftime("%b%d_%H-%M-%S")
            log_dir = os.path.join("runs", current_time + "_" + socket.gethostname() + comment)

        base_dir = os.path.dirname(log_dir)
        if not base_dir:
            base_dir = "experiments"
            name = log_dir
        else:
            name = os.path.basename(log_dir)

        self.log_dir = log_dir
        self._exp = expman.Experiment(
            name=name,
            base_dir=base_dir,
            flush_interval_rows=50,
            flush_interval_ms=500,
            redirect_console=True,
        )
        # Ensure files are created immediately
        self._exp.log_params({})
        self._exp.info("SummaryWriter initialized")

    def add_scalar(
        self,
        tag: str,
        scalar_value: float | int,
        global_step: int | None = None,
        walltime: float | None = None,
        new_style: bool = False,
        double_precision: bool = False,
    ):
        """
        Add a scalar value to the summary.

        Args:
            tag: Data identifier (e.g. ``"train/loss"``).
            scalar_value: The scalar value to log.
            global_step: Global step value to record.
            walltime: Ignored (expman uses its own timestamps).
            new_style: Ignored (TensorBoard compatibility).
            double_precision: Ignored (TensorBoard compatibility).

        Example::

            writer.add_scalar("train/loss", 0.42, global_step=10)
        """
        self._exp.log_vector({tag: float(scalar_value)}, step=global_step)

    def add_scalars(
        self,
        main_tag: str,
        tag_scalar_dict: dict[str, float],
        global_step: int | None = None,
        walltime: float | None = None,
    ):
        """
        Add multiple scalars under a common group tag.

        Each key in ``tag_scalar_dict`` is prefixed with ``main_tag/``.

        Args:
            main_tag: Parent tag for grouping (e.g. ``"metrics"``).
            tag_scalar_dict: Mapping of sub-tag → value.
            global_step: Global step value to record.
            walltime: Ignored.

        Example::

            writer.add_scalars("metrics", {"accuracy": 0.95, "f1": 0.92}, step=10)
        """
        prefixed_dict = {f"{main_tag}/{k}": float(v) for k, v in tag_scalar_dict.items()}
        self._exp.log_vector(prefixed_dict, step=global_step)

    def add_text(
        self,
        tag: str,
        text_string: str,
        global_step: int | None = None,
        walltime: float | None = None,
    ):
        """
        Add text data to the summary.

        Logged as an info message in expman's run log.

        Args:
            tag: Data identifier.
            text_string: String value to log.
            global_step: Global step value.
            walltime: Ignored.

        Example::

            writer.add_text("experiment_notes", "Switched to Adam optimizer", 0)
        """
        self._exp.info(f"{tag}[{global_step}]: {text_string}")

    def add_hparams(self, hparam_dict, metric_dict, *args, **kwargs):
        """
        Add a set of hyperparameters and associated metrics.

        Hyperparameters are logged via ``log_params`` and metrics via
        ``log_vector``.

        Args:
            hparam_dict: Dictionary of hyperparameter names → values.
            metric_dict: Dictionary of metric names → values.

        Example::

            writer.add_hparams(
                {"lr": 0.001, "batch_size": 32},
                {"hparam/accuracy": 0.95}
            )
        """
        self._exp.log_params(hparam_dict)
        if metric_dict:
            self._exp.log_vector(metric_dict)

    def flush(self):
        """Flush pending events to disk. No-op — expman auto-flushes asynchronously."""
        pass

    def close(self):
        """Close the writer and flush all pending data."""
        self._exp.close()

    def __enter__(self):
        """Enter context manager."""
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Exit context manager, closing the writer."""
        self.close()

    # ── Stub methods ────────────────────────────────────────────────────
    # These are no-ops to prevent TypeErrors when replacing TensorBoard's
    # SummaryWriter. They accept arbitrary arguments silently.

    def add_histogram(self, *args, **kwargs):
        """Stub: histograms are not supported. No-op."""
        pass

    def add_image(self, *args, **kwargs):
        """Stub: images are not supported. No-op."""
        pass

    def add_images(self, *args, **kwargs):
        """Stub: image batches are not supported. No-op."""
        pass

    def add_figure(self, *args, **kwargs):
        """Stub: matplotlib figures are not supported. No-op."""
        pass

    def add_video(self, *args, **kwargs):
        """Stub: video data is not supported. No-op."""
        pass

    def add_audio(self, *args, **kwargs):
        """Stub: audio data is not supported. No-op."""
        pass

    def add_graph(self, *args, **kwargs):
        """Stub: model graphs are not supported. No-op."""
        pass

    def add_embedding(self, *args, **kwargs):
        """Stub: embeddings are not supported. No-op."""
        pass

    def add_pr_curve(self, *args, **kwargs):
        """Stub: PR curves are not supported. No-op."""
        pass

    def add_custom_scalars(self, *args, **kwargs):
        """Stub: custom scalar layouts are not supported. No-op."""
        pass

    def add_mesh(self, *args, **kwargs):
        """Stub: 3D mesh data is not supported. No-op."""
        pass
