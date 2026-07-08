// Phase C4 — tween interpolation core, ported from interpStroke/qBez in
// src/js/tweens.js. Deliberately scoped to JUST the interpolation math:
// given two ALREADY-matched, already-resampled (equal point count),
// already-aligned point arrays and an eased t, produce the in-between
// stroke. The much bigger surrounding machinery in tweens.js — auto-
// matching strokes between keyframes via a Hungarian-algorithm assignment
// over shape-feature descriptors, fitting similarity transforms, searching
// reverse/rotate alignments to avoid a closed loop "swirling" — stays in JS
// for now; it's a substantially larger, separate port, not attempted here.
// JS also still owns the easing curve itself (arbitrary bezier UI) and
// motion-arc storage (state.motionArcs) — it resolves both down to a plain
// `et` (already-eased t) and an `arcCtrl` world-space point before calling
// this, so the engine stays decoupled from app/UI state entirely.
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SegIn {
    point: [f64; 2],
    #[serde(default)]
    handle_in: [f64; 2],
    #[serde(default)]
    handle_out: [f64; 2],
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SegOut {
    point: [f64; 2],
    handle_in: [f64; 2],
    handle_out: [f64; 2],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StrokeIn {
    segments: Vec<SegIn>,
    #[serde(default)]
    widths: Option<Vec<f64>>,
    #[serde(default = "default_one")]
    stroke_width: f64,
    #[serde(default = "default_one")]
    opacity: f64,
    fill_color: Option<[u8; 4]>,
    stroke_color: Option<[u8; 4]>,
}
fn default_one() -> f64 {
    1.0
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InterpInput {
    a: StrokeIn,
    b: StrokeIn,
    et: f64,
    arc_ctrl: Option<[f64; 2]>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InterpOutput {
    segments: Vec<SegOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    widths: Option<Vec<f64>>,
    stroke_width: f64,
    opacity: f64,
    fill_color: Option<[u8; 4]>,
    stroke_color: Option<[u8; 4]>,
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}
fn qbez(a: f64, c: f64, b: f64, t: f64) -> f64 {
    let u = 1.0 - t;
    u * u * a + 2.0 * u * t * c + t * t * b
}
// Discrete color switch at the interpolation midpoint, matching the JS
// reference exactly (`et<.5?A:B`) rather than blending RGB — a tween's
// fill/stroke color hard-cuts partway through, it doesn't fade.
fn pick_color(a: &Option<[u8; 4]>, b: &Option<[u8; 4]>, et: f64) -> Option<[u8; 4]> {
    if et < 0.5 {
        *a
    } else {
        *b
    }
}

#[wasm_bindgen]
pub fn interp_stroke(json: &str) -> Result<String, JsValue> {
    let input: InterpInput = serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let (a, b, et) = (&input.a, &input.b, input.et);
    let n = a.segments.len().min(b.segments.len());
    if n == 0 {
        return Err(JsValue::from_str("both strokes need at least one segment"));
    }

    let (mut cxa, mut cya, mut cxb, mut cyb) = (0.0, 0.0, 0.0, 0.0);
    for i in 0..n {
        cxa += a.segments[i].point[0];
        cya += a.segments[i].point[1];
        cxb += b.segments[i].point[0];
        cyb += b.segments[i].point[1];
    }
    let nf = n as f64;
    cxa /= nf;
    cya /= nf;
    cxb /= nf;
    cyb /= nf;

    let arc = input.arc_ctrl.unwrap_or([(cxa + cxb) / 2.0, (cya + cyb) / 2.0]);
    let cx2 = qbez(cxa, arc[0], cxb, et);
    let cy2 = qbez(cya, arc[1], cyb, et);

    let mut segments = Vec::with_capacity(n);
    for i in 0..n {
        let sa = &a.segments[i];
        let sb = &b.segments[i];
        segments.push(SegOut {
            point: [
                cx2 + lerp(sa.point[0] - cxa, sb.point[0] - cxb, et),
                cy2 + lerp(sa.point[1] - cya, sb.point[1] - cyb, et),
            ],
            handle_in: [lerp(sa.handle_in[0], sb.handle_in[0], et), lerp(sa.handle_in[1], sb.handle_in[1], et)],
            handle_out: [lerp(sa.handle_out[0], sb.handle_out[0], et), lerp(sa.handle_out[1], sb.handle_out[1], et)],
        });
    }

    let widths = match (&a.widths, &b.widths) {
        (Some(wa), Some(wb)) => Some(
            (0..n)
                .map(|i| lerp(*wa.get(i).unwrap_or(&1.0), *wb.get(i).unwrap_or(&1.0), et))
                .collect(),
        ),
        _ => None,
    };

    let output = InterpOutput {
        segments,
        widths,
        stroke_width: lerp(a.stroke_width, b.stroke_width, et),
        opacity: lerp(a.opacity, b.opacity, et),
        fill_color: pick_color(&a.fill_color, &b.fill_color, et),
        stroke_color: pick_color(&a.stroke_color, &b.stroke_color, et),
    };
    serde_json::to_string(&output).map_err(|e| JsValue::from_str(&e.to_string()))
}
