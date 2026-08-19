+++
title = "HTTP API Reference"
description = "Complete route specification for expman-rs axum HTTP server."
weight = 2
+++

# Reference — HTTP API

*Hand-written from `src/api/*.rs`, 2026-07-27. Authoritative over
`src/api/README.md`, which is stale (it documents a nonexistent `/api/events`
endpoint and mentions `utoipa`).*

Router: `src/api/mod.rs:42-140`. All routes below are prefixed with `/api`
(`mod.rs:153`). Anything else falls through to the embedded frontend
(`mod.rs:155`). CORS is fully permissive — `Any` origin/method/header
(`mod.rs:146-149`).

Rows marked **NEW** are from the uncommitted TensorBoard work.

## Projects

| Method | Path | Handler | Response |
|---|---|---|---|
| GET | `/projects` | `projects.rs` | `[{id, display_name, description, tags, experiments_count, created_at, generated, generated_from, generated_at}]` |
| POST | `/projects` | `projects.rs:53` | `ProjectMetadata` |
| GET | `/projects/{project}` | `projects.rs` | `{id, display_name, description, tags, created_at, generated, generated_from, generated_at, readme, experiments: [...]}` |
| PATCH | `/projects/{project}` | `projects.rs:151` | updated `ProjectMetadata`. Body: `{display_name?, description?, tags?}` |
| DELETE | `/projects/{project}` | `projects.rs:178` | `204 No Content` (unassigns experiments) |
| GET | `/projects/{project}/runs` | `projects.rs` | cross-experiment runs table with facets — see below |
| GET | `/projects/{project}/readme` | `projects.rs` | `{content}` |
| PUT | `/projects/{project}/readme` | `projects.rs:133` | `{content}`. Body: `{content}` |

## Experiments

| Method | Path | Handler | Response |
|---|---|---|---|
| GET | `/experiments` | `experiments.rs:17` | `[{id, display_name, description, tags, project, runs_count}]` (ad-hoc `json!`) |
| GET | `/experiments/{exp}/metadata` | `experiments.rs:39` | `ExperimentMetadata` |
| PATCH | `/experiments/{exp}/metadata` | `experiments.rs:60` | merged `ExperimentMetadata`. Body: `{display_name?, description?, tags?, project?}` |
| GET | `/experiments/{exp}/stats` | `stats.rs:16` | `[{run, status, started_at, finished_at, duration_secs, last_metrics}]` |

## Runs

| Method | Path | Handler | Notes |
|---|---|---|---|
| GET | `/experiments/{exp}/runs` | `runs.rs:26` | query `?metrics=a,b,c` filters columns. Backfills `vectors` from `vectors.parquet` when empty (`runs.rs:56-68`) |
| GET | `/experiments/{exp}/runs/{run}/metadata` | `runs.rs:88` | `RunMetadata` with latest scalars merged into `vectors` |
| PATCH | `/experiments/{exp}/runs/{run}/metadata` | `runs.rs:118` | Body: `{name?, description?, tags?}` |
| GET | `/experiments/{exp}/runs/{run}/config` | `metrics.rs:118` | `config.yaml` as JSON |

## Metrics and streams

| Method | Path | Handler | Notes |
|---|---|---|---|
| GET | `/experiments/{exp}/runs/{run}/metrics` | `metrics.rs:28` | query `?since_step=N`. Returns `Vec<HashMap<String, Value>>` |
| **GET (SSE)** | `/run/{exp}/{run}/stream/vectors` | `metrics.rs:41` | every 500 ms re-reads `vectors.parquet` past the last seen `step`, emits a JSON array of new rows. 15 s keep-alive. |
| **GET (SSE)** | `/experiments/{exp}/runs/{run}/log/stream` | `metrics.rs:78` | query `?file=` (default `run.log`). Tails by byte offset; resets on truncation (`:99`). 15 s keep-alive. |

Both streams terminate via `.take_until(shutdown.cancelled())`.

Note the vectors-SSE route is the **one route that breaks the naming
convention** — `/run/{exp}/{run}/...` rather than `/experiments/...`
(`mod.rs:56`). It is also currently unused by the frontend.

