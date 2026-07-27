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
| `publish.yml` | push on `main`, **no path filter** | `check-release` → `build-assets` → (`publish-cargo` ∥ `publish-pypi`) → `github-release` (pattern `wheels-*`) |
| `nix.yml` | push on `main`, paths `src/**`, `wrappers/python/**`, `flake.nix`, `Cargo.toml`, `Cargo.lock`, self | `nix build .#expman` + `.#python3Packages.expman-rs` → push to cachix → on a release commit, `github-release` (pattern `nix-build-results`) |
| `docs.yml` | push on `main`, **no path filter** | `just build-docs` → gh-pages via `peaceiris/actions-gh-pages@v4` with `force_orphan: true` |

## What a release commit actually sets off

All four entry points fire at once, because a bump commit touches
`Cargo.toml`, `Cargo.lock`, `pyproject.toml`, and `flake.nix`.

Two consequences that are not obvious from any single file:

- **`github-release.yml` is invoked twice** for the same tag, from `publish.yml`
  and from `nix.yml`, with different `artifact-pattern`s. The action
  creates-or-updates, so the second to finish wins and the final asset set is
  race-dependent. Release notes may regenerate twice.
- **`publish-cargo` and `publish-pypi` run in parallel and neither depends on
  `ci.yml`.** `github-release` needs `publish-pypi` (so wheels exist) but *not*
  `publish-cargo` — if cargo publish fails, PyPI and the GitHub Release still go
  out.

## Smaller oddities

- `python.yml:29` computes the CLI artifact name from `runner.os` with a nested
  ternary, but the job has no matrix and is always `ubuntu-latest`. Implies an
  intended-but-absent OS matrix; harmless.
- `publish-pypi.yml:24-28` downloads `frontend-dist` into `dist/`, but the
  maturin build uses `features = ["python"]` only, so `build.rs` never invokes
  trunk. Unnecessary defensive work.
- `docs.yml` has no path filter and `force_orphan: true`, so **every** push to
  main rewrites gh-pages history from scratch.
