# How-to — add an API endpoint end to end

*Written 2026-07-27 by tracing how the TensorBoard integration was added.*

Adding a route touches five or six files. The TensorBoard work in the current
working tree is the best worked example — follow it file by file.

## 1. Handler — `src/api/<area>.rs`

Handlers take `State<AppState>` and `Path`/`Query` extractors and return
`Json<serde_json::Value>` (most handlers build responses with the `json!` macro
rather than typed structs — follow the local style).

```rust
pub async fn status_tensorboard(
    State(state): State<AppState>,
    Path((exp, run)): Path<(String, String)>,
) -> Json<serde_json::Value> { ... }
```

Resolve paths from `state.base_dir` — never trust a client path. For anything
that reads a user-supplied path, copy the canonicalize + `starts_with` guard in
`src/api/artifacts.rs:47-58`.

## 2. Register the route — `src/api/mod.rs`

Add `pub mod <name>;` and a `.route(...)` line in the router
(`mod.rs:42-140`). Keep the `/experiments/{exp}/runs/{run}/...` shape — only one
existing route deviates and it is regretted.

## 3. State, if the endpoint owns something — `src/api/state.rs`

`AppState` is `Clone` and cloned per request. Anything shared goes behind
`Arc<Mutex<...>>` in a `Clone` newtype, following `JupyterManager`
(`jupyter_service.rs:262-265`).

**Use `std::sync::Mutex`, and never hold the guard across an `.await`.** Every
existing critical section is a short map operation. If you need to hold a lock
across await points, that is a design change, not a local one.

If the endpoint spawns a process, register cleanup in `serve()`'s shutdown path
(`mod.rs:186-187`) and honour `state.shutdown_token`.

## 4. Frontend model — `src/app/models.rs`

Add a `Deserialize` struct mirroring the response. These are **hand-mirrored,
not shared** with `core::models` — the wasm boundary keeps them separate. Match
the wire format exactly; there is no compiler check tying the two together.

## 5. Frontend fetch — `src/app/fetch.rs`

Plain `gloo_net::http::Request`, returning `Result<T, String>`. The house style
is `resp.text()` then `serde_json::from_str` rather than `resp.json()`.

**Check the HTTP status.** Several existing functions (`stop_jupyter`,
`stop_tensorboard`) ignore it — do not copy that.

## 6. Component — `src/app/components/`

Wrap the fetch in `LocalResource::new(...)` and render inside `<Suspense>`.
Declare it in `components/mod.rs` and mount it where it belongs (a tab in
`pages/experiment_detail.rs:375,405`, for example).

If you open an `EventSource` or spawn anything, **close it in `on_cleanup`** —
`components/console.rs:105` is the correct pattern. `components/tensorboard.rs`
omits this and leaks processes on navigation.

## 7. Test — `tests/api_test.rs`

In-process via `tower::ServiceExt::oneshot` against `build_router` + `AppState`,
with a `TempDir` fixture (`api_test.rs:12-49`). No network, fast. Add a case
per route; the existing 7 tests are the template.

## Checklist

- [ ] handler with path-traversal guard where relevant
- [ ] route registered in `mod.rs`
- [ ] `AppState` field + shutdown cleanup if it owns a resource
- [ ] frontend model matching the wire format
- [ ] fetch function that checks the status
- [ ] component with `on_cleanup` for anything long-lived
- [ ] test in `tests/api_test.rs`
- [ ] `docs/reference/http-api.md` updated
- [ ] `just ci` green

## Things to know before you start

- **There is no caching.** Every request re-reads the filesystem and re-parses
  Parquet. If your endpoint is hot, that is the constraint to design against.
- **Realtime is SSE only.** Both existing streams are 500 ms poll loops with 15 s
  keep-alives, terminated by `.take_until(shutdown.cancelled())`
  (`metrics.rs:41`, `:78`). Axum's `ws` feature is enabled but unused.
- **CORS is fully permissive** (`mod.rs:146-149`). Fine for a localhost tool;
  do not add anything that assumes an origin check.
- **Spawned services are not proxied.** Jupyter and TensorBoard are reached
  directly at `http://localhost:{port}`, which is why the dashboard only works
  from the machine running the server. If you are adding a third such service,
  consider building the proxy route instead — see
  [../decisions.md](../decisions.md).
