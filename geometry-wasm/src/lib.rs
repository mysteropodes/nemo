// Boolean path operations (union/subtract/intersect/exclude), ported from
// the JS/Paper.js reference implementation in src/js/tools.js's booleanOp().
// This is the first module of an incremental Rust port — geometry stays
// pure/stateless (polygons in, polygons out) so it can be dropped in behind
// a JS fallback with zero risk to anything else in the app. Curves are
// flattened to polylines on the JS side before crossing into Rust (see
// src/js/geometry-wasm.js) since exact-bezier boolean ops are a much bigger
// undertaking than most 2D vector tools actually bother with — flatten,
// clip, done.
use geo_booleanop::boolean::{BooleanOp, Operation};
use geo_types::{Coordinate, LineString, MultiPolygon, Polygon};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

mod fill;
pub use fill::fill_find;
mod engine;
pub use engine::VelloEngine;
#[cfg(target_arch = "wasm32")]
pub use engine::create_engine;
mod hit;
pub use hit::hit_test;
mod tween;
pub use tween::interp_stroke;
mod tweenmatch;
pub use tweenmatch::{align_pair, auto_match, resample_stroke};
mod eraser;
pub use eraser::erase_at_point;
mod shapes;
pub use shapes::{ellipse_segments, line_segments, rect_segments};
mod track;
pub use track::track_points;
mod interp;
pub use interp::{compute_flow, interpolate_at, FlowField};
mod timeline;
mod strokemodeler;
pub use timeline::{effective_frame_index, resolve_symbol_frame};

#[derive(Deserialize)]
struct PolygonIn {
    exterior: Vec<[f64; 2]>,
    #[serde(default)]
    holes: Vec<Vec<[f64; 2]>>,
}

#[derive(Serialize)]
struct PolygonOut {
    exterior: Vec<[f64; 2]>,
    holes: Vec<Vec<[f64; 2]>>,
}

fn to_line_string(pts: &[[f64; 2]]) -> LineString<f64> {
    LineString(
        pts.iter()
            .map(|p| Coordinate { x: p[0], y: p[1] })
            .collect(),
    )
}

fn to_polygon(p: &PolygonIn) -> Polygon<f64> {
    Polygon::new(
        to_line_string(&p.exterior),
        p.holes.iter().map(|h| to_line_string(h)).collect(),
    )
}

fn from_line_string(ls: &LineString<f64>) -> Vec<[f64; 2]> {
    ls.0.iter().map(|c| [c.x, c.y]).collect()
}

fn from_multipolygon(mp: &MultiPolygon<f64>) -> Vec<PolygonOut> {
    mp.0.iter()
        .map(|poly| PolygonOut {
            exterior: from_line_string(poly.exterior()),
            holes: poly.interiors().iter().map(from_line_string).collect(),
        })
        .collect()
}

/// `op` is one of "unite" | "subtract" | "intersect" | "exclude" (matching
/// the Paper.js method names already used by the JS fallback, so callers
/// don't need to translate between two different vocabularies).
#[wasm_bindgen]
pub fn boolean_op(op: &str, a_json: &str, b_json: &str) -> Result<String, JsValue> {
    let a: PolygonIn =
        serde_json::from_str(a_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let b: PolygonIn =
        serde_json::from_str(b_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let poly_a = to_polygon(&a);
    let poly_b = to_polygon(&b);

    let operation = match op {
        "unite" => Operation::Union,
        "subtract" => Operation::Difference,
        "intersect" => Operation::Intersection,
        "exclude" => Operation::Xor,
        other => return Err(JsValue::from_str(&format!("unknown boolean op: {}", other))),
    };

    let result = poly_a.boolean(&poly_b, operation);
    let out = from_multipolygon(&result);
    serde_json::to_string(&out).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Same operations as `boolean_op`, but `a_json` is a JSON array of polygons
/// (a MultiPolygon) instead of a single one. Needed to fold a 3rd+ operand
/// into an already-disjoint multi-piece accumulator: `boolean_op` can only
/// take a single polygon per side, so a naive JS-side fold that collapses
/// the accumulator to "the single largest piece" between folds silently
/// drops every other disjoint piece already accumulated (e.g. uniting 3
/// mutually non-overlapping shapes loses the middle one). geo_booleanop's
/// BooleanOp trait already implements MultiPolygon-vs-Polygon natively
/// (boolean/mod.rs) — this just exposes that instead of reinventing it.
#[wasm_bindgen]
pub fn boolean_op_multi(op: &str, a_json: &str, b_json: &str) -> Result<String, JsValue> {
    let a: Vec<PolygonIn> =
        serde_json::from_str(a_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let b: PolygonIn =
        serde_json::from_str(b_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let multi_a = MultiPolygon(a.iter().map(to_polygon).collect());
    let poly_b = to_polygon(&b);

    let operation = match op {
        "unite" => Operation::Union,
        "subtract" => Operation::Difference,
        "intersect" => Operation::Intersection,
        "exclude" => Operation::Xor,
        other => return Err(JsValue::from_str(&format!("unknown boolean op: {}", other))),
    };

    let result = multi_a.boolean(&poly_b, operation);
    let out = from_multipolygon(&result);
    serde_json::to_string(&out).map_err(|e| JsValue::from_str(&e.to_string()))
}
