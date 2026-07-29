//! Project detail page with README and experiments list.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use lucide_leptos::{ChevronRight, Eye, FlaskConical, FolderKanban, Lock, Pencil, Save};

use crate::app::components::{ErrorState, HParams};
use crate::app::fetch;

fn render_markdown(md: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(md, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

#[component]
pub(crate) fn ProjectDetail() -> impl IntoView {
    let params = use_params_map();
    let project_id = move || params.read().get("id").unwrap_or_default();

    let project = LocalResource::new(move || {
        let id = project_id();
        fetch::fetch_project_detail(id)
    });

    let active_tab = RwSignal::new("overview".to_string());
    let editing_readme = RwSignal::new(false);
    let readme_content = RwSignal::new(String::new());
    let save_status = RwSignal::new(Option::<String>::None);

    view! {
        <Suspense fallback=|| view! {
            <div class="space-y-4 animate-pulse">
                <div class="h-8 bg-slate-800 rounded w-1/3"></div>
                <div class="h-64 bg-slate-900 rounded-xl"></div>
            </div>
        }>
            {move || Suspend::new(async move {
                let pid = project_id();
                match project.await {
                    Ok(p) => {
                        // Initialize readme content
                        let initial_readme = p.readme.clone().unwrap_or_default();
                        readme_content.set(initial_readme.clone());

                        let experiments = p.experiments.clone();
                        let display_name = p.display_name.clone();
                        let description = p.description.clone();
                        // A generated project is overwritten wholesale by the next
                        // sync. Offering an Edit button here would take the user's
                        // work and lose it, so the affordance is removed entirely
                        // rather than left to fail on save.
                        let generated = p.generated;
                        let generated_from = p.generated_from.clone();

                        view! {
                            <div class="space-y-6">
                                // Header
                                <div class="flex items-center space-x-3">
                                    <A href="/projects" attr:class="text-slate-500 hover:text-slate-300 transition-colors">
                                        <FolderKanban size=20 />
                                    </A>
                                    <span class="text-slate-600">"/"</span>
                                    <h1 class="text-3xl font-bold text-white">{display_name}</h1>
                                </div>
                                {description.map(|d| view! {
                                    <p class="text-slate-400 -mt-2">{d}</p>
                                })}
                                {generated.then(|| {
                                    let source = generated_from.clone()
                                        .unwrap_or_else(|| "an external source".to_string());
                                    view! {
                                        <div class="flex items-start space-x-3 bg-amber-950/30 border border-amber-900/50 rounded-lg px-4 py-3">
                                            <span class="text-amber-500 mt-0.5"><Lock size=16 /></span>
                                            <div class="text-sm">
                                                <p class="text-amber-300 font-medium">"Generated project — read-only"</p>
                                                <p class="text-amber-200/60 mt-0.5">
                                                    "Projected from " <span class="font-mono">{source}</span>
                                                    " and regenerated on each sync. Edit the source and re-run "
                                                    <span class="font-mono">"exp project sync"</span> "."
                                                </p>
                                            </div>
                                        </div>
                                    }
                                })}

                                // Tab bar
                                <div class="flex space-x-1 bg-slate-900/50 p-1 rounded-lg w-fit border border-slate-800">
                                    <TabButton label="Overview" tab="overview" active_tab=active_tab />
                                    <TabButton label="Compare" tab="compare" active_tab=active_tab />
                                    <TabButton label="Experiments" tab="experiments" active_tab=active_tab />
                                </div>

                                // Tab content
                                <div>
                                    {move || {
                                        let tab = active_tab.get();
                                        if tab == "overview" {
                                            let current_readme = readme_content.get();
                                            let is_editing = editing_readme.get();
                                            let pid_save = pid.clone();

                                            view! {
                                                <div class="bg-slate-900 border border-slate-800 rounded-xl">
                                                    <div class="flex items-center justify-between px-6 py-3 border-b border-slate-800">
                                                        <span class="text-sm font-medium text-slate-400">"README.md"</span>
                                                        <div class="flex items-center space-x-2">
                                                            {move || save_status.get().map(|s| view! {
                                                                <span class="text-xs text-green-400">{s}</span>
                                                            })}
                                                            {(!generated).then(|| view! {
                                                                <button
                                                                    on:click=move |_| editing_readme.update(|v| *v = !*v)
                                                                    class="flex items-center space-x-1 px-3 py-1.5 text-xs text-slate-400 hover:text-white bg-slate-800 hover:bg-slate-700 rounded-lg transition-colors"
                                                                >
                                                                    {move || if editing_readme.get() {
                                                                        view! { <Eye size=14 /> }.into_any()
                                                                    } else {
                                                                        view! { <Pencil size=14 /> }.into_any()
                                                                    }}
                                                                    <span>{move || if editing_readme.get() { "Preview" } else { "Edit" }}</span>
                                                                </button>
                                                            })}
                                                            {move || (!generated && editing_readme.get()).then(|| {
                                                                let pid_inner = pid_save.clone();
                                                                view! {
                                                                    <button
                                                                        on:click=move |_| {
                                                                            let content = readme_content.get_untracked();
                                                                            let pid_c = pid_inner.clone();
                                                                            leptos::task::spawn_local(async move {
                                                                                match fetch::save_project_readme(pid_c, content).await {
                                                                                    Ok(_) => save_status.set(Some("Saved!".to_string())),
                                                                                    Err(e) => save_status.set(Some(format!("Error: {}", e))),
                                                                                }
                                                                            });
                                                                        }
                                                                        class="flex items-center space-x-1 px-3 py-1.5 text-xs text-white bg-blue-600 hover:bg-blue-500 rounded-lg transition-colors"
                                                                    >
                                                                        <Save size=14 />
                                                                        <span>"Save"</span>
                                                                    </button>
                                                                }
                                                            })}
                                                        </div>
                                                    </div>
                                                    <div class="p-6">
                                                        {if is_editing {
                                                            view! {
                                                                <textarea
                                                                    class="w-full h-96 px-4 py-3 bg-slate-800 border border-slate-700 rounded-lg text-white font-mono text-sm focus:outline-none focus:border-blue-500 transition-colors resize-y"
                                                                    on:input=move |ev| readme_content.set(event_target_value(&ev))
                                                                    prop:value=move || readme_content.get()
                                                                ></textarea>
                                                            }.into_any()
                                                        } else if current_readme.is_empty() {
                                                            view! {
                                                                <div class="text-center py-12 text-slate-600">
                                                                    <p class="text-sm">"No README yet. Click Edit to add one."</p>
                                                                </div>
                                                            }.into_any()
                                                        } else {
                                                            let html = render_markdown(&current_readme);
                                                            view! {
                                                                <div
                                                                    class="prose prose-invert prose-sm max-w-none prose-headings:text-white prose-p:text-slate-300 prose-a:text-blue-400 prose-strong:text-white prose-code:text-blue-300 prose-code:bg-slate-800 prose-code:px-1 prose-code:rounded prose-pre:bg-slate-800 prose-li:text-slate-300 prose-table:border-slate-700 prose-th:text-slate-300 prose-td:text-slate-400"
                                                                    inner_html=html
                                                                />
                                                            }.into_any()
                                                        }}
                                                    </div>
                                                </div>
                                            }.into_any()
                                        } else if tab == "compare" {
                                            // The view a sweep exists for: params
                                            // against final metrics, across every
                                            // experiment in the project.
                                            view! { <HParams project=pid.clone() /> }.into_any()
                                        } else {
                                            let exps = experiments.clone();
                                            if exps.is_empty() {
                                                view! {
                                                    <div class="bg-slate-900 border border-slate-800 rounded-xl p-12 text-center">
                                                        <div class="flex justify-center mb-4 text-slate-600">
                                                            <FlaskConical size=48 />
                                                        </div>
                                                        <p class="text-slate-400 text-lg font-medium mb-2">"No experiments in this project"</p>
                                                        <p class="text-slate-600 text-sm">"Assign experiments to this project from the Experiments page."</p>
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <div class="bg-slate-900 border border-slate-800 rounded-xl overflow-hidden">
                                                        <table class="w-full text-left border-collapse">
                                                            <thead>
                                                                <tr class="bg-slate-800/50">
                                                                    <th class="px-6 py-4 font-semibold text-slate-300">"Name"</th>
                                                                    <th class="px-6 py-4 font-semibold text-slate-300">"Description"</th>
                                                                    <th class="px-6 py-4 font-semibold text-slate-300">"Tags"</th>
                                                                    <th class="px-6 py-4 font-semibold text-slate-300">"Runs"</th>
                                                                    <th class="px-6 py-4"></th>
                                                                </tr>
                                                            </thead>
                                                            <tbody class="divide-y divide-slate-800">
                                                                {exps.into_iter().map(|exp| {
                                                                    let id = exp.id.clone();
                                                                    view! {
                                                                        <tr class="hover:bg-slate-800/30 transition-colors">
                                                                            <td class="px-6 py-4 font-medium">
                                                                                <A href=format!("/experiments/{}", id) attr:class="text-blue-400 hover:underline">
                                                                                    {exp.display_name}
                                                                                </A>
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
                                                                            <td class="px-6 py-4 text-right">
                                                                                <A href=format!("/experiments/{}", id) attr:class="text-slate-600 hover:text-blue-400 transition-colors">
                                                                                    <ChevronRight size=16 />
                                                                                </A>
                                                                            </td>
                                                                        </tr>
                                                                    }
                                                                }).collect_view()}
                                                            </tbody>
                                                        </table>
                                                    </div>
                                                }.into_any()
                                            }
                                        }
                                    }}
                                </div>
                            </div>
                        }.into_any()
                    },
                    Err(err) => view! {
                        <ErrorState
                            title="Failed to Load Project"
                            message=err
                            action_label="Retry"
                            on_action=Callback::new(move |_| { project.refetch(); })
                        />
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn TabButton(
    label: &'static str,
    tab: &'static str,
    active_tab: RwSignal<String>,
) -> impl IntoView {
    let is_active = move || active_tab.get() == tab;
    view! {
        <button
            on:click=move |_| active_tab.set(tab.to_string())
            class=move || if is_active() {
                "px-4 py-2 text-sm font-medium text-white bg-slate-800 rounded-md transition-colors"
            } else {
                "px-4 py-2 text-sm font-medium text-slate-500 hover:text-slate-300 rounded-md transition-colors"
            }
        >
            {label}
        </button>
    }
}
