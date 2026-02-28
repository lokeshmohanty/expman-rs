use chrono::{DateTime, Local};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::{Route, Router, Routes, A};
use leptos_router::hooks::use_params_map;
use leptos_router::path;
use lucide_leptos::{
    ChevronRight, FlaskConical, LayoutDashboard, Package, Settings as SettingsIcon, TriangleAlert,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;

use std::rc::Rc;
#[derive(Clone, Copy)]
struct SidebarContext(RwSignal<Option<Rc<dyn Fn() -> AnyView>>, LocalStorage>);

fn format_date(iso: &str) -> String {
    if let Ok(dt) = DateTime::parse_from_rfc3339(iso) {
        let local = dt.with_timezone(&Local);
        local.format("%H:%M, %d %b, %Y").to_string()
    } else {
        iso.to_string()
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Default)]
pub struct Experiment {
    pub id: String,
    pub display_name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub runs_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Run {
    pub name: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_secs: Option<f64>,
    pub description: Option<String>,
    pub metrics: Option<std::collections::HashMap<String, f64>>,
    pub language: Option<String>,
    pub env_path: Option<String>,
}

async fn fetch_experiments() -> Result<Vec<Experiment>, String> {
    let resp = gloo_net::http::Request::get("/api/experiments")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!("Error fetching experiments: {}", resp.status()));
    }

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

async fn fetch_runs(exp_id: String) -> Result<Vec<Run>, String> {
    let resp = gloo_net::http::Request::get(&format!("/api/experiments/{}/runs", exp_id))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!("Error fetching runs: {}", resp.status()));
    }

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

async fn update_experiment_metadata(
    exp_id: String,
    display_name: Option<String>,
    description: Option<String>,
    tags: Option<Vec<String>>,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "display_name": display_name,
        "description": description,
        "tags": tags,
    });
    let resp = gloo_net::http::Request::patch(&format!("/api/experiments/{}/metadata", exp_id))
        .json(&payload)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!("Error updating metadata: {}", resp.status()));
    }
    Ok(())
}

#[allow(dead_code)]
async fn update_run_metadata(
    exp_id: String,
    run_id: String,
    name: Option<String>,
    description: Option<String>,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "name": name,
        "description": description,
    });
    let resp = gloo_net::http::Request::patch(&format!(
        "/api/experiments/{}/runs/{}/metadata",
        exp_id, run_id
    ))
    .json(&payload)
    .map_err(|e| e.to_string())?
    .send()
    .await
    .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!("Error updating run metadata: {}", resp.status()));
    }
    Ok(())
}

#[component]
fn App() -> impl IntoView {
    let sidebar_content = RwSignal::new_local(None);
    provide_context(SidebarContext(sidebar_content));

    view! {
        <Router>
            <div class="flex h-screen bg-slate-950 text-slate-100 font-sans">
                // Sidebar
                <nav class="w-64 border-r border-slate-800 flex flex-col p-4 bg-slate-900/50">
                    <div class="flex items-center space-x-3 px-2 py-6 mb-6">
                        <div class="p-2 bg-blue-600 rounded-lg shadow-lg shadow-blue-900/20">
                            <Package size=24 />
                        </div>
                        <span class="text-2xl font-bold tracking-tight text-white">"ExpMan"</span>
                    </div>

                    <div class="space-y-1">
                        <A href="/" attr:class="flex items-center space-x-3 px-4 py-3 rounded-xl hover:bg-slate-800 transition-all duration-200 text-slate-400 hover:text-white group">
                            <div class="group-hover:text-blue-400 transition-colors">
                                <LayoutDashboard size=20 />
                            </div>
                            <span class="font-medium">"Dashboard"</span>
                        </A>

                        <A href="/experiments" attr:class="flex items-center space-x-3 px-4 py-3 rounded-xl hover:bg-slate-800 transition-all duration-200 text-slate-400 hover:text-white group">
                            <div class="group-hover:text-blue-400 transition-colors">
                                <FlaskConical size=20 />
                            </div>
                            <span class="font-medium">"Experiments"</span>
                        </A>

                        <div class="pt-4 mt-4 border-t border-slate-800 empty:hidden">
                             {move || sidebar_content.get().map(|f| f())}
                        </div>
                    </div>

                    <div class="mt-auto">
                        <A href="/settings" attr:class="flex items-center space-x-3 px-4 py-3 rounded-xl hover:bg-slate-800 transition-all duration-200 text-slate-400 hover:text-white group">
                            <div class="group-hover:text-blue-400 transition-colors">
                                <SettingsIcon size=20 />
                            </div>
                            <span class="font-medium">"Settings"</span>
                        </A>
                    </div>
                </nav>

                // Main Content
                <main class="flex-grow overflow-auto p-8">
                    <Routes fallback=|| view! { <NotFound /> }.into_any()>
                        <Route path=path!("/") view=|| view! { <Dashboard /> } />
                        <Route path=path!("/experiments") view=|| view! { <Experiments /> } />
                        <Route path=path!("/experiments/:id") view=|| view! { <ExperimentDetail /> } />
                        <Route path=path!("/settings") view=|| view! { <SettingsPage /> } />
                    </Routes>
                </main>
            </div>
        </Router>
    }.into_any()
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Default)]
pub struct GlobalStats {
    pub total_experiments: usize,
    pub total_runs: usize,
    pub active_runs: usize,
    pub total_storage_bytes: u64,
}

async fn fetch_global_stats() -> Result<GlobalStats, String> {
    let resp = gloo_net::http::Request::get("/api/stats")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!("Error fetching stats: {}", resp.status()));
    }

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

