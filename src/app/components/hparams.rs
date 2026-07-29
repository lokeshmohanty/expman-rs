//! Hyperparameter comparison: a params × final-metrics table and a scatter.
//!
//! This is the view a sweep exists for. It reads `/projects/{p}/runs`, which
//! already returns the cross-experiment runs table plus facets, so no scanning
//! happens here.
//!
//! Parameters come from **tags** rather than `config.yaml`: a sweep tags every
//! trial with `name:value`, so the columns are exactly the axes that were swept,
//! and they arrive already faceted. Reading `config.yaml` per run would need one
//! request per run and would include every constant the script logged.

use std::collections::{BTreeMap, BTreeSet};

use leptos::prelude::*;

use crate::app::fetch;
use crate::app::models::Run;

/// Split a `name:value` tag. Tags without a colon are labels, not parameters.
fn split_param(tag: &str) -> Option<(String, String)> {
    let (key, value) = tag.split_once(':')?;
    (!key.is_empty() && !value.is_empty()).then(|| (key.to_string(), value.to_string()))
}

/// The final value of `metric` for a run, from scalars or the latest vector.
fn metric_value(run: &Run, metric: &str) -> Option<f64> {
    run.scalars
        .as_ref()
        .and_then(|s| s.get(metric))
        .or_else(|| run.vectors.as_ref().and_then(|v| v.get(metric)))
        .and_then(|v| v.to_string().parse::<f64>().ok())
}

