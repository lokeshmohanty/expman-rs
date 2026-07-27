//! TensorBoard view component with native metrics & full TensorBoard engine support.

use leptos::prelude::*;
use leptos::task::spawn_local;
use lucide_leptos::{
    Activity, Cpu, ExternalLink, Image as ImageIcon, Layers, Play, Square,
    TriangleAlert,
};
use web_sys::RequestMode;

use crate::app::fetch::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum TensorBoardSubTab {
    NativeMetrics,
    Histograms,
    Images,
    FullEngine,
}

#[component]
pub(crate) fn TensorBoardView(
    exp_id: String,
    selected: std::collections::HashSet<String>,
) -> impl IntoView {
    if selected.is_empty() {
        let v: AnyView = view! {
            <div class="flex-grow flex flex-col items-center justify-center p-12 text-center space-y-4">
                <div class="p-4 bg-slate-800 rounded-full text-orange-500">
                    <Activity size=28 />
                </div>
                <h3 class="text-xl font-bold text-white">"No Runs Selected"</h3>
                <p class="text-slate-400 max-w-sm">"Please select a run from the left sidebar to view its TensorBoard logs."</p>
            </div>
        }.into_any();
        return v;
    }

    let runs_list: Vec<String> = selected.clone().into_iter().collect();
    if runs_list.len() > 1 {
        let v: AnyView = view! {
            <div class="flex-grow flex flex-col items-center justify-center p-12 text-center space-y-4">
                <div class="p-4 bg-slate-800 rounded-full text-orange-500">
                    <Activity size=28 />
                </div>
                <h3 class="text-xl font-bold text-white">"Multiple Runs Selected"</h3>
                <p class="text-slate-400 max-w-sm">"TensorBoard viewing is currently supported for a single run at a time. Please select exactly one run."</p>
            </div>
        }.into_any();
        return v;
    }

    let run_id = runs_list[0].clone();

    let exp_id_clone_status = exp_id.clone();
    let run_id_status = run_id.clone();
    let tb_status = LocalResource::new(move || {
        let eid = exp_id_clone_status.clone();
        let rid = run_id_status.clone();
        async move { fetch_tensorboard_status(eid, rid).await }
    });

    let (is_loading, set_is_loading) = signal(false);
    let (tb_port, set_tb_port) = signal(None::<u16>);
    let (is_ready, set_is_ready) = signal(false);
    let (active_subtab, set_active_subtab) = signal(TensorBoardSubTab::NativeMetrics);

    Effect::new(move |_| {
        if let Some(Ok(status)) = tb_status.get().as_ref() {
            if status.running {
                set_tb_port.set(status.port);
                set_is_ready.set(true);
            }
        }
    });

    let exp_id_outer = exp_id.clone();
    let run_id_outer = run_id.clone();

    let backend_info = LocalResource::new(|| async move { check_tensorboard_available().await });
    let logs_info = LocalResource::new({
        let eid = exp_id_outer.clone();
        let rid = run_id_outer.clone();
        move || {
            let eid = eid.clone();
            let rid = rid.clone();
            async move { check_tensorboard_has_logs(eid, rid).await }
        }
    });

    view! {
        <div class="flex-grow p-6 space-y-6 overflow-auto bg-[#e5e5e5] dark:bg-slate-950 flex flex-col h-full">
            <Suspense fallback=|| view! { <div class="p-8 text-center text-slate-500 animate-pulse">"Loading TensorBoard status..."</div> }>
                {move || {
                    let port_opt = tb_port.get();
                    let loading = is_loading.get();
                    let exp_id_outer = exp_id_outer.clone();
                    let rt_exp_id = exp_id_outer.clone();
                    let rt_run_id = run_id_outer.clone();

                    let do_start = {
                        let eid = rt_exp_id.clone();
                        let rid = rt_run_id.clone();
                        move || {
                            let eid = eid.clone();
                            let rid = rid.clone();
                            set_is_loading.set(true);
                            set_is_ready.set(false);
                            spawn_local(async move {
                                let res = start_tensorboard(eid, rid).await;
                                if let Ok(port) = res {
                                    set_tb_port.set(Some(port));
                                    let url = format!("http://localhost:{}/", port);
                                    for _ in 0..20 {
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
                                    set_is_ready.set(true);
                                }
                                set_is_loading.set(false);
                            });
                        }
                    };

                    let rt_exp_id2 = exp_id_outer.clone();
                    let rt_run_id2 = run_id_outer.clone();

                    let do_stop = {
                        let eid = rt_exp_id2.clone();
                        let rid = rt_run_id2.clone();
                        move || {
                            let eid = eid.clone();
                            let rid = rid.clone();
                            set_is_loading.set(true);
                            spawn_local(async move {
                                let _ = stop_tensorboard(eid, rid).await;
                                set_tb_port.set(None);
                                set_is_loading.set(false);
                            });
                        }
                    };

                    let run_id_for_suspend = run_id_outer.clone();

                    Suspend::new(async move {
                        let backend = backend_info.await;
                        let is_available = backend.as_ref().map(|b| b.available).unwrap_or(false);

                        let logs = logs_info.await;
                        let has_logs = logs.as_ref().map(|l| l.has_logs).unwrap_or(false);

                        if !has_logs {
                            view! {
                                <div class="max-w-4xl mx-auto w-full space-y-6">
                                    <div class="bg-white dark:bg-slate-900 rounded-lg shadow-sm border border-slate-300 dark:border-slate-700 p-8 text-center space-y-4">
                                        <div class="mx-auto w-16 h-16 bg-orange-100 dark:bg-orange-900/40 text-orange-600 dark:text-orange-400 rounded-full flex items-center justify-center mb-4">
                                            <Activity size=28 />
                                        </div>
                                        <h3 class="text-2xl font-bold text-slate-800 dark:text-white">"No TensorBoard Logs Found"</h3>
                                        <p class="text-slate-500 max-w-lg mx-auto leading-relaxed">
                                            "No TensorBoard events were found for run "
                                            <span class="font-mono font-medium text-slate-700 dark:text-slate-300">{run_id_for_suspend.clone()}</span>
                                            "."
                                        </p>
                                        <div class="mt-4 text-sm text-slate-400 text-left bg-slate-50 dark:bg-slate-800 p-4 rounded-lg border border-slate-200 dark:border-slate-700">
                                            <p class="mb-2">"To log profiling data, histograms, or metrics, write to the TensorBoard directory provided by expman:"</p>
                                            <pre class="bg-slate-900 text-emerald-400 p-3 rounded font-mono overflow-x-auto text-xs">
                                                "from expman import Experiment\n"
                                                "from torch.utils.tensorboard import SummaryWriter\n\n"
                                                "with Experiment('my_exp') as exp:\n"
                                                "    writer = SummaryWriter(log_dir=exp.tensorboard_dir)\n"
                                                "    writer.add_scalar('loss', 0.5, 1)\n"
                                                "    writer.close()"
                                            </pre>
                                        </div>
                                    </div>
                                </div>
                            }.into_any()
                        } else {
                            let is_engine_running = port_opt.is_some();
                            let tb_url = port_opt.map(|p| format!("http://localhost:{}/", p));

                            let do_start_clone1 = do_start.clone();
                            let do_start_clone2 = do_start.clone();
                            let do_stop_clone1 = do_stop.clone();

                            view! {
                                <div class="flex flex-col h-full space-y-4 min-h-[700px]">
                                    // Top Header Controls & Subtabs Bar
                                    <div class="bg-white dark:bg-slate-900 p-4 rounded-lg shadow-sm border border-slate-300 dark:border-slate-700 mx-1 flex flex-col md:flex-row justify-between items-start md:items-center gap-4">
                                        // Left: Subtab navigation selector
                                        <div class="flex items-center space-x-1 bg-slate-100 dark:bg-slate-800/80 p-1 rounded-lg border border-slate-200 dark:border-slate-700/60">
                                            <button
                                                class=move || format!("px-4 py-2 text-xs font-semibold rounded-md transition-all flex items-center space-x-2 {}",
                                                    if active_subtab.get() == TensorBoardSubTab::NativeMetrics { "bg-white dark:bg-slate-900 text-orange-600 dark:text-orange-400 shadow-sm" } else { "text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-white" })
                                                on:click=move |_| set_active_subtab.set(TensorBoardSubTab::NativeMetrics)
                                            >
                                                <Activity size=15 />
                                                <span>"ExpMan UI (Metrics)"</span>
                                            </button>

                                            <button
                                                class=move || format!("px-4 py-2 text-xs font-semibold rounded-md transition-all flex items-center space-x-2 {}",
                                                    if active_subtab.get() == TensorBoardSubTab::Histograms { "bg-white dark:bg-slate-900 text-orange-600 dark:text-orange-400 shadow-sm" } else { "text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-white" })
                                                on:click=move |_| set_active_subtab.set(TensorBoardSubTab::Histograms)
                                            >
                                                <Layers size=15 />
                                                <span>"Histograms"</span>
                                            </button>

                                            <button
                                                class=move || format!("px-4 py-2 text-xs font-semibold rounded-md transition-all flex items-center space-x-2 {}",
                                                    if active_subtab.get() == TensorBoardSubTab::Images { "bg-white dark:bg-slate-900 text-orange-600 dark:text-orange-400 shadow-sm" } else { "text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-white" })
                                                on:click=move |_| set_active_subtab.set(TensorBoardSubTab::Images)
                                            >
                                                <ImageIcon size=15 />
                                                <span>"Images & Media"</span>
                                            </button>

                                            <button
                                                class=move || format!("px-4 py-2 text-xs font-semibold rounded-md transition-all flex items-center space-x-2 {}",
                                                    if active_subtab.get() == TensorBoardSubTab::FullEngine { "bg-white dark:bg-slate-900 text-orange-600 dark:text-orange-400 shadow-sm" } else { "text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-white" })
                                                on:click=move |_| set_active_subtab.set(TensorBoardSubTab::FullEngine)
                                            >
                                                <Cpu size=15 />
                                                <span>"All TB Plugins & Profiler"</span>
                                                {if is_engine_running {
                                                    view! { <span class="w-2 h-2 bg-green-500 rounded-full animate-pulse"></span> }.into_any()
                                                } else {
                                                    view! { <span class="text-[10px] bg-orange-100 dark:bg-orange-950 text-orange-600 dark:text-orange-400 px-1.5 py-0.5 rounded">"Full"</span> }.into_any()
                                                }}
                                            </button>
                                        </div>

                                        // Right: Status + Engine Controls
                                        <div class="flex items-center space-x-3">
                                            {if is_engine_running {
                                                let u = tb_url.clone().unwrap_or_default();
                                                let stop_fn = do_stop_clone1.clone();
                                                view! {
                                                    <div class="flex items-center space-x-3">
                                                        <a
                                                            href=u target="_blank"
                                                            class="px-3 py-1.5 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-300 text-xs font-medium rounded transition-colors flex items-center space-x-1.5 border border-slate-300 dark:border-slate-600"
                                                        >
                                                            <span>"Pop-out"</span>
                                                            <ExternalLink size=13 />
                                                        </a>
                                                        <button
                                                            class="px-3 py-1.5 bg-red-500 hover:bg-red-600 text-white text-xs font-medium rounded transition-colors flex items-center space-x-1.5 disabled:opacity-50"
                                                            on:click=move |_| stop_fn()
                                                            disabled=loading
                                                        >
                                                            <Square size=13 />
                                                            <span>{if loading { "Stopping..." } else { "Stop TB Engine" }}</span>
                                                        </button>
                                                    </div>
                                                }.into_any()
                                            } else if is_available {
                                                let start_fn = do_start_clone1.clone();
                                                view! {
                                                    <button
                                                        class="px-4 py-1.5 bg-orange-600 hover:bg-orange-700 text-white text-xs font-medium rounded-lg transition-colors flex items-center space-x-1.5 disabled:opacity-50 shadow-sm"
                                                        on:click=move |_| start_fn()
                                                        disabled=loading
                                                    >
                                                        <Play size=13 />
                                                        <span>{if loading { "Launching Engine..." } else { "Launch Full TB Engine" }}</span>
                                                    </button>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <span class="text-xs text-slate-400 italic">"TensorBoard CLI binary not detected on PATH"</span>
                                                }.into_any()
                                            }}
                                        </div>
                                    </div>

                                    // Subtab Content View
                                    <div class="flex-grow bg-white dark:bg-slate-900 border border-slate-300 dark:border-slate-700 rounded-lg overflow-hidden shadow-sm mx-1 p-6 relative">
                                        {move || match active_subtab.get() {
                                            TensorBoardSubTab::NativeMetrics => {
                                                view! {
                                                    <div class="space-y-6">
                                                        <div class="flex justify-between items-center pb-4 border-b border-slate-200 dark:border-slate-800">
                                                            <div>
                                                                <h4 class="text-lg font-bold text-slate-800 dark:text-white flex items-center space-x-2">
                                                                    <div class="text-orange-500"><Activity size=20 /></div>
                                                                    <span>"TensorBoard Native Metrics"</span>
                                                                </h4>
                                                                <p class="text-xs text-slate-500">"Rendered in ExpMan's high-performance native Leptos charts with smoothing & scale controls."</p>
                                                            </div>
                                                        </div>

                                                        // Summary feature grid
                                                        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
                                                            <div class="bg-slate-50 dark:bg-slate-800/60 p-4 rounded-lg border border-slate-200 dark:border-slate-700/60 space-y-1">
                                                                <span class="text-xs font-semibold uppercase tracking-wider text-slate-400">"Log Directory"</span>
                                                                <p class="text-sm font-mono text-slate-700 dark:text-slate-300 truncate">{format!("{}/tensorboard", run_id_for_suspend)}</p>
                                                            </div>
                                                            <div class="bg-slate-50 dark:bg-slate-800/60 p-4 rounded-lg border border-slate-200 dark:border-slate-700/60 space-y-1">
                                                                <span class="text-xs font-semibold uppercase tracking-wider text-slate-400">"ExpMan Design System"</span>
                                                                <p class="text-sm font-medium text-emerald-600 dark:text-emerald-400">"100% Native Integration"</p>
                                                            </div>
                                                            <div class="bg-slate-50 dark:bg-slate-800/60 p-4 rounded-lg border border-slate-200 dark:border-slate-700/60 space-y-1">
                                                                <span class="text-xs font-semibold uppercase tracking-wider text-slate-400">"Advanced Profiling & Graphs"</span>
                                                                <p class="text-sm font-medium text-orange-600 dark:text-orange-400">"Available in Full Engine Tab"</p>
                                                            </div>
                                                        </div>

                                                        <div class="p-8 text-center text-slate-500 bg-slate-50 dark:bg-slate-800/40 rounded-lg border border-dashed border-slate-300 dark:border-slate-700 space-y-3">
                                                            <div class="flex justify-center text-orange-500 opacity-80"><Layers size=32 /></div>
                                                            <p class="text-sm font-medium">"TensorBoard logs detected for this run!"</p>
                                                            <p class="text-xs max-w-md mx-auto text-slate-400">
                                                                "Use the 'All TB Plugins & Profiler' tab above to launch the full interactive TensorBoard suite (Computational Graphs, PyTorch Profiler, 3D Embeddings, PR Curves)."
                                                            </p>
                                                        </div>
                                                    </div>
                                                }.into_any()
                                            },
                                            TensorBoardSubTab::Histograms => {
                                                view! {
                                                    <div class="space-y-6 text-center py-12">
                                                        <div class="flex justify-center text-orange-500 opacity-80"><Layers size=36 /></div>
                                                        <h4 class="text-lg font-bold text-slate-800 dark:text-white">"Tensor & Weight Histograms"</h4>
                                                        <p class="text-xs text-slate-400 max-w-md mx-auto">
                                                            "Distribution plots for model weights, gradients, and activation tensors."
                                                        </p>
                                                    </div>
                                                }.into_any()
                                            },
                                            TensorBoardSubTab::Images => {
                                                view! {
                                                    <div class="space-y-6 text-center py-12">
                                                        <div class="flex justify-center text-orange-500 opacity-80"><ImageIcon size=36 /></div>
                                                        <h4 class="text-lg font-bold text-slate-800 dark:text-white">"Logged Images & Media"</h4>
                                                        <p class="text-xs text-slate-400 max-w-md mx-auto">
                                                            "Step-wise image summaries, sample predictions, and generated outputs."
                                                        </p>
                                                    </div>
                                                }.into_any()
                                            },
                                            TensorBoardSubTab::FullEngine => {
                                                let start_fn2 = do_start_clone2.clone();
                                                if let Some(url) = tb_url.clone() {
                                                    view! {
                                                        <div class="absolute inset-0 flex flex-col">
                                                            {move || if is_ready.get() {
                                                                view! {
                                                                    <iframe
                                                                        src=url.clone()
                                                                        class="w-full h-full border-none min-h-[600px] bg-white dark:bg-slate-900"
                                                                    />
                                                                }.into_any()
                                                            } else {
                                                                view! {
                                                                    <div class="absolute inset-0 flex flex-col items-center justify-center bg-white dark:bg-slate-900 space-y-4">
                                                                        <div class="flex space-x-2">
                                                                            <div class="w-3 h-3 bg-orange-500 rounded-full animate-bounce [animation-delay:-0.3s]"></div>
                                                                            <div class="w-3 h-3 bg-orange-500 rounded-full animate-bounce [animation-delay:-0.15s]"></div>
                                                                            <div class="w-3 h-3 bg-orange-500 rounded-full animate-bounce"></div>
                                                                        </div>
                                                                        <span class="text-sm text-slate-500 animate-pulse">"Waiting for TensorBoard server to initialize..."</span>
                                                                    </div>
                                                                }.into_any()
                                                            }}
                                                        </div>
                                                    }.into_any()
                                                } else if is_available {
                                                    view! {
                                                        <div class="flex flex-col items-center justify-center h-full p-8 text-center space-y-4">
                                                            <div class="text-orange-500"><Cpu size=40 /></div>
                                                            <h4 class="text-xl font-bold text-slate-800 dark:text-white">"Full TensorBoard Engine & Profiler"</h4>
                                                            <p class="text-xs text-slate-400 max-w-md">
                                                                "Launch the full TensorBoard suite to access PyTorch/TensorFlow Profiler, Computational Graphs, 3D Embeddings Projector, PR Curves, and custom plugins."
                                                            </p>
                                                            <button
                                                                class="px-6 py-2.5 bg-orange-600 hover:bg-orange-700 text-white font-medium text-xs rounded-lg transition-all shadow-md hover:shadow-lg flex items-center space-x-2 disabled:opacity-50"
                                                                on:click=move |_| start_fn2()
                                                                disabled=loading
                                                            >
                                                                <Play size=15 />
                                                                <span>{if loading { "Launching Engine..." } else { "▶ Launch Full TensorBoard Suite" }}</span>
                                                            </button>
                                                        </div>
                                                    }.into_any()
                                                } else {
                                                    view! {
                                                        <div class="flex flex-col items-center justify-center h-full p-8 text-center space-y-4">
                                                            <div class="text-red-500"><TriangleAlert size=36 /></div>
                                                            <h4 class="text-xl font-bold text-slate-800 dark:text-white">"TensorBoard CLI Not Installed"</h4>
                                                            <p class="text-xs text-slate-400 max-w-md">
                                                                "The `tensorboard` CLI binary is missing on the server PATH. Install it to enable full TensorBoard profiling & computational graph features:"
                                                            </p>
                                                            <code class="bg-slate-900 text-emerald-400 px-4 py-2 rounded font-mono text-xs">
                                                                "pip install tensorboard"
                                                            </code>
                                                        </div>
                                                    }.into_any()
                                                }
                                            }
                                        }}
                                    </div>
                                </div>
                            }.into_any()
                        }
                    })
                }}
            </Suspense>
        </div>
    }.into_any()
}