#[component]
fn Dashboard() -> impl IntoView {
    let experiments = LocalResource::new(fetch_experiments);
    let stats = LocalResource::new(fetch_global_stats);

    view! {
        <div class="space-y-6">
            <h1 class="text-3xl font-bold text-white">"Dashboard Overview"</h1>

            <Suspense fallback=|| view! { <div class="animate-pulse grid grid-cols-1 md:grid-cols-3 gap-6"><div class="bg-slate-900 h-32 rounded-xl"></div><div class="bg-slate-900 h-32 rounded-xl"></div><div class="bg-slate-900 h-32 rounded-xl"></div></div> }>
                {move || Suspend::new(async move {
                    let s = stats.get().as_deref().cloned().unwrap_or(Ok(GlobalStats::default())).unwrap_or_default();
                    let exps = experiments.get().as_deref().cloned().unwrap_or(Ok(vec![])).unwrap_or_default();

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
fn StatCard(label: &'static str, value: String, children: Children) -> impl IntoView {
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

#[component]
fn Experiments() -> impl IntoView {
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
                                let exps = experiments.get().as_deref().cloned().unwrap_or(Ok(vec![])).unwrap_or_default();
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

#[component]
fn ExperimentDetail() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.read().get("id").unwrap_or_default();
    let sidebar_ctx = use_context::<SidebarContext>().expect("SidebarContext not found");

    let runs = LocalResource::new(move || {
        let exp_id = id();
        async move { fetch_runs(exp_id).await }
    });

    let (selected_runs, set_selected_runs) = signal(std::collections::HashSet::<String>::new());
    let (active_tab, set_active_tab) = signal("metrics".to_string());

    // Experiment Edit
    let (show_edit, set_show_edit) = signal(false);
    let (edit_name, set_edit_name) = signal("".to_string());
    let (edit_desc, set_edit_desc) = signal("".to_string());
    let (edit_tags, set_edit_tags) = signal("".to_string());

    // Run Edit
    let (show_run_edit, set_show_run_edit) = signal(false);
    let (edit_run_id, set_edit_run_id) = signal("".to_string());
    let (edit_run_name, set_edit_run_name) = signal("".to_string());
    let (edit_run_desc, set_edit_run_desc) = signal("".to_string());

    let toggle_run = move |name: String| {
        set_selected_runs.update(|set| {
            if set.contains(&name) {
                set.remove(&name);
            } else {
                set.insert(name);
            }
        });
    };

    let save_metadata = move |_| {
        let eid = id();
        let name = edit_name.get();
        let desc = edit_desc.get();
        let tags: Vec<String> = edit_tags
            .get()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        spawn_local(async move {
            let _ = update_experiment_metadata(eid, Some(name), Some(desc), Some(tags)).await;
            set_show_edit.set(false);
        });
    };

    let save_run_metadata = move |_| {
        let eid = id();
        let rid = edit_run_id.get();
        let name = edit_run_name.get();
        let desc = edit_run_desc.get();

        spawn_local(async move {
            let _ = update_run_metadata(eid, rid, Some(name), Some(desc)).await;
            set_show_run_edit.set(false);
            runs.refetch();
        });
    };

    let open_run_edit = move |r: Run| {
        set_edit_run_id.set(r.name.clone());
        set_edit_run_name.set(r.name);
        set_edit_run_desc.set(r.description.unwrap_or_default());
        set_show_run_edit.set(true);
    };

    async fn fetch_experiment_metadata(eid: String) -> Option<Experiment> {
        let resp = gloo_net::http::Request::get(&format!("/api/experiments/{}/metadata", eid))
            .send()
            .await;
        if let Ok(r) = resp {
            if let Ok(text) = r.text().await {
                serde_json::from_str(&text).ok()
            } else {
                None
            }
        } else {
            Option::<Experiment>::None
        }
    }

    let exp_metadata = LocalResource::new(move || {
        let eid = id();
        async move { fetch_experiment_metadata(eid).await }
    });

    // Sidebar View Effect
    Effect::new(move |_| {
        sidebar_ctx.0.set(Some(Rc::new(move || {
            view! {
            <div class="h-full flex flex-col">
                <div class="px-4 py-2 border-b border-slate-800 bg-slate-900/50">
                    <h2 class="font-bold text-slate-200 text-sm">"Select Runs"</h2>
                    <p class="text-[10px] text-slate-500">"Select to compare metrics"</p>
                </div>
                <div class="flex-grow overflow-auto p-2 space-y-1 custom-scrollbar">
                     <Suspense fallback=|| view! { <div class="p-4 text-slate-500 text-xs">"Loading runs..."</div> }>
                        {move || Suspend::new(async move {
                            let run_list: Vec<Run> = runs.await.unwrap_or_default();
                            view! {
                                {run_list.into_iter().map(|run| {
                                    let rid_inner = run.name.clone();
                                    let is_selected = Signal::derive(move || selected_runs.with(|set| set.contains(&rid_inner)));
                                    let is_running = run.status == "RUNNING";
                                    let run_clone = run.clone();
                                    let rid_click = run.name.clone();

                                    let duration = run.duration_secs.map(|d| format!("{:.0}s", d));

                                    view! {
                                        <div
                                            class=move || format!(
                                                "p-2 rounded-lg transition-all duration-200 border group/item relative pr-8 {} {}",
                                                if is_selected.get() { "bg-blue-600/10 border-blue-500/50" } else { "hover:bg-slate-800/50 border-transparent text-slate-400" },
                                                if is_selected.get() { "text-white" } else { "" }
                                            )
                                        >
                                            <div class="cursor-pointer" on:click=move |_| toggle_run(rid_click.clone())>
                                                <div class="flex items-center justify-between">
                                                    <div class="flex items-center space-x-2 overflow-hidden">
                                                        <div class=format!("w-1.5 h-1.5 rounded-full flex-shrink-0 {}", if is_running { "bg-green-500 animate-pulse shadow-[0_0_8px_rgba(34,197,94,0.6)]" } else { "bg-slate-600" })></div>
                                                        <span class="font-medium text-xs truncate">{run.name.clone()}</span>
                                                    </div>
                                                </div>
                                                <div class="mt-1 ml-3.5 space-y-0.5">
                                                    <p class="text-[10px] text-slate-500">{format_date(&run.started_at)}</p>
                                                    {duration.map(|d| view! { <p class="text-[9px] text-slate-600 font-mono">"Dur: " {d}</p> })}
                                                </div>
                                            </div>

                                            // Edit Button (visible on hover)
                                            <button
                                                on:click=move |e| {
                                                    e.stop_propagation();
                                                    open_run_edit(run_clone.clone());
                                                }
                                                class="absolute top-2 right-2 p-1 text-slate-600 hover:text-blue-400 opacity-0 group-hover/item:opacity-100 transition-opacity"
                                                title="Edit Run"
                                            >
                                                <SettingsIcon size=12 />
                                            </button>
                                        </div>
                                    }
                                }).collect_view()}
                            }
                        })}
                    </Suspense>

                </div>
            </div>
            }.into_any()
        })));
    });

    on_cleanup(move || sidebar_ctx.0.set(None));

    view! {
        <div class="space-y-6 relative h-full flex flex-col">
            // Edit Run Modal
            {move || show_run_edit.get().then(|| {
                view! {
                    <div class="fixed inset-0 bg-slate-950/80 backdrop-blur-sm z-50 flex items-center justify-center p-4">
                        <div class="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-lg shadow-2xl p-6 space-y-4">
                            <h2 class="text-xl font-bold text-white">"Edit Run Metadata"</h2>
                            <div class="space-y-4">
                                <div>
                                    <label class="block text-xs font-semibold text-slate-500 uppercase mb-1">"Run Name"</label>
                                    <input
                                        type="text"
                                        on:input=move |ev| set_edit_run_name.set(event_target_value(&ev))
                                        prop:value=edit_run_name
                                        class="w-full bg-slate-950 border border-slate-800 rounded-lg px-4 py-2 text-white focus:border-blue-500 outline-none"
                                    />
                                </div>
                                <div>
                                    <label class="block text-xs font-semibold text-slate-500 uppercase mb-1">"Description"</label>
                                    <textarea
                                        on:input=move |ev| set_edit_run_desc.set(event_target_value(&ev))
                                        prop:value=edit_run_desc
                                        class="w-full bg-slate-950 border border-slate-800 rounded-lg px-4 py-2 text-white h-32 focus:border-blue-500 outline-none"
                                        placeholder="Run description..."
                                    ></textarea>
                                </div>
                            </div>
                            <div class="flex justify-end space-x-3 pt-4">
                                <button on:click=move |_| set_show_run_edit.set(false) class="px-4 py-2 text-slate-400 hover:text-white transition-colors">"Cancel"</button>
                                <button on:click=save_run_metadata class="px-6 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-lg font-medium transition-colors">"Save Changes"</button>
                            </div>
                        </div>
                    </div>
                }
            })}

            // Edit Experiment Modal
            {move || show_edit.get().then(|| {
                view! {
                    <div class="fixed inset-0 bg-slate-950/80 backdrop-blur-sm z-50 flex items-center justify-center p-4">
                        <div class="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-lg shadow-2xl p-6 space-y-4">
                            <h2 class="text-xl font-bold text-white">"Edit Experiment Metadata"</h2>
                            <div class="space-y-4">
                                <div>
                                    <label class="block text-xs font-semibold text-slate-500 uppercase mb-1">"Display Name"</label>
                                    <input
                                        type="text"
                                        on:input=move |ev| set_edit_name.set(event_target_value(&ev))
                                        prop:value=edit_name
                                        class="w-full bg-slate-950 border border-slate-800 rounded-lg px-4 py-2 text-white focus:border-blue-500 outline-none"
                                        placeholder="Experiment Name"
                                    />
                                </div>
                                <div>
                                    <label class="block text-xs font-semibold text-slate-500 uppercase mb-1">"Description"</label>
                                    <textarea
                                        on:input=move |ev| set_edit_desc.set(event_target_value(&ev))
                                        prop:value=edit_desc
                                        class="w-full bg-slate-950 border border-slate-800 rounded-lg px-4 py-2 text-white h-32 focus:border-blue-500 outline-none"
                                        placeholder="Provide a detailed description..."
                                    ></textarea>
                                </div>
                                <div>
                                    <label class="block text-xs font-semibold text-slate-500 uppercase mb-1">"Tags (comma separated)"</label>
                                    <input
                                        type="text"
                                        on:input=move |ev| set_edit_tags.set(event_target_value(&ev))
                                        prop:value=edit_tags
                                        class="w-full bg-slate-950 border border-slate-800 rounded-lg px-4 py-2 text-white focus:border-blue-500 outline-none"
                                        placeholder="research, mnist, baseline"
                                    />
                                </div>
                            </div>
                            <div class="flex justify-end space-x-3 pt-4">
                                <button on:click=move |_| set_show_edit.set(false) class="px-4 py-2 text-slate-400 hover:text-white transition-colors">"Cancel"</button>
                                <button on:click=save_metadata class="px-6 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-lg font-medium transition-colors">"Save Changes"</button>
                            </div>
                        </div>
                    </div>
                }
            })}

            <div class="flex items-center justify-between pb-6 border-b border-slate-800 flex-shrink-0">
                <div class="space-y-4 max-w-2xl">
                    <h1 class="text-3xl font-bold text-white flex items-center space-x-3">
                        <div class="text-blue-500">< FlaskConical size=32 /></div>
                        <span>{id}</span>
                    </h1>
                    <Suspense fallback=|| view! { <div class="h-4 bg-slate-800 rounded w-1/2 animate-pulse"></div> }.into_any()>
                        {move || Suspend::new(async move {
                            let meta: Experiment = exp_metadata.get().as_deref().cloned().flatten().unwrap_or_default();
                            let count = runs.get().as_deref().cloned().unwrap_or(Ok(vec![])).map(|r| r.len()).unwrap_or(0);

                            view! {
                                <div class="space-y-2">
                                    <p class="text-slate-400 text-sm leading-relaxed">{meta.description.unwrap_or_else(|| "No description provided.".to_string())}</p>
                                    <div class="flex flex-wrap gap-2 pt-2">
                                        <div class="px-2 py-0.5 bg-blue-500/10 text-blue-400 rounded-md text-xs border border-blue-500/20 flex items-center space-x-1">
                                            <LayoutDashboard size=12 />
                                            <span>{count} " Runs"</span>
                                        </div>
                                        {meta.tags.into_iter().map(|tag| view! {
                                            <div class="px-2 py-0.5 bg-slate-800 text-slate-400 rounded-md text-xs border border-slate-700">
                                                {tag}
                                            </div>
                                        }).collect_view()}
                                    </div>
                                </div>
                            }.into_any()
                        })}
                    </Suspense>
                </div>
                <div class="flex space-x-2">
                    <button on:click=move |_| set_show_edit.set(true) class="px-4 py-2 bg-slate-800 hover:bg-slate-700 rounded-lg text-sm transition-colors border border-slate-700">
                        "Edit Metadata"
                    </button>
                    // New Run button removed
                </div>
            </div>

            <div class="flex-grow flex flex-col space-y-4 min-h-0">
                // Tabs
                <div class="flex space-x-1 bg-slate-900 border border-slate-800 p-1 rounded-xl w-fit flex-shrink-0">
                    {["runs", "metrics", "artifacts", "console", "interactive"].into_iter().map(|t| {
                        let tab = t.to_string();
                        let tab_click = tab.clone();
                        let is_active = move || active_tab.get() == tab;
                        view! {
                            <button
                                on:click=move |_| set_active_tab.set(tab_click.clone())
                                class=move || format!(
                                    "px-6 py-2 rounded-lg text-sm font-medium transition-all duration-200 {}",
                                    if is_active() { "bg-slate-800 text-white shadow-sm" } else { "text-slate-500 hover:text-slate-300" }
                                )
                            >
                                {t.to_uppercase()}
                            </button>
                        }
                    }).collect_view()}
                </div>

                // Content Area (Full Width)
                <div class="bg-slate-900 border border-slate-800 rounded-2xl flex-grow flex flex-col overflow-hidden min-h-0">
                    {move || match active_tab.get().as_str() {
                        "runs" => view! { <RunsTableView exp_id=id() /> }.into_any(),
                        "metrics" => view! { <MetricsView exp_id=id() selected=selected_runs.get() /> }.into_any(),
                        "artifacts" => view! { <ArtifactView exp_id=id() selected=selected_runs.get() /> }.into_any(),
                        "console" => view! { <ConsoleView exp_id=id() selected=selected_runs.get() /> }.into_any(),
                        "interactive" => view! { <InteractiveView exp_id=id() selected=selected_runs.get() /> }.into_any(),
                        _ => view! { <div class="p-8 text-slate-500 text-center">"Select a tab"</div> }.into_any(),
                    }}
                </div>
            </div>
        </div>
    }
}

#[component]
fn MetricsView(exp_id: String, selected: std::collections::HashSet<String>) -> impl IntoView {
    if selected.is_empty() {
        return view! {
            <div class="flex-grow flex flex-col items-center justify-center p-12 text-center space-y-4">
                <div class="p-4 bg-slate-800 rounded-full text-blue-500">
                    <LayoutDashboard size=48 />
                </div>
                <h3 class="text-xl font-bold text-white">"No Runs Selected"</h3>
                <p class="text-slate-400 max-w-sm">"Please select one or more runs from the left sidebar to visualize and compare metrics in real-time."</p>
            </div>
        }.into_any();
    }

    view! {
        <div class="flex-grow p-6 space-y-6 overflow-auto">
            <div class="grid grid-cols-1 gap-6">
                <div class="bg-slate-950 border border-slate-800 rounded-xl p-6 h-96 flex flex-col">
                    <div class="flex items-center justify-between mb-4">
                        <h4 class="text-sm font-semibold text-slate-300">"Metric Comparison"</h4>
                        <div class="flex space-x-3">
                             {selected.clone().into_iter().enumerate().map(|(i, s)| {
                                 let colors = ["#3b82f6", "#10b981", "#f59e0b", "#ef4444", "#8b5cf6"];
                                 let color = colors[i % colors.len()];
                                 view! {
                                     <div class="flex items-center space-x-1 text-[10px] text-slate-400">
                                         <span class=format!("w-2 h-2 rounded-full") style=format!("background-color: {}", color)></span>
                                         <span>{s}</span>
                                     </div>
                                 }
                             }).collect_view()}
                        </div>
                    </div>
                    <div class="flex-grow bg-slate-900/40 rounded-lg overflow-hidden relative border border-slate-800/50">
                        <LineChart exp_id=exp_id.clone() selected_runs=selected.clone() />
                    </div>
                </div>
            </div>

            <div class="bg-slate-950 border border-slate-800 rounded-xl p-6">
                 <h4 class="text-sm font-semibold text-slate-300 mb-4">"Selected Runs Summary"</h4>
                 <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                    {selected.into_iter().map(|run_name| {
                        view! {
                            <div class="p-4 bg-slate-900 border border-slate-800 rounded-lg">
                                <div class="flex items-center justify-between mb-2">
                                    <div class="flex items-center space-x-2">
                                        <div class="w-3 h-3 rounded-full bg-blue-500 shadow-[0_0_8px_rgba(59,130,246,0.3)]"></div>
                                        <span class="text-sm font-medium text-white">{run_name}</span>
                                    </div>
                                    <span class="text-[10px] text-slate-500 italic">"Active"</span>
                                </div>
                                <div class="space-y-1 text-[10px] text-slate-500">
                                    <p class="flex justify-between"><span>"Loss"</span> <span class="text-slate-300">"0.123"</span></p>
                                    <p class="flex justify-between"><span>"Accuracy"</span> <span class="text-slate-300">"98.2%"</span></p>
                                    <p class="flex justify-between"><span>"Step"</span> <span class="text-slate-300">"145"</span></p>
                                </div>
                            </div>
                        }
                    }).collect_view()}
                 </div>
            </div>
        </div>
    }.into_any()
}

use plotly::{
    common::Title,
    layout::{Axis, Margin},
    Layout, Plot, Scatter,
};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Plotly, js_name = newPlot)]
    fn new_plot(root: &JsValue, data: &JsValue, layout: &JsValue, config: &JsValue);
}

#[component]
fn LineChart(
    #[allow(unused_variables)] exp_id: String,
    selected_runs: std::collections::HashSet<String>,
) -> impl IntoView {
    let div_ref = NodeRef::<leptos::html::Div>::new();

    Effect::new(move |_| {
        if let Some(div) = div_ref.get() {
            let mut p = Plot::new();
            let layout = Layout::new()
                .margin(Margin::new().left(50).right(50).top(30).bottom(50))
                .show_legend(true)
                .paper_background_color("rgba(0,0,0,0)")
                .plot_background_color("rgba(0,0,0,0)")
                .font(plotly::common::Font::new().color("#94a3b8"))
                .x_axis(
                    Axis::new()
                        .title(Title::from("Step"))
                        .show_grid(true)
                        .grid_color("#1e293b"),
                )
                .y_axis(
                    Axis::new()
                        .title(Title::from("Value"))
                        .show_grid(true)
                        .grid_color("#1e293b"),
                );

            p.set_layout(layout);

            // Mock data for now - in real app this would fetch from backend/SSE
            for run_id in selected_runs.iter() {
                let x: Vec<f64> = (0..20).map(|i| i as f64).collect();
                let y: Vec<f64> = (0..20)
                    .map(|i| {
                        let base = (i as f64).sin();
                        base + (run_id.len() as f64 % 10.0) / 10.0
                    })
                    .collect();

                let trace = Scatter::new(x, y)
                    .name(run_id.as_str())
                    .mode(plotly::common::Mode::LinesMarkers);
                p.add_trace(trace);
            }

            // Serialize to JSON string using plotly's to_json
            let json_str = p.to_json();

            // Parse JSON string to JS object
            if let Ok(js_value) = js_sys::JSON::parse(&json_str) {
                let data =
                    js_sys::Reflect::get(&js_value, &"data".into()).unwrap_or(JsValue::UNDEFINED);
                let layout =
                    js_sys::Reflect::get(&js_value, &"layout".into()).unwrap_or(JsValue::UNDEFINED);
                let config =
                    js_sys::Reflect::get(&js_value, &"config".into()).unwrap_or(JsValue::UNDEFINED);

                let div_element: &web_sys::HtmlElement = &div;
                new_plot(&div_element.into(), &data, &layout, &config);
            } else {
                leptos::logging::error!("Failed to parse Plotly JSON");
            }
        }
    });

    view! {
        <div class="w-full h-full p-2">
            <div node_ref=div_ref class="w-full h-full"></div>
        </div>
    }
}

#[component]
fn TabularPreview(content: String) -> impl IntoView {
    // Try to parse as JSON first (backend sends {type: "parquet", data: [...]})
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
        if json["type"] == "parquet" {
            if let Some(data) = json["data"].as_array() {
                if data.is_empty() {
                    return view! { <div class="p-8 text-slate-500 italic">"No data available in this parquet file."</div> }.into_any();
                }

                let headers: Vec<_> = data[0]
                    .as_object()
                    .map(|obj| obj.keys().cloned().collect())
                    .unwrap_or_default();

                return view! {
                    <div class="overflow-auto max-h-full">
                        <table class="w-full text-left border-collapse min-w-max">
                            <thead class="sticky top-0 bg-slate-900 border-b border-slate-800">
                                <tr>
                                    {headers.iter().cloned().map(|h| view! {
                                        <th class="p-3 text-[10px] font-bold text-slate-400 uppercase tracking-wider">{h}</th>
                                    }).collect_view()}
                                </tr>
                            </thead>
                            <tbody class="divide-y divide-slate-800/50">
                                {data.iter().map(|row| {
                                    let fields: Vec<_> = headers.iter().map(|h| row[h].to_string().replace("\"", "")).collect();
                                    view! {
                                        <tr class="hover:bg-slate-800/30 transition-colors">
                                            {fields.into_iter().map(|f| view! {
                                                <td class="p-3 text-slate-300 font-mono">{f}</td>
                                            }).collect_view()}
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                }.into_any();
            }
        }
    }

    // Default: check if it looks like CSV
    if content.contains(',') && content.lines().count() > 1 {
        let lines: Vec<&str> = content.lines().collect();
        let headers: Vec<String> = lines[0].split(',').map(|s| s.trim().to_string()).collect();
        let rows: Vec<Vec<String>> = lines[1..]
            .iter()
            .map(|line| line.split(',').map(|s| s.trim().to_string()).collect())
            .collect();

        return view! {
            <div class="overflow-auto max-h-full">
                <table class="w-full text-left border-collapse min-w-max">
                    <thead class="sticky top-0 bg-slate-900 border-b border-slate-800">
                        <tr>
                            {headers.into_iter().map(|h| view! {
                                <th class="p-3 text-[10px] font-bold text-slate-400 uppercase tracking-wider">{h}</th>
                            }).collect_view()}
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-slate-800/50">
                        {rows.into_iter().map(|row| {
                            view! {
                                <tr class="hover:bg-slate-800/30 transition-colors">
                                    {row.into_iter().map(|f| view! {
                                        <td class="p-3 text-slate-300 font-mono">{f}</td>
                                    }).collect_view()}
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
        }.into_any();
    }

    // Fallback to text
    view! { <div class="whitespace-pre p-4">{content}</div> }.into_any()
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Artifact {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub ext: String,
}

async fn fetch_artifacts(exp_id: String, run_id: String) -> Result<Vec<Artifact>, String> {
    let resp = gloo_net::http::Request::get(&format!(
        "/api/experiments/{}/runs/{}/artifacts",
        exp_id, run_id
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!("Error fetching artifacts: {}", resp.status()));
    }

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

async fn fetch_artifact_content(
    exp_id: String,
    run_id: String,
    path: String,
) -> Result<String, String> {
    let resp = gloo_net::http::Request::get(&format!(
        "/api/experiments/{}/runs/{}/artifacts/content?path={}",
        exp_id, run_id, path
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!(
            "Error fetching artifact content: {}",
            resp.status()
        ));
    }

    resp.text().await.map_err(|e| e.to_string())
}

#[component]
fn ArtifactView(exp_id: String, selected: std::collections::HashSet<String>) -> impl IntoView {
    let run_id = selected.iter().next().cloned().unwrap_or_default();
    let (selected_path, set_selected_path) = signal("run.log".to_string());

    let exp_id_val = StoredValue::new(exp_id);
    let run_id_val = StoredValue::new(run_id);

    let artifact_resource = LocalResource::new(move || {
        let eid = exp_id_val.with_value(|v| v.clone());
        let rid = run_id_val.with_value(|v| v.clone());
        async move {
            if rid.is_empty() {
                return Ok(vec![]);
            }
            fetch_artifacts(eid, rid).await
        }
    });

    let content_resource = LocalResource::new(move || {
        let eid = exp_id_val.with_value(|v| v.clone());
        let rid = run_id_val.with_value(|v| v.clone());
        let path = selected_path.get();
        async move {
            if rid.is_empty() {
                return Ok("Select a run".to_string());
            }
            fetch_artifact_content(eid, rid, path).await
        }
    });

    if run_id_val.with_value(|v| v.is_empty()) {
        return view! { <div class="p-12 text-center text-slate-500">"Select a single run to browse artifacts."</div> }.into_any();
    }

    view! {
        <div class="flex h-full divide-x divide-slate-800">
            // Left: List
            <div class="w-1/3 overflow-auto bg-slate-900/30 p-2 space-y-1">
                <div class="p-2 text-xs font-bold text-slate-500 uppercase tracking-wider mb-2">"Files"</div>
                <Suspense fallback=|| view! { <div class="p-4 text-slate-500 text-sm">"Loading..."</div> }>
                    {move || Suspend::new(async move {
                        let list = artifact_resource.await.unwrap_or_default();
                        view! {
                            <div class="space-y-1">
                                {list.into_iter().map(|a| {
                                    let path = a.path.clone();
                                    let is_active = move || selected_path.get() == a.path;
                                    view! {
                                        <div
                                            on:click=move |_| set_selected_path.set(path.clone())
                                            class=move || format!(
                                                "p-3 rounded-lg text-sm transition-colors cursor-pointer {}",
                                                if is_active() { "bg-blue-600/10 text-blue-400 font-medium border border-blue-500/20" } else { "text-slate-400 hover:bg-slate-800 border border-transparent" }
                                            )
                                        >
                                            <div class="flex items-center space-x-2">
                                                <Package size=14 />
                                                <span class="truncate">{a.name}</span>
                                            </div>
                                            <p class="text-[10px] text-slate-600 mt-1">{(a.size as f64 / 1024.0).round()} " KB"</p>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        }
                    })}
                </Suspense>
            </div>
            // Right: Preview
            <div class="w-2/3 flex flex-col h-full bg-slate-950">
                <div class="p-3 border-b border-slate-800 bg-slate-900 flex items-center justify-between">
                    <span class="text-xs font-mono text-slate-400">"Preview: " {move || selected_path.get()}</span>
                    <button class="text-[10px] text-blue-500 hover:underline">"Download Raw"</button>
                </div>
                <div class="flex-grow flex flex-col min-h-0 bg-slate-950 overflow-hidden text-slate-300">
                    <Suspense fallback=|| view! { <div class="p-8 animate-pulse space-y-2"><div class="h-2 bg-slate-800 rounded w-3/4"></div><div class="h-2 bg-slate-800 rounded w-1/2"></div></div> }>
                        {move || Suspend::new(async move {
                            let content = content_resource.await.unwrap_or_else(|e| format!("Error loading preview: {}", e));
                            view! { <TabularPreview content=content /> }
                        })}
                    </Suspense>
                </div>
            </div>
        </div>
    }.into_any()
}

#[component]
fn ConsoleView(exp_id: String, selected: std::collections::HashSet<String>) -> impl IntoView {
    let run_id = selected.iter().next().cloned().unwrap_or_default();
    let (logs, set_logs) = signal(Vec::<String>::new());

    let exp_id_val = StoredValue::new(exp_id.clone());
    let run_id_val = StoredValue::new(run_id.clone());

    // Effect to handle SSE streaming
    Effect::new(move |_| {
        let rid = run_id_val.with_value(|v| v.clone());
        if rid.is_empty() {
            return;
        }

        let url = format!(
            "/api/experiments/{}/runs/{}/log/stream",
            exp_id_val.with_value(|v| v.clone()),
            rid
        );
        let event_source = web_sys::EventSource::new(&url).unwrap();

        let on_message = wasm_bindgen::prelude::Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
            move |e: web_sys::MessageEvent| {
                if let Some(data) = e.data().as_string() {
                    if !data.is_empty() {
                        set_logs.update(|l| l.push(data));
                    }
                }
            },
        );

        event_source.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget(); // Leak for simplicity in this demo/agentic context, or store in cleanup
    });

    if run_id_val.with_value(|v| v.is_empty()) {
        return view! { <div class="p-12 text-center text-slate-500">"Select a single run to view live console output."</div> }.into_any();
    }

    view! {
        <div class="flex-grow flex flex-col bg-black overflow-hidden font-mono text-xs p-4">
            <div class="flex-grow overflow-auto space-y-1 custom-scrollbar" id="console-scroll">
                <div class="text-green-500">"$ tail -f /api/experiments/" {exp_id} "/runs/" {run_id} "/log/stream"</div>
                <div class="text-slate-400">"[system] Connection established to SSE stream..."</div>
                <For
                    each=move || logs.get().into_iter().enumerate()
                    key=|(i, _)| *i
                    children=|(_, line)| view! { <div class="text-white whitespace-pre-wrap">{line}</div> }
                />
            </div>
            <div class="mt-4 pt-4 border-t border-slate-800 flex items-center justify-between">
                <span class="text-slate-600">"Streaming Live"</span>
                <span class="text-blue-500 animate-pulse">"●"</span>
            </div>
        </div>
    }.into_any()
}

#[component]
fn SettingsPage() -> impl IntoView {
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

async fn fetch_run_metadata(exp_id: String, run_id: String) -> Result<Run, String> {
    let resp = gloo_net::http::Request::get(&format!(
        "/api/experiments/{}/runs/{}/metadata",
        exp_id, run_id
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(format!("Error fetching run metadata: {}", resp.status()));
    }

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

#[derive(Clone, Debug, Deserialize)]
struct JupyterStatus {
    running: bool,
    port: Option<u16>,
}

#[derive(Clone, Debug, Deserialize)]
struct JupyterAvailableResponse {
    available: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct JupyterStartResponse {
    port: u16,
}

async fn check_jupyter_available() -> Result<bool, String> {
    let resp = gloo_net::http::Request::get("/api/jupyter/available")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let text = resp.text().await.map_err(|e| e.to_string())?;
    let res: JupyterAvailableResponse = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(res.available)
}

async fn fetch_jupyter_status(exp: String, run: String) -> Result<JupyterStatus, String> {
    let resp = gloo_net::http::Request::get(&format!(
        "/api/experiments/{}/runs/{}/jupyter/status",
        exp, run
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

async fn start_jupyter(exp: String, run: String) -> Result<u16, String> {
    let resp = gloo_net::http::Request::post(&format!(
        "/api/experiments/{}/runs/{}/jupyter/start",
        exp, run
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;

    let text = resp.text().await.map_err(|e| e.to_string())?;
    let res: JupyterStartResponse = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(res.port)
}

async fn stop_jupyter(exp: String, run: String) -> Result<(), String> {
    gloo_net::http::Request::post(&format!(
        "/api/experiments/{}/runs/{}/jupyter/stop",
        exp, run
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[component]
fn InteractiveView(exp_id: String, selected: std::collections::HashSet<String>) -> impl IntoView {
    if selected.is_empty() {
        return view! {
            <div class="flex-grow flex flex-col items-center justify-center p-12 text-center space-y-4">
                <div class="p-4 bg-slate-800 rounded-full text-blue-500">
                    <FlaskConical size=48 />
                </div>
                <h3 class="text-xl font-bold text-white">"No Run Selected"</h3>
                <p class="text-slate-400 max-w-sm">"Select a single run from the sidebar to view interactive analysis tools."</p>
            </div>
        }.into_any();
    }

    if selected.len() > 1 {
        return view! {
            <div class="flex-grow flex flex-col items-center justify-center p-12 text-center space-y-4">
                <div class="p-4 bg-slate-800 rounded-full text-blue-500">
                    <FlaskConical size=48 />
                </div>
                <h3 class="text-xl font-bold text-white">"Multiple Runs Selected"</h3>
                <p class="text-slate-400 max-w-sm">"Please select exactly one run to view its interactive notebook."</p>
            </div>
        }.into_any();
    }

    let run_id = selected.into_iter().next().unwrap();

    let exp_id_clone_status = exp_id.clone();
    let run_id_clone_status = run_id.clone();
    let jupyter_status = LocalResource::new(move || {
        let eid = exp_id_clone_status.clone();
        let rid = run_id_clone_status.clone();
        async move { fetch_jupyter_status(eid, rid).await }
    });

    let (is_loading, set_is_loading) = signal(false);
    let (jupyter_port, set_jupyter_port) = signal(None::<u16>);

    Effect::new(move |_| {
        if let Some(Ok(status)) = jupyter_status.get().as_deref() {
            if status.running {
                set_jupyter_port.set(status.port);
            }
        }
    });

    let run_id_outer = run_id.clone();
    let exp_id_outer = exp_id.clone();

    let run_data = LocalResource::new(move || {
        let eid = exp_id.clone();
        let rid = run_id.clone();
        async move { fetch_run_metadata(eid, rid).await }
    });

    let jupyter_available =
        LocalResource::new(|| async move { check_jupyter_available().await.unwrap_or(false) });

    view! {
        <div class="flex-grow p-6 space-y-6 overflow-auto bg-[#e5e5e5] dark:bg-slate-950 flex flex-col h-full">
            <Suspense fallback=|| view! { <div class="p-8 text-center text-slate-500 animate-pulse">"Loading notebook status..."</div> }>
                {move || {
                    let port_opt = jupyter_port.get();
                    let loading = is_loading.get();
                    let rt_exp_id = exp_id_outer.clone();
                    let rt_run_id = run_id_outer.clone();

                    let start_notebook = move |_| {
                        let eid = rt_exp_id.clone();
                        let rid = rt_run_id.clone();
                        set_is_loading.set(true);
                        spawn_local(async move {
                            if let Ok(port) = start_jupyter(eid, rid).await {
                                set_jupyter_port.set(Some(port));
                            }
                            set_is_loading.set(false);
                        });
                    };

                    let rt_exp_id2 = exp_id_outer.clone();
                    let rt_run_id2 = run_id_outer.clone();

                    let stop_notebook = move |_| {
                        let eid = rt_exp_id2.clone();
                        let rid = rt_run_id2.clone();
                        set_is_loading.set(true);
                        spawn_local(async move {
                            let _ = stop_jupyter(eid, rid).await;
                            set_jupyter_port.set(None);
                            set_is_loading.set(false);
                        });
                    };

                    Suspend::new(async move {
                        let run = run_data.get().as_deref().cloned().unwrap_or(Err("Failed to load".to_string()));
                        let view_result: leptos::prelude::AnyView = match run {
                            Ok(run_info) => {
                                let lang = run_info.language.clone().unwrap_or_else(|| "python".to_string()).to_lowercase();
                                let env_str = run_info.env_path.clone().unwrap_or_else(|| "unknown".to_string());
                                let is_py = lang != "rust";
                                let snippet = if is_py {
                                    format!(
                                        "# Environment: {}\n# Install required dependencies into this environment\nimport sys\n!uv pip install polars matplotlib pyarrow fastparquet --python {{sys.executable}}\n\nimport polars as pl\nimport matplotlib.pyplot as plt\n\n# Load run metrics\nmetrics_path = 'metrics.parquet'\ndf = pl.read_parquet(metrics_path)\n\n# Display the latest metrics\ndf.tail()",
                                        env_str
                                    )
                                } else {
                                    format!(
                                        "// Environment: {}\nuse polars::prelude::*;\n\nfn main() -> Result<(), PolarsError> {{\n    // Load run metrics\n    let mut file = std::fs::File::open(\"metrics.parquet\").unwrap();\n    let df = ParquetReader::new(&mut file).finish()?;\n\n    println!(\"{{:?}}\", df.tail(Some(5)));\n    Ok(())\n}}",
                                        env_str
                                    )
                                };

                                let name_str = run_info.name.clone();

                                if let Some(p) = port_opt {
                                    let url = format!("http://localhost:{}/notebooks/interactive.ipynb", p);
                                    view! {
                                        <div class="flex flex-col h-full space-y-4 min-h-[700px]">
                                            <div class="flex justify-between items-center bg-white dark:bg-slate-900 p-4 rounded-lg shadow-sm border border-slate-300 dark:border-slate-700 mx-1">
                                                <div class="flex items-center space-x-3">
                                                    <div class="w-3 h-3 bg-green-500 rounded-full animate-pulse"></div>
                                                    <span class="font-semibold text-slate-800 dark:text-white">"Live Jupyter Notebook Active"</span>
                                                </div>
                                                <div class="flex items-center space-x-3">
                                                    <a href=url.clone() target="_blank" class="px-4 py-2 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-300 text-sm font-medium rounded transition-colors border border-slate-300 dark:border-slate-600">
                                                        "Pop-out"
                                                    </a>
                                                    <button
                                                        class="px-4 py-2 bg-red-500 hover:bg-red-600 text-white text-sm font-medium rounded transition-colors disabled:opacity-50"
                                                        on:click=stop_notebook
                                                        disabled=loading
                                                    >
                                                        {if loading { "Stopping..." } else { "Stop Notebook" }}
                                                    </button>
                                                </div>
                                            </div>
                                            <div class="flex-grow bg-white dark:bg-slate-900 border border-slate-300 dark:border-slate-700 rounded-lg overflow-hidden shadow-sm mx-1">
                                                <iframe src=url class="w-full h-full border-none min-h-[600px]"/>
                                            </div>
                                        </div>
                                    }.into_any()
                                } else {
                                    let env_disp = env_str.clone();
                                    let name_disp = name_str.clone();
                                    let snippet_disp = snippet.clone();
                                    let lang_disp = if is_py { "Python" } else { "Rust" };

                                    let available_res = jupyter_available.get();
                                    let is_available = match available_res.as_deref() {
                                        Some(&avail) => avail,
                                        None => false, // Loading or error
                                    };

                                    view! {
                                        <div class="max-w-4xl mx-auto w-full space-y-6">
                                            <div class="bg-white dark:bg-slate-900 rounded-lg shadow-sm border border-slate-300 dark:border-slate-700 p-8 text-center space-y-4">
                                                <div class="mx-auto w-16 h-16 bg-blue-100 dark:bg-blue-900/40 text-blue-600 dark:text-blue-400 rounded-full flex items-center justify-center mb-4">
                                                    <ChevronRight size=28 />
                                                </div>
                                                <h3 class="text-2xl font-bold text-slate-800 dark:text-white">"Launch Live Analysis"</h3>
                                                <p class="text-slate-500 max-w-lg mx-auto leading-relaxed">
                                                    "Spawn a fully functional Jupyter instance inside this run's folder {" 
                                                    <span class="font-mono font-medium text-slate-700 dark:text-slate-300">{name_disp}</span> 
                                                    "}, globally tied to the dashboard execution environment:"
                                                    <br/><br/>
                                                    <code class="text-xs font-semibold bg-slate-100 dark:bg-slate-800 px-2 py-1 rounded inline-block shadow-inner">{env_disp}</code>
                                                </p>
                                                <div class="pt-6">
                                                    <button
                                                        class="px-8 py-3 bg-blue-600 hover:bg-blue-700 focus:ring focus:ring-blue-500/50 text-white font-medium rounded-lg transition-all flex items-center justify-center mx-auto space-x-2 disabled:opacity-50 disabled:cursor-not-allowed shadow-md hover:shadow-lg"
                                                        on:click=start_notebook
                                                        disabled=move || loading || !is_available || jupyter_available.get().is_none()
                                                    >
                                                        <span>{if loading { "Launching Notebook..." } else if !is_available { "Jupyter Not Available" } else { "▶ Launch Live Jupyter Notebook" }}</span>
                                                    </button>
                                                    {
                                                        if !is_available && jupyter_available.get().is_some() {
                                                            view! {
                                                                <div class="mt-4 p-3 bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800 rounded-md max-w-lg mx-auto flex items-start space-x-3 text-left">
                                                                    <div class="text-yellow-600 dark:text-yellow-500 mt-0.5">
                                                                       <TriangleAlert size=18 />
                                                                    </div>
                                                                    <div class="text-sm text-yellow-800 dark:text-yellow-200">
                                                                        <p class="font-bold">"Jupyter Notebook is not installed"</p>
                                                                        <p class="mt-1">"To enable this feature, install Jupyter in the environment where the ExpMan Dashboard is running (e.g., "<code class="text-xs bg-yellow-100 dark:bg-yellow-900 px-1 rounded">"pip install notebook"</code>")."</p>
                                                                    </div>
                                                                </div>
                                                            }.into_any()
                                                        } else {
                                                            view! { <span class="hidden"></span> }.into_any()
                                                        }
                                                    }
                                                </div>
                                            </div>
                                            <div class="flex items-center space-x-4 my-8 mx-12">
                                                <div class="h-px bg-slate-300 dark:bg-slate-700 flex-grow"></div>
                                                <span class="text-slate-400 text-xs font-bold uppercase tracking-widest whitespace-nowrap">"Or Use Snippet Manually"</span>
                                                <div class="h-px bg-slate-300 dark:bg-slate-700 flex-grow"></div>
                                            </div>
                                            <div class="bg-white dark:bg-slate-900 border border-slate-300 dark:border-slate-700 rounded-lg overflow-hidden shadow-sm">
                                                <div class="flex bg-slate-50 dark:bg-slate-800 border-b border-slate-300 dark:border-slate-700 px-4 py-3 text-xs text-slate-500 items-center justify-between">
                                                    <div class="flex items-center space-x-2">
                                                        <span class="font-mono bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-400 px-2 py-0.5 rounded font-bold">"In [1]:"</span>
                                                        <span class="font-medium text-slate-700 dark:text-slate-300">{lang_disp}</span>
                                                    </div>
                                                </div>
                                                <div class="p-5 font-mono text-sm overflow-x-auto text-slate-800 dark:text-slate-300 bg-slate-50 dark:bg-slate-950">
                                                    <pre><code class="leading-relaxed">{snippet_disp}</code></pre>
                                                </div>
                                            </div>
                                        </div>
                                    }.into_any()
                                }
                            },
                            Err(e) => {
                                let err_msg = e.clone();
                                view! {
                                    <div class="p-8 text-red-500 text-center bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800/50 rounded-lg max-w-md mx-auto mt-10">
                                        <div class="font-bold flex items-center justify-center space-x-2 mb-2">
                                            <span>"Failed to Load Run"</span>
                                        </div>
                                        <p class="text-sm opacity-80">{err_msg}</p>
                                    </div>
                                }.into_any()
                            }
                        };
                        view_result
                    })
                }}
            </Suspense>
        </div>
    }.into_any()
}

#[component]
fn NotFound() -> impl IntoView {
    view! {
        <div class="flex flex-col items-center justify-center h-full space-y-4">
            <h1 class="text-4xl font-bold">"404"</h1>
            <p class="text-slate-400">"Page not found"</p>
            <A href="/" attr:class="text-blue-400 hover:underline">"Back to Dashboard"</A>
        </div>
    }
    .into_any()
}

fn main() {
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
#[component]
fn RunsTableView(exp_id: String) -> impl IntoView {
    let runs = LocalResource::new(move || {
        let id = exp_id.clone();
        async move { fetch_runs(id).await }
    });

    // Which metric columns are currently visible (None = all visible)
    let (selected_metrics, set_selected_metrics) =
        signal(std::collections::HashSet::<String>::new());
    let (metrics_initialized, set_metrics_initialized) = signal(false);

    view! {
        <div class="flex-grow p-6 overflow-auto space-y-4">
             <Suspense fallback=|| view! { <div class="p-4 text-center text-slate-500">"Loading runs..."</div> }>
                {move || Suspend::new(async move {
                    let run_list = runs.await.unwrap_or_default();
                    if run_list.is_empty() {
                        return view! { <div class="p-12 text-center text-slate-500">"No runs found for this experiment."</div> }.into_any();
                    }

                    // Collect all unique metric keys (sorted)
                    let all_metric_keys: Vec<String> = {
                        let mut keys = std::collections::BTreeSet::new();
                        for run in &run_list {
                            if let Some(metrics) = &run.metrics {
                                for key in metrics.keys() {
                                    keys.insert(key.clone());
                                }
                            }
                        }
                        keys.into_iter().collect()
                    };

                    // Initialize selected_metrics to all keys on first load
                    if !metrics_initialized.get() && !all_metric_keys.is_empty() {
                        set_selected_metrics.set(all_metric_keys.iter().cloned().collect());
                        set_metrics_initialized.set(true);
                    }

                    let keys_for_filter = all_metric_keys.clone();
                    let keys_for_table = all_metric_keys.clone();

                    view! {
                        // ── Metric filter chips ──────────────────────────────────
                        {if !keys_for_filter.is_empty() {
                            view! {
                                <div class="flex flex-wrap items-center gap-2">
                                    <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider mr-1">"Metrics:"</span>
                                    {keys_for_filter.into_iter().map(|key| {
                                        let k1 = key.clone();
                                        let k2 = key.clone();
                                        let is_on = Signal::derive(move || selected_metrics.with(|s| s.contains(&k1)));
                                        view! {
                                            <button
                                                on:click=move |_| {
                                                    let k = k2.clone();
                                                    set_selected_metrics.update(|s| {
                                                        if s.contains(&k) { s.remove(&k); } else { s.insert(k); }
                                                    });
                                                }
                                                class=move || format!(
                                                    "px-3 py-1 rounded-full text-xs font-medium border transition-all duration-150 {}",
                                                    if is_on.get() {
                                                        "bg-blue-600/20 border-blue-500/50 text-blue-300 hover:bg-blue-600/30"
                                                    } else {
                                                        "bg-slate-800 border-slate-700 text-slate-500 hover:border-slate-600 hover:text-slate-400"
                                                    }
                                                )
                                            >
                                                {key}
                                            </button>
                                        }
                                    }).collect_view()}
                                </div>
                            }.into_any()
                        } else {
                            view! { <div></div> }.into_any()
                        }}

                        // ── Runs table ───────────────────────────────────────────
                        <div class="bg-slate-900 border border-slate-800 rounded-xl overflow-hidden">
                            <table class="w-full text-left border-collapse">
                                <thead class="bg-slate-950 text-xs uppercase text-slate-500 font-semibold sticky top-0">
                                    <tr>
                                        <th class="p-4 border-b border-slate-800">"Run ID"</th>
                                        <th class="p-4 border-b border-slate-800">"Status"</th>
                                        {keys_for_table.iter().filter(|k| selected_metrics.with(|s| s.contains(*k))).map(|k| view! {
                                            <th class="p-4 border-b border-slate-800 text-blue-400">{k.clone()}</th>
                                        }).collect_view()}
                                        <th class="p-4 border-b border-slate-800">"Duration"</th>
                                        <th class="p-4 border-b border-slate-800">"Started"</th>
                                        <th class="p-4 border-b border-slate-800">"Description"</th>
                                    </tr>
                                </thead>
                                <tbody class="divide-y divide-slate-800/50 text-sm text-slate-300">
                                    {run_list.into_iter().map(|run| {
                                        let duration = run.duration_secs.map(|d| format!("{:.1}s", d)).unwrap_or("-".to_string());
                                        let (status_color, status_bg, status_border) = match run.status.as_str() {
                                            "RUNNING"   => ("text-blue-400",   "bg-blue-500",   "border-blue-500"),
                                            "COMPLETED" => ("text-emerald-400", "bg-emerald-500", "border-emerald-500"),
                                            "FAILED"    => ("text-red-400",     "bg-red-500",     "border-red-500"),
                                            _           => ("text-slate-400",   "bg-slate-600",   "border-slate-500"),
                                        };
                                        let dot_class = if run.status == "RUNNING" { "animate-pulse" } else { "" };
                                        let run_metrics = run.metrics.clone().unwrap_or_default();
                                        let metric_cols: Vec<String> = keys_for_table.iter()
                                            .filter(|k| selected_metrics.with(|s| s.contains(*k)))
                                            .cloned()
                                            .collect();

                                        view! {
                                            <tr class="hover:bg-slate-800/30 transition-colors group">
                                                <td class="p-4 font-mono text-white flex items-center space-x-2">
                                                    <div class=format!("w-2 h-2 rounded-full {} {}", status_bg, dot_class)></div>
                                                    <span>{run.name}</span>
                                                </td>
                                                <td class="p-4">
                                                    <span class=format!("px-2 py-1 rounded text-xs font-medium bg-opacity-10 border border-opacity-20 {} {} {}", status_bg, status_color, status_border)>
                                                        {run.status}
                                                    </span>
                                                </td>
                                                {metric_cols.into_iter().map(|k| {
                                                    let val = run_metrics.get(&k)
                                                        .map(|f| format!("{:.4}", f))
                                                        .unwrap_or_else(|| "-".to_string());
                                                    view! { <td class="p-4 font-mono text-slate-400">{val}</td> }
                                                }).collect_view()}
                                                <td class="p-4 font-mono text-slate-400">{duration}</td>
                                                <td class="p-4 text-slate-400 whitespace-nowrap">{format_date(&run.started_at)}</td>
                                                <td class="p-4 text-slate-500 truncate max-w-xs group-hover:text-slate-300 transition-colors">{run.description.unwrap_or_default()}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                })}
             </Suspense>
        </div>
    }
}
