//! Shared utility functions for the frontend.

use chrono::{DateTime, Local};
use leptos::prelude::*;
use std::rc::Rc;

#[derive(Clone)]
pub(crate) struct SidebarContext(pub RwSignal<Option<Rc<dyn Fn() -> AnyView>>, LocalStorage>);

pub(crate) const CHART_COLORS: [&str; 5] = ["#3b82f6", "#10b981", "#f59e0b", "#ef4444", "#8b5cf6"];

/// Clip a polyline to the axis-aligned viewport [x_min, x_max] × [y_min, y_max].
///
/// Returns zero or more connected sub-polylines, each fully inside the viewport.
/// Line segments that cross the boundary are trimmed to the exact intersection
/// point, so lines never "stick" to the edge during pan/zoom.
pub(crate) fn clip_polyline_to_viewport(
    points: &[(f64, f64)],
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
) -> Vec<Vec<(f64, f64)>> {
    if points.len() < 2 {
        return vec![];
    }
    let mut segments: Vec<Vec<(f64, f64)>> = vec![];
    let mut current: Vec<(f64, f64)> = vec![];
    for window in points.windows(2) {
        let (x0, y0) = window[0];
        let (x1, y1) = window[1];
        if let Some((cx0, cy0, cx1, cy1)) = liang_barsky(x0, y0, x1, y1, x_min, x_max, y_min, y_max)
        {
            let start_matches = current
                .last()
                .map(|&(px, py)| (px - cx0).abs() < 1e-10 && (py - cy0).abs() < 1e-10)
                .unwrap_or(false);
            if !start_matches {
                if !current.is_empty() {
                    segments.push(current.clone());
                }
                current = vec![(cx0, cy0)];
            }
            current.push((cx1, cy1));
        } else if !current.is_empty() {
            segments.push(current.clone());
            current = vec![];
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

/// Liang-Barsky parametric line clipping.
/// Returns `Some((x0, y0, x1, y1))` of the clipped segment, or `None` if entirely outside.
#[allow(clippy::too_many_arguments)]
pub(crate) fn liang_barsky(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
) -> Option<(f64, f64, f64, f64)> {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let p = [-dx, dx, -dy, dy];
    let q = [x0 - x_min, x_max - x0, y0 - y_min, y_max - y0];
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 1.0;
    for i in 0..4 {
        if p[i] == 0.0 {
            if q[i] < 0.0 {
                return None;
            }
        } else {
            let t = q[i] / p[i];
            if p[i] < 0.0 {
                if t > t1 {
                    return None;
                }
                if t > t0 {
                    t0 = t;
                }
            } else {
                if t < t0 {
                    return None;
                }
                if t < t1 {
                    t1 = t;
                }
            }
        }
    }
    Some((x0 + t0 * dx, y0 + t0 * dy, x0 + t1 * dx, y0 + t1 * dy))
}

pub(crate) fn download_canvas_as_png(canvas: &web_sys::HtmlCanvasElement, filename: &str) {
    use wasm_bindgen::JsCast;
    let data_url = canvas
        .to_data_url_with_type("image/png")
        .unwrap_or_default();
    let doc = web_sys::window().unwrap().document().unwrap();
    let a: web_sys::HtmlAnchorElement = doc.create_element("a").unwrap().dyn_into().unwrap();
    a.set_href(&data_url);
    a.set_download(filename);
    a.click();
}

pub(crate) fn format_date(iso: &str) -> String {
    if let Ok(dt) = DateTime::parse_from_rfc3339(iso) {
        let local = dt.with_timezone(&Local);
        local.format("%H:%M, %d %b, %Y").to_string()
    } else {
        iso.to_string()
    }
}
