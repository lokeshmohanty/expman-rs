+++
title = "Architecture"
description = "Components, data flow, threading, and design rationale of expman-rs."
weight = 1
+++

# Architecture

*Last verified 2026-07-27 against v0.5.3 + uncommitted TensorBoard work.*

## The shape of the thing

One crate, `expman`, with `default = []` and three opt-in features. Nothing is
built unless asked for:

| feature | pulls in | gives you |
|---|---|---|
| `cli` | clap, comfy-table, indicatif, tboard, tensorboard-rs | the `exp` binary (`src/main.rs`, `src/cli/`) |
| `server` | axum, tower-http, rust-embed, mime_guess, tokio-util, futures-util | `src/api/` and the embedded frontend |
| `python` | pyo3 (`extension-module`, `abi3-py39`) | `src/wrappers/python/` — the PyO3 module |

Plus a fourth, orthogonal axis: **target arch**. `src/app/` (the Leptos
frontend) compiles only for `wasm32-unknown-unknown`; `src/core/engine.rs` and
`src/core/storage.rs` compile only for *not* wasm32. See `src/lib.rs:6-18` and
`src/core/mod.rs:9-19`.

The wasm boundary is why `src/app/models.rs` exists: the frontend can see
`core::models` and `core::error` but not the engine, so it re-declares its own
mirror types for JSON deserialization. **These two model sets can drift** — e.g.
`app::models::Run.started_at` is a `String` where the server sends a serialized
`DateTime<Utc>`. That drift is a standing hazard, accepted for now because
sharing the types would drag serde-compatible variants across the boundary for
little gain.

```
                    ┌─────────────────────────────┐
  Python training   │  expman (PyPI: expman-rs)   │
  loop  ───────────▶│  Experiment.log_vector()    │
                    └──────────────┬──────────────┘
                            PyO3, no GIL release
                                   ▼
  Rust training     ┌─────────────────────────────┐
  loop  ───────────▶│  core::LoggingEngine        │
                    │  unbounded mpsc channel     │
                    └──────────────┬──────────────┘
                                   ▼  (dedicated 1-worker tokio runtime)
                    ┌─────────────────────────────┐
                    │  background_task            │
                    │  batches → core::storage    │
                    └──────────────┬──────────────┘
                                   ▼
                    <base_dir>/<experiment>/<run>/
                      vectors.parquet, run.yaml,
                      config.yaml, run.log, artifacts/
                                   ▲
                                   │ reads (no cache)
                    ┌──────────────┴──────────────┐
                    │  api:: axum server          │──SSE──▶  Leptos WASM
                    │  + rust-embed'd dist/       │◀─fetch─  frontend
                    └─────────────────────────────┘
```

## The logging engine — why it is shaped this way

`src/core/engine.rs`. `LoggingEngine` holds *only* a channel sender, an `Arc<Runtime>`,
and the config (`engine.rs:55-60`). Every file handle and buffer lives inside the
spawned `background_task` (`engine.rs:218`). Consequences:

- **`log_vector` cannot block.** It builds a `VectorRow` and does a synchronous
  `UnboundedSender::send` (`engine.rs:137-141`). No lock on a file, no await, no
  fsync. That is the whole basis of the "~100ns, never blocks your training
  loop" claim.
- **The engine builds its own runtime** — `worker_threads(1)`, thread name
  `expman-io` (`engine.rs:100-107`) — because it must work from plain sync Rust
  *and* from Python, where there is no ambient tokio runtime to borrow.
- **The channel is unbounded**, so a fast producer applies no backpressure and
  the writer can fall arbitrarily behind. Combined with `flush_vectors` clearing
  its buffer even when the write errored (`engine.rs:342-345`), metric loss is
  silent. This is a deliberate latency-over-durability trade.
- **All write-path methods are fire-and-forget** (`let _ = send(...)`). Logging
  to a closed engine is a silent no-op. Only `flush()` — the one `async` method
  — can report failure.

### Batching

Task-local state at `engine.rs:230-235`. Three independent flush triggers:

1. `vector_buffer.len() >= flush_interval_rows` (default **50**)
2. a `tokio::time::interval` every `flush_interval_ms` (default **500 ms**),
   which also rewrites `run.yaml` with the current scalar/vector summary
3. `log_lines.len() >= 20` (hardcoded, `engine.rs:279`)

The select loop is `biased;` (`engine.rs:242`) so draining commands always wins
over the flush tick.

### Lifecycle

`run.yaml` is written eagerly with `status: Running` at construction
(`engine.rs:74-83`). `close(status)` blocks the calling thread on a oneshot with
**no timeout** (`engine.rs:188`); `Drop` does a best-effort shutdown as
`Finished` with a **5 s timeout** (`engine.rs:197-214`). That asymmetry is
unintentional but harmless in practice.

