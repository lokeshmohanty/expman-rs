---
name: expman-build
description: >
  Use when building, running, testing, or linting expman-rs — before any cargo,
  trunk, maturin, or nix command in this repo. Covers the feature-gate matrix,
  the build.rs frontend trap that hard-exits(1), which just recipe to reach for,
  and why clippy passing locally does not mean CI is green.
---

# expman-rs — build and test

Prefer `just` recipes over raw cargo. Several exist specifically to route around
`build.rs`. Full detail: `docs/how-to/build-and-run.md` and
`docs/how-to/test-and-lint.md`.

## Before running any cargo command

`default = []`. Nothing builds unless a feature asks for it:

| you want | features |
|---|---|
| CLI only | `cli` |
| CLI + dashboard | `cli,server` |
| Python wheel | `python` |
| tests / clippy | `--all-features` |

**Any build with `server` on (including `--all-features`) invokes `trunk build
--release` from `build.rs` and hard-exits(1) if trunk fails and `dist/index.html`
does not exist** (`build.rs:44`). The error does not obviously say "install
trunk".

Escape hatches — use one when you are not iterating on the frontend:

```bash
export EXPMAN_SKIP_FRONTEND_BUILD=1    # skip trunk; requires dist/index.html
just prep-dist                          # mkdir -p dist && touch dist/index.html
```

CI uses both, plus a downloaded `frontend-dist` artifact.

## The commands

```bash
just check     # fmt-check + lint-rust + lint-py + cargo check --all-features
just ci        # fmt-check lint test lint-py test-py — the full local gate
just test      # test-py, then cargo nextest run --all-features
just dev-py    # the Python dev loop: build CLI, copy into package, maturin develop
just serve     # dashboard on 127.0.0.1:8000
just bench     # test_log_vector_is_fast in release with output
```

Run `just ci` before proposing any change as done, and before any release —
nothing else gates a release. See the `expman-release` skill.

## Traps

- **`just lint-rust` runs clippy twice** — once against wasm32 (`lint-frontend`)
  and once native. The pre-commit hook only does the native pass, so **frontend
  clippy failures surface in CI, not locally.** Always use the just recipe.
- **`ruff format` is not in CI**, only `ruff check`. Formatting drift is invisible
  to CI.
- **Python tests only run on Linux** in CI despite the code implying an OS matrix.
- `cli_test.rs` needs the `cli` feature — that is why the suite is always
  `--all-features`.
- The dashboard loads **Tailwind from a CDN** (`src/app/index.html:8`). If it
  renders unstyled, you are offline; the binary is not actually self-contained.
- Jupyter and TensorBoard are **not proxied** — the browser hits
  `http://localhost:{port}` directly, so those tabs only work when the browser
  and server share a machine. A blank iframe is usually this, not a bug in your
  change.

## Memory

- `memory/verification.md` — what "done" requires here, and what CI will not
  catch for you.

## See also

- `expman-release` — versioning and publishing. Read it before touching a
  version, a workflow, `Cargo.toml` metadata, or `build.rs`.
- `docs/architecture.md` — why the engine, storage, and build are shaped this way.
