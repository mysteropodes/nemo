// wasm-bindgen boundary for image vectorization (2026-09) — runs the SAME
// vtracer pipeline as the native Tauri command (src-tauri/src/vectorize.rs)
// via the shared ../vectorize-core crate, but here compiled to
// wasm32-unknown-unknown so it works in a plain browser tab AND Nemo's web
// public beta, not just the desktop app. Confirmed to compile clean with
// no patches (vtracer + visioncortex + image all wasm-friendly as-is).
//
// Built like geometry-wasm/ (wasm-pack build --target web --out-dir
// ../src/wasm-vectorize), loaded lazily on demand — see
// vectorize-bridge.js's loader, which only imports this module the first
// time the Vectorize dialog is actually opened, not on every app boot
// (unlike geometry-wasm-loader.js, which the real-time renderer needs
// eagerly). Keeps this rarely-used library's ~1MB+ of wasm out of the
// startup bundle.

use wasm_bindgen::prelude::*;
use vectorize_core::{vectorize_image_bytes, VectorizeConfigIn};

#[wasm_bindgen]
pub fn vectorize_image(bytes: &[u8], config_json: &str) -> Result<String, JsValue> {
    let config: VectorizeConfigIn = if config_json.is_empty() {
        VectorizeConfigIn::default()
    } else {
        serde_json::from_str(config_json).map_err(|e| JsValue::from_str(&e.to_string()))?
    };
    let result = vectorize_image_bytes(bytes, config).map_err(|e| JsValue::from_str(&e))?;
    serde_json::to_string(&result).map_err(|e| JsValue::from_str(&e.to_string()))
}
