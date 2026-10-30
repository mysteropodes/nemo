/* Joystick / slider widgets on a native object project (#2255): the ordinary pad, puck and settings of rig-widget.js without
 * `state.layers`. Facts come from ONE bounded native read (`readWidgets`, the widget view of query.document.object: the composed pad,
 * the puck and rest offsets, the axes) pinned to the actual presentation; every change is one native command through
 * NemoNativeObjectWidget (create / write / settings). The drag is a local preview and ONE `widget.write` on release, so a gesture is
 * one history entry. This file never reads or writes a JS layer record; what has no native route (moving the whole pad) is a typed
 * unavailable. The pointer listeners stay on `document` in the capture phase (CLAUDE.md section 13: motion.js owns a capture
 * listener on #canvas-area and loads first), and each handler stops the event only when a pad is actually under the pointer. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectWidgetUI = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const SVG = 'http://www.w3.org/2000/svg';
  const IDLE = 'rgba(74,158,255,1)', ACCENT = 'rgba(255,184,108,1)', WHITE = 'rgba(255,255,255,1)';
  const TRACK = 'rgba(120,170,255,.67)', GHOST = 'rgba(120,170,255,.35)';
  const PIN = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  const MOVE_REASON = 'widget_move_unavailable';
  const samePin = (a, b) => !!a && !!b && PIN.every((k) => a[k] === b[k]);
  const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));

  function session(win) {
    try {
      const module = win.NemoNativeObjectProject, s = module && typeof module.desktop === 'function' ? module.desktop(win) : null;
      return s && s.blocksLegacy() ? s : null;
    } catch (_) { return null; }
  }
  function active(win) { return !!session(win); }
  function toast(win, message) { try { if (win.showToast) win.showToast(message, 'warn'); } catch (_) { /* presentation only */ } }
  function failure(win, error) {
    toast(win, (error && error.message || 'Native widget is unavailable.') + (error && error.code ? ' (' + error.code + (error.reason ? ': ' + error.reason : '') + ')' : ''));
  }
  // The proof that the widgets are read from the presentation the user is looking at, at the playhead.
  function proofNow(win) {
    const s = session(win);
    if (!s || !s.isActive() || typeof s.readWidgets !== 'function' || !win.state || win.state.playing) return null;
    try {
      const info = s.info(), shown = s.readPresentedObjectSession(), p = shown && shown.presentation;
      if (!info || !p || shown.available !== true || p.owner !== 'native' || p.status !== 'presented' || p.contextId !== 'scene-root' ||
        p.frame !== win.state.currentFrame || !samePin(p, info) || !samePin(shown, info)) return null;
      const ticket = s.readSubscriptionGeneration();
      const current = () => {
        try {
          const live = session(win), now = live && live.readPresentedObjectSession();
          return live === s && live.isActive() && !win.state.playing && win.state.currentFrame === p.frame && samePin(live.info(), info) &&
            live.readSubscriptionGeneration() === ticket && !!now && now.presentation === p;
        } catch (_) { return false; }
      };
      return { session: s, info, presentation: p, selector: { contextId: 'scene-root', frame: p.frame }, current };
    } catch (_) { return null; }
  }
  function mappingFor(win, entry) {
    const m = win.NemoNativeViewportUnderlay && win.NemoNativeViewportUnderlay.canvasMapping;
    if (!m || !m.bounds || !m.point || !m.screen || !m.matchesView || !m.matchesView(win, entry.proof.presentation)) return null;
    try {
      const bounds = m.bounds(win, entry.proof.info);
      return bounds ? { api: m, bounds, presentation: entry.proof.presentation, canvasW: entry.proof.info.canvasW, canvasH: entry.proof.info.canvasH } : null;
    } catch (_) { return null; }
  }
  // Pad geometry, in world units. `item.angle` is degrees; local offsets are pad-local with Y down.
  function toWorld(item, lx, ly) {
    const a = item.angle * Math.PI / 180, c = Math.cos(a), s = Math.sin(a);
    return { x: item.center[0] + lx * c - ly * s, y: item.center[1] + lx * s + ly * c };
  }
  function toLocal(item, wx, wy) {
    const a = item.angle * Math.PI / 180, c = Math.cos(a), s = Math.sin(a), dx = wx - item.center[0], dy = wy - item.center[1];
    return [dx * c + dy * s, -dx * s + dy * c];
  }
  function unit(map) {
    const a = map.api.screen({ x: 0, y: 0 }, map), b = map.api.screen({ x: 1, y: 0 }, map), d = Math.hypot(b.x - a.x, b.y - a.y);
    return d > 0 && Number.isFinite(d) ? d : 1;
  }
  const puckRadius = (item, scale) => Math.max(6 / scale, Math.min(item.size[0], item.size[1]) * 0.12);

  function create(win) {
    let cached = null, serial = 0, queue = Promise.resolve(), overlay = null, paintQueued = false, drag = null, busy = false;
    const keyOf = (proof) => JSON.stringify([proof.info.instanceId, proof.info.documentId, proof.info.lifecycleGeneration,
      proof.info.contentRevision, proof.info.documentSnapshotId, proof.selector.frame]);
    const usable = () => (cached && cached.read && cached.proof.current() ? cached : null);

    function refresh() {
      const proof = proofNow(win), prior = cached;
      if (!proof) { if (cached) { serial++; cached = null; cancel(); } paint(); return Promise.resolve(null); }
      const key = keyOf(proof);
      if (prior && prior.key === key && prior.proof.current()) { prior.proof = proof; paint(); return prior.promise || Promise.resolve(prior.read); }
      cancel(); serial++;
      const token = serial, entry = { proof, key, read: null, promise: null };
      cached = entry;
      entry.promise = queue.catch(() => {}).then(() => (token === serial && proof.current() ? proof.session.readWidgets(proof.selector, proof.info) : null))
        .then((read) => {
          if (token !== serial || cached !== entry || !read || !samePin(read.info, proof.info) || !proof.current()) return null;
          entry.read = read; paint(); return read;
        }).catch((error) => { if (token === serial && cached === entry) { entry.error = error; paint(); } return null; })
        .finally(() => { if (cached === entry) entry.promise = null; });
      queue = entry.promise.catch(() => {});
      return entry.promise;
    }
    function items() { const entry = usable(); return entry ? entry.read.items : []; }

    // ---- overlay (presentation only) ----
    function node(name, attrs) {
      const n = win.document.createElementNS(SVG, name);
      Object.keys(attrs).forEach((k) => n.setAttribute(k, attrs[k]));
      return n;
    }
    function ensureOverlay() {
      if (overlay && overlay.isConnected) return overlay;
      overlay = node('svg', { 'aria-hidden': 'true' });
      Object.assign(overlay.style, { position: 'fixed', left: '0', top: '0', width: '100vw', height: '100vh', overflow: 'visible', pointerEvents: 'none', zIndex: '900' });
      (win.document.body || win.document.documentElement).appendChild(overlay);
      return overlay;
    }
    function line(svg, a, b, color, width) {
      svg.appendChild(node('line', { x1: a.x, y1: a.y, x2: b.x, y2: b.y, stroke: color, 'stroke-width': width, 'vector-effect': 'non-scaling-stroke' }));
    }
    function paint() {
      if (paintQueued) return;
      paintQueued = true;
      (win.requestAnimationFrame || ((fn) => fn()))(() => { paintQueued = false; draw(); });
    }
    function draw() {
      const svg = ensureOverlay();
      while (svg.firstChild) svg.removeChild(svg.firstChild);
      const entry = usable(), map = entry && mappingFor(win, entry);
      if (!entry || !map) return;
      const scale = unit(map), screen = (item, lx, ly) => { const w = toWorld(item, lx, ly); return map.api.screen(w, map); };
      entry.read.items.forEach((item) => {
        if (!item.visible || !item.center) return;
        const dragging = drag && drag.item.layerUid === item.layerUid, color = dragging ? ACCENT : IDLE, hw = item.size[0] / 2, hh = item.size[1] / 2;
        const corners = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]].map((c) => screen(item, c[0], c[1]));
        svg.appendChild(node('polygon', { points: corners.map((p) => p.x + ',' + p.y).join(' '), fill: 'none', stroke: color, 'stroke-width': 1.4 }));
        if (item.kind === 'joystick') { line(svg, screen(item, -hw, 0), screen(item, hw, 0), GHOST, 1); line(svg, screen(item, 0, -hh), screen(item, 0, hh), GHOST, 1); }
        else line(svg, screen(item, -hw, 0), screen(item, hw, 0), TRACK, 1.4);
        if (item.rest) line(svg, screen(item, item.rest[0] - 4 / scale, item.rest[1]), screen(item, item.rest[0] + 4 / scale, item.rest[1]), GHOST, 1);
        const puck = dragging ? drag.local : item.puck;
        if (!puck) return;
        const at = screen(item, puck[0], puck[1]), r = puckRadius(item, scale) * scale;
        if (item.kind === 'joystick') line(svg, map.api.screen({ x: item.center[0], y: item.center[1] }, map), at, TRACK, 1.4);
        svg.appendChild(node('circle', { cx: at.x, cy: at.y, r, fill: color, stroke: WHITE, 'stroke-width': Math.max(1, r * 0.22) }));
      });
    }

    // ---- gestures ----
    function areaOf(e) {
      const area = win.document.getElementById('canvas-area');
      return area && e.target && area.contains(e.target) ? area : null;
    }
    // Topmost first: the last widget of the list wins, like layer order everywhere else. A locked or hidden pad never takes a gesture
    // (the padlock is the escape hatch that lets the pen draw over a pad).
    function hit(e) {
      const entry = usable(), map = entry && mappingFor(win, entry);
      if (!entry || !map) return null;
      let world;
      try { world = map.api.point(e, map); } catch (_) { return null; }
      const scale = unit(map), list = entry.read.items;
      for (let n = list.length - 1; n >= 0; n--) {
        const item = list[n];
        if (!item.visible || item.locked || !item.center) continue;
        const loc = toLocal(item, world.x, world.y);
        const tol = Math.max(puckRadius(item, scale), 10 / scale);
        if (item.puck && Math.hypot(loc[0] - item.puck[0], loc[1] - item.puck[1]) <= tol) return { entry, map, item, world, loc, onPuck: true };
        if (Math.abs(loc[0]) <= item.size[0] / 2 && Math.abs(loc[1]) <= item.size[1] / 2) return { entry, map, item, world, loc, onPuck: false };
      }
      return null;
    }
    function consume(e) { e.stopImmediatePropagation(); e.preventDefault(); }
    function adapter() { const m = win.NemoNativeObjectWidget; return m && typeof m.desktop === 'function' ? m.desktop(win) : null; }
    function cancel() { if (drag) { drag = null; paint(); } }
    function onDown(e) {
      if (drag || busy || (e.button !== undefined && e.button !== 0) || !areaOf(e)) return;
      const found = hit(e);
      if (!found) return;
      consume(e);
      if (!found.onPuck) { toast(win, 'Moving a native widget pad is unavailable (' + MOVE_REASON + ').'); return; }
      drag = { entry: found.entry, map: found.map, item: found.item, pointerId: e.pointerId, local: found.item.puck.slice(), world: [found.world.x, found.world.y], moved: false };
      paint();
    }
    function onMove(e) {
      if (!drag || (e.pointerId !== undefined && drag.pointerId !== undefined && e.pointerId !== drag.pointerId)) return;
      consume(e);
      if (!drag.entry.proof.current()) { cancel(); return; }
      try {
        const w = drag.map.api.point(e, drag.map), loc = toLocal(drag.item, w.x, w.y), item = drag.item;
        drag.world = [w.x, w.y];
        drag.local = [clamp(loc[0], -item.size[0] / 2, item.size[0] / 2), item.kind === 'joystick' ? clamp(loc[1], -item.size[1] / 2, item.size[1] / 2) : 0];
        drag.moved = true; paint();
      } catch (_) { cancel(); }
    }
    function write(entry, send) {
      const api = adapter();
      if (!api) { toast(win, 'Native widget is unavailable.'); return Promise.resolve(false); }
      busy = true;
      return Promise.resolve().then(() => send(api, entry.proof.selector.frame)).then(() => true, (error) => { failure(win, error); return false; })
        .then((ok) => { busy = false; return refresh().then(() => ok); });
    }
    function onUp(e) {
      if (!drag) return;
      consume(e);
      const done = drag; drag = null;
      if (!done.moved || !done.entry.proof.current()) { paint(); return; }
      write(done.entry, (api, frame) => api.writeWorld(done.item.layerUid, frame, done.world, done.item.center, done.item.angle));
    }
    function onDbl(e) {
      if (drag || busy || !areaOf(e)) return;
      const found = hit(e);
      if (!found) return;
      consume(e);
      write(found.entry, (api, frame) => api.writeRest(found.item.layerUid, frame));
    }
    function listeners() {
      const doc = win.document, capture = { capture: true };
      doc.addEventListener('pointerdown', (e) => { if (active(win)) onDown(e); }, capture);
      doc.addEventListener('pointermove', (e) => { if (drag) onMove(e); }, capture);
      doc.addEventListener('pointerup', (e) => { if (drag) onUp(e); }, capture);
      doc.addEventListener('pointercancel', () => { if (drag) cancel(); }, capture);
      doc.addEventListener('dblclick', (e) => { if (active(win)) onDbl(e); }, capture);
    }

    // ---- creation and the right panel ----
    function createLayer(kind) {
      const s = session(win), api = adapter(), info = s && s.info(), widget = win.NemoNativeObjectWidget;
      if (!s || !api || !info || !widget) { toast(win, 'Native widget is unavailable.'); return Promise.resolve(null); }
      const name = widget.layerName(kind, 'Layer ' + ((info.layers || []).length + 1));
      return api.create({ kind: widget.kindOf(kind), name, pos: [info.canvasW / 2, info.canvasH / 2] }).then((receipt) => {
        try {
          const index = s.info().layers.findIndex((layer) => layer.layerUid === receipt.layerUid);
          if (index >= 0 && win.NemoNativeObjectProject.selectLayerUI) win.NemoNativeObjectProject.selectLayerUI(win, index);
          if (win.showToast && win.SM && win.SM.t) win.showToast(win.SM.t('toastWidgetLayerAdded'));
        } catch (_) { /* the layer exists; selecting it is presentation */ }
        return refresh().then(() => receipt);
      }, (error) => { failure(win, error); return null; });
    }
    const el = (id) => win.document.getElementById(id);
    function activeItem() {
      const entry = usable(), info = entry && entry.proof.info, layer = info && info.layers && info.layers[win.state.activeLayerIdx];
      return layer ? (entry.read.items.find((item) => item.layerUid === layer.layerUid) || null) : null;
    }
    function paintPanel() {
      const section = el('p-widget-sec');
      if (!section) return;
      refresh();
      const item = activeItem();
      if (!item) { section.style.display = 'none'; return; }
      section.style.display = '';
      const put = (id, value) => { const field = el(id); if (field && win.document.activeElement !== field) field.value = value; };
      const text = (id, value) => { const field = el(id); if (field) field.textContent = value; };
      put('p-widget-w', Math.round(item.size ? item.size[0] : 0)); put('p-widget-h', Math.round(item.size ? item.size[1] : 0));
      text('p-widget-xlabel', item.x.label || 'X');
      put('p-widget-xmin', item.x.min); put('p-widget-xmax', item.x.max); put('p-widget-xrest', item.x.rest);
      ['p-widget-yrange-row', 'p-widget-yrest-row'].forEach((id) => { if (el(id)) el(id).style.display = item.y ? '' : 'none'; });
      if (item.y) { text('p-widget-ylabel', item.y.label || 'Y'); put('p-widget-ymin', item.y.min); put('p-widget-ymax', item.y.max); put('p-widget-yrest', item.y.rest); }
      ['p-widget-xadd', 'p-widget-yadd'].forEach((id) => { if (el(id)) el(id).style.display = 'none'; });
    }
    // A panel edit is one `widget.settings` command. The pad size keeps the other edge at its current value.
    const SETTINGS = {
      'p-widget-w': (item, v) => ({ size: [Math.max(8, v), item.size[1]] }), 'p-widget-h': (item, v) => ({ size: [item.size[0], Math.max(8, v)] }),
      'p-widget-xmin': (item, v) => ({ x: { min: v } }), 'p-widget-xmax': (item, v) => ({ x: { max: v } }), 'p-widget-xrest': (item, v) => ({ x: { rest: v } }),
      'p-widget-ymin': (item, v) => ({ y: { min: v } }), 'p-widget-ymax': (item, v) => ({ y: { max: v } }), 'p-widget-yrest': (item, v) => ({ y: { rest: v } }),
    };
    function bindPanel() {
      Object.keys(SETTINGS).forEach((id) => {
        const field = el(id);
        if (!field) return;
        field.addEventListener('change', () => {
          if (!active(win)) return;
          const item = activeItem(), value = parseFloat(field.value), api = adapter();
          if (!item || !item.size || !Number.isFinite(value) || !api || (/^p-widget-y/.test(id) && !item.y)) { refresh(); return; }
          api.settings(item.layerUid, SETTINGS[id](item, value)).then(refresh, (error) => { failure(win, error); refresh(); });
        });
      });
    }
    function install() { listeners(); bindPanel(); return api; }
    const api = Object.freeze({ install, refresh, paintPanel, createLayer, items, hit, active: () => active(win) });
    return api;
  }
  const controllers = new WeakMap();
  function desktop(win) {
    if (!controllers.has(win)) controllers.set(win, create(win));
    return controllers.get(win);
  }
  return Object.freeze({ create, desktop, active, toLocal, toWorld, MOVE_REASON });
}));
