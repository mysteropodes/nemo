// Runs vectorize-wasm's actual tracing call OFF the main thread (2026-09,
// "que le rastérize se fasse en arrière-plan" — Cyril). vectorize_image()
// is a synchronous wasm export: color clustering + spline fitting on a
// real photo can take real seconds, and a synchronous call blocks
// whichever thread calls it for its entire duration regardless of how
// the JS around it is wrapped in async/await — awaiting a promise only
// yields BETWEEN synchronous calls, never DURING one. A Worker is the
// only way this doesn't freeze the editor: same wasm module, same
// vectorize_image() export, loaded fresh in this separate thread.
//
// Module worker (the `type:'module'` requirement lives in how this file
// gets constructed — see the loader on the main-thread side) so the glue
// JS's own `export function vectorize_image` / `export default` shape
// (wasm-pack --target web output, same as geometry-wasm's) can be
// imported directly, no bundler-specific worker-loader needed.
let ready = false;
const bust = Date.now();
import('../wasm-vectorize/vectorize_wasm.js?v=' + bust).then((mod) => {
  return mod.default({ module_or_path: new URL('../wasm-vectorize/vectorize_wasm_bg.wasm?v=' + bust, import.meta.url) }).then(() => {
    ready = true;
    self.postMessage({ type: 'ready' });
    self.onmessage = (e) => {
      const { bytes, configJson, jobId } = e.data;
      try {
        const resultJson = mod.vectorize_image(bytes, configJson);
        self.postMessage({ type: 'done', jobId, resultJson });
      } catch (err) {
        self.postMessage({ type: 'error', jobId, message: err && err.message ? err.message : String(err) });
      }
    };
  });
}).catch((err) => {
  self.postMessage({ type: 'load-error', message: err && err.message ? err.message : String(err) });
});
