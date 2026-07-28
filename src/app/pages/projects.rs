//! Projects listing page with create modal.

use leptos::prelude::*;
use leptos_router::components::A;
use lucide_leptos::{FolderKanban, Plus, X};

use crate::app::components::ErrorState;
use crate::app::fetch;

#[component]
pub(crate) fn Projects() -> impl IntoView {
    let projects = LocalResource::new(fetch::fetch_projects);
    let show_create = RwSignal::new(false);
    let new_name = RwSignal::new(String::new());
    let new_display = RwSignal::new(String::new());
    let new_desc = RwSignal::new(String::new());
    let create_error = RwSignal::new(Option::<String>::None);

    let on_create = move |_| {
        let name = new_name.get_untracked();
        if name.trim().is_empty() {
            create_error.set(Some("Project name cannot be empty".to_string()));
            return;
        }
        let display = new_display.get_untracked();
        let desc = new_desc.get_untracked();
        leptos::task::spawn_local(async move {
            match fetch::create_project(
                name,
                if display.is_empty() {
                    None
                } else {
                    Some(display)
                },
                if desc.is_empty() { None } else { Some(desc) },
            )
            .await
            {
                Ok(_) => {
                    show_create.set(false);
                    new_name.set(String::new());
                    new_display.set(String::new());
                    new_desc.set(String::new());
                    create_error.set(None);
                    projects.refetch();
                }
                Err(e) => create_error.set(Some(e)),
            }
        });
    };

    view! {
        <div class="space-y-6">
            <div class="flex items-center justify-between">
                <h1 class="text-3xl font-bold text-white">"Projects"</h1>
                <button
                    on:click=move |_| show_create.set(true)
                    class="flex items-center space-x-2 px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-lg transition-colors text-sm font-medium"
                >
                    <Plus size=16 />
                    <span>"New Project"</span>
                </button>
            </div>

            // Create modal
            {move || show_create.get().then(|| view! {
                <div class="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center">
                    <div class="bg-slate-900 border border-slate-700 rounded-2xl p-6 w-full max-w-md shadow-2xl">
                        <div class="flex items-center justify-between mb-6">
                            <h2 class="text-xl font-bold text-white">"Create Project"</h2>
                            <button
                                on:click=move |_| { show_create.set(false); create_error.set(None); }
                                class="text-slate-400 hover:text-white transition-colors"
                            >
                                <X size=20 />
                            </button>
                        </div>

                        {move || create_error.get().map(|err| view! {
                            <div class="mb-4 px-3 py-2 bg-red-900/30 border border-red-800 rounded-lg text-red-400 text-sm">
                                {err}
                            </div>
                        })}

                        <div class="space-y-4">
                            <div>
                                <label class="block text-sm font-medium text-slate-400 mb-1">"Project ID"</label>
                                <input
                                    type="text"
                                    placeholder="my-project"
                                    class="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-lg text-white placeholder:text-slate-600 focus:outline-none focus:border-blue-500 transition-colors"
                                    on:input=move |ev| new_name.set(event_target_value(&ev))
                                    prop:value=move || new_name.get()
                                />
                                <p class="text-xs text-slate-600 mt-1">"Used in URLs and file paths. No spaces."</p>
                            </div>
                            <div>
                                <label class="block text-sm font-medium text-slate-400 mb-1">"Display Name"</label>
                                <input
                                    type="text"
                                    placeholder="My Project"
                                    class="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-lg text-white placeholder:text-slate-600 focus:outline-none focus:border-blue-500 transition-colors"
                                    on:input=move |ev| new_display.set(event_target_value(&ev))
                                    prop:value=move || new_display.get()
                                />
                            </div>
                            <div>
                                <label class="block text-sm font-medium text-slate-400 mb-1">"Description"</label>
                                <textarea
                                    placeholder="What is this project about?"
                                    rows=3
                                    class="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-lg text-white placeholder:text-slate-600 focus:outline-none focus:border-blue-500 transition-colors resize-none"
                                    on:input=move |ev| new_desc.set(event_target_value(&ev))
                                    prop:value=move || new_desc.get()
                                ></textarea>
                            </div>
                        </div>

                        <div class="flex justify-end space-x-3 mt-6">
                            <button
                                on:click=move |_| { show_create.set(false); create_error.set(None); }
                                class="px-4 py-2 text-slate-400 hover:text-white transition-colors text-sm"
                            >
                                "Cancel"
                            </button>
                            <button
                                on:click=on_create
                                class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-lg transition-colors text-sm font-medium"
                            >
                                "Create"
                            </button>
                        </div>
                    </div>
                </div>
            })}

            // Projects grid
            <Suspense fallback=|| view! {
                <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                    <div class="bg-slate-900 border border-slate-800 rounded-xl h-40 animate-pulse"></div>
                    <div class="bg-slate-900 border border-slate-800 rounded-xl h-40 animate-pulse"></div>
                    <div class="bg-slate-900 border border-slate-800 rounded-xl h-40 animate-pulse"></div>
                </div>
            }>
                {move || Suspend::new(async move {
                    match projects.await {
                        Ok(projs) => {
                            if projs.is_empty() {
                                view! {
                                    <div class="bg-slate-900 border border-slate-800 rounded-xl p-12 text-center">
                                        <div class="flex justify-center mb-4 text-slate-600">
                                            <FolderKanban size=48 />
                                        </div>
                                        <p class="text-slate-400 text-lg font-medium mb-2">"No projects yet"</p>
                                        <p class="text-slate-600 text-sm">"Create your first project to organize experiments."</p>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                                        {projs.into_iter().map(|p| {
                                            let id = p.id.clone();
                                            view! {
                                                <A href=format!("/projects/{}", id) attr:class="block bg-slate-900 border border-slate-800 rounded-xl p-5 hover:border-slate-600 hover:bg-slate-800/50 transition-all duration-200 group">
                                                    <div class="flex items-start justify-between mb-3">
                                                        <div class="p-2 bg-blue-600/10 rounded-lg text-blue-400 group-hover:bg-blue-600/20 transition-colors">
                                                            <FolderKanban size=20 />
                                                        </div>
                                                        <span class="text-xs text-slate-600 font-mono">{p.experiments_count} " experiments"</span>
                                                    </div>
                                                    <h3 class="text-lg font-semibold text-white mb-1 group-hover:text-blue-400 transition-colors">{p.display_name}</h3>
                                                    <p class="text-sm text-slate-500 line-clamp-2">{p.description.unwrap_or_default()}</p>
                                                    {(!p.tags.is_empty()).then(|| view! {
                                                        <div class="flex flex-wrap gap-1 mt-3">
                                                            {p.tags.into_iter().map(|t| view! {
                                                                <span class="px-2 py-0.5 bg-slate-800 text-slate-400 rounded text-[10px]">{t}</span>
                                                            }).collect_view()}
                                                        </div>
                                                    })}
                                                </A>
                                            }
                                        }).collect_view()}
                                    </div>
                                }.into_any()
                            }
                        },
                        Err(err) => view! {
                            <ErrorState
                                title="Failed to Load Projects"
                                message=err
                                action_label="Retry"
                                on_action=Callback::new(move |_| { projects.refetch(); })
                            />
                        }.into_any(),
                    }
                })}
            </Suspense>
        </div>
    }
}
