/* Native Select drag gestures (#2372): marquee, move, scale (8 handles) and rotate (ring) on a native object project. The press arrives
 * through select-canvas-intent (which owns the pointer capture and the click rule); this module decides what a drag over the presented
 * layer means, previews it as a disposable SVG (NemoNativeSelectOverlay.preview) and, on release, commits it as exactly ONE native
 * `object.vector.batch` of reshapes (the route the Labs sculpt and knife use), checked against the document pin read when the drag
 * began. It holds no document state and writes nothing while the pointer moves: Escape, a lost capture or a stale document ends the
 * gesture with no write. Baseline (1065e9d8 select-bridge onDown/onMove/onUp): Shift on a corner scales proportionally, Alt scales from
 * the box centre, Shift snaps a rotation to 15 degrees, Shift on the press extends the selection. Not carried over: skew zones, a custom
 * anchor, the Alt lasso, ruler snapping. Images, text, parametric shapes and brushes have no readable native transform here, so a drag
 * that includes one is refused with a notice (images have move/resize/rotate item commands but the stored rect is not readable). */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeSelectGesture = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var DIRS = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'], DRAG = 3, MAX_STEPS = 64;
  var PINS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  var LABEL = { raster: 'images', text: 'text', shape: 'shapes', brush: 'brushes', fill: 'fills' };
  function samePin(a, b) { return !!a && !!b && PINS.every(function (k) { return a[k] === b[k]; }); }
  function notice(root, text) { if (typeof root.showToast === 'function') try { root.showToast(text); } catch (_) {} }
  // The selection's items that have no native transform here, as labels (null when every one can be moved, scaled and rotated).
  function unsupported(paths) {
    var found = [];
    paths.forEach(function (p) {
      var role = p.data && p.data.nativeRole;
      if (role !== 'plain' && role !== 'ribbon' && found.indexOf(LABEL[role] || 'this item') < 0) found.push(LABEL[role] || 'this item');
    });
    return found.length ? found.join(', ') : null;
  }
  // Which handle of the oriented box a world point grabs: the rotate ring first (a band of 7 screen px), then the 8 scale handles
  // (9 px), as the baseline hitHandles did. `wpp` is world units per screen pixel.
  function hitHandle(box, p, wpp) {
    var ring = Math.min(36 * wpp, Math.max(box.width, box.height) * 0.3);
    if (Math.abs(Math.hypot(p.x - box.center.x, p.y - box.center.y) - ring) < 7 * wpp) return { type: 'rotate' };
    var best = null, bestD = 9 * wpp;
    box.handles.forEach(function (h, i) { var d = Math.hypot(p.x - h[0], p.y - h[1]); if (d < bestD) { bestD = d; best = { type: 'scale', dir: DIRS[i], index: i }; } });
    return best;
  }
  // The world affine [a b c d e f] of a drag from `g.start` to the world point `at` (baseline onMove rules).
  function matrixOf(g, at, shift, alt) {
    if (g.kind === 'move') return [1, 0, 0, 1, at.x - g.start.x, at.y - g.start.y];
    if (g.kind === 'rotate') {
      var c = g.box.center, delta = Math.atan2(at.y - c.y, at.x - c.x) * 180 / Math.PI - g.startAngle;
      if (shift) delta = Math.round(delta / 15) * 15;
      var r = delta * Math.PI / 180, cos = Math.cos(r), sin = Math.sin(r);
      g.angle = delta;
      return [cos, sin, -sin, cos, c.x - cos * c.x + sin * c.y, c.y - sin * c.x - cos * c.y];
    }
    var i = g.index, h = g.box.handles[i], dir = g.dir, sx = 1, sy = 1;
    var anchor = alt ? [g.box.center.x, g.box.center.y] : g.box.handles[(i + 4) % 8], ax = anchor[0], ay = anchor[1], odx = h[0] - ax, ody = h[1] - ay;
    var cdx = at.x - ax, cdy = at.y - ay;
    if (dir.length === 2) {
      sx = odx !== 0 ? cdx / odx : 1; sy = ody !== 0 ? cdy / ody : 1;
      if (shift) {
        var u = Math.hypot(odx, ody) !== 0 ? Math.hypot(cdx, cdy) / Math.hypot(odx, ody) : 1;
        sx = u * (sx < 0 ? -1 : 1); sy = u * (sy < 0 ? -1 : 1);
      }
    } else if (dir === 'n' || dir === 's') sy = ody !== 0 ? cdy / ody : 1;
    else sx = odx !== 0 ? cdx / odx : 1;
    if (Math.abs(sx) < 0.05) sx = sx < 0 ? -0.05 : 0.05;
    if (Math.abs(sy) < 0.05) sy = sy < 0 ? -0.05 : 0.05;
    return [sx, 0, 0, sy, ax - sx * ax, ay - sy * ay];
  }
  var identity = function (m) { return Math.abs(m[0] - 1) + Math.abs(m[1]) + Math.abs(m[2]) + Math.abs(m[3] - 1) + Math.abs(m[4]) + Math.abs(m[5]) < 1e-9; };
  // A ribbon's world centreline through the affine; its width follows the mean axis scale (baseline scaleCenterSegments).
  function mapCenter(segs, m) {
    var k = (Math.hypot(m[0], m[1]) + Math.hypot(m[2], m[3])) / 2, lin = function (v) { return [m[0] * v[0] + m[2] * v[1], m[1] * v[0] + m[3] * v[1]]; };
    segs.forEach(function (s) {
      s.point = [m[0] * s.point[0] + m[2] * s.point[1] + m[4], m[1] * s.point[0] + m[3] * s.point[1] + m[5]];
      s.handleIn = lin(s.handleIn); s.handleOut = lin(s.handleOut);
      if (s.width !== undefined) s.width *= k;
    });
  }

  function begin(root, e, abort) {
    var I = root.NemoSelectCanvasIntent, O = root.NemoNativeSelectOverlay, st = root.state, shown = I && st && st.tool === 'select' && st.appMode !== 'motion' && I.presentedState(root);
    if (!shown) return null;
    var world = function (x, y) { return I.worldPoint(root, x, y, shown.pin, shown.presentation); };
    var Matrix = root.Matrix || root.paper.Matrix, origin = world(e.clientX, e.clientY), next = world(e.clientX + 1, e.clientY);
    var wpp = Math.hypot(next.x - origin.x, next.y - origin.y) || origin.perPixel;
    var g = { kind: null, active: false, cancelled: false, start: origin, shift: !!e.shiftKey, last: e, pin: shown.pin, frame: shown.presentation.frame,
      sel: [], box: null, angle: 0, ctx: null, matrix: [1, 0, 0, 1, 0, 0], previewed: false };
    var picked = null, pickDone = null;
    var current = (root.selectedPaths || []).filter(function (p) { return p && p.data && p.data.nativeStableTarget; });
    var box = O && current.length ? O.boxOf(root, current) : null, handle = box && hitHandle(box, origin, wpp);
    if (handle) {
      g.kind = handle.type; g.sel = current; g.box = box; g.dir = handle.dir; g.index = handle.index; g.ctx = I.pickContext(root);
      g.startAngle = Math.atan2(origin.y - box.center.y, origin.x - box.center.x) * 180 / Math.PI;
    } else {
      pickDone = I.pick(root, e.clientX, e.clientY).then(function (item) { picked = { item: item }; }, function () { picked = { item: undefined }; }).then(function () {
        if (g.active && !g.kind && !g.cancelled) { classify(); if (g.kind) update(); } // the drag outran the read: it starts now
      });
    }
    function listen(on) {
      var d = root.document;
      if (d && typeof d.addEventListener === 'function') d[on ? 'addEventListener' : 'removeEventListener']('keydown', onKey, true);
    }
    function onKey(k) {
      if (k.key !== 'Escape') return;
      if (g.active) { k.stopImmediatePropagation(); k.preventDefault(); }
      cancel(); if (abort) abort();
    }
    function cancel() {
      if (g.cancelled) return;
      g.cancelled = true; listen(false);
      if (g.previewed && O) O.preview(root, null);
    }
    // What the press means once the drag is real: a press on nothing drags a marquee, a press on an item moves it (selecting it first
    // unless it is already selected). A pick that could not be read ends the gesture with a notice.
    function classify() {
      if (g.kind || !picked) return;
      var item = picked.item, selected = root.selectedPaths || [];
      if (item === undefined) { g.kind = 'none'; notice(root, 'Could not read the canvas just now — try again.'); return; }
      if (item === null) { I.applyPick(root, null, g.shift); g.kind = 'marquee'; g.ctx = I.pickContext(root); return; }
      if (!selected.some(function (p) { return p && p.data && p.data.strokeId === item.data.strokeId; })) I.applyPick(root, item, g.shift);
      g.kind = 'move'; g.sel = (root.selectedPaths || []).filter(function (p) { return p && p.data && p.data.nativeStableTarget; }); g.ctx = I.pickContext(root);
    }
    // The drag's live preview: the selection's transformed copies (box, handles and ring follow) or the marquee rectangle.
    function update() {
      if (g.cancelled || !O) return;
      var at = world(g.last.clientX, g.last.clientY);
      if (g.kind === 'marquee') { g.rect = [Math.min(g.start.x, at.x), Math.min(g.start.y, at.y), Math.max(g.start.x, at.x), Math.max(g.start.y, at.y)]; O.preview(root, { marquee: g.rect }); g.previewed = true; return; }
      if (g.kind !== 'move' && g.kind !== 'scale' && g.kind !== 'rotate') return;
      var bad = unsupported(g.sel);
      if (bad) { g.kind = 'none'; notice(root, 'Transforming ' + bad + ' is not available on native projects yet.'); return; }
      g.matrix = matrixOf(g, at, !!g.last.shiftKey, !!g.last.altKey);
      var m = new Matrix(g.matrix[0], g.matrix[1], g.matrix[2], g.matrix[3], g.matrix[4], g.matrix[5]);
      O.preview(root, { paths: g.sel.map(function (p) {
        var c = p.clone({ insert: false });
        c.data = Object.assign({}, p.data, g.kind === 'rotate' ? { boxAngle: (((p.data.boxAngle || 0) + g.angle) % 360) } : {});
        c.transform(m);
        return c;
      }) });
      g.previewed = true;
    }
    function move(ev) {
      if (g.cancelled) return;
      g.last = ev;
      if (!g.active) {
        if (Math.hypot(ev.clientX - e.clientX, ev.clientY - e.clientY) <= DRAG) return;
        g.active = true; listen(true);
      }
      if (!g.kind) classify();
      if (g.kind) update();
    }
    async function marqueeEnd() {
      if (I.pickContext(root) !== g.ctx || !g.rect) return;
      var found = await I.marquee(root, g.rect);
      if (!found) return notice(root, 'Could not read the canvas just now — try again.');
      if (I.pickContext(root) !== g.ctx) return;
      var held = g.shift ? (root.selectedPaths || []) : [];
      root.selectedPaths = held.concat(found.filter(function (p) { return !held.some(function (q) { return q.data.strokeId === p.data.strokeId; }); }));
      root.state.selectedStrokeIndices = [];
      if (O) O.show(root);
      if (typeof root.updateUI === 'function') root.updateUI();
    }
    // ONE native batch of reshapes for the whole selection, against the document the drag began on.
    async function commit() {
      var bad = unsupported(g.sel), ids = g.sel.map(function (p) { return p.data.strokeId; }), layerIdx = g.sel.length ? g.sel[0].data.nativeLayerIndex : undefined;
      if (bad) return notice(root, 'Transforming ' + bad + ' is not available on native projects yet.');
      if (!ids.length || layerIdx === undefined || g.sel.some(function (p) { return p.data.nativeLayerIndex !== layerIdx; })) return notice(root, 'Select items of one layer to transform them.');
      if (ids.length > MAX_STEPS) return notice(root, 'Too many items to transform at once (' + MAX_STEPS + ' at most).');
      var m = g.matrix, now = I.presentedState(root), route = root.NemoNativeObjectLabs && root.NemoNativeObjectLabs.desktop(root);
      if (!now || !samePin(now.pin, g.pin) || now.presentation.frame !== g.frame || I.pickContext(root) !== g.ctx) return notice(root, 'The document changed during the drag — nothing was changed.');
      if (!route) return notice(root, 'Transforming is not available right now.');
      var layers = await I.layerPaths(root, function (item) { return ids.indexOf(item.stableTarget.strokeId) >= 0; }, true, layerIdx);
      var again = I.presentedState(root);
      if (!layers || layers.paths.length !== ids.length || !again || !samePin(again.pin, g.pin)) return notice(root, 'The document changed during the drag — nothing was changed.');
      var matrix = new Matrix(m[0], m[1], m[2], m[3], m[4], m[5]);
      layers.paths.forEach(function (p) { p.transform(matrix); if (p.data.centerSegments) mapCenter(p.data.centerSegments, m); });
      var result;
      try { result = await route.batchStrokes(layers.layerUid, layers.frame, I.reshapeSteps(layers.paths)); } catch (_) { result = null; }
      if (!result || result.status !== 'applied') return notice(root, 'The transform could not be applied.');
      await reselect(ids, layerIdx);
    }
    // The committed strokes as fresh presented paths (the old transient ones hold the geometry from before), keeping each box angle.
    async function reselect(ids, layerIdx) {
      var angles = {}, fresh = null;
      g.sel.forEach(function (p) { angles[p.data.strokeId] = (p.data.boxAngle || 0) + (g.kind === 'rotate' ? g.angle : 0); });
      for (var tries = 0; tries < 2 && !fresh; tries++) {
        if (tries) await new Promise(function (done) { (root.setTimeout || setTimeout)(done, 120); });
        try { fresh = await I.layerPaths(root, function (item) { return ids.indexOf(item.stableTarget.strokeId) >= 0; }, false, layerIdx); } catch (_) { fresh = null; }
      }
      root.selectedPaths = fresh ? fresh.paths.sort(function (a, b) { return ids.indexOf(a.data.strokeId) - ids.indexOf(b.data.strokeId); })
        .map(function (p) { if (angles[p.data.strokeId]) p.data.boxAngle = angles[p.data.strokeId] % 360; return p; }) : [];
      root.state.selectedStrokeIndices = [];
    }
    async function end(ev) {
      if (g.cancelled || !g.active) return cancel();
      g.last = ev;
      try {
        if (!g.kind && pickDone) { await pickDone; if (g.cancelled) return; classify(); }
        if (g.kind === 'marquee') { update(); await marqueeEnd(); }
        else if (g.kind === 'move' || g.kind === 'scale' || g.kind === 'rotate') {
          update();
          if (g.kind && !identity(g.matrix) && !g.cancelled) await commit();
        }
      } catch (_) { notice(root, 'The transform could not be applied.'); }
      finally {
        listen(false);
        if (O && !g.cancelled) { O.show(root); O.preview(root, null); }
        g.cancelled = true;
        if (typeof root.updateUI === 'function') root.updateUI();
        if (root.SMEngineBridge && typeof root.SMEngineBridge.renderNow === 'function') root.SMEngineBridge.renderNow();
      }
    }
    return { move: move, end: end, cancel: cancel, owns: function () { return !!handle; } };
  }
  function bind(root) { return Object.freeze({ begin: function (e, abort) { return begin(root, e, abort); } }); }
  return Object.freeze({ bind: bind, begin: begin, hitHandle: hitHandle, matrixOf: matrixOf, unsupported: unsupported });
}));
