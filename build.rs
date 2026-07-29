//! Builds the dashboard frontend when the `server` feature is on.
//!
//! **This build script never fails the build.** It used to `exit(1)` when trunk
//! was missing or failed with no `dist/` present, which meant `cargo build
//! --all-features` on a clean checkout died at build-script time with an error
//! that never said "install trunk". Every workaround in this repo — the
//! `EXPMAN_SKIP_FRONTEND_BUILD` escape hatch, `just prep-dist`, the `CARGO_DOC`
//! special case, CI downloading a prebuilt `dist/` — descends from that one
//! branch.
//!
//! Instead, a missing or broken frontend produces a placeholder `dist/` and a
//! loud `cargo:warning`. The binary still builds and still runs; the dashboard
//! serves a page that says how to build it properly. A broken *frontend* is not
//! a reason to be unable to build the *CLI*.

use std::path::Path;

/// The page served when the real frontend was never built. Deliberately
/// self-explanatory: whoever hits this needs one command, not a stack trace.
const PLACEHOLDER_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>ExpMan — frontend not built</title>
  <style>
    body { font-family: ui-sans-serif, system-ui, sans-serif; background: #020617;
           color: #e2e8f0; margin: 0; display: grid; place-items: center;
           min-height: 100vh; line-height: 1.6; }
    main { max-width: 34rem; padding: 2rem; }
    h1 { font-weight: 600; letter-spacing: -0.01em; }
    code { font-family: ui-monospace, monospace; background: #1e293b;
           padding: 0.15rem 0.4rem; border-radius: 0.25rem; }
    pre { background: #0f172a; border: 1px solid #1e293b; border-radius: 0.5rem;
          padding: 1rem; overflow-x: auto; }
    p.muted { color: #94a3b8; font-size: 0.9rem; }
  </style>
</head>
<body>
  <main>
    <h1>Dashboard frontend not built</h1>
    <p>This binary was compiled without a usable <code>dist/</code>, so the web
       UI is a placeholder. The API and the <code>exp</code> CLI work normally.</p>
    <pre>just build-frontend    # or: trunk build --release</pre>
    <p class="muted">Then rebuild with <code>--features server</code>. Building the
       frontend needs <code>trunk</code> and <code>tailwindcss</code> on PATH;
       <code>nix develop</code> provides both.</p>
  </main>
</body>
</html>
"#;

/// Write a minimal `dist/` so `rust_embed` has something to embed.
///
/// `frontend.rs` embeds `dist/` at compile time and fails to compile if the
/// directory does not exist, so this is what keeps a frontend-less build
/// possible at all.
fn write_placeholder_dist(reason: &str) {
    println!("cargo:warning=Dashboard frontend unavailable ({reason}). Building with a placeholder page; run `just build-frontend` for the real UI.");
    if std::fs::create_dir_all("dist").is_err() {
        return;
    }
    let _ = std::fs::write("dist/index.html", PLACEHOLDER_HTML);
}

fn main() {
    // Only rerun if app source or config changes
    println!("cargo:rerun-if-changed=src/app");
    println!("cargo:rerun-if-changed=Trunk.toml");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-changed=tailwind.config.js");

    let is_server = std::env::var("CARGO_FEATURE_SERVER").is_ok();
    let is_wasm = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default() == "wasm32";
    let skip_frontend = std::env::var("EXPMAN_SKIP_FRONTEND_BUILD").is_ok();

    if !is_server || is_wasm {
        return;
    }

    let has_dist = Path::new("dist/index.html").exists();

    if skip_frontend {
        // The CI and fast-iteration path: a prebuilt dist/ is expected, but its
        // absence is still only a warning.
        if !has_dist {
            write_placeholder_dist(
                "EXPMAN_SKIP_FRONTEND_BUILD is set and dist/index.html is missing",
            );
        }
        return;
    }

    eprintln!("expman: Building dashboard frontend with trunk...");
    let status = std::process::Command::new("trunk")
        .env("CARGO_TARGET_DIR", "target/wasm_build")
        .env_remove("MAKEFLAGS")
        .env_remove("CARGO_MAKEFLAGS")
        .args(["build", "--release"])
        .stderr(std::process::Stdio::inherit())
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("cargo:warning=Dashboard frontend built successfully.");
        }
        // Keep whatever was already built rather than overwriting it with a
        // placeholder — a stale dashboard beats no dashboard.
        _ if has_dist => {
            println!("cargo:warning=Trunk build failed; using the existing dist/ directory.");
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            write_placeholder_dist("trunk is not installed")
        }
        Err(e) => write_placeholder_dist(&format!("trunk could not be run: {e}")),
        Ok(s) => write_placeholder_dist(&format!("trunk exited with {s}")),
    }
}
