// Phase C5 — Shape tools (line/rect/ellipse). Genuinely trivial geometry —
// included for completeness and so JS never needs to hand-roll segment
// arrays itself, keeping the "Rust owns the geometry, JS just orchestrates"
// convention consistent across every tool, not just the complex ones.
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct SegOut {
    point: [f64; 2],
    #[serde(rename = "handleIn")]
    handle_in: [f64; 2],
    #[serde(rename = "handleOut")]
    handle_out: [f64; 2],
}
#[derive(Serialize)]
struct ShapeOut {
    segments: Vec<SegOut>,
    closed: bool,
}
fn corner(p: [f64; 2]) -> SegOut {
    SegOut { point: p, handle_in: [0.0, 0.0], handle_out: [0.0, 0.0] }
}

#[wasm_bindgen]
pub fn line_segments(x0: f64, y0: f64, x1: f64, y1: f64) -> String {
    let out = ShapeOut { segments: vec![corner([x0, y0]), corner([x1, y1])], closed: false };
    serde_json::to_string(&out).unwrap()
}

#[wasm_bindgen]
pub fn rect_segments(x0: f64, y0: f64, x1: f64, y1: f64) -> String {
    let out = ShapeOut {
        segments: vec![corner([x0, y0]), corner([x1, y0]), corner([x1, y1]), corner([x0, y1])],
        closed: true,
    };
    serde_json::to_string(&out).unwrap()
}

/// Bezier oval fitting the (x0,y0)-(x1,y1) bounding box — same kappa-
/// constant 4-curve construction Paper.js's `Path.Ellipse` uses, so an
/// ellipse drawn here looks identical to one drawn today.
#[wasm_bindgen]
pub fn ellipse_segments(x0: f64, y0: f64, x1: f64, y1: f64) -> String {
    const K: f64 = 0.5522847498;
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (rx, ry) = ((x1 - x0).abs() / 2.0, (y1 - y0).abs() / 2.0);
    let (kx, ky) = (rx * K, ry * K);
    let segments = vec![
        SegOut { point: [cx + rx, cy], handle_in: [0.0, -ky], handle_out: [0.0, ky] },
        SegOut { point: [cx, cy + ry], handle_in: [kx, 0.0], handle_out: [-kx, 0.0] },
        SegOut { point: [cx - rx, cy], handle_in: [0.0, ky], handle_out: [0.0, -ky] },
        SegOut { point: [cx, cy - ry], handle_in: [-kx, 0.0], handle_out: [kx, 0.0] },
    ];
    let out = ShapeOut { segments, closed: true };
    serde_json::to_string(&out).unwrap()
}
