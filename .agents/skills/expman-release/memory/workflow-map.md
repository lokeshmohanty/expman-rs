# The 11 GitHub workflows

*Recorded 2026-07-27.*

## Reusable building blocks (`workflow_call` only)

| workflow | what |
|---|---|
| `build-assets.yml` | builds the frontend (`just build-frontend` → artifact `frontend-dist`), then a 3-OS matrix CLI build with `CARGO_PROFILE_RELEASE_LTO: fat` → artifacts `cli-bin-<os>`. 1-day retention. The `cli` job `needs: frontend` — the CLI is built with `--features cli,server`, so `dist/` must exist first. |
| `check-release.yml` | regex-matches the head commit message; outputs `should_release` + `version`. |
| `rust.yml` | `lint` (`just fmt-check` + `just lint-rust`) and `test` (`just test-release`). Both download `frontend-dist` and set `EXPMAN_SKIP_FRONTEND_BUILD=1`. |
| `python.yml` | `lint` (`just lint-py`) and `test` (download CLI artifact → `just bundle-cli-bin` → `uv sync --extra dev` → build+install wheel → pytest). Always `ubuntu-latest`. |
| `publish-cargo.yml` | downloads `frontend-dist` into `dist/`, installs protoc, `cargo publish --allow-dirty`. Environment `CARGO`. |
| `publish-pypi.yml` | 3-OS matrix maturin build (`--release --out dist --find-interpreter`, `manylinux: auto`, LTO fat) → `wheels-<os>`; then a `publish` job merging all wheels and running `uv publish`. Environment `PYPI`. |
| `github-release.yml` | `softprops/action-gh-release@v2` at tag `v<version>`, `generate_release_notes: true`. Takes `version` and an optional `artifact-pattern`. |

## Entry points (triggered by pushes)

| workflow | trigger | chain |
|---|---|---|
| `ci.yml` | push + PR on `main`, `paths-ignore`: `**.md`, `examples/**`, `docs/**`, `.gitignore`, `LICENSE` | `build-assets` → `rust` ∥ `python` |
| `publish.yml` | push on `main`, **no path filter** | `check-release` → `build-assets` → (`rust` ∥ `python`) → (`publish-cargo` ∥ `publish-pypi`) → `github-release` (pattern `wheels-*`). The `rust`/`python` jobs are the release gate, added 2026-07-28. |
| `nix.yml` | push on `main`, paths `src/**`, `wrappers/python/**`, `flake.nix`, `Cargo.toml`, `Cargo.lock`, self | `nix build .#expman` + `.#python3Packages.expman-rs` → push to cachix → on a release commit, `github-release` (pattern `nix-build-results`) |
| `docs.yml` | push on `main`, **no path filter** | `zola --root docs build` → `just build-docs` → gh-pages via `peaceiris/actions-gh-pages@v4` with `force_orphan: true` |

## What a release commit actually sets off

All four entry points fire at once. A bump commit now touches only `Cargo.toml`
and `Cargo.lock` (the version is single-sourced), which is still enough to match
`nix.yml`'s path filter.

Two consequences that are not obvious from any single file:

- **`github-release.yml` is invoked twice** for the same tag, from `publish.yml`
  and from `nix.yml`, with different `artifact-pattern`s. The action
  creates-or-updates, so the second to finish wins and the final asset set is
  race-dependent. Release notes may regenerate twice.
- **`publish-cargo` and `publish-pypi` run in parallel and neither depends on
  `ci.yml`** — but since 2026-07-28 both depend on `rust` and `python` jobs
  *inside* `publish.yml`, so tests do gate publication. `ci.yml` remains
  independent and gates nothing; a release commit runs the suite twice.
  `github-release` needs `publish-pypi` (so wheels exist) but *not*
  `publish-cargo` — if cargo publish fails, PyPI and the GitHub Release still go
  out.

## `docs.yml` fails alone and silently

Nothing depends on it and it gates nothing, so a broken docs deploy shows up only
as a red mark on a push everyone reads as "the release worked". That is how
`zola@latest` — the `taiki-e/install-action@zola` shorthand — took out releases
1.2.0, 1.2.1 and 1.3.0 unnoticed when zola 0.23 replaced the template engine the
reticle theme is written against.

Since 2026-08-19 the zola version is pinned: `zola_version` in the **Justfile** is
the only place it is written, `docs.yml` reads it with `just --evaluate` and
installs exactly it, and `just check-zola` (part of `just ci`) fails if the dev
shell's zola has drifted from it. **Do not restore an unpinned tool shorthand
here, and do not put the version in the workflow** — `flake.nix` cannot hold it
because nixpkgs publishes no versioned zola attribute.

When checking a release, look at `docs.yml` explicitly. It is the one entry point
whose failure costs nothing at the time and everything later.

## Smaller oddities

- `python.yml:29` computes the CLI artifact name from `runner.os` with a nested
  ternary, but the job has no matrix and is always `ubuntu-latest`. Implies an
  intended-but-absent OS matrix; harmless.
- `publish-pypi.yml:24-28` downloads `frontend-dist` into `dist/`, but the
  maturin build uses `features = ["python"]` only, so `build.rs` never invokes
  trunk. Unnecessary defensive work.
- `docs.yml` has no path filter and `force_orphan: true`, so **every** push to
  main rewrites gh-pages history from scratch.
