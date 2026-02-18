// build.rs for expman-server
//
// rust_embed requires the embedded folder to exist at compile time.
// When building in CI (test/lint only, no `trunk build` run), frontend/dist
// won't exist yet. We create an empty placeholder so the crate always compiles.

use std::path::Path;

fn main() {
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../frontend/dist");

    if !dist.exists() {
        std::fs::create_dir_all(&dist).expect("failed to create placeholder frontend/dist");
        // Write a minimal index.html so rust_embed has at least one file
        std::fs::write(
            dist.join("index.html"),
            "<!-- placeholder: run `just build-frontend` to build the real dashboard -->\n",
        )
        .expect("failed to write placeholder index.html");
    }

    // Re-run this script only if the dist directory changes
    println!("cargo:rerun-if-changed=../../frontend/dist");
}
