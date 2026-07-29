# expman-rs — Agent Index

> Minimal index. `STATUS.md` is on demand, not by default: read it (or grep the section you need) only when the task turns on current state. Full documentation in
> `docs/` (start at `docs/index.md`). Global harness rules: `~/.agents/AGENTS.md`.

## What this is

`expman` — a high-performance ML experiment manager in Rust (v1.0.1, MIT). One
crate ships three faces: an `exp` CLI, an axum dashboard server with an embedded
Leptos/WASM frontend, and a PyO3 Python extension whose `log_vector()` is a
non-blocking channel send. Metrics land in per-run Parquet; metadata in YAML.
Entry points: `src/main.rs` (CLI), `src/api/mod.rs` (server), `src/app/main.rs`
(frontend), `src/wrappers/python/mod.rs` (PyO3). Everything is feature-gated —
`default = []`; see `docs/architecture.md`.

## Map

| path | what |
|---|---|
| `STATUS.md` | volatile: current focus, next actions, obligations |
| `docs/` | full documentation — answer questions from here first |
| `.agents/skills/` | project skills + memories (invoke on demand) |
| `src/core/` | engine, storage, models, dto, sweep, sysmetrics, provenance — `models`, `error` and `dto` are wasm-visible |
| `src/api/`, `src/app/` | server (`server` feature) and Leptos frontend (wasm32) |
| `wrappers/python/` | the `expman-rs` PyPI package (import name `expman`) |
| `Justfile` | every build/test/lint/release command — prefer it over raw cargo |

## Project skills & subagents

- `expman-build` — local build/test/lint loop, feature gates, the `build.rs`
  frontend trap. **Read before running any cargo command in this repo.**
- `expman-release` — versioning, `just bump`, the 11 workflows, publish traps.
  **Read before touching a version, a workflow, or `Cargo.toml` metadata.**

The global delegation protocol applies (`~/.agents/AGENTS.md`): tasks go to the
shared subagent fleet, which auto-picks skills/memories via `harness-skill-pick`.
Project-specific worker overrides go in `.agents/agents/*.md`.

## Binding rules

1. Significant changes update `docs/` in the same session (`docs-sync` skill).
2. Durable knowledge → a skill's `memory/`; volatile state → `STATUS.md`; never bloat this file.
3. Summaries in main context; exploration in sub-agents.
4. Never `git add wrappers/python/expman/bin/` — it holds a 13 MB platform-specific
   binary that must stay untracked-but-not-gitignored. See `expman-release`.
