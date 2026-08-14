"""
expman: High-performance experiment manager with Rust backend.

Usage:
    from expman import Experiment

    exp = Experiment("my_experiment")
    exp.log_params({"lr": 0.001, "epochs": 100})

    for i in range(100):
        exp.log_vector({"loss": 1.0 - i * 0.01, "acc": i * 0.01}, step=i)

    exp.close()  # or use as context manager

Context manager:
    with Experiment("my_experiment") as exp:
        exp.log_params({"lr": 0.001})
        for i in range(100):
            exp.log_vector({"loss": 1.0 - i * 0.01}, step=i)

Terminal status:
    A run that died is not a run that finished. `close()` still defaults to
    FINISHED, but it takes an explicit terminal status, and a process exiting
    because of an uncaught exception records FAILED without being asked:

        expman.close(status="failed", reason="loss diverged")
        expman.fail(reason="loss diverged")     # the same thing, spelled shorter
"""

from __future__ import annotations

import atexit
import os
import sys
from typing import Any

from .tensorboard import SummaryWriter

_current_exp: Experiment | None = None

# Statuses a run may be *closed* with. RUNNING is deliberately absent: closing
# is the act that makes a status terminal, so closing to RUNNING is a bug.
_TERMINAL_STATUSES = ("FINISHED", "FAILED", "CRASHED")


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
    from . import expman as _expman_rs
    from .expman import Experiment as _RustExperiment
    from .expman import __version__
except ImportError as e:
    raise ImportError(
        "expman Rust extension not found. "
        "Build it with: just dev-py (or uv pip install -e .)\n"
        f"Original error: {e}"
    ) from e


def _normalise_status(status: str | None) -> str:
    """Canonicalise a caller-supplied terminal status, or raise.

    Rejecting a typo matters more here than anywhere else in this package: the
    write path swallows errors by design, so a status that quietly fell back to
    FINISHED would turn a dead run into a good-looking one — the exact failure
    this API exists to prevent.
    """
    if status is None:
        return "FINISHED"
    normalised = str(status).strip().upper()
    if normalised not in _TERMINAL_STATUSES:
        raise ValueError(
            f"invalid terminal run status {status!r}; "
            f"expected one of {', '.join(_TERMINAL_STATUSES)} (case-insensitive)"
        )
    return normalised


def _interpreter_exception() -> BaseException | None:
    """The uncaught exception the interpreter is dying from, or None.

    CPython's `_PyErr_PrintEx` sets `sys.last_exc` (3.12+) and the legacy
    `sys.last_value` (all versions) *before* it dispatches to `sys.excepthook`,
    and `atexit` callbacks run after that during finalisation. So reading it
    here is strictly more robust than chaining `sys.excepthook`:

    * it does not depend on which excepthook is installed, nor on expman having
      installed its own before some framework (IPython, absl, sentry) replaced
      it — a race we would lose silently and never notice;
    * it needs no second shutdown path. The existing atexit close stays the one
      place a run is finalised; it merely learns to read the status.

    The cost is that it is process-global state which can be *stale* — a REPL
    keeps the last traceback around forever. `Experiment` handles that by
    snapshotting the value at construction and only trusting a change.
    """
    exc = getattr(sys, "last_exc", None)  # Python 3.12+
    if exc is None:
        exc = getattr(sys, "last_value", None)  # Python <= 3.11
    return exc if isinstance(exc, BaseException) else None


def _describe_exception(exc: BaseException, limit: int = 400) -> str:
    """One line: the exception type, its message, and where it was raised."""
    text = f"{type(exc).__name__}: {exc}".strip().rstrip(":")
    tb = exc.__traceback__
    if tb is not None:
        while tb.tb_next is not None:
            tb = tb.tb_next
        where = os.path.basename(tb.tb_frame.f_code.co_filename)
        text += f" (at {where}:{tb.tb_lineno} in {tb.tb_frame.f_code.co_name})"
    text = " ".join(text.split())
    return text if len(text) <= limit else text[: limit - 1] + "…"


