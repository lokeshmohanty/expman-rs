//! Artifact browsing and preview components.

use crate::app::components::zoom::ZoomControls;
use crate::app::fetch::*;
use leptos::prelude::*;

#[component]
pub(crate) fn TabularPreview(content: String) -> impl IntoView {
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

#[component]
pub(crate) fn ArtifactView(
    exp_id: String,
    selected: std::collections::HashSet<String>,
) -> impl IntoView {
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
pub(crate) fn SingleArtifactView(exp_id: String, run_id: String) -> impl IntoView {
    let (selected_path, set_selected_path) = signal("run.log".to_string());
    let (zoom_scale, set_zoom_scale) = signal(1.0);
    let (pan_x, set_pan_x) = signal(0.0_f64);
    let (pan_y, set_pan_y) = signal(0.0_f64);
    let (is_panning, set_is_panning) = signal(false);
    let (last_pan_pos, set_last_pan_pos) = signal(None::<(i32, i32)>);

    // Reset zoom AND pan when path changes
    Effect::new(move |_| {
        selected_path.track();
        set_zoom_scale.set(1.0);
        set_pan_x.set(0.0);
        set_pan_y.set(0.0);
    });

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
                    <div class="flex items-center space-x-4">
                        <ZoomControls
                            zoom_scale=zoom_scale
                            set_zoom_scale=set_zoom_scale
                            size=14
                        />
                        {
                            let dl_exp_id = exp_id.clone();
                            let dl_run_id = run_id.clone();
                            view! { <a href=move || format!("/api/experiments/{}/runs/{}/artifacts/content?path={}", dl_exp_id.clone(), dl_run_id.clone(), selected_path.get()) download class="text-[10px] text-blue-500 hover:underline">"Download Raw"</a> }
                        }
                    </div>
                </div>
                // Scroll-to-zoom: adjust zoom_scale on wheel events
                <div
                    class="flex-grow flex flex-col min-h-0 bg-slate-950 overflow-auto text-slate-300 relative"
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
                    <div
                        class="flex-grow flex flex-col items-center justify-center min-h-full min-w-full"
                    >
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
                                   <div class="flex items-center justify-center p-4">
                                       <video controls class="max-w-full rounded-lg shadow-lg" src=media_url></video>
                                   </div>
                               }.into_any()
                            } else if is_audio {
                               view! {
                                   <div class="flex items-center justify-center p-8">
                                       <audio controls class="w-full max-w-md shadow-lg" src=media_url></audio>
                                   </div>
                               }.into_any()
                            } else if is_image {
                               view! {
                                   <div
                                       class="w-full h-full overflow-hidden relative select-none"
                                       style="cursor: grab;"
                                       on:wheel=move |ev: web_sys::WheelEvent| {
                                           ev.prevent_default();
                                           let delta = ev.delta_y();
                                           let zoom_factor = if delta < 0.0 { 1.1_f64 } else { 1.0 / 1.1 };
                                           set_zoom_scale.update(|z| {
                                               *z = (*z * zoom_factor).clamp(0.1, 20.0);
                                           });
                                       }
                                       on:mousedown=move |ev: web_sys::MouseEvent| {
                                           ev.prevent_default();
                                           set_is_panning.set(true);
                                           set_last_pan_pos.set(Some((ev.client_x(), ev.client_y())));
                                       }
                                       on:mousemove=move |ev: web_sys::MouseEvent| {
                                           if is_panning.get() {
                                               if let Some((lx, ly)) = last_pan_pos.get() {
                                                   let dx = (ev.client_x() - lx) as f64;
                                                   let dy = (ev.client_y() - ly) as f64;
                                                   set_pan_x.update(|x| *x += dx);
                                                   set_pan_y.update(|y| *y += dy);
                                                   set_last_pan_pos.set(Some((ev.client_x(), ev.client_y())));
                                               }
                                           }
                                       }
                                       on:mouseup=move |_| {
                                           set_is_panning.set(false);
                                           set_last_pan_pos.set(None);
                                       }
                                       on:mouseleave=move |_| {
                                           set_is_panning.set(false);
                                           set_last_pan_pos.set(None);
                                       }
                                   >
                                       <div
                                           class="absolute inset-0 flex items-center justify-center"
                                           style=move || format!(
                                               "transform: translate({}px, {}px) scale({}); transform-origin: center center;",
                                               pan_x.get(), pan_y.get(), zoom_scale.get()
                                           )
                                       >
                                           <img
                                               class="max-w-none rounded-lg shadow-lg pointer-events-none"
                                               src=media_url
                                               draggable="false"
                                           />
                                       </div>
                                       // Zoom hint overlay
                                       <div class="absolute bottom-2 right-2 text-[10px] font-mono text-slate-500 bg-slate-900/70 px-2 py-1 rounded pointer-events-none">
                                           {move || format!("{:.0}%", zoom_scale.get() * 100.0)}
                                       </div>
                                       // Reset button
                                       <button
                                           class="absolute top-2 right-2 p-1.5 bg-slate-800/80 hover:bg-slate-700 text-slate-400 hover:text-white rounded-md border border-slate-700 text-[10px] font-medium transition-colors"
                                           on:click=move |_| {
                                               set_zoom_scale.set(1.0);
                                               set_pan_x.set(0.0);
                                               set_pan_y.set(0.0);
                                           }
                                       >
                                           "Reset"
                                       </button>
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
        </div>
    }.into_any()
}
