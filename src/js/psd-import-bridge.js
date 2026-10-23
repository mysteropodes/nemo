// ---- PSD import (2026-07 — audit gap "pas d'import/export PSD") ----
// Read-only import: one Nemo layer per top-level PSD layer, each a plain
// Raster (the layer's already-composited pixels, exactly as Photoshop
// rendered them — ag-psd does NOT re-render vector/text/smart-object
// layers, it hands back the bitmap Photoshop itself baked into the file,
// which is all any PSD reader can do without reimplementing Photoshop's
// own rendering engine). No PSD *write*/export — reading is the tractable
// half of this gap; writing a spec-correct .psd back out is a much larger
// undertaking (layer effects, text re-flow, color modes) deliberately left
// for later.
//
// Vendored via esm.sh's bundle mode (ag-psd.vendor.mjs, 293KB, ONE
// remaining external import patched to the local node-buffer-shim.vendor.mjs,
// 28KB Buffer polyfill) rather than fetched live from a CDN at runtime —
// same "self-contained vendored dependency" precedent as opentype.js
// (vector-text-bridge.js) earlier this session, since this is a desktop
// app that should work offline.
//
// Scope of THIS pass, deliberately: flat top-level layers only. A PSD
// layer GROUP is skipped as a group and its children are NOT recursively
// flattened — ag-psd represents a group as a `children` node with no
// `canvas` of its own, so it's simply invisible to the "does this child
// have a canvas" check below. Recursing into groups (and deciding how to
// name/order the flattened result) is a reasonable follow-up, not
// attempted here to keep this slice small and correct rather than large
// and half-tested.
(function () {
  var modPromise = null;
  function loadModule() {
    if (!modPromise) modPromise = import('./ag-psd.vendor.mjs');
    return modPromise;
  }
  loadModule().catch(function (e) { console.warn('[psd-import] failed to load ag-psd module', e); });
  // L10-J cutover: the importer itself is native-import-psd.js / native-import-desktop.js. This legacy file only keeps the one literal dynamic import of
  // the unprofiled vendor module and hands the cached promise over (the profiled adapters must not carry that import).
  window.SMAgPsdLoader = { load: loadModule };
})();
