// Phase C3 — hit-testing over the same scene JSON format used by engine.rs's
// render(), so callers can hand it the exact same scene they just rendered.
// Reuses build_bezpath from engine.rs — a pressure-brush `centerline` item
// and a plain `fillColor` item are both "does this filled shape contain the
// point" tests; a `strokeColor`-only item is a "is the point within
// strokeWidth/2 (or the caller's tolerance, whichever is larger) of the
// path" test, via flattening + point-to-segment distance (same pragmatic
// flatten-then-measure approach already used in boolean.rs/fill.rs).
use crate::engine::{build_bezpath, SceneIn};
use serde::Serialize;
use vello::kurbo::{flatten, PathEl, Point, Shape};
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct HitResult {
    #[serde(rename = "layerIndex")]
    layer_index: usize,
    #[serde(rename = "itemIndex")]
    item_index: usize,
    kind: &'static str,
}

fn point_to_segment_dist(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-12 {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
    p.distance(Point::new(a.x + t * dx, a.y + t * dy))
}

fn min_dist_to_path(path: &vello::kurbo::BezPath, p: Point) -> f64 {
    let mut pts: Vec<Point> = Vec::new();
    flatten(path.elements().iter().copied(), 0.5, |el| match el {
        PathEl::MoveTo(pt) | PathEl::LineTo(pt) => pts.push(pt),
        _ => {}
    });
    let mut best = f64::INFINITY;
    for w in pts.windows(2) {
        best = best.min(point_to_segment_dist(p, w[0], w[1]));
    }
    best
}

/// Core scan, reused by both the standalone wasm export below and
/// `VelloEngine::select_at` in engine.rs (which needs the raw tuple to
/// update its persisted selection, not a JSON string).
pub(crate) fn hit_test_scene(scene: &SceneIn, x: f64, y: f64, tolerance: f64) -> Option<(usize, usize, &'static str)> {
    let pt = Point::new(x, y);
    for (li, layer) in scene.layers.iter().enumerate().rev() {
        for (ii, item) in layer.items.iter().enumerate().rev() {
            let bez = match build_bezpath(item) {
                Some(b) => b,
                None => continue,
            };
            // A centerline item's geometry IS the filled ribbon regardless
            // of whether fillColor was set — always a contains-test, never
            // a stroke-distance test, unlike plain segments-with-stroke items.
            if item.centerline.is_some() || item.fill_color.is_some() {
                if bez.contains(pt) {
                    return Some((li, ii, "fill"));
                }
            }
            if item.centerline.is_none() && item.stroke_color.is_some() {
                let tol = tolerance.max(item.stroke_width / 2.0);
                if min_dist_to_path(&bez, pt) <= tol {
                    return Some((li, ii, "stroke"));
                }
            }
        }
    }
    None
}

/// Returns a JSON `{layerIndex,itemIndex,kind}` for the topmost hit item
/// (last layer, last item within it, scanned first — matches draw order:
/// last-drawn is topmost), or the literal string `"null"` for no hit.
///
/// NOT a drop-in for select-bridge.js's click/marquee hit-testing — that
/// caller does far more than "which item is under the point": locked-layer
/// skip (except a symbol/component layer, which must still hit as one
/// rigid whole), cross-layer active-layer switching when the hit lands on
/// a non-active layer, a component-layer fallback scan
/// (hitTestComponentLayers) with its own double-click-to-enter-symbol
/// timing, and node/handle-level hits (hitTestHandles, for the node-edit
/// tool) that this function has no concept of at all — it only knows
/// fill/stroke containment over a static scene snapshot. Wiring this in
/// would mean re-implementing all of that business logic here too, not
/// just swapping the geometry test. Verified unused as of the 2026-07-13
/// optimization pass (grep across src/js/*.js found zero callers) — no
/// measured slowness in select-bridge.js's current Paper.js hitTest() to
/// justify the port either, per the same pass.
#[wasm_bindgen]
pub fn hit_test(scene_json: &str, x: f64, y: f64, tolerance: f64) -> Result<String, JsValue> {
    let scene: SceneIn = serde_json::from_str(scene_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    match hit_test_scene(&scene, x, y, tolerance) {
        Some((li, ii, kind)) => {
            let result = HitResult { layer_index: li, item_index: ii, kind };
            Ok(serde_json::to_string(&result).unwrap())
        }
        None => Ok("null".to_string()),
    }
}
