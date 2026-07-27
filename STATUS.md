# STATUS — volatile state

*Update in place; keep short; absolute dates. History lives in git log.*

## Current focus (2026-07-27)

**In-flight, uncommitted: TensorBoard-in-the-dashboard integration.** Released
version is 0.5.3 (`384c540`). The working tree adds an embedded TensorBoard tab
to the run detail page, spanning:

- new: `src/api/tensorboard_service.rs`, `src/api/tensorboard_handlers.rs`,
  `src/app/components/tensorboard.rs`
- modified: `src/api/mod.rs`, `src/api/state.rs`, `src/app/models.rs`,
  `src/app/fetch.rs`, `src/app/components/mod.rs`,
  `src/app/pages/experiment_detail.rs`, `wrappers/python/expman/__init__.py`

It is fully wired end to end (5 routes, `AppState.tensorboard`, a UI tab, and a
Python `Experiment.tensorboard_dir` property + module-level accessor) — this is
finished-but-unpolished, not a stub. Distinct from the *already released*
TensorBoard support: the `SummaryWriter` drop-in, `exp export --format
tensorboard`, and `exp import`.

## Next actions

- [ ] Decide multi-run TensorBoard: `TensorBoardView` bails on >1 selected run
      (`src/app/components/tensorboard.rs:29`) though `--logdir` supports it and
      the Jupyter path already has a `__multi__` mode.
- [ ] Fix the fake readiness check — `tensorboard.rs:102-113` pings with
      `RequestMode::NoCors` (opaque responses always look OK) then sets
      `is_ready` unconditionally after the loop.
- [ ] `src/api/mod.rs` `serve()` now does `let _ = server.await;` — restore the
      `?` so a bind/serve failure isn't silently swallowed.
- [ ] Verify `TENSORBOARD_CSP=frame-ancestors *` (`tensorboard_service.rs:97`)
      is a real TensorBoard env var; if not, the iframe will be blocked.
- [ ] `--bind_all` binds TensorBoard to 0.0.0.0 with no auth, unlike Jupyter's
      loopback default. Decide whether that's acceptable.
- [ ] Add tests: `tensorboard_service.rs` has none, though it is a structural
      copy of `jupyter_service.rs` whose tests (`:449-537`) would port nearly
      verbatim. `tests/api_test.rs` covers none of the new routes either.
- [ ] Split the unrelated rider in the same diff: `src/api/mod.rs` flips most
      submodules from private to `pub`. Separate commit, or revert.
- [ ] Refresh `src/api/README.md` (rendered into rustdoc via `#![doc =
      include_str!]`) — it still documents a nonexistent `/api/events` endpoint
      and mentions `utoipa`.

## Open obligations / blockers

- **`wrappers/python/expman/bin/` is untracked and must stay that way.** It is
  deliberately NOT in `.gitignore` (maturin honours `.gitignore` and would drop
  the binary from the wheel). Run once per clone:
  `echo "wrappers/python/expman/bin/" >> .git/info/exclude` — **not yet done on
  this machine**, so `git add -A` would commit a 13 MB NixOS-linked binary.
- **Untracked local debris at repo root**: `scratch/`, `test_artifacts/`,
  `test_results/`, `test_experiments/`. Not part of the test suite (which uses
  `TempDir`/`tmp_path` exclusively). Deliberately left alone on 2026-07-27 —
  clean up or gitignore when convenient.
- **Known defect, unfixed**: `src/core/storage.rs:272` downcasts the `step`
  column to `UInt64Array`, but it is written as `Int64` (`:475`, `:486`). The
  downcast always returns `None`, so cross-batch step dedup is dead code and
  re-logging a step across a flush boundary yields duplicate rows.
