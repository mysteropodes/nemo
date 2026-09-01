// Spins up vectorize-worker.js (a Web Worker running vectorize-wasm's
// actual tracing call, see that file's own comment for why a worker is
// the only way this doesn't freeze the editor) on demand — unlike
// geometry-wasm-loader.js, this does NOT start eagerly at page load: the
// real-time renderer needs its wasm engine on every frame from the
// start, but vectorization is opened rarely, so keeping its ~850KB wasm
// (now loaded INSIDE the worker thread, never on the main thread at all)
// out of the startup bundle only costs a one-time worker spin-up the
// first time someone actually opens the Vectorize dialog.
//
// window.VectorizeWasm.vectorize(bytes, configJson) returns a Promise
// resolving to the result JSON STRING (same shape runVectorize already
// expects — JSON.parse it same as before) — the worker/message-passing
// plumbing is entirely hidden behind this one call.
window.VectorizeWasm = { ready: false, vectorize: null };
(function () {
  var worker = null;
  var readyPromise = null;
  var nextJobId = 1;
  var pending = {}; // jobId -> {resolve, reject}

  function ensureWorker() {
    if (readyPromise) return readyPromise;
    readyPromise = new Promise(function (resolve, reject) {
      // Resolved relative to THIS module's own URL, not the document's —
      // matches geometry-wasm-loader.js's identical convention for its
      // own wasm binary path (import.meta.url), rather than relying on a
      // bare relative string's implementation-specific fallback behavior.
      worker = new Worker(new URL('vectorize-worker.js', import.meta.url), { type: 'module' });
      worker.onmessage = function (e) {
        var msg = e.data;
        if (msg.type === 'ready') {
          window.VectorizeWasm.ready = true;
          resolve();
        } else if (msg.type === 'load-error') {
          reject(new Error(msg.message));
        } else if (msg.type === 'done') {
          var job = pending[msg.jobId];
          if (job) { delete pending[msg.jobId]; job.resolve(msg.resultJson); }
        } else if (msg.type === 'error') {
          var job2 = pending[msg.jobId];
          if (job2) { delete pending[msg.jobId]; job2.reject(new Error(msg.message)); }
        }
      };
      worker.onerror = function (e) {
        reject(new Error(e.message || 'vectorize-worker failed to start'));
      };
    });
    return readyPromise;
  }

  function vectorize(bytes, configJson) {
    return ensureWorker().then(function () {
      return new Promise(function (resolve, reject) {
        var jobId = nextJobId++;
        pending[jobId] = { resolve: resolve, reject: reject };
        // bytes is transferred (not copied) — cheap even for a large
        // photo, and fine since the caller doesn't need its own copy of
        // this Uint8Array back afterward.
        worker.postMessage({ bytes: bytes, configJson: configJson, jobId: jobId }, [bytes.buffer]);
      });
    });
  }
  window.VectorizeWasm.vectorize = vectorize;
})();
