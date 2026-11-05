/* Native Eraser (B-eraser-tool). On a native object project the Paper layer holds no live items, so eraser-bridge.js's eraseAtPoint bit nothing and the
 * pointer was swallowed in silence. This adapter drives the native owner instead, one pointer gesture at a time (tools.js @1065e9d8 eraser branches):
 *   press   pushUndo + ensureKeyframe, then the first bite is a circle of radius state.eraserSize/2 (the pen's pressure scales it, eraseRadiusFor)
 *   drag    each sample bites the capsule swept from the previous sample (buildEraserCapsule), the stabilizer's moving average first (stabilizeErasePoint);
 *           the hit is the top-most item under the point - an exact hit before a max(8, radius)/zoom tolerance one (eraseAt) - resampled every eraserSize/4
 *   Alt     drag resizes the eraser (native-object-draw-resize.js with the eraser's own target); no write
 * Every bite is one `object.boolean.erase` through the existing group intent (the engine subtracts, splits islands, keys a held cell). The first bite of a
 * gesture adds the undo entry and the next ones send `joinHistory`, so the drag is ONE revision per bite but ONE undo/redo step, the baseline's single
 * pushUndo. Hits are read from the presented layer (NemoSelectCanvasIntent.layerPaths); nothing is kept: a change of document, frame, layer or tool under
 * the gesture (an undo, a scrub) ends it with no further write. There is no Paper/WASM writer to fall back to. */
