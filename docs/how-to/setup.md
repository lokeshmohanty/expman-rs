# How-to — set up a dev environment

*Verified 2026-07-27.*

## Nix (the supported path)

```bash
nix develop
```

Gives you (`flake.nix:57-80`): the fenix stable Rust toolchain with the
`wasm32-unknown-unknown` target, `pkg-config`, `openssl`, `just`, `trunk`,
`wasm-bindgen-cli`, `uv`, `maturin`, `protobuf`, and `cargo-nextest`. Sets
`RUST_LOG=debug`, `RUST_BACKTRACE=1`, and `PYO3_PYTHON` to nixpkgs python312.

A Cachix cache is declared in the flake's `nixConfig`, so it applies to you
automatically. To use it outside the flake:

```bash
cachix use lokeshmohanty
```

## Without Nix

You need: a Rust toolchain with the `wasm32-unknown-unknown` target, `just`,
`trunk`, `wasm-bindgen-cli`, `uv`, `maturin`, `protobuf` (the `tboard` crate
needs `protoc`), and `cargo-nextest`.

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk wasm-bindgen-cli cargo-nextest
# just, uv, maturin, protobuf via your package manager
```

## One-time, per clone — required

The `exp` binary bundled into the Python wheel lives at
`wrappers/python/expman/bin/`. It **must not be committed** (it is 13 MB and
platform-specific — on this machine it is dynamically linked against a
`/nix/store` glibc) and it **must not be added to `.gitignore`** (maturin
honours `.gitignore` and would drop it from the wheel).

The only correct place to exclude it is your local git exclude file:

```bash
echo "wrappers/python/expman/bin/" >> .git/info/exclude
```

This is documented in `CONTRIBUTING.md:32-44`. **Until you run it, `git add -A`
will stage a 13 MB binary.**

## Python dev environment

```bash
just dev-py
```

Builds the CLI binary, copies it into the package, creates a `uv venv --seed
--python 3.12` if absent, runs `maturin develop`, and installs the package
editable with its `dev` extra.

## Optional — pre-commit hooks

`.pre-commit-config.yaml` exists but nothing installs it, and `CONTRIBUTING.md`
does not mention it. If you want it:

```bash
uv run --extra dev pre-commit install
```

It runs whitespace/EOF/yaml/toml checks, `cargo fmt --check`, `cargo clippy
--all-targets --all-features -D warnings`, and ruff (`check --fix` plus
`format`).

> **Two caveats.** (1) The clippy hook uses `--all-features`, which enables
> `server`, which makes `build.rs` invoke `trunk` on every Rust commit unless
> `dist/index.html` already exists — so hooks can be slow or can fail on a clean
> checkout. Export `EXPMAN_SKIP_FRONTEND_BUILD=1` to avoid it. (2) `ruff-format`
> runs here but **not** in CI, so formatting drift from contributors without
> hooks installed is never caught.

## Next

- [build-and-run.md](build-and-run.md) — the build graph and how to run things
- [test-and-lint.md](test-and-lint.md) — what each suite covers
