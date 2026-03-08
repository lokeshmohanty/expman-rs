fn main() {
    // Only rerun if app source or config changes
    println!("cargo:rerun-if-changed=src/app");
    println!("cargo:rerun-if-changed=Trunk.toml");
    println!("cargo:rerun-if-changed=Cargo.toml");

    // Only attempt to build frontend if server feature is enabled
    // and we are not building the WASM app itself
    let is_server = std::env::var("CARGO_FEATURE_SERVER").is_ok();
    let is_wasm = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default() == "wasm32";
    let skip_frontend = std::env::var("EXPMAN_SKIP_FRONTEND_BUILD").is_ok();
    let is_doc = std::env::var("CARGO_DOC").is_ok();

    if is_server && !is_wasm && !skip_frontend {
        eprintln!("expman: Building dashboard frontend with trunk...");
        let status = std::process::Command::new("trunk")
            .env("CARGO_TARGET_DIR", "target/wasm_build")
            .env_remove("MAKEFLAGS")
            .env_remove("CARGO_MAKEFLAGS")
            .args(["build", "--release"])
            .stderr(std::process::Stdio::inherit())
            .status();

        if matches!(status, Ok(s) if s.success()) {
            println!("cargo:warning=Dashboard frontend built successfully.");
        } else {
            // Check if we have an existing dist/ directory as a fallback
            if std::path::Path::new("dist/index.html").exists() {
                println!("cargo:warning=Trunk build failed, but using existing dist/ directory.");
            } else if is_doc {
                println!("cargo:warning=Trunk build failed during documentation build. Creating placeholders.");
                std::fs::create_dir_all("dist").ok();
                std::fs::write(
                    "dist/index.html",
                    "<html><body>Placeholder for doc build</body></html>",
                )
                .ok();
                std::fs::write("dist/app.js", "").ok();
                std::fs::write("dist/app.wasm", "").ok();
                std::fs::write("dist/style.css", "").ok();
            } else {
                eprintln!("Error: Dashboard frontend build failed and no existing 'dist/index.html' found.");
                eprintln!("The 'server' feature requires the frontend to be built.");
                std::process::exit(1);
            }
        }
    } else if is_server && !is_wasm && skip_frontend {
        // Just verify that dist/index.html exists when build is skipped
        if !std::path::Path::new("dist/index.html").exists() {
            eprintln!(
                "Error: 'EXPMAN_SKIP_FRONTEND_BUILD' is set but 'dist/index.html' is missing."
            );
            std::process::exit(1);
        }
    }
}
