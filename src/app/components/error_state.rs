//! Reusable error state component for displaying friendly, actionable error cards.

use leptos::prelude::*;
use lucide_leptos::{RefreshCw, TriangleAlert};

#[component]
pub(crate) fn ErrorState(
    #[prop(into)] title: String,
    #[prop(into)] message: String,
    #[prop(optional, into)] action_label: Option<String>,
    #[prop(optional)] on_action: Option<Callback<()>>,
) -> impl IntoView {
    let action_cb = on_action;
    let has_action = action_label.is_some() && action_cb.is_some();
    let action_text = action_label.unwrap_or_else(|| "Retry".to_string());

    view! {
        <div class="flex flex-col items-center justify-center p-8 text-center h-full w-full max-w-md mx-auto">
            <div class="w-12 h-12 rounded-full bg-red-500/10 border border-red-500/20 text-red-400 flex items-center justify-center mb-4 flex-shrink-0">
                <TriangleAlert size=24 />
            </div>
            <h3 class="text-base font-semibold text-slate-200 mb-1">{title}</h3>
            <p class="text-xs text-slate-400 leading-relaxed mb-4 max-w-xs">{message}</p>
            {if has_action {
                let cb = action_cb.unwrap();
                view! {
                    <button
                        on:click=move |_| cb.run(())
                        class="inline-flex items-center space-x-1.5 px-3 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-medium rounded-lg border border-slate-700 transition-colors shadow-sm"
                    >
                        <span class="text-slate-400"><RefreshCw size=12 /></span>
                        <span>{action_text}</span>
                    </button>
                }.into_any()
            } else {
                view! { <span class="hidden"></span> }.into_any()
            }}
        </div>
    }
}
