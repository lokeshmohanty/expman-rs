//! Interactive Jupyter notebook view.

use leptos::prelude::*;
use leptos::task::spawn_local;
use lucide_leptos::{Cog as SettingsIcon, FlaskConical, TriangleAlert};
use web_sys::RequestMode;

use crate::app::fetch::*;

#[component]
pub(crate) fn InteractiveView(
    exp_id: String,
    selected: std::collections::HashSet<String>,
) -> impl IntoView {
    if selected.is_empty() {
        let v: AnyView = view! {
            <div class="flex-grow flex flex-col items-center justify-center p-12 text-center space-y-4">
                <div class="p-4 bg-slate-800 rounded-full text-blue-500">
                    <FlaskConical size=28 />
                </div>
                <h3 class="text-xl font-bold text-white">"No Runs Selected"</h3>
                <p class="text-slate-400 max-w-sm">"Please select one or more runs from the left sidebar to launch an interactive session."</p>
            </div>
        }.into_any();
        return v;
    }

    let runs_list: Vec<String> = selected.clone().into_iter().collect();
    let is_multi = runs_list.len() > 1;

    let exp_id_clone_status = exp_id.clone();
    let runs_list_status = runs_list.clone();
    let jupyter_status = LocalResource::new(move || {
        let eid = exp_id_clone_status.clone();
        let rlist = runs_list_status.clone();
        let multi = is_multi;
        async move {
            if multi {
                fetch_multi_jupyter_status(eid).await
            } else {
                fetch_jupyter_status(eid, rlist[0].clone()).await
            }
        }
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

    let runs_list_outer = runs_list.clone();
    let exp_id_outer = exp_id.clone();

    let runs_list_meta = runs_list.clone();
    let run_data = LocalResource::new(move || {
        let eid = exp_id.clone();
        let rlist = runs_list_meta.clone();
        async move { fetch_run_metadata(eid, rlist[0].clone()).await }
    });

    let backend_info = LocalResource::new(|| async move { check_backend().await });

    // Fetch notebook content from server (reactive to notebook_version for refresh)
    let nb_exp_id = exp_id_outer.clone();
    let nb_runs_list = runs_list_outer.clone();
    let notebook_resource = LocalResource::new(move || {
        let eid = nb_exp_id.clone();
        let rlist = nb_runs_list.clone();
        let multi = is_multi;
        let _version = notebook_version.get();
        async move {
            if multi {
                fetch_multi_notebook_content(eid).await
            } else {
                fetch_notebook_content(eid, rlist[0].clone()).await
            }
        }
    });

    view! {
        <div class="flex-grow p-6 space-y-6 overflow-auto bg-[#e5e5e5] dark:bg-slate-950 flex flex-col h-full">
            <Suspense fallback=|| view! { <div class="p-8 text-center text-slate-500 animate-pulse">"Loading notebook status..."</div> }>
                {move || {
                    let port_opt = jupyter_port.get();
                    let loading = is_loading.get();
                    let exp_id_outer = exp_id_outer.clone();
                    let rt_exp_id = exp_id_outer.clone();
                    let rt_runs_list = runs_list_outer.clone();
                    let multi1 = is_multi;

                    let start_notebook = {
                        let eid = rt_exp_id.clone();
                        let rlist = rt_runs_list.clone();
                        let multi = multi1;
                        move |_| {
                            let eid = eid.clone();
                            let rlist = rlist.clone();
                            set_is_loading.set(true);
                            set_is_ready.set(false);
                            spawn_local(async move {
                                let res = if multi {
                                    start_multi_jupyter(eid, rlist).await
                                } else {
                                    start_jupyter(eid, rlist[0].clone()).await
                                };
                            if let Ok(port) = res {
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
                    }
                };

                let rt_exp_id2 = exp_id_outer.clone();
                    let rt_runs_list2 = runs_list_outer.clone();
                    let multi2 = is_multi;

                    let stop_notebook = {
                        let eid = rt_exp_id2.clone();
                        let rlist = rt_runs_list2.clone();
                        let multi = multi2;
                        move |_| {
                            let eid = eid.clone();
                            let rlist = rlist.clone();
                            set_is_loading.set(true);
                            spawn_local(async move {
                                if multi {
                                    let _ = stop_multi_jupyter(eid).await;
                                } else {
                                    let _ = stop_jupyter(eid, rlist[0].clone()).await;
                                }
                                set_jupyter_port.set(None);
                                set_is_loading.set(false);
                            });
                        }
                    };

                    let runs_count = runs_list_outer.len();
                    let runs_list_for_suspend = runs_list_outer.clone();
                    Suspend::new(async move {
                        let run = run_data.await;
                        let notebook_info = notebook_resource.await;

                        let backend = backend_info.await;
                        let backend_str = backend.as_ref().map(|b| b.backend.clone()).unwrap_or_else(|_| "none".to_string());

                        let view_result: leptos::prelude::AnyView = match run {
                            Ok(run_info) => {
                                let name_str = if is_multi { format!("Multiple Runs ({})", runs_count) } else { run_info.name.clone() };

                                let nb_exists = notebook_info.as_ref().map(|n| n.exists).unwrap_or(false);
                                let nb_sources: Vec<String> = notebook_info
                                    .as_ref()
                                    .ok()
                                    .and_then(|n| n.content.as_deref())
                                    .map(extract_cell_sources)
                                    .unwrap_or_default();

                                let create_exp_id = exp_id_outer.clone();
                                let create_runs_list = runs_list_for_suspend.clone();
                                let multi3 = is_multi;
                                let create_notebook_click = {
                                    let eid = create_exp_id.clone();
                                    let rlist = create_runs_list.clone();
                                    let multi = multi3;
                                    move |_| {
                                        let eid = eid.clone();
                                        let rlist = rlist.clone();
                                        set_is_loading.set(true);
                                        spawn_local(async move {
                                            if multi {
                                                let _ = create_multi_notebook(eid, rlist).await;
                                            } else {
                                                let _ = create_default_notebook(eid, rlist[0].clone()).await;
                                            }
                                            set_notebook_version.update(|v| *v += 1);
                                            set_is_loading.set(false);
                                        });
                                    }
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