/// Format compactly: sweep values are usually either small decimals or integers.
fn short_number(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let magnitude = value.abs();
    if !(1e-4..1e6).contains(&magnitude) {
        format!("{value:.3e}")
    } else if magnitude >= 100.0 {
        format!("{value:.1}")
    } else {
        // Trim trailing zeros so 0.010 reads as 0.01.
        let s = format!("{value:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[component]
pub(crate) fn HParams(project: String) -> impl IntoView {
    let project_for_fetch = project.clone();
    let data = LocalResource::new(move || {
        let id = project_for_fetch.clone();
        async move { fetch::fetch_project_runs(id).await }
    });

    let sort_by = RwSignal::new(Option::<String>::None);
    let sort_desc = RwSignal::new(false);
    let x_axis = RwSignal::new(String::new());
    let y_axis = RwSignal::new(String::new());

    view! {
        <Suspense fallback=move || view! {
            <div class="h-64 bg-slate-900 rounded-xl animate-pulse"></div>
        }>
            {move || Suspend::new(async move {
                match data.await {
                    Err(e) => view! {
                        <div class="bg-slate-900 border border-slate-800 rounded-xl p-8 text-center text-slate-400">
                            "Could not load runs: " {e}
                        </div>
                    }.into_any(),
                    Ok(payload) => {
                        let runs = payload.runs.clone();
                        if runs.is_empty() {
                            return view! {
                                <div class="bg-slate-900 border border-slate-800 rounded-xl p-12 text-center">
                                    <p class="text-slate-400 text-lg font-medium mb-2">"No runs in this project yet"</p>
                                    <p class="text-slate-600 text-sm">"Run a sweep and its trials will appear here."</p>
                                </div>
                            }.into_any();
                        }

                        // Parameter columns come from the tags, so only axes that
                        // actually vary become columns.
                        let mut param_values: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
                        for run in &runs {
                            for tag in run.tags.clone().unwrap_or_default() {
                                if let Some((k, v)) = split_param(&tag) {
                                    param_values.entry(k).or_default().insert(v);
                                }
                            }
                        }
                        let params: Vec<String> = param_values
                            .iter()
                            .filter(|(_, values)| values.len() > 1)
                            .map(|(k, _)| k.clone())
                            .collect();
                        // If nothing varies, showing the constants beats showing
                        // an empty table.
                        let params = if params.is_empty() {
                            param_values.keys().cloned().collect::<Vec<_>>()
                        } else {
                            params
                        };

                        let metrics = payload.metrics.clone();
                        // Default to a pair that actually has points. Taking the
                        // first param and the first metric is easy and routinely
                        // wrong: a project holding both a sweep and an unrelated
                        // experiment has params and metrics that never co-occur,
                        // so the view would open on an empty plot.
                        if x_axis.get_untracked().is_empty() || y_axis.get_untracked().is_empty() {
                            let best = params.iter().find_map(|param| {
                                metrics.iter().find_map(|metric| {
                                    let hits = runs
                                        .iter()
                                        .filter(|run| {
                                            metric_value(run, metric).is_some()
                                                && run
                                                    .tags
                                                    .clone()
                                                    .unwrap_or_default()
                                                    .iter()
                                                    .any(|t| {
                                                        split_param(t)
                                                            .map(|(k, _)| k == *param)
                                                            .unwrap_or(false)
                                                    })
                                        })
                                        .count();
                                    (hits > 0).then(|| (param.clone(), metric.clone()))
                                })
                            });
                            let (x, y) = best.unwrap_or_else(|| {
                                (
                                    params.first().cloned().unwrap_or_default(),
                                    metrics.first().cloned().unwrap_or_default(),
                                )
                            });
                            x_axis.set(x);
                            y_axis.set(y);
                        }

                        let params_for_table = params.clone();
                        let metrics_for_table = metrics.clone();
                        let runs_for_table = runs.clone();
                        let runs_for_plot = runs.clone();
                        let params_for_axes = params.clone();
                        let metrics_for_axes = metrics.clone();

                        view! {
                            <div class="space-y-6">
                                <ScatterPanel
                                    runs=runs_for_plot
                                    params=params_for_axes
                                    metrics=metrics_for_axes
                                    x_axis=x_axis
                                    y_axis=y_axis
                                />
                                <ComparisonTable
                                    runs=runs_for_table
                                    params=params_for_table
                                    metrics=metrics_for_table
                                    sort_by=sort_by
                                    sort_desc=sort_desc
                                />
                            </div>
                        }.into_any()
                    }
                }
            })}
        </Suspense>
    }
}

/// Any-param-vs-any-metric scatter, drawn as SVG.
///
/// SVG rather than the canvas the line charts use: this plot is a few dozen
/// points with tooltips and no panning, and SVG gets hit-testing and crisp
/// scaling for free.
#[component]
fn ScatterPanel(
    runs: Vec<Run>,
    params: Vec<String>,
    metrics: Vec<String>,
    x_axis: RwSignal<String>,
    y_axis: RwSignal<String>,
) -> impl IntoView {
    // A param may be categorical ("adam"/"sgd"); position those by index so the
    // axis still means something rather than dropping the run.
    view! {
        <div class="bg-slate-900 border border-slate-800 rounded-xl p-6">
            <div class="flex flex-wrap items-center gap-3 mb-5">
                <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider">"Plot"</span>
                <select
                    class="bg-slate-800 border border-slate-700 rounded-lg px-3 py-1.5 text-sm text-white font-mono"
                    on:change=move |ev| x_axis.set(event_target_value(&ev))
                >
                    {params.iter().map(|p| {
                        let value = p.clone();
                        let label = p.clone();
                        let selected = x_axis.get_untracked() == *p;
                        view! { <option value=value selected=selected>{label}</option> }
                    }).collect_view()}
                </select>
                <span class="text-slate-500 text-sm">"vs"</span>
                <select
                    class="bg-slate-800 border border-slate-700 rounded-lg px-3 py-1.5 text-sm text-white font-mono"
                    on:change=move |ev| y_axis.set(event_target_value(&ev))
                >
                    {metrics.iter().map(|m| {
                        let value = m.clone();
                        let label = m.clone();
                        let selected = y_axis.get_untracked() == *m;
                        view! { <option value=value selected=selected>{label}</option> }
                    }).collect_view()}
                </select>
            </div>

            {move || {
                let x_key = x_axis.get();
                let y_key = y_axis.get();
                if x_key.is_empty() || y_key.is_empty() {
                    return view! {
                        <p class="text-slate-600 text-sm py-8 text-center">
                            "Nothing to plot: this project has no swept parameters yet."
                        </p>
                    }.into_any();
                }

                // Collect (x, y, label). Categorical x values get an index.
                let mut categories: Vec<String> = vec![];
                let mut points: Vec<(f64, f64, String, String)> = vec![];
                for run in &runs {
                    let Some(y) = metric_value(run, &y_key) else { continue };
                    let raw = run
                        .tags
                        .clone()
                        .unwrap_or_default()
                        .into_iter()
                        .find_map(|t| split_param(&t).filter(|(k, _)| *k == x_key).map(|(_, v)| v));
                    let Some(raw) = raw else { continue };
                    let x = match raw.parse::<f64>() {
                        Ok(v) => v,
                        Err(_) => {
                            let idx = categories.iter().position(|c| *c == raw).unwrap_or_else(|| {
                                categories.push(raw.clone());
                                categories.len() - 1
                            });
                            idx as f64
                        }
                    };
                    points.push((x, y, run.name.clone(), raw));
                }

                if points.is_empty() {
                    return view! {
                        <p class="text-slate-600 text-sm py-8 text-center">
                            "No run reports both " <span class="font-mono">{x_key}</span>
                            " and " <span class="font-mono">{y_key}</span> "."
                        </p>
                    }.into_any();
                }

                let (w, h) = (720.0_f64, 320.0_f64);
                let (pad_l, pad_r, pad_t, pad_b) = (64.0_f64, 20.0_f64, 16.0_f64, 44.0_f64);
                let (min_x, max_x) = bounds(points.iter().map(|p| p.0));
                let (min_y, max_y) = bounds(points.iter().map(|p| p.1));

                let sx = move |v: f64| pad_l + (v - min_x) / (max_x - min_x) * (w - pad_l - pad_r);
                // SVG y grows downward; invert so larger metric values sit higher.
                let sy = move |v: f64| h - pad_b - (v - min_y) / (max_y - min_y) * (h - pad_t - pad_b);

                let best = points
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1 .1.total_cmp(&b.1 .1))
                    .map(|(i, _)| i);

                let circles = points.iter().enumerate().map(|(i, (x, y, label, raw))| {
                    let is_best = best == Some(i);
                    view! {
                        <g>
                            <circle
                                cx=sx(*x) cy=sy(*y) r=if is_best { 7.0 } else { 5.0 }
                                class=if is_best {
                                    "fill-emerald-400 stroke-emerald-200"
                                } else {
                                    "fill-blue-500/70 stroke-blue-300/50 hover:fill-blue-400"
                                }
                                stroke-width="1.5"
                            >
                                <title>{format!("{label}\n{}={raw}\n{}={}", x_axis.get(), y_axis.get(), short_number(*y))}</title>
                            </circle>
                        </g>
                    }
                }).collect_view();

                let y_ticks = (0..=4).map(|i| {
                    let value = min_y + (max_y - min_y) * i as f64 / 4.0;
                    let y = sy(value);
                    view! {
                        <g>
                            <line x1=pad_l y1=y x2=w - pad_r y2=y class="stroke-slate-800" stroke-width="1" />
                            <text x=pad_l - 8.0 y=y + 4.0 text-anchor="end"
                                  class="fill-slate-500 text-[10px] font-mono">
                                {short_number(value)}
                            </text>
                        </g>
                    }
                }).collect_view();

                let x_ticks = (0..=4).map(|i| {
                    let value = min_x + (max_x - min_x) * i as f64 / 4.0;
                    let x = sx(value);
                    let label = if categories.is_empty() {
                        short_number(value)
                    } else {
                        categories.get(value.round() as usize).cloned().unwrap_or_default()
                    };
                    view! {
                        <text x=x y=h - pad_b + 18.0 text-anchor="middle"
                              class="fill-slate-500 text-[10px] font-mono">{label}</text>
                    }
                }).collect_view();

                view! {
                    <div class="overflow-x-auto">
                        <svg viewBox=format!("0 0 {w} {h}") class="w-full" style=format!("min-width:{}px", w / 1.6)>
                            {y_ticks}
                            {x_ticks}
                            <text x=w / 2.0 y=h - 6.0 text-anchor="middle"
                                  class="fill-slate-400 text-[11px] font-mono">{x_axis.get()}</text>
                            <text x=14.0 y=h / 2.0 text-anchor="middle"
                                  transform=format!("rotate(-90 14 {})", h / 2.0)
                                  class="fill-slate-400 text-[11px] font-mono">{y_axis.get()}</text>
                            {circles}
                        </svg>
                        <p class="text-[11px] text-slate-600 mt-2">
                            <span class="text-emerald-400">"●"</span>
                            " lowest " <span class="font-mono">{y_axis.get()}</span>
                            " — hover a point for its run"
                        </p>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

/// Min/max with a guard so a single distinct value still renders.
fn bounds(values: impl Iterator<Item = f64>) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for v in values {
        min = min.min(v);
        max = max.max(v);
    }
    if !min.is_finite() || !max.is_finite() {
        return (0.0, 1.0);
    }
    if (max - min).abs() < f64::EPSILON {
        // All points identical: centre them instead of dividing by zero.
        return (min - 1.0, max + 1.0);
    }
    (min, max)
}

#[component]
fn ComparisonTable(
    runs: Vec<Run>,
    params: Vec<String>,
    metrics: Vec<String>,
    sort_by: RwSignal<Option<String>>,
    sort_desc: RwSignal<bool>,
) -> impl IntoView {
    view! {
        <div class="bg-slate-900 border border-slate-800 rounded-xl overflow-hidden">
            <div class="px-6 py-3 border-b border-slate-800 flex items-center justify-between">
                <span class="text-sm font-medium text-slate-400">"Trials"</span>
                <span class="text-xs text-slate-600 font-mono">{runs.len()} " runs"</span>
            </div>
            <div class="overflow-x-auto">
                <table class="w-full text-sm">
                    <thead class="bg-slate-950 text-xs uppercase text-slate-500">
                        <tr>
                            <th class="px-4 py-3 text-left">"Run"</th>
                            {params.iter().map(|p| {
                                let key = p.clone();
                                let label = p.clone();
                                view! {
                                    <th class="px-4 py-3 text-left cursor-pointer hover:text-slate-300"
                                        on:click=move |_| toggle_sort(sort_by, sort_desc, &key)>
                                        {label}
                                    </th>
                                }
                            }).collect_view()}
                            {metrics.iter().map(|m| {
                                let key = m.clone();
                                let label = m.clone();
                                view! {
                                    <th class="px-4 py-3 text-right cursor-pointer hover:text-slate-300"
                                        on:click=move |_| toggle_sort(sort_by, sort_desc, &key)>
                                        {label}
                                    </th>
                                }
                            }).collect_view()}
                            <th class="px-4 py-3 text-left">"Status"</th>
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-slate-800">
                        {move || {
                            let mut rows = runs.clone();
                            if let Some(key) = sort_by.get() {
                                let params_set: BTreeSet<String> = params.iter().cloned().collect();
                                let desc = sort_desc.get();
                                rows.sort_by(|a, b| {
                                    let value = |run: &Run| -> Option<f64> {
                                        if params_set.contains(&key) {
                                            run.tags.clone().unwrap_or_default().into_iter().find_map(|t| {
                                                split_param(&t)
                                                    .filter(|(k, _)| *k == key)
                                                    .and_then(|(_, v)| v.parse::<f64>().ok())
                                            })
                                        } else {
                                            metric_value(run, &key)
                                        }
                                    };
                                    // Runs missing the sort key sink to the bottom
                                    // rather than pretending to be zero.
                                    match (value(a), value(b)) {
                                        (Some(x), Some(y)) => if desc { y.total_cmp(&x) } else { x.total_cmp(&y) },
                                        (Some(_), None) => std::cmp::Ordering::Less,
                                        (None, Some(_)) => std::cmp::Ordering::Greater,
                                        (None, None) => std::cmp::Ordering::Equal,
                                    }
                                });
                            }
                            let params = params.clone();
                            let metrics = metrics.clone();
                            rows.into_iter().map(|run| {
                                let tag_map: BTreeMap<String, String> = run
                                    .tags
                                    .clone()
                                    .unwrap_or_default()
                                    .iter()
                                    .filter_map(|t| split_param(t))
                                    .collect();
                                let status = run.status.to_string();
                                view! {
                                    <tr class="hover:bg-slate-800/30 transition-colors">
                                        <td class="px-4 py-3 font-mono text-white whitespace-nowrap">{run.name.clone()}</td>
                                        {params.iter().map(|p| {
                                            let v = tag_map.get(p).cloned().unwrap_or_else(|| "-".to_string());
                                            view! { <td class="px-4 py-3 font-mono text-slate-300">{v}</td> }
                                        }).collect_view()}
                                        {metrics.iter().map(|m| {
                                            let v = metric_value(&run, m)
                                                .map(short_number)
                                                .unwrap_or_else(|| "-".to_string());
                                            view! { <td class="px-4 py-3 font-mono text-slate-300 text-right">{v}</td> }
                                        }).collect_view()}
                                        <td class="px-4 py-3 text-slate-400">{status}</td>
                                    </tr>
                                }
                            }).collect_view()
                        }}
                    </tbody>
                </table>
            </div>
        </div>
    }
}

/// Clicking a column sorts by it; clicking it again reverses.
fn toggle_sort(sort_by: RwSignal<Option<String>>, sort_desc: RwSignal<bool>, key: &str) {
    if sort_by.get_untracked().as_deref() == Some(key) {
        sort_desc.update(|d| *d = !*d);
    } else {
        sort_by.set(Some(key.to_string()));
        sort_desc.set(false);
    }
}
