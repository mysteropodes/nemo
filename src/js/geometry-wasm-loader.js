// Loads the geometry-wasm module (built via wasm-pack, output in src/wasm/)
// and exposes it as window.GeometryWasm = {ready, boolean_op, fill_find,
// create_engine}. Anything that wants Rust-side geometry MUST check `.ready`
// before calling — if this script fails to load (missing build, unsupported
// browser, whatever), the rest of the app keeps working unmodified via its
// existing Paper.js code, by design (see booleanOp()/fillVectorFind() in
// tools.js). create_engine (async — WebGPU device negotiation) is the
// Phase C1 engine-skeleton entry point — not wired into the live app yet,
// only used by its own isolated test harness until later migration phases.
window.GeometryWasm = {
  ready: false, boolean_op: null, boolean_op_multi: null, fill_find: null, create_engine: null, hit_test: null,
  interp_stroke: null, auto_match: null, resample_stroke: null, align_pair: null, erase_at_point: null,
  line_segments: null, rect_segments: null, ellipse_segments: null,
  resolve_symbol_frame: null, effective_frame_index: null, track_points: null, compute_flow: null, interpolate_at: null,
};
// Cache-busted on every load: the glue JS and the .wasm binary live at fixed
// URLs the browser caches aggressively (documented project gotcha — a stale
// cached .wasm survives reloads and even server restarts, so freshly-rebuilt
// engine code silently never reaches the page). This is a localhost dev app;
// re-fetching ~1.4MB per page load is a non-issue compared to debugging
// against a binary that isn't the one on disk.
var wasmBust = Date.now();
import('../wasm/geometry_wasm.js?v=' + wasmBust).then(function (mod) {
  return mod.default({ module_or_path: new URL('../wasm/geometry_wasm_bg.wasm?v=' + wasmBust, import.meta.url) }).then(function () {
    window.GeometryWasm.boolean_op = mod.boolean_op;
    window.GeometryWasm.boolean_op_multi = mod.boolean_op_multi;
    window.GeometryWasm.fill_find = mod.fill_find;
    window.GeometryWasm.create_engine = mod.create_engine;
    window.GeometryWasm.hit_test = mod.hit_test;
    window.GeometryWasm.interp_stroke = mod.interp_stroke;
    window.GeometryWasm.auto_match = mod.auto_match;
    window.GeometryWasm.resample_stroke = mod.resample_stroke;
    window.GeometryWasm.align_pair = mod.align_pair;
    window.GeometryWasm.erase_at_point = mod.erase_at_point;
    window.GeometryWasm.line_segments = mod.line_segments;
    window.GeometryWasm.rect_segments = mod.rect_segments;
    window.GeometryWasm.ellipse_segments = mod.ellipse_segments;
    window.GeometryWasm.resolve_symbol_frame = mod.resolve_symbol_frame;
    window.GeometryWasm.effective_frame_index = mod.effective_frame_index;
    // Class export (not a function): stateful per-stroke input smoother —
    // see stroke-modeler.js (JS reference) / strokemodeler.rs (Rust port).
    window.GeometryWasm.track_points = mod.track_points;   // suivi Lucas-Kanade (track.rs)
    window.GeometryWasm.compute_flow = mod.compute_flow;   // interpolation compensée (interp.rs)
    window.GeometryWasm.interpolate_at = mod.interpolate_at;
    window.GeometryWasm.StrokeModeler = mod.StrokeModeler;
    window.GeometryWasm.ready = true;
  });
}).catch(function (e) {
  console.warn('[geometry-wasm] failed to load, JS fallback will be used', e);
});
// WKWebView WebGPU compatibility: verified 2026-07-06 inside the actual
// built Tauri app (not just the Chromium dev preview) — navigator.gpu
// present, vello engine creation + render + pixel readback all succeeded
// pixel-perfect. The C7 cutover can rely on WebGPU unconditionally on this
// baseline (macOS Sequoia-era WKWebView); older-macOS support remains a
// distribution decision, not a code question.