## Artifacts

| Method | Path | Handler | Notes |
|---|---|---|---|
| GET | `/experiments/{exp}/runs/{run}/artifacts` | `artifacts.rs:18` | `Vec<ArtifactInfo>` |
| GET | `/experiments/{exp}/runs/{run}/artifacts/content` | `artifacts.rs:34` | query `?path=`. Raw bytes with a guessed content-type; `.parquet` returns `{type:"parquet", data:[first 100 rows]}` (`:69-73`). Path traversal is guarded by canonicalize + `starts_with` (`:47-58`). |

## Global

| Method | Path | Handler | Response |
|---|---|---|---|
| GET | `/stats` | `stats.rs` | `{total_experiments, total_projects, total_runs, active_runs, stale_runs, total_storage_bytes}` |
| GET | `/config` | `stats.rs` | `{live_mode, read_only, version}` — both read from `AppState`, so they reflect the flags `exp serve` was actually given |

## Jupyter

Per-run, plus a `__multi__` variant scoped to the experiment.

| Method | Path | Handler |
|---|---|---|
| GET | `/jupyter/available` | `jupyter_handlers.rs` `available_jupyter` → `{backend: "jupyter"\|"python"\|"none"}` |
| POST | `/experiments/{exp}/runs/{run}/jupyter/start` | `start_jupyter` → `{port}` |
| POST | `/experiments/{exp}/runs/{run}/jupyter/stop` | `stop_jupyter` |
| GET | `/experiments/{exp}/runs/{run}/jupyter/status` | `status_jupyter` → `{running, port}` |
| GET | `/experiments/{exp}/runs/{run}/jupyter/notebook` | `get_jupyter_notebook` → `{exists, content}` |
| POST | `/experiments/{exp}/runs/{run}/jupyter/notebook` | `create_jupyter_notebook` → `{created, content}` or 409 |
| POST | `/experiments/{exp}/jupyter/start` | `start_multi_jupyter`, body `{runs: [String]}` |
| POST | `/experiments/{exp}/jupyter/stop` | `stop_multi_jupyter` |
| GET | `/experiments/{exp}/jupyter/status` | `status_multi_jupyter` |
| GET | `/experiments/{exp}/jupyter/notebook` | `get_multi_jupyter_notebook` |
| POST | `/experiments/{exp}/jupyter/notebook` | `create_multi_jupyter_notebook` |

