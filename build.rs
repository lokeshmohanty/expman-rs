fn main() {
    // Only rerun if app source or config changes
    println!("cargo:rerun-if-changed=src/app");
    println!("cargo:rerun-if-changed=Trunk.toml");
    println!("cargo:rerun-if-changed=Cargo.toml");

    // Only attempt to build frontend if server feature is enabled
    // and we are not building the WASM app itself
    let is_server = std::env::var("CARGO_FEATURE_SERVER").is_ok();
    let is_wasm = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default() == "wasm32";

    if is_server && !is_wasm {
        // Run trunk build --release
        // We use 'release' because we want to embed optimized assets
        let status = std::process::Command::new("trunk")
            .args(["build", "--release"])
            .status();

        match status {
            Ok(s) if s.success() => {
                println!("cargo:warning=Dashboard frontend built successfully.");
            }
            Ok(s) => {
                println!("cargo:warning=Trunk build failed with status {}. Dashboard might be stale or missing.", s);
            }
            Err(e) => {
                println!("cargo:warning=Failed to execute trunk: {}. Dashboard might be missing. Ensure trunk is installed: cargo install trunk", e);
            }
        }
    }
}
