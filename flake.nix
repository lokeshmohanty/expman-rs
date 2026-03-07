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

  nixConfig = {
    extra-substituters = [ "https://lokeshmohanty.cachix.org" ];
    extra-trusted-public-keys = [
      "lokeshmohanty.cachix.org-1:XkCPbX2XsKzlr0P/MecvqruyTeOA8SzJzwMcCOfuLuI="
    ];
  };

  outputs =
    {
      self,
      nixpkgs,
      fenix,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
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

      in
      {
        devShells.default = pkgs.mkShell {
          name = "expman-rs";

          packages = [
            fullToolchain
            pkgs.pkg-config
            pkgs.openssl
            pkgs.just
            pkgs.trunk
            pkgs.wasm-bindgen-cli
            pkgs.uv
            pkgs.maturin
          ];

          RUST_LOG = "debug";
          RUST_BACKTRACE = "1";
          PYO3_PYTHON = "${pythonBase}/bin/python3";

          shellHook = ''
            echo "🦀 expman-rs dev environment"
          '';
        };

        packages = rec {
          # Rust CLI package (Backend + Integrated Frontend build)
          expman = pkgs.rustPlatform.buildRustPackage {
            pname = "expman";
            version = "0.4.8";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;

            buildInputs = [ pkgs.openssl ];
            nativeBuildInputs = [
              pkgs.pkg-config
              pkgs.lld
              pkgs.trunk
              pkgs.wasm-bindgen-cli
              pkgs.binaryen
              fullToolchain
            ];

            # Build the frontend before the main package
            # Setting TRUNK_OFFLINE=true ensures it uses the Nix-provided wasm-bindgen-cli
            preBuild = ''
              export HOME=$TMPDIR
              export TRUNK_OFFLINE=true
              export TRUNK_BUILD_WASM_OPT=false
              trunk build --release
            '';

            buildFeatures = [
              "cli"
              "server"
            ];
            cargoBuildFlags = [
              "--bin"
              "exp"
            ];

            # Tests are handled in CI
            doCheck = false;
          };

          # Python package
          python3Packages = {
            expman-rs = pkgs.python3.pkgs.buildPythonPackage {
              pname = "expman-rs";
              version = "0.4.8";
              format = "pyproject";
              src = ./.;
              postPatch = "cp Cargo.lock wrappers/python/";
              sourceRoot = "source/wrappers/python";
              nativeBuildInputs = [
                pkgs.maturin
                pkgs.rustPlatform.maturinBuildHook
                pkgs.rustPlatform.cargoSetupHook
                pkgs.cargo
                pkgs.rustc
              ];
              buildInputs = [ pkgs.openssl ];
              cargoDeps = pkgs.rustPlatform.importCargoLock {
                lockFile = ./Cargo.lock;
              };
            };
          };

          exp = expman;
          default = exp;
        };
      }
    );
}
