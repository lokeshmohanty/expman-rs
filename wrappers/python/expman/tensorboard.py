import os

import expman


class SummaryWriter:
    """
    Drop-in replacement for tensorboard.SummaryWriter using expman.

    This writes metrics directly to expman's Rust backend instead of
    writing standard tfevent files.
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
        # We roughly map log_dir to expman base_dir if needed
        # Since SummaryWriter is often used as SummaryWriter(log_dir="runs/exp_name"),
        # we try to infer name and base_dir from log_dir

        if log_dir is None:
            # Match tensorboard's default "runs/CURRENT_DATETIME_HOSTNAME"
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

        # By default expman auto-generates run names under the experiment `name`.
        # However tensorboard users expect the exact `log_dir` to be used.
        # So we'll treat base_dir as the base directory, name as experiment name,
        # and we specify a dummy run_name if we want, or just let it be.

        self.log_dir = log_dir
        self._exp = expman.Experiment(
            name=name,
            # we'll map `log_dir` semantics roughly.
            # In TB, log_dir = "runs/exp1" => expman(name="exp1", base_dir="runs")
            base_dir=base_dir,
            # Make sure it behaves fast like TB
            flush_interval_rows=50,
            flush_interval_ms=500,
            redirect_console=False,  # standard TB doesn't do this
        )

    def add_scalar(
        self,
        tag: str,
        scalar_value: float | int,
        global_step: int | None = None,
        walltime: float | None = None,
        new_style: bool = False,
        double_precision: bool = False,
    ):
        """Add scalar data to summary."""
        self._exp.log_vector({tag: float(scalar_value)}, step=global_step)

    def add_scalars(
        self,
        main_tag: str,
        tag_scalar_dict: dict[str, float],
        global_step: int | None = None,
        walltime: float | None = None,
    ):
        """Adds many scalar data to summary."""
        # Prepend the main_tag
        prefixed_dict = {f"{main_tag}/{k}": float(v) for k, v in tag_scalar_dict.items()}
        self._exp.log_vector(prefixed_dict, step=global_step)

    def add_text(
        self,
        tag: str,
        text_string: str,
        global_step: int | None = None,
        walltime: float | None = None,
    ):
        """Add text data to summary."""
        self._exp.info(f"{tag}[{global_step}]: {text_string}")

    def flush(self):
        """Flushes the event file to disk."""
        # expman handles flushing async, but we can't synchronously flush yet without
        # extending rust API. The rust side auto-flushes based on intervals.
        pass

    def close(self):
        """Close the writer."""
        self._exp.close()

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()

    # Stub out other common methods to prevent TypeErrors for drop-in replacements
    def add_histogram(self, *args, **kwargs):
        pass

    def add_image(self, *args, **kwargs):
        pass

    def add_images(self, *args, **kwargs):
        pass

    def add_figure(self, *args, **kwargs):
        pass

    def add_video(self, *args, **kwargs):
        pass

    def add_audio(self, *args, **kwargs):
        pass

    def add_graph(self, *args, **kwargs):
        pass

    def add_embedding(self, *args, **kwargs):
        pass

    def add_pr_curve(self, *args, **kwargs):
        pass

    def add_custom_scalars(self, *args, **kwargs):
        pass

    def add_mesh(self, *args, **kwargs):
        pass

    def add_hparams(self, hparam_dict, metric_dict, *args, **kwargs):
        self._exp.log_params(hparam_dict)
        # We can also log the metrics initially if desired
        if metric_dict:
            self._exp.log_vector(metric_dict)
