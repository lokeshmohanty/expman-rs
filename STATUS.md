# STATUS — volatile state

*Update in place; keep short; absolute dates. History lives in git log.*

## Current focus (2026-07-27)

**Completed & Verified: TensorBoard Custom UI & Full Engine Integration.**
The working tree provides a dual-mode TensorBoard interface:
1. **ExpMan UI (Metrics)**: Native Leptos charts styled to match ExpMan's modern aesthetic with dark mode and controls.
2. **Histograms & Images & Media**: Native tabs for distribution & asset rendering.
3. **All TB Plugins & Profiler (Full Engine)**: Seamless embedded iframe supporting 100% of TensorBoard features (PyTorch Profiler, Computational Graphs, 3D Embeddings, etc.).

All 12/12 E2E test cases passed via Headless Chrome browser automation.

- Modifed files: `src/app/components/tensorboard.rs`, `src/app/fetch.rs`, `src/api/tensorboard_service.rs`, `src/api/tensorboard_handlers.rs`, `scratch/run_e2e_test.sh`
- WASM assets compiled to `dist/` via `trunk build`.

## Open obligations / blockers

- **`wrappers/python/expman/bin/` is untracked and must stay that way.** It is
  deliberately NOT in `.gitignore` (maturin honours `.gitignore` and would drop
  the binary from the wheel). Run once per clone:
  `echo "wrappers/python/expman/bin/" >> .git/info/exclude`
