//! Chart components: MetricsView, LineChart, ScalarChart.

use leptos::prelude::*;
use lucide_leptos::{Download, LayoutDashboard, LayoutGrid, RefreshCw, StretchVertical};
use wasm_bindgen::JsCast;

use crate::app::fetch::*;
use crate::app::models::*;
use crate::app::utils::*;

#[component]
pub(crate) fn MetricsView(
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

    let selected_runs_data: Vec<&Run> = runs.iter().filter(|r| selected.contains(&r.id)).collect();

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
                            let runs_clone = runs.clone();
                             view! {
                                 <div class="bg-slate-950 border border-slate-800 rounded-xl p-6 flex flex-col" style="resize: both; overflow: hidden; min-width: 300px; max-width: 100%; aspect-ratio: 16/9;">
                                     <div class="flex items-center justify-between mb-4 flex-shrink-0">
                                         <h4 class="text-sm font-semibold text-slate-300">{vk_clone}</h4>
                                         <div class="flex space-x-3">
                                              {selected_clone.clone().into_iter().enumerate().map(|(i, s)| {
                                                  let color = CHART_COLORS[i % CHART_COLORS.len()];
                                                  let run_name = runs.iter().find(|r| r.id == s).map(|r| r.name.clone()).unwrap_or(s.clone());
                                                  view! {
                                                      <div class="flex items-center space-x-1 text-[10px] text-slate-400">
                                                          <span class=format!("w-2 h-2 rounded-full") style=format!("background-color: {}", color)></span>
                                                          <span class="font-mono">{run_name}</span>
                                                      </div>
                                                  }
                                              }).collect_view()}
                                         </div>
                                     </div>
                                     <div class="flex-grow rounded-lg overflow-hidden relative" style="width: 100%; height: 100%;">
                                         <LineChart exp_id=exp_id_clone selected_runs=selected_clone metric_key=vk runs=runs_clone />
                                     </div>
                                 </div>
                             }.into_any()
                        }).collect_view().into_any()
                    }
                }
            </div>

            <div class="bg-slate-950 border border-slate-800 rounded-xl p-6">
                 <h4 class="text-sm font-semibold text-slate-300 mb-4">"Scalars Overview"</h4>
                 <div class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-6">
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
                                 <div class="col-span-full py-12 text-center text-slate-500 italic">
                                     "No scalar data available for selected runs."
                                 </div>
                             }.into_any()
                         } else {
                             keys.into_iter().map(|k| {
                                 let k_label = k.clone();
                                 let runs_subset = selected_runs_data.iter().map(|r| (*r).clone()).collect::<Vec<_>>();
                                 let runs_for_legend = runs_subset.clone();
                                 view! {
                                     <div class="bg-slate-900/50 border border-slate-800 rounded-lg p-4 flex flex-col h-80">
                                         <h5 class="text-xs font-medium text-slate-400 mb-2 truncate" title=k_label.clone()>{k_label.clone()}</h5>
                                         <div class="flex-grow min-h-0">
                                             <ScalarChart scalar_key=k runs=runs_subset />
                                         </div>
                                         <div class="mt-3 flex flex-wrap gap-2 overflow-y-auto max-h-16">
                                             {runs_for_legend.into_iter().enumerate().map(|(i, run)| {
                                                 let color = CHART_COLORS[i % CHART_COLORS.len()];
                                                 view! {
                                                     <div class="flex items-center space-x-1 text-[10px] text-slate-400">
                                                         <span class=format!("w-2 h-2 rounded-full shrink-0") style=format!("background-color: {}", color)></span>
                                                         <span class="font-mono truncate max-w-[120px]" title={run.name.clone()}>{run.name.clone()}</span>
                                                     </div>
                                                 }
                                             }).collect_view()}
                                         </div>
                                     </div>
                                 }
                             }).collect_view().into_any()
                         }
                     }
                 </div>
            </div>
        </div>
    }.into_any()
}

