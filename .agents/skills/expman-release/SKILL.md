---
name: expman-release
description: >
  Use before touching a version number, any file in .github/workflows/, Cargo.toml
  packaging metadata, build.rs, or wrappers/python/pyproject.toml in expman-rs —
  and before cutting a release. Covers `just bump`, the commit-message release
  trigger, the dist/ + --allow-dirty knot, and the binary that must never be
  committed.
---

# expman-rs — release engineering

Full detail: `docs/how-to/release.md`. This is the operational summary.

## Cutting a release

```bash
just ci          # nothing else gates this — CI runs in PARALLEL with publishing
git status       # is wrappers/python/expman/bin/ untracked? debris staged?
just bump patch  # or minor / major
git push         # the bump commit MUST be the last commit of the push
```

`just bump` rewrites the version in `Cargo.toml`,
`wrappers/python/pyproject.toml`, and `flake.nix` (two places), refreshes
`Cargo.lock`, and commits as `release: bump version to X.Y.Z`. It does not tag
and does not push.

**Never hand-edit a version.** It lives in four places plus `Cargo.lock`, and
nothing in CI validates alignment. Python's `__version__` comes from
`CARGO_PKG_VERSION`, not `pyproject.toml`, so a desync ships a wheel whose
metadata disagrees with `expman.__version__`.

## The rules that bite

1. **The commit message is the trigger**, matched by an anchored regex:
   `^release: bump version to (\d+\.\d+\.\d+)$`. No `v`, no suffix, exactly
   three components. A squash-merge or merge commit that rewrites the subject
   publishes nothing — **silently**. No job fails. If a release did not happen,
   check the subject first.
2. Only the **last** commit of a push is inspected.
3. **Releases do not wait for tests.** `publish.yml` and `ci.yml` are separate
   workflows with no dependency. Publication to crates.io and PyPI happens in
   parallel with the test run. `just ci` locally is the only real gate.
4. **Two workflows race to create the same GitHub Release** — `publish.yml`
   (wheels) and `nix.yml` (nix results). Whichever finishes second updates it,
   so the final asset set is race-dependent. This is the cause of "the release
   is missing the wheels / the nix binary".
5. **Never `git add wrappers/python/expman/bin/`.** 13 MB, platform-specific.
   It must stay untracked *and* out of `.gitignore` (maturin honours
   `.gitignore` and would drop it from the wheel). Correct exclusion, once per
   clone: `echo "wrappers/python/expman/bin/" >> .git/info/exclude`.

## Before editing build.rs, Cargo.toml `include`, or publish-cargo.yml

Read commits `49c24ca` and `d423ff8` together first — the second exists because
of the first, and the interaction is not obvious. See
`memory/dist-and-allow-dirty.md`.

## Memory

- `memory/dist-and-allow-dirty.md` — why `--allow-dirty` and the `CARGO_DOC`
  branch exist; do not remove either without understanding the chain.
- `memory/workflow-map.md` — what each of the 11 workflows does and how they
  chain.

## See also

- `expman-build` — the local build/test loop and the `build.rs` trap.
- `docs/decisions.md` — the dated rationale, including open questions about
  single-sourcing the version and gating releases on tests.
