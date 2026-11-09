/* The Motion canvas on a native object project (#2520/#2525): the whole-layer gizmo of motion.js@1065e9d8 (box, scale handles, Rotate ring,
 * pivot crosshair, Position path with its key diamonds) and its pointer gestures, without `state.layers`. Facts come from bounded native reads
 * pinned to the presentation the user sees (the layer's own `layerBounds`, the raw Position / Anchor / Rotation / Scale at the playhead and the
 * authored Position keys); the maths is native-motion-canvas-math.js. A drag is a local preview and ONE `object.property.edit` on release
 * (`setValue` at the playhead, `setKey` for a key diamond), so a gesture is one revision and one history entry: the click threshold, Escape,
 * a frame or document change, a locked layer and a replaced owner each commit nothing. Shift locks a move to an axis, Alt+drag moves the anchor
 * (with Rotation or Scale set the edit carries `keepPose`, so Position follows in the same revision), a key diamond is grabbed only on its own frame and
 * a spatial key handle (hIn / hOut) on any frame (`setKeyHandle`). A parented layer is drawn and dragged through its ancestors' `parentAffine`; the
 * Position path is sampled once per frame through each key's easing. Inside an entered component the gizmo follows the component's own layer (the held context id on every read and on the edit); several selected layers draw nothing and take no
 * gesture, except that the effector handles of a duplicator layer (a crosshair and a falloff ring each, grabbed last, even on the layer the duplicator
 * locks) move one effector anchor through ONE `object.duplicator.config` `effectorPos` edit, a static setting and never a key; a layer the native read cannot box (a text layer) says so once in a notice instead of staying silent. The pointer listeners stay on `document` in the capture phase (CLAUDE.md section 13). This file never reads or writes a JS layer record. */
