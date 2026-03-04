//! WASM entry point for the Leptos frontend dashboard.
//!
//! This binary is compiled to WASM via trunk and served as the embedded frontend.

#[cfg(target_arch = "wasm32")]
fn main() {
    use expman_server::app::App;
    use leptos::prelude::*;

    let window = web_sys::window().expect("no global `window` exists");
    let local_storage = window
        .local_storage()
        .expect("no local storage exists")
        .expect("no local storage exists");
    let debug_enabled =
        local_storage.get_item("debug_enabled").unwrap_or_default() == Some("true".to_string());

    let level = if debug_enabled {
        log::Level::Debug
    } else {
        log::Level::Info
    };
    _ = console_log::init_with_level(level);
    console_error_panic_hook::set_once();
    mount_to_body(App);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("This binary is for WASM only. Use expman-cli to start the server.");
    std::process::exit(1);
}
