# expman-rs Justfile
# Run `just` to see available commands

# The Zola version the documentation site is built with — the single source of
# truth for it. `.github/workflows/docs.yml` reads this with
# `just --evaluate zola_version` and installs exactly it; `just check-zola`
# asserts the zola the dev shell puts on PATH is the same one.
#
# Pinned because docs.yml used to install `zola@latest`. Zola 0.23 replaced the
# template engine and the reticle theme does not parse under it, so from 0.23.3's
# release every push to main failed to deploy the docs — three consecutive
# releases, silently, because nothing else depends on that job.
#
# To move it: bump nixpkgs so the dev shell provides the new zola, fix the theme
# submodule if it needs it, confirm `just build-zola-docs` passes, then edit this.
zola_version := "0.22.1"

default:
    @just --list

# Start development workflow (alias for dev-py)
dev: dev-py

# Build all features, Python extension, and frontend dashboard
build: build-frontend build-py
    cargo build --all-features

# Build in release mode
build-release: build-frontend build-py
    cargo build --all-features --release

# Run all tests
test: test-py
    cargo nextest run --all-features

# Run tests with output
test-release:
    cargo nextest run --all-features --no-capture

# Watch and re-run tests on change
test-watch:
    cargo watch -x 'nextest run --workspace'

# Build the frontend dashboard
build-frontend:
    @echo "Building frontend with trunk..."
    trunk build --release

# CI Helper: Ensure dist directory exists for rust-embed (avoids build failures)
prep-dist:
    mkdir -p dist
    touch dist/index.html

# Build documentation with a custom landing page from README.md
build-docs: prep-dist
    @echo "Building Rust documentation..."
    cargo doc --no-deps --all-features
    @echo '<meta http-equiv="refresh" content="0; url=expman/index.html">' > target/doc/index.html

# Serve Zola documentation site
docs:
    zola --root docs serve

# Serve Zola documentation site (alias)
serve-docs: docs

# Build Zola documentation site
build-zola-docs:
    zola --root docs build



# Build the CLI binary and copy it to the Python package (platform-aware)
build-cli-for-py:
    mkdir -p wrappers/python/expman/bin
    cargo build --release --features cli,server
    @if [ -f "target/release/exp.exe" ]; then \
        cp target/release/exp.exe wrappers/python/expman/bin/exp.exe; \
    elif [ -f "target/release/exp" ]; then \
        cp target/release/exp wrappers/python/expman/bin/exp; \
        chmod +x wrappers/python/expman/bin/exp; \
    fi

# Bundle a pre-built CLI binary into the Python package (source defaults to target/release)
bundle-cli-bin SRC_DIR="target/release":
    mkdir -p wrappers/python/expman/bin
    @if [ -f "{{SRC_DIR}}/exp.exe" ]; then \
        cp "{{SRC_DIR}}/exp.exe" wrappers/python/expman/bin/exp.exe; \
    elif [ -f "{{SRC_DIR}}/exp" ]; then \
        cp "{{SRC_DIR}}/exp" wrappers/python/expman/bin/exp; \
        chmod +x wrappers/python/expman/bin/exp; \
    fi

# CI Helper: Prepare Python package assets (LICENSE, etc.)
prep-py-assets:
    cp LICENSE wrappers/python/ || true

# Build the Python extension and place the shared library in the package directory
build-py: build-cli-for-py
    @if [ ! -d ".venv" ]; then \
        uv venv --seed --python 3.12; \
    fi
    cd wrappers/python && uv pip install -e .
    cd wrappers/python && uv run maturin develop --release

# Build and install the Python extension for development
dev-py: build-cli-for-py
    @if [ ! -d ".venv" ]; then \
        echo "Creating virtual environment with uv..."; \
        uv venv --seed --python 3.12; \
    fi
    @# Note: we use 'uv run' to ensure maturin uses the venv
    cd wrappers/python && uv run maturin develop
    cd wrappers/python && uv pip install -e .
    cd wrappers/python && uv pip install -e ".[dev]"

# Run the CLI
run *ARGS:
    cargo run --features cli,server -- {{ARGS}}

# Start the dashboard server
serve DIR="./experiments":
    cargo run --features cli,server -- serve {{DIR}}

# List experiments
list DIR="./experiments":
    cargo run --features cli -- list {{DIR}}

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Format code
fmt:
    cargo fmt --all

# Run clippy (excludes frontend WASM crate — use lint-frontend for that)
lint: lint-rust lint-py

lint-rust: lint-frontend
    cargo clippy --all-features --all-targets -- -D warnings

# Run clippy on the frontend (requires wasm32-unknown-unknown target)
lint-frontend:
    cargo clippy -p expman --lib --target wasm32-unknown-unknown -- -D warnings

# Run Python linter (ruff)
lint-py:
    cd wrappers/python && uv run --extra dev ruff check . ../../examples/

