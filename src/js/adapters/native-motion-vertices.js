/* Motion on a native object project: the VERTEX dots of an element's open Path group (#2520/#2525, C-motion-canvas-4). Baseline motion.js@1065e9d8
 * buildOverlayItems (~5600-5640: a violet diamond per vertex while the element's Path group is expanded) and onDown/onDrag 'vertex' (~6322-6345, ~6608):
 * a press on a dot grabs it, the drag moves it, and the vertex's `vtxN` Motion property takes the new OFFSET (a key at the playhead when the property
 * is animated, the static value otherwise), never the stored geometry.
 * Ordinary path dots use presented anchors; ribbon dots use presented source centerline vertices: the hit-test listing already carries the `vtxN` offsets in its points (readPresentedStrokes),
 * so a dot always sits where the shape is drawn. The drag is a local preview (the dot follows the pointer); the release is ONE `object.property.edit` of
 * `vtxN` (one revision, one history entry). The world delta is pulled back through the item's presented matrix (element pose and layer pose), so the offset
 * stays in the shape's own space. Escape, a changed document or frame, a stale selection and a refused edit write nothing.
 * Pointer events are taken on `document` in CAPTURE (as image-mesh-bridge does), so a press on a dot never reaches the Select gesture. This file holds no
 * document state: which element's Path group is open comes from the Motion panel (`open`). */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else {
    root.NemoNativeMotionVertices = api;
    if (root.document && typeof root.document.addEventListener === 'function') { try { api.install(root); } catch (_) { /* nothing to listen on */ } }
  }
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var NS = 'http://www.w3.org/2000/svg', PINS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  var VIOLET = 'rgb(190,130,240)', REACH = 9, THRESHOLD = 3, GONE = 'The document changed during the drag — nothing was changed.';
  var round = function (v) { return Math.round(v * 1e6) / 1e6; };
  var states = new WeakMap();
  function st(win) { if (!states.has(win)) states.set(win, { target: null, drag: null }); return states.get(win); }
  function notice(win, text) { if (typeof win.showToast === 'function') try { win.showToast(text, 'warn'); } catch (_) { /* presentation only */ } }
  function redraw(win) { var O = win.NemoNativeSelectOverlay; if (O && typeof O.show === 'function') try { O.show(win); } catch (_) { /* presentation only */ } }
  // The Motion panel opened (target) or closed (null) an element's Path group.
  function open(win, target) { st(win).target = target ? { contextId: target.contextId, layerUid: target.layerUid, strokeId: target.strokeId } : null; st(win).drag = null; redraw(win); }
  // The single selected element whose Path group is open, in Motion and with its presented segments; otherwise null.
  function pathOf(win) {
    var s = st(win), state = win.state, sel = (win.selectedPaths || []).filter(function (p) { return p && p.data && p.data.nativeStableTarget; });
    if (!s.target || !state || state.appMode !== 'motion' || state.playing === true || state.tool !== 'select' || sel.length !== 1) return null;
    var p = sel[0], t = p.data.nativeStableTarget;
    return t.strokeId === s.target.strokeId && t.layerUid === s.target.layerUid && t.contextId === s.target.contextId && Array.isArray(p.segments) && p.segments.length && Array.isArray(p.data.nativeMatrix) ? p : null;
  }
  function dots(win, p) {
    var d = st(win).drag, center = p.data && p.data.centerSegments;
    // A ribbon's editable identity is its centerline, never its painted edge.
    var points = Array.isArray(center) && center.length >= 2 ? center.map(function(s) { return s.point; }) : p.segments.map(function(s) { return [s.point.x,s.point.y]; });
    return points.map(function (at, i) { return d && d.armed && d.i === i ? [at[0] + d.delta[0], at[1] + d.delta[1]] : at; });
  }
  // Drawn by native-select-overlay.js inside its svg: `c` = { V, pin, bounds } (world to canvas units), `shown` the paths it outlines.
  function draw(win, svg, c, shown) {
    var p = pathOf(win);
    if (!p || shown.length !== 1 || shown[0] !== p) return;
    var V = c.V, k = c.pin.canvasW / c.bounds.width, r = 4.5 * k;
    dots(win, p).forEach(function (w) {
      var x = V[0] * w[0] + V[2] * w[1] + V[4], y = V[1] * w[0] + V[3] * w[1] + V[5], n = win.document.createElementNS(NS, 'polygon');
      n.setAttribute('points', [[x - r, y], [x, y - r], [x + r, y], [x, y + r]].join(' '));
      n.setAttribute('fill', VIOLET); n.setAttribute('stroke', 'rgb(30,30,30)'); n.setAttribute('stroke-width', '1'); n.setAttribute('vector-effect', 'non-scaling-stroke');
      svg.appendChild(n);
    });
  }
  // A world delta in the shape's own space: the inverse of the linear part of the item's presented matrix (element pose, layer pose and parents).
  function local(m, d) {
    var det = m[0] * m[3] - m[1] * m[2];
    return Math.abs(det) < 1e-12 ? null : [(m[3] * d[0] - m[2] * d[1]) / det, (-m[1] * d[0] + m[0] * d[1]) / det];
  }
  function hit(win, p, e, shown) {
    var I = win.NemoSelectCanvasIntent, at = I.worldPoint(win, e.clientX, e.clientY, shown.pin, shown.presentation), next = I.worldPoint(win, e.clientX + 1, e.clientY, shown.pin, shown.presentation);
    var wpp = Math.hypot(next.x - at.x, next.y - at.y) || at.perPixel, best = -1, reach = REACH * wpp;
    dots(win, p).forEach(function (w, i) { var dist = Math.hypot(w[0] - at.x, w[1] - at.y); if (dist <= reach && (best < 0 || dist < reach)) { best = i; reach = dist; } });
    return best < 0 ? null : { i: best, from: at, wpp: wpp, shown: shown };
  }
  function consume(e) { try { e.stopImmediatePropagation(); e.preventDefault(); } catch (_) { /* already dispatched */ } }
  function cancel(win) { var s = st(win); if (s.drag) { s.drag = null; redraw(win); } }
  // ONE `object.property.edit` of `vtxN`: the offset read at the playhead plus the pulled-back delta. Resolves to 'applied' or a refusal ('refused' / 'stale' / 'failed' / 'noop').
  async function commit(win, d, p) {
    var I = win.NemoSelectCanvasIntent, W = win.NemoNativeObjectWidgetUI, Wait = win.NemoNativePresentationWait, sel = [p];
    var delta = local(p.data.nativeMatrix, d.delta);
    if (!delta) { notice(win, 'This vertex cannot be moved here.'); return 'refused'; }
    var held = I.heldId(win), heldArg = held === 'scene-root' ? undefined : held, session = W.session(win), proof = W.proofNow(win, 'readProperty', heldArg);
    if (!proof && session && Wait) {
      await Wait.until(win, session, function () { return !!W.proofNow(win, 'readProperty', heldArg); }, function () { return !d.cancelled; });
      proof = W.proofNow(win, 'readProperty', heldArg);
    }
    if (d.cancelled) return 'cancelled';
    var same = function (a, b) { return !!a && !!b && PINS.every(function (k) { return a[k] === b[k]; }); };
    if (!proof || !proof.current(true) || !same(proof.info, d.shown.pin) || proof.selector.frame !== d.frame || I.pickContext(win) !== d.ctx) { notice(win, GONE); return 'stale'; }
    try {
      var target = { contextId: proof.selector.contextId, layerUid: p.data.nativeStableTarget.layerUid, strokeId: p.data.nativeStableTarget.strokeId }, property = 'vtx' + d.i;
      var base = await proof.session.readProperty({ instanceId: proof.info.instanceId, documentId: proof.info.documentId, atRevision: proof.info.contentRevision,
        documentSnapshotId: proof.info.documentSnapshotId, view: { kind: 'rawSample', target: target, property: property, frame: d.frame } });
      if (!base || !Array.isArray(base.value) || base.value.length !== 2 || !base.value.every(Number.isFinite)) throw new Error('The vertex offset could not be read.');
      var pin = {}; PINS.forEach(function (k) { pin[k] = proof.info[k]; });
      if (!proof.current(true)) { notice(win, GONE); return 'stale'; }
      var response = await proof.session.editProperty({ command: 'object.property.edit', target: target, property: property,
        action: { kind: 'setValue', frame: d.frame, values: [round(base.value[0] + delta[0]), round(base.value[1] + delta[1])] } }, pin, function () { return proof.current(); });
      if (!response || response.ok !== true || !response.result || typeof response.result.applied !== 'boolean') throw new Error('Native property edit was not confirmed.');
      if (!response.result.applied) return 'noop';
    } catch (error) {
      notice(win, (error && error.message || 'Native Motion edit is unavailable.') + (error && error.code ? ' (' + error.code + ')' : ''));
      return 'failed';
    }
    var fresh = null;
    try { fresh = await I.reread(win, sel, false); } catch (_) { fresh = null; }
    win.selectedPaths = fresh && fresh.complete ? fresh.paths : [];
    win.state.selectedStrokeIndices = [];
    return 'applied';
  }
  function install(win) {
    var doc = win.document, area = function (e) { var a = doc.getElementById && doc.getElementById('canvas-area'); return a && e.target && typeof a.contains === 'function' && a.contains(e.target); };
    doc.addEventListener('pointerdown', function (e) {
      var s = st(win), p = pathOf(win), I = win.NemoSelectCanvasIntent, shown;
      if (s.drag || !p || !I || (e.button !== undefined && e.button !== 0) || !area(e) || win.state.spaceDown || win.state.isPanning) return;
      try { shown = I.presentedState(win); } catch (_) { shown = null; }
      var found = shown && hit(win, p, e, shown);
      if (!found) return;
      consume(e);
      s.drag = { i: found.i, from: found.from, wpp: found.wpp, shown: shown, frame: shown.presentation.frame, ctx: I.pickContext(win), x: e.clientX, y: e.clientY, delta: [0, 0], armed: false, cancelled: false, pointerId: e.pointerId };
    }, { capture: true });
    doc.addEventListener('pointermove', function (e) {
      var s = st(win), d = s.drag, p = pathOf(win);
      if (!d || (e.pointerId !== undefined && d.pointerId !== undefined && e.pointerId !== d.pointerId)) return;
      consume(e);
      if (!p) { cancel(win); return; }
      if (!d.armed && Math.hypot(e.clientX - d.x, e.clientY - d.y) < THRESHOLD) return;
      var at = win.NemoSelectCanvasIntent.worldPoint(win, e.clientX, e.clientY, d.shown.pin, d.shown.presentation);
      d.armed = true; d.delta = [at.x - d.from.x, at.y - d.from.y]; redraw(win);
    }, { capture: true });
    doc.addEventListener('pointerup', function (e) {
      var s = st(win), d = s.drag, p = pathOf(win);
      if (!d || (e.pointerId !== undefined && d.pointerId !== undefined && e.pointerId !== d.pointerId)) return;
      consume(e);
      if (!d.armed || !p) { cancel(win); return; }
      // The preview stays until the committed element is read back; a refusal drops it.
      commit(win, d, p).then(function () { if (st(win).drag === d) st(win).drag = null; redraw(win); }, function () { if (st(win).drag === d) st(win).drag = null; redraw(win); });
    }, { capture: true });
    doc.addEventListener('keydown', function (e) { var d = st(win).drag; if (d && e.key === 'Escape') { d.cancelled = true; consume(e); cancel(win); } }, { capture: true });
    return api;
  }
  var api = Object.freeze({ open: open, draw: draw, install: install, pathOf: pathOf, local: local, cancel: cancel });
  return api;
}));
