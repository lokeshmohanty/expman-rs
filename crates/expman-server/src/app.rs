//! Leptos frontend application (compiled to WASM via trunk).
use chrono::{DateTime, Local};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::{Route, Router, Routes, A};
use leptos_router::hooks::use_params_map;
use leptos_router::path;
use lucide_leptos::{
    Book, ChevronRight, Cog as SettingsIcon, FlaskConical, Github, LayoutDashboard, Package,
    TriangleAlert,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use web_sys::RequestMode;

use std::cell::Cell;
use std::rc::Rc;
#[derive(Clone)]
struct SidebarContext(RwSignal<Option<Rc<dyn Fn() -> AnyView>>, LocalStorage>);

const CHART_COLORS: [&str; 5] = ["#3b82f6", "#10b981", "#f59e0b", "#ef4444", "#8b5cf6"];

fn format_date(iso: &str) -> String {
    if let Ok(dt) = DateTime::parse_from_rfc3339(iso) {
        let local = dt.with_timezone(&Local);
        local.format("%H:%M, %d %b, %Y").to_string()
    } else {
        iso.to_string()
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Default)]
pub struct ExperimentMetadata {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
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
#[serde(untagged)]
pub enum MetricValue {
    Float(f64),
    Int(i64),
    Bool(bool),
    Text(String),
}

impl std::fmt::Display for MetricValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Float(v) => write!(f, "{}", v),
            Self::Int(v) => write!(f, "{}", v),
            Self::Bool(v) => write!(f, "{}", v),
            Self::Text(v) => write!(f, "{}", v),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Run {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_secs: Option<f64>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub scalars: Option<std::collections::HashMap<String, MetricValue>>,
    pub vectors: Option<std::collections::HashMap<String, MetricValue>>,
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
    tags: Option<Vec<String>>,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "name": name,
        "description": description,
        "tags": tags,
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
pub fn App() -> impl IntoView {
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

                    <div class="mt-auto space-y-1">
                        <A href="/settings" attr:class="flex items-center space-x-3 px-4 py-3 rounded-xl hover:bg-slate-800 transition-all duration-200 text-slate-400 hover:text-white group">
                            <div class="group-hover:text-blue-400 transition-colors">
                                <SettingsIcon size=20 />
                            </div>
                            <span class="font-medium">"Settings"</span>
                        </A>

                        <a href="https://lokeshmohanty.github.io/expman-rs" target="_blank" rel="noopener noreferrer" class="flex items-center space-x-3 px-4 py-3 rounded-xl hover:bg-slate-800 transition-all duration-200 text-slate-400 hover:text-white group">
                            <div class="group-hover:text-blue-400 transition-colors">
                                <Book size=20 />
                            </div>
                            <span class="font-medium">"Documentation"</span>
                        </a>

                        <a href="https://github.com/lokeshmohanty/expman-rs" target="_blank" rel="noopener noreferrer" class="flex items-center space-x-3 px-4 py-3 rounded-xl hover:bg-slate-800 transition-all duration-200 text-slate-400 hover:text-white group">
                            <div class="group-hover:text-blue-400 transition-colors">
                                <Github size=20 />
                            </div>
                            <span class="font-medium">"GitHub"</span>
                        </a>
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

#[component]
fn ExperimentDetail() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.read().get("id").unwrap_or_default();
    let sidebar_ctx = use_context::<SidebarContext>().expect("SidebarContext not found");

    let runs = LocalResource::new(move || {
        let exp_id = id();
        async move { fetch_runs(exp_id).await }
    });

    async fn fetch_experiment_metadata(eid: String) -> Option<ExperimentMetadata> {
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
            None
        }
    }

    let exp_metadata = LocalResource::new(move || {
        let eid = id();
        async move { fetch_experiment_metadata(eid).await }
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
    let (edit_run_tags, set_edit_run_tags) = signal("".to_string());

    let toggle_run = move |id: String| {
        set_selected_runs.update(|set| {
            if set.contains(&id) {
                set.remove(&id);
            } else {
                set.insert(id);
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
            exp_metadata.refetch();
        });
    };

    let save_run_metadata = move |_| {
        let eid = id();
        let rid = edit_run_id.get();
        let name = edit_run_name.get();
        let desc = edit_run_desc.get();
        let tags: Vec<String> = edit_run_tags
            .get()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        spawn_local(async move {
            let _ = update_run_metadata(eid, rid, Some(name), Some(desc), Some(tags)).await;
            set_show_run_edit.set(false);
            runs.refetch();
        });
    };

    let open_run_edit = move |r: Run| {
        set_edit_run_id.set(r.id.clone());
        set_edit_run_name.set(r.name);
        set_edit_run_desc.set(r.description.unwrap_or_default());
        if let Some(tags) = r.tags {
            set_edit_run_tags.set(tags.join(", "));
        } else {
            set_edit_run_tags.set("".to_string());
        }
        set_show_run_edit.set(true);
    };

    // Sidebar View Effect
    Effect::new(move |_| {
        sidebar_ctx.0.set(Some(Rc::new(move || {
            view! {
            <div class="h-full flex flex-col">
                <div class="px-4 py-2 border-b border-slate-800 bg-slate-900/50 flex items-center justify-between">
                    <div>
                        <h2 class="font-bold text-slate-200 text-sm">"Select Runs"</h2>
                        <p class="text-[10px] text-slate-500">"Select to compare metrics"</p>
                    </div>
                    <div class="flex space-x-1">
                        <button 
                            on:click=move |_| {
                                if let Some(Ok(run_list)) = runs.get() {
                                    let all_ids: std::collections::HashSet<String> = run_list.into_iter().map(|r| r.id).collect();
                                    set_selected_runs.set(all_ids);
                                }
                            }
                            class="text-[9px] px-1.5 py-0.5 bg-slate-800 hover:bg-slate-700 text-slate-400 rounded border border-slate-700 transition-colors"
                        >
                            "All"
                        </button>
                        <button 
                            on:click=move |_| set_selected_runs.set(std::collections::HashSet::new())
                            class="text-[9px] px-1.5 py-0.5 bg-slate-800 hover:bg-slate-700 text-slate-400 rounded border border-slate-700 transition-colors"
                        >
                            "None"
                        </button>
                    </div>
                </div>
                <div class="flex-grow overflow-auto p-2 space-y-1 custom-scrollbar">
                     <Suspense fallback=|| view! { <div class="p-4 text-slate-500 text-xs">"Loading runs..."</div> }>
                        {move || Suspend::new(async move {
                            let run_list: Vec<Run> = runs.await.unwrap_or_default();
                            view! {
                                {run_list.into_iter().map(|run| {
                                    let rid_inner = run.id.clone();
                                    let is_selected = Signal::derive(move || selected_runs.with(|set| set.contains(&rid_inner)));
                                    let is_running = run.status == "RUNNING";
                                    let run_clone = run.clone();
                                    let rid_click = run.id.clone();

                                    let v: AnyView = view! {
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

                                                    <div class="flex flex-wrap gap-1 mt-1 empty:hidden">
                                                        {run_clone.tags.clone().unwrap_or_default().into_iter().take(2).map(|t| view! {
                                                            <span class="px-1.5 py-0.5 bg-blue-500/10 text-blue-400 rounded-md text-[9px] border border-blue-500/20">{t}</span>
                                                        }).collect_view()}
                                                    </div>
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
                                    }.into_any();
                                    v
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
                                <div>
                                    <label class="block text-xs font-semibold text-slate-500 uppercase mb-1">"Tags (comma separated)"</label>
                                    <input
                                        type="text"
                                        on:input=move |ev| set_edit_run_tags.set(event_target_value(&ev))
                                        prop:value=edit_run_tags
                                        class="w-full bg-slate-950 border border-slate-800 rounded-lg px-4 py-2 text-white focus:border-blue-500 outline-none"
                                        placeholder="gpu, large-batch"
                                    />
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
                <div class="w-full max-w-2xl">
                    <Suspense fallback=move || {
                        let id_clone = id();
                        view! {
                            <div class="space-y-4">
                                <h1 class="text-3xl font-bold text-white flex items-center space-x-3">
                                    <div class="text-blue-500"><FlaskConical size=32 /></div>
                                    <span> {id_clone} </span>
                                </h1>
                                <div class="h-4 bg-slate-800 rounded w-1/2 animate-pulse"></div>
                            </div>
                        }.into_any()
                    }>
                        {move || Suspend::new(async move {
                            let meta: ExperimentMetadata = exp_metadata.get().flatten().unwrap_or_default();
                            let count = runs.get().and_then(|r| r.ok()).map(|r| r.len()).unwrap_or(0);
                            let title = meta.display_name.clone().unwrap_or_else(id);

                            let v: AnyView = view! {
                                <div class="space-y-4">
                                    <h1 class="text-3xl font-bold text-white flex items-center space-x-3">
                                        <div class="text-blue-500"><FlaskConical size=32 /></div>
                                        <span> {title} </span>
                                    </h1>
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
                                </div>
                            }.into_any();
                            v
                        })}
                    </Suspense>
                </div>
                <div class="flex space-x-2">
                    <button on:click=move |_| {
                        // Pre-fill edit form with current metadata
                        if let Some(Some(meta)) = exp_metadata.get() {
                            set_edit_name.set(meta.display_name.clone().unwrap_or_else(id));
                            set_edit_desc.set(meta.description.clone().unwrap_or_default());
                            set_edit_tags.set(meta.tags.join(", "));
                        } else {
                            set_edit_name.set(id());
                            set_edit_desc.set(String::new());
                            set_edit_tags.set(String::new());
                        }
                        set_show_edit.set(true);
                    } class="px-4 py-2 bg-slate-800 hover:bg-slate-700 rounded-lg text-sm transition-colors border border-slate-700">
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
                        "runs" => {
                            let run_list: Vec<Run> = runs.get().and_then(|r| r.ok()).unwrap_or_default();
                            let edit_callback = move |r: Run| open_run_edit(r);
                            view! { <RunsTableView runs=run_list on_edit=edit_callback /> }.into_any()
                        },
                        "metrics" => {
                            let run_list: Vec<Run> = runs.get().and_then(|r| r.ok()).unwrap_or_default();
                            view! { <MetricsView exp_id=id() selected=selected_runs.get() runs=run_list /> }.into_any()
                        },
                        "artifacts" => view! { <ArtifactView exp_id=id() selected=selected_runs.get() /> }.into_any(),
                        "console" => {
                            let run_list: Vec<Run> = runs.get().and_then(|r| r.ok()).unwrap_or_default();
                            view! { <ConsoleView exp_id=id() selected=selected_runs.get() runs=run_list /> }.into_any()
                        },
                        "interactive" => view! { <InteractiveView exp_id=id() selected=selected_runs.get() /> }.into_any(),
                        _ => view! { <div class="p-8 text-slate-500 text-center">"Select a tab"</div> }.into_any(),
                    }}
                </div>
            </div>
        </div>
    }
}

#[component]
fn MetricsView(
    exp_id: String,
    selected: std::collections::HashSet<String>,
    runs: Vec<Run>,
) -> impl IntoView {
    if selected.is_empty() {
        let v: AnyView = view! {
            <div class="flex-grow flex flex-col items-center justify-center p-12 text-center space-y-4">
                <div class="p-4 bg-slate-800 rounded-full text-blue-500">
                    <LayoutDashboard size=48 />
                </div>
                <h3 class="text-xl font-bold text-white">"No Runs Selected"</h3>
                <p class="text-slate-400 max-w-sm">"Please select one or more runs from the left sidebar to visualize and compare metrics in real-time."</p>
            </div>
        }.into_any();
        return v;
    }

    let selected_runs_data: Vec<&Run> =
        runs.iter().filter(|r| selected.contains(&r.id)).collect();

    let mut vector_keys = std::collections::BTreeSet::new();
    for r in &selected_runs_data {
        if let Some(vectors) = &r.vectors {
            for k in vectors.keys() {
                vector_keys.insert(k.clone());
            }
        }
    }
    let v_keys: Vec<String> = vector_keys.into_iter().collect();

    view! {
        <div class="flex-grow p-6 space-y-6 overflow-auto">
            <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
                {
                    if v_keys.is_empty() {
                        view! {
                            <div class="lg:col-span-2 bg-slate-950 border border-slate-800 rounded-xl p-6 h-96 flex items-center justify-center">
                                <p class="text-slate-500 italic">"No vector data available for selected runs."</p>
                            </div>
                        }.into_any()
                    } else {
                        v_keys.into_iter().map(|vk| {
                            let exp_id_clone = exp_id.clone();
                            let selected_clone = selected.clone();
                            let vk_clone = vk.clone();
                            view! {
                                <div class="bg-slate-950 border border-slate-800 rounded-xl p-6 flex flex-col" style="resize: both; overflow: hidden; min-width: 300px; max-width: 100%; aspect-ratio: 16/9;">
                                    <div class="flex items-center justify-between mb-4 flex-shrink-0">
                                        <h4 class="text-sm font-semibold text-slate-300">{vk_clone}</h4>
                                        <div class="flex space-x-3">
                                             {selected_clone.clone().into_iter().enumerate().map(|(i, s)| {
                                                 let color = CHART_COLORS[i % CHART_COLORS.len()];
                                                 view! {
                                                     <div class="flex items-center space-x-1 text-[10px] text-slate-400">
                                                         <span class=format!("w-2 h-2 rounded-full") style=format!("background-color: {}", color)></span>
                                                         <span class="font-mono">{s}</span>
                                                     </div>
                                                 }
                                             }).collect_view()}
                                        </div>
                                    </div>
                                    <div class="flex-grow rounded-lg overflow-hidden relative" style="width: 100%; height: 100%;">
                                        <LineChart exp_id=exp_id_clone selected_runs=selected_clone metric_key=vk />
                                    </div>
                                </div>
                            }.into_any()
                        }).collect_view().into_any()
                    }
                }
            </div>

            <div class="bg-slate-950 border border-slate-800 rounded-xl p-6">
                 <h4 class="text-sm font-semibold text-slate-300 mb-4">"Scalars Summary"</h4>
                 <div class="overflow-x-auto">
                     {
                         // Collect all scalar keys from selected runs
                         let mut scalar_keys = std::collections::BTreeSet::new();
                         for r in &selected_runs_data {
                             if let Some(scalars) = &r.scalars {
                                 for k in scalars.keys() {
                                     scalar_keys.insert(k.clone());
                                 }
                             }
                         }
                         let keys: Vec<String> = scalar_keys.into_iter().collect();

                         if keys.is_empty() {
                             view! {
                                 <p class="text-sm text-slate-500 italic">"No scalar data available for selected runs."</p>
                             }.into_any()
                         } else {
                             let keys_for_header = keys.clone();
                             view! {
                                 <table class="w-full text-left border-collapse text-xs">
                                     <thead class="bg-slate-900 text-slate-500 uppercase font-semibold">
                                         <tr>
                                             <th class="p-3 border-b border-slate-800">"Run"</th>
                                             {keys_for_header.into_iter().map(|k| view! {
                                                 <th class="p-3 border-b border-slate-800 text-blue-400">{k}</th>
                                             }).collect_view()}
                                         </tr>
                                     </thead>
                                     <tbody class="divide-y divide-slate-800/50 text-slate-300">
                                         {selected_runs_data.into_iter().map(|r| {
                                             let scalars = r.scalars.clone().unwrap_or_default();
                                             let current_keys = keys.clone();
                                             view! {
                                                 <tr class="hover:bg-slate-800/20">
                                                     <td class="p-3 font-medium text-white">{r.name.clone()}</td>
                                                     {current_keys.into_iter().map(|k| {
                                                         let val = scalars.get(&k).map(|v| v.to_string()).unwrap_or_else(|| "-".to_string());
                                                         view! { <td class="p-3 font-mono text-slate-400">{val}</td> }
                                                     }).collect_view()}
                                                 </tr>
                                             }
                                         }).collect_view()}
                                     </tbody>
                                 </table>
                             }.into_any()
                         }
                     }
                 </div>
            </div>
        </div>
    }.into_any()
}

#[component]
fn LineChart(
    #[allow(unused_variables)] exp_id: String,
    selected_runs: std::collections::HashSet<String>,
    metric_key: String,
) -> impl IntoView {
    let canvas_ref = NodeRef::<leptos::html::Canvas>::new();
    let (view_range_x, set_view_range_x) = signal((0.0, 20.0));
    let (view_range_y, set_view_range_y) = signal((0.0, 2.0));
    let (is_dragging, set_is_dragging) = signal(false);
    let (last_mouse_pos, set_last_mouse_pos) = signal(None::<(i32, i32)>);

    let on_mousedown = move |ev: web_sys::MouseEvent| {
        set_is_dragging.set(true);
        set_last_mouse_pos.set(Some((ev.client_x(), ev.client_y())));
    };

    let on_mousemove = move |ev: web_sys::MouseEvent| {
        if is_dragging.get() {
            if let Some((lx, ly)) = last_mouse_pos.get() {
                let dx = ev.client_x() - lx;
                let dy = ev.client_y() - ly;

                if let Some(canvas) = canvas_ref.get() {
                    let w = canvas.client_width() as f64;
                    let h = canvas.client_height() as f64;

                    let (x_min, x_max) = view_range_x.get();
                    let (y_min, y_max) = view_range_y.get();

                    let x_range = x_max - x_min;
                    let y_range = y_max - y_min;

                    let shift_x = (dx as f64 / w) * x_range;
                    let shift_y = (dy as f64 / h) * y_range;

                    set_view_range_x.set((x_min - shift_x, x_max - shift_x));
                    set_view_range_y.set((y_min + shift_y, y_max + shift_y));
                    set_last_mouse_pos.set(Some((ev.client_x(), ev.client_y())));
                }
            }
        }
    };

    let on_mouseup = move |_| {
        set_is_dragging.set(false);
        set_last_mouse_pos.set(None);
    };

    let on_wheel = move |ev: web_sys::WheelEvent| {
        ev.prevent_default();
        let delta = ev.delta_y();
        let zoom_factor = if delta > 0.0 { 1.1 } else { 0.9 };

        if let Some(canvas) = canvas_ref.get() {
            let rect = canvas.get_bounding_client_rect();
            let mouse_x = ev.client_x() as f64 - rect.left();
            let w = canvas.client_width() as f64;

            let (x_min, x_max) = view_range_x.get();
            let x_range = x_max - x_min;
            let cursor_x_rel = mouse_x / w;
            let pivot_x = x_min + cursor_x_rel * x_range;

            let new_x_min = pivot_x - (pivot_x - x_min) * zoom_factor;
            let new_x_max = pivot_x + (x_max - pivot_x) * zoom_factor;

            let (y_min, y_max) = view_range_y.get();
            let y_range = y_max - y_min;
            let new_y_min = y_min; // Keep Y static for now or zoom both
            let new_y_max = y_min + y_range * zoom_factor;

            set_view_range_x.set((new_x_min, new_x_max));
            set_view_range_y.set((new_y_min, new_y_max));
        }
    };

    Effect::new(move |_| {
        use plotters::prelude::*;
        use plotters_canvas::CanvasBackend;

        if let Some(canvas) = canvas_ref.get() {
            let (x_min, x_max) = view_range_x.get();
            let (y_min, y_max) = view_range_y.get();

            let w = canvas.parent_element().unwrap().client_width() as u32;
            let h = canvas.parent_element().unwrap().client_height() as u32;
            if w > 0 && h > 0 {
                canvas.set_width(w);
                canvas.set_height(h);
            }

            let backend = CanvasBackend::with_canvas_object(canvas.clone().into()).unwrap();
            let root = backend.into_drawing_area();
            let _ = root.fill(&WHITE);

            let mut chart = ChartBuilder::on(&root)
                .caption(
                    &metric_key,
                    ("sans-serif", 14)
                        .into_font()
                        .color(&BLACK),
                )
                .margin(10)
                .x_label_area_size(30)
                .y_label_area_size(40)
                .build_cartesian_2d(x_min..x_max, y_min..y_max)
                .unwrap();

            chart
                .configure_mesh()
                .disable_x_mesh()
                .y_desc("Value")
                .axis_style(RGBColor(203, 213, 225)) // Slate-300
                .label_style(
                    ("sans-serif", 10)
                        .into_font()
                        .color(&BLACK),
                )
                .draw()
                .unwrap();

            for (i, run_id) in selected_runs.iter().enumerate() {
                let run_len: f64 = run_id.len() as f64;
                let y_data: Vec<(f64, f64)> = (0..100)
                    .map(|i| {
                        let x = i as f64;
                        let base = x.sin().abs();
                        let adjusted = base + (run_len % 10.0) / 10.0;
                        (x, adjusted)
                    })
                    .filter(|(x, y)| *x >= x_min - 2.0 && *x <= x_max + 2.0 && *y >= y_min - 2.0 && *y <= y_max + 2.0)
                    .collect();

                let hex = CHART_COLORS[i % CHART_COLORS.len()];
                let r = u8::from_str_radix(&hex[1..3], 16).unwrap_or(0);
                let g = u8::from_str_radix(&hex[3..5], 16).unwrap_or(0);
                let b = u8::from_str_radix(&hex[5..7], 16).unwrap_or(0);
                let color = RGBColor(r, g, b);

                chart
                    .draw_series(LineSeries::new(y_data, color.stroke_width(2)))
                    .unwrap();
            }

            let _ = root.present();
        }
    });

    view! {
        <div class="w-full h-full relative" style="min-height: 250px;">
            <canvas
                node_ref=canvas_ref
                on:mousedown=on_mousedown
                on:mousemove=on_mousemove
                on:mouseup=on_mouseup
                on:mouseleave=on_mouseup
                on:wheel=on_wheel
                class="absolute inset-0 w-full h-full cursor-crosshair"
            ></canvas>
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
    pub is_default: bool,
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
    if selected.is_empty() {
        return view! { <div class="p-12 text-center text-slate-500">"Select one or more runs to browse artifacts."</div> }.into_any();
    }

    let selected_runs: Vec<String> = selected.into_iter().collect();

    view! {
        <div class="flex-grow flex flex-col overflow-auto h-full space-y-6 p-6">
            {selected_runs.into_iter().map(|run_id| {
                 view! {
                     <div class="flex flex-col h-96 border border-slate-800 rounded-xl overflow-hidden mb-6 flex-shrink-0">
                         <div class="bg-slate-900 border-b border-slate-800 p-2 text-xs font-bold text-slate-400 uppercase tracking-wider">
                             "Run: " {run_id.clone()}
                         </div>
                         <SingleArtifactView exp_id=exp_id.clone() run_id=run_id />
                     </div>
                 }
            }).collect_view()}
        </div>
    }.into_any()
}

#[component]
fn SingleArtifactView(exp_id: String, run_id: String) -> impl IntoView {
    let (selected_path, set_selected_path) = signal("run.log".to_string());

    let exp_id_val = StoredValue::new(exp_id.clone());
    let run_id_val = StoredValue::new(run_id.clone());

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

            let ext = path.split('.').next_back().unwrap_or("").to_lowercase();
            if matches!(
                ext.as_str(),
                "mp4"
                    | "webm"
                    | "ogg"
                    | "mp3"
                    | "wav"
                    | "flac"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "svg"
                    | "gif"
                    | "webp"
            ) {
                return Ok(String::new());
            }

            fetch_artifact_content(eid, rid, path).await
        }
    });

    view! {
        <div class="flex h-full divide-x divide-slate-800">
            // Left: List
            <div class="w-1/3 overflow-auto bg-slate-900/30 p-2 space-y-1">
                <div class="p-2 text-xs font-bold text-slate-500 uppercase tracking-wider mb-2">"Files"</div>
                <Suspense fallback=|| view! { <div class="p-4 text-slate-500 text-sm">"Loading..."</div> }>
                    {move || Suspend::new(async move {
                        let list = artifact_resource.await.unwrap_or_default();

                        let mut default_artifacts = Vec::new();
                        let mut stored_artifacts = Vec::new();

                        for a in list.into_iter() {
                            if !a.is_default {
                                stored_artifacts.push(a);
                            } else {
                                default_artifacts.push(a);
                            }
                        }

                        view! {
                            <div class="space-y-4 pb-8">
                                <div class="space-y-1">
                                    {if !default_artifacts.is_empty() {
                                        view! {
                                            <div class="pt-2 pb-1 px-2 text-[10px] font-bold text-slate-500 uppercase tracking-wider flex items-center space-x-2">
                                                <div class="h-px bg-slate-700 flex-grow"></div>
                                                <span>"Default"</span>
                                                <div class="h-px bg-slate-700 flex-grow"></div>
                                            </div>
                                            {default_artifacts.into_iter().map(|a| {
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
                                                        <div class="flex items-center justify-between">
                                                            <div class="flex items-center space-x-2 truncate">
                                                                <div class="flex-shrink-0 w-2 h-2 rounded-full bg-slate-700"></div>
                                                                <span class="truncate">{a.name}</span>
                                                            </div>
                                                            <span class="flex-shrink-0 ml-2 text-[10px] text-slate-600 font-mono">{(a.size / 1024).max(1)} " KB"</span>
                                                        </div>
                                                    </div>
                                                }.into_any()
                                            }).collect_view()}
                                        }.into_any()
                                    } else {
                                        view! { <span class="hidden"></span> }.into_any()
                                    }}

                                    {if !stored_artifacts.is_empty() {
                                        view! {
                                            <div class="pt-4 pb-1 px-2 text-[10px] font-bold text-slate-500 uppercase tracking-wider flex items-center space-x-2">
                                                <div class="h-px bg-slate-700 flex-grow"></div>
                                                <span>"Artifacts"</span>
                                                <div class="h-px bg-slate-700 flex-grow"></div>
                                            </div>
                                            {stored_artifacts.into_iter().map(|a| {
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
                                                        <div class="flex items-center justify-between">
                                                            <div class="flex items-center space-x-2 truncate">
                                                                <div class="flex-shrink-0 w-2 h-2 rounded-full bg-slate-700"></div>
                                                                <span class="truncate">{a.name}</span>
                                                            </div>
                                                            <span class="flex-shrink-0 ml-2 text-[10px] text-slate-600 font-mono">{(a.size / 1024).max(1)} " KB"</span>
                                                        </div>
                                                    </div>
                                                }.into_any()
                                            }).collect_view()}
                                        }.into_any()
                                    } else {
                                        view! { <span class="hidden"></span> }.into_any()
                                    }}
                                </div>
                            </div>
                        }.into_any()
                    })}
                </Suspense>
            </div>
            // Right: Preview
            <div class="w-2/3 flex flex-col h-full bg-slate-950">
                <div class="p-3 border-b border-slate-800 bg-slate-900 flex items-center justify-between">
                    <span class="text-xs font-mono text-slate-400">"Preview: " {move || selected_path.get()}</span>
                    {
                        let dl_exp_id = exp_id.clone();
                        let dl_run_id = run_id.clone();
                        view! { <a href=move || format!("/api/experiments/{}/runs/{}/artifacts/content?path={}", dl_exp_id.clone(), dl_run_id.clone(), selected_path.get()) download class="text-[10px] text-blue-500 hover:underline">"Download Raw"</a> }
                    }
                </div>
                <div class="flex-grow flex flex-col min-h-0 bg-slate-950 overflow-hidden text-slate-300 relative justify-center">
                    {
                        let prev_exp_id = exp_id.clone();
                        let prev_run_id = run_id.clone();
                        move || {
                            let path = selected_path.get();
                            let ext = path.split('.').next_back().unwrap_or("").to_lowercase();
                            let is_video = matches!(ext.as_str(), "mp4" | "webm" | "ogg");
                            let is_audio = matches!(ext.as_str(), "mp3" | "wav" | "flac");
                            let is_image = matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "svg" | "gif" | "webp");

                            let media_url = format!("/api/experiments/{}/runs/{}/artifacts/content?path={}", prev_exp_id.clone(), prev_run_id.clone(), path);

                        if is_video {
                           view! {
                               <div class="flex items-center justify-center p-4 w-full h-full">
                                   <video controls class="max-w-full max-h-full rounded-lg shadow-lg" src=media_url></video>
                               </div>
                           }.into_any()
                        } else if is_audio {
                           view! {
                               <div class="flex items-center justify-center p-8 w-full h-full">
                                   <audio controls class="w-full max-w-md shadow-lg" src=media_url></audio>
                               </div>
                           }.into_any()
                        } else if is_image {
                           view! {
                               <div class="flex items-center justify-center p-4 w-full h-full overflow-hidden">
                                   <div class="absolute inset-0 max-w-full max-h-full flex items-center justify-center p-4">
                                       <img class="max-w-full max-h-full object-contain rounded-lg shadow-lg" src=media_url />
                                   </div>
                               </div>
                           }.into_any()
                        } else {
                           view! {
                               <div class="flex-grow flex flex-col h-full w-full">
                                   <Suspense fallback=|| view! { <div class="p-8 animate-pulse space-y-2"><div class="h-2 bg-slate-800 rounded w-3/4"></div><div class="h-2 bg-slate-800 rounded w-1/2"></div></div> }>
                                       {move || Suspend::new(async move {
                                           let content = content_resource.await.unwrap_or_else(|e| format!("Error loading preview: {}", e));
                                           view! { <TabularPreview content=content /> }
                                       })}
                                   </Suspense>
                               </div>
                           }.into_any()
                        }
                    }}
                </div>
            </div>
        </div>
    }.into_any()
}

#[component]
fn ConsoleView(
    exp_id: String,
    selected: std::collections::HashSet<String>,
    runs: Vec<Run>,
) -> impl IntoView {
    if selected.is_empty() {
        return view! { <div class="p-12 text-center text-slate-500">"Select one or more runs to view live console output."</div> }.into_any();
    }

    let selected_runs: Vec<String> = selected.into_iter().collect();

    view! {
        <div class="flex-grow flex flex-col overflow-auto p-4 space-y-8 bg-black">
            {selected_runs.into_iter().map(|run| {
                 let status = runs.iter().find(|r| r.name == run).map(|r| r.status.clone()).unwrap_or_else(|| "UNKNOWN".to_string());
                 view! {
                     <div class="flex flex-col flex-shrink-0" style="min-height: 20rem;">
                         <div class="text-xs font-bold text-slate-400 border-b border-slate-800 pb-1 mb-2 uppercase">
                             "Run: " {run.clone()} " - " {status.clone()}
                         </div>
                         <SingleConsoleView exp_id=exp_id.clone() run_id=run status=status />
                     </div>
                 }
            }).collect_view()}
        </div>
    }.into_any()
}

#[component]
fn SingleConsoleView(exp_id: String, run_id: String, status: String) -> impl IntoView {
    let (logs, set_logs) = signal(Vec::<String>::new());
    let is_connected = Rc::new(Cell::new(true));

    let exp_id_val = StoredValue::new(exp_id.clone());
    let run_id_val = StoredValue::new(run_id.clone());

    let on_message = wasm_bindgen::prelude::Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
        move |e: web_sys::MessageEvent| {
            if let Some(data) = e.data().as_string() {
                if !data.is_empty() {
                    set_logs.update(|l| l.push(data));
                }
            }
        },
    );

    let url = format!(
        "/api/experiments/{}/runs/{}/log/stream",
        exp_id_val.with_value(|v| v.clone()),
        run_id_val.with_value(|v| v.clone())
    );
    let event_source = web_sys::EventSource::new(&url).unwrap();
    let es_clone = event_source.clone();

    let is_connected_clone = is_connected.clone();
    let on_error = wasm_bindgen::prelude::Closure::<dyn FnMut(web_sys::Event)>::new(
        move |_e: web_sys::Event| {
            if es_clone.ready_state() == web_sys::EventSource::CLOSED {
                is_connected_clone.set(false);
                set_logs.update(|l| l.push("[system] Connection permanently closed.".to_string()));
            }
        },
    );

    event_source.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    event_source.set_onerror(Some(on_error.as_ref().unchecked_ref()));

    on_message.forget();
    on_error.forget();

    on_cleanup(move || {
        event_source.close();
    });

    view! {
        <div class="flex-grow flex flex-col overflow-hidden font-mono text-xs">
            <div class="flex-grow overflow-auto space-y-1 custom-scrollbar" id="console-scroll">
                <For
                    each=move || logs.get().into_iter().enumerate()
                    key=|(i, _)| *i
                    children=|(_, line)| view! { <div class="text-white whitespace-pre-wrap">{line}</div> }
                />
            </div>
            <div class="mt-4 pt-4 border-t border-slate-800 flex items-center justify-between">
                {
                    let connected = is_connected.get();
                    if connected && status == "RUNNING" {
                        view! {
                            <span class="text-slate-600">"Streaming Live"</span>
                            <span class="text-blue-500 animate-pulse">"●"</span>
                        }.into_any()
                    } else if status == "FAILED" {
                        view! {
                            <span class="text-slate-600">"Run Failed / Connection Closed"</span>
                            <span class="text-red-500">"●"</span>
                        }.into_any()
                    } else {
                        view! {
                            <span class="text-slate-600">"Run Completed / Connection Closed"</span>
                            <span class="text-emerald-500">"●"</span>
                        }.into_any()
                    }
                }
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
struct BackendInfo {
    backend: String,
}

#[derive(Clone, Debug, Deserialize)]
struct JupyterStatus {
    running: bool,
    port: Option<u16>,
}

#[derive(Clone, Debug, Deserialize)]
struct JupyterStartResponse {
    port: u16,
}

async fn check_backend() -> Result<BackendInfo, String> {
    let resp = gloo_net::http::Request::get("/api/jupyter/available")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
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

#[derive(Clone, Debug, Deserialize)]
struct NotebookInfo {
    exists: bool,
    content: Option<String>,
}

async fn fetch_notebook_content(exp: String, run: String) -> Result<NotebookInfo, String> {
    let resp = gloo_net::http::Request::get(&format!(
        "/api/experiments/{}/runs/{}/jupyter/notebook",
        exp, run
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

async fn create_default_notebook(exp: String, run: String) -> Result<String, String> {
    let resp = gloo_net::http::Request::post(&format!(
        "/api/experiments/{}/runs/{}/jupyter/notebook",
        exp, run
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;

    let text = resp.text().await.map_err(|e| e.to_string())?;
    let parsed: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(parsed["content"].as_str().unwrap_or("").to_string())
}

/// Extract human-readable source code from ipynb JSON cells.
fn extract_cell_sources(ipynb_content: &str) -> Vec<String> {
    let parsed: serde_json::Value = match serde_json::from_str(ipynb_content) {
        Ok(v) => v,
        Err(_) => return vec![ipynb_content.to_string()],
    };
    let cells = match parsed["cells"].as_array() {
        Some(c) => c,
        None => return vec![],
    };
    cells
        .iter()
        .filter_map(|cell| {
            let source = &cell["source"];
            if let Some(arr) = source.as_array() {
                Some(
                    arr.iter()
                        .filter_map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(""),
                )
            } else {
                source.as_str().map(|s| s.to_string())
            }
        })
        .collect()
}

#[component]
fn InteractiveView(exp_id: String, selected: std::collections::HashSet<String>) -> impl IntoView {
    if selected.is_empty() {
        let v: AnyView = view! {
            <div class="flex-grow flex flex-col items-center justify-center p-12 text-center space-y-4">
                <div class="p-4 bg-slate-800 rounded-full text-blue-500">
                    <FlaskConical size=28 />
                </div>
                <h3 class="text-xl font-bold text-white">"No Runs Selected"</h3>
                <p class="text-slate-400 max-w-sm">"Please select a single run from the left sidebar to launch an interactive session."</p>
            </div>
        }.into_any();
        return v;
    }

    if selected.len() > 1 {
        let v: AnyView = view! {
            <div class="flex-grow flex flex-col items-center justify-center p-12 text-center space-y-4">
                <div class="p-4 bg-slate-800 rounded-full text-blue-500">
                    <FlaskConical size=28 />
                </div>
                <h3 class="text-xl font-bold text-white">"Too Many Runs Selected"</h3>
                <p class="text-slate-400 max-w-sm">"Please select only one run for an interactive session."</p>
            </div>
        }.into_any();
        return v;
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
    let (is_ready, set_is_ready) = signal(false);
    let (notebook_version, set_notebook_version) = signal(0u32);

    Effect::new(move |_| {
        if let Some(Ok(status)) = jupyter_status.get().as_ref() {
            if status.running {
                set_jupyter_port.set(status.port);
                set_is_ready.set(true);
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

    let backend_info = LocalResource::new(|| async move { check_backend().await });

    // Fetch notebook content from server (reactive to notebook_version for refresh)
    let nb_exp_id = exp_id_outer.clone();
    let nb_run_id = run_id_outer.clone();
    let notebook_resource = LocalResource::new(move || {
        let eid = nb_exp_id.clone();
        let rid = nb_run_id.clone();
        let _version = notebook_version.get();
        async move { fetch_notebook_content(eid, rid).await }
    });

    view! {
        <div class="flex-grow p-6 space-y-6 overflow-auto bg-[#e5e5e5] dark:bg-slate-950 flex flex-col h-full">
            <Suspense fallback=|| view! { <div class="p-8 text-center text-slate-500 animate-pulse">"Loading notebook status..."</div> }>
                {move || {
                    let port_opt = jupyter_port.get();
                    let loading = is_loading.get();
                    let exp_id_outer = exp_id_outer.clone();
                    let run_id_outer = run_id_outer.clone();
                    let rt_exp_id = exp_id_outer.clone();
                    let rt_run_id = run_id_outer.clone();

                    let start_notebook = move |_| {
                        let eid = rt_exp_id.clone();
                        let rid = rt_run_id.clone();
                        set_is_loading.set(true);
                        set_is_ready.set(false);
                        spawn_local(async move {
                            if let Ok(port) = start_jupyter(eid, rid).await {
                                set_jupyter_port.set(Some(port));
                                // Ping the root URL until it responds.
                                // We use NoCors mode because the dashboard and jupyter are on different ports.
                                // Any response (even a 302 redirect) indicates the server is UP.
                                let url = format!("http://localhost:{}/", port);
                                for _ in 0..20 { // Try for 20 seconds
                                    let resp = gloo_net::http::Request::get(&url)
                                        .mode(RequestMode::NoCors)
                                        .send()
                                        .await;
                                    if resp.is_ok() {
                                        set_is_ready.set(true);
                                        break;
                                    }
                                    gloo_timers::future::TimeoutFuture::new(1000).await;
                                }
                                // Fallback: set ready even if ping fails after timeout to show the iframe (user can refresh)
                                set_is_ready.set(true);
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
                        let run = run_data.await;
                        let notebook_info = notebook_resource.await;

                        let backend = backend_info.await;
                        let backend_str = backend.as_ref().map(|b| b.backend.clone()).unwrap_or_else(|_| "none".to_string());

                        let view_result: leptos::prelude::AnyView = match run {
                            Ok(run_info) => {
                                let name_str = run_info.name.clone();

                                let nb_exists = notebook_info.as_ref().map(|n| n.exists).unwrap_or(false);
                                let nb_sources: Vec<String> = notebook_info
                                    .as_ref()
                                    .ok()
                                    .and_then(|n| n.content.as_deref())
                                    .map(extract_cell_sources)
                                    .unwrap_or_default();

                                let create_exp_id = exp_id_outer.clone();
                                let create_run_id = run_id_outer.clone();
                                let create_notebook_click = move |_| {
                                    let eid = create_exp_id.clone();
                                    let rid = create_run_id.clone();
                                    set_is_loading.set(true);
                                    spawn_local(async move {
                                        let _ = create_default_notebook(eid, rid).await;
                                        set_notebook_version.update(|v| *v += 1);
                                        set_is_loading.set(false);
                                    });
                                };

                                if let Some(p) = port_opt {
                                    // Jupyter currently running — show iframe
                                    let url = format!("http://localhost:{}/notebooks/interactive.ipynb", p);
                                    view! {
                                        <div class="flex flex-col h-full space-y-4 min-h-[700px]">
                                            <div class="flex justify-between items-center bg-white dark:bg-slate-900 p-4 rounded-lg shadow-sm border border-slate-300 dark:border-slate-700 mx-1">
                                                <div class="flex items-center space-x-3">
                                                    <div class="w-3 h-3 bg-green-500 rounded-full animate-pulse"></div>
                                                    <span class="font-semibold text-slate-800 dark:text-white">"Live Jupyter Notebook"</span>
                                                    <span class="text-xs text-slate-500 font-mono">{name_str}</span>
                                                </div>
                                                <div class="flex items-center space-x-3">
                                                    <a href=url.clone() target="_blank" class="px-4 py-2 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-300 text-sm font-medium rounded transition-colors border border-slate-300 dark:border-slate-600">
                                                        "Pop-out ↗"
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
                                            <div class="flex-grow bg-white dark:bg-slate-900 border border-slate-300 dark:border-slate-700 rounded-lg overflow-hidden shadow-sm mx-1 relative">
                                                {move || if is_ready.get() {
                                                    view! {
                                                        <iframe src=url.clone() class="w-full h-full border-none min-h-[600px]"/>
                                                    }.into_any()
                                                } else {
                                                    view! {
                                                        <div class="absolute inset-0 flex flex-col items-center justify-center bg-white dark:bg-slate-900 space-y-4">
                                                            <div class="flex space-x-2">
                                                                 <div class="w-3 h-3 bg-blue-500 rounded-full animate-bounce [animation-delay:-0.3s]"></div>
                                                                 <div class="w-3 h-3 bg-blue-500 rounded-full animate-bounce [animation-delay:-0.15s]"></div>
                                                                 <div class="w-3 h-3 bg-blue-500 rounded-full animate-bounce"></div>
                                                            </div>
                                                            <span class="text-sm text-slate-500 animate-pulse">"Waiting for Jupyter server to initialize..."</span>
                                                        </div>
                                                    }.into_any()
                                                }}
                                            </div>
                                        </div>
                                    }.into_any()
                                } else if backend_str == "jupyter" {
                                    // Jupyter available but not running — launch button + notebook preview/create
                                    view! {
                                        <div class="max-w-4xl mx-auto w-full space-y-6">
                                            <div class="bg-white dark:bg-slate-900 rounded-lg shadow-sm border border-slate-300 dark:border-slate-700 p-8 text-center space-y-4">
                                                <div class="mx-auto w-16 h-16 bg-blue-100 dark:bg-blue-900/40 text-blue-600 dark:text-blue-400 rounded-full flex items-center justify-center mb-4">
                                                    <SettingsIcon size=28 />
                                                </div>
                                                <h3 class="text-2xl font-bold text-slate-800 dark:text-white">"Interactive Analysis"</h3>
                                                <p class="text-slate-500 max-w-lg mx-auto leading-relaxed">
                                                    "Launch a live Jupyter Notebook for run "
                                                    <span class="font-mono font-medium text-slate-700 dark:text-slate-300">{name_str}</span>
                                                    "."
                                                </p>
                                                <div class="pt-6 flex items-center justify-center space-x-3">
                                                    <button
                                                        class="px-8 py-3 bg-blue-600 hover:bg-blue-700 focus:ring focus:ring-blue-500/50 text-white font-medium rounded-lg transition-all flex items-center justify-center space-x-2 disabled:opacity-50 disabled:cursor-not-allowed shadow-md hover:shadow-lg"
                                                        on:click=start_notebook
                                                        disabled=move || loading || !nb_exists
                                                    >
                                                        <span>{
                                                            if loading { "Launching Notebook..." }
                                                            else if !nb_exists { "Create Notebook First" }
                                                            else { "▶ Launch Live Jupyter Notebook" }
                                                        }</span>
                                                    </button>
                                                    {if !nb_exists {
                                                        view! {
                                                            <button
                                                                class="px-6 py-3 bg-emerald-600 hover:bg-emerald-700 text-white font-medium rounded-lg transition-all disabled:opacity-50 shadow-md hover:shadow-lg"
                                                                on:click=create_notebook_click.clone()
                                                                disabled=loading
                                                            >
                                                                {if loading { "Creating..." } else { "✨ Create Notebook" }}
                                                            </button>
                                                        }.into_any()
                                                    } else {
                                                        view! { <span class="hidden"></span> }.into_any()
                                                    }}
                                                </div>
                                            </div>
                                            // Show notebook cell preview
                                            {if !nb_sources.is_empty() {
                                                view! {
                                                    <div class="space-y-4">
                                                        {nb_sources.into_iter().enumerate().map(|(i, src)| {
                                                            view! {
                                                                <div class="bg-white dark:bg-slate-900 border border-slate-300 dark:border-slate-700 rounded-lg overflow-hidden shadow-sm">
                                                                    <div class="flex bg-slate-50 dark:bg-slate-800 border-b border-slate-300 dark:border-slate-700 px-4 py-3 text-xs text-slate-500 items-center">
                                                                        <span class="font-mono bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-400 px-2 py-0.5 rounded font-bold">
                                                                            {format!("In [{}]:", i + 1)}
                                                                        </span>
                                                                    </div>
                                                                    <div class="p-5 font-mono text-sm overflow-x-auto text-slate-800 dark:text-slate-300 bg-slate-50 dark:bg-slate-950">
                                                                        <pre><code class="leading-relaxed">{src}</code></pre>
                                                                    </div>
                                                                </div>
                                                            }
                                                        }).collect_view()}
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! { <span class="hidden"></span> }.into_any()
                                            }}
                                        </div>
                                    }.into_any()
                                } else {
                                    // No Jupyter — ipython/python fallback: show code cells with copy guidance
                                    let tool_name = "Python";
                                    let tool_cmd = "python3";
                                    view! {
                                        <div class="max-w-4xl mx-auto w-full space-y-6">
                                            <div class="bg-white dark:bg-slate-900 rounded-lg shadow-sm border border-slate-300 dark:border-slate-700 p-8 text-center space-y-4">
                                                <div class="mx-auto w-16 h-16 bg-amber-100 dark:bg-amber-900/40 text-amber-600 dark:text-amber-400 rounded-full flex items-center justify-center mb-4">
                                                    <TriangleAlert size=28 />
                                                </div>
                                                <h3 class="text-2xl font-bold text-slate-800 dark:text-white">"Interactive Analysis"</h3>
                                                <p class="text-slate-500 max-w-lg mx-auto leading-relaxed">
                                                    "Jupyter is not available in this environment. You can use "
                                                    <span class="font-semibold text-slate-700 dark:text-slate-300">{tool_name}</span>
                                                    " to run the notebook code in your terminal:"
                                                </p>
                                                <code class="bg-slate-800 border border-slate-700 px-4 py-2 rounded-lg text-emerald-400 font-mono text-sm inline-block">
                                                    {format!("cd <run_dir> && {}", tool_cmd)}
                                                </code>
                                                <p class="text-xs text-slate-400">
                                                    "Install Jupyter for an embedded notebook experience: "
                                                    <code class="bg-slate-100 dark:bg-slate-800 px-1 rounded">"pip install notebook"</code>
                                                </p>
                                                {if !nb_exists {
                                                    view! {
                                                        <div class="pt-4">
                                                            <button
                                                                class="px-6 py-3 bg-emerald-600 hover:bg-emerald-700 text-white font-medium rounded-lg transition-all disabled:opacity-50 shadow-md hover:shadow-lg"
                                                                on:click=create_notebook_click.clone()
                                                                disabled=loading
                                                            >
                                                                {if loading { "Creating..." } else { "✨ Create Notebook" }}
                                                            </button>
                                                        </div>
                                                    }.into_any()
                                                } else {
                                                    view! { <span class="hidden"></span> }.into_any()
                                                }}
                                            </div>
                                            // Show notebook cells as copyable code
                                            {if !nb_sources.is_empty() {
                                                view! {
                                                    <div class="space-y-4">
                                                        {nb_sources.into_iter().enumerate().map(|(i, src)| {
                                                            view! {
                                                                <div class="bg-white dark:bg-slate-900 border border-slate-300 dark:border-slate-700 rounded-lg overflow-hidden shadow-sm">
                                                                    <div class="flex bg-slate-50 dark:bg-slate-800 border-b border-slate-300 dark:border-slate-700 px-4 py-3 text-xs text-slate-500 items-center justify-between">
                                                                        <span class="font-mono bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-400 px-2 py-0.5 rounded font-bold">
                                                                            {format!("In [{}]:", i + 1)}
                                                                        </span>
                                                                        <span class="text-slate-400 text-xs">"Copy and paste into your terminal"</span>
                                                                    </div>
                                                                    <div class="p-5 font-mono text-sm overflow-x-auto text-slate-800 dark:text-slate-300 bg-slate-50 dark:bg-slate-950">
                                                                        <pre><code class="leading-relaxed">{src}</code></pre>
                                                                    </div>
                                                                </div>
                                                            }
                                                        }).collect_view()}
                                                    </div>
                                                }.into_any()
                                            } else if nb_exists {
                                                view! {
                                                    <div class="p-6 text-center text-slate-500">"Notebook exists but has no cells."</div>
                                                }.into_any()
                                            } else {
                                                view! { <span class="hidden"></span> }.into_any()
                                            }}
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

#[component]
fn RunsTableView(runs: Vec<Run>, #[prop(into)] on_edit: Callback<Run>) -> impl IntoView {
    // Which metric columns are currently visible (None = all visible)
    let (selected_metrics, set_selected_metrics) =
        signal(std::collections::HashSet::<String>::new());
    let (metrics_initialized, set_metrics_initialized) = signal(false);

    if runs.is_empty() {
        return view! {
            <div class="flex-grow p-6 overflow-auto space-y-4">
                <div class="p-12 text-center text-slate-500">"No runs found for this experiment."</div>
            </div>
        }.into_any();
    }

    // Collect all unique scalar keys (sorted)
    let all_scalar_keys: Vec<String> = {
        let mut keys = std::collections::BTreeSet::new();
        for run in &runs {
            if let Some(scalars) = &run.scalars {
                for key in scalars.keys() {
                    keys.insert(key.clone());
                }
            }
        }
        keys.into_iter().collect()
    };

    // Collect all unique tags
    let all_tags: Vec<String> = {
        let mut tags = std::collections::BTreeSet::new();
        for run in &runs {
            if let Some(t) = &run.tags {
                for val in t {
                    tags.insert(val.clone());
                }
            }
        }
        tags.into_iter().collect()
    };

    let (selected_meta, set_selected_meta) = signal({
        let mut s = std::collections::HashSet::new();
        s.insert("Status".to_string());
        s.insert("Duration".to_string());
        s.insert("Started".to_string());
        s.insert("Finished".to_string());
        s
    });
    let metadata_cols = vec![
        "Status".to_string(),
        "Duration".to_string(),
        "Started".to_string(),
        "Finished".to_string(),
    ];

    // Which tags are selected for filtering (None = all visible)
    let (selected_tags, set_selected_tags) = signal(std::collections::HashSet::<String>::new());

    // Initialize selected_metrics to all keys on first load
    if !metrics_initialized.get() && !all_scalar_keys.is_empty() {
        set_selected_metrics.set(all_scalar_keys.iter().cloned().collect());
        set_metrics_initialized.set(true);
    }

    let runs_for_filter = runs.clone();
    let filtered_runs = move || {
        let active_tags = selected_tags.get();
        if active_tags.is_empty() {
            runs_for_filter.clone()
        } else {
            runs_for_filter
                .clone()
                .into_iter()
                .filter(|r| {
                    let current_run_tags: std::collections::HashSet<String> =
                        r.tags.clone().unwrap_or_default().into_iter().collect();
                    active_tags.iter().all(|t| current_run_tags.contains(t))
                })
                .collect::<Vec<_>>()
        }
    };

    let keys_for_filter = all_scalar_keys.clone();
    let keys_for_table = all_scalar_keys.clone();
    // Clones for export closures
    let runs_for_csv = runs.clone();
    let runs_for_latex = runs.clone();
    let runs_for_typst = runs.clone();
    let keys_csv = all_scalar_keys.clone();
    let keys_latex = all_scalar_keys.clone();
    let keys_typst = all_scalar_keys.clone();

    // ── Helper: trigger a browser file download ───────────────────
    fn trigger_download(content: &str, filename: &str, mime: &str) {
        use wasm_bindgen::JsCast;
        let blob_parts = js_sys::Array::new();
        blob_parts.push(&wasm_bindgen::JsValue::from_str(content));
        let opts = web_sys::BlobPropertyBag::new();
        opts.set_type(mime);
        let blob = web_sys::Blob::new_with_str_sequence_and_options(&blob_parts, &opts).unwrap();
        let url = web_sys::Url::create_object_url_with_blob(&blob).unwrap();
        let doc = web_sys::window().unwrap().document().unwrap();
        let a: web_sys::HtmlAnchorElement = doc.create_element("a").unwrap().dyn_into().unwrap();
        a.set_href(&url);
        a.set_download(filename);
        a.click();
        let _ = web_sys::Url::revoke_object_url(&url);
    }

    // ── Export: CSV ───────────────────────────────────────────────
    let export_csv = move |_| {
        let sel: std::collections::HashSet<String> = selected_metrics.get();
        let active_keys: Vec<&String> = keys_csv.iter().filter(|k| sel.contains(*k)).collect();
        let mut csv = String::from("Run ID,Status,");
        csv.push_str(
            &active_keys
                .iter()
                .map(|k| k.as_str())
                .collect::<Vec<_>>()
                .join(","),
        );
        csv.push_str(",Duration,Started,Finished,Description,Tags\n");
        for run in &runs_for_csv {
            let scalars = run.scalars.clone().unwrap_or_default();
            let dur = run
                .duration_secs
                .map(|d| format!("{:.1}", d))
                .unwrap_or("-".into());
            let desc = run
                .description
                .clone()
                .unwrap_or_default()
                .replace(',', ";");
            let tags = run.tags.clone().unwrap_or_default().join("; ");
            csv.push_str(&format!("{},{},", run.name, run.status));
            for k in &active_keys {
                let v = scalars.get(*k).map(|v| v.to_string()).unwrap_or("-".into());
                csv.push_str(&format!("{},", v));
            }
                let finished = run.finished_at.as_ref().map(|f| f.as_str()).unwrap_or("-");
                csv.push_str(&format!("{},{},{},{},{}\n", dur, run.started_at, finished, desc, tags));
        }
        trigger_download(&csv, "runs.csv", "text/csv");
    };

    // ── Export: LaTeX ─────────────────────────────────────────────
    let export_latex = move |_| {
        let sel: std::collections::HashSet<String> = selected_metrics.get();
        let active_keys: Vec<&String> = keys_latex.iter().filter(|k| sel.contains(*k)).collect();
        let ncols = 3 + active_keys.len() + 3; // Run, Status, metrics…, Duration, Started, Desc
        let col_spec = format!("{{{}}}", "l".repeat(ncols));
        let mut tex = format!("\\begin{{tabular}}{}\n\\toprule\nRun ID & Status", col_spec);
        for k in &active_keys {
            tex.push_str(&format!(" & {}", k));
        }
        tex.push_str(" & Duration & Started & Finished & Description \\\\\n\\midrule\n");
        for run in &runs_for_latex {
            let scalars = run.scalars.clone().unwrap_or_default();
            let dur = run
                .duration_secs
                .map(|d| format!("{:.1}s", d))
                .unwrap_or("-".into());
            let desc = run.description.clone().unwrap_or_default();
            tex.push_str(&format!("{} & {}", run.name, run.status));
            for k in &active_keys {
                let v = scalars.get(*k).map(|v| v.to_string()).unwrap_or("-".into());
                tex.push_str(&format!(" & {}", v));
            }
            let finished = run.finished_at.as_ref().map(|f| format_date(f)).unwrap_or_else(|| "-".into());
            tex.push_str(&format!(
                " & {} & {} & {} & {} \\\\\n",
                dur,
                format_date(&run.started_at),
                finished,
                desc
            ));
        }
        tex.push_str("\\bottomrule\n\\end{tabular}\n");
        trigger_download(&tex, "runs.tex", "application/x-tex");
    };

    // ── Export: Typst ─────────────────────────────────────────────
    let export_typst = move |_| {
        let sel: std::collections::HashSet<String> = selected_metrics.get();
        let active_keys: Vec<&String> = keys_typst.iter().filter(|k| sel.contains(*k)).collect();
        let ncols = 3 + active_keys.len() + 3;
        let mut typ = format!("#table(\n  columns: {},\n  ", ncols);
        // Header row
        typ.push_str("[*Run ID*], [*Status*]");
        for k in &active_keys {
            typ.push_str(&format!(", [*{}*]", k));
        }
        typ.push_str(", [*Duration*], [*Started*], [*Finished*], [*Description*],\n");
        for run in &runs_for_typst {
            let scalars = run.scalars.clone().unwrap_or_default();
            let dur = run
                .duration_secs
                .map(|d| format!("{:.1}s", d))
                .unwrap_or("-".into());
            let desc = run.description.clone().unwrap_or_default();
            typ.push_str(&format!("  [{}], [{}]", run.name, run.status));
            for k in &active_keys {
                let v = scalars.get(*k).map(|v| v.to_string()).unwrap_or("-".into());
                typ.push_str(&format!(", [{}]", v));
            }
            let finished = run.finished_at.as_ref().map(|f| format_date(f)).unwrap_or_else(|| "-".into());
            typ.push_str(&format!(
                ", [{}], [{}], [{}], [{}],\n",
                dur,
                format_date(&run.started_at),
                finished,
                desc
            ));
        }
        typ.push_str(")\n");
        trigger_download(&typ, "runs.typ", "text/plain");
    };

    view! {
            <div class="flex items-center justify-between flex-shrink-0 flex-wrap gap-2 mb-2">
                <div class="flex items-center gap-2 pl-4">
                    <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider mr-1">"Export:"</span>
                    <button on:click=export_csv class="px-3 py-1.5 rounded-lg text-xs font-medium bg-emerald-600/15 border border-emerald-500/30 text-emerald-400 hover:bg-emerald-600/25 transition-all">"CSV"</button>
                    <button on:click=export_latex class="px-3 py-1.5 rounded-lg text-xs font-medium bg-violet-600/15 border border-violet-500/30 text-violet-400 hover:bg-violet-600/25 transition-all">"LaTeX"</button>
                    <button on:click=export_typst class="px-3 py-1.5 rounded-lg text-xs font-medium bg-amber-600/15 border border-amber-500/30 text-amber-400 hover:bg-amber-600/25 transition-all">"Typst"</button>
                </div>
            </div>
            <div class="flex items-center justify-between flex-shrink-0 flex-wrap gap-2 mb-4">
                <div class="flex flex-col gap-2">
                    <div class="flex flex-wrap items-center gap-2 pl-4">
                        <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider mr-1">"Metadata:"</span>
                        {metadata_cols.into_iter().map(|key| {
                            let k1 = key.clone();
                            let k2 = key.clone();
                            let is_on = Signal::derive(move || selected_meta.with(|s| s.contains(&k1)));
                            view! {
                                <button
                                    on:click=move |_| {
                                        let k = k2.clone();
                                        set_selected_meta.update(|s| {
                                            if s.contains(&k) { s.remove(&k); } else { s.insert(k); }
                                        });
                                    }
                                    class=move || format!(
                                        "px-3 py-1 rounded-full text-xs font-medium border transition-all duration-150 {}",
                                        if is_on.get() {
                                            "bg-violet-600/20 border-violet-500/50 text-violet-300 hover:bg-violet-600/30"
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
                    {if !keys_for_filter.is_empty() {
                        view! {
                            <div class="flex flex-wrap items-center gap-2 pl-4">
                                <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider mr-1">"Scalars:"</span>
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

                    {if !all_tags.is_empty() {
                        view! {
                            <div class="flex flex-wrap items-center gap-2 pl-4">
                                <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider mr-1">"Tags:"</span>
                                {all_tags.into_iter().map(|tag| {
                                    let t1 = tag.clone();
                                    let t2 = tag.clone();
                                    let is_on = Signal::derive(move || selected_tags.with(|s| s.contains(&t1)));
                                    view! {
                                        <button
                                            on:click=move |_| {
                                                let t = t2.clone();
                                                set_selected_tags.update(|s| {
                                                    if s.contains(&t) { s.remove(&t); } else { s.insert(t); }
                                                });
                                            }
                                            class=move || format!(
                                                "px-3 py-1 rounded-full text-[10px] font-medium border transition-all duration-150 {}",
                                                if is_on.get() {
                                                    "bg-emerald-600/20 border-emerald-500/50 text-emerald-300 hover:bg-emerald-600/30"
                                                } else {
                                                    "bg-slate-800 border-slate-700 text-slate-500 hover:border-slate-600 hover:text-slate-400"
                                                }
                                            )
                                        >
                                            {tag}
                                        </button>
                                    }
                                }).collect_view()}
                            </div>
                        }.into_any()
                    } else {
                        view! { <div class="hidden"></div> }.into_any()
                    }}
                </div>
            </div>

            // ── Scrollable runs table ─────────────────────────────────
            <div class="bg-slate-900 border border-slate-800 rounded-xl overflow-auto flex-grow min-h-0">
                <table class="w-full text-left border-collapse">
                    <thead class="bg-slate-950 text-xs uppercase text-slate-500 font-semibold sticky top-0 z-10">
                        <tr>
                            <th class="p-4 border-b border-slate-800">"Run ID"</th>
                            {move || if selected_meta.with(|s| s.contains("Status")) { view! { <th class="p-4 border-b border-slate-800">"Status"</th> }.into_any() } else { view!{<th class="hidden"></th>}.into_any() } }
                            {
                                let kt = keys_for_table.clone();
                                move || kt.clone().into_iter().filter(|k| selected_metrics.with(|s| s.contains(k))).map(|k| view! {
                                    <th class="p-4 border-b border-slate-800 text-blue-400">{k}</th>
                                }).collect_view()
                            }
                            {move || if selected_meta.with(|s| s.contains("Duration")) { view! { <th class="p-4 border-b border-slate-800">"Duration"</th> }.into_any() } else { view!{<th class="hidden"></th>}.into_any() } }
                            {move || if selected_meta.with(|s| s.contains("Started")) { view! { <th class="p-4 border-b border-slate-800">"Started"</th> }.into_any() } else { view!{<th class="hidden"></th>}.into_any() } }
                            {move || if selected_meta.with(|s| s.contains("Finished")) { view! { <th class="p-4 border-b border-slate-800">"Finished"</th> }.into_any() } else { view!{<th class="hidden"></th>}.into_any() } }
                            <th class="p-4 border-b border-slate-800">"Description / Tags"</th>
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-slate-800/50 text-sm text-slate-300">
                        {move || filtered_runs().into_iter().map(|run| {
                            let run = run.clone();
                            let duration = run.duration_secs.map(|d| format!("{:.1}s", d)).unwrap_or("-".to_string());
                            let (status_color, status_bg, status_border) = match run.status.as_str() {
                                "RUNNING"   => ("text-blue-400",   "bg-blue-500",   "border-blue-500"),
                                "COMPLETED" => ("text-emerald-400", "bg-emerald-500", "border-emerald-500"),
                                "FAILED"    => ("text-red-400",     "bg-red-500",     "border-red-500"),
                                _           => ("text-slate-400",   "bg-slate-600",   "border-slate-500"),
                            };
                            let dot_class = if run.status == "RUNNING" { "animate-pulse" } else { "" };
                            let run_scalars = run.scalars.clone().unwrap_or_default();
                            let scalar_cols: Vec<String> = keys_for_table.iter()
                                .filter(|k| selected_metrics.with(|s| s.contains(*k)))
                                .cloned()
                                .collect();

                            let run_for_edit = run.clone();
                            let name = run.name.clone();
                            let status = run.status.clone();
                            let desc = run.description.clone();
                            let tags = run.tags.clone();

                            view! {
                                <tr class="hover:bg-slate-800/30 transition-colors group">
                                    <td class="p-4 font-mono text-white flex items-center space-x-2">
                                        <div class=format!("w-2 h-2 rounded-full {} {}", status_bg, dot_class)></div>
                                        <span>{name}</span>
                                    </td>
                                    {if selected_meta.with(|s| s.contains("Status")) {
                                        let my_status = status.clone();
                                        view! {
                                            <td class="p-4">
                                                <span class=format!("px-2 py-1 rounded text-xs font-medium bg-opacity-10 border border-opacity-20 {} {} {}", status_bg, status_color, status_border)>
                                                    {my_status}
                                                </span>
                                            </td>
                                        }.into_any()
                                    } else { view!{<td class="hidden"></td>}.into_any() }}

                                    {scalar_cols.clone().into_iter().map(|k| {
                                        let val = run_scalars.get(&k)
                                            .map(|v| v.to_string())
                                            .unwrap_or_else(|| "-".to_string());
                                        view! { <td class="p-4 font-mono text-slate-400">{val}</td> }
                                    }).collect_view()}

                                    {if selected_meta.with(|s| s.contains("Duration")) {
                                        let d = duration.clone();
                                        view! { <td class="p-4 font-mono text-slate-400">{d}</td> }.into_any()
                                    } else { view!{<td class="hidden"></td>}.into_any() }}

                                    {if selected_meta.with(|s| s.contains("Started")) {
                                        view! { <td class="p-4 text-slate-400 whitespace-nowrap">{format_date(&run.started_at)}</td> }.into_any()
                                    } else { view!{<td class="hidden"></td>}.into_any() }}

                                    {if selected_meta.with(|s| s.contains("Finished")) {
                                        let finished = run.finished_at.as_ref().map(|f| format_date(f)).unwrap_or_else(|| "-".to_string());
                                        view! { <td class="p-4 text-slate-400 whitespace-nowrap">{finished}</td> }.into_any()
                                    } else { view!{<td class="hidden"></td>}.into_any() }}

                                    <td class="p-4 transition-colors max-w-sm">
                                        <div class="flex items-start justify-between group/cell">
                                            <div class="flex-grow">
                                                <div class="text-slate-300 font-medium">{desc.unwrap_or_default()}</div>
                                                <div class="flex flex-wrap gap-1 mt-1 empty:hidden">
                                                    {tags.unwrap_or_default().into_iter().map(|t| view! {
                                                        <span class="px-2 py-0.5 bg-blue-500/10 text-blue-400 rounded-md text-[10px] border border-blue-500/20">{t}</span>
                                                    }).collect_view()}
                                                </div>
                                            </div>
                                            <button
                                                on:click=move |_| on_edit.run(run_for_edit.clone())
                                                class="ml-2 p-1.5 text-slate-500 hover:text-blue-400 opacity-0 group-hover:opacity-100 transition-opacity rounded-md hover:bg-slate-800 shrink-0"
                                                title="Edit Run"
                                            >
                                                <SettingsIcon size=14 />
                                            </button>
                                        </div>
                                    </td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
    }.into_any()
}
