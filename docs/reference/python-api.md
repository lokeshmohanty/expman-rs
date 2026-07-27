# Reference — Python API

*Hand-written from `wrappers/python/expman/*.py` and
`src/wrappers/python/mod.rs`, 2026-07-27.*

`pip install expman-rs` → `import expman`. `expman.__version__` comes from
`CARGO_PKG_VERSION` (`mod.rs:295`), i.e. it tracks `Cargo.toml`, **not**
`pyproject.toml`.

Only six methods and two getters cross into Rust. Everything else — console
redirection, `tensorboard_dir`, the whole module-level singleton API, `cli.py`,
and all of `tensorboard.py` — is pure Python.

## `class Experiment`

```python
Experiment(name, run_name=None, base_dir="experiments",
           flush_interval_rows=50, flush_interval_ms=500,
           redirect_console=True)
```

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

**Silent no-op stubs** (`:217-259`), present so ported code does not `TypeError`:
`add_histogram`, `add_image`, `add_images`, `add_figure`, `add_video`,
`add_audio`, `add_graph`, `add_embedding`, `add_pr_curve`,
`add_custom_scalars`, `add_mesh`.

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
