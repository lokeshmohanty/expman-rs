# STATUS — volatile state

*Update in place; keep short; absolute dates. History lives in git log.*

## Current focus (2026-08-19)

**1.3.0 — the Jupyter integration became configurable.** Three features, driven by
a JAX RL lab whose training runs on headless hosts: the rollout GIF fails to
render there, so the run has a checkpoint and no video, and the Interactive tab
had to be able to render it.

- `exp serve --notebook-template <PATH>` — the generated `.ipynb` comes from a
  project template. Discovery: flag → `<DIR>/.expman/notebook.ipynb` → built-in.
  `{{run_dir}}` `{{run_name}}` `{{experiment}}` `{{store}}` `{{project}}`,
  **JSON-escaped** (they sit inside JSON string literals). An unparsable result
  logs the template path and falls back rather than writing a broken notebook.
- `exp serve --jupyter-command <CMD>` — a command *line* (`shlex`-split, no
  shell), default `jupyter`. Set it to `uv run --extra nb jupyter` and the
  kernel *is* the project's venv, so no `ipykernel install` and no kernelspec.
  Deliberately writes nothing to `~/.local/share/jupyter`.
- **Staleness** — every generated notebook carries
  `metadata.expman.{template_hash, content_hash}`. Unedited + template moved on →
  rewritten. Edited, or no metadata → left alone, warned about. Previously an
  existing notebook was kept forever, so a template fix never reached old runs.

Gate green: `just ci` exit 0 — fmt, clippy wasm32 + native at `-D warnings`,
**96/96** Rust (23 of them new, in `jupyter_service`), **71/71** Python, ruff,
`check-versions`. Also verified against a live server: template rendering with
real absolute paths, all five staleness paths over the HTTP API, and the three
`--jupyter-command` error paths.

**Bug found on the way, fixed here:** the multi-run generator spliced run names
into JSON string literals unescaped, so a run directory named with a `"` or `\`
had always produced an unopenable `.ipynb`. Silent and pre-existing; surfaced
only because the new code parses what it generates.

**Multi-run notebooks stay on the built-in default** — no single run, so three of
the five placeholders have nothing to bind to. Recorded as an open question in
`docs/content/decisions.md`, not left to be discovered.

**Semver caveat:** user-facing surfaces (CLI, HTTP, Python) are all backward
compatible, which is why this is a minor. The **Rust library surface is not**:
`cli::cmd_serve` now takes a `ServerConfig`, and
`api::jupyter_service::{generate_notebook, detect_backend, JupyterManager::spawn}`
changed signature. The crate has no written stability policy for `pub` items
inside the feature-gated `api`/`cli` modules; if one is ever wanted, that is the
decision to make.

---

## Previous focus (2026-07-29)

**Done, uncommitted:** the thesis-suite TODO (P0+P1+P2), the typography
standard, every open question from `docs/decisions.md`, and **Tier 1 + Tier 2**
of the "replace W&B/MLflow/TensorBoard" roadmap.

Gate is green: fmt, `check-versions`, clippy native + wasm32 at `-D warnings`,
**71/71** Rust tests, **51/51** Python tests, ruff, `nix build .#expman`, plus
browser verification of every dashboard page.

### Tier 1 + 2 (2026-07-29)

**The write path was rebuilt first**, because everything else piles data onto it.
Metrics are now append-only Arrow IPC segments compacted to Parquet at close,
instead of rewriting the whole Parquet per flush.

> Measured: 10k steps went from **48.2s → 0.39s**. Per-run cost is now flat at
> ~11.6 ms/1k steps and doubles exactly with step count (was quadratic).

`run.yaml` is mutated only under an advisory lock, and written atomically — DDP
ranks were racing, and a dashboard poll could catch a truncated file and report
a live run as CRASHED.

**Tier 1**
- **System metrics** — subprocess probes (`nvidia-smi`, `rocm-smi`, `tpu-info`,
  `/proc`) into `system.parquet`, sampled every 15s. Absent binaries are skipped
  silently. User-extensible via config, no rebuild.
- **Grouped runs** — `group`/`rank` auto-detected from the launcher, so a DDP
  script needs no changes. Dashboard rolls a job into one row with `▸ N ranks`.
- **Provenance** — git commit/branch/dirty, command, hostname, scheduler ids.
  The diff is opt-in; a dirty tree can carry secrets.

**Tier 2**
- **Sweeps** — `exp sweep preview|run|slurm|status`, grid + seeded random,
  trials as a group, params delivered *both* as `{placeholders}` and
  `EXPMAN_PARAM_*`. The emitted sbatch array was verified by executing it.
- **Media + histograms** — `log_image/figure/audio/video/histogram`, stored in
  `media/` + `histograms.parquet`.
- **Compare tab** — params × final-metrics table with sorting, plus an SVG
  scatter; defaults to axes that actually have data.

### The bug this run fixed

`SummaryWriter.add_image` / `add_histogram` / `add_figure` were **silent
no-ops**. Anyone swapping `torch.utils.tensorboard.SummaryWriter` for expman's
lost every image and histogram with nothing said at runtime. They now store
natively; the four genuinely unsupported methods warn once each.

