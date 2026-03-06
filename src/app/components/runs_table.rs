//! Runs table view with filtering, export (CSV/LaTeX/Typst).

use leptos::prelude::*;
use lucide_leptos::Cog as SettingsIcon;

use crate::app::models::*;
use crate::app::utils::format_date;

#[component]
pub(crate) fn RunsTableView(runs: Vec<Run>, #[prop(into)] on_edit: Callback<Run>) -> impl IntoView {
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
            let finished = run.finished_at.as_deref().unwrap_or("-");
            csv.push_str(&format!(
                "{},{},{},{},{}\n",
                dur, run.started_at, finished, desc, tags
            ));
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
            let finished = run
                .finished_at
                .as_ref()
                .map(|f| format_date(f))
                .unwrap_or_else(|| "-".into());
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
            let finished = run
                .finished_at
                .as_ref()
                .map(|f| format_date(f))
                .unwrap_or_else(|| "-".into());
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
