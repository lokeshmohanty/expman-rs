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

        # Python environment for development
        pythonEnv = pkgs.python312.withPackages (ps: with ps; [
          pip
          pytest
          numpy
        ]);

      in {
        devShells.default = pkgs.mkShell {
          name = "expman-rs-dev";

          packages = [
            # Rust
            rustToolchain
            pkgs.cargo-watch      # cargo watch -x test
            pkgs.cargo-nextest    # faster test runner
            pkgs.cargo-edit       # cargo add/rm
            pkgs.cargo-expand     # macro expansion debugging

            # Python + maturin for PyO3 builds
            pythonEnv
            pkgs.maturin

            # Build tools
            pkgs.pkg-config
            pkgs.openssl

            # Dev utilities
            pkgs.just             # justfile task runner
            pkgs.hyperfine        # benchmarking
            pkgs.tokei            # code stats
          ];

          # Environment variables
          RUST_LOG = "debug";
          RUST_BACKTRACE = "1";

          # Ensure maturin can find Python
          PYO3_PYTHON = "${pythonEnv}/bin/python3";

          shellHook = ''
            echo "🦀 expman-rs dev environment"
            echo "  Rust: $(rustc --version)"
            echo "  Python: $(python3 --version)"
            echo "  Maturin: $(maturin --version)"
            echo ""
            echo "Commands:"
            echo "  cargo nextest run          - run all tests"
            echo "  cargo watch -x 'nextest run' - watch mode"
            echo "  maturin develop            - build + install Python extension"
            echo "  cargo run -p expman-cli -- serve ./experiments"
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
