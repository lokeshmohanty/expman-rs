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
``add_hparams``, ``add_image``, ``add_images``, ``add_figure`` and
``add_histogram`` — all stored natively.

The handful of methods expman genuinely cannot store (``add_graph``,
``add_embedding``, ``add_pr_curve``, ``add_mesh``) emit a warning once each
rather than dropping data silently. For those, point a real TensorBoard writer
at ``exp.tensorboard_dir``; the dashboard renders it in the TensorBoard tab.
"""

import os
import warnings

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
        # One warning per unsupported method, not per call: a training loop
        # calling add_graph every epoch should say so once.
        self._warned: set[str] = set()
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

    # ── TensorBoard compatibility ────────────────────────────────────────
    #
    # These used to be silent no-ops. Swapping
    # `torch.utils.tensorboard.SummaryWriter` for this class therefore threw
    # away every image, histogram and figure a user logged, with nothing said at
    # runtime — the worst kind of failure, because the code looks like it works.
    #
    # What expman can store natively is now stored. What it genuinely cannot is
    # warned about **once per method**, so the loss is visible without turning a
    # training loop into a wall of warnings.

    def _dropped(self, method: str, reason: str) -> None:
        """Warn once that a call could not be stored, without raising.

        The native API (`exp.log_image`) raises on bad input, because that is new
        code and a hard error is the fastest way to fix it. This compatibility
        layer must not: it wraps code written for TensorBoard, often inside a
        multi-day training run, and killing that run over an unencodable image
        would be a worse failure than the one being reported.
        """
        if method in self._warned:
            return
        self._warned.add(method)
        warnings.warn(
            f"expman's SummaryWriter dropped a {method}() call: {reason}",
            stacklevel=3,
        )

    def _unsupported(self, method: str, alternative: str = "") -> None:
        if method in self._warned:
            return
        self._warned.add(method)
        suffix = f" {alternative}" if alternative else ""
        warnings.warn(
            f"expman's SummaryWriter does not support {method}(); "
            f"those calls are dropped.{suffix}",
            stacklevel=3,
        )

    def add_histogram(self, tag=None, values=None, global_step=None, *args, **kwargs):
        """Record a distribution. Stored natively as binned counts."""
        if tag is None or values is None:
            return
        try:
            self._exp.log_histogram(tag, values, step=global_step)
        except (TypeError, ValueError) as e:
            self._dropped("add_histogram", str(e))

    def add_image(self, tag=None, img_tensor=None, global_step=None, *args, **kwargs):
        """Record an image. Stored natively under the run's media/ directory."""
        if tag is None or img_tensor is None:
            return
        try:
            self._exp.log_image(tag, img_tensor, step=global_step)
        except (TypeError, ValueError, OSError) as e:
            self._dropped("add_image", str(e))

    def add_images(self, tag=None, img_tensor=None, global_step=None, *args, **kwargs):
        """Record a batch of images, one media entry per image."""
        if tag is None or img_tensor is None:
            return
        try:
            batch = list(img_tensor) if not isinstance(img_tensor, (str, bytes)) else [img_tensor]
        except TypeError:
            batch = [img_tensor]
        for index, image in enumerate(batch):
            try:
                self._exp.log_image(f"{tag}/{index}", image, step=global_step)
            except (TypeError, ValueError, OSError) as e:
                self._dropped("add_images", str(e))
                return

    def add_figure(self, tag=None, figure=None, global_step=None, *args, **kwargs):
        """Record a matplotlib figure as a PNG."""
        if tag is None or figure is None:
            return
        figures = figure if isinstance(figure, (list, tuple)) else [figure]
        for index, item in enumerate(figures):
            name = tag if len(figures) == 1 else f"{tag}/{index}"
            try:
                self._exp.log_figure(name, item, step=global_step)
            except (TypeError, ValueError, OSError) as e:
                self._dropped("add_figure", str(e))
                return

    def add_audio(self, tag=None, snd_tensor=None, global_step=None, *args, **kwargs):
        """Record audio. Encoded bytes are stored; tensors are not encodable here."""
        if tag is None or snd_tensor is None:
            return
        if isinstance(snd_tensor, (bytes, bytearray, memoryview)):
            self._exp.log_audio(tag, bytes(snd_tensor), step=global_step)
        else:
            self._unsupported(
                "add_audio",
                "Pass encoded WAV bytes to log_audio() to store audio.",
            )

    def add_video(self, tag=None, vid_tensor=None, global_step=None, *args, **kwargs):
        """Record video. Encoded bytes are stored; tensors are not encodable here."""
        if tag is None or vid_tensor is None:
            return
        if isinstance(vid_tensor, (bytes, bytearray, memoryview)):
            self._exp.log_video(tag, bytes(vid_tensor), step=global_step)
        else:
            self._unsupported(
                "add_video",
                "Pass encoded MP4 bytes to log_video() to store video.",
            )

    def add_graph(self, *args, **kwargs):
        self._unsupported(
            "add_graph",
            "Use a real TensorBoard writer against exp.tensorboard_dir; the "
            "dashboard renders it in the TensorBoard tab.",
        )

    def add_embedding(self, *args, **kwargs):
        self._unsupported(
            "add_embedding",
            "Use a real TensorBoard writer against exp.tensorboard_dir for the "
            "embedding projector.",
        )

    def add_pr_curve(self, *args, **kwargs):
        self._unsupported("add_pr_curve")

    def add_custom_scalars(self, *args, **kwargs):
        # Purely a dashboard layout hint; dropping it loses no data at all, so
        # this one stays quiet by design.
        pass

    def add_mesh(self, *args, **kwargs):
        self._unsupported("add_mesh")
