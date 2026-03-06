//! Dashboard page with stats overview.

use leptos::prelude::*;
use leptos_router::components::A;
use lucide_leptos::{ChevronRight, FlaskConical, LayoutDashboard, Package};

use crate::app::fetch::*;

#[component]
pub(crate) fn Dashboard() -> impl IntoView {
    let experiments = LocalResource::new(fetch_experiments);
    let stats = LocalResource::new(fetch_global_stats);

    view! {
        <div class="space-y-6">
            <h1 class="text-3xl font-bold text-white">"Dashboard Overview"</h1>

            <Suspense fallback=|| view! { <div class="animate-pulse grid grid-cols-1 md:grid-cols-3 gap-6"><div class="bg-slate-900 h-32 rounded-xl"></div><div class="bg-slate-900 h-32 rounded-xl"></div><div class="bg-slate-900 h-32 rounded-xl"></div></div> }>
                {move || Suspend::new(async move {
                    let s = stats.get().and_then(|r| r.ok()).unwrap_or_default();
                    let exps = experiments.get().and_then(|r| r.ok()).unwrap_or_default();

                    view! {
                        <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
                            <StatCard label="Total Experiments" value=s.total_experiments.to_string()>
                                <FlaskConical size=24 />
                            </StatCard>
                            <StatCard label="Active Runs" value=s.active_runs.to_string() >
                                <div class="relative">
                                    <LayoutDashboard size=24 />
                                    {move || (s.active_runs > 0).then(|| view! { <span class="absolute -top-1 -right-1 w-2 h-2 bg-green-500 rounded-full animate-ping"></span> })}
                                </div>
                            </StatCard>
                            <StatCard label="Total Storage" value="0 MB".to_string() >
                                <Package size=24 />
                            </StatCard>
                        </div>

                        <div class="bg-slate-900 border border-slate-800 rounded-xl p-6">
                            <h2 class="text-xl font-semibold mb-4 text-white">"Recent Experiments"</h2>
                            <div class="divide-y divide-slate-800">
                                {exps.into_iter().take(5).map(|exp| {
                                    let id = exp.id.clone();
                                    view! {
                                        <A href=format!("/experiments/{}", id) attr:class="flex items-center justify-between py-3 hover:bg-slate-800/30 transition-colors px-2 rounded-lg group text-slate-300">
                                            <div>
                                                <p class="font-medium text-slate-100">{exp.display_name}</p>
                                                <p class="text-sm text-slate-500">{exp.description.unwrap_or_default()}</p>
                                            </div>
                                            <div class="flex items-center space-x-4">
                                                <span class="text-xs text-slate-600 font-mono">{exp.runs_count} " runs"</span>
                                                <div class="text-slate-600 group-hover:text-blue-400 transition-colors">
                                                    <ChevronRight size=18 />
                                                </div>
                                            </div>
                                        </A>
                                    }
                                }).collect_view()}
                            </div>
                        </div>
                    }
                })}
            </Suspense>
        </div>
    }
}

#[component]
pub(crate) fn StatCard(label: &'static str, value: String, children: Children) -> impl IntoView {
    view! {
        <div class="bg-slate-900 border border-slate-800 rounded-xl p-6 flex items-center space-x-4">
            <div class="p-3 bg-slate-800 rounded-lg">
                {children()}
            </div>
            <div>
                <p class="text-sm text-slate-400">{label}</p>
                <p class="text-2xl font-bold text-white">{value}</p>
            </div>
        </div>
    }
}
