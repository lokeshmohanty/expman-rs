# Reference — module map

*Hand-written 2026-07-27. ~8,100 lines of Rust in `src/`.*

Two orthogonal gating axes: **cargo features** (`cli`, `server`, `python`; all
opt-in, `default = []`) and **target arch** (`wasm32` vs not).

## Crate roots

| file | gate | what |
|---|---|---|
| `src/lib.rs` | — | 18 lines of module declarations; crate docs are `include_str!("../README.md")` |
| `src/main.rs` | `cli` | the `exp` binary: `init_tracing()` then `run_cli()` |
| `src/app/main.rs` | wasm32 | the `expman-app` binary: `mount_to_body(App)` |
| `build.rs` | — | invokes `trunk build --release` when `server` is on. **Read `docs/how-to/build-and-run.md` before touching.** |

```rust
// src/lib.rs
pub mod core;                                             // always
#[cfg(all(feature = "server", not(wasm32)))] pub mod api;
#[cfg(wasm32)]                               pub mod app;
#[cfg(feature = "cli")]                      pub mod cli;
#[cfg(feature = "python")]                   pub mod wrappers;
```

## `src/core/` — always compiled

| file | gate | what |
|---|---|---|
| `models.rs` | — | `ExperimentConfig`, `MetricValue`, `VectorRow`, `RunStatus`, `RunMetadata`, `ExperimentMetadata`, `ProjectMetadata` — the **storage** models |
| `dto.rs` | — | the **wire** types: `Experiment`, `Project`, `ProjectDetail`, `Run`, `Artifact`, `GlobalStats`, `ServerConfig`, `ReadmeContent`, `InteractiveBackend`, … Compiled for native *and* wasm32 so the server and the frontend share one definition; `src/app/models.rs` only re-exports them |
| `error.rs` | — | `ExpmanError` (`Arrow`/`Parquet` variants are non-wasm only) |
| `engine.rs` | **not** wasm32 | `LoggingEngine`, `LogLevel`, `LogCommand`, `background_task` |
| `storage.rs` | **not** wasm32 | Parquet read/write, YAML helpers, `list_runs`/`list_experiments`/`list_artifacts`, `ArtifactInfo`, the project helpers (`set_experiment_project`, `list_project_experiments`), and the shared run index (`RunEntry`, `RunQuery`, `query_runs`, `parse_tag_expr`, `is_run_stale`, `looks_alive`) |
| `projects.rs` | **not** wasm32 | one-way project sync from a YAML manifest: `ProjectManifest`, `sync_project`, README rendering, the `generated` marker |
| `sweep.rs` | **not** wasm32 | sweep expansion (grid/random, seeded SplitMix64), trial command/env rendering, sbatch emission |
| `sysmetrics.rs` | **not** wasm32 | subprocess probes for GPU/CPU/RAM: `ProbeSpec`, `SystemSampler`, the nvidia-CSV and generic-JSON parsers |
| `provenance.rs` | **not** wasm32 | git/command/hostname/scheduler capture written to `provenance.yaml` |

**The wasm boundary is here**: `models` and `error` are the only core modules the
frontend sees. It reuses the data types but gets no engine, no storage, and no
Arrow/Parquet/tokio.

Re-exported at `core::` root: `LoggingEngine`, `LogLevel`, `ExpmanError`,
`ExperimentConfig`, `MetricValue`, `RunMetadata`, `RunStatus`, `VectorRow`.
**Not** re-exported (reach via the module path): `models::ExperimentMetadata`,
`storage::ArtifactInfo`, `error::Result`, all `storage` free functions.

`core/mod.rs:21-23` also has a doc-only `pub mod jupyter_integration {}` that
exists purely to inline `src/app/README.md` into rustdoc.

## `src/api/` — `server` feature, non-wasm

