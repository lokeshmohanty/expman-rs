# How-to — build and run

*Verified 2026-07-27. Prefer `just` recipes over raw cargo — several of them
exist specifically to route around `build.rs`.*

## The one thing to understand first: `build.rs`

When the `server` feature is enabled and the target is not wasm32, `build.rs`
shells out to `trunk build --release` to produce `dist/`. On failure it falls
back to an existing `dist/index.html`, writes placeholder assets during
`CARGO_DOC`, and otherwise **hard-exits(1)** (`build.rs:44`).

So `cargo build --all-features` on a clean checkout without `trunk` on PATH
fails at build-script time with an error that does not obviously say "install
trunk".

Two escape hatches:

```bash
export EXPMAN_SKIP_FRONTEND_BUILD=1   # skip trunk; asserts dist/index.html exists
just prep-dist                         # mkdir -p dist && touch dist/index.html
```

CI uses both: every Rust job downloads a prebuilt `frontend-dist` artifact into
`dist/` and sets `EXPMAN_SKIP_FRONTEND_BUILD=1`.

`build.rs` reruns on changes to `src/app`, `Trunk.toml`, and `Cargo.toml`, and
builds with `CARGO_TARGET_DIR=target/wasm_build` with `MAKEFLAGS` stripped (a
recursive-cargo guard).

## Common commands

| command | what |
|---|---|
| `just build` | frontend + python extension + `cargo build --all-features` |
| `just build-release` | same, release profile |
| `just build-frontend` | `trunk build --release` → `dist/` |
| `just build-py` | build the CLI, copy it into the package, `maturin develop --release` |
| `just dev-py` | same but debug — the normal Python dev loop |
| `just build-docs` | `cargo doc --no-deps --all-features` + a redirect `index.html` |
| `just check` | `fmt-check` + `lint-rust` + `lint-py` + `cargo check --all-features` |
| `just clean` | `cargo clean` and remove built `.so`s |

## Running

```bash
just serve [DIR]           # dashboard on 127.0.0.1:8000, DIR defaults to ./experiments
just list [DIR]            # list experiments
just run <ARGS>            # cargo run --features cli,server -- <ARGS>
just example-rust          # examples/rust/logging.rs
just example-py            # examples/python/basic_training.py (runs dev-py first)
just bench                 # test_log_vector_is_fast in release, with output
just stats                 # tokei over src/ and wrappers/python/
```

The dashboard needs both features: `cargo run --features cli,server -- serve`.
`serve` is gated on `server`; without it the subcommand does not exist.

## Feature combinations that matter

| you want | features |
|---|---|
| CLI only, no dashboard | `--features cli` |
| CLI + dashboard | `--features cli,server` ← triggers the trunk build |
| Python wheel | `--features python` ← never triggers trunk |
| everything (tests, clippy) | `--all-features` ← triggers the trunk build |

Wheel builds set `features = ["python"]` only (`pyproject.toml:31`), which is
why `maturin` never needs `trunk`.

## Frontend-only iteration

`Trunk.toml` targets `src/app/index.html` with `dist = "dist"`. `trunk serve`
will hot-reload the WASM app, but the API calls need a backend — run
`just serve` in another shell and point the frontend at it (CORS is fully
permissive, so cross-origin from the trunk dev server works).

Note the WASM bundle is built with `data-wasm-opt="0"` — the current
`expman-app_bg.wasm` is ~2.6 MB.

## Nix builds

```bash
nix build .#expman                          # the exp CLI (features cli,server)
nix build .#python3Packages.expman-rs       # the Python package
nix run github:lokeshmohanty/expman-rs      # run the CLI without cloning
```

`packages.expman` builds the frontend in `preBuild` with `TRUNK_OFFLINE=true`
(forcing the Nix-provided `wasm-bindgen-cli`) and `doCheck = false` — tests are
CI's job.

> **Caveat:** `packages.python3Packages.expman-rs` has no step that populates
> `expman/bin/exp`, so its `exp` console script reports *"Bundled binary not
> found"*. Get the CLI from `.#expman` instead.

## Troubleshooting

| symptom | cause |
|---|---|
| build script exits 1 with a trunk error | `server` feature on, no trunk. Set `EXPMAN_SKIP_FRONTEND_BUILD=1` or `just prep-dist`. |
| dashboard loads unstyled | Tailwind is loaded from a CDN (`src/app/index.html:8`). You are offline. |
| Jupyter/TensorBoard tab shows a blank iframe | those services are not proxied — the browser connects directly to `localhost:{port}`, so this only works when the browser and the server are on the same machine. |
| `ImportError` from `import expman` | the extension is not built. `just dev-py`. |
| clippy passes locally, fails in CI | `just lint-rust` includes the wasm target via `lint-frontend`; the pre-commit hook does not. |
