# expman-rs Justfile
# Run `just` to see available commands

default:
    @just --list

# Setup development environment
setup:
    pip install -e .
    just build

# Start development workflow (alias for dev-py)
dev: dev-py

# Build all crates
build:
    cargo build --workspace

# Build in release mode
build-release:
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

# Build and install the Python extension (for development)
dev-py:
    maturin develop --manifest-path crates/expman-py/Cargo.toml

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

# Run clippy
lint:
    cargo clippy --workspace --all-targets -- -D warnings

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