(function (root, factory) {
  var node = typeof module === 'object' && module.exports;
  var api = factory(node ? require('./native-object-draw-resize.js') : root.NemoNativeDrawResize);
  if (node) module.exports = api; else root.NemoNativeObjectEraserTool = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Resize) {
  'use strict';
  var SVG = 'http://www.w3.org/2000/svg', PINS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  var RING = { fill: 'rgba(255,255,255,0.122)', stroke: 'rgba(74,158,255,0.902)' }; // engine-bridge buildEraserCursorItems
  var VECTOR = { plain: 1, ribbon: 1, fill: 1, shape: 1, brush: 1, mask: 1 };
  function same(a, b) { return !!a && !!b && PINS.every(function (k) { return a[k] === b[k]; }); }
  function fail(message) { throw new Error(message); }
  function consume(e) { if (e && e.stopImmediatePropagation) e.stopImmediatePropagation(); if (e && e.preventDefault) e.preventDefault(); }
  function round(n) { return Math.round(n * 1000) / 1000; }
  // buildEraserCapsule: the stadium swept from `a` to `b` at `radius` (a circle for the first sample or a sweep under half a unit), flattened within 0.25.
  function capsule(a, b, radius) {
    var r = Math.max(radius, 0.1), k = 2 * Math.max(2, Math.min(20, Math.ceil(Math.PI / (4 * Math.acos(Math.max(0, 1 - 0.25 / Math.max(r, 0.3)))))));
    var sweep = a && Math.hypot(b[0] - a[0], b[1] - a[1]) >= 0.5, out = [], i, t;
    if (!sweep) { for (i = 0; i < 2 * k; i++) { t = Math.PI * i / k; out.push([round(b[0] + r * Math.cos(t)), round(b[1] + r * Math.sin(t))]); } return out; }
    var angle = Math.atan2(b[1] - a[1], b[0] - a[0]);
    for (i = 0; i <= k; i++) { t = angle - Math.PI / 2 + Math.PI * i / k; out.push([round(b[0] + r * Math.cos(t)), round(b[1] + r * Math.sin(t))]); }
    for (i = 0; i <= k; i++) { t = angle + Math.PI / 2 + Math.PI * i / k; out.push([round(a[0] + r * Math.cos(t)), round(a[1] + r * Math.sin(t))]); }
    return out;
  }
  // The same polygon in a record's own space: the listed record-to-world matrix [a b c d e f] inverted (null when it is singular or not finite).
  function inRecord(points, m) {
    var det = m[0] * m[3] - m[1] * m[2];
    if (!Number.isFinite(det) || Math.abs(det) < 1e-9) return null;
    return points.map(function (p) { var x = p[0] - m[4], y = p[1] - m[5]; return [round((m[3] * x - m[2] * y) / det), round((-m[1] * x + m[0] * y) / det)]; });
  }
  function create(root, ports) {
    var Mapping = ports.mapping, intents = new WeakMap(), drag = null, ring = null, last = null, retired = false, lastToast = 0, lastPressure = null, settled = Promise.resolve({ status: 'idle' });
    function session() { return root.NemoNativeObjectProject && root.NemoNativeObjectProject.desktop(root); }
    function nativeMode() { try { var s = session(); return !!s && s.blocksLegacy(); } catch (_) { return true; } }
    function notice(text) { if (typeof root.showToast === 'function') root.showToast(text); }
    // eraser-bridge.js shows nothing twice within 1.5 s for the same refusal (canEditActiveLayer); so do the typed refusals here.
    function once(text) { var now = Date.now(); if (now - lastToast > 1500) { lastToast = now; notice(text); } }
    function stage(e) { var d = root.document; return e.target === d.getElementById('drawing-canvas') || e.target === d.getElementById('canvas-area'); }
    function contextId() { var c = root.NemoNativeComponentContext; return c ? c.contextId(root) : 'scene-root'; }
    function bounds(info) {
      var d = root.document, el = d.getElementById('drawing-canvas'), r = el && el.getBoundingClientRect(), html = d.documentElement;
      if (!r || ![r.left, r.top, r.width, r.height, info.canvasW, info.canvasH].every(Number.isFinite) || r.width <= 0 || r.height <= 0 || info.canvasW <= 0 || info.canvasH <= 0) fail('Native canvas presentation is unavailable.');
      if (html && html.classList && !html.classList.contains('nemo-native-underlay')) fail('Native canvas presentation is unavailable.');
      return { left: r.left, top: r.top, width: r.width, height: r.height };
    }
    // CSS pixels per document unit as presented (the baseline's view.zoom); null when the presentation is not uniform.
    function scaleOf(t) {
      var m = Mapping.transform({ presentation: t.presentation }, false), kx = t.bounds.width / t.pin.canvasW, ky = t.bounds.height / t.pin.canvasH;
      var a = m[0] * kx, b = m[1] * ky, c = m[2] * kx, d = m[3] * ky, det = a * d - b * c, scale = Math.sqrt(det);
      return det > 0 && Number.isFinite(scale) && Math.abs(a - d) <= scale / 1024 && Math.abs(b + c) <= scale / 1024 ? scale : null;
    }
    // The presented canvas, or null (with `onRefusal(reason)`) when it cannot be proven right now.
    function view(onRefusal) {
      var s = session(), pin = s && s.isActive() && s.info(), shown = pin && s.readPresentedObjectSession(onRefusal), st = root.state;
      if (!pin || !shown || !same(pin, shown) || !shown.presentation || shown.presentation.owner !== 'native' || shown.presentation.status !== 'presented' ||
          !Mapping.matchesView(root, shown.presentation) || st.appMode !== 'anim2d' || st.playing) return null;
      var v = { s: s, pin: pin, presentation: shown.presentation }; v.bounds = bounds(pin); v.scale = scaleOf(v);
      if (!(v.scale > 0)) fail('Native canvas presentation is unavailable.');
      return v;
    }
    var sizer = Resize ? Resize.create(root, { Mapping: Mapping, session: session, bounds: bounds, scale: scaleOf, notice: notice, consume: consume, target: {
      key: 'eraserSize', field: 'p-erasersize', fill: RING.fill, stroke: RING.stroke, unavailable: 'Native eraser resize is unavailable.',
      size: function (start, x0, x) { return Math.max(2, start + (x - x0) * 0.5); }, toast: function (v) { return 'Taille de la gomme : ' + Math.round(v) + 'px'; } } }) : null;
    // eraseRadiusFor: the nominal radius, scaled by a pen's pressure (held across the gesture so a lift-off sample reads as the last pressure).
    function radiusFor(e) {
      var st = root.state, base = st.eraserSize / 2, p = null, force = root._stylus;
      if (e.pointerType === 'pen') p = typeof e.pressure === 'number' && e.pressure > 0 ? Math.min(1, e.pressure) : lastPressure;
      if (p == null && force && force.force > 0 && Date.now() - force.forceT < 250) p = force.force;
      if (p == null) return base;
      lastPressure = p;
      return base * (st.pressureMin / 100 + (st.pressureMax / 100 - st.pressureMin / 100) * p);
    }
    function showRing(v, e, radius) {
      var d = root.document, circle;
      if (!ring) {
        ring = d.createElementNS(SVG, 'svg'); circle = d.createElementNS(SVG, 'circle');
        ring.setAttribute('aria-hidden', 'true'); circle.setAttribute('fill', RING.fill); circle.setAttribute('stroke', RING.stroke); circle.setAttribute('stroke-width', '1');
        ring.appendChild(circle); d.body.appendChild(ring);
      }
      circle = ring.firstChild;
      Object.assign(ring.style, { position: 'fixed', pointerEvents: 'none', zIndex: '20', left: v.bounds.left + 'px', top: v.bounds.top + 'px', width: v.bounds.width + 'px', height: v.bounds.height + 'px' });
      ring.setAttribute('viewBox', '0 0 ' + v.bounds.width + ' ' + v.bounds.height);
      circle.setAttribute('cx', e.clientX - v.bounds.left); circle.setAttribute('cy', e.clientY - v.bounds.top); circle.setAttribute('r', radius * v.scale);
    }
    function hideRing() { if (ring) { ring.remove(); ring = null; } }
    function hover(e) { try { var v = view(); if (v) { last = v; showRing(v, e, radiusFor(e)); } else if (last) showRing(last, e, radiusFor(e)); } catch (_) { /* a missing presentation shows no ring */ } }
    function refusal(layer, row) {
      if (row.symbolId) return 'Calque composant — double-clique dessus pour entrer dedans et dessiner';
      if (layer.locked === true) return 'Calque verrouillé — déverrouille-le (cadenas) pour dessiner dessus';
      if (layer.visible === false) return 'Calque masqué — affiche-le pour y effacer';
      if (layer.threeD === true || row.threeD) return 'La gomme n’efface pas encore sur un calque 3D.';
      return null;
    }
    // The press needs no presentation: the first bite waits for one (`ready`), so a press right after the previous gesture's last bite is not lost.
    function open(e) {
      var s = session(), pin = s && s.isActive() && s.info(), st = root.state, row = st.layers && st.layers[st.activeLayerIdx], c = root.NemoNativeComponentContext;
      if (st.spaceDown || st.isPanning) return null;
      if (!pin) fail('La gomme est indisponible.');
      if (st.appMode !== 'anim2d' || st.playing) fail('La gomme n\u2019est disponible qu\u2019en Animation 2D, hors lecture.');
      var layer = row && (c && c.layerRows(root) || pin.layers).filter(function (l) { return l.layerUid === row.layerUid; })[0];
      if (!layer) fail('S\u00e9lectionne un calque de dessin.');
      var why = refusal(layer, row); if (why) fail(why);
      return { session: s, pointer: e.pointerId, layerIdx: st.activeLayerIdx, layerUid: layer.layerUid, context: contextId(), frame: st.currentFrame, pin: null, queue: [],
        stab: [], last: null, joined: false, cancelled: false, running: false, ended: false };
    }
    function holds(t) {
      var s = session(), st = root.state;
      return !retired && !t.cancelled && s === t.session && s.isActive() && s.blocksLegacy() && st.tool === 'eraser' && st.currentFrame === t.frame && st.activeLayerIdx === t.layerIdx && contextId() === t.context;
    }
    function cancel() { if (sizer) sizer.cancel(); if (drag) { var t = drag; drag = null; t.cancelled = true; t.queue.length = 0; release(t); } return { status: 'cancelled' }; }
    function release(t) { try { var el = root.document.getElementById('canvas-area') || root.document.getElementById('drawing-canvas'); if (el.releasePointerCapture) el.releasePointerCapture(t.pointer); } catch (_) { /* already released */ } }
    // stabilizeErasePoint: a moving average of the last 3 / 6 / 10 samples for Low / Medium / High (the higher modeler levels act as High).
    function stabilize(t, p) {
      var level = Math.min(3, root.state.stabilizer || 0);
      if (!level) { t.stab.length = 0; return p; }
      t.stab.push(p); while (t.stab.length > (level === 1 ? 3 : level === 2 ? 6 : 10)) t.stab.shift();
      return [t.stab.reduce(function (a, q) { return a + q[0]; }, 0) / t.stab.length, t.stab.reduce(function (a, q) { return a + q[1]; }, 0) / t.stab.length];
    }
    // eraseAt: the top-most item under the point; an exact hit wins before a tolerance one (a stray fragment near the cursor must not steal it).
    function hitAt(paths, p, tolerance) {
      var Point = root.Point || root.paper.Point, at = new Point(p[0], p[1]);
      return [0, tolerance].reduce(function (found, tol) {
        for (var i = paths.length - 1; !found && i >= 0; i--) if (paths[i].hitTest(at, { fill: true, stroke: true, tolerance: tol })) found = paths[i];
        return found;
      }, null);
    }
    function intent(s) {
      var Group = root.NemoNativeObjectGroupIntent, Component = root.NemoNativeObjectComponentIntent, Contract = root.NemoNativeObjectEditContract;
      if (!Group || !Component || !Contract || typeof Component.portsFromSession !== 'function') return null;
      if (!intents.has(s)) {
        var base = Component.portsFromSession(s);
        intents.set(s, Group.createGroupIntent({ pin: base.pin, requestId: base.requestId, submit: base.submit, declared: function (op) { return Contract.admitted(op) && base.readDeclared() === true; } }));
      }
      return intents.get(s);
    }
    async function ready(t) {
      var W = root.NemoNativePresentationWait, read = function (why) { try { return view(why); } catch (_) { return null; } };
      if (!W) return read();
      var r = await W.await(root, { read: read, ready: function (v) { return !!v; }, holds: function () { return holds(t); }, notice: 'Le canevas est occupé — la gomme s’est arrêtée.' }, { tries: 17 });
      return r.ready ? r.value : null;
    }
    // One bite: the capsule from the previous sample to this one, on every distinct top-most hit along the sweep.
    async function bite(t, sample) {
      var v = await ready(t);
      if (!v || !holds(t) || t.pin && !same(v.pin, t.pin)) { t.cancelled = true; return; } // an undo, a scrub or another edit ended the gesture: no further write
      t.pin = v.pin; last = v;
      var w = Mapping.point({ clientX: sample.x, clientY: sample.y }, { bounds: v.bounds, canvasW: v.pin.canvasW, canvasH: v.pin.canvasH, presentation: v.presentation });
      var p = stabilize(t, [w.x, w.y]), from = t.last, r = sample.r, tol = Math.max(8, r) / v.scale;
      var box = [Math.min(p[0], from ? from[0] : p[0]) - r - tol, Math.min(p[1], from ? from[1] : p[1]) - r - tol, Math.max(p[0], from ? from[0] : p[0]) + r + tol, Math.max(p[1], from ? from[1] : p[1]) + r + tol];
      var layers = await root.NemoSelectCanvasIntent.layerPaths(root, function (item, b) {
        return !!VECTOR[item.role] && b[0] <= box[2] && b[2] >= box[0] && b[1] <= box[3] && b[3] >= box[1]; }, false, t.layerIdx);
      t.last = p;
      if (!layers || !holds(t)) return;
      var steps = from ? Math.min(40, Math.max(1, Math.ceil(Math.hypot(p[0] - from[0], p[1] - from[1]) / Math.max(1, root.state.eraserSize / 4)))) : 1, hits = [], ids = {};
      for (var i = 1; i <= steps; i++) {
        var at = from ? [from[0] + (p[0] - from[0]) * i / steps, from[1] + (p[1] - from[1]) * i / steps] : p, hit = hitAt(layers.paths, at, tol);
        if (hit && !ids[hit.data.strokeId]) { ids[hit.data.strokeId] = true; hits.push(hit); }
      }
      var shape = capsule(from, p, r), made = intent(t.session);
      if (!made) fail('Native erase is unavailable.');
      for (var k = 0; k < hits.length && !t.cancelled; k++) {
        var points = hits[k].data.nativeMatrix ? inRecord(shape, hits[k].data.nativeMatrix) : shape;
        if (!points) continue;
        var input = { layerUid: layers.layerUid, contextId: t.context, frame: layers.frame, strokeId: hits[k].data.strokeId, footprint: points };
        if (t.joined) input.joinHistory = true;
        var out = await made.booleanErase(input);
        if (out.ok && out.applied) { t.joined = true; t.pin = t.session.info(); }
        else if (!out.ok) outcome(t, out);
      }
    }
    // A foreign change ends the gesture; a refusal of one shape (curved beyond the owner's limits, animated, too large) is said once and the sweep goes on.
    function outcome(t, out) {
      if (['stale_revision', 'wrong_document', 'wrong_instance', 'cancelled_before_dispatch', 'stale_selection'].indexOf(out.code) >= 0) { t.cancelled = true; return; }
      if (out.indeterminate) { t.cancelled = true; notice('La gomme n’a pas pu confirmer la dernière coupe.'); return; }
      once(out.code === 'request_too_large' ? 'La gomme est trop grande pour cette forme.' : 'La gomme ne peut pas couper cette forme.');
    }
    function take(t) { var q = t.queue.splice(0), keep = []; q.forEach(function (s, i) { if (i === q.length - 1 || q.length <= 6 || i % Math.ceil(q.length / 6) === 0) keep.push(s); }); return keep; }
    async function run(t, before) {
      t.running = true;
      try { await before; while (t.queue.length && !t.cancelled) { var batch = take(t); for (var i = 0; i < batch.length && !t.cancelled; i++) await bite(t, batch[i]); } }
      catch (error) { t.cancelled = true; once(error && error.message || 'La gomme est indisponible.'); }
      finally { t.running = false; }
      return { status: t.cancelled ? 'cancelled' : 'done' };
    }
    function push(t, e) {
      var events = typeof e.getCoalescedEvents === 'function' ? e.getCoalescedEvents() : null;
      (events && events.length ? events : [e]).forEach(function (s) { t.queue.push({ x: s.clientX, y: s.clientY, r: radiusFor(s) }); });
      if (!t.running) { var before = settled; settled = run(t, before); }
    }
    function handle(kind, e) {
      if (retired) return false;
      if (root.state.tool !== 'eraser') { hideRing(); if (drag) cancel(); return false; }
      if (!nativeMode()) return false;
      if (sizer && sizer.active()) return sizer.handle(kind, e);
      if (drag) {
        consume(e);
        if (e.pointerId !== drag.pointer) return true;
        if (kind === 'cancel') { cancel(); return true; }
        if (!holds(drag)) { cancel(); return true; }
        if (kind === 'move') { hover(e); push(drag, e); }
        if (kind === 'up') { var t = drag; drag = null; release(t); t.ended = true; }
        return true;
      }
      if (!stage(e)) return true;
      consume(e);
      if (kind === 'move') { hover(e); return true; }
      if (kind !== 'down' || e.button !== 0 || e.isPrimary === false || !Number.isSafeInteger(e.pointerId)) return true;
      if (e.altKey && sizer) return sizer.begin(e);
      try {
        lastPressure = null; drag = open(e);
        if (!drag) return true;
        var el = root.document.getElementById('canvas-area') || root.document.getElementById('drawing-canvas');
        if (el.setPointerCapture) try { el.setPointerCapture(e.pointerId); } catch (_) { /* a replayed press has no pointer left */ }
        hover(e); push(drag, e);
      } catch (error) { cancel(); notice(error.message || 'La gomme est indisponible.'); }
      return true;
    }
    return Object.freeze({ handle: handle, cancel: cancel, whenSettled: function () { return settled; }, isDragging: function () { return !!drag; }, ringElement: function () { return ring; },
      dispose: function () { retired = true; cancel(); hideRing(); } });
  }
  return Object.freeze({ create: create, capsule: capsule, inRecord: inRecord });
}));
