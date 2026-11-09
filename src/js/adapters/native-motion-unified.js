/* Motion on a native object project: the UNIFIED path of a multi-element selection (#2520/#2525, C-motion-canvas-4). Baseline motion.js@1065e9d8
 * unifiedMotionTargets / unifiedFrames / unifiedPointAt / buildUnifiedOverlay (~5278-5330) and onDown/onDrag 'unified' (~6313, ~6640): with two or more
 * selected elements of ONE layer that have Position keys, a violet dashed path joins one dot per key frame of any of them, each dot at the selection's
 * centroid plus the MEAN Position of the elements at that frame (an element with no key there is read interpolated). Dragging a dot moves the Position key of
 * EVERY element at that frame by the same delta; an element with no key there gets one, frozen at its interpolated value first, so its curve is not yanked.
 * The write is ONE `object.property.batch` of `setKey` entries (one revision, one history entry); the world delta is pulled back through the layer pose,
 * as the single-element move does. Reads are bounded (authoredKeys pages, rawSample for the frames an element has no key on) and pinned to the presentation.
 * Drawn by native-select-overlay.js inside its svg; pointer events are taken on `document` in capture, ahead of the Select gesture. No document state. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else {
    root.NemoNativeMotionUnified = api;
    if (root.document && typeof root.document.addEventListener === 'function') { try { api.install(root); } catch (_) { /* nothing to listen on */ } }
  }
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var NS = 'http://www.w3.org/2000/svg', PINS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  var VIOLET = 'rgba(189,147,249,0.86)', ORANGE = 'rgb(255,170,40)', MAX_ELEMENTS = 16, MAX_FRAMES = 60, MAX_PAGES = 4, REACH = 8, THRESHOLD = 3;
  var GONE = 'The document changed during the drag — nothing was changed.';
  var round = function (v) { return Math.round(v * 1e6) / 1e6; };
  var states = new WeakMap();
  function st(win) { if (!states.has(win)) states.set(win, { key: '', data: null, loading: null, drag: null }); return states.get(win); }
  function notice(win, text) { if (typeof win.showToast === 'function') try { win.showToast(text, 'warn'); } catch (_) { /* presentation only */ } }
  function redraw(win) { var O = win.NemoNativeSelectOverlay; if (O && typeof O.show === 'function') try { O.show(win); } catch (_) { /* presentation only */ } }
  // The selected elements of ONE layer (two or more), in Motion with the Select tool; otherwise null.
  function selection(win) {
    var state = win.state, sel = (win.selectedPaths || []).filter(function (p) { return p && p.data && p.data.nativeStableTarget && p.data.nativeStableTarget.strokeId; });
    if (!state || state.appMode !== 'motion' || state.playing === true || state.tool !== 'select' || sel.length < 2) return null;
    var first = sel[0].data.nativeStableTarget;
    if (!sel.every(function (p) { var t = p.data.nativeStableTarget; return t.layerUid === first.layerUid && t.contextId === first.contextId; })) return null;
    return sel;
  }
  function keyOf(win, sel, shown) { return JSON.stringify([PINS.map(function (k) { return shown.pin[k]; }), shown.presentation.frame, sel.map(function (p) { return p.data.nativeStableTarget.strokeId; })]); }
  var at = function (win) { var I = win.NemoSelectCanvasIntent; return I && typeof I.presentedState === 'function' ? I.presentedState(win) : null; };
  // Every selected element's Position keys and, for each frame any of them is keyed on, its value there; null when fewer than two elements have keys.
  async function load(win, sel, shown) {
    var W = win.NemoNativeObjectWidgetUI, held = win.NemoSelectCanvasIntent.heldId(win), proof = W.proofNow(win, 'readProperty', held === 'scene-root' ? undefined : held);
    if (!proof || !proof.current(true) || PINS.some(function (k) { return proof.info[k] !== shown.pin[k]; })) return null;
    var frame = proof.selector.frame, one = function (view) { return proof.session.readProperty({ instanceId: proof.info.instanceId, documentId: proof.info.documentId, atRevision: proof.info.contentRevision, documentSnapshotId: proof.info.documentSnapshotId, view: view }); };
    var seen = {}, targets = [];
    for (var i = 0; i < sel.length && targets.length < MAX_ELEMENTS; i++) {
      var t = sel[i].data.nativeStableTarget, id = t.strokeId;
      if (seen[id]) continue; seen[id] = true;
      var target = { contextId: proof.selector.contextId, layerUid: t.layerUid, strokeId: id }, keys = [];
      for (var offset = 0, n = 0; n < MAX_PAGES; n++) {
        var page = await one({ kind: 'authoredKeys', target: target, property: 'position', offset: offset, limit: 8 });
        page.items.forEach(function (k) { keys.push(k); });
        if (page.nextOffset === null || page.nextOffset === undefined) break;
        offset = page.nextOffset;
      }
      if (keys.length) targets.push({ target: target, keys: keys, idx: sel[i].data.nativeLayerIndex });
    }
    if (targets.length < 2) return null;
    var set = {}; targets.forEach(function (x) { x.keys.forEach(function (k) { set[k.frame] = true; }); });
    var frames = Object.keys(set).map(Number).sort(function (a, b) { return a - b; });
    if (frames.length > MAX_FRAMES) return null;
    for (var j = 0; j < targets.length; j++) {
      var x = targets[j]; x.values = {};
      for (var f = 0; f < frames.length; f++) {
        var own = x.keys.find(function (k) { return k.frame === frames[f]; });
        x.values[frames[f]] = own ? { v: own.v.slice(), keyed: true } : { v: (await one({ kind: 'rawSample', target: x.target, property: 'position', frame: frames[f] })).value.slice(), keyed: false };
      }
    }
    var cx = 0, cy = 0; sel.forEach(function (p) { var c = p.bounds.center; cx += c.x; cy += c.y; }); cx /= sel.length; cy /= sel.length;
    var points = frames.map(function (fr) { var sx = 0, sy = 0; targets.forEach(function (x) { sx += x.values[fr].v[0]; sy += x.values[fr].v[1]; }); return { frame: fr, x: cx + sx / targets.length, y: cy + sy / targets.length }; });
    return { frames: frames, points: points, targets: targets, current: frame };
  }
  // The unified path for the current selection, loading it in the background (the overlay redraws when it lands); null while unknown.
  function dataOf(win) {
    var s = st(win), sel = selection(win), shown = sel && at(win);
    if (!sel || !shown) { s.key = ''; s.data = null; return null; }
    var key = keyOf(win, sel, shown);
    if (key === s.key) return s.data;
    s.key = key; s.data = null;
    var mine = s.loading = load(win, sel, shown).then(function (d) { if (s.loading === mine && s.key === key) { s.data = d; redraw(win); } }, function () { /* nothing to draw */ });
    return null;
  }
  function pointsOf(win, d) {
    var drag = st(win).drag;
    return d.points.map(function (p) { return drag && drag.armed && drag.frame === p.frame ? { frame: p.frame, x: p.x + drag.delta[0], y: p.y + drag.delta[1] } : p; });
  }
  function draw(win, svg, c, shown) {
    var d = dataOf(win), sel = selection(win);
    if (!d || !sel || shown.length < 2) return;
    var V = c.V, k = c.pin.canvasW / c.bounds.width, pts = pointsOf(win, d).map(function (p) { return { frame: p.frame, x: V[0] * p.x + V[2] * p.y + V[4], y: V[1] * p.x + V[3] * p.y + V[5] }; });
    var add = function (tag, attrs) { var n = win.document.createElementNS(NS, tag); Object.keys(attrs).forEach(function (a) { n.setAttribute(a, attrs[a]); }); n.setAttribute('vector-effect', 'non-scaling-stroke'); svg.appendChild(n); };
    if (pts.length > 1) add('polyline', { points: pts.map(function (p) { return p.x + ',' + p.y; }).join(' '), fill: 'none', stroke: VIOLET, 'stroke-width': '1.5', 'stroke-dasharray': '5 4' });
    pts.forEach(function (p) { var cur = p.frame === d.current; add('circle', { cx: p.x, cy: p.y, r: (cur ? 6 : 4.5) * k, fill: cur ? ORANGE : VIOLET, stroke: 'rgb(30,30,30)', 'stroke-width': '1.2' }); });
  }
  function consume(e) { try { e.stopImmediatePropagation(); e.preventDefault(); } catch (_) { /* already dispatched */ } }
  function cancel(win) { var s = st(win); if (s.drag) { s.drag = null; redraw(win); } }
  // ONE `object.property.batch` of `setKey`: each element's key at the dragged frame, its value there plus the pulled-back delta.
  async function commit(win, drag, d) {
    var I = win.NemoSelectCanvasIntent, W = win.NemoNativeObjectWidgetUI, Wait = win.NemoNativePresentationWait, M = win.NemoNativeMotionElement;
    var held = I.heldId(win), heldArg = held === 'scene-root' ? undefined : held, session = W.session(win), proof = W.proofNow(win, 'readProperty', heldArg);
    if (!proof && session && Wait) {
      await Wait.until(win, session, function () { return !!W.proofNow(win, 'readProperty', heldArg); }, function () { return !drag.cancelled; });
      proof = W.proofNow(win, 'readProperty', heldArg);
    }
    if (drag.cancelled) return 'cancelled';
    if (!proof || !proof.current(true) || PINS.some(function (k) { return proof.info[k] !== drag.shown.pin[k]; }) || proof.selector.frame !== drag.current || I.pickContext(win) !== drag.ctx) { notice(win, GONE); return 'stale'; }
    if (!proof.session || typeof proof.session.editPropertyBatch !== 'function') { notice(win, 'Native property batch is unavailable.'); return 'refused'; }
    try {
      var edits = [];
      for (var i = 0; i < d.targets.length; i++) {
        var x = d.targets[i], local = M ? await M.localDelta(win, x.idx, proof.info, proof.selector.frame, drag.delta) : drag.delta.slice(), v = x.values[drag.frame].v;
        edits.push({ target: x.target, property: 'position', action: { kind: 'setKey', frame: drag.frame, values: [round(v[0] + local[0]), round(v[1] + local[1])] } });
      }
      var pin = {}; PINS.forEach(function (k) { pin[k] = proof.info[k]; });
      if (!proof.current(true)) { notice(win, GONE); return 'stale'; }
      var response = await proof.session.editPropertyBatch({ command: 'object.property.batch', edits: edits }, pin, function () { return proof.current(); });
      if (!response || response.ok !== true || !response.result || typeof response.result.applied !== 'boolean') throw new Error('Native property edit was not confirmed.');
      if (!response.result.applied) return 'noop';
      // The committed elements as fresh presented paths (the old ones hold the geometry from before the edit), still selected.
      var fresh = null; try { fresh = await I.reread(win, selection(win) || [], false); } catch (_) { fresh = null; }
      if (fresh && fresh.complete) { win.selectedPaths = fresh.paths; win.state.selectedStrokeIndices = []; }
      return 'applied';
    } catch (error) {
      notice(win, (error && error.message || 'Native Motion edit is unavailable.') + (error && error.code ? ' (' + error.code + ')' : ''));
      return 'failed';
    }
  }
  function install(win) {
    var doc = win.document, area = function (e) { var a = doc.getElementById && doc.getElementById('canvas-area'); return a && e.target && typeof a.contains === 'function' && a.contains(e.target); };
    doc.addEventListener('pointerdown', function (e) {
      var s = st(win), d = s.data, sel = selection(win), I = win.NemoSelectCanvasIntent, shown;
      if (s.drag || !d || !sel || !I || (e.button !== undefined && e.button !== 0) || !area(e) || win.state.spaceDown || win.state.isPanning) return;
      try { shown = at(win); } catch (_) { shown = null; }
      if (!shown || keyOf(win, sel, shown) !== s.key) return;
      var here = I.worldPoint(win, e.clientX, e.clientY, shown.pin, shown.presentation), next = I.worldPoint(win, e.clientX + 1, e.clientY, shown.pin, shown.presentation);
      var reach = REACH * (Math.hypot(next.x - here.x, next.y - here.y) || here.perPixel), best = null;
      d.points.forEach(function (p) { var dist = Math.hypot(p.x - here.x, p.y - here.y); if (dist <= reach && (!best || dist < best.dist)) best = { frame: p.frame, dist: dist }; });
      if (!best) return;
      consume(e);
      s.drag = { frame: best.frame, from: here, shown: shown, current: d.current, ctx: I.pickContext(win), x: e.clientX, y: e.clientY, delta: [0, 0], armed: false, cancelled: false, pointerId: e.pointerId };
    }, { capture: true });
    doc.addEventListener('pointermove', function (e) {
      var s = st(win), drag = s.drag;
      if (!drag || (e.pointerId !== undefined && drag.pointerId !== undefined && e.pointerId !== drag.pointerId)) return;
      consume(e);
      if (!drag.armed && Math.hypot(e.clientX - drag.x, e.clientY - drag.y) < THRESHOLD) return;
      var to = win.NemoSelectCanvasIntent.worldPoint(win, e.clientX, e.clientY, drag.shown.pin, drag.shown.presentation);
      drag.armed = true; drag.delta = [to.x - drag.from.x, to.y - drag.from.y]; redraw(win);
    }, { capture: true });
    doc.addEventListener('pointerup', function (e) {
      var s = st(win), drag = s.drag, d = s.data;
      if (!drag || (e.pointerId !== undefined && drag.pointerId !== undefined && e.pointerId !== drag.pointerId)) return;
      consume(e);
      if (!drag.armed || !d || !selection(win)) { cancel(win); return; }
      var done = function () { if (st(win).drag === drag) st(win).drag = null; st(win).key = ''; redraw(win); }; // the path is read afresh from the committed keys
      commit(win, drag, d).then(done, done);
    }, { capture: true });
    doc.addEventListener('keydown', function (e) { var drag = st(win).drag; if (drag && e.key === 'Escape') { drag.cancelled = true; consume(e); cancel(win); } }, { capture: true });
    return api;
  }
  var api = Object.freeze({ draw: draw, install: install, selection: selection, dataOf: dataOf, cancel: cancel });
  return api;
}));
