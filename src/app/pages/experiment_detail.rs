//! Experiment detail page with run selection, tabs, and metadata editing.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;
use lucide_leptos::{Cog as SettingsIcon, FlaskConical, LayoutDashboard};
use std::rc::Rc;

use crate::app::components::*;
use crate::app::fetch::*;
use crate::app::models::*;
use crate::app::utils::SidebarContext;

#[component]
pub(crate) fn ExperimentDetail() -> impl IntoView {
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
