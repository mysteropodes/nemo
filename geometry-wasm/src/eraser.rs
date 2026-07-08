// Phase C5 — Eraser tool, ported from eraseAtPoint/eraseExpandStrokeToFill
// in src/js/tools.js. Reuses everything already built this session rather
// than inventing new geometry: a stroke-only path is first expanded to its
// filled outline via kurbo's own stroker (kurbo::stroke::stroke — exactly
// what eraseExpandStrokeToFill hand-rolled in JS via Paper.js), a filled
// item (bucket fill, pressure-brush ribbon) is used as-is, a round eraser
// "brush" is built as a circle polygon, and the actual erase is just the
// already-verified boolean subtract from lib.rs's boolean_op — the eraser
// doesn't need any new geometric algorithm, only new *glue*.
use crate::engine::{build_bezpath, ItemIn};
use crate::{from_multipolygon, to_polygon, PolygonIn};
use geo_booleanop::boolean::{BooleanOp, Operation};
use serde::Deserialize;
use vello::kurbo::{flatten, stroke, PathEl, Stroke, StrokeOpts};
use wasm_bindgen::prelude::*;

fn bezpath_to_polygon_in(path: &vello::kurbo::BezPath) -> PolygonIn {
    let mut exterior: Vec<[f64; 2]> = Vec::new();
    flatten(path.elements().iter().copied(), 0.25, |el| match el {
        PathEl::MoveTo(p) | PathEl::LineTo(p) => exterior.push([p.x, p.y]),
        _ => {}
    });
    PolygonIn { exterior, holes: Vec::new() }
}

fn circle_bezpath(cx: f64, cy: f64, r: f64) -> vello::kurbo::BezPath {
    // Same 4-cubic-bezier kappa-constant circle construction used
    // elsewhere this session (e.g. the C1/C2 test scenes).
    const K: f64 = 0.5522847498;
    let mut p = vello::kurbo::BezPath::new();
    p.move_to((cx + r, cy));
    p.curve_to((cx + r, cy + r * K), (cx + r * K, cy + r), (cx, cy + r));
    p.curve_to((cx - r * K, cy + r), (cx - r, cy + r * K), (cx - r, cy));
    p.curve_to((cx - r, cy - r * K), (cx - r * K, cy - r), (cx, cy - r));
    p.curve_to((cx + r * K, cy - r), (cx + r, cy - r * K), (cx + r, cy));
    p.close_path();
    p
}

// Swept "capsule" (stadium) shape between two points at the given radius —
// same technique buildEraserCapsule uses in tools.js (a 2-point line
// stroked at radius*2 via kurbo's own stroker), used instead of a lone
// circle when the caller supplies eraser_from: subtracting ONE continuous
// shape per drag segment avoids the scalloped "chain of circles" look a
// sequence of discrete circle-subtracts leaves when pointermove samples are
// spaced farther apart than the eraser's own radius.
fn capsule_bezpath(from_x: f64, from_y: f64, to_x: f64, to_y: f64, r: f64) -> vello::kurbo::BezPath {
    let mut line = vello::kurbo::BezPath::new();
    line.move_to((from_x, from_y));
    line.line_to((to_x, to_y));
    let stroke_style = Stroke::new(r.max(0.05) * 2.0);
    stroke(line.elements().iter().copied(), &stroke_style, &StrokeOpts::default(), 0.1)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EraseInput {
    item: ItemIn,
    eraser_x: f64,
    eraser_y: f64,
    eraser_radius: f64,
    // Previous erase-sample point of this drag gesture, world coords —
    // present for every sample after the first. See capsule_bezpath's own
    // comment for why this beats a lone circle mid-drag.
    #[serde(default)]
    eraser_from: Option<[f64; 2]>,
}

/// Erases a round "brush" (or, mid-drag, a capsule swept from the previous
/// sample point) at `(eraserX, eraserY)` out of `item`. Returns the same
/// `[{exterior,holes}, ...]` JSON shape as `boolean_op` (zero, one, or
/// several polygons — erasing the middle of an open stroke splits it into
/// two separate pieces, same as biting a notch out of a filled shape).
#[wasm_bindgen]
pub fn erase_at_point(json: &str) -> Result<String, JsValue> {
    let input: EraseInput = serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let item = &input.item;

    let target_bez = build_bezpath(item).ok_or_else(|| JsValue::from_str("item has no usable geometry"))?;
    // A plain stroke-only path (no fill, no centerline ribbon) has no
    // interior area to subtract from — expand it to its filled outline
    // first, exactly like eraseExpandStrokeToFill did via Paper.js.
    let is_already_filled = item.centerline.is_some() || item.fill_color.is_some();
    let target_poly_in = if is_already_filled {
        bezpath_to_polygon_in(&target_bez)
    } else {
        let stroke_style = Stroke::new(item.stroke_width.max(0.1));
        let outline = stroke(target_bez.elements().iter().copied(), &stroke_style, &StrokeOpts::default(), 0.1);
        bezpath_to_polygon_in(&outline)
    };

    let r = input.eraser_radius.max(0.1);
    let eraser_bez = match input.eraser_from {
        Some([fx, fy]) if (fx - input.eraser_x).hypot(fy - input.eraser_y) >= 0.5 => {
            capsule_bezpath(fx, fy, input.eraser_x, input.eraser_y, r)
        }
        _ => circle_bezpath(input.eraser_x, input.eraser_y, r),
    };
    let eraser_poly_in = bezpath_to_polygon_in(&eraser_bez);

    let target_poly = to_polygon(&target_poly_in);
    let eraser_poly = to_polygon(&eraser_poly_in);
    let result = target_poly.boolean(&eraser_poly, Operation::Difference);
    let out = from_multipolygon(&result);
    serde_json::to_string(&out).map_err(|e| JsValue::from_str(&e.to_string()))
}
