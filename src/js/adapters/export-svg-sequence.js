// SVG sequence export binding (P18/#1020): where the bounded export job
// (application/export-job.js) meets the live document and the Tauri
// filesystem. export.js hands over its existing helpers as ports; this file
// supplies the document fingerprint and the job's filesystem contract and
// maps the terminal job state back to the result shapes render-manager.js
// and the export dialog already consume.
//
// Fingerprint = "is this still the same document view the job started on":
// the application document identity + revision (opacity-application.js
// meta), the scene version app.js bumps on every frame save/load, the
// component/montage context and the canvas size. A change between two
// batches ends the job (document_changed / document_replaced) instead of
// mixing revisions across files — see export-job.js for the contract.
var NemoExportSvgSequence = (function () {
  'use strict';

  function documentFingerprint() {
    var meta = (window.NemoOpacityApplication && window.NemoOpacityApplication.meta)
      ? window.NemoOpacityApplication.meta() : {};
    return {
      documentId: meta.documentId || '',
      key: [meta.revision || 0, window._sceneVersion || 0, state.activeSymbolId || '', state.activeMontageViewId || '',
        state.canvasW, state.canvasH, state.layers.length].join('|'),
    };
  }

  function newId() { return 'xj' + Date.now().toString(36) + Math.random().toString(36).slice(2, 8); }

  // Today's result shapes, so the callers keep working unchanged.
  function result(terminal, dir) {
    if (terminal.status === 'succeeded') return { ok: true, dir: dir };
    if (terminal.status === 'cancelled') return { cancelled: true };
    return { ok: false, error: (terminal.error && terminal.error.message) || 'export failed' };
  }

  // ports: evaluateFrame(f) -> svg text (export.js exportFrameSVGString),
  //        frameName(i), tauri() -> window.__TAURI__, mkdir(dir), removeDir(dir),
  //        writeText(path, text), saveAllLayerFrames().
  function create(ports) {
    var job = NemoExportJob.create({
      newId: newId,
      fingerprint: documentFingerprint,
      beforeCapture: function () { ports.saveAllLayerFrames(); },
      evaluateFrame: ports.evaluateFrame,
      frameName: ports.frameName,
      // Created-or-not is what decides whether cleanup may remove the directory.
      mkdir: async function (dir) {
        var existed = false;
        try { existed = !!(await ports.tauri().fs.exists(dir)); } catch (e) { /* treat as absent */ }
        await ports.mkdir(dir);
        return { created: !existed };
      },
      write: ports.writeText,
      // Relative names the job wrote, its directory, and whether it created
      // that directory: pre-existing files and directories are never touched.
      remove: async function (names, dir, createdDir) {
        var fs = ports.tauri().fs;
        for (var i = 0; i < names.length; i++) { try { await fs.remove(dir + '/' + names[i]); } catch (e) { /* best effort */ } }
        if (createdDir) await ports.removeDir(dir);
      },
    });

    // One call for the existing callers: begin, wait for the terminal state,
    // map it. Cancellation and status stay reachable through `job` (P19).
    async function run(input) {
      var begun = job.begin(input);
      if (!begun.ok) return { ok: false, error: begun.error.message };
      var terminal = await job.done(begun.jobId);
      return result(terminal, input.dir);
    }

    return { begin: job.begin, status: job.status, cancel: job.cancel, done: job.done, meta: job.meta, run: run };
  }

  return { create: create, documentFingerprint: documentFingerprint, result: result };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoExportSvgSequence;
