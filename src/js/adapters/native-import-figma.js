/* L10-J Figma import adapter. The Figma REST API (`GET /v1/files/:key?geometry=paths`, `/nodes`, `/images`) stays the parser and the
 * node walk of the baseline figma-import.js is kept verbatim as pure functions: one layer per visible top-level FRAME, vector geometry as
 * one SVG string per frame in frame-local space (parsed by Paper through native-import-svg.js with `noFit`), the frame's own box as the
 * canvas size. The whole file becomes ONE batch: layers, vector and native Text records, deduplicated existing font sources, and typed
 * skips. Text resolves through the existing typography catalog and readonly Google lookup, preserving the original fallback order.
 * An injected owned PNG stage places image fills in that SAME batch; absent providers remain explicit unavailable rows. No individual
 * font/asset import or legacy writer is called. The first frame's
 * canvas resize remains a separate project-settings edit sent only AFTER the batch committed. */
(function (root, factory) {
  const svg = typeof module === 'object' && module.exports ? require('./native-import-svg.js') : root.NemoNativeImportSvg;
  const plan = typeof module === 'object' && module.exports ? require('./native-import-figma-plan.js') : root.NemoNativeImportFigmaPlan;
  const api = factory(svg, plan, root);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeImportFigma = api;
}(typeof globalThis === 'object' ? globalThis : this, function (svg, plan, host) {
  'use strict';
  const {convert,parseFileKey,parseNodeId,parsedFont,fontSource,textBlock}=plan;

  // ---- REST glue (importFromToken), the network and the clock behind ports ----
  async function fetchFigma(fetchFn, fileKeyOrUrl, token, opts) {
    const key = parseFileKey(fileKeyOrUrl);
    if (!key) throw new Error('URL ou clé de fichier Figma invalide');
    if (!token) throw new Error('Token Figma manquant (Réglages > Figma)');
    const headers = { 'X-Figma-Token': token };
    const mergedOpts = Object.assign({}, opts);
    let figmaJson;
    const nodeId = parseNodeId(fileKeyOrUrl);
    if (nodeId) {
      const nodeRes = await fetchFn('https://api.figma.com/v1/files/' + key + '/nodes?ids=' + encodeURIComponent(nodeId) + '&geometry=paths', { headers });
      if (!nodeRes.ok) throw new Error('Figma API: HTTP ' + nodeRes.status + (nodeRes.status === 403 ? ' (token invalide ou sans accès à ce fichier)' : ''));
      const nodeJson = await nodeRes.json();
      const entry = nodeJson.nodes && nodeJson.nodes[nodeId];
      if (!entry || !entry.document) throw new Error('Nœud Figma introuvable (node-id ' + nodeId + ') — le lien de sélection ne correspond plus à un élément de ce fichier');
      mergedOpts.scopedNode = entry.document;
    } else {
      const fileRes = await fetchFn('https://api.figma.com/v1/files/' + key + '?geometry=paths', { headers });
      if (!fileRes.ok) throw new Error('Figma API: HTTP ' + fileRes.status + (fileRes.status === 403 ? ' (token invalide ou sans accès à ce fichier)' : ''));
      figmaJson = await fileRes.json();
    }
    try {
      const imgRes = await fetchFn('https://api.figma.com/v1/files/' + key + '/images', { headers });
      if (imgRes.ok) { const imgJson = await imgRes.json(); mergedOpts.imageMap = imgJson.images || {}; }
    } catch (e) { /* image fills degrade to "skipped", not a hard failure */ }
    return { figmaJson, opts: mergedOpts };
  }
  // The settings-panel toast of the baseline click handler.
  function summaryMessage(report) {
    let msg = report.framesImported + ' frame(s), ' + report.shapesImported + ' forme(s), ' + report.textsImported + ' texte(s), ' + report.imagesImported + ' image(s) importé(s)';
    if (report.skipped.length) msg += ' — ' + report.skipped.length + ' élément(s) ignoré(s) (voir console)';
    return msg;
  }

  class IntentRefusal extends Error {
    constructor(status, reason) { super(reason); this.status = status; this.reason = reason; }
  }

  function create(ports) {
    const P = ports.paper, core = ports.core, t = ports.t, notify = ports.notify || function () {};
    function current(ctx) {
      const ready = core.begin();
      if (!ready.ok) throw new IntentRefusal(ready.status, ready.reason);
      const next = ready.ctx;
      if (next.session !== ctx.session || next.frame !== ctx.frame ||
          ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'].some(key => next.before[key] !== ctx.before[key]) ||
          next.selection.length !== ctx.selection.length || next.selection.some((id, i) => id !== ctx.selection[i])) {
        throw new IntentRefusal('stale', 'context_changed');
      }
    }
    async function imageBlob(response, ctx) {
      if (!response.body || typeof response.body.getReader !== 'function') {
        const error = new Error('PNG response stream unavailable'); error.code = 'unavailable'; throw error;
      }
      const reader = response.body.getReader(), chunks = [], limit = 16 * 1024 * 1024;
      let size = 0, complete = false;
      try {
        const declared = response.headers && response.headers.get('Content-Length');
        if (declared !== null && declared !== undefined && (!/^\d+$/.test(declared) || !Number.isSafeInteger(Number(declared)) || Number(declared) > limit)) {
          const error = new Error('PNG response exceeds byte limit'); error.code = 'png_size'; throw error;
        }
        while (true) {
          const next = await reader.read();
          current(ctx);
          if (next.done === true) { complete = true; break; }
          if (!(next.value instanceof Uint8Array) || next.value.byteLength > limit - size) {
            const error = new Error('PNG response exceeds byte limit'); error.code = 'png_size'; throw error;
          }
          size += next.value.byteLength;
          chunks.push(next.value.slice());
        }
        return new Blob(chunks, { type: 'image/png' });
      } finally {
        if (!complete) { try { await reader.cancel(); } catch (_) { /* original refusal remains authoritative */ } }
        reader.releaseLock();
      }
    }
    // Imports an already fetched file. Returns the baseline-shaped report next to the typed status of the single batch.
    async function importFile(figmaJson, opts) {
      let begun = core.begin();
      if (!begun.ok && begun.reason === 'no_presented_native_session' && typeof core.beginSettled === 'function') begun = await core.beginSettled();
      if (!begun.ok) return { status: begun.status, reason: begun.reason };
      const ctx = begun.ctx, canvas = { w: ctx.before.canvasW, h: ctx.before.canvasH };
      const plan = convert(figmaJson, opts, canvas);
      const report = { framesImported: 0, shapesImported: 0, textsImported: 0, imagesImported: 0, skipped: plan.skipped.slice() };
      if (!plan.frames.length) return { status: 'nothing', report, plan };
      const batch = { layers: [], records: [], skipped: [], unavailable: [] };
      const frameNotes = [], sources = [], textRows = [], rasterRows = [], objectOrder = [], fontIds = new Map(), lookups = new Map();
      let attempt;
      async function abort(outcome) {
        if (attempt) {
          const cleanup = await attempt.abort();
          if (cleanup.status === 'retained') outcome.staging = cleanup;
        }
        return outcome;
      }
      async function lookup(family, weight, italic) {
        const api = host.__TAURI__ && host.__TAURI__.core;
        if (!api || typeof api.invoke !== 'function' || typeof host.atob !== 'function' || !host.opentype || typeof host.opentype.parse !== 'function') return null;
        current(ctx);
        const tuple = JSON.stringify([family, weight, italic]);
        if (!lookups.has(tuple)) lookups.set(tuple, Promise.resolve()
          .then(() => api.invoke('fetch_google_font', { family, weight, italic }))
          .then(encoded => parsedFont(encoded, value => host.atob(value), bytes => host.opentype.parse(bytes)), () => false));
        const found = await lookups.get(tuple);
        current(ctx);
        return found;
      }
      try {
        for (const fp of plan.frames) {
          const layerRef = `figma:${fp.index}`;
          batch.layers.push({ sourceId: layerRef, name: fp.layerName });
          let shapes = 0, classes = [];
          if (fp.svgText) {
            const parsed = svg.parse(P, fp.svgText, { noFit: true }, canvas);
            if (parsed.status === 'parse_failed') notify(t('toastSvgImportFailed'));
            else if (parsed.status === 'empty') notify(t('toastSvgImportEmpty'));
            else {
              const out = svg.explode(P, parsed, `${layerRef}:svg`, layerRef, ctx.frame);
              batch.records.push(...out.records); batch.skipped.push(...out.skipped);
              objectOrder.push(...out.records.map(row => row.sourceId));
              shapes = out.records.length; classes = parsed.skipped.map((s) => s.className);
            }
          }
          frameNotes.push({ shapes, classes, ran: !!fp.svgText });
          fp.skipped.forEach((reason, k) => batch.skipped.push({ kind: 'element', sourceId: `${layerRef}:skip:${k}`, reason }));
          for (const [k, tx] of fp.texts.entries()) {
            const font = await fontSource(tx, lookup, report, ports.readCatalog), sourceId = `${layerRef}:text:${k}`;
            current(ctx);
            if (!font) {
              batch.unavailable.push({ sourceId, need: 'text' });
              if (tx.family) batch.unavailable.push({ sourceId: `${layerRef}:font:${k}`, need: 'font' });
              continue;
            }
            const token = JSON.stringify(font.source);
            if (!fontIds.has(token)) {
              const id = `figma:font:${sources.length}`;
              fontIds.set(token, id);
              sources.push({ sourceId: id, name: font.key, mediaType: 'font/ttf', source: font.source });
            }
            textRows.push({ sourceId, entry: { target: { contextId: 'scene-root', frameScope: { kind: 'authored', frame: ctx.frame },
              layerUid: layerRef, strokeId: '' }, block: textBlock(tx, fontIds.get(token), svg.Refusal) } });
            objectOrder.push(sourceId);
          }
          for (const [k, img] of fp.images.entries()) {
            const sourceId = `${layerRef}:image:${k}`;
            if (!ports.stage || typeof ports.stage.createAttempt !== 'function' || typeof ports.fetch !== 'function') {
              batch.unavailable.push({ sourceId, need: 'raster-asset' });
              continue;
            }
            if (!attempt) attempt = ports.stage.createAttempt(ctx);
            const response = await ports.fetch(img.source);
            current(ctx);
            if (!response || !response.ok) {
              const error = new Error('PNG image fetch unavailable');
              error.code = 'unavailable';
              throw error;
            }
            const blob = await imageBlob(response, ctx);
            current(ctx);
            const assetId = `${layerRef}:asset:${k}`;
            sources.push(await attempt.stageFile(assetId, img.name, blob));
            current(ctx);
            rasterRows.push({ sourceId, entry: {
              target: { contextId: 'scene-root', frameScope: { kind: 'authored', frame: ctx.frame }, layerUid: layerRef, strokeId: '' },
              item: { assetId, x: img.x, y: img.y, width: img.width, height: img.height, opacity: img.opacity },
            } });
            objectOrder.push(sourceId);
          }
        }
      } catch (error) {
        if (error instanceof IntentRefusal) return abort({ status: error.status, reason: error.reason, report, plan });
        if (error instanceof svg.Refusal) return abort({ status: 'invalid', reason: error.reason, detail: error.detail, report, plan });
        if (attempt) return abort({ status: error && error.code === 'invalid_request' ? 'invalid' : 'unavailable', reason: 'png_stage_failed', code: error && error.code || 'unavailable', report, plan });
        throw error;
      }
      if (textRows.length) batch.text = textRows;
      if (rasterRows.length) batch.raster = rasterRows;
      if (sources.length) batch.sources = sources;
      if (textRows.length || rasterRows.length) batch.objectOrder = objectOrder;
      let outcome;
      if (attempt) {
        try { outcome = await attempt.submit(batch); }
        catch (_) { return abort({ status: 'unconfirmed', reason: 'png_submit_failed', report, plan }); }
      } else outcome = await core.submit(ctx, batch);
      if (outcome.status !== 'imported') return Object.assign({ report, plan, unavailable: batch.unavailable.map((u) => u.need) }, outcome);
      report.framesImported = plan.frames.length;
      report.shapesImported = batch.records.length;
      report.textsImported = textRows.length;
      report.imagesImported = rasterRows.length;
      frameNotes.forEach((n) => { if (n.ran && n.shapes) notify(svg.message(t, n.shapes, n.classes)); });
      const done = Object.assign({ report, plan }, outcome);
      if (plan.resize && ports.resizeCanvas) {
        try { done.canvasResize = { status: await ports.resizeCanvas(plan.resize[0], plan.resize[1]), width: plan.resize[0], height: plan.resize[1] }; } catch (error) { done.canvasResize = { status: 'failed', message: String(error && error.message || error) }; }
      }
      return done;
    }
    // The settings-panel click: fetch, import, toast; the typed result is returned whatever the toast says.
    async function run(urlOrKey, token, opts) {
      notify('Import Figma…');
      try {
        const fetched = await fetchFigma(ports.fetch, urlOrKey, token, opts);
        const result = await importFile(fetched.figmaJson, fetched.opts);
        if (result.status === 'imported') notify(summaryMessage(result.report));
        return result;
      } catch (err) {
        notify('Import Figma échoué : ' + (err && err.message ? err.message : 'erreur inconnue'));
        return { status: 'failed', message: String(err && err.message || err) };
      }
    }
    return Object.freeze({ importFile, run });
  }
  const {walkNode,textRequest,matIdentity,matMultiply,matInvert,nodeAbsMatrix,clamp01,firstSolidPaint,
    firstImagePaint,paintToCss,geometryToPathTags,NO_ARTBOARD}=plan;
  return Object.freeze({create,convert,fetchFigma,summaryMessage,walkNode,parseFileKey,parseNodeId,textRequest,
    matIdentity,matMultiply,matInvert,nodeAbsMatrix,clamp01,firstSolidPaint,firstImagePaint,paintToCss,geometryToPathTags,NO_ARTBOARD});
}));
