/* L10-J Figma import adapter. The Figma REST API (`GET /v1/files/:key?geometry=paths`, `/nodes`, `/images`) stays the parser and the
 * node walk of the baseline figma-import.js is kept verbatim as pure functions: one layer per visible top-level FRAME, vector geometry as
 * one SVG string per frame in frame-local space (parsed by Paper through native-import-svg.js with `noFit`), the frame's own box as the
 * canvas size. The whole file becomes ONE batch: N layers, M vector records, the walk's skipped elements as typed skips, and every text
 * block or image fill as an `unavailable` row naming its missing provider (lane 6 text / raster records, Assistant2 retained fonts), so
 * the owner refuses the batch instead of the importer dropping them. Nothing is fetched for an element no provider can hold. The canvas
 * resize of the first frame is a separate project-settings edit sent only AFTER the batch committed. */
(function (root, factory) {
  const svg = typeof module === 'object' && module.exports ? require('./native-import-svg.js') : root.NemoNativeImportSvg;
  const api = factory(svg);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeImportFigma = api;
}(typeof globalThis === 'object' ? globalThis : this, function (svg) {
  'use strict';

  // ---- verbatim baseline pure functions (figma-import.js) ----
  function matIdentity() { return { a: 1, b: 0, c: 0, d: 1, e: 0, f: 0 }; }
  function matMultiply(m1, m2) {
    return {
      a: m1.a * m2.a + m1.c * m2.b, b: m1.b * m2.a + m1.d * m2.b, c: m1.a * m2.c + m1.c * m2.d, d: m1.b * m2.c + m1.d * m2.d,
      e: m1.a * m2.e + m1.c * m2.f + m1.e, f: m1.b * m2.e + m1.d * m2.f + m1.f,
    };
  }
  function matInvert(m) {
    const det = m.a * m.d - m.b * m.c;
    if (!det) return matIdentity();
    return { a: m.d / det, b: -m.b / det, c: -m.c / det, d: m.a / det, e: (m.c * m.f - m.d * m.e) / det, f: (m.b * m.e - m.a * m.f) / det };
  }
  function nodeAbsMatrix(node) {
    const t = node.absoluteTransform;
    if (t && t.length === 2) return { a: t[0][0], c: t[0][1], e: t[0][2], b: t[1][0], d: t[1][1], f: t[1][2] };
    const bb = node.absoluteBoundingBox;
    return { a: 1, b: 0, c: 0, d: 1, e: bb ? bb.x : 0, f: bb ? bb.y : 0 };
  }
  function clamp01(v) { return v < 0 ? 0 : v > 1 ? 1 : v; }
  // Fills / strokes arrays are ordered bottom to top: the TOP-most visible SOLID (or IMAGE with a ref) wins.
  function firstSolidPaint(paints) {
    if (!paints) return null;
    for (let i = paints.length - 1; i >= 0; i--) { const p = paints[i]; if (p && p.visible !== false && p.type === 'SOLID') return p; }
    return null;
  }
  function firstImagePaint(paints) {
    if (!paints) return null;
    for (let i = paints.length - 1; i >= 0; i--) { const p = paints[i]; if (p && p.visible !== false && p.type === 'IMAGE' && p.imageRef) return p; }
    return null;
  }
  function paintToCss(paint, nodeOpacity) {
    if (!paint) return null;
    const c = paint.color || { r: 0, g: 0, b: 0, a: 1 };
    const a = clamp01((paint.opacity != null ? paint.opacity : 1) * (nodeOpacity != null ? nodeOpacity : 1) * (c.a != null ? c.a : 1));
    return 'rgba(' + Math.round(clamp01(c.r) * 255) + ',' + Math.round(clamp01(c.g) * 255) + ',' + Math.round(clamp01(c.b) * 255) + ',' + a + ')';
  }
  function geometryToPathTags(geomArr, cssColor, mat) {
    if (!geomArr || !geomArr.length || !cssColor) return [];
    const byRule = {};
    geomArr.forEach((g) => {
      if (!g || !g.path) return;
      const rule = g.windingRule === 'EVENODD' ? 'evenodd' : 'nonzero';
      byRule[rule] = (byRule[rule] || '') + ' ' + g.path;
    });
    const mtx = 'matrix(' + mat.a + ' ' + mat.b + ' ' + mat.c + ' ' + mat.d + ' ' + mat.e + ' ' + mat.f + ')';
    return Object.keys(byRule).map((rule) => '<path transform="' + mtx + '" d="' + byRule[rule].trim() + '" fill-rule="' + rule + '" fill="' + cssColor + '"/>');
  }
  const ALIGN_MAP = { LEFT: 'left', CENTER: 'center', RIGHT: 'right', JUSTIFIED: 'left' };
  // Walks one node and its descendants into FRAME-LOCAL space; geometry leaves stop the recursion, an image fill wins over fillGeometry
  // (and still emits its stroke), TEXT becomes a descriptor, a childless non-GROUP node is reported as skipped.
  function walkNode(node, frameInv, out) {
    if (!node || node.visible === false) return;
    const mat = matMultiply(frameInv, nodeAbsMatrix(node));
    const imagePaint = firstImagePaint(node.fills);
    if (imagePaint) {
      const bb = node.absoluteBoundingBox;
      out.images.push({ imageRef: imagePaint.imageRef, mat, w: bb ? bb.width : 0, h: bb ? bb.height : 0, opacity: node.opacity != null ? node.opacity : 1, name: node.name });
      if (node.strokeGeometry && node.strokeGeometry.length) {
        const imgStrokeCss = paintToCss(firstSolidPaint(node.strokes), node.opacity);
        if (imgStrokeCss) out.svgTags = out.svgTags.concat(geometryToPathTags(node.strokeGeometry, imgStrokeCss, mat));
      }
      return;
    }
    const hasFillGeo = node.fillGeometry && node.fillGeometry.length;
    const hasStrokeGeo = node.strokeGeometry && node.strokeGeometry.length;
    if (hasFillGeo || hasStrokeGeo) {
      if (hasFillGeo) out.svgTags = out.svgTags.concat(geometryToPathTags(node.fillGeometry, paintToCss(firstSolidPaint(node.fills), node.opacity), mat));
      if (hasStrokeGeo) out.svgTags = out.svgTags.concat(geometryToPathTags(node.strokeGeometry, paintToCss(firstSolidPaint(node.strokes), node.opacity), mat));
      if (!hasFillGeo && !hasStrokeGeo) out.skipped.push(node.type + ' (' + node.name + ', no paintable geometry)');
      return;
    }
    if (node.type === 'TEXT') {
      const tbb = node.absoluteBoundingBox;
      out.texts.push({ text: node.characters || '', node, x: mat.e, y: mat.f, w: tbb ? tbb.width : 200, style: node.style || {},
        color: paintToCss(firstSolidPaint(node.fills), node.opacity) || 'rgba(0,0,0,1)' });
      return;
    }
    if (node.children && node.children.length) { node.children.forEach((c) => walkNode(c, frameInv, out)); return; }
    if (node.type !== 'GROUP') out.skipped.push(node.type + ' (' + (node.name || '?') + ')');
  }
  function parseFileKey(urlOrKey) {
    if (!urlOrKey) return null;
    const s = urlOrKey.trim();
    const m = s.match(/figma\.com\/(?:file|design|proto)\/([a-zA-Z0-9]+)/);
    if (m) return m[1];
    if (/^[a-zA-Z0-9]+$/.test(s)) return s;
    return null;
  }
  function parseNodeId(urlOrKey) {
    if (!urlOrKey) return null;
    const m = String(urlOrKey).match(/[?&]node-id=([^&]+)/);
    if (!m) return null;
    let raw = decodeURIComponent(m[1]);
    if (raw.indexOf(':') < 0) raw = raw.replace('-', ':');
    return raw;
  }

  // ---- file JSON -> plan (convertFileJson without any side effect) ----
  const NO_ARTBOARD = 'La sélection Figma ne contient pas d’artboard (frame) — sélectionne un artboard, ou un groupe/section qui en contient, avant de copier le lien';
  function textRequest(t) {
    const size = t.style.fontSize || 24;
    const lhPx = t.style.lineHeightPx;
    return { name: t.node.name, text: t.text, family: t.style.fontFamily, size, color: t.color, align: ALIGN_MAP[t.style.textAlignHorizontal] || 'left',
      width: t.w || null, x: t.x, y: t.y, bold: (t.style.fontWeight || 400) >= 700, italic: !!t.style.italic,
      lineHeightMult: (lhPx && size) ? (lhPx / size) : null, letterSpacing: t.style.letterSpacing || 0 };
  }
  function convert(figmaJson, opts, canvas) {
    opts = opts || {};
    const plan = { frames: [], resize: null, skipped: [] };
    let frames;
    if (opts.scopedNode) {
      const sn = opts.scopedNode;
      frames = sn.type === 'FRAME' ? [sn] : (sn.children || []).filter((n) => n.type === 'FRAME');
      if (!frames.length) { plan.skipped.push(NO_ARTBOARD); return plan; }
    } else {
      const doc = figmaJson && figmaJson.document;
      if (!doc || !doc.children || !doc.children.length) { plan.skipped.push('empty document'); return plan; }
      const page = doc.children[opts.pageIndex || 0] || doc.children[0];
      frames = (page.children || []).filter((n) => n.type === 'FRAME');
      if (opts.frameNames && opts.frameNames.length) frames = frames.filter((f) => opts.frameNames.indexOf(f.name) >= 0);
      if (!frames.length) { plan.skipped.push('no FRAME node found on this page'); return plan; }
    }
    let canvasW = canvas.w, canvasH = canvas.h;
    for (let fi = 0; fi < frames.length; fi++) {
      const frame = frames[fi];
      if (frame.visible === false) continue;
      const frameInv = matInvert(nodeAbsMatrix(frame));
      const out = { svgTags: [], texts: [], images: [], skipped: [] };
      (frame.children || []).forEach((c) => walkNode(c, frameInv, out));
      const bb = frame.absoluteBoundingBox;
      // Same quirk as the baseline: the canvas follows the FIRST entry of the frame list, and only when that frame is visible and sized.
      if (fi === 0 && bb && bb.width > 0 && bb.height > 0) { plan.resize = [Math.round(bb.width), Math.round(bb.height)]; canvasW = plan.resize[0]; canvasH = plan.resize[1]; }
      const svgW = bb ? Math.round(bb.width) : canvasW, svgH = bb ? Math.round(bb.height) : canvasH;
      const images = [], skipped = out.skipped.slice();
      if (opts.imageMap) {
        out.images.forEach((img) => {
          const source = opts.imageMap[img.imageRef];
          if (!source) { skipped.push('IMAGE fill (' + img.name + '): no imageMap entry for ' + img.imageRef); return; }
          images.push({ name: img.name, imageRef: img.imageRef, source, width: Math.max(1, img.w), height: Math.max(1, img.h), x: img.mat.e + img.w / 2, y: img.mat.f + img.h / 2, opacity: img.opacity });
        });
      } else if (out.images.length) skipped.push(out.images.length + ' image fill(s) skipped — pass opts.imageMap (GET /v1/files/:key/images) to import them');
      plan.frames.push({
        index: fi, layerName: frame.name || ('Figma ' + (fi + 1)),
        svgText: out.svgTags.length ? '<svg xmlns="http://www.w3.org/2000/svg" width="' + svgW + '" height="' + svgH + '">' + out.svgTags.join('') + '</svg>' : null,
        texts: out.texts.filter((t) => t.text).map(textRequest), images, skipped,
      });
      plan.skipped = plan.skipped.concat(skipped);
    }
    return plan;
  }

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

  function create(ports) {
    const P = ports.paper, core = ports.core, t = ports.t, notify = ports.notify || function () {};
    // Imports an already fetched file. Returns the baseline-shaped report next to the typed status of the single batch.
    async function importFile(figmaJson, opts) {
      const begun = core.begin();
      if (!begun.ok) return { status: begun.status, reason: begun.reason };
      const ctx = begun.ctx, canvas = { w: ctx.before.canvasW, h: ctx.before.canvasH };
      const plan = convert(figmaJson, opts, canvas);
      const report = { framesImported: 0, shapesImported: 0, textsImported: 0, imagesImported: 0, skipped: plan.skipped.slice() };
      if (!plan.frames.length) return { status: 'nothing', report, plan };
      const batch = { layers: [], records: [], skipped: [], unavailable: [] };
      const frameNotes = [];
      try {
        plan.frames.forEach((fp) => {
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
              shapes = out.records.length; classes = parsed.skipped.map((s) => s.className);
            }
          }
          frameNotes.push({ shapes, classes, ran: !!fp.svgText });
          fp.skipped.forEach((reason, k) => batch.skipped.push({ kind: 'element', sourceId: `${layerRef}:skip:${k}`, reason }));
          fp.texts.forEach((tx, k) => {
            batch.unavailable.push({ sourceId: `${layerRef}:text:${k}`, need: 'text' });
            if (tx.family) batch.unavailable.push({ sourceId: `${layerRef}:font:${k}`, need: 'font' });
          });
          fp.images.forEach((img, k) => batch.unavailable.push({ sourceId: `${layerRef}:image:${k}`, need: 'raster-asset' }));
        });
      } catch (error) {
        if (error instanceof svg.Refusal) return { status: 'invalid', reason: error.reason, detail: error.detail, report, plan };
        throw error;
      }
      const outcome = await core.submit(ctx, batch);
      if (outcome.status !== 'imported') return Object.assign({ report, plan, unavailable: batch.unavailable.map((u) => u.need) }, outcome);
      report.framesImported = plan.frames.length;
      report.shapesImported = batch.records.length;
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
  return Object.freeze({
    create, convert, fetchFigma, summaryMessage, walkNode, parseFileKey, parseNodeId, textRequest,
    matIdentity, matMultiply, matInvert, nodeAbsMatrix, clamp01, firstSolidPaint, firstImagePaint, paintToCss, geometryToPathTags, NO_ARTBOARD,
  });
}));
