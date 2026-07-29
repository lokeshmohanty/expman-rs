# Reference — Python API

*Hand-written from `wrappers/python/expman/*.py` and
`src/wrappers/python/mod.rs`, updated 2026-07-28.*

`pip install expman-rs` → `import expman`. `expman.__version__` comes from
`CARGO_PKG_VERSION` (`mod.rs:295`), i.e. it tracks `Cargo.toml`, **not**
`pyproject.toml`.

The write path, the projects layer, and the read API cross into Rust. Console
redirection, `tensorboard_dir`, the module-level singleton API, the `to_pandas`
convenience, `cli.py`, and all of `tensorboard.py` are pure Python.

## `class Experiment`

```python
Experiment(name, run_name=None, base_dir="experiments",
           flush_interval_rows=50, flush_interval_ms=500,
           redirect_console=True,
           project=None, tags=None, description=None,
           heartbeat_interval_secs=30,
           group=None, rank=None,
           system_metrics_interval_secs=15,
           capture_provenance=True, capture_diff=False)
```

`project`, `tags` and `description` are written **at creation**, offline — no
server required, so this works on a compute node running a SLURM batch job.
`project` lands in `experiment.yaml`; `tags` and `description` land in
`run.yaml`. Before this, consumers had to hand-patch `run.yaml` after `close()`.

`heartbeat_interval_secs` controls how often the engine refreshes
`run.yaml.heartbeat_at`, which is what lets `exp reap` distinguish a
hard-killed run from a legitimately long one. Pass `0` to disable.

> `experiment.yaml` is only *created* when absent, but an explicit `project=`
> on a later run still takes effect: the engine updates that one field rather
> than ignoring it.

`group` and `rank` are **auto-detected** and rarely passed by hand. `rank` comes
from `EXPMAN_RANK` / `RANK` / `SLURM_PROCID` / `OMPI_COMM_WORLD_RANK` /
`LOCAL_RANK`; `group` from `EXPMAN_GROUP` or the scheduler's job id. A DDP script
therefore needs *no* expman-specific changes: each rank becomes its own run
(named `<group>-rank<N>`, so ranks cannot collide), and the dashboard rolls the
group into a single expandable row.

`system_metrics_interval_secs` samples GPU/CPU/RAM into `system.parquet` via
subprocess probes — `nvidia-smi`, `rocm-smi`, `tpu-info`, plus `/proc`. Probes
whose binary is absent are skipped silently, so a laptop logs CPU and memory and
says nothing about GPUs. `0` disables sampling.

`capture_provenance` records git commit/branch/dirty, the command line, hostname
and scheduler job ids into `provenance.yaml`. `capture_diff` additionally records
the working-tree diff; it is **off by default** because a dirty tree routinely
contains an edited `.env` or a pasted key, and this store may be shared.

`redirect_console` is **Python-only** (not a PyO3 parameter). When true, it opens
`<run_dir>/console.log` line-buffered and swaps `sys.stdout`/`sys.stderr` for a
`Tee` (`__init__.py:34-48`) that writes to both. Redirect failure is caught and
printed as a warning. `atexit.register(self.close)` is registered at
`__init__.py:112`.

| member | backing | notes |
|---|---|---|
| `log_params(params: dict)` | PyO3 | merged into `config.yaml`. Types are **preserved** (bool/int/float stay distinct). |
| `log_vector(values: dict, step: int\|None = None)` | PyO3 | the hot path — a channel send. `True` and `1` both become `Float(1.0)`. |
| `log_scalar(key: str, value)` | PyO3 | replaces an existing value; lands in `run.yaml.scalars`. |
| `save_artifact(path: str)` | PyO3 | copied under the run's `artifacts/`; relative paths preserved. |
| `info(message)` / `warn(message)` | PyO3 | appended to `run.log`. |
| `set_project(project: str\|None)` | PyO3 | writes only `project:` in `experiment.yaml`; offline. `None` unassigns. |
| `project` | PyO3 getter | reads it back from `experiment.yaml`. |
| `set_tags(tags)` / `add_tags(tags)` | PyO3 | replace or union this run's tags. `add_tags` is idempotent per tag. |
| `set_description(text)` | PyO3 | sets this run's description. |
| `group` / `rank` | PyO3 getters | this run's place in its group, or None. |
| `is_primary` | Python | True on rank 0 **or** a non-distributed run. Guard once-per-job work with it, not `rank == 0`, which is False when there is no rank. |
| `log_image(tag, image, step=None)` | PyO3 | bytes, a path, a PIL Image, a matplotlib Figure, or an HWC/CHW numpy array. Raises on anything else. |
| `log_figure` / `log_audio` / `log_video` | PyO3 | as above; audio/video take encoded bytes. |
| `log_histogram(tag, values=..., bins=64)` | PyO3 | bins here (numpy if present, pure Python otherwise), or pass `edges=`/`counts=` precomputed. |
| `run_dir` / `run_name` | PyO3 getters | **raise** `RuntimeError("Engine is closed")` after close — unlike the write methods, which silently no-op. |
| `tensorboard_dir` | pure Python **(uncommitted)** | `<run_dir>/tensorboard`, `makedirs(exist_ok=True)`. Intended for `torch.profiler.tensorboard_trace_handler`. |
| `close(status="FINISHED")` | both | restores stdout/stderr, closes the console file, then closes the engine. Idempotent. |
| `__enter__` / `__exit__` | Python | `__exit__` maps an exception to `status="FAILED"` and returns `False` (does not suppress). |

