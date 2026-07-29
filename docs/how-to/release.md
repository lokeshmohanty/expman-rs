# How-to — cut a release

*Verified 2026-07-27 against the 11 workflows in `.github/workflows/`.*

## The procedure

```bash
just bump patch      # or minor, or major
git push
```

That is the whole thing. **The commit message is the release trigger.**

`just bump`:

1. reads the current version from `Cargo.toml`
2. rewrites it **there and nowhere else** — `pyproject.toml` takes it via
   maturin's `dynamic = ["version"]`, and `flake.nix` reads it with
   `builtins.fromTOML`
3. `cargo update -p expman` to refresh `Cargo.lock`
4. runs `just check-versions`, which fails if a literal has been reintroduced
5. commits `Cargo.toml` + `Cargo.lock` as `release: bump version to X.Y.Z`

It does **not** tag and does **not** push. The tag `v<version>` is created later
by the GitHub Release action.

## What fires on push

`check-release.yml:26` regex-matches `github.event.head_commit.message` against:

```
^release: bump version to (\d+\.\d+\.\d+)$
```

Because a bump commit touches `Cargo.toml` and `Cargo.lock`, **four workflows
fire simultaneously**: `publish.yml` (no path filter), `nix.yml` (matches the
changed paths), `docs.yml` (no path filter), and `ci.yml`.

`publish.yml` orchestrates: `check-release` → `build-assets` → **`rust` +
`python`** → `publish-cargo` + `publish-pypi` in parallel → `github-release`.
The `rust` and `python` jobs are the release gate: nothing publishes unless both
pass.

| target | workflow | secret / environment |
|---|---|---|
| crates.io | `publish-cargo.yml` | env `CARGO`, `CARGO_REGISTRY_TOKEN` |
| PyPI | `publish-pypi.yml` (3-OS matrix → `uv publish`) | env `PYPI`, `PYPI_API_TOKEN` |
| GitHub Releases | `github-release.yml` | — |
| gh-pages (rustdoc) | `docs.yml`, on **every** push to main | — |
| Cachix | `nix.yml` | `lokeshmohanty` cache |

## The traps

### 1. The regex is exact and failure is silent

Anchored, single spaces, no `v` prefix, no trailing period, exactly three
numeric components. A squash-merge that rewrites the subject, a merge commit, or
any suffix yields `should_release=false` — **the push succeeds, nothing
publishes, and no job fails.** There is no negative signal. If a release did not
happen, check the commit subject first.

Related: `head_commit.message` is only the **last** commit of the push. Pushing
several commits where the bump is not last releases nothing.

### 2. Releases wait for tests — but not the ones in `ci.yml`

Fixed 2026-07-28. `publish.yml` calls `rust.yml` and `python.yml` itself, and
both publish jobs `need` them, so a failing test blocks publication.

The subtlety worth keeping in mind: the `ci.yml` run you see on the same commit
is **still** a separate workflow and still gates nothing. The gate is the `rust`
and `python` jobs *inside* the publish run. A release commit therefore runs the
suite twice, in parallel. Run `just ci` locally anyway — it is faster than
discovering it in CI.

### 3. Two workflows race to create the same GitHub Release

Both `publish.yml:33-42` (with `artifact-pattern: wheels-*`) and `nix.yml:55-63`
(with `artifact-pattern: nix-build-results`) invoke `github-release.yml` for the
same tag. `softprops/action-gh-release` creates-or-updates, so whichever
finishes second updates the release — **the final asset set depends on race
ordering**, and release notes may be regenerated twice. This is the likely cause
of any "the release is missing the wheels" or "missing the nix binary"
flakiness.

### 4. The `dist/` + `--allow-dirty` knot

Read commits `49c24ca` and `d423ff8` together:

- `dist/` is gitignored, so `cargo package` was omitting the built frontend, and
  consumers building with `--features server` hit `build.rs`'s `exit(1)`.
  Fix: `Cargo.toml` gained `include = [... "dist/**/*"]`.
- That made things worse for CI: it downloads `frontend-dist` into `dist/`,
  which is now both gitignored *and* in Cargo's `include`, and cargo refuses to
  package files it will ship but that are not committed. Fix:
  `publish-cargo.yml:29` gained **`--allow-dirty`**.
- The same commit added a `CARGO_DOC` branch to `build.rs` (`:12`, `:30-40`)
  writing placeholder `dist/` assets instead of `exit(1)`, which is what
  unblocked `docs.yml`.

If you touch `Cargo.toml`'s `include`, `build.rs`, or `publish-cargo.yml`,
re-read all three together.

### 5. The version lives in exactly one place

Fixed 2026-07-28. `Cargo.toml` is the source; `pyproject.toml` is
`dynamic = ["version"]` and `flake.nix` reads `builtins.fromTOML ./Cargo.toml`.
`just check-versions` asserts no literal has crept back and runs in CI's lint
job.

Why it mattered: Python's `__version__` comes from `CARGO_PKG_VERSION`, not
`pyproject.toml`, so a desync shipped a wheel whose metadata disagreed with
`expman.__version__` — a discrepancy nothing would have caught.

Still never hand-edit a version. Always `just bump`.

### 6. Never commit `wrappers/python/expman/bin/`

13 MB, platform-specific, and it must stay untracked-but-not-gitignored so
maturin still ships it. See [setup.md](setup.md).

### 7. `docs.yml` uses `force_orphan: true`

Every push to main rewrites gh-pages history from scratch (`docs.yml:33`).

## Pre-flight checklist

```bash
just ci                      # nothing else gates this
git status                   # bin/ untracked? debris staged?
git log --oneline -1         # will the bump commit be LAST on the push?
just bump patch
git push
```

Then watch: `publish.yml`, `nix.yml`, `docs.yml`, `ci.yml` all running; the
release at `v<version>` should end up with both the wheels and the nix results
attached.