**Both start routes write the notebook first**, and since 1.3.0 that write may
*replace* an existing `interactive.ipynb` — when expman wrote it, you have not
edited it, and the template has changed since. An edited notebook is never
overwritten. Full rules, and the `--notebook-template` placeholder list:
[CLI reference](/reference/cli/#exp-serve-dir).

`GET /jupyter/available` reports on the **configured** Jupyter command
(`exp serve --jupyter-command`, default `jupyter`), not a hardcoded `jupyter` —
so a project that reaches Jupyter through `uv run` is detected rather than
falling back to the ipython/python copy-paste view.

**409 from the notebook POSTs means "the file on disk stands"** — whether because
it is already current or because it was edited and left alone. The two are the
same answer to the caller; the distinction is in the server log (warn for an
edited file, info for a regeneration).

## TensorBoard — **NEW / uncommitted**

| Method | Path | Handler |
|---|---|---|
| GET | `/tensorboard/available` | `tensorboard_handlers.rs:14` → `{available}` |
| GET | `/experiments/{exp}/runs/{run}/tensorboard/has_logs` | `:20` → `{has_logs}` |
| POST | `/experiments/{exp}/runs/{run}/tensorboard/start` | `:30` → `{port}` |
| POST | `/experiments/{exp}/runs/{run}/tensorboard/stop` | `:43` |
| GET | `/experiments/{exp}/runs/{run}/tensorboard/status` | `:54` → `{running, port}` |

No multi-run variant exists, unlike Jupyter — see `STATUS.md`.

## Frontend fallback

`GET /*` → `frontend::serve_frontend` (`frontend.rs:18`). Serves a `rust-embed`
asset if the path matches, else falls back to `index.html` for anything that
looks like an SPA route (heuristic: the path contains no `.`, `:44`), else 404.

## Notes

- **No WebSocket endpoints exist**, despite axum's `ws` feature being enabled.
- **No caching.** Every request re-reads the filesystem and re-parses Parquet.
- Routes the frontend never calls: `/run/{exp}/{run}/stream/vectors`,
  `/experiments/{exp}/runs/{run}/config`, `/experiments/{exp}/stats`,
  `/config`.
- Test coverage: `tests/api_test.rs` covers 7 routes in-process via
  `tower::ServiceExt::oneshot`. The Jupyter and TensorBoard routes are
  **untested**.


---

## `GET /projects/{project}/runs`

`GET /projects/{project}` returns an experiment list only, which is not a view
you can work in: with one project per study, the comparison worth living in cuts
*across* experiments. This returns the flat runs table plus what a filter UI
needs.

Query parameters (all optional):

| param | meaning |
|---|---|
| `tags` | expression, e.g. `arm:tiered AND (study:1 OR study:2)` |
| `status` | `RUNNING`, `FINISHED`, `FAILED`, `CRASHED` (400 otherwise) |
| `experiment` | narrow to one experiment |
| `group` | narrow to one DDP job or sweep cohort |

```json
{
  "project": "study-1",
  "total": 2,
  "runs": [{ "run": "tiered-s2", "experiment": "...", "project": "study-1",
             "status": "FINISHED", "started_at": "...", "heartbeat_at": null,
             "tags": ["arm:tiered"], "scalars": {}, "vectors": {},
             "path": "experiments/..." }],
  "facets": {
    "tags":        { "arm:tiered": 2, "seed:1": 1, "study:1": 2 },
    "status":      { "FINISHED": 2 },
    "experiments": { "e1-drift-regret-slope": 2 },
    "groups":      { "lr-sweep": 6, "job-77001": 4 }
  },
  "metrics": ["regret"]
}
```

This is `core::dto::ProjectRuns`, so the frontend deserializes the same type the
handler serializes. It backs the dashboard's **Compare** tab.

Facets are counted over the **returned** runs, so they narrow as filters are
applied — the behaviour a filter UI needs to avoid offering options that lead to
empty results. `metrics` is the union of metric names across those runs, so a
caller knows what is comparable before fetching any series.

## Read-only mode

`exp serve --read-only` rejects every request that is not `GET`/`HEAD`/`OPTIONS`
with `403 {"error": "read_only", "message": ...}`. It is middleware over the
whole router (`enforce_read_only` in `api/mod.rs`), not a per-handler check, so
new routes are covered by default. `/config` reports `read_only` so the frontend
can hide controls rather than offering writes that will fail.

## Generated projects

A project created by `exp project sync` carries `generated: true`. Writes to it
— `PATCH /projects/{p}` and `PUT /projects/{p}/readme` — are refused with `409`:

```json
{
  "error": "generated_project",
  "message": "Project 'study-1' is generated from studies.yaml (thesis repo) and is regenerated on each sync. Edit the source and re-run `exp project sync` instead.",
  "generated_from": "studies.yaml (thesis repo)"
}
```

Accepting the write would be worse than refusing it: the dashboard would report
success and the edit would vanish at the next sync with no trace. `DELETE` is
refused on the same grounds.

## Downsampling

`GET /experiments/{exp}/runs/{run}/metrics` returns at most **2000 points**.

| param | meaning |
|---|---|
| `max_points` | change the cap; `0` disables it |
| `full=1` | return every row — the escape hatch for export and analysis |
| `since_step` | only rows past this step (used by the SSE stream) |

The reduction is **Largest-Triangle-Three-Buckets**, not a stride. A stride drops
the single-row loss spike or divergence, which is exactly what someone opens a
chart to find; LTTB keeps whichever point in each bucket contributes the most
visible area, so extremes survive. First and last rows are always kept, so
endpoints are exact.

Without this, a million-step run serialises to hundreds of MB of JSON and hangs
the tab before it draws anything.
