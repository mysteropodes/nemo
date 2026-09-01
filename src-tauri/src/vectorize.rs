// Image vectorization — Tauri command wrapper. The actual pipeline
// (vtracer color clustering + spline fitting) lives in `vectorize-core`
// (repo root), shared verbatim with `vectorize-wasm/` — see that crate's
// own module doc for why (CLAUDE.md §3: two callers of the same logic
// must never duplicate it). This file is now only base64 decode in,
// JSON-shaped Result out.
//
// Kept around (rather than dropping the native path entirely once the
// wasm build proved to work everywhere, including inside the Tauri
// webview itself) only in case a future desktop-only optimization (e.g.
// multi-threaded tracing via rayon, not available to a
// wasm32-unknown-unknown build without extra cross-origin-isolation
// plumbing) becomes worth it. Today both paths run the exact same
// single-threaded vectorize-core, so there is no behavioral difference —
// vectorize-bridge.js prefers the wasm path unconditionally.

use base64::Engine;
use vectorize_core::{vectorize_image_bytes, VectorizeConfigIn, VectorizeResult};

#[tauri::command]
pub async fn vectorize_image(image_base64: String, config: VectorizeConfigIn) -> Result<VectorizeResult, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(image_base64.as_bytes())
        .map_err(|e| e.to_string())?;
    vectorize_image_bytes(&bytes, config)
}
