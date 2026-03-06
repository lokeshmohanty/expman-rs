#[cfg(target_arch = "wasm32")]
use expman::app::App;
#[cfg(target_arch = "wasm32")]
use leptos::prelude::*;

#[cfg(target_arch = "wasm32")]
fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    // This binary is only for WASM
}
