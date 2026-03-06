//! 404 Not Found page.

use leptos::prelude::*;
use leptos_router::components::A;

#[component]
pub(crate) fn NotFound() -> impl IntoView {
    view! {
        <div class="flex flex-col items-center justify-center h-full space-y-4">
            <h1 class="text-4xl font-bold">"404"</h1>
            <p class="text-slate-400">"Page not found"</p>
            <A href="/" attr:class="text-blue-400 hover:underline">"Back to Dashboard"</A>
        </div>
    }
    .into_any()
}