One real trap: `[profile.release] panic = "abort"` (`Cargo.toml:112`) means
`Drop` does **not** run on a release-mode panic, so the run stays `RUNNING` on
disk. Conversely, an unwinding debug-mode panic records `FINISHED`, not
`CRASHED`. A run only reads back as `CRASHED` when `run.yaml` is missing or
unparseable, via `minimal_run_metadata` (`storage.rs:193-212`).

## Storage — the honest version

`src/core/storage.rs`. The design is documented in its own header comment
(`:224-227`) as **read → concat → write back the whole file**. There is no true
append, no compaction, no rotation.

- `append_vectors` (`:228-300`) reads the existing Parquet, unions schemas via
  `merge_schemas`/`align_batch` (`:404-446`) so a metric first logged at step
  500 can coexist with earlier rows (missing columns back-fill as typed nulls),
  then rewrites the file with `fs::File::create` + SNAPPY.
- **Cost is O(total rows) per flush** — quadratic over a run. And the write is
  non-atomic: no temp-file-and-rename, so a crash mid-write truncates the run's
  entire metric history.
- **Every read path loads the whole file into memory**, including
  `read_vectors_since` (`:312-330`), which filters in memory *after* reading
  everything — and that is what the SSE endpoint calls every 500 ms.

The header comment names Arrow IPC columnar append as the intended fix.

### Schema

Inferred per batch, not fixed (`rows_to_record_batch`, `:448-535`):

- `step` → `Int64`, nullable
- `timestamp` → `Timestamp(Microsecond, "UTC")`, non-nullable
- one column per metric key, in first-seen order; type from the first non-null
  occurrence. `Float`/`Int` both widen to `Float64`; everything else becomes
  `Utf8`.

> **Known defect:** `:272` downcasts the existing `step` column to `UInt64Array`
> while `:475`/`:486` write it as `Int64`. The downcast always returns `None`,
> the `else` at `:286` keeps the existing batch unfiltered, and cross-batch step
> dedup (`:270-291`) is dead code. Re-logging the same step in two different
> flush batches produces duplicate rows. Verified 2026-07-27.

## The server

`src/api/`. Routes nested under `/api` (`mod.rs:153`); everything else falls
through to the embedded frontend (`mod.rs:155`). CORS is fully permissive —
`Any` origin/method/header (`mod.rs:146-149`), appropriate for a localhost tool
and a liability anywhere else.

`AppState` (`state.rs:8`) is `Clone`, cloned per request, and holds
`Arc<PathBuf> base_dir` plus two process managers and a `CancellationToken`.
The managers wrap `Arc<Mutex<HashMap<String, Instance>>>` using **`std::sync::Mutex`,
never held across an `.await`** — deliberate, since every critical section is a
short map operation.

**There is no caching at any layer.** Every request re-reads the filesystem and
re-parses Parquet. Fine for a single-user localhost dashboard; the first thing
to change if that assumption ever breaks.

Realtime is **SSE only** — two endpoints, both 500 ms poll loops with 15 s
keep-alives (`metrics.rs:41`, `:78`), terminated by `.take_until(shutdown.cancelled())`.
Axum's `ws` feature is enabled in `Cargo.toml` but **no WebSocket endpoint
exists**.

### Spawned-process services

`jupyter_service.rs` and (uncommitted) `tensorboard_service.rs` follow one
pattern: detect the tool by shelling out to `--version`, pick a free port by
scanning a range (8888–9999 for Jupyter, 6006–7999 for TensorBoard), spawn the
child with auth and framing protections disabled, register it in the map keyed
`"{exp}:{run}"`, and reap it on `status`/`stop`/`shutdown_all`.

**Jupyter's launch command is configuration, not a constant** (1.3.0). `exp serve
--jupyter-command` is a command *line* — shell-word-split, no shell — that the
`notebook --no-browser --port=…` arguments are appended to, so `uv run --extra nb
jupyter` composes. This is the whole kernel story: the kernel a notebook gets is
the interpreter Jupyter runs under, so launching Jupyter from inside the
project's environment makes the project's package importable with no
`ipykernel install` and no kernelspec for expman to keep in sync. Nothing is
written to `~/.local/share/jupyter`, by design.

**The generated notebook is a rendered template with a provenance stamp.** The
content comes from `--notebook-template`, else `<base_dir>/.expman/notebook.ipynb`,
else a built-in default; `{{run_dir}}`-style placeholders are substituted with
JSON-escaped values, and the result is parsed before being written so a bad
template degrades to the built-in rather than to a corrupt `.ipynb`. Each write
records `metadata.expman.{template_hash, content_hash}`, which is what lets a
later launch tell "expman wrote this and the template has since changed" (rewrite)
from "the user changed this" (never touch). Rules and rationale:
[CLI reference](/reference/cli/#exp-serve-dir).

**No HTTP proxying.** The browser connects directly to `http://localhost:{port}`
(`interactive.rs:120,210`; `tensorboard.rs:101,149`). That is why Jupyter is
launched with `--ServerApp.token='' --ServerApp.password='' --disable_check_xsrf`
and a loosened CSP. It also means **the dashboard only works when viewed from
the machine running the server** — a real limitation, and the reason a proxy
route is the obvious next architectural step.

