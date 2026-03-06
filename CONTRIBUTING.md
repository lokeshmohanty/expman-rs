# Contributing to expman-rs

First off, thank you for considering contributing to `expman-rs`!

## Development Setup

We orchestrate the development environment using Nix and `uv`, ensuring reproducible installations of Rust and Python toolchains.

### Prerequisites
- [Nix](https://nixos.org/download.html) (optional but highly recommended for reproducible environments)
- [Just](https://github.com/casey/just) command runner

### Getting Started

1. **Enter the development environment**:
   ```bash
   nix develop
   ```
   This drops you into a shell with `cargo`, `rustc`, `python` 3.12, `trunk`, `just`, and `uv` pre-configured.

2. **Build the Python extension & CLI for development**:
   ```bash
   just dev-py
   ```
   *Note: This automatically compiles the Rust components, copies the bundled binary to `wrappers/python/expman/bin/`, and builds the Python bindings.*

3. **Running tests**:
   ```bash
   just test
   ```

### Important: Local Git Configuration

To ensure a seamless installation experience across both `cargo` and `pip`, the `exp` binary is compiled and then securely **bundled directly inside the Python package** prior to building the wheel.

Because `maturin` respects `.gitignore` rules, we **cannot** put the bundled binary path inside the repository's `.gitignore` file.

To prevent the compiled binary from cluttering your local `git status` while still allowing `maturin` to discover and package it properly, you must add it to your local git exclude list exactly **once** after cloning:

```bash
echo "wrappers/python/expman/bin/" >> .git/info/exclude
```

This ensures your workspace remains pristine locally without breaking the CI or packaging pipeline!

### Commands Reference
- `just build`: Build everything (Rust + Python + Frontend)
- `cargo watch -x 'nextest run'`: Watch mode for tests
- `just serve ./experiments`: Start dashboard
- `just build-docs`: Build and open documentation
