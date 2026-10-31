/* L10-J PSD import adapter. ag-psd (the vendored, untouched ag-psd.vendor.mjs, behind the `readPsd` port) stays the parser. The plan is the
 * baseline `importChildren` / `importFile` decision rebuilt without a side effect: one layer per top-level child that has a canvas and is
 * not hidden (`!layer.canvas || layer.hidden` skips), the baseline name counter, the placement rect, the opacity and the one-time canvas
 * resize rule. An injected owned PNG stage encodes eligible canvases into scoped FilePNG sources for the SAME native batch as their
 * raster placements. Without that provider the original unavailable rows refuse the whole batch. Native admission/history remain the
 * only publisher; no individual asset attach or legacy writer is reachable. */
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
    function changed(ctx) {
      const ready = core.begin();
      if (!ready.ok) return { status: ready.status, reason: ready.reason };
      const next = ready.ctx;
      return next.session !== ctx.session || next.frame !== ctx.frame ||
        ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'].some(key => next.before[key] !== ctx.before[key]) ||
        next.selection.length !== ctx.selection.length || next.selection.some((id, i) => id !== ctx.selection[i])
        ? { status: 'stale', reason: 'context_changed' } : null;
    }
    async function importRaster(ctx, psd, decided, batch) {
      let attempt;
      async function abort(outcome) {
        if (attempt) {
          const cleanup = await attempt.abort();
          if (cleanup.status === 'retained') outcome.staging = cleanup;
        }
        return Object.assign({ plan: decided, eligible: decided.count }, outcome);
      }
      try {
        attempt = ports.stage.createAttempt(ctx);
        const sources = [], raster = [];
        for (const row of decided.rows) {
          const layerRef = `psd:${row.childIndex}`, assetId = `${layerRef}:asset`;
          sources.push(await attempt.stageCanvas(assetId, row.name, psd.children[row.childIndex].canvas));
          const stale = changed(ctx); if (stale) return abort(stale);
          raster.push({ sourceId: `${layerRef}:raster`, entry: {
            target: { contextId: 'scene-root', frameScope: { kind: 'authored', frame: ctx.frame }, layerUid: layerRef, strokeId: '' },
            item: { assetId, x: row.x, y: row.y, width: row.w, height: row.h, opacity: row.opacity },
          } });
        }
        const outcome = await attempt.submit(Object.assign({}, batch, { unavailable: [], sources, raster }));
        const done = Object.assign({ plan: decided, eligible: decided.count }, outcome);
        if (outcome.status === 'imported') {
          done.count = decided.count;
          if (outcome.readback === 'confirmed') notify(successToast(decided.count, t));
          if (outcome.readback === 'confirmed' && outcome.selected && decided.resize && ports.resizeCanvas) {
            try { done.canvasResize = { status: await ports.resizeCanvas(...decided.resize), width: decided.resize[0], height: decided.resize[1] }; }
            catch (_) { done.canvasResize = { status: 'failed' }; }
          }
        }
        return done;
      } catch (error) {
        const code = error && error.code;
        return abort({ status: code === 'invalid_request' ? 'invalid' : code === 'stale_revision' ? 'stale' : 'unavailable', reason: 'png_stage_failed', code: code || 'unavailable' });
      }
    }
    async function importFile(file) {
      let begun = core.begin();
      if (!begun.ok && begun.reason === 'no_presented_native_session' && typeof core.beginSettled === 'function') begun = await core.beginSettled();
      if (!begun.ok) return { status: begun.status, reason: begun.reason };
      let psd;
      try { psd = await ports.readPsd(file); } catch (err) { notify(failureToast(err, t)); return { status: 'failed', message: String(err && err.message || err) }; }
      // The context is read again after the (asynchronous) parse: the pin used for the batch is the one the submit sees.
      let ready = core.begin();
      if (!ready.ok && ready.reason === 'no_presented_native_session' && typeof core.beginSettled === 'function') ready = await core.beginSettled();
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
      if (ports.stage && typeof ports.stage.createAttempt === 'function') return importRaster(ctx, psd, decided, batch);
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