## The frontend

`src/app/`, Leptos 0.8 CSR, built by trunk from `src/app/index.html` into
`dist/`, embedded via `rust-embed` (`api/frontend.rs:9-15`), with the SPA
fallback decided by `is_asset_request`: a path whose **last segment** ends in a
known asset extension (`ASSET_EXTENSIONS`) 404s when it is not in the bundle;
everything else gets `index.html` and is routed client-side.

The rule used to be "a path containing a `.` is an asset", which broke every
experiment named after a dm_control task — `/experiments/dmc-cartpole.swingup`
404'd, so the dashboard could not be deep-linked to one. The extension list is
deliberately wider than what the bundle embeds: serving `index.html` for a
missing `.js` hands the browser HTML where it expected JavaScript, and the
resulting syntax error names the wrong file. `html` is excluded, because
answering an HTML request with the shell is exactly what the fallback is for.

- Data fetching is plain `gloo_net` returning `Result<T, String>`, wrapped in
  `LocalResource` + `<Suspense>`. **Metrics do not auto-refresh** — only logs
  are live (SSE); charts have manual refresh buttons.
- Charts are `plotters` + `plotters-canvas` with hand-rolled pan/zoom, including
  a manual Liang–Barsky clipper in `utils.rs:16-100` because `CanvasBackend`
  does not clip.
- **Styling is Tailwind from a CDN**, hardcoded in `src/app/index.html:8` —
  along with a Google Fonts link. `stylist` is declared in `Cargo.toml:91` and
  **used nowhere in `src/`**. The practical consequence: the "single
  self-contained binary" story breaks offline, because the dashboard renders
  unstyled without internet. Verified 2026-07-27.

## The build dance

`build.rs` is the riskiest file in the repo for release engineering. When the
`server` feature is on and the target is not wasm32, it shells out to
`trunk build --release` (with `CARGO_TARGET_DIR=target/wasm_build` and
`MAKEFLAGS` stripped to avoid recursive-cargo deadlock). On failure it falls
back to an existing `dist/index.html`, writes placeholder assets during
`CARGO_DOC`, and otherwise **hard `exit(1)`s** (`build.rs:44`).

Every escape hatch in the repo exists to route around that one branch:
`EXPMAN_SKIP_FRONTEND_BUILD`, the `CARGO_DOC` placeholder path, `just prep-dist`,
`include = ["dist/**/*"]` in `Cargo.toml`, `cargo publish --allow-dirty`, and CI
downloading a `frontend-dist` artifact into `dist/` before every Rust job. See
[how-to/release](/how-to/release/).

## Python bridge

`src/wrappers/python/mod.rs`. A `#[pyclass] Experiment` holding
`Arc<Mutex<Option<LoggingEngine>>>` — the `Option` is the closed-sentinel, so a
double close is a no-op.

**There is no GIL management anywhere** — no `allow_threads`, no
`Python::with_gil`, no `py.detach()`. The design relies on every logging method
being bounded and non-blocking *while holding the GIL*: convert the dict, take
an uncontended mutex, send on the channel. That is a load-bearing assumption,
not an enforced invariant; if the send ever blocked, it would stall every Python
thread.

Two type-coercion helpers with **deliberately different ordering**:

- `py_dict_to_map` → `MetricValue`, tries `f64` first (`:254-264`). Since Python
  `bool` and `int` both extract as `f64`, **`True` and `1` both become
  `Float(1.0)`** — the `Int`/`Bool` arms are effectively dead for metrics.
- `py_dict_to_yaml` → `serde_yaml::Value`, tries `bool` → `i64` → `f64`
  (`:274-284`), so **params preserve their types faithfully**.

The asymmetry is intentional (metrics are numeric vectors, params are
configuration) but surprising if you only read one of the two.

Also note: all write-path errors are swallowed, `close()` maps any unrecognized
status string silently to `Finished` (`:194`), and `panic = "abort"` means a
Rust panic aborts the whole Python process rather than raising.

## Related

- Data model and on-disk layout: [reference/storage-layout](/reference/storage-layout/)
- Why each of these choices was made, dated: [decisions](/decisions/)