def _detect_rank() -> int | None:
    """Rank from whatever launcher started us, or None if not distributed.

    torchrun sets RANK; SLURM sets SLURM_PROCID. Checking these means a DDP
    script needs no expman-specific changes to log correctly.
    """
    # EXPMAN_RANK first: a sweep sets it deliberately, and it should win over
    # a launcher variable that happens to be present for another reason.
    for var in ("EXPMAN_RANK", "RANK", "SLURM_PROCID", "OMPI_COMM_WORLD_RANK", "LOCAL_RANK"):
        value = os.environ.get(var)
        if value is not None:
            try:
                return int(value)
            except ValueError:
                continue
    return None


def _detect_group() -> str | None:
    """A stable identifier shared by every rank of one job.

    The job id is the right key: it is identical across ranks and unique across
    jobs, which is exactly the grouping we want.
    """
    # A sweep names its own group; only fall back to job ids outside one.
    explicit = os.environ.get("EXPMAN_GROUP")
    if explicit:
        return explicit
    for var in ("SLURM_ARRAY_JOB_ID", "SLURM_JOB_ID", "TORCHELASTIC_RUN_ID", "PBS_JOBID"):
        value = os.environ.get(var)
        if value:
            return f"job-{value}"
    return None


def _encode_image(image: Any) -> tuple[bytes, str]:
    """Coerce the many things people call an image into encoded bytes.

    Raises on anything unrecognised. Guessing, or quietly logging nothing, is
    how image logging silently disappears from a run.
    """
    if isinstance(image, (bytes, bytearray, memoryview)):
        return bytes(image), "png"

    if isinstance(image, (str, os.PathLike)):
        path = os.fspath(image)
        with open(path, "rb") as f:
            return f.read(), (os.path.splitext(path)[1].lstrip(".") or "png")

    # matplotlib Figure — duck-typed so matplotlib is never imported here.
    if hasattr(image, "savefig"):
        import io

        buf = io.BytesIO()
        image.savefig(buf, format="png", bbox_inches="tight")
        return buf.getvalue(), "png"

    # PIL Image
    if hasattr(image, "save") and hasattr(image, "mode"):
        import io

        buf = io.BytesIO()
        image.save(buf, format="PNG")
        return buf.getvalue(), "png"

    # numpy array — encode via PIL, which is the only dependency that can.
    if hasattr(image, "shape") and hasattr(image, "astype"):
        try:
            import numpy as np
            from PIL import Image
        except ImportError as e:
            raise TypeError(
                "logging a numpy array as an image needs Pillow "
                "(pip install Pillow), or pass encoded bytes instead"
            ) from e
        array = np.asarray(image)
        # CHW -> HWC, the layout torch hands you.
        if array.ndim == 3 and array.shape[0] in (1, 3, 4) and array.shape[2] not in (1, 3, 4):
            array = array.transpose(1, 2, 0)
        if array.ndim == 3 and array.shape[2] == 1:
            array = array[:, :, 0]
        # Float images are conventionally 0..1; ints are already 0..255.
        if array.dtype.kind == "f":
            array = (np.clip(array, 0.0, 1.0) * 255).round()
        array = array.astype("uint8")
        import io

        buf = io.BytesIO()
        Image.fromarray(array).save(buf, format="PNG")
        return buf.getvalue(), "png"

    raise TypeError(
        f"cannot log {type(image).__name__} as an image; pass bytes, a path, "
        "a PIL Image, a matplotlib Figure, or a numpy array"
    )


