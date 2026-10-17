/* L10-J SVG import adapter. Paper.js stays the PARSER and the baker: `project.importSVG` parses, each leaf's accumulated world
 * matrix is baked into a clone, the shrink-only fit (or `noFit`) places the artwork, and `explodeLeaf` (the verbatim baseline
 * exterior / hole classification with the keyhole merge of `_mergeHoleIntoExterior`) flattens compound leaves into independent
 * Paths held in memory only. Each resulting Path becomes one native `editable-vector-path` record carrying exactly what the
 * baseline serializer persisted for it. The whole import (all files, all records, every skipped element and unclaimed hole) is ONE
 * batch sent through native-import-core.js; no Paper item is inserted into a layer and no legacy frame writer is reconnected.
 * Toasts and refusals follow the baseline `importSVGString` order: parse, empty, layer. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeImportSvg = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  const MIN_ANCHORS = 2, MAX_ANCHORS = 256;
  const HEX = /^#[0-9a-fA-F]{6}(?:[0-9a-fA-F]{2})?$/;

  // ---- verbatim baseline helpers (app.js colorHex8 / _r3 / path serializer, tools.js hole merge, svg-import.js worldMatrix / collectLeaves) ----
  function colorHex8(c) {
    if (!c) return null;
    if (c.alpha === undefined || c.alpha >= 1) return c.toCSS(true);
    const rgba = c.toCSS(false).match(/[\d.]+/g);
    function h(n) { return Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, '0'); }
    return '#' + h(rgba[0]) + h(rgba[1]) + h(rgba[2]) + h(parseFloat(rgba[3]) * 255);
  }
  function r3(v) { return isFinite(v) ? Math.round(v * 1000) / 1000 : 0; }
  function polyClosestPair(a, b) {
    let bestD = Infinity, bi = 0, bj = 0;
    for (let i = 0; i < a.length; i++) {
      for (let j = 0; j < b.length; j++) {
        const dx = a[i].x - b[j].x, dy = a[i].y - b[j].y, d = dx * dx + dy * dy;
        if (d < bestD) { bestD = d; bi = i; bj = j; }
      }
    }
    return { i: bi, j: bj };
  }
  function mergeHoleIntoExterior(extSegs, holeSegs) {
    const pair = polyClosestPair(extSegs.map((s) => s.point), holeSegs.map((s) => s.point));
    const out = [];
    for (let k = 0; k <= pair.i; k++) out.push(extSegs[k]);
    for (let h = 0; h <= holeSegs.length; h++) out.push(holeSegs[(pair.j + h) % holeSegs.length]);
    out.push(extSegs[pair.i]); // back out through the same slit point (zero-width seam)
    for (let k2 = pair.i + 1; k2 < extSegs.length; k2++) out.push(extSegs[k2]);
    return out;
  }
  function worldMatrix(P, item) {
    const m = new P.Matrix(), chain = [];
    let cur = item;
    while (cur) { chain.unshift(cur); cur = cur.parent; }
    chain.forEach((n) => { if (n.matrix) m.append(n.matrix); });
    return m;
  }
  // Pre-order over EVERY node (the root is 0): a leaf is a Path or CompoundPath (a compound is not descended into), an item with
  // children is a container, any other childless item is skipped and reported under `className || 'inconnu'`.
  function collectLeaves(P, root) {
    const leaves = [], skipped = [];
    let next = 0;
    (function go(item) {
      const index = next++;
      if (item instanceof P.Path || item instanceof P.CompoundPath) { leaves.push({ index, item }); return; }
      if (item.children && item.children.length) { item.children.slice().forEach(go); return; }
      skipped.push({ index, className: item.className || 'inconnu' });
    }(root));
    return { leaves, skipped };
  }
  function segsOf(path) {
    return path.segments.map((s) => ({ point: [s.point.x, s.point.y], handleIn: [s.handleIn.x, s.handleIn.y], handleOut: [s.handleOut.x, s.handleOut.y] }));
  }
  // explodeLeaf of the baseline with the layer replaced by an in-memory sink. Returns the flat paths and the compound children that no
  // exterior claimed (the baseline dropped those silently; here they are reported).
  function explodeLeaf(P, leaf) {
    const inserted = [];
    if (!(leaf instanceof P.CompoundPath)) { inserted.push(leaf); return { paths: inserted, unclaimed: [] }; }
    const fillColor = leaf.fillColor, strokeColor = leaf.strokeColor, strokeWidth = leaf.strokeWidth, opacity = leaf.opacity;
    const children = leaf.children.slice();
    const exteriors = children.filter((c) => c.clockwise);
    const holes = children.filter((c) => !c.clockwise);
    const claimed = new Set();
    if (!holes.length) {
      (exteriors.length ? exteriors : children).forEach((isl) => {
        isl.remove();
        isl.fillColor = fillColor; isl.strokeColor = strokeColor; isl.strokeWidth = strokeWidth; isl.opacity = opacity;
        inserted.push(isl);
      });
    } else {
      exteriors.forEach((ext) => {
        let extSegs = segsOf(ext);
        const mine = holes.filter((h) => ext.bounds.contains(h.bounds));
        mine.forEach((h) => { claimed.add(h); extSegs = mergeHoleIntoExterior(extSegs, segsOf(h)); });
        const merged = new P.Path({ insert: false });
        extSegs.forEach((s) => {
          merged.add(new P.Segment(new P.Point(s.point[0], s.point[1]), new P.Point(s.handleIn[0], s.handleIn[1]), new P.Point(s.handleOut[0], s.handleOut[1])));
        });
        merged.closed = true;
        merged.fillColor = fillColor; merged.strokeColor = strokeColor; merged.strokeWidth = strokeWidth; merged.opacity = opacity;
        inserted.push(merged);
      });
    }
    return { paths: inserted, unclaimed: holes.filter((h) => !claimed.has(h)).map((h) => children.indexOf(h)) };
  }

  // The members of the baseline path serializer (app.js) an SVG-imported Path can carry: no vector-brush, texture, channel or
  // ownership tags. A pure snapshot; it writes nothing.
  function snapshotPath(p) {
    return {
      segments: p.segments.map((s) => ({ point: [r3(s.point.x), r3(s.point.y)], handleIn: [r3(s.handleIn.x), r3(s.handleIn.y)], handleOut: [r3(s.handleOut.x), r3(s.handleOut.y)] })),
      closed: !!p.closed,
      strokeColor: p.strokeColor ? colorHex8(p.strokeColor) : '#ffffff',
      hasRealStroke: !!p.strokeColor,
      strokeWidth: p.strokeWidth, strokeCap: p.strokeCap || 'round', strokeJoin: p.strokeJoin || 'round', miterLimit: p.miterLimit,
      fillColor: p.fillColor ? colorHex8(p.fillColor) : null,
      opacity: p.opacity !== undefined ? p.opacity : 1,
      dashArray: (p.dashArray && p.dashArray.length) ? p.dashArray.slice() : undefined,
      dashOffset: p.dashOffset,
    };
  }

  // ---- native record (the form the accepted L10-H transaction stores for what the baseline serializer persisted) ----
  class Refusal extends Error { constructor(reason, detail) { super(reason); this.reason = reason; this.detail = detail || {}; } }
  function solid(token, field) {
    if (typeof token !== 'string' || !HEX.test(token)) throw new Refusal('unsupported_paint', { field, token: String(token) });
    const byte = (i) => parseInt(token.slice(1 + 2 * i, 3 + 2 * i), 16) / 255;
    return { kind: 'solid', r: byte(0), g: byte(1), b: byte(2), a: token.length === 9 ? byte(3) : 1 };
  }
  function num(value, field) { if (typeof value !== 'number' || !Number.isFinite(value)) throw new Refusal('non_finite', { field }); return value; }
  function toRecord(s, target) {
    if (s.segments.length < MIN_ANCHORS || s.segments.length > MAX_ANCHORS) throw new Refusal('segment_count', { count: s.segments.length });
    const anchor = (p) => ({ x: p[0], y: p[1] });
    const vector = { stroke: s.hasRealStroke ? solid(s.strokeColor, 'stroke') : null, hasRealStroke: s.hasRealStroke };
    vector.strokeWidth = num(s.strokeWidth, 'strokeWidth');
    vector.strokeCap = s.strokeCap; vector.strokeJoin = s.strokeJoin;
    if (s.miterLimit !== undefined && s.miterLimit !== null) vector.miterLimit = num(s.miterLimit, 'miterLimit');
    if (s.dashArray) vector.dashArray = s.dashArray.map((d) => num(d, 'dashArray'));
    if (s.dashOffset !== undefined && s.dashOffset !== null) vector.dashOffset = num(s.dashOffset, 'dashOffset');
    vector.opacity = num(s.opacity, 'opacity');
    return {
      schemaVersion: 1, family: 'editable-vector-path', target,
      geometry: { kind: 'cubic-path', closed: s.closed, coordinateSpace: 'document-world', handleSpace: 'relative',
        segments: s.segments.map((g) => ({ point: anchor(g.point), handleIn: anchor(g.handleIn), handleOut: anchor(g.handleOut) })) },
      fill: s.fillColor ? solid(s.fillColor, 'fill') : null,
      vector,
    };
  }

  // ---- parse -> bake -> fit ----
  function parse(P, text, opts, canvas) {
    let root;
    try { root = P.project.importSVG(text, { insert: false, expandShapes: true }); } catch (_) { return { status: 'parse_failed' }; }
    if (!root) return { status: 'parse_failed' };
    const found = collectLeaves(P, root);
    if (!found.leaves.length) { root.remove(); return { status: 'empty', skipped: found.skipped }; }
    // Bake each leaf's accumulated parent transform while the tree (and its `.parent` chain) is intact, THEN detach the import root.
    const baked = found.leaves.map((leaf) => {
      const clone = leaf.item.clone({ insert: false });
      clone.transform(worldMatrix(P, leaf.item));
      return clone;
    });
    root.remove();
    let b = baked[0].bounds.clone();
    for (let i = 1; i < baked.length; i++) b = b.unite(baked[i].bounds);
    const s = opts.noFit ? 1 : ((b.width > 0 && b.height > 0) ? Math.min(1, canvas.w / b.width, canvas.h / b.height) : 1);
    const targetCx = canvas.w / 2, targetCy = canvas.h / 2;
    baked.forEach((c) => {
      if (opts.noFit) return; // coordinates are already final absolute canvas positions
      c.scale(s, b.center);
      c.translate(targetCx - b.center.x, targetCy - b.center.y);
    });
    return { status: 'parsed', leaves: found.leaves.map((leaf, i) => ({ index: leaf.index, baked: baked[i] })), skipped: found.skipped };
  }

  // One parsed file as records, unclaimed-hole skips and element skips (source ids `<prefix>:<node>:<ordinal>`, `<prefix>:<node>:hole:<child>`
  // and `<prefix>:<node>`). Throws a Refusal naming the path that cannot become a record.
  function explode(P, parsed, prefix, layerRef, frame) {
    const records = [], skipped = [];
    parsed.leaves.forEach((leaf) => {
      const out = explodeLeaf(P, leaf.baked);
      out.paths.forEach((path, ordinal) => {
        try {
          records.push({ sourceId: `${prefix}:${leaf.index}:${ordinal}`,
            record: toRecord(snapshotPath(path), { contextId: 'scene-root', frameScope: { kind: 'authored', frame }, layerUid: layerRef, strokeId: '' }) });
        } catch (error) {
          // A paint Paper cannot print as #rrggbb[aa] (a gradient or pattern color throws or prints something else) is refused, never guessed.
          const refusal = error instanceof Refusal ? error : new Refusal('unsupported_paint', { message: String(error && error.message || error) });
          refusal.detail.sourceId = `${prefix}:${leaf.index}:${ordinal}`;
          throw refusal;
        }
      });
      out.unclaimed.forEach((child) => skipped.push({ kind: 'element', sourceId: `${prefix}:${leaf.index}:hole:${child}`, reason: 'unclaimed-hole' }));
    });
    parsed.skipped.forEach((row) => skipped.push({ kind: 'element', sourceId: `${prefix}:${row.index}`, reason: `unsupported-element:${row.className}` }));
    return { records, skipped };
  }

  function isSvgFile(f) { return !!f && (f.type === 'image/svg+xml' || /\.svg$/i.test(f.name || '')); }
  function looksLikeSvg(t) { return typeof t === 'string' && /<svg[\s>]/i.test(t); }
  function message(t, n, classes) {
    let msg = t('toastSvgImported').replace('{n}', n);
    if (classes.length) msg += ' — ' + t('toastSvgSkipped').replace('{n}', classes.length) + ' (' + classes.join(', ') + ')';
    return msg;
  }

  function create(ports) {
    const P = ports.paper, core = ports.core, t = ports.t, notify = ports.notify || function () {};
    // items: [{text}]; one atomic batch for all of them. A file that fails to parse or is empty reports its baseline toast and adds nothing.
    async function importTexts(items, opts) {
      opts = opts || {};
      const begun = core.begin();
      if (!begun.ok) return { status: begun.status, reason: begun.reason, items: [] };
      const ctx = begun.ctx, canvas = { w: ctx.before.canvasW, h: ctx.before.canvasH };
      const rows = items.map((item, k) => {
        const parsed = parse(P, item.text, opts, canvas);
        if (parsed.status === 'parse_failed') { notify(t('toastSvgImportFailed')); return { index: k, outcome: 'parse_failed' }; }
        if (parsed.status === 'empty') { notify(t('toastSvgImportEmpty')); return { index: k, outcome: 'empty', skippedClasses: parsed.skipped.map((s) => s.className) }; }
        return { index: k, outcome: 'parsed', parsed };
      });
      const live = rows.filter((r) => r.outcome === 'parsed');
      if (!live.length) return { status: 'nothing', items: rows.map(pub) };
      // Baseline layer check: a missing or locked active layer refuses (once per import, after parse and empty).
      const uid = ports.activeLayerUid();
      const layer = ctx.before.layers.find((row) => row.layerUid === uid);
      if (!layer || layer.locked === true) {
        live.forEach((r) => { notify(t('toastInvalidLockedLayer')); r.outcome = 'refused_layer'; });
        return { status: 'refused', reason: 'layer', items: rows.map(pub) };
      }
      const batch = { layers: [], records: [], skipped: [], unavailable: [] };
      try {
        live.forEach((r) => {
          const out = explode(P, r.parsed, `svg:${r.index}`, layer.layerUid, ctx.frame);
          r.count = out.records.length; r.skippedClasses = r.parsed.skipped.map((s) => s.className);
          batch.records.push(...out.records); batch.skipped.push(...out.skipped);
        });
      } catch (error) {
        if (error instanceof Refusal) return { status: 'invalid', reason: error.reason, detail: error.detail, items: rows.map(pub) };
        throw error;
      }
      const outcome = await core.submit(ctx, batch);
      if (outcome.status !== 'imported') return Object.assign({ items: rows.map(pub) }, outcome);
      live.forEach((r) => { r.outcome = 'imported'; notify(message(t, r.count, r.skippedClasses)); });
      return Object.assign({ items: rows.map(pub), count: batch.records.length }, outcome);
    }
    const pub = (r) => ({ index: r.index, outcome: r.outcome, count: r.count || 0, skippedClasses: r.skippedClasses || [] });
    async function readAll(files) {
      return Promise.all(files.map(async (file) => ({ name: file.name, text: String(await ports.readText(file)) })));
    }
    return Object.freeze({
      importTexts,
      importText: (text, opts) => importTexts([{ text }], opts),
      importFiles: async (files, opts) => importTexts(await readAll((files || []).filter(isSvgFile)), opts),
      // Menu entry: the picker is a port (desktop dialog + fs read, or a file input); null means cancelled.
      openFile: async (opts) => { const picked = ports.pickFile ? await ports.pickFile() : null; return picked ? importTexts([{ text: picked.text }], opts) : { status: 'cancelled' }; },
    });
  }
  return Object.freeze({ create, parse, explode, explodeLeaf, collectLeaves, snapshotPath, toRecord, colorHex8, r3, isSvgFile, looksLikeSvg, message, Refusal, mergeHoleIntoExterior, polyClosestPair });
}));
