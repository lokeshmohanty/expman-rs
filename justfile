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
test: test-py
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

# Build documentation with a custom landing page from README.md
build-docs:
    @echo "Building Rust documentation..."
    cargo doc --no-deps --workspace
    @echo "Generating landing page from README.md..."
    @mkdir -p target/doc
    @cp -r assets target/doc/ 2>/dev/null || true
    @npx -y marked -i README.md -o target/doc/readme_content.html
    @echo '<!DOCTYPE html> \
    <html lang="en"> \
    <head> \
        <meta charset="UTF-8"> \
        <meta name="viewport" content="width=device-width, initial-scale=1.0"> \
        <title>ExpMan Documentation</title> \
        <link rel="stylesheet" href="rustdoc.css" id="mainThemeStyle"> \
        <style> \
            body { max-width: 900px; margin: 0 auto; padding: 40px; background: #0f172a; color: #e2e8f0; font-family: sans-serif; line-height: 1.6; } \
            .container { background: rgba(30, 41, 59, 0.5); padding: 40px; border-radius: 12px; border: 1px solid #334155; } \
            a { color: #3b82f6; text-decoration: none; } \
            a:hover { text-decoration: underline; } \
            pre { background: #020617; padding: 16px; border-radius: 8px; overflow-x: auto; border: 1px solid #1e293b; } \
            code { font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 0.9em; } \
            img { max-width: 100%; height: auto; border-radius: 8px; margin: 20px 0; } \
            h1, h2, h3 { color: #f8fafc; border-bottom: 1px solid #334155; padding-bottom: 8px; margin-top: 40px; } \
            .nav { margin-bottom: 30px; display: flex; gap: 10px; border-bottom: 1px solid #334155; padding-bottom: 20px; } \
            .nav a { background: #334155; color: #f8fafc; padding: 8px 16px; border-radius: 6px; font-weight: 600; font-size: 0.85em; transition: all 0.2s; border: 1px solid transparent; } \
            .nav a:hover { background: #3b82f6; border-color: #60a5fa; color: white; text-decoration: none; transform: translateY(-1px); } \
        </style> \
    </head> \
    <body> \
        <div class="nav"> \
            <a href="expman/index.html">CORE</a> \
            <a href="expman_cli/index.html">CLI</a> \
            <a href="expman_py/index.html">PYTHON</a> \
            <a href="expman_server/index.html">SERVER</a> \
            <a href="frontend/index.html">FRONTEND</a> \
        </div> \
        <div class="container">' > target/doc/index.html
    @cat target/doc/readme_content.html >> target/doc/index.html
    @echo '</div></body></html>' >> target/doc/index.html
    @rm target/doc/readme_content.html
    @echo "Documentation built at target/doc/index.html"

# Build the Python extension and copy the shared library to the package directory
build-py:
    @if [ ! -d ".venv" ]; then \
        uv venv --seed --python 3.12; \
    fi
    uv build --out-dir target/python-wheels
    # Copy the built library into the python package for local use without full install
    cp target/debug/libexpman_py.so python/expman/expman.so 2>/dev/null || cp target/debug/libexpman_py.dylib python/expman/expman.so 2>/dev/null || true

# Build and install the Python extension (auto-manages uv venv)
dev-py:
    @if [ ! -d ".venv" ]; then \
        echo "Creating virtual environment with uv..."; \
        uv venv --seed --python 3.12; \
    fi
    @# Note: we use 'uv run' to ensure maturin uses the venv
    uv pip install -e .
    uv pip install -e ".[dev]"

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
lint: lint-frontend lint-py
    cargo clippy --workspace --exclude frontend --all-targets -- -D warnings

# Run clippy on the frontend (requires wasm32-unknown-unknown target)
lint-frontend:
    cargo clippy -p frontend --target wasm32-unknown-unknown -- -D warnings

# Run Python linter (ruff)
lint-py:
    uv run --extra dev ruff check python/ examples/

# Run Python tests (pytest)
test-py:
    uv run --extra dev pytest python/tests

# Run the Rust logging example
example-rust:
    cargo run --example logging -p expman-cli

# Run the Python basic training example
example-py: dev-py
    uv run python examples/python/basic_training.py

# Full CI check
ci: fmt-check lint test lint-py test-py

# Clean build artifacts
clean:
    cargo clean
    rm -rf python/expman/*.so

# Publish to PyPI (requires UV_PUBLISH_TOKEN)
publish:
    uv build
    uv publish

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
    @# Update all internal dependency versions in crates
    @find crates -name "Cargo.toml" -exec sed -i 's/version = "[0-9.]\+"/version = "{{VERSION}}"/g' {} +
    @# Update pyproject.toml (project version)
    @sed -i 's/^version = "[0-9.]\+"/version = "{{VERSION}}"/' pyproject.toml
    @cargo check > /dev/null 2>&1 || true
    @git add Cargo.toml pyproject.toml crates/*/Cargo.toml
    @git commit -m "release: bump version to {{VERSION}}"
    @echo "Bumped version to {{VERSION}}"
