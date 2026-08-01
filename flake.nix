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

        # Single source of truth for the version. Everything else — the Python
        # wheel (via maturin's dynamic version) and both packages below — reads
        # it from Cargo.toml, so `just bump` edits one file and nothing can drift.
        version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;

      in
      {
        devShells.default = pkgs.mkShell {
          name = "expman-rs";

          packages = [
            fullToolchain
            pkgs.pkg-config
            pkgs.openssl
            pkgs.just
            # Must match the `wasm-bindgen` crate version in Cargo.lock exactly.
            # trunk normally downloads the matching binary itself, but the package
            # build below runs with TRUNK_OFFLINE=true and so uses this one; a
            # mismatch fails with "linked against version X ... this binary is
            # version Y". `pkgs.wasm-bindgen-cli` tracks nixpkgs, which drifts
            # ahead of us, so pin the versioned attribute and bump it in step with
            # Cargo.lock.
            pkgs.trunk
            pkgs.wasm-bindgen-cli_0_2_108
            # Matches the Trunk.toml [tools] pin, so a dev-shell build and a
            # network build produce the same stylesheet.
            pkgs.tailwindcss_3
            pkgs.uv
            pkgs.maturin
            pkgs.protobuf
            pkgs.cargo-nextest
            pkgs.zola
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
            inherit version;
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;

            buildInputs = [ pkgs.openssl ];
            nativeBuildInputs = [
              pkgs.pkg-config
              pkgs.lld
              # Must match the `wasm-bindgen` crate version in Cargo.lock exactly.
              # trunk normally downloads the matching binary itself, but the package
              # build below runs with TRUNK_OFFLINE=true and so uses this one; a
              # mismatch fails with "linked against version X ... this binary is
              # version Y". `pkgs.wasm-bindgen-cli` tracks nixpkgs, which drifts
              # ahead of us, so pin the versioned attribute and bump it in step with
              # Cargo.lock.
              pkgs.trunk
              pkgs.wasm-bindgen-cli_0_2_108
              # Trunk builds the stylesheet with tailwindcss. Offline it will not
              # download the pinned binary, so it must come from here — and the
              # version must match Trunk.toml's [tools] pin or the two builds
              # generate different CSS.
              pkgs.tailwindcss_3
              pkgs.binaryen
              pkgs.protobuf
              fullToolchain
            ];

            # Build the frontend before the main package
            # Setting TRUNK_OFFLINE=true ensures it uses the Nix-provided wasm-bindgen-cli
            preBuild = ''
              export HOME=$TMPDIR
              export TRUNK_OFFLINE=true
              export TRUNK_BUILD_WASM_OPT=false
              export TRUNK_TOOLS_TAILWINDCSS=${pkgs.tailwindcss_3.version}
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
              inherit version;
              format = "pyproject";
              src = ./.;
              postPatch = "cp ../../Cargo.lock .";
              postUnpack = ''
                export sourceRoot=$sourceRoot/wrappers/python
              '';
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
