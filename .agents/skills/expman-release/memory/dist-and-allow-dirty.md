# The dist/ + --allow-dirty knot

*Recorded 2026-07-27 from commits `49c24ca` and `d423ff8`. Read both together —
the second exists because of the first.*

## The chain

`build.rs` hard-`exit(1)`s when the `server` feature is on, `trunk` fails, and
no `dist/index.html` exists (`build.rs:44`). Every workaround in this repo
descends from that single branch.

1. **`dist/` is in `.gitignore`** (it is trunk output), so `cargo package` was
   omitting the built frontend from the published crate. A consumer running
   `cargo install expman --features server` then hit the `exit(1)`.

2. **`49c24ca` — "fix: include dist in cargo.toml"** added an explicit
   `include = [...]` list to `Cargo.toml` naming `"dist/**/*"`, so the built
   frontend ships in the crate.

3. That broke publishing. CI downloads a `frontend-dist` artifact into `dist/`,
   which is now both gitignored *and* in Cargo's `include` — and **cargo refuses
   to package files it will ship but that are not committed.**

4. **`d423ff8` — "fix: cargo publish"** added **`--allow-dirty`** to
   `publish-cargo.yml:29`. That is the only reason publishing works.

5. The same commit also:
   - reverted `49c24ca`'s `prep-dist` change, restoring
     `touch dist/index.html` (`Justfile:37-38`)
   - added a `CARGO_DOC` branch to `build.rs` (`:12`, `:30-40`) that writes
     placeholder `dist/index.html`, `app.js`, `app.wasm`, `style.css` instead of
     `exit(1)` — **this is what unblocks `docs.yml`**, which runs
     `cargo doc --all-features` (enabling `server`) without trunk available
   - carried unrelated riders: `.ipynb_checkpoints/` filtering in
     `storage.rs:36,59`, and `tensorboard.py` switching to
     `redirect_console=True` with eager `log_params({})` / `info(...)`

## What this means for you

Do not remove any of these in isolation:

| thing | remove it and… |
|---|---|
| `include = [... "dist/**/*"]` in `Cargo.toml` | published crate can't build with `--features server` |
| `--allow-dirty` in `publish-cargo.yml` | cargo publish fails on the uncommitted `dist/` |
| the `CARGO_DOC` branch in `build.rs` | `docs.yml` fails and gh-pages stops updating |
| `touch dist/index.html` in `just prep-dist` | `just build-docs` fails locally |
| CI downloading `frontend-dist` into `dist/` | every Rust job invokes trunk, or fails |
| `EXPMAN_SKIP_FRONTEND_BUILD=1` in CI | same |

## The clean fix, if anyone wants it

Make `build.rs` degrade gracefully instead of `exit(1)` — emit a
`cargo:warning` and a placeholder `dist/`, letting `frontend.rs` serve a "run
`just build-frontend`" page. Every hack above collapses into that one change.
Not done as of 2026-07-27.