#[component]
pub(crate) fn LineChart(
    exp_id: String,
    selected_runs: std::collections::HashSet<String>,
    metric_key: String,
    runs: Vec<Run>,
) -> impl IntoView {
    let canvas_ref = NodeRef::<leptos::html::Canvas>::new();
    let (view_range_x, set_view_range_x) = signal((0.0, 100.0));
    let (view_range_y, set_view_range_y) = signal((0.0, 1.0));
    let (is_dragging, set_is_dragging) = signal(false);
    let (last_mouse_pos, set_last_mouse_pos) = signal(None::<(i32, i32)>);
    let (grid_dense, set_grid_dense) = signal(true);

    let metrics_resource = LocalResource::new({
        let exp_id = exp_id.clone();
        let selected_runs = selected_runs.clone();
        move || {
            let eid = exp_id.clone();
            let runs = selected_runs.clone();
            async move {
                let mut results =
                    std::collections::HashMap::<String, Vec<serde_json::Value>>::new();
                for rid in runs {
                    if let Ok(m) = fetch_run_metrics(eid.clone(), rid.clone()).await {
                        results.insert(rid, m);
                    }
                }
                results
            }
        }
    });

    // Auto-fit range when data is loaded
    Effect::new({
        let metric_key = metric_key.clone();
        move |_| {
            let data = metrics_resource.get();
            if let Some(runs_data) = data {
                let mut min_x = f64::MAX;
                let mut max_x = f64::MIN;
                let mut min_y = f64::MAX;
                let mut max_y = f64::MIN;
                let mut found = false;

                for rows in runs_data.values() {
                    for row in rows {
                        if let (Some(step), Some(val)) = (
                            row.get("step").and_then(|v| v.as_f64()),
                            row.get(&metric_key).and_then(|v| v.as_f64()),
                        ) {
                            min_x = min_x.min(step);
                            max_x = max_x.max(step);
                            min_y = min_y.min(val);
                            max_y = max_y.max(val);
                            found = true;
                        }
                    }
                }

                if found {
                    // Add some padding
                    let x_padding = if max_x > min_x {
                        (max_x - min_x) * 0.05
                    } else {
                        1.0
                    };
                    let y_padding = if max_y > min_y {
                        (max_y - min_y) * 0.1
                    } else {
                        0.1
                    };
                    set_view_range_x.set((min_x, max_x + x_padding));
                    set_view_range_y.set((min_y - y_padding, max_y + y_padding));
                }
            }
        }
    });

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
            let new_y_min = y_min;
            let new_y_max = y_min + y_range * zoom_factor;

            set_view_range_x.set((new_x_min, new_x_max));
            set_view_range_y.set((new_y_min, new_y_max));
        }
    };

    let download_chart = {
        let metric_key = metric_key.clone();
        let selected_runs = selected_runs.clone();
        let runs_meta = runs.clone();
        move |_: leptos::ev::MouseEvent| {
            use plotters::prelude::*;
            use plotters_canvas::CanvasBackend;
            use wasm_bindgen::JsCast;

            let (x_min, x_max) = view_range_x.get();
            let (y_min, y_max) = view_range_y.get();
            if x_min >= x_max || y_min >= y_max {
                return;
            }

            // 1600x900 offscreen canvas, appended off-screen so CanvasBackend can get a 2D context
            let doc = web_sys::window().unwrap().document().unwrap();
            let offscreen: web_sys::HtmlCanvasElement =
                doc.create_element("canvas").unwrap().dyn_into().unwrap();
            offscreen.set_width(1600);
            offscreen.set_height(900);
            offscreen
                .set_attribute("style", "position:absolute;left:-9999px;top:-9999px;")
                .unwrap();
            doc.body().unwrap().append_child(&offscreen).unwrap();

            let backend = CanvasBackend::with_canvas_object(offscreen.clone()).unwrap();
            let root = backend.into_drawing_area();
            let _ = root.fill(&WHITE);

            let mut chart = ChartBuilder::on(&root)
                .caption(&metric_key, ("sans-serif", 28).into_font().color(&BLACK))
                .margin(40)
                .x_label_area_size(70)
                .y_label_area_size(90)
                .build_cartesian_2d(x_min..x_max, y_min..y_max)
                .unwrap();

            chart
                .configure_mesh()
                .x_desc("Step")
                .y_desc("Value")
                .axis_desc_style(("sans-serif", 22).into_font().color(&RGBColor(17, 24, 39)))
                .axis_style(RGBColor(55, 65, 81))
                .label_style(("sans-serif", 18).into_font().color(&RGBColor(17, 24, 39)))
                .light_line_style(RGBColor(229, 231, 235))
                .bold_line_style(RGBColor(209, 213, 219))
                .draw()
                .unwrap();

            if let Some(runs_data) = metrics_resource.get() {
                for (i, run_id) in selected_runs.iter().enumerate() {
                    if let Some(rows) = runs_data.get(run_id) {
                        let raw_points: Vec<(f64, f64)> = rows
                            .iter()
                            .filter_map(|row| {
                                let x = row.get("step").and_then(|v| v.as_f64())?;
                                let y = row.get(&metric_key).and_then(|v| v.as_f64())?;
                                Some((x, y))
                            })
                            .collect();
                        if raw_points.is_empty() {
                            continue;
                        }

                        let hex = CHART_COLORS[i % CHART_COLORS.len()];
                        let r = u8::from_str_radix(&hex[1..3], 16).unwrap_or(0);
                        let g = u8::from_str_radix(&hex[3..5], 16).unwrap_or(0);
                        let b = u8::from_str_radix(&hex[5..7], 16).unwrap_or(0);

                        let run_name = runs_meta
                            .iter()
                            .find(|r| r.id == *run_id)
                            .map(|r| r.name.clone())
                            .unwrap_or_else(|| run_id.clone());

                        // Use same clipping as the live chart
                        let clipped =
                            clip_polyline_to_viewport(&raw_points, x_min, x_max, y_min, y_max);
                        let mut first = true;
                        for segment in clipped {
                            if segment.len() >= 2 {
                                let series = chart
                                    .draw_series(LineSeries::new(
                                        segment.into_iter(),
                                        RGBColor(r, g, b).stroke_width(3),
                                    ))
                                    .unwrap();
                                if first {
                                    let color_clone = RGBColor(r, g, b);
                                    series.label(run_name.clone()).legend(move |(x, y)| {
                                        PathElement::new(
                                            vec![(x, y), (x + 20, y)],
                                            color_clone.stroke_width(2),
                                        )
                                    });
                                    first = false;
                                }
                            }
                        }
                    }
                }

                chart
                    .configure_series_labels()
                    .background_style(WHITE.mix(0.8))
                    .border_style(BLACK)
                    .position(SeriesLabelPosition::UpperRight)
                    .draw()
                    .unwrap();
            }

            let _ = root.present();
            let fname = format!("{}.png", metric_key.replace('/', "_"));
            download_canvas_as_png(&offscreen, &fname);
            let _ = doc.body().unwrap().remove_child(&offscreen);
        }
    };

    Effect::new({
        let metric_key = metric_key.clone();
        let selected_runs = selected_runs.clone();
        move |_| {
            use plotters::prelude::*;
            use plotters_canvas::CanvasBackend;

            if let Some(canvas) = canvas_ref.get() {
                let (x_min, x_max) = view_range_x.get();
                let (y_min, y_max) = view_range_y.get();

                let parent = canvas.parent_element().unwrap();
                let w = parent.client_width() as u32;
                let h = parent.client_height() as u32;

                if w > 0 && h > 0 {
                    canvas.set_width(w);
                    canvas.set_height(h);
                }

                let backend = CanvasBackend::with_canvas_object(canvas.clone()).unwrap();
                let root = backend.into_drawing_area();

                // Fill with dark background matching the UI (slate-950)
                let _ = root.fill(&RGBColor(2, 6, 23));

                if x_min >= x_max || y_min >= y_max {
                    let _ = root.present();
                    return;
                }

                let mut chart = ChartBuilder::on(&root)
                    .caption(
                        &metric_key,
                        ("sans-serif", 16)
                            .into_font()
                            .color(&RGBColor(248, 250, 252)),
                    )
                    .margin(20)
                    .x_label_area_size(50)
                    .y_label_area_size(70)
                    .build_cartesian_2d(x_min..x_max, y_min..y_max)
                    .unwrap();

                let mut mesh = chart.configure_mesh();

                mesh.x_desc("Step")
                    .y_desc("Value")
                    .axis_desc_style(
                        ("sans-serif", 14)
                            .into_font()
                            .color(&RGBColor(203, 213, 225)),
                    )
                    .axis_style(RGBColor(71, 85, 105)) // slate-600
                    .label_style(
                        ("sans-serif", 12)
                            .into_font()
                            .color(&RGBColor(203, 213, 225)),
                    ) // slate-300
                    .light_line_style(RGBColor(30, 41, 59)) // slate-800
                    .bold_line_style(RGBColor(51, 65, 85)); // slate-700

                if !grid_dense.get() {
                    mesh.disable_x_mesh().disable_y_mesh();
                }

                mesh.draw().unwrap();

                // Draw series with Liang-Barsky clipped data so lines are trimmed
                // exactly at the viewport boundary instead of sticking to the edge.
                if let Some(runs_data) = metrics_resource.get() {
                    for (i, run_id) in selected_runs.iter().enumerate() {
                        if let Some(rows) = runs_data.get(run_id) {
                            let raw_points: Vec<(f64, f64)> = rows
                                .iter()
                                .filter_map(|row| {
                                    let x = row.get("step").and_then(|v| v.as_f64())?;
                                    let y = row.get(&metric_key).and_then(|v| v.as_f64())?;
                                    Some((x, y))
                                })
                                .collect();

                            if raw_points.is_empty() {
                                continue;
                            }

                            let hex = CHART_COLORS[i % CHART_COLORS.len()];
                            let r = u8::from_str_radix(&hex[1..3], 16).unwrap_or(0);
                            let g = u8::from_str_radix(&hex[3..5], 16).unwrap_or(0);
                            let b = u8::from_str_radix(&hex[5..7], 16).unwrap_or(0);
                            let color = RGBColor(r, g, b);

                            // Clip each sub-polyline to the current viewport in data space.
                            // This produces geometrically correct entry/exit points so no
                            // line ever "hugs" the border when panning or zooming.
                            let clipped_segments =
                                clip_polyline_to_viewport(&raw_points, x_min, x_max, y_min, y_max);

                            for segment in clipped_segments {
                                if segment.len() >= 2 {
                                    chart
                                        .draw_series(LineSeries::new(
                                            segment.into_iter(),
                                            color.stroke_width(2),
                                        ))
                                        .unwrap();
                                }
                            }
                        }
                    }
                }

                let _ = root.present();
            }
        }
    });

    view! {
        <div class="w-full h-full relative group" style="min-height: 250px;">
            <div class="absolute top-2 right-2 z-10 opacity-0 group-hover:opacity-100 transition-opacity flex space-x-1">
                <button
                    on:click=move |_| set_grid_dense.update(|d| *d = !*d)
                    class="p-1.5 bg-slate-800/80 hover:bg-blue-600/80 text-slate-300 hover:text-white rounded-md backdrop-blur-sm transition-all border border-slate-700"
                    title=move || if grid_dense.get() { "Hide Grid" } else { "Show Grid" }
                >
                    {move || if grid_dense.get() {
                        view! { <StretchVertical size=14 /> }.into_any()
                    } else {
                        view! { <LayoutGrid size=14 /> }.into_any()
                    }}
                </button>
                <button
                    on:click=move |_| {
                        // Trigger re-fit by clearing and letting the effect handle it?
                        // Or just force a re-calc? For now just reset to a default if data is missing
                        set_view_range_x.set((0.0, 100.0));
                        set_view_range_y.set((0.0, 1.0));
                        // The effect should pick up data if it exists
                    }
                    class="p-1.5 bg-slate-800/80 hover:bg-blue-600/80 text-slate-300 hover:text-white rounded-md backdrop-blur-sm transition-all border border-slate-700"
                    title="Reset Zoom"
                >
                    <RefreshCw size=14 />
                </button>
                <button
                    on:click=download_chart
                    class="p-1.5 bg-slate-800/80 hover:bg-blue-600/80 text-slate-300 hover:text-white rounded-md backdrop-blur-sm transition-all border border-slate-700"
                    title="Download PNG"
                >
                    <Download size=14 />
                </button>
            </div>
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
pub(crate) fn ScalarChart(scalar_key: String, runs: Vec<Run>) -> impl IntoView {
    let canvas_ref = NodeRef::<leptos::html::Canvas>::new();
    let (view_range_x, set_view_range_x) = signal((0.0, 100.0));
    let (view_range_y, set_view_range_y) = signal((0.0, 1.0));
    let (is_dragging, set_is_dragging) = signal(false);
    let (last_mouse_pos, set_last_mouse_pos) = signal(None::<(i32, i32)>);
    let (grid_dense, set_grid_dense) = signal(true);

    // Initial value setup
    Effect::new({
        let scalar_key = scalar_key.clone();
        let runs = runs.clone();
        move |_| {
            let mut min_val = f64::MAX;
            let mut max_val = f64::MIN;
            let mut min_dur = f64::MAX;
            let mut max_dur = f64::MIN;
            let mut found = false;

            for run in &runs {
                if let Some(scalars) = &run.scalars {
                    if let Some(val) = scalars.get(&scalar_key) {
                        let numeric_val = match val {
                            MetricValue::Float(f) => Some(*f),
                            MetricValue::Int(i) => Some(*i as f64),
                            MetricValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
                            MetricValue::Text(_) => None,
                        };

                        if let Some(v) = numeric_val {
                            let dur = run.duration_secs.unwrap_or(0.0);
                            min_val = min_val.min(v);
                            max_val = max_val.max(v);
                            min_dur = min_dur.min(dur);
                            max_dur = max_dur.max(dur);
                            found = true;
                        }
                    }
                }
            }

            if found {
                let y_padding = if max_val > min_val {
                    (max_val - min_val) * 0.2
                } else {
                    1.0
                };
                let x_padding = if max_dur > min_dur {
                    (max_dur - min_dur) * 0.1
                } else {
                    1.0
                };
                set_view_range_x.set(((min_dur - x_padding).max(0.0), max_dur + x_padding));
                set_view_range_y.set((min_val - y_padding, max_val + y_padding));
            }
        }
    });

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
            let new_y_min = y_min;
            let new_y_max = y_min + y_range * zoom_factor;

            set_view_range_x.set((new_x_min, new_x_max));
            set_view_range_y.set((new_y_min, new_y_max));
        }
    };

    let download_chart = {
        let scalar_key = scalar_key.clone();
        let runs = runs.clone();
        move |_: leptos::ev::MouseEvent| {
            use plotters::prelude::*;
            use plotters_canvas::CanvasBackend;
            use wasm_bindgen::JsCast;

            let (x_min, x_max) = view_range_x.get();
            let (y_min, y_max) = view_range_y.get();
            if x_min >= x_max || y_min >= y_max {
                return;
            }

            let doc = web_sys::window().unwrap().document().unwrap();
            let offscreen: web_sys::HtmlCanvasElement =
                doc.create_element("canvas").unwrap().dyn_into().unwrap();
            offscreen.set_width(1600);
            offscreen.set_height(900);
            offscreen
                .set_attribute("style", "position:absolute;left:-9999px;top:-9999px;")
                .unwrap();
            doc.body().unwrap().append_child(&offscreen).unwrap();

            let backend = CanvasBackend::with_canvas_object(offscreen.clone()).unwrap();
            let root = backend.into_drawing_area();
            let _ = root.fill(&WHITE);

            // Collect the same data as the live render
            let mut plot_data: Vec<(f64, f64, usize)> = Vec::new();
            for (idx, run) in runs.iter().enumerate() {
                if let Some(scalars) = &run.scalars {
                    if let Some(val) = scalars.get(&scalar_key) {
                        let numeric_val = match val {
                            MetricValue::Float(f) => Some(*f),
                            MetricValue::Int(i) => Some(*i as f64),
                            MetricValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
                            MetricValue::Text(_) => None,
                        };
                        if let Some(v) = numeric_val {
                            plot_data.push((run.duration_secs.unwrap_or(0.0), v, idx));
                        }
                    }
                }
            }

            if plot_data.is_empty() {
                let _ = doc.body().unwrap().remove_child(&offscreen);
                return;
            }

            let mut chart = ChartBuilder::on(&root)
                .caption(&scalar_key, ("sans-serif", 28).into_font().color(&BLACK))
                .margin(40)
                .x_label_area_size(70)
                .y_label_area_size(90)
                .build_cartesian_2d(x_min..x_max, y_min..y_max)
                .unwrap();

            chart
                .configure_mesh()
                .x_desc("Duration (s)")
                .y_desc(scalar_key.clone())
                .axis_desc_style(("sans-serif", 22).into_font().color(&RGBColor(17, 24, 39)))
                .axis_style(RGBColor(55, 65, 81))
                .label_style(("sans-serif", 18).into_font().color(&RGBColor(17, 24, 39)))
                .light_line_style(RGBColor(229, 231, 235))
                .bold_line_style(RGBColor(209, 213, 219))
                .draw()
                .unwrap();

            for (idx, run) in runs.iter().enumerate() {
                let hex = CHART_COLORS[idx % CHART_COLORS.len()];
                let r = u8::from_str_radix(&hex[1..3], 16).unwrap_or(0);
                let g = u8::from_str_radix(&hex[3..5], 16).unwrap_or(0);
                let b = u8::from_str_radix(&hex[5..7], 16).unwrap_or(0);
                let color = RGBColor(r, g, b);

                let run_points: Vec<(f64, f64)> = plot_data
                    .iter()
                    .filter(|(_, _, c_idx)| *c_idx == idx)
                    .filter(|(x, y, _)| *x >= x_min && *x <= x_max && *y >= y_min && *y <= y_max)
                    .map(|(x, y, _)| (*x, *y))
                    .collect();

                if !run_points.is_empty() {
                    chart
                        .draw_series(
                            run_points
                                .into_iter()
                                .map(|(x, y)| Circle::new((x, y), 8, color.filled())),
                        )
                        .unwrap()
                        .label(run.name.clone())
                        .legend(move |(x, y)| Circle::new((x + 10, y), 5, color.filled()));
                }
            }

            chart
                .configure_series_labels()
                .background_style(WHITE.mix(0.8))
                .border_style(BLACK)
                .position(SeriesLabelPosition::UpperRight)
                .draw()
                .unwrap();

            let _ = root.present();
            let fname = format!("{}.png", scalar_key.replace('/', "_"));
            download_canvas_as_png(&offscreen, &fname);
            let _ = doc.body().unwrap().remove_child(&offscreen);
        }
    };

    Effect::new({
        let scalar_key = scalar_key.clone();
        let runs = runs.clone();
        move |_| {
            use plotters::prelude::*;
            use plotters_canvas::CanvasBackend;

            if let Some(canvas) = canvas_ref.get() {
                let (x_min, x_max) = view_range_x.get();
                let (y_min, y_max) = view_range_y.get();

                let parent = canvas.parent_element().unwrap();
                let w = parent.client_width() as u32;
                let h = parent.client_height() as u32;

                if w > 0 && h > 0 {
                    canvas.set_width(w);
                    canvas.set_height(h);
                }

                let backend = CanvasBackend::with_canvas_object(canvas.clone()).unwrap();
                let root = backend.into_drawing_area();
                let _ = root.fill(&RGBColor(2, 6, 23)); // slate-950 equivalent for inside card

                if x_min >= x_max || y_min >= y_max {
                    let _ = root.present();
                    return;
                }

                // Process data
                let mut plot_data = Vec::new();

                for (idx, run) in runs.iter().enumerate() {
                    if let Some(scalars) = &run.scalars {
                        if let Some(val) = scalars.get(&scalar_key) {
                            let numeric_val = match val {
                                MetricValue::Float(f) => Some(*f),
                                MetricValue::Int(i) => Some(*i as f64),
                                MetricValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
                                MetricValue::Text(_) => None,
                            };

                            if let Some(v) = numeric_val {
                                let dur = run.duration_secs.unwrap_or(0.0);
                                plot_data.push((dur, v, idx));
                            }
                        }
                    }
                }

                if plot_data.is_empty() {
                    let _ = root.present();
                    return;
                }

                let mut chart = ChartBuilder::on(&root)
                    .margin(20)
                    .x_label_area_size(50)
                    .y_label_area_size(70)
                    .build_cartesian_2d(x_min..x_max, y_min..y_max)
                    .unwrap();

                let mut mesh = chart.configure_mesh();

                mesh.x_desc("Duration (s)")
                    .y_desc(scalar_key.clone())
                    .axis_desc_style(
                        ("sans-serif", 14)
                            .into_font()
                            .color(&RGBColor(203, 213, 225)),
                    )
                    .axis_style(RGBColor(71, 85, 105)) // slate-600
                    .label_style(
                        ("sans-serif", 12)
                            .into_font()
                            .color(&RGBColor(203, 213, 225)),
                    ) // slate-300
                    .light_line_style(RGBColor(30, 41, 59)) // slate-800
                    .bold_line_style(RGBColor(51, 65, 85)); // slate-700

                if !grid_dense.get() {
                    mesh.disable_x_mesh().disable_y_mesh();
                }

                mesh.draw().unwrap();

                // Manual clipping for CanvasBackend
                let (x_range, y_range) = chart.plotting_area().get_pixel_range();
                let ctx = canvas
                    .get_context("2d")
                    .ok()
                    .flatten()
                    .and_then(|c| c.dyn_into::<web_sys::CanvasRenderingContext2d>().ok());

                if let Some(ctx) = ctx {
                    ctx.save();
                    ctx.begin_path();
                    let x = x_range.start as f64;
                    let y = y_range.start as f64;
                    let w = (x_range.end - x_range.start) as f64;
                    let h = (y_range.end - y_range.start) as f64;
                    ctx.rect(x, y, w, h);
                    ctx.clip();
                }

                chart
                    .draw_series(
                        plot_data
                            .into_iter()
                            // Filter out-of-viewport points so plotters doesn't
                            // try to render circles with extreme pixel coordinates.
                            .filter(|(x, y, _)| {
                                *x >= x_min && *x <= x_max && *y >= y_min && *y <= y_max
                            })
                            .map(|(x, y, color_idx)| {
                                let hex = CHART_COLORS[color_idx % CHART_COLORS.len()];
                                let r = u8::from_str_radix(&hex[1..3], 16).unwrap_or(0);
                                let g = u8::from_str_radix(&hex[3..5], 16).unwrap_or(0);
                                let b = u8::from_str_radix(&hex[5..7], 16).unwrap_or(0);
                                let color = RGBColor(r, g, b);
                                Circle::new((x, y), 5, color.filled())
                            }),
                    )
                    .unwrap();

                if let Some(ctx) = canvas
                    .get_context("2d")
                    .ok()
                    .flatten()
                    .and_then(|c| c.dyn_into::<web_sys::CanvasRenderingContext2d>().ok())
                {
                    ctx.restore();
                }

                let _ = root.present();
            }
        }
    });

    view! {
        <div class="w-full h-full relative group" style="min-height: 250px;">
            <div class="absolute top-2 right-2 z-10 opacity-0 group-hover:opacity-100 transition-opacity flex space-x-1">
                <button
                    on:click=move |_| set_grid_dense.update(|d| *d = !*d)
                    class="p-1.5 bg-slate-800/80 hover:bg-blue-600/80 text-slate-300 hover:text-white rounded-md backdrop-blur-sm transition-all border border-slate-700"
                    title=move || if grid_dense.get() { "Hide Grid" } else { "Show Grid" }
                >
                    {move || if grid_dense.get() {
                        view! { <StretchVertical size=14 /> }.into_any()
                    } else {
                        view! { <LayoutGrid size=14 /> }.into_any()
                    }}
                </button>
                <button
                    on:click=move |_| {
                        let mut min_val = f64::MAX;
                        let mut max_val = f64::MIN;
                        let mut min_dur = f64::MAX;
                        let mut max_dur = f64::MIN;
                        for run in &runs {
                            if let Some(scalars) = &run.scalars {
                                if let Some(val) = scalars.get(&scalar_key) {
                                    let numeric_val = match val {
                                        MetricValue::Float(f) => Some(*f),
                                        MetricValue::Int(i) => Some(*i as f64),
                                        MetricValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
                                        MetricValue::Text(_) => None,
                                    };
                                    if let Some(v) = numeric_val {
                                        let dur = run.duration_secs.unwrap_or(0.0);
                                        min_val = min_val.min(v);
                                        max_val = max_val.max(v);
                                        min_dur = min_dur.min(dur);
                                        max_dur = max_dur.max(dur);
                                    }
                                }
                            }
                        }
                        if min_val <= max_val {
                            let y_padding = if max_val > min_val { (max_val - min_val) * 0.2 } else { 1.0 };
                            let x_padding = if max_dur > min_dur { (max_dur - min_dur) * 0.1 } else { 1.0 };
                            set_view_range_x.set(((min_dur - x_padding).max(0.0), max_dur + x_padding));
                            set_view_range_y.set((min_val - y_padding, max_val + y_padding));
                        }
                    }
                    class="p-1.5 bg-slate-800/80 hover:bg-blue-600/80 text-slate-300 hover:text-white rounded-md backdrop-blur-sm transition-all border border-slate-700"
                    title="Reset Zoom"
                >
                    <RefreshCw size=14 />
                </button>
                <button
                    on:click=download_chart
                    class="p-1.5 bg-slate-800/80 hover:bg-blue-600/80 text-slate-300 hover:text-white rounded-md backdrop-blur-sm transition-all border border-slate-700"
                    title="Download PNG"
                >
                    <Download size=14 />
                </button>
            </div>
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
