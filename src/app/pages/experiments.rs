//! Experiments listing page.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::app::fetch::*;

#[component]
pub(crate) fn Experiments() -> impl IntoView {
    let experiments = LocalResource::new(fetch_experiments);

    view! {
        <div class="space-y-6">
            <h1 class="text-3xl font-bold">"Experiments"</h1>
            <div class="bg-slate-900 border border-slate-800 rounded-xl overflow-hidden">
                <table class="w-full text-left border-collapse">
                    <thead>
                        <tr class="bg-slate-800/50">
                            <th class="px-6 py-4 font-semibold text-slate-300">"Name"</th>
                            <th class="px-6 py-4 font-semibold text-slate-300">"Description"</th>
                            <th class="px-6 py-4 font-semibold text-slate-300">"Tags"</th>
                            <th class="px-6 py-4 font-semibold text-slate-300">"Runs"</th>
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-slate-800">
                        <Suspense fallback=|| view! { <tr><td colspan="4" class="px-6 py-10 text-center text-slate-500">"Loading..."</td></tr> }>
                            {move || Suspend::new(async move {
                                let exps = experiments.get().and_then(|r| r.ok()).unwrap_or_default();
                                view! {
                                    {exps.into_iter().map(|exp| {
                                        let id = exp.id.clone();
                                        view! {
                                            <tr class="hover:bg-slate-800/30 transition-colors cursor-pointer" on:click=move |_| {
                                                 // Navigate to details on row click
                                            }>
                                                <td class="px-6 py-4 font-medium">
                                                    <A href=format!("/experiments/{}", id) attr:class="text-blue-400 hover:underline">{exp.display_name}</A>
                                                </td>
                                                <td class="px-6 py-4 text-slate-400 text-sm">{exp.description.unwrap_or_default()}</td>
                                                <td class="px-6 py-4">
                                                    <div class="flex flex-wrap gap-1">
                                                        {exp.tags.into_iter().map(|t| view! {
                                                            <span class="px-2 py-0.5 bg-slate-800 text-slate-400 rounded text-[10px]">{t}</span>
                                                        }).collect_view()}
                                                    </div>
                                                </td>
                                                <td class="px-6 py-4 text-slate-300 text-sm font-mono">{exp.runs_count}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                }
                            })}
                        </Suspense>
                    </tbody>
                </table>
            </div>
        </div>
    }
}