Typing the wire contract also surfaced that the status-colour match tested for
`"COMPLETED"`, which the API never emits — so every finished run had been
rendering grey instead of green.

### Also done: hardware-verified probes and the three follow-ups (2026-07-29)

Probes tested on **real hardware** (`archimedes`: 2× RTX A6000; `tpu-node-1`: TPU
v6e, 8 chips):

- **NVIDIA worked first time**, unchanged — 6 metrics per GPU.
- **The TPU guess was wrong.** `tpu-info` has no `--json`; it prints Rich pipe
  tables. Added a `PipeTable` parser and the real `--metric` flags, with both
  hosts' verbatim output pinned as test fixtures.
- **libtpu-direct investigated and rejected**: port 8431 is not listening while
  the TPU is idle, so a native gRPC client has *identical* availability and fixes
  nothing; no `.proto` ships; saving is ~2.6% of one core. Probes now sample
  concurrently instead.
- Added **`exp probes`** — lists available probes and takes a live sample.

Then the three recommendations:

- **Downsampling** — `/metrics` caps at 2000 points via LTTB, `?full=1` to opt
  out. LTTB not stride, so a one-row loss spike survives; tested.
- **Metadata memo, not SQLite.** Profiling redirected this: 136ms of the 149ms
  to query 800 runs was YAML parsing, 3ms was `stat`. An `(mtime, len)` memo gave
  **167ms → 9.6ms warm** with no new dependency, no index to go stale.
- **`build.rs` degrades** instead of `exit(1)` — verified by building with
  `trunk` off PATH: warns, succeeds, API works, placeholder page explains itself.
  `CARGO_DOC` special case removed. `publish-cargo.yml` now refuses to publish a
  placeholder, closing the hazard this created.

Also fixed a **flaky perf test**: it asserted wall-clock timing while running
beside 70+ other tests, so it measured contention. It now runs alone
(`.config/nextest.toml`). Stable over repeated runs.

### Next actions

- *(1.2.0, done)* bumped and pushed.
- The thesis repo can drop its `experiment.yaml` patching and pyyaml workaround,
  and read through `expman.load_runs()`.
- **The thesis repo can now ship `<store>/.expman/notebook.ipynb`** with cells
  that load `{{run_dir}}`'s checkpoint and render the rollout, and start the
  dashboard with `--jupyter-command 'uv run --project <repo>/experiments --extra
  nb jupyter'`. Untested against the real lab — that is the next verification.

## Known gaps / open questions

- **Nothing tests that a Jupyter kernel can actually import a project's package.**
  The 23 `jupyter_service` unit tests cover template discovery, substitution,
  escaping, staleness and command splitting; the only spawn path they exercise is
  the failure one. The claim that `--jupyter-command 'uv run … jupyter'` yields an
  importable environment was reasoned from how Jupyter picks its kernel and is
  **not** verified against a real project. Verify in the browser before relying on
  it.
- **`uv run` resolves its project from cwd, and expman sets cwd to the run
  directory.** So `--jupyter-command 'uv run …'` only works when the store lives
  inside the project; otherwise `--project <dir>` must be passed. Documented in
  `reference/cli.md`, but it is the most likely thing to trip a first user.
- **Executing a cell pins a notebook forever.** `content_hash` is sensitive to
  outputs and `execution_count`, so a run whose notebook has been used will never
  pick up a template fix. Intended (outputs are the user's), but the only escape
  is deleting the file, and the dashboard offers no button for that.
- **`build.rs` still `exit(1)`s** rather than degrading. It is the root of the
  `dist/` + `--allow-dirty` knot, and now of the Tailwind tool dependency: no
  network *and* no `tailwindcss` gives a hard failure whose message does not say
  so. The clean fix is in `expman-release/memory/dist-and-allow-dirty.md`.
- **TPU `hbm_usage`/`duty_cycle_percent` never seen with a live value** — the
  format is verified and the numeric path is exercised by
  `tensorcore_utilization` on the same host, but the TPU was idle so those two
  columns only ever read `N/A`.
- **`exp reap` is manual by design.** The server never mutates `run.yaml` on a
  read path, so a store nobody reaps keeps stale runs — visible as `stale_runs`
  in `/stats` rather than hidden inside `active_runs`.
- **A release commit runs the test suite twice**, once via `ci.yml` and once as
  the publish gate. Deduplicating means teaching `ci.yml` to skip release commits,
  which is easy to get subtly wrong.
- **`github-release.yml` is still invoked twice** for one tag (`publish.yml` and
  `nix.yml`), so the final asset set stays race-dependent.
- A **new asset must be `git add`ed before `nix build` sees it** — a flake's
  `src = ./.` reads only tracked files, and the error names a file that plainly
  exists. Cost me a build this session; documented in `how-to/build-and-run.md`.

## Open obligations / blockers

- **`wrappers/python/expman/bin/` is untracked and must stay that way.** It is
  deliberately NOT in `.gitignore` (maturin honours `.gitignore` and would drop
  the binary from the wheel). Run once per clone:
  `echo "wrappers/python/expman/bin/" >> .git/info/exclude`
- `test_artifacts/`, `test_experiments/`, `test_results/` are untracked debris in
  the working tree — decide whether to ignore or delete before committing.
