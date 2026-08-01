+++
title = "Decisions Log"
description = "Append-only log of architectural decisions, choices, and consequences in expman-rs."
weight = 2
+++

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

## Tailwind via CDN — **superseded 2026-07-28**

> Reversed. Both the Tailwind CDN and the Google Fonts link are gone; see
> *"Type is self-hosted and assigned by role"* and *"Tailwind is built, not
> fetched from a CDN"* below. Kept for the record.

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

## 2026-07 — Virtual projects layer via `.projects` metadata

**Decision.** Experiments can be assigned to a project via a `project` field in `experiment.yaml`. Projects are stored as metadata in `<base_dir>/.projects/<project_name>/project.yaml` and `README.md` (markdown frontpage). `list_experiments` ignores all dot-directories (such as `.projects` and `.ipynb_checkpoints`).

**Why.** Avoids altering the flat directory layout of existing experiments (`<base_dir>/<experiment>/<run>`), maintaining full backwards compatibility. Experiments without a project remain unassigned in the main listing by default.

**Consequences.** Non-breaking file layout. Project deletion unassigns member experiments without touching their directory contents or runs. Client-side Markdown rendering via `pulldown-cmark` in WASM provides project frontpages with live browser editing.

---

## 2026-07-28 — The projects layer must be reachable without a server

**Decision.** Every project operation exists offline: `Experiment(project=...)`,
`Experiment.set_project()`, `expman.assign_project()`, and `exp project
ls/new/show/assign/unassign/rm/sync`. All of them write only the `project:`
field of `experiment.yaml`.

**Why.** The layer was HTTP-only, so a compute node running a SLURM batch job —
the case it exists for — could not use it. A feature reachable only from the
dashboard is not usable by the runs it is meant to organise.

**Consequences.** `experiment.yaml` is still written only when absent, but an
explicit `project=` on a later run now updates that one field rather than being
silently ignored. Nothing else in the file is touched, so display name,
description and tags survive.

---

## 2026-07-28 — A generated project is read-only, and says so

**Decision.** `exp project sync` projects a YAML manifest into the store and
marks the result `generated: true`, with `generated_from` naming the authority.
The HTTP API refuses `PATCH`/`PUT`/`DELETE` on such a project with `409
generated_project`; the dashboard hides its edit affordances and shows a banner;
the README carries an `<!-- expman:generated ... -->` first line.

