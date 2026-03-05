//! Settings page.

use leptos::prelude::*;

#[component]
pub(crate) fn SettingsPage() -> impl IntoView {
    let window = web_sys::window().expect("no global `window` exists");
    let local_storage = window
        .local_storage()
        .expect("no local storage exists")
        .expect("no local storage exists");
    let initial_debug =
        local_storage.get_item("debug_enabled").unwrap_or_default() == Some("true".to_string());

    let (debug_enabled, set_debug_enabled) = signal(initial_debug);

    Effect::new(move |_| {
        let val = debug_enabled.get();
        let _ = local_storage.set_item("debug_enabled", if val { "true" } else { "false" });
    });

    view! {
        <div class="space-y-6">
            <h1 class="text-3xl font-bold text-white">"Settings"</h1>
            <div class="bg-slate-900 border border-slate-800 rounded-2xl p-6 space-y-6">
                <div class="flex items-center justify-between">
                    <div>
                        <h3 class="text-lg font-medium text-white">"Debug Logs"</h3>
                        <p class="text-sm text-slate-400">"Show detailed debug messages in the browser console. Requires page reload."</p>
                    </div>
                    <button
                        on:click=move |_| set_debug_enabled.update(|v| *v = !*v)
                        class=move || format!(
                            "w-12 h-6 rounded-full transition-colors relative {}",
                            if debug_enabled.get() { "bg-blue-600" } else { "bg-slate-700" }
                        )
                    >
                        <div class=move || format!(
                            "absolute top-1 left-1 w-4 h-4 bg-white rounded-full transition-transform {}",
                            if debug_enabled.get() { "translate-x-6" } else { "" }
                        )></div>
                    </button>
                </div>
            </div>
        </div>
    }.into_any()
}
