# Decisions

*Append-only log of significant decisions. Dates are the date the decision became
visible in the repo (commit date), not necessarily when it was made. Reconstructed
from source and git history on 2026-07-27 — entries before that date are inferred,
so rationale marked "inferred" was not written down at the time.*

---

## Unbounded channel + fire-and-forget logging

**Decision.** `LoggingEngine`'s command channel is `mpsc::unbounded_channel`, and
every write-path method discards send errors (`let _ = ...`).

**Why.** The product claim is that `log_vector()` never perturbs a training loop.
A bounded channel would either block the caller or force an async API; either
breaks the claim. (Inferred from `src/core/engine.rs:5-7` doc comment.)

**Consequences we accept.** No backpressure — a fast producer can push the writer
arbitrarily behind. `flush_vectors` clears its buffer even when the write failed
(`engine.rs:342-345`), so metric loss is silent. Logging to a closed engine
silently no-ops. Only `flush()` can report an error.

---

## Read-concat-rewrite instead of true Parquet append

**Decision.** Each flush reads the whole `vectors.parquet`, concatenates the new
batch, and rewrites the file (`src/core/storage.rs:228-300`).

**Why.** Parquet has no append primitive, and per-flush row groups would
fragment the file. The header comment (`storage.rs:224-227`) states the intent
and names Arrow IPC columnar append as the eventual fix.

**Consequences we accept.** O(total rows) per flush → quadratic over a long run.
The write is non-atomic (`fs::File::create`, no temp-and-rename), so a crash
mid-write destroys the run's whole metric history. Every read path loads the
entire file into memory, including the 500 ms SSE poll.

---

## Per-batch inferred schema with diagonal concat

**Decision.** The Arrow schema is derived from each batch's keys rather than
declared up front; `merge_schemas` + `align_batch` union the fields and back-fill
missing columns with typed nulls (`storage.rs:404-446`).

**Why.** Training code adds metrics mid-run (a validation metric that first
appears at step 500 must coexist with earlier training-only rows).

**Consequences.** Type is fixed by the *first non-null* occurrence of a key.
`Float` and `Int` both widen to `Float64`; anything else becomes `Utf8`.

---

## Dedicated single-worker tokio runtime inside the engine

**Decision.** `LoggingEngine::new` builds its own `worker_threads(1)` runtime
named `expman-io` rather than requiring an ambient one (`engine.rs:100-107`).

**Why.** The engine is called from plain sync Rust and from Python, neither of
which has a tokio runtime to borrow.

---

## No GIL release in the PyO3 bridge

**Decision.** No `allow_threads` / `Python::with_gil` / `py.detach()` anywhere
in `src/wrappers/python/mod.rs`.

**Why.** Every logging method is bounded and non-blocking — dict conversion, an
uncontended mutex, a channel send. Releasing and re-acquiring the GIL around
that would cost more than the work itself.

**Consequence.** Load-bearing assumption, not an enforced invariant. If the send
ever became blocking, it would stall all Python threads.

---

## Metrics float-flatten; params keep their types

**Decision.** `py_dict_to_map` tries `f64` first (`mod.rs:254-264`), so Python
`True` and `1` both land as `MetricValue::Float(1.0)`. `py_dict_to_yaml` tries
`bool` → `i64` → `f64` (`:274-284`), preserving types.

**Why.** Metrics are numeric time series; hyperparameters are configuration that
must round-trip faithfully into `config.yaml`.

**Consequence.** The `Int`/`Bool` arms of `MetricValue` are effectively dead for
Python-sourced metrics. Exercised by
`wrappers/python/tests/test_experiment.py::test_vectors_vs_scalar`.

---

## Spawn-and-iframe rather than proxy, for Jupyter and TensorBoard

**Decision.** The server spawns `jupyter notebook` / `tensorboard` as child
processes on a scanned free port, and the browser connects **directly** to
`http://localhost:{port}`. No HTTP proxying through the axum server.

**Why.** Proxying Jupyter (websockets, XSRF, kernel protocol) is substantial
work; direct iframe was the cheap path. (Inferred.)

**Consequences we accept.** Jupyter runs with `--ServerApp.token=''
--ServerApp.password='' --disable_check_xsrf=True` and a loosened CSP
(`jupyter_service.rs:305-315`). **The dashboard therefore only works when viewed
from the machine running the server.** A proxy route is the obvious next
architectural step. The uncommitted TensorBoard service repeats the pattern and
additionally uses `--bind_all` (0.0.0.0, no auth) — see `STATUS.md`.

---

## SSE, not WebSockets

**Decision.** Realtime uses two Server-Sent Events endpoints, each a 500 ms poll
loop (`src/api/metrics.rs:41`, `:78`).

**Why.** One-directional server→client streaming with automatic browser
reconnection, no protocol upgrade to manage. (Inferred.)

**Note.** Axum's `ws` feature is enabled in `Cargo.toml:71` but no WebSocket
endpoint exists. The feature is dead weight.

---

## Tailwind via CDN

**Decision.** `src/app/index.html:8` loads `https://cdn.tailwindcss.com` and a
Google Fonts stylesheet at runtime. `stylist` is a declared dependency
(`Cargo.toml:91`) that is **used nowhere**.

**Consequence.** The "single self-contained binary" story is not actually
self-contained: the dashboard renders unstyled without internet. Verified
2026-07-27. Either vendor Tailwind into `dist/` or drop the claim.

---

## 2026-07 (`49c24ca`) — ship `dist/` inside the published crate

**Decision.** `Cargo.toml` gained an explicit `include = [...]` list containing
`"dist/**/*"`.

**Why.** `dist/` is in `.gitignore`, so `cargo package` was omitting the built
frontend. Consumers building with `--features server` then hit `build.rs`'s hard
`exit(1)`.

---

## 2026-07 (`d423ff8`) — `--allow-dirty` and the `CARGO_DOC` escape hatch

**Decision.** Four changes in one commit:

1. `publish-cargo.yml:29` gained `--allow-dirty`. Direct consequence of the
   above: CI downloads `frontend-dist` into `dist/`, which is both git-ignored
   *and* in Cargo's `include`, and cargo refuses to package files it will ship
   but that are not committed.
2. Reverted the previous commit's `prep-dist` change — `touch dist/index.html`
   is back (`Justfile:37-38`).
3. `build.rs` gained a `CARGO_DOC` branch (`:12`, `:30-40`) writing placeholder
   `dist/` assets instead of `exit(1)`, unblocking `docs.yml`.
4. Riders: `.ipynb_checkpoints/` filtering in `storage.rs:36,59`, and
   `tensorboard.py` switching to `redirect_console=True` with eager
   `log_params({})` / `info(...)` so files appear immediately.

**Read `49c24ca` and `d423ff8` together** — the second exists because of the first.

---

## Open questions

- Should the version live in one place? It is duplicated across `Cargo.toml:3`,
  `wrappers/python/pyproject.toml:7`, and `flake.nix:86,127`, kept aligned only
  by `just bump`, with nothing in CI validating it.
- Should releases block on tests? Today `publish.yml` and `ci.yml` are separate
  workflows with no dependency, so a release publishes **in parallel with** its
  own test run. A test failure does not block publication.
- `.maturinignore` at the repo root has paths missing the `wrappers/` prefix and
  may be dead — `pyproject.toml`'s `include` glob is what actually works.