# Run Python tests (pytest)
test-py:
    cd wrappers/python && uv run --extra dev pytest tests

# Run the Rust logging example
example-rust:
    cargo run --example logging

# Run the Python basic training example
example-py: dev-py
    uv run python examples/python/basic_training.py

# Quick check (lint + type check without full build)
check: fmt-check lint-rust lint-py
    cargo check --all-features

# Verify every version site still resolves to Cargo.toml's version.
#
# The version is single-sourced, so this should be impossible to fail — which is
# exactly why it is worth asserting. It catches someone re-introducing a literal
# into pyproject.toml or flake.nix, which nothing else in the build would notice
# until a wheel shipped with metadata disagreeing with expman.__version__.
# Deliberately POSIX-only (no sd/rg) so it runs in CI without extra tooling.
check-versions:
    #!/usr/bin/env bash
    set -euo pipefail
    CARGO=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
    echo "Cargo.toml: $CARGO"
    STATUS=0

    if grep -qE '^version = ' wrappers/python/pyproject.toml; then
        echo "  FAIL pyproject.toml pins a literal version; it must use dynamic = [\"version\"]"
        STATUS=1
    else
        echo "  ok   pyproject.toml is dynamic"
    fi

    if grep -qE '^[[:space:]]+version = "[0-9]' flake.nix; then
        echo "  FAIL flake.nix pins a literal version; it must read Cargo.toml"
        STATUS=1
    else
        echo "  ok   flake.nix reads Cargo.toml"
    fi

    # The authoritative check where nix is available: ask it what it resolved.
    if command -v nix >/dev/null 2>&1; then
        NIX=$(nix eval --raw .#expman.version 2>/dev/null || echo "")
        if [ -n "$NIX" ] && [ "$NIX" != "$CARGO" ]; then
            echo "  FAIL nix resolves $NIX, expected $CARGO"
            STATUS=1
        elif [ -n "$NIX" ]; then
            echo "  ok   nix resolves $NIX"
        fi
    fi

    exit $STATUS

# Verify the zola on PATH is the one the Docs workflow pins.
#
# nixpkgs carries no versioned zola attribute, so flake.nix can only ask for
# `zola` and a `nix flake update` is free to move it. That is the one way the dev
# shell and `zola_version` can drift apart, and this is what makes the move loud
# here instead of a surprise in a docs deploy nobody watches.
#
# Skips when zola is absent so the CI lint job, which has no zola, keeps working.
check-zola:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v zola >/dev/null 2>&1; then
        echo "  skip zola not on PATH"
        exit 0
    fi
    HAVE=$(zola --version | awk '{print $2}')
    if [ "$HAVE" != "{{zola_version}}" ]; then
        echo "  FAIL zola on PATH is $HAVE, but docs.yml pins {{zola_version}}"
        echo "       Either the flake moved or the pin is stale — reconcile them,"
        echo "       and only raise zola_version once the docs still build."
        exit 1
    fi
    echo "  ok   zola $HAVE matches the docs.yml pin"

# Full CI check
ci: fmt-check lint test lint-py test-py check-versions check-zola

# Clean build artifacts
clean:
    cargo clean
    rm -rf wrappers/python/expman/*.so

# Publish to PyPI (requires UV_PUBLISH_TOKEN)
publish:
    cd wrappers/python && uv build
    cd wrappers/python && uv publish

# Show code statistics
stats:
    tokei src/ wrappers/python/

# Run a quick benchmark of log_vector throughput
bench:
    cargo test test_log_vector_is_fast --release -- --nocapture


# Bump version: just bump patch|minor|major
bump PART:
    #!/usr/bin/env bash
    set -euo pipefail
    CURRENT=$(grep '^version = ' Cargo.toml | head -1 | sd 'version = "(.*)"' '$1')
    MAJOR=$(echo $CURRENT | cut -d. -f1)
    MINOR=$(echo $CURRENT | cut -d. -f2)
    PATCH=$(echo $CURRENT | cut -d. -f3)
    case "{{PART}}" in
        major) MAJOR=$((MAJOR+1)); MINOR=0; PATCH=0 ;;
        minor) MINOR=$((MINOR+1)); PATCH=0 ;;
        patch) PATCH=$((PATCH+1)) ;;
        *) echo "Usage: just bump patch|minor|major"; exit 1 ;;
    esac
    VERSION="$MAJOR.$MINOR.$PATCH"
    echo "Bumping version $CURRENT → $VERSION..."
    # Cargo.toml is the single source of truth. pyproject.toml takes the version
    # from it via maturin's `dynamic = ["version"]`, and flake.nix reads it with
    # builtins.fromTOML — so this is the only file to edit.
    sd '^version = ".*"' "version = \"$VERSION\"" Cargo.toml
    cargo update -p expman
    just check-versions
    git add Cargo.toml Cargo.lock
    git commit -m "release: bump version to $VERSION"
    echo "Bumped version to $VERSION"
