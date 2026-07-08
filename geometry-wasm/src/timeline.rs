// Phase C5 (components) + C4 (timeline data model) — the frame-resolution
// logic ported from resolveSymbolFrameIdx/getEffectiveStrokes in
// src/js/app.js. Pure functions over the timeline data model: given a
// layer's frames array and a frame index, decide WHAT is actually visible
// there (a keyframe's own strokes, an interpolated frame's strokes, or the
// most recent held keyframe's strokes); given a component instance's play
// settings, decide WHICH internal frame of the symbol shows on a given
// main-timeline frame (loop / ping-pong / once / single, with speed and
// placement offset). No rendering here — this is the data-model brain the
// renderer asks before drawing each frame.
use serde::Deserialize;
use wasm_bindgen::prelude::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SymbolPlayback {
    total_frames: i32,
    #[serde(default)]
    play_mode: Option<String>,
    #[serde(default)]
    speed: Option<f64>,
    #[serde(default)]
    placed_at: Option<i32>,
    #[serde(default)]
    single_frame: Option<i32>,
}

/// Which internal frame of a component/symbol shows at `main_frame_idx` —
/// mirrors resolveSymbolFrameIdx (including the ping-pong mode added this
/// session) exactly.
#[wasm_bindgen]
pub fn resolve_symbol_frame(json: &str, main_frame_idx: i32) -> Result<i32, JsValue> {
    let p: SymbolPlayback = serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let placed_at = p.placed_at.unwrap_or(0);
    let speed = p.speed.unwrap_or(1.0);
    let elapsed = ((main_frame_idx - placed_at).max(0) as f64) * speed;
    let total = p.total_frames.max(1);
    match p.play_mode.as_deref() {
        Some("single") => Ok(p.single_frame.unwrap_or(0).max(0).min(total - 1)),
        Some("once") => Ok((elapsed.floor() as i32).min(total - 1)),
        Some("pingpong") => {
            if total < 2 {
                return Ok(0);
            }
            let cycle = (total - 1) * 2;
            let pos = (elapsed.floor() as i32) % cycle;
            Ok(if pos < total { pos } else { cycle - pos })
        }
        // default: loop
        _ => Ok((elapsed.floor() as i32) % total),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrameFlags {
    #[serde(default)]
    is_keyframe: bool,
    #[serde(default)]
    is_interpolated: bool,
}

/// Which frame index actually SUPPLIES the strokes shown at `frame_idx` —
/// the frame itself if it's a keyframe or an interpolated (tween) frame,
/// otherwise the nearest earlier keyframe (a "held" frame), or -1 when
/// nothing earlier exists (empty). Mirrors the scan in getEffectiveStrokes;
/// returning the index rather than the strokes keeps this a cheap pure
/// function — JS (or later, the Rust document model) owns the actual
/// stroke arrays and just indexes with the answer.
#[wasm_bindgen]
pub fn effective_frame_index(frames_json: &str, frame_idx: i32) -> Result<i32, JsValue> {
    let frames: Vec<FrameFlags> =
        serde_json::from_str(frames_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let idx = frame_idx.max(0) as usize;
    if idx >= frames.len() {
        return Ok(-1);
    }
    let f = &frames[idx];
    if f.is_keyframe || f.is_interpolated {
        return Ok(idx as i32);
    }
    for i in (0..idx).rev() {
        if frames[i].is_keyframe {
            return Ok(i as i32);
        }
    }
    Ok(-1)
}
