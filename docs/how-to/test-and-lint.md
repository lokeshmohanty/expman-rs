# How-to — test and lint

*Verified 2026-07-27.*

## Commands

| command | what |
|---|---|
| `just test` | `test-py` then `cargo nextest run --all-features` |
| `just test-release` | `cargo nextest run --all-features --no-capture` — what CI runs |
| `just test-py` | `cd wrappers/python && uv run --extra dev pytest tests` |
| `just test-watch` | `cargo watch -x 'nextest run --workspace'` |
| `just lint` | `lint-rust` + `lint-py` |
| `just lint-rust` | `lint-frontend` first, then `cargo clippy --all-features --all-targets -- -D warnings` |
| `just lint-frontend` | clippy against `--target wasm32-unknown-unknown` |
| `just fmt` / `just fmt-check` | `cargo fmt --all` |
| `just ci` | `fmt-check lint test lint-py test-py` — the full local gate |

All Rust test commands use `--all-features`, which enables `server` and so
triggers the `build.rs` trunk build. Set `EXPMAN_SKIP_FRONTEND_BUILD=1` if you
already have a `dist/`. See [build-and-run.md](build-and-run.md).

`cli_test.rs` uses `assert_cmd::cargo::cargo_bin_cmd!("exp")`, which needs the
`cli` feature — another reason the suite is always run with `--all-features`.

## What each suite covers

### `tests/integration_test.rs` — 12 tests, the core engine

Run dir creation · vectors → Parquet · params → YAML ·
**`test_log_vector_is_fast`** (the throughput benchmark, also `just bench`) ·
status persisted on close · artifact save (relative and absolute paths) ·
Parquet schema merge · latest-scalar readback · corrupt YAML metadata ·
concurrent logging from multiple threads · same-step replacement.

### `tests/api_test.rs` — 7 tests, the HTTP layer

In-process via `tower::ServiceExt::oneshot` against `build_router` + `AppState`
— no network. A `TempDir` fixture builds one experiment with one run.

Covers: list experiments, get/update experiment metadata, list runs, get run
metadata, server config, get metrics.

**Not covered:** every Jupyter route, every TensorBoard route, both SSE streams,
artifact content.

### `tests/cli_test.rs` — 10 tests, the binary

`list` (with and without `--experiment`) · `inspect` · `clean --keep 5` dry-run
(asserts nothing is deleted without `--force`) · `export` json/csv, with and
without data · `import` error paths · `export --format tensorboard` with no data.

### `wrappers/python/tests/` — 20 tests

`test_experiment.py` (6): basic file creation, singleton `init`, artifact
round-trip, complex types in `config.yaml`, crash → `status: FAILED`, and
**`test_vectors_vs_scalar`** — the important one, pinning that scalars land in
`run.yaml` and vectors in `vectors.parquet`, with cross-assertions both ways.

`test_tensorboard.py` (14): directory structure, context manager, default
`log_dir` generation, scalars with and without step, `add_scalars` prefixing,
`add_text`, `add_hparams` (including empty and `None` metrics), a parametrized
sweep asserting all 11 stub methods do not raise, `flush()` no-op, sequential
writers not interfering, and both `log_dir` mapping branches.

**Not covered:** `tensorboard_dir`; `cli.py` (the `exp` console script is never
exercised from Python).

> `test_default_log_dir` (`test_tensorboard.py:45`) takes no `tmp_path` and can
> write a `runs/` directory into the CWD — which is why
> `wrappers/python/.gitignore` contains exactly `runs/`.

## What CI runs

`ci.yml` (push + PR on `main`, ignoring `**.md`, `examples/**`, `docs/**`,
`.gitignore`, `LICENSE`) calls `build-assets`, then `rust.yml` and `python.yml`
in parallel.

- `rust.yml` — `just fmt-check` + `just lint-rust`, then `just test-release`.
  Both jobs download `frontend-dist` and set `EXPMAN_SKIP_FRONTEND_BUILD=1`.
- `python.yml` — `just lint-py` (ruff over `wrappers/python` and `examples/`),
  then downloads the CLI artifact, `just bundle-cli-bin`, `uv sync --extra dev`,
  builds and installs a wheel, and runs pytest.

Two gaps worth knowing:

- **`ruff format` never runs in CI** — only `ruff check`. Formatting drift is
  caught only by pre-commit hooks, which nothing installs automatically.
- **Python tests only ever run on Linux.** `python.yml:29` computes the CLI
  artifact name from `runner.os` with a nested ternary, implying an intended OS
  matrix that does not exist; the job is always `ubuntu-latest`.

And the big one: **a release does not wait for tests.** See
[release.md](release.md).
