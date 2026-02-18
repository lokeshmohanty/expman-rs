# expman-rs Justfile
# Run `just` to see available commands

default:
    @just --list

# Start development workflow (alias for dev-py)
dev: dev-py

# Build all crates, Python extension, and frontend dashboard
build: build-frontend build-py
    cargo build --workspace

# Build in release mode
build-release: build-frontend build-py
    cargo build --workspace --release

# Run all tests
test:
    cargo nextest run --workspace

# Run tests with output
test-verbose:
    cargo nextest run --workspace --no-capture

# Watch and re-run tests on change
watch:
    cargo watch -x 'nextest run --workspace'

# Build the frontend dashboard
build-frontend:
    @echo "Building frontend with trunk..."
    cd frontend && trunk build --release

# Build documentation
build-docs:
    cargo doc --no-deps --workspace --open

# Build the Python extension and copy the shared library to the package directory
build-py:
    @if [ ! -d ".venv" ]; then \
        uv venv --seed --python 3.12; \
    fi
    uv run maturin build --manifest-path crates/expman-py/Cargo.toml --out target/python-wheels
    # Copy the built library into the python package for local use without full install
    cp target/debug/libexpman.so python/expman/expman.so 2>/dev/null || cp target/debug/libexpman.dylib python/expman/expman.so 2>/dev/null || true

# Build and install the Python extension (auto-manages uv venv)
dev-py:
    @if [ ! -d ".venv" ]; then \
        echo "Creating virtual environment with uv..."; \
        uv venv --seed --python 3.12; \
    fi
    @# Note: we use 'uv run' to ensure maturin uses the venv
    uv run maturin develop --manifest-path crates/expman-py/Cargo.toml

# Run the CLI
run *ARGS:
    cargo run -p expman-cli -- {{ARGS}}

# Start the dashboard server
serve DIR="./experiments":
    cargo run -p expman-cli -- serve {{DIR}}

# List experiments
list DIR="./experiments":
    cargo run -p expman-cli -- list {{DIR}}

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Format code
fmt:
    cargo fmt --all

# Run clippy (excludes frontend WASM crate — use lint-frontend for that)
lint:
    cargo clippy --workspace --exclude frontend --all-targets -- -D warnings

# Run clippy on the frontend (requires wasm32-unknown-unknown target)
lint-frontend:
    cargo clippy -p frontend --target wasm32-unknown-unknown -- -D warnings

# Full CI check
ci: fmt-check lint test

# Clean build artifacts
clean:
    cargo clean
    rm -rf python/expman/*.so

# Publish to PyPI (requires MATURIN_PYPI_TOKEN)
publish:
    maturin publish

# Show code statistics
stats:
    tokei crates/ python/ frontend/

# Run a quick benchmark of log_metrics throughput
bench:
    cargo test test_log_metrics_is_fast --release -- --nocapture

# Bump version and create release commit
bump-version VERSION:
    @echo "Bumping version to {{VERSION}}..."
    @# Update Cargo.toml (workspace package version)
    @sed -i 's/^version = "[0-9.]\+"/version = "{{VERSION}}"/' Cargo.toml
    @# Update pyproject.toml (project version)
    @sed -i 's/^version = "[0-9.]\+"/version = "{{VERSION}}"/' pyproject.toml
    @# Update Cargo.lock
    @cargo check > /dev/null 2>&1 || true
    @git add Cargo.toml Cargo.lock pyproject.toml
    @git commit -m "release: bump version to {{VERSION}}"
    @echo "Bumped version to {{VERSION}}"
