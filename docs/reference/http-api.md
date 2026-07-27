# Reference — HTTP API

*Hand-written from `src/api/*.rs`, 2026-07-27. Authoritative over
`src/api/README.md`, which is stale (it documents a nonexistent `/api/events`
endpoint and mentions `utoipa`).*

Router: `src/api/mod.rs:42-140`. All routes below are prefixed with `/api`
(`mod.rs:153`). Anything else falls through to the embedded frontend
(`mod.rs:155`). CORS is fully permissive — `Any` origin/method/header
(`mod.rs:146-149`).

Rows marked **NEW** are from the uncommitted TensorBoard work.

## Experiments

| Method | Path | Handler | Response |
|---|---|---|---|
| GET | `/experiments` | `experiments.rs:17` | `[{id, display_name, description, tags, runs_count}]` (ad-hoc `json!`) |
| GET | `/experiments/{exp}/metadata` | `experiments.rs:39` | `ExperimentMetadata` |
| PATCH | `/experiments/{exp}/metadata` | `experiments.rs:57` | merged `ExperimentMetadata`. Body: `{display_name?, description?, tags?}` |
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
| GET | `/stats` | `stats.rs:54` | `{total_experiments, total_runs, active_runs, total_storage_bytes}` |
| GET | `/config` | `stats.rs:82` | `{live_mode: true, version}` — `live_mode` is **hardcoded** because `ServerConfig.live_mode` is dropped rather than stored in `AppState` |

## Jupyter

Per-run, plus a `__multi__` variant scoped to the experiment.

| Method | Path | Handler |
|---|---|---|
| GET | `/jupyter/available` | `jupyter_handlers.rs:17` → `{backend: "jupyter"\|"python"\|"none"}` |
| POST | `/experiments/{exp}/runs/{run}/jupyter/start` | `:23` → `{port}` |
| POST | `/experiments/{exp}/runs/{run}/jupyter/stop` | `:46` |
| GET | `/experiments/{exp}/runs/{run}/jupyter/status` | `:57` → `{running, port}` |
| GET | `/experiments/{exp}/runs/{run}/jupyter/notebook` | `:69` → `{exists, content}` |
| POST | `/experiments/{exp}/runs/{run}/jupyter/notebook` | `:87` → `{created, content}` or 409 |
| POST | `/experiments/{exp}/jupyter/start` | `:122`, body `{runs: [String]}` |
| POST | `/experiments/{exp}/jupyter/stop` | `:155` |
| GET | `/experiments/{exp}/jupyter/status` | `:166` |
| GET | `/experiments/{exp}/jupyter/notebook` | `:178` |
| POST | `/experiments/{exp}/jupyter/notebook` | `:196` |

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
