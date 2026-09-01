// Loads the vectorize-wasm module (built via wasm-pack, output in
// src/wasm-vectorize/) on demand — unlike geometry-wasm-loader.js, this
// does NOT import eagerly at page load: the real-time renderer needs its
// wasm engine on every frame from the start, but vectorization is opened
// rarely, so keeping its ~850KB wasm out of the startup bundle only costs
// a one-time fetch the first time someone actually opens the Vectorize
// dialog. window.VectorizeWasm.load() returns a Promise that resolves
// once, cached, for every call after the first.
window.VectorizeWasm = { ready: false, vectorize_image: null, load: null };
(function () {
  var loadingPromise = null;
  function load() {
    if (window.VectorizeWasm.ready) return Promise.resolve();
    if (loadingPromise) return loadingPromise;
    // Cache-busted like geometry-wasm-loader.js — same documented gotcha:
    // the glue JS and .wasm binary live at fixed URLs the browser caches
    // aggressively across dev-server restarts.
    var bust = Date.now();
    loadingPromise = import('../wasm-vectorize/vectorize_wasm.js?v=' + bust).then(function (mod) {
      return mod.default({ module_or_path: new URL('../wasm-vectorize/vectorize_wasm_bg.wasm?v=' + bust, import.meta.url) }).then(function () {
        window.VectorizeWasm.vectorize_image = mod.vectorize_image;
        window.VectorizeWasm.ready = true;
      });
    });
    return loadingPromise;
  }
  window.VectorizeWasm.load = load;
})();