> **Status strings are not validated.** `close()` maps anything it does not
> recognize silently to `FINISHED` (`mod.rs:194`). A typo'd status is not an
> error.

```python
with expman.Experiment("my_exp", base_dir="./experiments") as exp:
    exp.log_params({"lr": 3e-4, "batch_size": 32, "amp": True})
    for step in range(1000):
        exp.log_vector({"train/loss": loss, "train/acc": acc}, step=step)
    exp.log_scalar("best_acc", best)
    exp.save_artifact("model.pt")
```

```python
# Everything the projects layer needs, from a batch job with no dashboard.
with expman.Experiment(
    "e1-drift-regret-slope",
    base_dir="./experiments",
    project="study-1",
    tags=["arm:tiered", "seed:1", "study:1"],
    description="tiered allocation, seed 1",
) as exp:
    ...
```

## Module-level singleton

A global `_current_exp` (`__init__.py:31`). `init(...)` takes the same arguments
as `Experiment(...)` and closes any existing global experiment first.

```python
expman.init("my_exp")
expman.log_params({...}); expman.log_vector({...}, step=i)
expman.log_scalar(k, v); expman.save_artifact(path)
expman.tensorboard_dir()      # -> str | None   (uncommitted)
expman.info(msg); expman.warn(msg)
expman.close()
```

With no active experiment, `log_*`/`save_artifact`/`tensorboard_dir` print
`"Warning: No active experiment. Call expman.init() first."`; `info`/`warn`
**silently no-op** — an inconsistency, not a design.

## Read API

Everything here reads an existing store, and returns **plain dicts and lists**
so an analysis script works in a bare environment. `to_pandas=True` imports
pandas lazily and only then — pandas stays an optional extra, never a runtime
dependency of the wheel.

This is what previously did not exist, and its absence is why analysis scripts
read Parquet and a side-car JSON manifest directly, leaving the manifest rather
than expman as the durable record.

| function | returns |
|---|---|
| `load_runs(base_dir="experiments", project=None, experiment=None, status=None, tags=None, to_pandas=False)` | run dicts, newest first |
| `read_metrics(run_dir, to_pandas=False)` | one dict per logged row |
| `load_config(run_dir)` | the run's `config.yaml` (logged params) |
| `load_run(run_dir)` | the run's `run.yaml` metadata |
| `load_projects(base_dir="experiments")` | every project, with its experiments |
| `load_provenance(run_dir)` | git/command/hostname/scheduler captured at creation |
| `read_system_metrics(run_dir, to_pandas=False)` | sampled hardware metrics |
| `read_histograms(run_dir)` | logged distributions: tag, step, edges, counts, total |
| `read_media(run_dir)` | logged images/audio/video: tag, step, file, bytes |
| `sweep_params()` | this trial's hyperparameters, typed, or `{}` outside a sweep |
| `sweep_name()` | the sweep this process belongs to, or None |
| `assign_project(experiment, project, base_dir="experiments")` | `None`; assigns with no open run |

Each `load_runs()` row carries `run`, `experiment`, `project`, `status`,
`started_at`, `finished_at`, `heartbeat_at`, `duration_secs`, `description`,
`tags`, `scalars`, `vectors`, and `path`. Feed that `path` straight to
`read_metrics()` / `load_config()` — the two compose, so a caller never does
path arithmetic.

`load_runs()` also takes `group=` to select one DDP job or sweep cohort.

`tags` accepts either a list (all must match) or an expression string with
`AND`/`OR` and parentheses:

```python
import expman

runs = expman.load_runs(tags="arm:tiered AND (study:1 OR study:2)")
for run in runs:
    metrics = expman.read_metrics(run["path"])
    params = expman.load_config(run["path"])
    print(run["run"], params["lr"], metrics[-1]["regret"])

df = expman.read_metrics(runs[0]["path"], to_pandas=True)   # needs pandas
```

`assign_project()` is `Experiment.set_project()` without an open run — for a
sync script projecting an external manifest into the store.

