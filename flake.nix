{
  description = "expman-rs: High-performance experiment manager in Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, fenix, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ fenix.overlays.default ];
        };

        # Rust toolchain: stable with extras
        rustToolchain = pkgs.fenix.stable.withComponents [
          "cargo"
          "clippy"
          "rust-src"
          "rustc"
          "rustfmt"
          "rust-analyzer"
        ];

        # Combined toolchain with WASM target
        fullToolchain = pkgs.fenix.combine [
          rustToolchain
          pkgs.fenix.targets.wasm32-unknown-unknown.stable.rust-std
        ];

        # Base Python for uv
        pythonBase = pkgs.python312;

      in {
        devShells.default = pkgs.mkShell {
          name = "expman-rs-dev";

          packages = [
            # Rust
            fullToolchain
            pkgs.cargo-watch      # cargo watch -x test
            pkgs.cargo-nextest    # faster test runner
            pkgs.cargo-edit       # cargo add/rm
            pkgs.cargo-expand     # macro expansion debugging

            # Python + maturin for PyO3 builds
            pythonBase
            pkgs.uv
            pkgs.maturin

            # Build tools
            pkgs.pkg-config
            pkgs.openssl

            # Dev utilities
            pkgs.just             # justfile task runner
            pkgs.hyperfine        # benchmarking
            pkgs.tokei            # code stats
            pkgs.trunk            # WASM builder
            pkgs.wasm-bindgen-cli # WASM glue
            pkgs.pre-commit       # pre-commit hooks
            pkgs.ruff             # Python linter
            pkgs.python312Packages.pytest # Python testing
          ];

          # Environment variables
          RUST_LOG = "debug";
          RUST_BACKTRACE = "1";

          # Ensure maturin can find Python
          PYO3_PYTHON = "${pythonBase}/bin/python3";

          # We'll use uv to manage the environment now

          shellHook = ''
            echo "🦀 expman-rs dev environment (uv-managed)"
            echo "  Rust: $(rustc --version)"
            echo "  Python: $(python3 --version)"
            echo "  UV: $(uv --version)"
            echo ""
            echo "Commands:"
            echo "  just build                 - build everything (Rust + Python)"
            echo "  just test                  - run all tests"
            echo "  cargo watch -x 'nextest run' - watch mode"
            echo "  just dev-py                - build Python extension (uv pip install -e .)"
            echo "  just serve ./experiments"
            echo ""

            if [ ! -d ".venv" ]; then
              echo "💡 Run 'just dev-py' to initialize the uv virtual environment."
            fi
          '';
        };

        # Package: the CLI binary
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "expman";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          buildInputs = [ pkgs.openssl ];
          nativeBuildInputs = [ pkgs.pkg-config ];
          cargoBuildFlags = [ "-p" "expman-cli" ];
        };
      });
}