def _histogram(values: Any, bins: int) -> tuple[list[float], list[int]]:
    """Bin values, using numpy when present and pure Python otherwise.

    The pure-Python path exists so histogram logging is not silently unavailable
    in a minimal environment — which is exactly the failure this release fixes.
    """
    try:
        import numpy as np

        counts, edges = np.histogram(np.asarray(values, dtype="float64").ravel(), bins=bins)
        return [float(e) for e in edges], [int(c) for c in counts]
    except ImportError:
        pass

    data = [float(v) for v in values]
    if not data:
        return [0.0, 0.0], [0]
    low, high = min(data), max(data)
    if low == high:
        # A degenerate range still deserves a valid histogram rather than a
        # division by zero.
        low, high = low - 0.5, high + 0.5
    width = (high - low) / bins
    counts = [0] * bins
    for value in data:
        idx = int((value - low) / width)
        counts[min(idx, bins - 1)] += 1
    return [low + i * width for i in range(bins + 1)], counts


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
        project: Project this experiment belongs to. Written to experiment.yaml
                 offline, so it works on a compute node with no dashboard.
        tags: Tags for this run, written to run.yaml at creation.
        description: Description for this run, written to run.yaml at creation.
        heartbeat_interval_secs: Seconds between liveness heartbeats, which let
                 `exp reap` tell a killed run from a long-running one. 0 disables.
        group: Group this run belongs to. All ranks of one DDP job share a group
                 and roll up to a single row in the dashboard. Auto-detected from
                 the launcher when omitted.
        rank: Rank within the group. Auto-detected from RANK / LOCAL_RANK /
                 SLURM_PROCID when omitted; None means "not a distributed run".
        system_metrics_interval_secs: Seconds between hardware samples
                 (GPU/CPU/RAM via nvidia-smi, rocm-smi, tpu-info). 0 disables.
        capture_provenance: Record git commit/branch/dirty and scheduler ids.
        capture_diff: Also record the working-tree diff. Off by default because a
                 dirty tree can carry secrets into a store you may share.
    """

    def __init__(
        self,
        name: str,
        run_name: str | None = None,
        base_dir: str = "experiments",
        flush_interval_rows: int = 50,
        flush_interval_ms: int = 500,
        redirect_console: bool = True,
        project: str | None = None,
        tags: list[str] | None = None,
        description: str | None = None,
        heartbeat_interval_secs: int = 30,
        group: str | None = None,
        rank: int | None = None,
        system_metrics_interval_secs: int = 15,
        capture_provenance: bool = True,
        capture_diff: bool = False,
    ):
        # A sweep launches trials with these set, so a plain training script
        # lands in the right experiment, project and group with no edits.
        base_dir = os.environ.get("EXPMAN_BASE_DIR", base_dir)
        if project is None:
            project = os.environ.get("EXPMAN_PROJECT")
        # A sweep tags each trial with its parameters. Merge rather than replace
        # so a script's own tags survive.
        swept_tags = [t for t in os.environ.get("EXPMAN_TAGS", "").split(",") if t]
        if swept_tags:
            tags = list(dict.fromkeys(swept_tags + list(tags or [])))

        # Under torchrun/SLURM every rank constructs an Experiment. Without a
        # rank they would all race for the same run directory; with one they
        # form a group. Detecting it beats making every user remember to.
        if rank is None:
            rank = _detect_rank()
        if group is None:
            group = _detect_group()
        if run_name is None:
            run_name = os.environ.get("EXPMAN_RUN_NAME")
        # Ranks must not collide on a run name, or they overwrite each other.
        if run_name is None and rank is not None and group is not None:
            run_name = f"{group}-rank{rank}"

        self._exp = _RustExperiment(
            name=name,
            run_name=run_name,
            base_dir=base_dir,
            flush_interval_rows=flush_interval_rows,
            flush_interval_ms=flush_interval_ms,
            project=project,
            tags=tags,
            description=description,
            heartbeat_interval_secs=heartbeat_interval_secs,
            group=group,
            rank=rank,
            system_metrics_interval_secs=system_metrics_interval_secs,
            capture_provenance=capture_provenance,
            capture_diff=capture_diff,
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

        # Whatever traceback the interpreter was already sitting on before this
        # run existed — a stale one from an earlier REPL cell, say. Kept so the
        # exit hook can tell "the same old exception" from "a new one", by
        # identity rather than by value.
        #
        # A strong reference, because built-in exceptions are not weak-referenceable
        # (`weakref.ref(ValueError())` raises TypeError), and an id() alone could be
        # recycled by a later exception — which would read as "same as prior" and
        # silently restore the FINISHED bug. It is almost always None: it is only
        # non-None when the process already printed an uncaught traceback and kept
        # going, in which case `sys.last_exc` is pinning that object anyway.
        self._prior_exc = _interpreter_exception()

        # The one shutdown path, unchanged in shape: still a single atexit
        # callback, now one that decides the status instead of assuming it.
        atexit.register(self._close_at_exit)

    def log_params(self, params: dict[str, Any]) -> None:
        """Log hyperparameters/configuration. Non-blocking."""
        self._exp.log_params(params)

    def log_vector(
        self,
        values: dict[str, float],
        step: int | None = None,
    ) -> None:
        """
        Log a dictionary of vector metrics. Non-blocking (~100ns).

        Args:
            values: Dict of metric name → numeric value
            step: Optional step/epoch number
        """
        self._exp.log_vector(values, step)

    def log_scalar(self, key: str, value: Any) -> None:
        """
        Log a single scalar value. Non-blocking.
        Replaces existing value if already logged.

        Args:
            key: Metric name
            value: Scalar value (float, int, bool, str)
        """
        self._exp.log_scalar(key, value)

    def save_artifact(self, path: str) -> None:
        """
        Save an artifact file asynchronously. Non-blocking.

        Args:
            path: Path to the file to save. This path will be preserved relative to
                  the run's artifacts directory.
        """
        self._exp.save_artifact(path)

    def log_image(
        self,
        tag: str,
        image: Any,
        step: int | None = None,
    ) -> None:
        """
        Log an image. Non-blocking.

        Accepts whatever you already have: raw PNG/JPEG bytes, a path to an
        image file, a PIL Image, a matplotlib Figure, or an HWC/CHW numpy array.
        Anything else raises rather than silently logging nothing.
        """
        data, ext = _encode_image(image)
        self._exp.log_media(tag, data, ext, step)

    def log_figure(self, tag: str, figure: Any, step: int | None = None) -> None:
        """Log a matplotlib figure as a PNG. Non-blocking."""
        self.log_image(tag, figure, step=step)

    def log_audio(
        self,
        tag: str,
        data: bytes,
        step: int | None = None,
        extension: str = "wav",
    ) -> None:
        """Log encoded audio bytes. Non-blocking."""
        self._exp.log_media(tag, data, extension, step)

    def log_video(
        self,
        tag: str,
        data: bytes,
        step: int | None = None,
        extension: str = "mp4",
    ) -> None:
        """Log encoded video bytes. Non-blocking."""
        self._exp.log_media(tag, data, extension, step)

    def log_histogram(
        self,
        tag: str,
        values: Any = None,
        step: int | None = None,
        bins: int = 64,
        edges: list[float] | None = None,
        counts: list[int] | None = None,
    ) -> None:
        """
        Log a distribution. Non-blocking.

        Pass raw `values` (any sequence or numpy array) and they are binned
        here, or pass precomputed `edges` and `counts` if you already have them.
        `edges` must be one longer than `counts`.
        """
        if edges is not None and counts is not None:
            self._exp.log_histogram_bins(tag, list(edges), list(counts), step)
            return
        if values is None:
            raise ValueError("log_histogram needs either values= or edges= and counts=")
        computed_edges, computed_counts = _histogram(values, bins)
        self._exp.log_histogram_bins(tag, computed_edges, computed_counts, step)

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

    @property
    def project(self) -> str | None:
        """Project this experiment is assigned to, or None."""
        return self._exp.project

    @property
    def group(self) -> str | None:
        """Group this run belongs to, if it is part of one."""
        return self._exp.group

    @property
    def rank(self) -> int | None:
        """This run's rank within its group."""
        return self._exp.rank

    @property
    def is_primary(self) -> bool:
        """True on rank 0, or on any non-distributed run.

        Use it to guard work that must happen exactly once per job — saving a
        checkpoint, writing a summary artifact — rather than once per rank.
        """
        return self._exp.rank in (None, 0)

    def set_project(self, project: str | None) -> None:
        """
        Assign this experiment to a project. Works offline — no server needed.

        Writes only the `project:` field of experiment.yaml, so it is safe to
        call from a SLURM batch job. Pass None to unassign.
        """
        self._exp.set_project(project)

    def set_tags(self, tags: list[str]) -> None:
        """Replace this run's tags."""
        self._exp.set_tags(tags)

    def add_tags(self, tags: list[str]) -> None:
        """Add tags to this run, keeping any already set."""
        self._exp.add_tags(tags)

    def set_description(self, description: str) -> None:
        """Set this run's description."""
        self._exp.set_description(description)

    @property
    def tensorboard_dir(self) -> str:
        """Path to TensorBoard log directory for this run.
        
        Creates the directory if it doesn't exist. Pass this to your
        profiler or TensorBoard SummaryWriter:
        
            profiler = torch.profiler.profile(
                on_trace_ready=torch.profiler.tensorboard_trace_handler(exp.tensorboard_dir)
            )
        """
        tb_dir = os.path.join(self._exp.run_dir, "tensorboard")
        os.makedirs(tb_dir, exist_ok=True)
        return tb_dir

    def close(self, status: str = "FINISHED", reason: str | None = None) -> None:
        """
        Gracefully close the experiment.
        Flushes all pending metrics and writes final metadata.
        Called automatically via atexit and context manager __exit__.

        Args:
            status: Terminal status of the run — "FINISHED", "FAILED" or
                    "CRASHED", case-insensitive. Default: "FINISHED", so every
                    existing `close()` call site keeps its meaning.
            reason: Optional one-line explanation, appended to the run's
                    `run.log` (which the dashboard console renders). It does
                    not go into `run.yaml`: that schema has no field for it,
                    and `description` belongs to the user.

        Raises:
            ValueError: if `status` is not a terminal status.
        """
        if self._closed:
            return

        normalised = _normalise_status(status)

        # Before the shutdown command, not after: the engine drains its command
        # queue in order and the shutdown handler flushes run.log, so a line
        # sent here is on disk by the time close() returns.
        if reason:
            self._exp.warn(f"run {normalised}: {reason}")

        # Restore stdout/stderr
        if self._old_stdout:
            sys.stdout = self._old_stdout
        if self._old_stderr:
            sys.stderr = self._old_stderr
        if self._console_file:
            self._console_file.close()

        self._closed = True
        self._exp.close(status=normalised)
        # Nothing left for the exit hook to do, and dropping it lets this
        # Experiment be collected instead of living in the atexit registry.
        atexit.unregister(self._close_at_exit)

    def fail(self, reason: str | None = None) -> None:
        """Close this run as FAILED, optionally recording why.

        Shorthand for `close(status="FAILED", reason=...)`, for the common
        `except` block that wants to end the run honestly and re-raise.
        """
        self.close(status="FAILED", reason=reason)

    def _close_at_exit(self) -> None:
        """Close on interpreter shutdown, inferring the status.

        Registered with `atexit` in place of `close` itself. A process that is
        exiting because of an uncaught exception must not leave a FINISHED run
        behind — that is indistinguishable from success to everything that
        later reads the store.

        FAILED rather than CRASHED: the run did reach its own shutdown and said
        so. CRASHED stays reserved for runs that never got to speak — what
        `exp reap` writes when a heartbeat goes stale after a SIGKILL or a dead
        node. Nothing here duplicates that; a hard kill never runs atexit.
        """
        if self._closed:
            return
        exc = _interpreter_exception()
        if exc is None or exc is self._prior_exc:
            self.close()
        else:
            self.close(status="FAILED", reason=_describe_exception(exc))

    def __enter__(self) -> Experiment:
        return self

    def __exit__(self, exc_type, exc_val, exc_tb) -> bool:
        if exc_val is not None:
            self.close(status="FAILED", reason=_describe_exception(exc_val))
        else:
            self.close()
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
    project: str | None = None,
    tags: list[str] | None = None,
    description: str | None = None,
    heartbeat_interval_secs: int = 30,
    group: str | None = None,
    rank: int | None = None,
    system_metrics_interval_secs: int = 15,
    capture_provenance: bool = True,
    capture_diff: bool = False,
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
        project=project,
        tags=tags,
        description=description,
        heartbeat_interval_secs=heartbeat_interval_secs,
        group=group,
        rank=rank,
        system_metrics_interval_secs=system_metrics_interval_secs,
        capture_provenance=capture_provenance,
        capture_diff=capture_diff,
    )
    return _current_exp


