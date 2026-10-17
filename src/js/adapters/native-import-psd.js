/* L10-J PSD import adapter. ag-psd (the vendored, untouched ag-psd.vendor.mjs, behind the `readPsd` port) stays the parser. The plan is the
 * baseline `importChildren` / `importFile` decision rebuilt without a side effect: one layer per top-level child that has a canvas and is
 * not hidden (`!layer.canvas || layer.hidden` skips), the baseline name counter, the placement rect, the opacity and the one-time canvas
 * resize rule. Each layer is a RASTER: the native raster asset record (lane 6 G27 / G38b) does not exist yet, so every eligible layer is an
 * `unavailable` row and the owner refuses the whole batch with the named provider. Nothing is published, the canvas is NOT resized (the
 * baseline resized first and then imported, even when nothing was importable) and no pixel is read or kept. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeImportPsd = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  // The throw of the baseline importFile when no layer carries pixels.
  const NO_PIXELS = 'Aucun calque avec des pixels trouvé (fichier vide, ou uniquement des groupes/calques masqués)';

  // psd: { width, height, children: [{ name, hidden, canvas: { width, height } | undefined, left, top, opacity }] } as ag-psd returns it.
  function plan(psd, canvas) {
    const children = psd.children || [];
    const resize = (psd.width && psd.height && (psd.width !== canvas.w || psd.height !== canvas.h)) ? [psd.width, psd.height] : null;
    const rows = [], skipped = [];
    children.forEach((layer, childIndex) => {
      if (!layer.canvas || layer.hidden) { skipped.push({ childIndex, reason: !layer.canvas ? 'no-canvas' : 'hidden' }); return; }
      const left = layer.left || 0, top = layer.top || 0;
      const w = layer.canvas.width, h = layer.canvas.height;
      rows.push({
        childIndex, name: layer.name || ('Calque PSD ' + (rows.length + 1)),
        x: left + w / 2, y: top + h / 2, w, h, opacity: layer.opacity !== undefined ? layer.opacity : 1,
      });
    });
    return { resize, rows, skipped, count: rows.length, childCount: children.length, error: rows.length ? null : NO_PIXELS };
  }
  // The two toast expressions of the baseline file-picker handler (`t` is SM.t).
  function successToast(count, t) { return count + ' calque' + (count > 1 ? 's' : '') + t('toastImportedSuffix') + (count > 1 ? 's' : '') + ' depuis le PSD'; }
  function failureToast(err, t) { return t('toastPsdImportFailedSuffix') + (err && err.message ? err.message : 'erreur inconnue'); }
  // Reader over the lazily imported ag-psd module: `loadModule()` resolves `{ readPsd }`, the file answers `arrayBuffer()`.
  function readerFor(loadModule) {
    return async function readPsd(file) {
      const mod = await loadModule();
      return mod.readPsd(await file.arrayBuffer());
    };
  }

  function create(ports) {
    const core = ports.core, t = ports.t, notify = ports.notify || function () {};
    async function importFile(file) {
      const begun = core.begin();
      if (!begun.ok) return { status: begun.status, reason: begun.reason };
      let psd;
      try { psd = await ports.readPsd(file); } catch (err) { notify(failureToast(err, t)); return { status: 'failed', message: String(err && err.message || err) }; }
      // The context is read again after the (asynchronous) parse: the pin used for the batch is the one the submit sees.
      const ready = core.begin();
      if (!ready.ok) return { status: ready.status, reason: ready.reason };
      const ctx = ready.ctx;
      const decided = plan(psd, { w: ctx.before.canvasW, h: ctx.before.canvasH });
      if (decided.error) { notify(failureToast(new Error(decided.error), t)); return { status: 'failed', message: decided.error, plan: decided }; }
      const batch = {
        layers: decided.rows.map((r) => ({ sourceId: `psd:${r.childIndex}`, name: r.name })),
        records: [],
        skipped: decided.skipped.map((s) => ({ kind: 'layer', sourceId: `psd:${s.childIndex}`, reason: s.reason })),
        unavailable: decided.rows.map((r) => ({ sourceId: `psd:${r.childIndex}`, need: 'raster-asset' })),
      };
      const outcome = await core.submit(ctx, batch);
      // The owner must refuse a batch whose rows need a provider it lacks. A receipt for one is anomalous: it is reported as unconfirmed
      // (the publish, if any, is visible in the re-read session) and no canvas resize or success toast follows it.
      if (outcome.status === 'imported') return { status: 'unconfirmed', reason: 'owner_accepted_unavailable_rows', plan: decided, receipt: outcome };
      return Object.assign({ plan: decided, unavailable: ['raster-asset'], eligible: decided.count }, outcome);
    }
    return Object.freeze({ importFile });
  }
  return Object.freeze({ create, plan, successToast, failureToast, readerFor, NO_PIXELS });
}));
