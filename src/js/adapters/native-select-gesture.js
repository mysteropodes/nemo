/* Native Select drag gestures (#2372): marquee, move, scale (8 handles) and rotate (ring) on a native object project. The press arrives
 * through select-canvas-intent (which owns the pointer capture and the click rule); this module decides what a drag over the presented
 * layer means, previews it as a disposable SVG (NemoNativeSelectOverlay.preview) and, on release, commits it as exactly ONE native
 * `object.vector.batch` of reshapes (the route the Labs sculpt and knife use), checked against the document pin read when the drag
 * began. A plain stroke is reshaped by its transformed geometry; a ribbon, a fill, a parametric shape and a texture anchor go as one
 * record-space matrix the engine applies (it rebuilds a ribbon from its centreline and re-syncs its linked fill, carries a texture
 * anchor's dabs and a gradient's end points, keeps a shape's box). It holds no document state and writes nothing while the pointer
 * moves: Escape (also between the release and the send), a lost capture, or a pin / frame / tool / layer / selection that no longer matches the press ends the gesture with no write (P25 / #1027). Baseline (1065e9d8 select-bridge onDown/onMove/onUp):
 * A selection may span layers (a marquee reads every visible unlocked layer, a Shift pick adds from another): its move / scale / rotate is still ONE
 * batch, each step naming its own layer. Shift on a corner scales proportionally, Alt scales from the box centre, Shift snaps a rotation to 15 degrees, Shift on the press extends
 * the selection. An image and a text block go in the same batch as a record-space matrix in their own frame (the listing's bounds and
 * matrix): the engine moves the image's stored centre, scales its size and turns its rotation (a mesh bound to it follows) and re-anchors
 * a text block and scales its size, fixed width and spacing as the baseline resize did. An image or a text block takes a move, a scale
 * along its own axes and a turn (#2267: the text block stores a rotation about its anchor); a mirror of either and a shear of either have
 * no stored form. Not carried over: skew zones, a custom anchor, the Alt lasso, ruler snapping. Still refused with a notice: those transforms,
 * bitmap brushes (their image is baked) and a dab whose anchor could not be read. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeSelectGesture = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var DIRS = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'], DRAG = 3, MAX_STEPS = 64, MAX_BYTES = 60000; // under the 65536-byte vector request cap
  var PINS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  // A mask (MASK-NATIVE-REST) is a record like any other: it takes the record-space matrix, keeps its mask tags and its listed mode.
  var MAPPED = ['plain', 'ribbon', 'fill', 'shape', 'brush', 'raster', 'text', 'mask'];
  function samePin(a, b) { return !!a && !!b && PINS.every(function (k) { return a[k] === b[k]; }); }
  function notice(root, text) { if (typeof root.showToast === 'function') try { root.showToast(text); } catch (_) {} }
  // The selection's items that have no native transform here, as labels (null when every one can be moved, scaled and rotated).
  function unsupported(paths) {
    var found = [];
    paths.forEach(function (p) {
      var d = p.data || {}, label = d.nativeBrush === 'bitmap' ? 'bitmap brushes' : d.nativeBrush === 'copy' ? 'brush dabs' : MAPPED.indexOf(d.nativeRole) < 0 ? 'this item' : null;
      if (label && found.indexOf(label) < 0) found.push(label);
    });
    return found.length ? found.join(', ') : null;
  }
  // The transform an image or a text block has no stored form for, as a label (null when every one can take `m`, the world affine): the
  // matrix is read in each item's own frame (the presented matrix conjugated), where an image takes a move, a scale along its axes and a
  // turn (text the same, #2267).
  // `m` is one world affine for every item, or (align / distribute, #2372) a function giving each item its own, or null to leave it out.
  function matrixFor(m, p) { return typeof m === 'function' ? m(p) : m; }
  function unstorable(root, paths, m) {
    var conj = root.NemoSelectCanvasIntent && root.NemoSelectCanvasIntent.inRecordSpace, found = [];
    paths.forEach(function (p) {
      var role = p.data && p.data.nativeRole, own = matrixFor(m, p), r = own && (role === 'raster' || role === 'text') && conj ? conj(p.data.nativeMatrix, own) : null;
      if (!r) return;
      var det = r[0] * r[3] - r[1] * r[2];
      if (det < 0) found.push(role === 'text' ? 'Flipping text' : 'Flipping images');
      else if (Math.abs(r[0] * r[2] + r[1] * r[3]) > 1e-9 * Math.hypot(r[0], r[1]) * Math.hypot(r[2], r[3])) found.push(role === 'text' ? 'Skewing text (scale along its own axes)' : 'Skewing images (scale along their own axes)');
    });
    return found.length ? found.filter(function (l, i) { return found.indexOf(l) === i; }).join(', ') : null;
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
    // A handle drag scales along the box's OWN axes (`g.axis` degrees, 0 for an unturned box): the pointer and handle offsets are read in the
    // de-rotated frame and the world affine is R(axis) * diag(sx, sy) * R(-axis) about the anchor.
    var i = g.index, h = g.box.handles[i], dir = g.dir, sx = 1, sy = 1, th = (g.axis || 0) * Math.PI / 180, co = Math.cos(th), si = Math.sin(th);
    var anchor = alt ? [g.box.center.x, g.box.center.y] : g.box.handles[(i + 4) % 8], ax = anchor[0], ay = anchor[1];
    var odx = (h[0] - ax) * co + (h[1] - ay) * si, ody = -(h[0] - ax) * si + (h[1] - ay) * co;
    var cdx = (at.x - ax) * co + (at.y - ay) * si, cdy = -(at.x - ax) * si + (at.y - ay) * co;
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
    var la = sx * co * co + sy * si * si, lb = (sx - sy) * co * si, ld = sx * si * si + sy * co * co;
    return [la, lb, lb, ld, ax - la * ax - lb * ay, ay - lb * ax - ld * ay];
  }
  var identity = function (m) { return Math.abs(m[0] - 1) + Math.abs(m[1]) + Math.abs(m[2]) + Math.abs(m[3] - 1) + Math.abs(m[4]) + Math.abs(m[5]) < 1e-9; };
  // The committed strokes as fresh presented paths (the old transient ones hold the geometry from before). A vector record's box angle comes
  // back from the engine's listing (the turn was stored in the same batch); an image or text block has no stored one, so it keeps the session's.
  async function reselect(root, sel, angle) {
    var I = root.NemoSelectCanvasIntent, angles = {}, fresh = null;
    sel.forEach(function (p) { angles[p.data.strokeId] = (p.data.boxAngle || 0) + (angle || 0); });
    for (var tries = 0; tries < 2 && !fresh; tries++) {
      if (tries) await new Promise(function (done) { (root.setTimeout || setTimeout)(done, 120); });
      try { fresh = await I.reread(root, sel, false); } catch (_) { fresh = null; }
    }
    root.selectedPaths = fresh ? fresh.paths.map(function (p) { if (angles[p.data.strokeId] && (p.data.nativeRole === 'raster' || p.data.nativeRole === 'text')) p.data.boxAngle = angles[p.data.strokeId] % 360; return p; }) : [];
    root.state.selectedStrokeIndices = [];
  }
  // ONE native batch of reshapes for the whole selection `sel` (its items may sit on several layers: every step names its own layer), the world
  // affine `m` in each record's own space, against the document `o.pin` names. Used by the drag release and by the inspector's Transform fields
  // and the arrow-key nudge (#2372 insp-transform-fields). `o`: { pin, gone (the notice for a document that moved on), angle (degrees of a turn, for a box angle that has no stored form), stale()
  // (true when the document, frame, layer or selection moved on), cancelled() (Escape between the release and the send), release() (before the
  // send) }. `m` may be a function of the item (see matrixFor): the steps are then one per item that has a matrix. Resolves to 'applied', or to the refusal ('refused' / 'stale' / 'failed' / 'cancelled'); a refusal has already toasted.
  async function commitMatrix(root, sel, m, o) {
    o = o || {};
    var gone = o.gone || 'The document changed during the drag \u2014 nothing was changed.';
    var I = root.NemoSelectCanvasIntent, bad = unsupported(sel), ids = sel.map(function (p) { return p.data.strokeId; }), unread = sel.some(function (p) { return !Number.isInteger(p.data.nativeLayerIndex); });
    if (bad) { notice(root, 'Transforming ' + bad + ' is not available on native projects yet.'); return 'refused'; }
    if (!ids.length || unread) { notice(root, 'Select items to transform them.'); return 'refused'; }
    if (ids.length > MAX_STEPS) { notice(root, 'Too many items to transform at once (' + MAX_STEPS + ' at most).'); return 'refused'; }
    var cannot = unstorable(root, sel, m), route = root.NemoNativeObjectLabs && root.NemoNativeObjectLabs.desktop(root);
    if (cannot) { notice(root, cannot + ' is not available on native projects yet.'); return 'refused'; }
    if (o.stale && o.stale()) { notice(root, gone); return 'stale'; }
    if (!route) { notice(root, 'Transforming is not available right now.'); return 'refused'; }
    var layers = await I.reread(root, sel, false), again = I.presentedState(root);
    if (!layers || !layers.complete || !again || !samePin(again.pin, o.pin)) { notice(root, gone); return 'stale'; }
    var result, steps;
    try { steps = I.transformSteps(root, layers.paths, m); } catch (_) { steps = null; }
    if (steps && !steps.length) return 'noop'; // every item's matrix was null: nothing to write
    if (steps && JSON.stringify(steps).length > MAX_BYTES) { notice(root, 'Too much to transform at once \u2014 select fewer items.'); return 'refused'; }
    if (o.cancelled && o.cancelled()) return 'cancelled'; // Escape between the release and the send still cancels; once sent, the write is not cancellable (undo reverts it)
    if (o.release) o.release();
    try { result = steps ? await route.batchStrokes(layers.layerUid, layers.frame, steps, { select: true }) : null; } catch (_) { result = null; }
    if (!result || result.status !== 'applied') { notice(root, 'The transform could not be applied.'); return 'failed'; }
    await reselect(root, sel, o.angle);
    return 'applied';
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
    // The box, handles and ring exist only while the overlay draws them for THIS context: a selection that outlived a frame, layer or tool
    // change (nothing else clears root.selectedPaths on a native project) has invisible handles, so it is dropped rather than grabbed.
    if (current.length && !(O && typeof O.selectionContext === 'function' && O.selectionContext(root) === I.pickContext(root))) {
      root.selectedPaths = []; if (st.selectedStrokeIndices) st.selectedStrokeIndices = []; current = [];
      if (O) O.show(root);
    }
    var box = O && current.length ? O.boxOf(root, current) : null, handle = box && hitHandle(box, origin, wpp);
    if (handle) {
      g.kind = handle.type; g.sel = current; g.box = box; g.dir = handle.dir; g.index = handle.index; g.ctx = I.pickContext(root);
      // The box's own axes. A dynamic shape keeps the world-axis resize its engine kernel (and the baseline) applies: it refuses a skewed one.
      g.axis = current.some(function (p) { return p.data.nativeRole === 'shape'; }) ? 0 : box.angle || 0;
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
      var cannot = unstorable(root, g.sel, g.matrix);
      if (cannot) { g.kind = 'none'; if (g.previewed) { O.preview(root, null); g.previewed = false; } notice(root, cannot + ' is not available on native projects yet.'); return; }
      var m = new Matrix(g.matrix[0], g.matrix[1], g.matrix[2], g.matrix[3], g.matrix[4], g.matrix[5]);
      O.preview(root, { paths: g.sel.map(function (p) {
        var c = p.clone({ insert: false });
        c.data = Object.assign({}, p.data, g.kind === 'rotate' ? { boxAngle: (((p.data.boxAngle || 0) + g.angle) % 360) } : {});
        c.transform(m);
        return c;
      }) });
      g.previewed = true;
    }
    // P25 (#1027): a move / scale / rotate whose document pin, frame, tool, layer or selection no longer matches the snapshot taken at the press
    // ends there, with a notice and no write (it was only ever a preview); the release then has nothing to commit.
    function ids(list) { return (list || []).filter(function (p) { return p && p.data && p.data.nativeStableTarget; }).map(function (p) { return p.data.strokeId; }).join('|'); }
    function stale() {
      var now = I.presentedState(root);
      return !now || !samePin(now.pin, g.pin) || now.presentation.frame !== g.frame || (!!g.ctx && I.pickContext(root) !== g.ctx) || ids(root.selectedPaths) !== ids(g.sel);
    }
    function abandon() {
      g.kind = 'none';
      if (g.previewed && O) { O.preview(root, null); g.previewed = false; }
      notice(root, 'The document changed during the drag — nothing was changed.');
    }
    function move(ev) {
      if (g.cancelled) return;
      g.last = ev;
      if (!g.active) {
        if (Math.hypot(ev.clientX - e.clientX, ev.clientY - e.clientY) <= DRAG) return;
        g.active = true; listen(true);
      }
      if (!g.kind) classify();
      if ((g.kind === 'move' || g.kind === 'scale' || g.kind === 'rotate') && stale()) return abandon();
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
    // The release commits through commitMatrix (shared with the inspector's Transform fields and the arrow-key nudge).
    function commit() {
      return commitMatrix(root, g.sel, g.matrix, { pin: g.pin, stale: stale, cancelled: function () { return g.cancelled; }, release: function () { listen(false); }, angle: g.kind === 'rotate' ? g.angle : 0 });
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
        if (O) { O.show(root); O.preview(root, null); }
        g.cancelled = true;
        if (typeof root.updateUI === 'function') root.updateUI();
        if (root.SMEngineBridge && typeof root.SMEngineBridge.renderNow === 'function') root.SMEngineBridge.renderNow();
      }
    }
    return { move: move, end: end, cancel: cancel, owns: function () { return !!handle; } };
  }
  function bind(root) { return Object.freeze({ begin: function (e, abort) { return begin(root, e, abort); } }); }
  return Object.freeze({ bind: bind, begin: begin, hitHandle: hitHandle, matrixOf: matrixOf, commitMatrix: commitMatrix, unsupported: unsupported, unstorable: unstorable });
}));