def log_params(params: dict[str, Any]) -> None:
    """Log parameters to the current global experiment."""
    if _current_exp:
        _current_exp.log_params(params)
    else:
        print("Warning: No active experiment. Call expman.init() first.")


def log_vector(values: dict[str, float], step: int | None = None) -> None:
    """Log vector metrics to the current global experiment."""
    if _current_exp:
        _current_exp.log_vector(values, step=step)
    else:
        print("Warning: No active experiment. Call expman.init() first.")


def log_scalar(key: str, value: Any) -> None:
    """Log a scalar metric to the current global experiment."""
    if _current_exp:
        _current_exp.log_scalar(key, value)
    else:
        print("Warning: No active experiment. Call expman.init() first.")


def save_artifact(path: str) -> None:
    """Save an artifact to the current global experiment."""
    if _current_exp:
        _current_exp.save_artifact(path)
    else:
        print("Warning: No active experiment. Call expman.init() first.")


def tensorboard_dir() -> str | None:
    """Get the TensorBoard log directory for the current global experiment."""
    if _current_exp:
        return _current_exp.tensorboard_dir
    else:
        print("Warning: No active experiment. Call expman.init() first.")
        return None


def info(message: str) -> None:
    """Log an info message to the current global experiment."""
    if _current_exp:
        _current_exp.info(message)


