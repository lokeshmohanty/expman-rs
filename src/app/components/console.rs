//! Console/log viewer components.

use leptos::prelude::*;
use std::cell::Cell;
use std::rc::Rc;
use wasm_bindgen::JsCast;

use crate::app::components::zoom::ZoomControls;
use crate::app::models::*;

#[component]
pub(crate) fn ConsoleView(
    exp_id: String,
    selected: std::collections::HashSet<String>,
    runs: Vec<Run>,
) -> impl IntoView {
    if selected.is_empty() {
        return view! { <div class="p-12 text-center text-slate-500">"Select one or more runs to view live console output."</div> }.into_any();
    }

    view! {
        <div class="h-full flex flex-col p-6 space-y-6 overflow-auto custom-scrollbar">
            <div class="flex items-center space-x-2 text-xs text-slate-500 mb-2">
                <span class="px-2 py-0.5 bg-slate-800 rounded">"run.log"</span>
                <span>"&"</span>
                <span class="px-2 py-0.5 bg-slate-800 rounded">"console.log"</span>
            </div>

            <div class="flex-grow grid grid-cols-1 gap-8 min-h-0">
                {selected.clone().into_iter().map(|run_id| {
                    let run_id_inner = run_id.clone();
                    let status = runs.iter().find(|r| r.id == run_id_inner).map(|r| r.status.clone()).unwrap_or_default();
                    view! {
                        <div class="grid grid-cols-2 gap-4 h-[500px] flex-shrink-0">
                            <div class="flex flex-col h-full bg-slate-950 rounded-xl border border-slate-800 overflow-hidden shadow-2xl">
                                <div class="px-3 py-2 border-b border-slate-800 bg-slate-900/50 flex items-center justify-between">
                                    <span class="text-[10px] font-mono text-slate-400">"ID: " {run_id.clone()} " (" "run.log" ")"</span>
                                </div>
                                <SingleConsoleView exp_id=exp_id.clone() run_id=run_id.clone() filename="run.log".to_string() status=status.clone() />
                            </div>
                            <div class="flex flex-col h-full bg-slate-950 rounded-xl border border-slate-800 overflow-hidden shadow-2xl">
                                <div class="px-3 py-2 border-b border-slate-800 bg-slate-900/50 flex items-center justify-between">
                                    <span class="text-[10px] font-mono text-slate-400">"ID: " {run_id.clone()} " (" "console.log" ")"</span>
                                </div>
                                <SingleConsoleView exp_id=exp_id.clone() run_id=run_id.clone() filename="console.log".to_string() status=status.clone() />
                            </div>
                        </div>
                    }
                }).collect_view()}
            </div>
        </div>
    }.into_any()
}

#[component]
pub(crate) fn SingleConsoleView(
    exp_id: String,
    run_id: String,
    filename: String,
    status: String,
) -> impl IntoView {
    let (zoom_scale, set_zoom_scale) = signal(1.0);
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
        "/api/experiments/{}/runs/{}/log/stream?file={}",
        exp_id_val.with_value(|v| v.clone()),
        run_id_val.with_value(|v| v.clone()),
        filename
    );
    let event_source = web_sys::EventSource::new(&url).unwrap();
    let es_val = StoredValue::new_local(event_source.clone());
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
        es_val.with_value(|es: &web_sys::EventSource| es.close());
    });

    view! {
        <div class="flex-grow flex flex-col overflow-hidden font-mono text-[11px] leading-relaxed">
            // Zoom controls for console output
            <div class="px-3 py-1 border-b border-slate-800/50 flex items-center justify-end bg-slate-900/20">
                <ZoomControls
                    zoom_scale=zoom_scale
                    set_zoom_scale=set_zoom_scale
                    size=12
                />
            </div>
            // Scroll-to-zoom on console output
            <div
                class="flex-grow overflow-auto p-4 space-y-1 custom-scrollbar"
                id="console-scroll"
                on:wheel=move |ev: web_sys::WheelEvent| {
                    if ev.ctrl_key() {
                        ev.prevent_default();
                        let delta = ev.delta_y();
                        set_zoom_scale.update(|z: &mut f64| {
                            if delta > 0.0 { *z = (*z - 0.1).max(0.1); }
                            else { *z = (*z + 0.1).min(5.0); }
                        });
                    }
                }
            >
                <div style=move || format!("transform: scale({}); transform-origin: top left;", zoom_scale.get())>
                    <For
                        each=move || logs.get().into_iter().enumerate()
                        key=|(i, _)| *i
                        children=|(_, line)| view! { <div class="text-white whitespace-pre-wrap">{line}</div> }
                    />
                </div>
            </div>
            <div class="px-4 py-3 border-t border-slate-800 flex items-center justify-between bg-slate-900/30">
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
