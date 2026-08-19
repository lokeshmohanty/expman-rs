+++
title = "Building and Running"
description = "How to build components, run the CLI, dashboard, examples, and Nix packages."
weight = 2
+++

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

## Styling

Tailwind is **built**, not fetched. `src/app/index.html` declares
`data-trunk rel="tailwind-css" href="../../assets/tailwind.css"`; trunk runs the
standalone Tailwind CLI, pinned in `Trunk.toml`:

```toml
[tools]
tailwindcss = "3.4.17"
```

Trunk downloads that version on a normal build. The Nix build runs with
`TRUNK_OFFLINE=true`, so it takes `pkgs.tailwindcss_3` from `nativeBuildInputs`
instead — the two must stay the same version. They currently emit byte-identical
CSS; if you bump one, bump the other.

Config is `tailwind.config.js`. Two things to know:

- **Classes are scanned out of the Rust source** (`content: ["./src/**/*.rs"]`).
  A class assembled at runtime — `format!("text-{}", colour)` — will not be
  generated and the element renders unstyled. Build such classes from a `match`
  that returns whole literals.
- **`@tailwindcss/typography` needs no npm.** The standalone CLI bundles it, so
  the `prose` classes used for project READMEs work. They did *not* work under
  the old `cdn.tailwindcss.com` script, which ships no plugins — that markdown
  had been silently unstyled.

## Fonts

Three type roles, self-hosted:

| role | face | carries |
|---|---|---|
| display | Space Grotesk | headings, the wordmark |
| body | Nunito | prose, descriptions, list items |
| mono | Cascadia Code | run IDs, metrics, counts, timestamps, tags, uppercase labels, code |

The woff2 files are vendored into `assets/fonts/` (latin + latin-ext only,
~273 KB) with `@font-face` rules in `assets/fonts.css`. `index.html` copies them
via `data-trunk rel="copy-dir"` and links the CSS; `api/frontend.rs` embeds
`*.woff2` into the binary.

> Two ways this breaks silently. If `frontend.rs` loses its `#[include =
> "*.woff2"]`, the faces 404 and the page falls back to system fonts — which
> looks like a CSS bug. And `code`/`pre` are styled by Tailwind preflight from
> its own variable, not from the font config, so the mono role is set directly
> in the `<style>` block; drop that rule and every code block silently reverts.

To regenerate after a Fontsource release, unpack the three
`@fontsource-variable/*` tarballs and copy the latin subsets, taking the
`unicode-range` values from each package's own `unicode.json` rather than
transcribing them:

```bash
for pkg in space-grotesk nunito cascadia-code; do
  curl -sL "https://registry.npmjs.org/@fontsource-variable/$pkg/-/$pkg-<VERSION>.tgz" \
    | tar xz -C "$pkg" --strip-components=1
done
# then copy files/*-latin{,-ext}-wght-{normal,italic}.woff2 into assets/fonts/
# and rebuild assets/fonts.css from each package's unicode.json
```

Verify from a loaded page, not from the stylesheet:

```js
getComputedStyle(document.querySelector("h1")).fontFamily    // Space Grotesk Variable
getComputedStyle(document.querySelector("code")).fontFamily  // Cascadia Code Variable
[...document.fonts].filter(f => f.status === "loaded").map(f => f.family)
// Should be empty — the dashboard makes no third-party requests:
performance.getEntriesByType("resource").map(r => r.name).filter(n => !n.startsWith(location.origin))
```

## New asset? `git add` it before `nix build`

A flake's `src = ./.` sees only **git-tracked** files. An untracked
`assets/whatever.css` builds fine with `trunk` and then fails under
`nix build .#expman` with a confusing "No such file or directory" for a path that
plainly exists. Stage new assets before testing the Nix build.

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
| Jupyter/TensorBoard tab shows a blank iframe | those services are not proxied — the browser connects directly to `localhost:{port}`, so this only works when the browser and the server are on the same machine. Unaffected by `--jupyter-command`: that changes which interpreter is launched, not how the browser reaches it. |
| `ImportError` from `import expman` | the extension is not built. `just dev-py`. |
| Interactive tab cannot import your *project's* package | the kernel is the interpreter Jupyter runs under, which is whichever environment the *server* was started in. Launch Jupyter inside the project instead: `exp serve --jupyter-command 'uv run --extra nb jupyter'`. |
| edits to `.expman/notebook.ipynb` do not reach a run's Interactive tab | that run's `interactive.ipynb` has been edited, so expman will not overwrite it. The server logs a warn saying so; delete the file to opt back into regeneration. |
| `exp serve` exits with *"is not a parsable command line"* | an unbalanced quote in `--jupyter-command`. It is validated at startup rather than at the first click on the tab. |
| clippy passes locally, fails in CI | `just lint-rust` includes the wasm target via `lint-frontend`; the pre-commit hook does not. |