def warn(message: str) -> None:
    """Log a warning message to the current global experiment."""
    if _current_exp:
        _current_exp.warn(message)


def close(status: str = "FINISHED", reason: str | None = None) -> None:
    """
    Flush and close the current global experiment.

    Args:
        status: Terminal status — "FINISHED", "FAILED" or "CRASHED",
                case-insensitive. Defaults to "FINISHED", so a bare `close()`
                means exactly what it always did.
        reason: Optional one-line explanation, appended to the run's `run.log`.

    **A `try/finally: expman.close()` around a training loop records FINISHED
    even when the body raised.** The automatic path cannot correct it: this
    close already ran, marked the run terminal and unregistered the exit hook.
    Close in the `except` branch instead, or use the context manager:

        try:
            train()
        except BaseException as e:
            expman.fail(reason=str(e))
            raise
        else:
            expman.close()

    `tests/test_run_status.py::test_a_finally_close_still_wins_and_says_finished`
    pins this behaviour deliberately.
    """
    global _current_exp
    if _current_exp:
        _current_exp.close(status=status, reason=reason)
        _current_exp = None


def fail(reason: str | None = None) -> None:
    """Close the current global experiment as FAILED, optionally recording why."""
    close(status="FAILED", reason=reason)


# ─── Read API ────────────────────────────────────────────────────────────────
#
# Everything below reads an existing store. It is deliberately dependency-free:
# plain dicts and lists by default, so an analysis script works in a bare
# environment. Pass to_pandas=True where a DataFrame is more convenient — pandas
# is imported lazily and only then, so it stays an optional extra.


