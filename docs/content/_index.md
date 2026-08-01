+++
title = "expman-rs Documentation"
description = "Official documentation for expman-rs — high-performance ML experiment manager in Rust."
sort_by = "weight"
template = "section.html"
+++

# expman-rs — Documentation

*Last synced: 2026-07-29, against v1.0.1 plus the projects/read-API work.*

Entry point for all project documentation. Any LLM should be able to answer
questions about this project from this folder alone.

## What expman is

A high-performance experiment manager for ML training runs, written in Rust.
One crate (`expman`) ships three faces:

- **`exp` CLI** — list/inspect/clean/reap runs, manage projects (`exp project`),
  run hyperparameter sweeps locally or as a SLURM array (`exp sweep`), export to
  CSV/JSON/TensorBoard, import TensorBoard event files, and serve the dashboard
  (optionally `--read-only`).
- **Dashboard server** — axum HTTP API plus a Leptos/WASM single-page frontend
  compiled by trunk and embedded into the binary with `rust-embed`.
- **Python extension** — PyO3 module published to PyPI as `expman-rs`
  (import name `expman`), including a `SummaryWriter` drop-in for
  `torch.utils.tensorboard`, and a dependency-free **read API**
  (`load_runs`/`read_metrics`/`load_config`) so analysis scripts can treat
  expman as the durable record rather than reading Parquet directly.

The load-bearing claim is that `log_vector()` never blocks a training loop: it
is a send on an unbounded channel to a background tokio task that batches writes
into append-only Arrow IPC segments, compacted to Parquet when the run closes.

Alongside the user's own metrics the engine samples **hardware utilisation**
(GPU/CPU/RAM, via the vendors' own CLIs) and captures **provenance** (git commit,
command, scheduler job ids) — the two things that make a run diagnosable and
reproducible months later.

## Contents

| page | what it answers |
|---|---|
| [architecture](/architecture/) | components, data flow, threading, and the *why* behind them |
| [decisions](/decisions/) | dated log of significant decisions and their consequences |
| [how-to/setup](/how-to/setup/) | get a working dev environment |
| [how-to/build-and-run](/how-to/build-and-run/) | build the pieces, run the CLI/server/examples |
| [how-to/test-and-lint](/how-to/test-and-lint/) | the test suites and what each covers |
| [how-to/release](/how-to/release/) | cut a release; the traps that have bitten before |
| [how-to/add-an-api-endpoint](/how-to/add-an-api-endpoint/) | end-to-end recipe, server through frontend |
| [reference/storage-layout](/reference/storage-layout/) | on-disk format, data model, Parquet schema |
| [reference/http-api](/reference/http-api/) | every route, request, and response |
| [reference/cli](/reference/cli/) | every subcommand and flag |
| [reference/python-api](/reference/python-api/) | the `expman` Python surface |
| [reference/module-map](/reference/module-map/) | file-by-file map with feature/cfg gates |

## The hierarchy

`project → experiment → run`, with runs optionally in a **group**.

A group is the unit a distributed job or a sweep is reasoned about as: the N
ranks of a DDP job, or the trials of `exp sweep`, share one. It is a field on the
run, not a directory level, so nothing moves when one is created — and the
dashboard rolls a group into a single expandable row. A run's project is resolved **through** its
experiment (`experiment.yaml`'s `project:` field), so creating or reassigning a
project never moves run data. Every write to that field works offline, without a
server — which is what makes the layer usable from a cluster node.

A project may instead be a **generated projection** of an authoritative source
outside expman (`exp project sync`). Such a project carries only general
information and experiment membership; it is regenerated on each sync, marked
`generated: true`, and is read-only in the dashboard. See
[reference/cli](/reference/cli/#exp-project-sync).

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

## Credits & Acknowledgments

`expman-rs` stands on the shoulders of incredible tools, platforms, and AI assistants:

- **AI Pair Programming**:
  - **Antigravity** (Google DeepMind) & **Claude** (Anthropic) for agentic pair programming, codebase architecture, and refactoring assistance.
- **Experiment Tracking & Ecosystem Inspiration**:
  - **[TensorBoard](https://github.com/tensorflow/tensorboard)** — For the canonical `SummaryWriter` API design and event file interoperability.
  - **[ClearML](https://clear.ml)** — For inspiration on unified experiment organization, tracking, and dashboard workflows.
- **Frontend & Web Engine**:
  - **[Reticle](https://github.com/lokeshmohanty/reticle)** — The instrument-reading design system and Zola theme powering the documentation site.
  - **[Zola](https://www.getzola.org/)** — Fast static site generator powering the documentation.
  - **[Leptos](https://leptos.dev)** & **[Axum](https://github.com/tokio-rs/axum)** — Powering the embedded WASM dashboard and high-throughput async server.
- **Data & Serialization Core**:
  - **[Apache Arrow & Parquet](https://arrow.apache.org/)** — Columnar IPC and analytical storage format.
  - **[PyO3](https://pyo3.rs/)** — Seamless Rust bindings for Python.