**Why.** An expman project can be a one-way projection of a source that lives
elsewhere (a thesis repo's `studies.yaml`), carrying only general information and
experiment membership. Each sync overwrites it wholesale.

**Consequences.** Accepting a dashboard edit would report success and then lose
the work at the next sync, with no trace. Refusing is the honest failure. Sync
also *reconciles* membership: experiments dropped from the manifest are
unassigned, so the projection cannot drift into a superset of its source.

---

## 2026-07-28 — Liveness is a heartbeat, not an age

**Decision.** The engine refreshes `run.yaml.heartbeat_at` every
`heartbeat_interval_secs` (default 30s). `exp reap --older-than` marks runs
`CRASHED` whose heartbeat has gone stale. `/stats` splits `active_runs` from
`stale_runs`.

**Why.** A hard kill leaves `status: RUNNING` forever, so `active_runs`
overcounted silently. Reaping on `started_at` alone would have been simpler but
would kill legitimately long multi-day jobs — unacceptable for thesis runs.

**Consequences.** Runs written before heartbeats existed have `heartbeat_at:
null` and fall back to `started_at`, which only ever reaps them *later*, never
sooner. Reaping is a dry run without `--force`, and sets `finished_at` to the
last heartbeat rather than to the moment of reaping, so a run reaped a week late
does not report a week-long duration. Reaping stays a deliberate command: the
server never mutates `run.yaml` on a read path.

---

## 2026-07-28 — The read API returns builtins; pandas stays optional

**Decision.** `load_runs`, `read_metrics`, `load_config`, `load_run` and
`load_projects` return plain dicts and lists. `to_pandas=True` imports pandas
lazily, at call time.

**Why.** Without a read API, analysis scripts read Parquet and a side-car JSON
manifest directly, which left the manifest — not expman — as the durable record.
Two sources of truth would otherwise be permanent. Making pandas a hard
dependency of the wheel would push that cost onto every consumer, including
cluster nodes where the install is a constraint.

**Consequences.** `load_runs()` rows carry a `path` that feeds straight into
`read_metrics()`, so callers never do path arithmetic. Tag queries accept both a
list (conjunction) and an expression string with `AND`/`OR`, shared with the CLI
and the HTTP API through `storage::parse_tag_expr` — one grammar, three faces.

---

## 2026-07-28 — Type is self-hosted and assigned by role

**Decision.** Three roles: Space Grotesk (display), Nunito (running text),
Cascadia Code (labels, run IDs, data). Latin subsets are vendored from Fontsource
into `assets/fonts/` and embedded in the binary; the Google Fonts CDN link is
gone.

**Why.** The dashboard is distributed as a self-contained binary, so a CDN font
made it silently wrong offline and on air-gapped cluster nodes. The third role is
wider than "code": anything that is a *value* rather than a sentence — a run ID,
a count, a metric, a timestamp, an uppercase eyebrow label — is set mono so
tabular things align and scan as data.

**Consequences.** ~273 KB of woff2 in the binary. `frontend.rs` must include
`*.woff2` in its `rust_embed` filter or the faces 404 and fall back silently.

---

## 2026-07-28 — Tailwind is built, not fetched from a CDN

**Decision.** `data-trunk rel="tailwind-css"` builds `assets/tailwind.css` with
the standalone Tailwind CLI, pinned to 3.4.17 in `Trunk.toml` `[tools]` and
provided as `pkgs.tailwindcss_3` for the offline Nix build. The
`cdn.tailwindcss.com` script is gone.

**Why.** It was the last third-party request. A dashboard shipped as a
self-contained binary that renders unstyled without internet is not
self-contained. Verified in a browser: zero external resource loads.

**Consequences.** Two upsides that were not the goal. The stylesheet is
tree-shaken from the `.rs` sources (68 KB, 11 KB gzipped) instead of the CDN's
full runtime. And `prose` now works — the dashboard renders project READMEs with
typography classes that the plain Play CDN never shipped, so they had been
silently inert. The standalone CLI bundles `@tailwindcss/typography`, so this
needs no npm.

Two costs. Tailwind only sees classes it can find in the source, so a class
assembled at runtime (`format!("text-{}", colour)`) will not be generated —
build them from a match returning whole literals. And the pin must stay equal to
`pkgs.tailwindcss_3.version`; both were verified to emit byte-identical CSS.

---

## 2026-07-28 — Cargo.toml is the only place the version lives

**Decision.** `wrappers/python/pyproject.toml` uses `dynamic = ["version"]` and
`flake.nix` reads `builtins.fromTOML ./Cargo.toml`. `just bump` edits one file.
`just check-versions` asserts no literal has crept back and runs in CI.

**Why.** The version was duplicated across four files, aligned only by `just
bump`, with nothing validating it. Since `expman.__version__` comes from
`CARGO_PKG_VERSION` rather than `pyproject.toml`, a drift would have shipped a
wheel whose metadata disagreed with the module it contained.

**Consequences.** Verified by bumping Cargo.toml to 9.9.9: the wheel built as
`expman_rs-9.9.9` and `nix eval .#expman.version` returned 9.9.9, both without
touching another file. `check-versions` is deliberately POSIX-only so it needs no
extra tooling in CI.

---

## 2026-07-28 — Releases block on tests

**Decision.** `publish.yml` calls `rust.yml` and `python.yml` itself, and both
publish jobs `need` them.

**Why.** `ci.yml` fires on the same push but is a separate workflow, so it could
not gate anything. `cargo publish` and `uv publish` ran in parallel with the test
suite: a red build published anyway, and no job failed to say so.

**Consequences.** A release now waits for the suite, and a flaky test blocks a
release rather than merely reporting. `ci.yml` still runs on the same commit, so
a release commit runs the suite twice in parallel — accepted as the cost of
keeping both entry points independently meaningful.

---

## 2026-07-28 — `.maturinignore` was dead; deleted

**Decision.** Removed the root `.maturinignore`.

**Why.** Its paths lacked the `wrappers/` prefix, and maturin resolves the file
beside `pyproject.toml` anyway. Proven rather than assumed: wheels built with and
without it have byte-identical file lists, both containing `expman/bin/exp`.

**Consequences.** None to the wheel. `include = ["expman/bin/exp*"]` in
`pyproject.toml` is the actual mechanism, together with `expman/bin/` being
untracked but **not** gitignored — see the note in STATUS.md, which is the part
that genuinely matters.

---

## 2026-07-28 — One set of wire types for server and frontend

**Decision.** `core/dto.rs` holds the HTTP response shapes and is compiled for
both native and `wasm32`. Handlers construct and serialize those types;
`app/models.rs` re-exports them instead of redeclaring them.

**Why.** The frontend's models were a hand-maintained mirror of
`serde_json::json!` literals in the handlers. Adding a field meant remembering to
add it in two places, and it drifted again during this session's work.

**Consequences.** Three latent bugs surfaced immediately, because typing the
contract made them compile errors rather than silent mismatches:

- `runs_table.rs` matched run status against `"COMPLETED"`, which this API never
  emits (it is `FINISHED`), so every finished run rendered in the grey
  unknown-status colour instead of green.
- `format_date` parsed an RFC 3339 string and fell back to printing the raw
  string on failure. It now takes a `DateTime<Utc>`, so there is no parse and no
  fallback.
- The Jupyter backend was a stringly-typed comparison against `"jupyter"` on one
  side of an enum that serialized lowercase on the other.

Wire types stay separate from storage models on purpose: `dto::Run` may expose
less than `RunMetadata`, but it does so through a constructor, so removing a
storage field is a build error rather than a quietly changed endpoint.

---

## 2026-07-29 — Metrics are append-only while a run is live

**Decision.** Each flush appends an Arrow IPC batch to a segment file; segments
are folded into `vectors.parquet` when the run closes. A metric first logged
mid-run rolls a new segment, since one IPC stream carries one schema.

**Why.** Every flush previously rewrote the whole Parquet, so total write volume
grew with the *square* of the step count. Measured: 10k steps took **48.2s**
before and **0.39s** after — the same test, 124× less time, nearly all of it in
flush and close.

**Consequences.** Readers must union segments with the Parquet, so
`read_run_vectors` replaces "open vectors.parquet"; a reader that skipped this
would show nothing for a live run. Rows sharing a step are merged at read time,
preserving the old `log_vector(step=1)`-twice semantics. Crash behaviour
*improves*: a truncated segment yields every batch fully written, and compaction
writes the Parquet before deleting segments, so an interruption leaves both and
readers union them to the same result. `append_vectors` survives for one-shot
writes such as `exp import`.

---

## 2026-07-29 — `run.yaml` is mutated only under a lock

**Decision.** All run-metadata mutation goes through
`storage::update_run_metadata`, which takes an exclusive advisory lock.
`save_yaml` writes atomically via temp-file-and-rename.

**Why.** DDP puts N ranks in one run directory, each ticking its own metadata
update. Load-mutate-save races, and the loser's tags, scalars or final status
vanish. Separately, `fs::write` truncates before writing, so a dashboard polling
during the engine's 500ms write could read an empty file and report the run
CRASHED.

**Consequences.** Advisory locks only bind processes that take them, which is
exactly the contention here. A regression test runs 8 threads × 10 updates and
asserts all 8 tags survive.

---

## 2026-07-29 — System metrics come from vendor CLIs, not linked SDKs

**Decision.** Sampling shells out to `nvidia-smi`, `rocm-smi` and `tpu-info`,
plus `/proc` for CPU and memory. Probes are declarative (`ProbeSpec`) and a user
can add one in config without a rebuild.

**Why.** Chosen over NVML/ROCm bindings: no build-time vendor SDK, no runtime
`dlopen` that fails differently on every cluster image, and hardware we have
never seen is a config entry rather than a patch. The cost — a subprocess every
15s — is irrelevant next to a training step.

**Consequences.** Exactly two parsers. `nvidia-smi` is read back from a column
list *we* specified, so parsing is not guessing. Everything else is JSON,
flattened by taking **every numeric leaf** into a dotted key: schema-agnostic, so
it works for rocm-smi, for tpu-info, and for a user's own tool, and does not
break when a vendor adds a field.

**Verified on real hardware, 2026-07-29**, and the TPU guess was wrong. `tpu-info`
has no `--json` at all; it prints Rich pipe tables, one per `--metric`. That
added a third parser, `PipeTable`, which reads the first column as the device and
the remaining headers as metric names — and handles several tables in one pass,
which is what `tpu-info` emits for repeated `--metric` flags.

Two details only real output would have shown: the tables use ASCII `|` while the
surrounding Rich panels use box-drawing characters, so the panels are skipped
without needing to be recognised; and `hbm_usage`/`duty_cycle_percent` read `N/A`
until a framework opens the TPU, so they are skipped rather than recorded as
zero. Recording 0 GiB of HBM during a live job would be a lie.

NVIDIA needed no change: the probe worked first time on a 2× RTX A6000 host, all
six metrics per GPU.

---

## 2026-07-29 — A DDP job is one row, expandable

**Decision.** `group` + `rank` on every run, auto-detected from `RANK` /
`SLURM_PROCID` / `EXPMAN_RANK` and the scheduler's job id. The dashboard shows
rank 0 with a `▸ N ranks` badge; clicking expands the ranks.

**Why.** A 4-GPU job is one thing you ran, not four rows of near-identical
metrics — and a 100-trial sweep on 8 GPUs would otherwise be an 800-row table.
Auto-detection matters more than the flag: a DDP script needs *no*
expman-specific change, and ranks get distinct run names so they cannot
overwrite each other.

**Consequences.** `is_primary` is exposed and returns True on rank 0 **or** a
non-distributed run, because `rank == 0` is False when there is no rank — the
obvious guard is the wrong one.

---

## 2026-07-29 — Provenance is captured; the diff is opt-in

**Decision.** Git commit/branch/dirty, command line, hostname and scheduler job
ids are captured by default into `provenance.yaml`. The working-tree diff only
with `capture_diff=True`.

**Why.** The cheap fields cannot leak anything not already in the repository. A
diff can: an edited `.env`, a pasted key, an unpublished data path. Knowing a run
was dirty is cheap and safe; knowing exactly how is the user's call.

**Consequences.** `Provenance::is_reproducible()` is explicit that a dirty run
without a diff cannot be reconstructed. Scheduler capture takes an allow-list of
identifying variables, never a dump of the environment.

---

## 2026-07-29 — A sweep is a group of runs, with two backends

**Decision.** `exp sweep` expands a YAML config into trials that are ordinary
runs sharing a group and tagged with their parameters. `run` executes locally
with a concurrency cap; `slurm` emits an sbatch array.

**Why.** Reusing groups means no new storage concept and no server. Both backends
because a laptop and a cluster are different problems — and on a cluster the
scheduler is the only thing allowed to place work, so emitting sbatch and
stopping beats writing an agent that fights it.

**Consequences.** Params reach a trial *both* as `{placeholders}` in the command
and as `EXPMAN_PARAM_*`, so neither an argparse script nor a shell wrapper needs
editing. Random search uses a seeded SplitMix64 rather than `rand`, so a sweep
re-expanded next year yields the same trials — something a dependency bump
cannot promise. Grid refuses `min`/`max` instead of silently sampling. Tagging
trials with their params is what makes the Compare tab work at all.

---

## 2026-07-29 — Media and histograms are stored, and the silent stubs are gone

**Decision.** `log_image`, `log_figure`, `log_audio`, `log_video` and
`log_histogram` store natively. The TensorBoard `SummaryWriter` shim is backed by
them, and the four methods that genuinely cannot be stored warn once each.

**Why.** The shim's `add_image`/`add_histogram`/`add_figure` were **silent
no-ops**. Swapping `torch.utils.tensorboard.SummaryWriter` for expman's threw
away every image and histogram with nothing said at runtime — the worst kind of
failure, because the code looks like it works.

**Consequences.** Images go to `media/` with a JSONL manifest rather than into
Parquet: a column of image blobs would ruin the metrics file for its real
purpose, and the dashboard needs a URL anyway. Histograms are one row per (tag,
step) with JSON edges/counts, because bin counts vary and a column-per-bin schema
would churn on every call.

A deliberate asymmetry: the **native** API raises on input it cannot encode,
while the **compat layer warns**. New code deserves a hard error; code written
for TensorBoard and running for three days does not deserve to die over one
unencodable image.

---

## 2026-07-29 — `exp ... | head` must not panic

**Decision.** Restore the default SIGPIPE handler in `main`.

**Why.** Rust ignores SIGPIPE, so a closed downstream pipe becomes an `EPIPE`
write error and `println!` panics on it. `exp sweep preview | head` printed its
output and then a panic with a backtrace.

**Consequences.** One unix-only `libc` dependency, used for one call.

---

## 2026-07-29 — The perf test runs alone

**Decision.** `.config/nextest.toml` gives `test_log_vector_is_fast`
`threads-required = "num-test-threads"`, so nextest runs it with the machine to
itself.

**Why.** It asserts a wall-clock budget — 10,000 `log_vector` calls under 100ms.
Run in parallel with 70+ other tests, several of which shell out to `git`, it
measures contention rather than the hot path. It failed spuriously twice in one
session and passed immediately when re-run alone, which is exactly the shape of a
test that erodes trust in the suite.

**Consequences.** The suite takes marginally longer. The number the test prints
is now the number the assertion is about.

---

## 2026-07-29 — libtpu direct: investigated, rejected

**Decision.** Keep shelling out to `tpu-info`. Do not write a native gRPC client
against libtpu.

**Why.** `tpu_info` reaches libtpu over gRPC at `localhost:8431`
(`RuntimeMetricServiceStub`, local channel credentials), so a native client is
technically possible. Measured on the v6e host:

1. **`ss -ltn` shows nothing on 8431 while the TPU is idle.** libtpu starts that
   server *inside* the training process. A direct client would get
   connection-refused exactly as often as `tpu-info` returns `N/A` — so it fixes
   nothing about the availability gap, which is the only real limitation.
2. **No `.proto` source ships**, only compiled `_pb2.py`. tonic would mean
   vendoring a reconstructed, Google-internal, unversioned schema that a libtpu
   update can break silently.
3. `grpc.local_channel_credentials()` has no tonic equivalent.
4. The saving is ~391ms per sample (of which ~100ms is Python import) against a
   15s interval — **~2.6% of one core**, on a thread that never touches the
   training loop.

**Consequences.** Probes now sample **concurrently**, so a host with several
accelerators pays the slowest probe rather than their sum. Revisit only if
sub-second sampling is ever wanted — for which a profiler, not a sampler, is the
right tool.

---

## 2026-07-29 — Downsample server-side, preserving spikes

**Decision.** `GET /metrics` returns at most 2000 points, chosen by
Largest-Triangle-Three-Buckets. `?full=1` returns everything; `?max_points=N`
sets the cap.

**Why.** A long run has far more steps than a chart has pixels, and serialising
a million rows to JSON hangs the tab before it draws.

**Consequences.** LTTB rather than a stride, because stride sampling drops the
single-row loss spike or divergence — precisely what someone opens the chart to
find. First and last rows are always kept so endpoints are exact. A regression
test plants two spikes in 5000 rows and asserts both survive a reduction to 200.

---

## 2026-07-29 — A memo, not a SQLite index

**Decision.** `load_run_metadata_cached` memoises parsed `run.yaml` keyed on
`(mtime, len)`. No index, no new dependency.

**Why.** The earlier recommendation here was a SQLite index. Profiling changed
it: of 149ms to query 800 runs, **136ms was YAML parsing** and 3ms was `stat`.
The problem is not lookup, it is re-parsing unchanged files inside one
long-lived process. Measured after: **167ms cold → 9.6ms warm.**

An index would have been a second copy of the truth that can go stale, needs a
migration story, and pulls in a C dependency — to solve a problem a memo solves
transparently. A cold process simply parses as before.

**Consequences.** The memo is *not* used by `update_run_metadata`: a
read-modify-write must see the current file, and mtime granularity is too coarse
to trust at the engine's 500ms cadence. A test rewrites `run.yaml` to a
same-length, different-status document and asserts the change is seen.

---

## 2026-07-29 — `build.rs` degrades instead of failing

**Decision.** A missing or failing `trunk` now emits a `cargo:warning` naming the
cause and writes a placeholder `dist/index.html`. The build succeeds.

**Why.** `exit(1)` meant `cargo build --all-features` on a clean checkout died at
build-script time with a message that never said "install trunk" — and every
workaround in the repo descended from that branch. A broken *frontend* is not a
reason to be unable to build the *CLI*.

**Consequences.** Verified by building `--features cli,server` with `trunk` off
PATH and no `dist/`: it warns, succeeds, and the server answers `/api/stats`
normally while serving a placeholder page that says `just build-frontend`. The
`CARGO_DOC` special case is gone — the general path covers it, confirmed by
`just build-docs`.

The hazard this creates is the opposite one: a release built without `trunk`
would silently publish the placeholder. `publish-cargo.yml` therefore now
*verifies* that `dist/` holds a real bundle — a `.wasm` present and the
placeholder marker absent — and fails the publish otherwise. Trading a
build-time hard failure for a release-time one is the right way round: the first
blocked every contributor, the second blocks only a genuinely broken release.

---

## 2026-07-29 — Toolchain moved to 1.97; wasm-bindgen-cli pinned

**Decision.** `nix flake update` (fenix and nixpkgs, Feb → Jul 2026), plus
`flake.nix` now takes `pkgs.wasm-bindgen-cli_0_2_108` rather than
`pkgs.wasm-bindgen-cli`.

**Why the update.** Local clippy was 1.93 while CI ran 1.97, and 1.97's
`unnecessary_sort_by` failed the 1.1.0 release while everything was green
locally. Local and CI now both report `clippy 0.1.97 (8bab26f4f6 2026-07-14)`.

**Why the pin.** The same update moved nixpkgs' `wasm-bindgen-cli` from 0.2.108
to 0.2.121 while the `wasm-bindgen` *crate* stayed at 0.2.108 — `js-sys 0.3.85`
pins it exactly, so the crate cannot move alone, and the family can only reach
0.2.126, still not 0.2.121. That mismatch breaks `nix build .#expman`, which
runs `TRUNK_OFFLINE=true` and therefore uses the Nix-provided binary rather than
letting trunk fetch the matching one.

nixpkgs publishes versioned attributes for exactly this, so the fix is one word.
Pinning couples `flake.nix` to `Cargo.lock` rather than to nixpkgs' packaging
cadence, which is the right direction: the crate is ours to choose, nixpkgs'
default is not.

**Consequences.** Changing the `wasm-bindgen` crate now requires changing the
attribute in step; the check is two greps and is written down in the
`expman-build` skill. Caught before it reached CI — which is the argument for
doing a flake update as its own change rather than inside a release.

---

## 2026-07-29 — The release check no longer interpolates the commit message

**Decision.** check-release.yml passes the commit message through `env:` instead
of interpolating it into the script body.

**Why.** It was written as a bare interpolation inside a run block, which makes
the message part of the *script text*. A message containing backticks or a
dollar-paren therefore executes on the runner. That is script injection, and it
is reachable by anyone who can land a commit on main.

It fired for real: a commit body of mine mentioning a nix build command in
backticks was executed, and the step died with "nix: command not found". Benign
only because nix is not installed on the runner — a message invoking curl would
not have been.

**Consequences.** The whole release chain skipped, so nothing mispublished, but
the Publish workflow went red on a commit that was never meant to release. The
fixed version was verified against the injection payloads, the real release
subject, and the message that broke it. It also confirms that a release commit
must have **no body**: the anchored regex is matched against the entire message,
so a multi-line message never matches.

It is the only such interpolation in the eleven workflows; the rest were checked.

---

## Open questions

*The four questions that stood here on 2026-07-27 — version single-sourcing,
gating releases on tests, `.maturinignore`, and the Tailwind CDN — were all
resolved on 2026-07-28; see the entries above.*

- **`github-release.yml` is still invoked twice** for the same tag, from
  `publish.yml` and `nix.yml`, so the final asset set remains race-dependent.
  Untouched by the release gating work.
- **A release commit now runs the test suite twice**, once via `ci.yml` and once
  as the publish gate. Deduplicating means making `ci.yml` skip release commits,
  which is easy to get subtly wrong; left alone deliberately.
- **TPU `hbm_usage`/`duty_cycle_percent` have not been seen with a live value.**
  The format is verified and the numeric path is exercised by
  `tensorcore_utilization` on the same host, but the TPU was idle, so those two
  columns were only ever observed as `N/A`. If a live reading ever carries a unit
  suffix, `parse_leading_number` already strips it.
- **Sweeps have no Bayesian/early-stopping search.** Grid and random only;
  successive halving would be the next addition and needs a live-metrics read
  from the agent, which the append-only segments now make cheap.