| file | what |
|---|---|
| `mod.rs` | router, CORS, `serve()`, graceful shutdown |
| `state.rs` | `AppState` (base_dir, jupyter, tensorboard, shutdown token), `ServerConfig` |
| `projects.rs` · `experiments.rs` · `runs.rs` · `metrics.rs` · `artifacts.rs` · `stats.rs` | handlers — see [http-api.md](http-api.md) |
| `frontend.rs` | `rust-embed` of `dist/`, SPA fallback |
| `jupyter_service.rs` | 537 lines: backend detection, port scan 8888–9999, `.ipynb` generation, spawn/status/stop. Has unit tests at `:449-537`. |
| `jupyter_handlers.rs` | per-run and `__multi__` handlers |
| `tensorboard_service.rs` | **untracked**. 175 lines, near-verbatim clone of the Jupyter manager. Port scan 6006–7999. **No tests.** |
| `tensorboard_handlers.rs` | **untracked**. 5 handlers. |

## `src/app/` — wasm32 only (Leptos 0.8 CSR)

| file | what |
|---|---|
| `mod.rs` | `App`, sidebar shell, routes |
| `main.rs` | wasm entry |
| `models.rs` | thin re-export of `core::dto` (since 2026-07-28). Previously hand-mirrored JSON types, which drifted; add fields to `core::dto` instead |
| `fetch.rs` | 411 lines of `gloo_net` calls returning `Result<T, String>` |
| `utils.rs` | `SidebarContext`, `CHART_COLORS`, Liang–Barsky clipping, canvas→PNG download |
| `pages/dashboard.rs` | `/` — global stats + recent experiments + recent projects |
| `pages/projects.rs` | `/projects` — project grid and creation modal |
| `components/hparams.rs` | the Compare tab: params × final-metrics table plus an SVG scatter, fed by `/projects/{p}/runs` |
| `pages/project_detail.rs` | `/projects/:id` — overview with markdown README and experiments list |
| `pages/experiments.rs` | `/experiments` |
| `pages/experiment_detail.rs` | `/experiments/:id` — tabs + run-selection sidebar + metadata editing |
| `pages/settings.rs` | `/settings` — one `debug_enabled` toggle in localStorage |
| `pages/not_found.rs` | fallback |
| `components/charts.rs` | ~1013 lines: `MetricsView`, `LineChart`, `ScalarChart`. Hand-rolled pan/zoom over `plotters-canvas`. |
| `components/runs_table.rs` | table + CSV/LaTeX/Typst export via blob download |
| `components/artifacts.rs` | artifact browser, tabular preview |
| `components/console.rs` | live log via raw `web_sys::EventSource` |
| `components/interactive.rs` | Jupyter tab — iframes `http://localhost:{port}` |
| `components/tensorboard.rs` | **untracked**. TensorBoard tab, single-run only. |
| `components/zoom.rs` | shared zoom controls |

Frontend styling is **Tailwind from a CDN** hardcoded in `src/app/index.html:8`.
`stylist` is a declared dependency that is used nowhere.

## `src/cli/` — `cli` feature

Single `mod.rs`: clap definitions plus every subcommand implementation. See
[cli.md](cli.md).

## `src/wrappers/` — `python` feature

`mod.rs` re-gates `pub mod python`; `python/mod.rs` (298 lines) is the
`#[pymodule]`. See [python-api.md](python-api.md).

## Outside `src/`

| path | what |
|---|---|
| `wrappers/python/` | the PyPI package: `expman/{__init__,cli,tensorboard}.py`, `pyproject.toml`, `tests/` |
| `tests/` | `integration_test.rs` (12 core tests), `api_test.rs` (7 HTTP tests), `cli_test.rs` (10 binary tests) |
| `examples/rust/` | `logging.rs`, `test_rust.rs` |
| `examples/python/` | `basic_training.py`, `singleton_usage.py`, `tensorboard_migration.py` |
| `Justfile` | every build/test/lint/release command |
| `flake.nix` | devShell + `packages.expman` + `packages.python3Packages.expman-rs` |
| `.github/workflows/` | 11 workflows — see [../how-to/release.md](../how-to/release.md) |
| `dist/` | trunk output, gitignored but shipped in the crate via `include` |

## Stale in-tree docs

`src/api/README.md`, `src/app/README.md`, and `src/app/components/README.md` are
rendered into rustdoc via `#![doc = include_str!]`. **`src/api/README.md` is
stale** — it documents a nonexistent `/api/events` endpoint and mentions
`utoipa`. Prefer `docs/reference/`.