> `read_metrics()` reads the Parquet directly and is **never downsampled** — the
> cap applies to the HTTP endpoint the browser uses, not to analysis code.

### Media and distributions

```python
exp.log_image("samples/epoch", pil_image, step=epoch)   # or bytes, path, ndarray
exp.log_figure("confusion", fig, step=epoch)            # matplotlib
exp.log_histogram("weights/fc1", model.fc1.weight, step=epoch)
```

Images are written to `<run>/media/` with a `media.jsonl` manifest, not into
Parquet — a column of image blobs would make the metrics file useless for its
actual purpose. Histograms are stored as one row per (tag, step) with JSON
`edges`/`counts`, because bin counts vary and a column-per-bin schema would churn
on every call.

### Sweeps from a training script

```python
import expman

params = expman.sweep_params()      # {"lr": 0.001, "bs": 32} — typed, or {}
with expman.Experiment("e1") as exp:  # group/project/run name come from the env
    exp.log_params(params)
    ...
```

## `class SummaryWriter` — TensorBoard drop-in

`wrappers/python/expman/tensorboard.py`, pure Python. Signature-compatible with
`torch.utils.tensorboard.SummaryWriter`; `purge_step`, `max_queue`,
`flush_secs`, and `filename_suffix` are accepted and ignored.

**`log_dir` mapping** (`tensorboard.py:81-86`):

| `log_dir` | becomes |
|---|---|
| `"runs/exp1"` | `Experiment("exp1", base_dir="runs")` |
| `"my_exp"` (no separator) | `Experiment("my_exp", base_dir="experiments")` |
| `None` | `runs/<Mon DD_HH-MM-SS>_<hostname><comment>` |

Implemented: `add_scalar` → one `log_vector`; `add_scalars` → prefixes keys with
`f"{main_tag}/{k}"` into one `log_vector`; `add_text` → `info(...)`;
`add_hparams` → `log_params` + conditional `log_vector`; `flush()` is a
documented no-op (expman auto-flushes); `close()`, `__enter__`/`__exit__`.

Also implemented, since 2026-07-29: `add_histogram`, `add_image`, `add_images`
and `add_figure` store natively; `add_audio`/`add_video` store encoded bytes.

**Warn-once, no longer silent.** `add_graph`, `add_embedding`, `add_pr_curve` and
`add_mesh` cannot be stored, and now emit a `UserWarning` the first time each is
called. They were previously **silent no-ops**, so swapping
`torch.utils.tensorboard.SummaryWriter` for this class threw away every image and
histogram a user logged with nothing said at runtime — code that looked like it
worked. For those four, point a real TensorBoard writer at
`exp.tensorboard_dir`; the dashboard renders it in the TensorBoard tab.
`add_custom_scalars` stays quiet deliberately: it is a layout hint carrying no
data.

> The compat layer **warns rather than raises** on input it cannot encode, unlike
> the native `log_image`, which raises. It wraps code written for TensorBoard,
> often mid-way through a multi-day run; killing that run over one unencodable
> image would be a worse failure than the one being reported.

The constructor eagerly calls `log_params({})` and `info("SummaryWriter
initialized")` (`:97-98`) so the run's files appear on disk immediately.

## `expman.cli:main`

A 31-line shim (`cli.py`) exposed as the `exp` console script. Resolves
`<package_dir>/bin/exp` (`exp.exe` on win32), best-effort `chmod 0o755`, then
`subprocess.call([bin_path] + sys.argv[1:])` and forwards the exit code. Missing
binary → stderr message and `return 1`.

## Behaviors worth knowing

- **All write-path errors are swallowed** in the PyO3 layer. Lock poisoning and
  a closed engine both produce `Ok(())`. Only the getters raise.
- **The GIL is never released.** See
  [../architecture.md](../architecture.md#python-bridge).
- **`panic = "abort"`** means a Rust panic aborts the whole Python process
  rather than raising a Python exception.
- `abi3-py39` + `requires-python = ">=3.9"` means one wheel per platform covers
  3.9+. Note ruff is configured with `target-version = "py310"`
  (`pyproject.toml:36`) — a mismatch that could let 3.10-only syntax through and
  break 3.9 users.

## Tests

`wrappers/python/tests/test_experiment.py` (6 tests) and `test_tensorboard.py`
(14). The important one is `test_vectors_vs_scalar` (`test_experiment.py:90`),
which pins the scalars-vs-vectors split in both directions.

Gaps: no test for `tensorboard_dir`; no test exercises `cli.py`;
`test_default_log_dir` (`test_tensorboard.py:45`) takes no `tmp_path` and may
write a `runs/` dir into the CWD — which is why `wrappers/python/.gitignore`
contains exactly `runs/`.
