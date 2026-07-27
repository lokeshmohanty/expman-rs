# expman-rs — Documentation

*Last synced: 2026-07-27, against commit `384c540` (v0.5.3) plus an uncommitted
TensorBoard integration in the working tree. Refresh with the `docs-sync` skill.*

Entry point for all project documentation. Any LLM should be able to answer
questions about this project from this folder alone.

## What expman is

A high-performance experiment manager for ML training runs, written in Rust.
One crate (`expman`) ships three faces:

- **`exp` CLI** — list/inspect/clean runs, export to CSV/JSON/TensorBoard,
  import TensorBoard event files, and serve the dashboard.
- **Dashboard server** — axum HTTP API plus a Leptos/WASM single-page frontend
  compiled by trunk and embedded into the binary with `rust-embed`.
- **Python extension** — PyO3 module published to PyPI as `expman-rs`
  (import name `expman`), including a `SummaryWriter` drop-in for
  `torch.utils.tensorboard`.

The load-bearing claim is that `log_vector()` never blocks a training loop: it
is a send on an unbounded channel to a background tokio task that batches writes
into Parquet.

## Contents

| page | what it answers |
|---|---|
| [architecture.md](architecture.md) | components, data flow, threading, and the *why* behind them |
| [decisions.md](decisions.md) | dated log of significant decisions and their consequences |
| [how-to/setup.md](how-to/setup.md) | get a working dev environment |
| [how-to/build-and-run.md](how-to/build-and-run.md) | build the pieces, run the CLI/server/examples |
| [how-to/test-and-lint.md](how-to/test-and-lint.md) | the test suites and what each covers |
| [how-to/release.md](how-to/release.md) | cut a release; the traps that have bitten before |
| [how-to/add-an-api-endpoint.md](how-to/add-an-api-endpoint.md) | end-to-end recipe, server through frontend |
| [reference/storage-layout.md](reference/storage-layout.md) | on-disk format, data model, Parquet schema |
| [reference/http-api.md](reference/http-api.md) | every route, request, and response |
| [reference/cli.md](reference/cli.md) | every subcommand and flag |
| [reference/python-api.md](reference/python-api.md) | the `expman` Python surface |
| [reference/module-map.md](reference/module-map.md) | file-by-file map with feature/cfg gates |

## Three names for one project

Do not conflate them:

| context | name |
|---|---|
| crates.io / Rust crate | `expman` |
| PyPI distribution | `expman-rs` |
| Python import | `import expman` |
| CLI binary | `exp` |
| Nix package attr | `expman` (alias `exp`) |

## Honest gaps

- No document covers the frontend's chart interaction internals
  (`src/app/components/charts.rs`, ~1000 lines of hand-rolled pan/zoom against
  `plotters-canvas`). `reference/module-map.md` points at it; the details are
  in the source.
- Performance claims (`~100ns` per `log_vector`) come from the project README
  and `tests/integration_test.rs::test_log_vector_is_fast`; no benchmark
  results are recorded here.
- `src/api/README.md`, `src/app/README.md`, and
  `src/app/components/README.md` are rendered into rustdoc and are **stale** —
  `src/api/README.md` documents an `/api/events` endpoint that does not exist.
  Treat `reference/http-api.md` as authoritative over them.
