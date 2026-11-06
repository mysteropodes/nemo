/* The multi-layer box of Motion on a native object project (#2520/#2525, C-motion-canvas-4): motion.js@1065e9d8 multiLayerBox / multiLayerOverlay and the
 * multiLayerMove / multiLayerScale / multiLayerRotate drags of onDown / onDrag. Two or more selected layers (the timeline row selection, `_layerSel`) are
 * drawn as ONE box: the union of every visible, unlocked, non-3D layer's eight handle points, with a corner square at each corner and a Rotate ring about the
 * union centre. Inside the box a drag moves every layer by the same world delta (Shift locks the axis); a corner scales every layer by the same ratio about the
 * union centre (never below 1 %), and the ring rotates every layer by the same angle; Scale and Rotate also move each layer's Position so its centre follows
 * the union (the baseline desired-minus-current correction). Facts come from the same bounded reads as the single-layer gizmo (`layerBounds`, the raw Position /
 * Anchor / Rotation / Scale at the playhead); the values are recomputed from the values the gesture began with and written as ONE `object.property.batch`
 * (one revision, one history entry) on release: the click threshold, Escape, a frame or document change and a refused entry commit nothing. The pointer
 * listeners stay on `document` in the capture phase (CLAUDE.md section 13). This file never reads or writes a JS layer record. */