(function (root, factory) {
  const CJS = typeof module === 'object' && module.exports;
  const api = factory(CJS ? require('./native-motion-canvas-math.js') : root.NemoNativeMotionCanvasMath, CJS ? require('./native-motion-effector.js') : root.NemoNativeMotionEffector,
    CJS ? require('./native-motion-gizmo-3d.js') : root.NemoNativeMotionGizmo3d);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else {
    root.NemoNativeMotionCanvas = api;
    if (root.document && typeof root.document.addEventListener === 'function') { try { api.desktop(root).install(); } catch (_) { /* nothing to listen on */ } }
  }
}(typeof globalThis === 'object' ? globalThis : this, function (Math2, Eff, Giz) {
  'use strict';
  const SVG = 'http://www.w3.org/2000/svg', PIN = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  const HANDLE = 'rgba(255,184,108,.9)', BLUE = 'rgba(74,158,255,.86)', RING = 'rgba(74,158,255,.67)', PATH = 'rgba(120,170,255,.8)', WHITE = 'rgba(255,255,255,1)', ACCENT = 'rgba(255,184,108,1)', DOT = 'rgba(230,230,230,1)', CUR = 'rgba(255,170,40,1)';
  const THRESHOLD = 3, MAX_KEY_PAGES = 4, MAX_FRAME_DOTS = 120, round = (v) => Math.round(v * 1e6) / 1e6;
  const UNBOXED = 'The Motion box is not available for this layer on a native project (text layers have none yet). Use the property fields.';

  function create(win) {
    let cached = null, noticed = '', serial = 0, drag = null, busy = false, overlay = null, queued = false, watching = false, signature = '';
    const widgets = () => win.NemoNativeObjectWidgetUI;
    const toast = (m) => { try { if (win.showToast) win.showToast(m, 'warn'); } catch (_) { /* presentation only */ } };
    const motion = () => !!win.state && win.state.appMode === 'motion' && win.state.playing !== true;
    const active = () => { const W = widgets(); try { return !!W && W.active(win); } catch (_) { return false; } };
    function queueBusy() {
      const W = widgets(), P = win.NemoNativePresentationWait;
      try { const s = W && W.session(win); return !!P && !!s && P.busy((f) => s.readPresentedObjectSession(f)); } catch (_) { return false; }
    }
    // #2256: the entered component the workspace controller holds ({contextId, rows}), or null at the scene root.
    function heldNow() {
      try { const c = win.NemoNativeComponentContext, p = c && typeof c.presented === 'function' ? c.presented(win) : null, rows = p && c.layers(win); return rows ? { contextId: p.contextId, rows } : null; } catch (_) { return null; }
    }
    // The layer the gizmo belongs to: exactly one selected, visible layer of the root context, or of the entered component.
    function targetOf(proof) {
      const st = win.state, sel = win._layerSel, held = heldNow();
      if (!motion() || (st.activeSymbolId && !held) || (Array.isArray(sel) && sel.length > 1)) return null;
      const E = win.NemoNativeMotionElement; // a shape targeted inside its entered group (double-click) has its own box (the select overlay), not the layer's gizmo
      if (E && E.isolated(win)) return null;
      const layers = held ? held.rows : proof.info.layers, layer = layers && layers[st.activeLayerIdx];
      return layer && layer.visible !== false && typeof layer.layerUid === 'string' ? layer : null;
    }
    function selector(info, view) {
      return { instanceId: info.instanceId, documentId: info.documentId, atRevision: info.contentRevision, documentSnapshotId: info.documentSnapshotId, view };
    }
    // The effector handles of a duplicator layer, from one descriptor read pinned to the same revision as the gizmo; none when it cannot be proven.
    async function effectorsOf(proof, layer) {
      const C = win.NemoNativeObjectComponent, adapter = layer.hasDuplicator === true && C && typeof C.desktop === 'function' ? C.desktop(win) : null;
      if (!adapter || typeof adapter.read !== 'function') return [];
      const out = await adapter.read({ kind: 'duplicator', layerUid: layer.layerUid });
      const r = out && out.ok === true ? out.result : null;
      return r && r.layerUid === layer.layerUid && r.atRevision === proof.info.contentRevision && r.documentSnapshotId === proof.info.documentSnapshotId ? Eff.handles(r) : [];
    }
    async function load(proof, layer) {
      const s = proof.session, frame = proof.selector.frame, target = { contextId: proof.selector.contextId, layerUid: layer.layerUid };
      const one = (view) => s.readProperty(selector(proof.info, view));
      const raw = (property) => one({ kind: 'rawSample', target, property, frame }).then((r) => r.value);
      const effectors = effectorsOf(proof, layer).catch(() => []);
      let geo, refusal;
      try { geo = await one({ kind: 'layerBounds', target, frame }); } catch (error) {
        refusal = error;
        // A text layer: the native read refuses it, the glyph-run bounds come from the hit-test listing (native-motion-text-bounds.js).
        const T = win.NemoNativeMotionTextBounds, text = error && error.code === 'unavailable' && T ? await T.read(win, proof, layer).catch(() => null) : null;
        if (text) geo = { bounds: text.bounds, parentAffine: text.parentAffine };
      }
      if (!geo) {
        // A duplicator host with no geometry of its own (and no parent whose map it would need) still offers its effector handles.
        const list = refusal && refusal.code === 'unavailable' && layer.hasDuplicator === true && !layer.parentLayerUid ? await effectors : [];
        if (!list.length) throw refusal;
        return { layerUid: layer.layerUid, locked: layer.locked === true, outer: Eff.IDENTITY.slice(), bounds: null, keys: [], effectors: list };
      }
      const [position, anchor, rotation, scale] = await Promise.all([raw('position'), raw('anchor'), raw('rotation'), raw('scale')]);
      const keys = [];
      for (let offset = 0, n = 0; n < MAX_KEY_PAGES; n++) {
        const page = await one({ kind: 'authoredKeys', target, property: 'position', offset, limit: 8 });
        page.items.forEach((k) => keys.push(k));
        if (page.nextOffset === null || page.nextOffset === undefined) break;
        offset = page.nextOffset;
      }
      // A 3D layer (the baseline `ld.threeD`) swaps the box for the 3D gizmo: its three extra values and the canvas size give the pose it is drawn from.
      let pose3d = null;
      if (layer.threeD === true) {
        const [z, rx, ry] = await Promise.all([raw('positionZ'), raw('rotationX'), raw('rotationY')]);
        pose3d = Giz.pose({ bounds: geo.bounds, position, anchor, rotation: rotation[0], positionZ: z[0], rotationX: rx[0], rotationY: ry[0], canvasW: proof.info.canvasW, canvasH: proof.info.canvasH });
      }
      return { layerUid: layer.layerUid, locked: layer.locked === true, outer: geo.parentAffine, bounds: geo.bounds, position, anchor, rotation: rotation[0], scale, keys, pose3d, effectors: await effectors };
    }
    // A typed refusal of the geometry read means this layer has no box to follow: say so once per layer instead of drawing nothing.
    function unboxed(error, layer) {
      if (!error || error.code !== 'unavailable' || noticed === layer.layerUid) return;
      noticed = layer.layerUid; toast(UNBOXED);
    }
    const keyOf = (proof, layer) => JSON.stringify([PIN.map((k) => proof.info[k]), proof.selector.contextId, proof.selector.frame, layer.layerUid]);
    function refresh() {
      const W = widgets(), held = heldNow(), proof = W && active() && motion() ? W.proofNow(win, 'readProperty', held ? held.contextId : undefined) : null;
      if (!proof && queueBusy() && cached) return Promise.resolve(cached.read); // busy is not changed: keep the gizmo and the drag
      const layer = proof && targetOf(proof);
      if (!layer) { noticed = ''; if (cached) { serial++; cached = null; cancel(); } paint(); return Promise.resolve(null); }
      const key = keyOf(proof, layer);
      if (cached && cached.key === key) { cached.proof = proof; paint(); return cached.promise || Promise.resolve(cached.read); }
      cancel(); serial++;
      const token = serial, entry = { key, proof, layer, read: null, promise: null };
      cached = entry;
      entry.promise = load(proof, layer).then((read) => { if (token === serial && cached === entry) { entry.read = read; paint(); } return token === serial ? read : null; },
        (error) => { if (token === serial && cached === entry) { entry.read = null; unboxed(error, layer); paint(); } return null; }).then((r) => { if (cached === entry) entry.promise = null; return r; });
      return entry.promise;
    }
    function watch() {
      if (watching || !win.requestAnimationFrame) return;
      watching = true;
      win.requestAnimationFrame(() => { watching = false; if (!active() || !motion()) { if (cached) refresh(); return; } refresh(); if (cached) watch(); });
    }
    const usable = () => (cached && cached.read && cached.proof.current() ? cached : null);
    const mapping = (entry) => { const W = widgets(); try { return W.mappingFor(win, { proof: entry.proof }); } catch (_) { return null; } };
    const client = (map) => (p) => map.api.screen({ x: p.x, y: p.y }, map);
    function pxPerUnit(map) {
      const a = map.api.screen({ x: 0, y: 0 }, map), b = map.api.screen({ x: 1, y: 0 }, map), d = Math.hypot(b.x - a.x, b.y - a.y);
      return d > 0 && Number.isFinite(d) ? d : 1;
    }
    // The layout of the gizmo as it is now: the read values, with the previewed property of a drag replacing the stored one. A key diamond is
    // grabbed only on the playhead's own frame, where the key IS the Position, so its preview moves the box too.
    function layoutOf(entry, map, preview) {
      const r = entry.read, g = { bounds: r.bounds, position: r.position, anchor: r.anchor, rotation: r.rotation, scale: r.scale, outer: r.outer };
      if (preview && preview.property && !preview.handle) {
        if (preview.keepPose) g.position = Math2.follow(g, preview.values); // the box stays put while the anchor moves, as the engine's Position will
        if (preview.property === 'rotation') g.rotation = preview.values[0]; else g[preview.property] = preview.values;
      }
      return Math2.layout(g, pxPerUnit(map));
    }
    const keysNow = (entry, preview) => entry.read.keys.map((k) => (!preview || preview.frame !== k.frame ? k : Object.assign({}, k, preview.handle ? { [preview.handle]: preview.values } : { v: preview.values })));
    const easing = () => (win.SMAnimationCurve && win.SMAnimationCurve.evalCurvePoints) || null;

    // ---- overlay (presentation only) ----
    const node = (name, attrs) => { const n = win.document.createElementNS(SVG, name); Object.keys(attrs).forEach((k) => n.setAttribute(k, attrs[k])); return n; };
    function ensureOverlay() {
      if (overlay && overlay.isConnected !== false && overlay.parentNode) return overlay;
      overlay = node('svg', { 'aria-hidden': 'true', 'data-native-motion-canvas': '' });
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
      const sig = entry && map ? JSON.stringify([entry.key, drag && drag.preview, map.api.screen({ x: 0, y: 0 }, map), map.api.screen({ x: 1, y: 0 }, map), map.api.screen({ x: 0, y: 1 }, map)]) : '';
      if (sig === signature && sig) return;
      signature = sig;
      while (svg.firstChild) svg.removeChild(svg.firstChild);
      const prev = drag && drag.preview && drag.preview.property ? drag.preview : null;
      const l = entry && map && layoutOf(entry, map, prev);
      const effs = entry && map ? Eff.preview(entry.read.effectors, drag && drag.preview && drag.preview.effector) : [];
      if (!l && !effs.length) return;
      const c = client(map), scale = pxPerUnit(map);
      if (l) drawBox(svg, entry, l, c, scale, prev, !!entry.read.pose3d);
      if (l && entry.read.pose3d) draw3d(svg, entry.read, Giz.preview(entry.read.pose3d, prev), c);
      effs.forEach((h) => drawEffector(svg, Eff.glyph(h, entry.read.outer, c, scale)));
    }
    // The effector handle of motion.js@1065e9d8: a crosshair in a circle, plus a thin ring of the effector's own radius.
    function drawEffector(svg, g) {
      const col = 'rgba(255,120,220,1)';
      svg.appendChild(node('line', { x1: g.c.x - g.arm, y1: g.c.y, x2: g.c.x + g.arm, y2: g.c.y, stroke: col, 'stroke-width': 1.5 }));
      svg.appendChild(node('line', { x1: g.c.x, y1: g.c.y - g.arm, x2: g.c.x, y2: g.c.y + g.arm, stroke: col, 'stroke-width': 1.5 }));
      svg.appendChild(node('circle', { cx: g.c.x, cy: g.c.y, r: g.dot, fill: 'none', stroke: col, 'stroke-width': 1.5 }));
      svg.appendChild(node('circle', { cx: g.c.x, cy: g.c.y, r: g.ring, fill: 'none', stroke: 'rgba(255,120,220,.35)', 'stroke-width': 1 }));
    }
    // The 3D gizmo of motion.js@1065e9d8: three coloured arrows along the layer's local axes with a head each, and three translucent rings.
    function draw3d(svg, read, pose, c) {
      const through = (q) => c(Giz.through(read.outer, q)), arrows = Giz.arrows(pose), rings = Giz.rings(pose), col = (k, a) => 'rgba(' + Giz.COLORS[k].slice(0, 3).concat(a).join(',') + ')';
      Giz.AXES.forEach((k) => {
        const o = through(arrows[k].origin), t = through(arrows[k].tip);
        svg.appendChild(node('line', { x1: o.x, y1: o.y, x2: t.x, y2: t.y, stroke: col(k, 1), 'stroke-width': 2.2 }));
        svg.appendChild(node('polygon', { points: Giz.head(t, o, 9, 4).map(pt).join(' '), fill: col(k, 1), stroke: 'none' }));
      });
      Giz.AXES.forEach((k) => svg.appendChild(node('polygon', { points: rings[k].map((q) => pt(through(q))).join(' '), fill: 'none', stroke: col(k, 0.59), 'stroke-width': 1.4 })));
    }
    function drawBox(svg, entry, l, c, scale, prev, flat) {
      const color = drag ? ACCENT : BLUE;
      const keys = Math2.path(l, keysNow(entry, prev), easing());
      if (keys.dots.length) {
        if (keys.frames.length) { // one thin unbroken line through every frame sample, and a small dot per frame (capped on a very long segment)
          keys.frames.forEach((pts) => {
            svg.appendChild(node('polyline', { points: pts.map((q) => pt(c(q))).join(' '), fill: 'none', stroke: PATH, 'stroke-width': 1 }));
            const stride = Math.max(1, Math.ceil(pts.length / MAX_FRAME_DOTS));
            for (let i = 0; i < pts.length; i += stride) { const q = c(pts[i]); svg.appendChild(node('circle', { cx: q.x, cy: q.y, r: 1.3, fill: PATH })); }
          });
        } else if (keys.segments.length) svg.appendChild(node('path', { d: keys.segments.map((s, i) => (i ? '' : 'M' + pt(c(s.from)) + ' ') + 'C' + pt(c(s.c1)) + ' ' + pt(c(s.c2)) + ' ' + pt(c(s.to))).join(' '), fill: 'none', stroke: PATH, 'stroke-width': 1 }));
        keys.handles.filter((h) => !h.zero).forEach((h) => {
          const a = c(h.from), b = c(h);
          svg.appendChild(node('line', { x1: a.x, y1: a.y, x2: b.x, y2: b.y, stroke: HANDLE, 'stroke-width': 1 }));
          svg.appendChild(node('circle', { cx: b.x, cy: b.y, r: 4, fill: HANDLE, stroke: 'rgba(30,30,30,1)', 'stroke-width': 1 }));
        });
        keys.dots.forEach((d) => {
          const p = c(d), cur = d.key.frame === entry.proof.selector.frame, r = cur ? 6 : 4.5;
          svg.appendChild(node('polygon', { points: [[p.x - r, p.y], [p.x, p.y - r], [p.x + r, p.y], [p.x, p.y + r]].map((q) => q.join(',')).join(' '), fill: cur ? CUR : DOT, stroke: 'rgba(30,30,30,1)', 'stroke-width': 1 }));
        });
      }
      const box = ['nw', 'ne', 'se', 'sw'].map((k) => c(l.corners[k]));
      if (!flat) svg.appendChild(node('polygon', { points: box.map(pt).join(' '), fill: 'none', stroke: color, 'stroke-width': 1.2 })); // the 3D gizmo replaces the box
      if (!l.isNull && !flat) {
        Math2.EDGES.forEach((k) => { const p = c(l.corners[k]), r = k.length === 2 ? 3.8 : 3; svg.appendChild(node('rect', { x: p.x - r, y: p.y - r, width: 2 * r, height: 2 * r, fill: WHITE, stroke: color, 'stroke-width': 1 })); });
        const ring = c(l.pivot);
        svg.appendChild(node('circle', { cx: ring.x, cy: ring.y, r: l.ringRadius * scale, fill: 'none', stroke: RING, 'stroke-width': 1.1 }));
      }
      const a = c(l.pivot);
      svg.appendChild(node('circle', { cx: a.x, cy: a.y, r: 5, fill: 'none', stroke: color, 'stroke-width': 1.4 }));
      [[-8, 0, 8, 0], [0, -8, 0, 8]].forEach((q) => svg.appendChild(node('line', { x1: a.x + q[0], y1: a.y + q[1], x2: a.x + q[2], y2: a.y + q[3], stroke: color, 'stroke-width': 1.2 })));
    }
    // ---- gestures ----
    const areaOf = (e) => { const area = win.document.getElementById('canvas-area'); return area && e.target && area.contains(e.target) ? area : null; };
    const consume = (e) => { e.stopImmediatePropagation(); e.preventDefault(); };
    function worldOf(e, map) { try { const w = map.api.point(e, map); return Number.isFinite(w.x) && Number.isFinite(w.y) ? w : null; } catch (_) { return null; } }
    function cancel() { if (drag) { drag = null; signature = ''; paint(); } }
    function onDown(e) {
      const st = win.state;
      if (drag || busy || !motion() || !active() || (e.button !== undefined && e.button !== 0) || st.spaceDown || st.isPanning || !areaOf(e)) return;
      const entry = usable(), map = entry && mapping(entry);
      if (!entry || !map) return;
      const c = client(map), world = worldOf(e, map), pointer = { x: e.clientX, y: e.clientY };
      if (!world) return;
      // The box gestures first (never on a locked layer), the effector handles last: a duplicator layer is locked by the duplicator itself.
      const l = entry.read.bounds && !entry.read.locked ? layoutOf(entry, map) : null;
      let found = null;
      const probe = (body) => {
        const grabs = Math2.path(l, entry.read.keys);
        return Math2.hit(l, pointer, { client: c, pxPerUnit: pxPerUnit(map), alt: !!e.altKey, body, dots: grabs.dots, handles: grabs.handles, frame: entry.proof.selector.frame, noBox: !!entry.read.pose3d });
      };
      // The 3D arrows and rings are the precise grab of a 3D layer and win over its key dots and anchor, as the baseline orders them.
      const g3 = l && entry.read.pose3d ? Giz.hit(entry.read.pose3d, entry.read.outer, c, pointer) : null;
      if (g3) found = Giz.begin(g3, entry.read.pose3d, entry.read.outer, world);
      if (!found && l) found = probe(false);
      if (!found) found = Eff.hit(entry.read.effectors, entry.read.outer, c, pointer);
      if (!found && l && st.tool === 'select') found = probe(true); // the body of the box is the last grab, as the layer move of select-bridge is
      if (!found) return;
      consume(e);
      drag = { entry, map, mode: found, layout: l, from: world, pointerId: e.pointerId, x: e.clientX, y: e.clientY, armed: false, preview: null };
    }
    function onMove(e) {
      if (!drag || (e.pointerId !== undefined && drag.pointerId !== undefined && e.pointerId !== drag.pointerId)) return;
      consume(e);
      if (!drag.entry.proof.current() || cached !== drag.entry) { cancel(); return; }
      if (!drag.armed && Math.hypot(e.clientX - drag.x, e.clientY - drag.y) < THRESHOLD) return;
      drag.armed = true;
      const to = worldOf(e, drag.map);
      if (!to) return;
      if (drag.mode.type === 'effector') { // a static setting: the preview is the anchor the release would store, nothing when it is where it was
        const moved = Eff.move(drag.mode.index, drag.entry.read.outer, to), was = drag.entry.read.effectors.find((h) => h.index === drag.mode.index);
        drag.preview = moved && was && (moved.x !== was.pos.x || moved.y !== was.pos.y) ? { effector: moved } : null;
        paint();
        return;
      }
      const out = drag.mode.type === 'axis3d' || drag.mode.type === 'ring3d' ? Giz.drag(drag.mode, to) : Math2.drag(drag.mode, drag.layout, drag.from, to, { shift: !!e.shiftKey });
      drag.preview = out && out.values ? out : null;
      paint();
    }
    function proven(entry) {
      const P = win.NemoNativePresentationWait, W = widgets();
      if (entry.proof.current(true)) return Promise.resolve(true);
      if (!P) return Promise.resolve(false);
      return P.await(win, { read: (f) => { const s = W.session(win); return s ? s.readPresentedObjectSession(f) : null; }, notice: 'The canvas is still busy - try the gesture again.',
        holds: () => entry.proof.current() }).then((r) => r.ready && entry.proof.current(true));
    }
    // ONE `object.duplicator.config` edit stores the new anchor of one inline effector: a revision and a history entry, no key, no Motion property.
    async function writeEffector(done) {
      const { entry, preview } = done;
      busy = true;
      try {
        if (!(await proven(entry))) return false;
        const D = win.NemoNativeObjectDuplicator, adapter = D && typeof D.desktop === 'function' ? D.desktop(win) : null;
        if (!adapter) throw new Error('Native duplicator is unavailable.');
        const out = await adapter.configure(entry.layer.layerUid, { effectorPos: preview.effector });
        if (!out || out.ok !== true) throw Object.assign(new Error(out && out.message ? out.message : 'Duplicator refused.'), { code: out && out.code });
        if (typeof win.nativeCompositingRefresh === 'function') win.nativeCompositingRefresh();
        return true;
      } catch (error) {
        toast((error && error.message || 'Native duplicator edit is unavailable.') + (error && error.code ? ' (' + error.code + ')' : ''));
        return false;
      } finally { busy = false; signature = ''; refresh(); }
    }
    async function write(done) {
      if (done.preview.effector) return writeEffector(done);
      const { entry, preview } = done, proof = entry.proof, pin = {};
      PIN.forEach((k) => { pin[k] = proof.info[k]; });
      const keyed = preview.frame !== undefined, values = preview.values.map(round);
      // A handle drag edits one handle of the key at its own frame; a key diamond is a setKey; the rest write at the playhead.
      const action = preview.handle ? { kind: 'setKeyHandle', frame: preview.frame, handle: preview.handle, values }
        : { kind: keyed ? 'setKey' : 'setValue', frame: keyed ? preview.frame : proof.selector.frame, values };
      if (preview.keepPose) action.keepPose = true;
      const payload = { command: 'object.property.edit', target: { contextId: proof.selector.contextId, layerUid: entry.layer.layerUid }, property: preview.property, action };
      busy = true;
      try {
        if (!(await proven(entry))) return false;
        // The strict proof was taken above; inside the queue the presentation is hidden by design, so the fence is the non-strict one.
        const response = await proof.session.editProperty(payload, pin, () => proof.current());
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
      if (!done.armed || !done.preview || !done.entry.proof.current() || cached !== done.entry) return;
      write(done);
    }
    function onKey(e) { if (drag && e.key === 'Escape') { consume(e); cancel(); } }
    function install() {
      const doc = win.document, capture = { capture: true };
      doc.addEventListener('pointerdown', onDown, capture);
      doc.addEventListener('pointermove', (e) => { if (drag) onMove(e); else if (!watching && motion() && active()) { refresh(); watch(); } }, capture);
      doc.addEventListener('pointerup', onUp, capture);
      doc.addEventListener('pointercancel', cancel, capture);
      doc.addEventListener('keydown', onKey, capture);
      if (typeof win.setInterval === 'function') win.setInterval(() => { if (motion() && active()) { refresh(); watch(); } else if (cached) refresh(); }, 250);
      return api;
    }
    const api = Object.freeze({ install, refresh, paint, cancel, drawNow: draw, data: () => (cached ? cached.read : null), dragging: () => !!drag });
    return api;
  }
  const controllers = new WeakMap();
  function desktop(win) {
    if (!controllers.has(win)) controllers.set(win, create(win));
    return controllers.get(win);
  }
  return Object.freeze({ create, desktop, UNBOXED });
}));
