//! Reusable zoom controls.

use leptos::prelude::*;
use lucide_leptos::{Minus, Plus, RotateCcw};

/// Reusable zoom controls for previews and console output.
#[component]
pub(crate) fn ZoomControls(
    zoom_scale: ReadSignal<f64>,
    set_zoom_scale: WriteSignal<f64>,
    #[prop(default = 14)] size: usize,
) -> impl IntoView {
    view! {
        <div class="flex items-center bg-slate-800 rounded-lg p-0.5 border border-slate-700">
            <button
                on:click=move |_| set_zoom_scale.update(|z| *z = (*z - 0.1).max(0.1))
                class="p-1 hover:bg-slate-700 rounded text-slate-400 hover:text-white transition-colors"
                title="Zoom Out"
            >
                <Minus size=size />
            </button>
            <span class="px-2 text-[10px] font-mono text-slate-300 min-w-[45px] text-center">
                {move || format!("{:.0}%", zoom_scale.get() * 100.0)}
            </span>
            <button
                on:click=move |_| set_zoom_scale.update(|z| *z = (*z + 0.1).min(5.0))
                class="p-1 hover:bg-slate-700 rounded text-slate-400 hover:text-white transition-colors"
                title="Zoom In"
            >
                <Plus size=size />
            </button>
            <button
                on:click=move |_| set_zoom_scale.set(1.0)
                class="ml-1 p-1 hover:bg-slate-700 rounded text-slate-400 hover:text-white transition-colors border-l border-slate-700"
                title="Reset Zoom"
            >
                <RotateCcw size=size />
            </button>
        </div>
    }
}
