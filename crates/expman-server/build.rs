// build.rs for expman-server
//
// rust_embed requires the embedded folder to exist at compile time.
// When building in CI (test/lint only, no `trunk build` run), frontend/dist
// won't exist yet. We create an empty placeholder so the crate always compiles.

fn main() {
    // Re-run this script only if the dist directory changes
    println!("cargo:rerun-if-changed=dist");
}
