# Verification in expman-rs — what "done" actually requires

*Recorded 2026-07-27.*

## The gate

`just ci` = `fmt-check lint test lint-py test-py`. Run it and read the output
before claiming a change works. Nothing else gates a release — CI runs *in
parallel with* publication, so a green CI after the fact is not protection.

If `--all-features` fails at build-script time, that is the `build.rs` trunk
trap, not your change. `EXPMAN_SKIP_FRONTEND_BUILD=1` and retry.

## What CI will not catch

Do not treat a green CI as coverage for these:

- **`ruff format`** — never runs in CI, only `ruff check`.
- **Non-Linux Python** — `python.yml` always runs on `ubuntu-latest` despite an
  artifact-name expression that implies a matrix.
- **Version alignment** across `Cargo.toml`, `pyproject.toml`, and the two
  places in `flake.nix`. Nothing validates it.
- **The Jupyter and TensorBoard routes** — zero test coverage in
  `tests/api_test.rs`.
- **Both SSE streams** — untested.
- **The `exp` console script** (`wrappers/python/expman/cli.py`) — never
  exercised from Python.

## Test-fixture hygiene

Every suite uses `TempDir` / `tmp_path`. The `test_artifacts/`,
`test_experiments/`, `test_results/`, and `scratch/` directories at the repo
root are **local debris, not fixtures** — nothing reads them. Do not wire tests
to them.

One exception to watch: `test_default_log_dir`
(`wrappers/python/tests/test_tensorboard.py:45`) takes no `tmp_path` and can
write a `runs/` dir into the CWD.

## Claims that need evidence, not inference

The engine's write path swallows errors by design (`let _ = send(...)`,
`flush_vectors` clears its buffer even on failure). **A logging call succeeding
proves nothing about data reaching disk.** When verifying anything about
persistence, read the resulting `vectors.parquet` / `run.yaml`, do not infer
from a call returning.

Same for the Python layer: all write-path errors are swallowed there too. Only
the `run_dir` / `run_name` getters raise.

## Known-failing things — do not "fix" by accident

`src/core/storage.rs:272` downcasts `step` to `UInt64Array` when it is written
as `Int64`, so cross-batch step dedup is dead code. Tests pass anyway —
`test_log_vector_replaces_step` exercises the *in-batch* path, which works. If
you fix the downcast, the cross-batch path activates for the first time and
behavior around duplicate steps will change. Flag it rather than slipping it
into an unrelated change.