def _as_pandas(rows: list[dict[str, Any]]):
    try:
        import pandas as pd
    except ImportError as e:  # pragma: no cover - depends on the environment
        raise ImportError(
            "to_pandas=True requires pandas. Install it with: pip install pandas"
        ) from e
    return pd.DataFrame(rows)


def load_runs(
    base_dir: str = "experiments",
    project: str | None = None,
    experiment: str | None = None,
    group: str | None = None,
    status: str | None = None,
    tags: str | list[str] | None = None,
    to_pandas: bool = False,
):
    """
    Query runs across the whole store. Newest first.

    Args:
        base_dir: Root experiments directory.
        project: Only runs whose experiment belongs to this project.
        experiment: Only runs of this experiment.
        group: Only runs in this group (a DDP job or sweep cohort).
        status: One of RUNNING, FINISHED, FAILED, CRASHED.
        tags: Either a list of tags (all must match) or an expression string
              such as "arm:tiered AND (study:1 OR study:2)".
        to_pandas: Return a DataFrame instead of a list of dicts.

    Returns:
        A list of dicts (or a DataFrame). Each has run, experiment, project,
        status, started_at, finished_at, duration_secs, description, tags,
        scalars, vectors, and path. Feed that path to read_metrics().

    Example:
        >>> runs = load_runs(tags="arm:tiered AND study:1")
        >>> metrics = read_metrics(runs[0]["path"])
    """
    rows = _expman_rs.load_runs(
        base_dir=base_dir,
        project=project,
        experiment=experiment,
        group=group,
        status=status,
        tags=tags,
    )
    return _as_pandas(rows) if to_pandas else rows


def read_metrics(run_dir: str, to_pandas: bool = False):
    """
    Read a run's logged vector metrics as row dicts (or a DataFrame).

    Args:
        run_dir: A run directory — Experiment.run_dir, or the "path" of a
                 load_runs() result.
        to_pandas: Return a DataFrame instead of a list of dicts.

    Each row has step, timestamp, and one key per metric. Metrics that were not
    logged at that step are None.
    """
    rows = _expman_rs.read_metrics(run_dir)
    return _as_pandas(rows) if to_pandas else rows


def load_config(run_dir: str) -> dict[str, Any]:
    """Read a run's logged parameters (config.yaml) as a dict."""
    return _expman_rs.load_config(run_dir)


