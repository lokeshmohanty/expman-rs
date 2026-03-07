# expman-rs Justfile
# Run `just` to see available commands

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
    mkdir -p dist && touch dist/index.html

# Build documentation with a custom landing page from README.md
build-docs: prep-dist
    @echo "Building Rust documentation..."
    cargo doc --no-deps --all-features
    @echo '<meta http-equiv="refresh" content="0; url=expman/index.html">' > target/doc/index.html

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

# Full CI check
ci: fmt-check lint test lint-py test-py

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
    sd '^version = ".*"' "version = \"$VERSION\"" Cargo.toml
    sd '^version = ".*"' "version = \"$VERSION\"" wrappers/python/pyproject.toml
    sd 'version = ".*"; # Updated version' "version = \"$VERSION\"; # Updated version" flake.nix
    cargo check > /dev/null 2>&1 || true
    git add Cargo.toml wrappers/python/pyproject.toml flake.nix
    git commit -m "release: bump version to $VERSION"
    echo "Bumped version to $VERSION"