(function (root, factory) {
  const CJS = typeof module === 'object' && module.exports;
  const api = factory(CJS ? require('./native-motion-canvas-math.js') : root.NemoNativeMotionCanvasMath);
  if (CJS) module.exports = api;
  else {
    root.NemoNativeMotionMulti = api;
    if (root.document && typeof root.document.addEventListener === 'function') { try { api.desktop(root).install(); } catch (_) { /* nothing to listen on */ } }
  }
}(typeof globalThis === 'object' ? globalThis : this, function (Math2) {
  'use strict';
  const SVG = 'http://www.w3.org/2000/svg', PIN = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  const BLUE = 'rgba(74,158,255,.86)', RING = 'rgba(74,158,255,.67)', WHITE = 'rgba(255,255,255,1)', ACCENT = 'rgba(255,184,108,1)';
  const THRESHOLD = 3, MAX_EDITS = 64, MAX_LAYERS = 64, round = (v) => Math.round(v * 1e6) / 1e6, finite = (v) => Number.isFinite(v);
  const TOO_MANY = 'Too many layers are selected to be moved together. Select fewer layers.';
  const CORNERS = ['nw', 'ne', 'se', 'sw'];

  const centroid = (l) => ({ x: CORNERS.reduce((s, k) => s + l.corners[k].x, 0) / 4, y: CORNERS.reduce((s, k) => s + l.corners[k].y, 0) / 4 });
  // The union box of the laid-out layers: the min / max of every handle point, its centre as the pivot and the baseline ring radius.
  function union(layouts, pxPerUnit) {
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    layouts.forEach((l) => Object.keys(l.corners).forEach((k) => {
      const c = l.corners[k]; x0 = Math.min(x0, c.x); y0 = Math.min(y0, c.y); x1 = Math.max(x1, c.x); y1 = Math.max(y1, c.y);
    }));
    if (![x0, y0, x1, y1].every(finite)) return null;
    const w = x1 - x0, h = y1 - y0;
    return { left: x0, top: y0, right: x1, bottom: y1, width: w, height: h, pivot: { x: (x0 + x1) / 2, y: (y0 + y1) / 2 }, ringRadius: Math.min(36 / pxPerUnit, Math.max(w, h) * 0.3) };
  }
  // The values a drag means for every layer, from the values it began with (`items[i].g`), the world point it began at (`from`) and the pointer now (`to`).
  // Returns [{ item, position, scale?, rotation? }]: Position is corrected so the layer's centre follows the union exactly as the baseline does.
  function solve(mode, items, box, from, to, o) {
    const ppu = o.pxPerUnit;
    return items.map((item) => {
      const g = item.g, l = Math2.layout(g, ppu), out = { item, position: g.position.slice() };
      if (mode === 'move') {
        const d = l.ovec(...Math2.lockAxis(o.shift, to.x - from.x, to.y - from.y));
        out.position = [g.position[0] + d[0], g.position[1] + d[1]];
        return out;
      }
      const centre = centroid(l);
      let desired;
      if (mode === 'scale') {
        const ratio = Math.max(0.01, Math.hypot(to.x - box.pivot.x, to.y - box.pivot.y) / Math.max(1e-6, Math.hypot(from.x - box.pivot.x, from.y - box.pivot.y)));
        out.scale = [g.scale[0] * ratio, g.scale[1] * ratio];
        desired = { x: box.pivot.x + (centre.x - box.pivot.x) * ratio, y: box.pivot.y + (centre.y - box.pivot.y) * ratio };
      } else {
        const deg = (Math.atan2(to.y - box.pivot.y, to.x - box.pivot.x) - Math.atan2(from.y - box.pivot.y, from.x - box.pivot.x)) * 180 / Math.PI;
        const rad = deg * Math.PI / 180, c = Math.cos(rad), s = Math.sin(rad), dx = centre.x - box.pivot.x, dy = centre.y - box.pivot.y;
        out.rotation = g.rotation + deg;
        desired = { x: box.pivot.x + dx * c - dy * s, y: box.pivot.y + dx * s + dy * c };
      }
      const turned = Math2.layout(Object.assign({}, g, out.scale ? { scale: out.scale } : { rotation: out.rotation }), ppu), cur = centroid(turned);
      const d = turned.ovec(desired.x - cur.x, desired.y - cur.y);
      out.position = [g.position[0] + d[0], g.position[1] + d[1]];
      return out;
    });
  }
  // The batch entries of a solved gesture: only values that actually change, Position first (the baseline order in a tick).
  function entries(solved, contextId, frame) {
    const out = [], same = (a, b) => a.length === b.length && a.every((v, i) => round(v) === round(b[i]));
    solved.forEach((s) => {
      const target = { contextId, layerUid: s.item.layerUid }, g = s.item.g;
      const add = (property, values, was) => { if (!values.every(finite)) return; if (!same(values, was)) out.push({ target, property, action: { kind: 'setValue', frame, values: values.map(round) } }); };
      add('position', s.position, g.position);
      if (s.scale) add('scale', s.scale, g.scale);
      if (s.rotation !== undefined) add('rotation', [s.rotation], [g.rotation]);
    });
    return out;
  }

  function create(win) {
    let cached = null, serial = 0, drag = null, busy = false, overlay = null, queued = false, signature = '';
    const widgets = () => win.NemoNativeObjectWidgetUI;
    const toast = (m) => { try { if (win.showToast) win.showToast(m, 'warn'); } catch (_) { /* presentation only */ } };
    const motion = () => !!win.state && win.state.appMode === 'motion' && win.state.playing !== true;
    const active = () => { const W = widgets(); try { return !!W && W.active(win); } catch (_) { return false; } };
    function heldNow() {
      try { const c = win.NemoNativeComponentContext, p = c && typeof c.presented === 'function' ? c.presented(win) : null, rows = p && c.layers(win); return rows ? { contextId: p.contextId, rows } : null; } catch (_) { return null; }
    }
    // The selected layers the box covers: visible, unlocked, not 3D, in the root context or the entered component, at least two of them.
    function rowsOf(proof) {
      const st = win.state, sel = win._layerSel, held = heldNow();
      if (!motion() || (st.activeSymbolId && !held) || !Array.isArray(sel) || sel.length < 2 || sel.length > MAX_LAYERS) return null;
      const layers = held ? held.rows : proof.info.layers, seen = {}, rows = [];
      sel.forEach((i) => { const r = layers && layers[i]; if (r && !seen[i] && r.visible !== false && r.locked !== true && r.threeD !== true && typeof r.layerUid === 'string') { seen[i] = true; rows.push(Object.assign({ idx: i }, r)); } });
      return rows.length >= 2 ? rows : null;
    }
    const selector = (info, view) => ({ instanceId: info.instanceId, documentId: info.documentId, atRevision: info.contentRevision, documentSnapshotId: info.documentSnapshotId, view });
    async function loadOne(proof, row) {
      const s = proof.session, frame = proof.selector.frame, target = { contextId: proof.selector.contextId, layerUid: row.layerUid };
      const one = (view) => s.readProperty(selector(proof.info, view)), raw = (property) => one({ kind: 'rawSample', target, property, frame }).then((r) => r.value);
      try {
        const geo = await one({ kind: 'layerBounds', target, frame });
        const [position, anchor, rotation, scale] = await Promise.all([raw('position'), raw('anchor'), raw('rotation'), raw('scale')]);
        return { layerUid: row.layerUid, g: { bounds: geo.bounds, position, anchor, rotation: rotation[0], scale, outer: geo.parentAffine } };
      } catch (_) { return null; } // a layer the native read cannot box takes no part, as the baseline skips a layer without bounds
    }
    const keyOf = (proof, rows) => JSON.stringify([PIN.map((k) => proof.info[k]), proof.selector.contextId, proof.selector.frame, rows.map((r) => r.layerUid)]);
    function refresh() {
      const W = widgets(), held = heldNow(), proof = W && active() && motion() ? W.proofNow(win, 'readProperty', held ? held.contextId : undefined) : null;
      const rows = proof && rowsOf(proof);
      if (!rows) { if (cached) { serial++; cached = null; cancel(); } paint(); return Promise.resolve(null); }
      const key = keyOf(proof, rows);
      if (cached && cached.key === key) { cached.proof = proof; paint(); return cached.promise || Promise.resolve(cached.items); }
      cancel(); serial++;
      const token = serial, entry = { key, proof, items: null, promise: null };
      cached = entry;
      entry.promise = Promise.all(rows.map((r) => loadOne(proof, r))).then((all) => {
        const items = all.filter(Boolean);
        if (token === serial && cached === entry) { entry.items = items.length >= 2 ? items : null; paint(); }
        return token === serial ? entry.items : null;
      }).then((r) => { if (cached === entry) entry.promise = null; return r; });
      return entry.promise;
    }
    const usable = () => (cached && cached.items && cached.proof.current() ? cached : null);
    const mapping = (entry) => { const W = widgets(); try { return W.mappingFor(win, { proof: entry.proof }); } catch (_) { return null; } };
    const client = (map) => (p) => map.api.screen({ x: p.x, y: p.y }, map);
    function pxPerUnit(map) {
      const a = map.api.screen({ x: 0, y: 0 }, map), b = map.api.screen({ x: 1, y: 0 }, map), d = Math.hypot(b.x - a.x, b.y - a.y);
      return d > 0 && Number.isFinite(d) ? d : 1;
    }
    // The layouts as they are now, or as the previewed gesture leaves them.
    function layoutsOf(entry, ppu, preview) {
      const list = entry.items.map((item, i) => {
        const p = preview && preview[i], g = p ? Object.assign({}, item.g, { position: p.position }, p.scale ? { scale: p.scale } : {}, p.rotation !== undefined ? { rotation: p.rotation } : {}) : item.g;
        return Math2.layout(g, ppu);
      });
      return list.every(Boolean) ? list : null;
    }

    // ---- overlay (presentation only) ----
    const node = (name, attrs) => { const n = win.document.createElementNS(SVG, name); Object.keys(attrs).forEach((k) => n.setAttribute(k, attrs[k])); return n; };
    function ensureOverlay() {
      if (overlay && overlay.isConnected !== false && overlay.parentNode) return overlay;
      overlay = node('svg', { 'aria-hidden': 'true', 'data-native-motion-multi': '' });
      Object.assign(overlay.style, { position: 'fixed', left: '0', top: '0', width: '100vw', height: '100vh', overflow: 'visible', pointerEvents: 'none', zIndex: '899' });
      (win.document.body || win.document.documentElement).appendChild(overlay);
      return overlay;
    }
    const pt = (p) => p.x + ',' + p.y;
    function paint() {
      if (queued) return;
      queued = true;
      (win.requestAnimationFrame || ((fn) => fn()))(() => { queued = false; draw(); });
    }
    function draw() {
      const entry = usable(), map = entry && mapping(entry), had = overlay, svg = ensureOverlay();
      if (svg !== had) signature = '';
      const sig = entry && map ? JSON.stringify([entry.key, drag && drag.values, map.api.screen({ x: 0, y: 0 }, map), map.api.screen({ x: 1, y: 0 }, map), map.api.screen({ x: 0, y: 1 }, map)]) : '';
      if (sig === signature && sig) return;
      signature = sig;
      while (svg.firstChild) svg.removeChild(svg.firstChild);
      if (!entry || !map) return;
      const ppu = pxPerUnit(map), ls = layoutsOf(entry, ppu, drag && drag.values), box = ls && union(ls, ppu);
      if (!box) return;
      const c = client(map), color = drag ? ACCENT : BLUE;
      const q = [[box.left, box.top], [box.right, box.top], [box.right, box.bottom], [box.left, box.bottom]].map((p) => c({ x: p[0], y: p[1] }));
      svg.appendChild(node('polygon', { points: q.map(pt).join(' '), fill: 'none', stroke: color, 'stroke-width': 1.2 }));
      q.forEach((p) => svg.appendChild(node('rect', { x: p.x - 3.8, y: p.y - 3.8, width: 7.6, height: 7.6, fill: WHITE, stroke: color, 'stroke-width': 1.2 })));
      const ring = c(box.pivot);
      svg.appendChild(node('circle', { cx: ring.x, cy: ring.y, r: box.ringRadius * ppu, fill: 'none', stroke: RING, 'stroke-width': 1.1 }));
    }
    // ---- gestures ----
    const areaOf = (e) => { const area = win.document.getElementById('canvas-area'); return area && e.target && area.contains(e.target) ? area : null; };
    const consume = (e) => { e.stopImmediatePropagation(); e.preventDefault(); };
    function worldOf(e, map) { try { const w = map.api.point(e, map); return finite(w.x) && finite(w.y) ? w : null; } catch (_) { return null; } }
    function cancel() { if (drag) { drag = null; signature = ''; paint(); } }
    // The baseline order: the Rotate ring (a band about the circumference), then the corner squares, then anywhere inside the union box.
    function hitMode(box, world, ppu) {
      const tol = 1 / ppu;
      if (Math.abs(Math.hypot(world.x - box.pivot.x, world.y - box.pivot.y) - box.ringRadius) < 7 * tol) return 'rotate';
      if ([[box.left, box.top], [box.right, box.top], [box.right, box.bottom], [box.left, box.bottom]].some((p) => Math.hypot(world.x - p[0], world.y - p[1]) < 9 * tol)) return 'scale';
      return world.x >= box.left && world.x <= box.right && world.y >= box.top && world.y <= box.bottom ? 'move' : null;
    }
    function onDown(e) {
      const st = win.state;
      if (drag || busy || !motion() || !active() || (e.button !== undefined && e.button !== 0) || st.spaceDown || st.isPanning || !areaOf(e)) return;
      const entry = usable(), map = entry && mapping(entry), world = map && worldOf(e, map);
      if (!entry || !map || !world) return;
      const ppu = pxPerUnit(map), ls = layoutsOf(entry, ppu), box = ls && union(ls, ppu), mode = box && hitMode(box, world, ppu);
      if (!mode) return;
      consume(e);
      drag = { entry, map, mode, box, ppu, from: world, pointerId: e.pointerId, x: e.clientX, y: e.clientY, armed: false, values: null };
    }
    function onMove(e) {
      if (!drag || (e.pointerId !== undefined && drag.pointerId !== undefined && e.pointerId !== drag.pointerId)) return;
      consume(e);
      if (!drag.entry.proof.current() || cached !== drag.entry) { cancel(); return; }
      if (!drag.armed && Math.hypot(e.clientX - drag.x, e.clientY - drag.y) < THRESHOLD) return;
      drag.armed = true;
      const to = worldOf(e, drag.map);
      if (!to) return;
      drag.values = solve(drag.mode, drag.entry.items.map((i) => i), drag.box, drag.from, to, { shift: !!e.shiftKey, pxPerUnit: drag.ppu });
      paint();
    }
    function proven(entry) {
      const P = win.NemoNativePresentationWait, W = widgets();
      if (entry.proof.current(true)) return Promise.resolve(true);
      if (!P) return Promise.resolve(false);
      return P.await(win, { read: (f) => { const s = W.session(win); return s ? s.readPresentedObjectSession(f) : null; }, notice: 'The canvas is still busy - try the gesture again.',
        holds: () => entry.proof.current() }).then((r) => r.ready && entry.proof.current(true));
    }
    async function write(done) {
      const { entry, values } = done, proof = entry.proof, pin = {};
      PIN.forEach((k) => { pin[k] = proof.info[k]; });
      const edits = entries(values, proof.selector.contextId, proof.selector.frame);
      if (!edits.length) return false;
      if (edits.length > MAX_EDITS) { toast(TOO_MANY); return false; }
      busy = true;
      try {
        if (!(await proven(entry))) return false;
        if (typeof proof.session.editPropertyBatch !== 'function') throw new Error('Native property batch is unavailable.');
        const response = await proof.session.editPropertyBatch({ command: 'object.property.batch', edits }, pin, () => proof.current());
        if (!response || response.ok !== true || !response.result || typeof response.result.applied !== 'boolean') throw new Error('Native property edit was not confirmed.');
        return response.result.applied;
      } catch (error) {
        toast((error && error.message || 'Native Motion edit is unavailable.') + (error && error.code ? ' (' + error.code + ')' : ''));
        return false;
      } finally { busy = false; signature = ''; refresh(); }
    }
    function onUp(e) {
      if (!drag || (e.pointerId !== undefined && drag.pointerId !== undefined && e.pointerId !== drag.pointerId)) return;
      consume(e);
      const done = drag; drag = null; signature = ''; paint();
      if (!done.armed || !done.values || !done.entry.proof.current() || cached !== done.entry) return;
      write(done);
    }
    function onKey(e) { if (drag && e.key === 'Escape') { consume(e); cancel(); } }
    function install() {
      const doc = win.document, capture = { capture: true };
      doc.addEventListener('pointerdown', onDown, capture);
      doc.addEventListener('pointermove', (e) => { if (drag) onMove(e); else if (motion() && active()) refresh(); }, capture);
      doc.addEventListener('pointerup', onUp, capture);
      doc.addEventListener('pointercancel', cancel, capture);
      doc.addEventListener('keydown', onKey, capture);
      if (typeof win.setInterval === 'function') win.setInterval(() => { if (motion() && active()) refresh(); else if (cached) refresh(); }, 250);
      return api;
    }
    const api = Object.freeze({ install, refresh, paint, cancel, drawNow: draw, data: () => (cached ? cached.items : null), dragging: () => !!drag });
    return api;
  }
  const controllers = new WeakMap();
  function desktop(win) {
    if (!controllers.has(win)) controllers.set(win, create(win));
    return controllers.get(win);
  }
  return Object.freeze({ create, desktop, union, solve, entries, TOO_MANY });
}));