def load_run(run_dir: str) -> dict[str, Any]:
    """Read a single run's metadata (run.yaml) as a dict."""
    return _expman_rs.load_run(run_dir)


def log_image(tag: str, image: Any, step: int | None = None) -> None:
    """Log an image to the current global experiment."""
    if _current_exp:
        _current_exp.log_image(tag, image, step=step)
    else:
        print("Warning: No active experiment. Call expman.init() first.")


def log_histogram(tag: str, values: Any = None, step: int | None = None, **kwargs) -> None:
    """Log a histogram to the current global experiment."""
    if _current_exp:
        _current_exp.log_histogram(tag, values, step=step, **kwargs)
    else:
        print("Warning: No active experiment. Call expman.init() first.")


def read_histograms(run_dir: str) -> list[dict[str, Any]]:
    """
    Read a run's logged histograms.

    Rows carry `tag`, `step`, `edges`, `counts` and `total`; edges and counts
    are JSON strings, since bin counts vary between rows.
    """
    return _expman_rs.read_histograms(run_dir)


def read_media(run_dir: str) -> list[dict[str, Any]]:
    """
    List a run's logged media.

    Rows carry `tag`, `step`, `file` (relative to the run directory) and `bytes`.
    """
    return _expman_rs.read_media(run_dir)


def sweep_params() -> dict[str, Any]:
    """
    Parameters assigned to this trial by `exp sweep`, or {} outside a sweep.

    Read from the EXPMAN_PARAM_* environment variables, so a script can pick up
    its hyperparameters without an argparse layer. Values are coerced to int or
    float where they look numeric, since the environment only carries strings.

    Example:
        >>> params = expman.sweep_params()      # {"lr": 0.001, "bs": 32}
        >>> lr = params.get("lr", 3e-4)
    """
    out: dict[str, Any] = {}
    for key, raw in os.environ.items():
        if not key.startswith("EXPMAN_PARAM_"):
            continue
        name = key[len("EXPMAN_PARAM_") :].lower()
        value: Any = raw
        try:
            value = int(raw)
        except ValueError:
            try:
                value = float(raw)
            except ValueError:
                pass
        out[name] = value
    return out


def sweep_name() -> str | None:
    """Name of the sweep this process belongs to, or None outside one."""
    return os.environ.get("EXPMAN_SWEEP")


def load_provenance(run_dir: str) -> dict[str, Any]:
    """
    Read what code and environment produced a run.

    Carries git commit/branch/dirty (and the diff, if it was captured), the
    command line, hostname, and any scheduler job ids. Empty for runs logged
    before provenance capture existed or with it disabled.
    """
    return _expman_rs.load_provenance(run_dir)


def read_system_metrics(run_dir: str, to_pandas: bool = False):
    """
    Read a run's sampled hardware metrics (GPU/CPU/RAM).

    Rows carry `step` (sample index), `timestamp`, and one key per probe
    reading, e.g. `gpu.0.util_pct`, `mem.used_gb`. Empty when no probe was
    available on the machine that ran it.
    """
    rows = _expman_rs.read_system_metrics(run_dir)
    return _as_pandas(rows) if to_pandas else rows


def load_projects(base_dir: str = "experiments") -> list[dict[str, Any]]:
    """List every project in the store, each with its assigned experiments."""
    return _expman_rs.load_projects(base_dir=base_dir)


def assign_project(
    experiment: str,
    project: str | None,
    base_dir: str = "experiments",
) -> None:
    """
    Assign an experiment to a project without a running server.

    The same write Experiment.set_project() performs, callable with no open run
    — for a sync script projecting an external manifest into the store.
    Pass project=None to unassign.
    """
    _expman_rs.assign_project(experiment, project, base_dir=base_dir)


__all__ = [
    "Experiment",
    "init",
    "log_vector",
    "log_scalar",
    "log_params",
    "save_artifact",
    "tensorboard_dir",
    "info",
    "warn",
    "close",
    "fail",
    "load_runs",
    "read_metrics",
    "load_config",
    "load_run",
    "load_projects",
    "load_provenance",
    "sweep_params",
    "sweep_name",
    "read_system_metrics",
    "read_histograms",
    "read_media",
    "log_image",
    "log_histogram",
    "assign_project",
    "__version__",
    "SummaryWriter",
]
