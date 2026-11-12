/* B-pen-continue: the Pen continues an existing OPEN path from its endpoint instead of always starting a new object.
 * Baseline 1065e9d8 pen-bridge.js onDown (~133-160): on the first press, an open Path of the active layer whose first or last point lies within
 * 10 / view.zoom (10 screen pixels) of the pointer is the path the gesture continues (the nearest endpoint wins, the earlier path on a tie, first
 * before last); pressing at its FIRST point reverses it, so every further anchor is appended at the end. A vector-brush ribbon and a Fill Brush
 * shape are never continued (here also: a closed path, a texture anchor, a mask, a linked fill, an image, a text block: only role "plain").
 * The path is found in the presented listing of the active layer (readPresentedStrokes, the read Select picks with) and the extended geometry is
 * committed as ONE batch step on that record, so the stroke keeps its id, style, fill and order and the edit is one revision and one history entry:
 * a reshape of its geometry, or, with Taper Ends on (finalizePen replaces the path by a tapered ribbon of the current style), a replace.
 * A held frame is keyed with a copy in Animation 2D and edited in place in Motion, as every Select edit is (native-object-labs.js batchStrokes).
 * Pointer state only; it writes through the vector client the Pen already uses. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativePenContinue = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var TOLERANCE_PX = 10;
  function apply(m, p) { return [m[0] * p[0] + m[2] * p[1] + m[4], m[1] * p[0] + m[3] * p[1] + m[5]]; }
  function linear(m, v) { return [m[0] * v[0] + m[2] * v[1], m[1] * v[0] + m[3] * v[1]]; }
  function inverse(m) {
    var d = m[0] * m[3] - m[1] * m[2];
    if (!Number.isFinite(d) || Math.abs(d) < 1e-12) throw new Error('The path’s transform cannot be inverted.');
    var v = function (u) { return [(m[3] * u[0] - m[2] * u[1]) / d, (-m[1] * u[0] + m[0] * u[1]) / d]; };
    return { vector: v, point: function (p) { return v([p[0] - m[4], p[1] - m[5]]); } };
  }
  function identity(m) { return [1, 0, 0, 1, 0, 0].every(function (n, i) { return Math.abs(m[i] - n) < 1e-9; }); }
  function xy(a) { return { x: a[0], y: a[1] }; }
  // The listed records a Pen press may continue: an open plain stroke with its geometry paged (role "plain" excludes ribbons, fill shapes,
  // linked fills, texture anchors and copies, masks, images and text).
  function continuable(item) { return !!item && item.role === 'plain' && !item.brush && item.closed === false && Array.isArray(item.segments) && item.segments.length >= 2 && Array.isArray(item.matrix); }
  function reversed(segments) {
    return segments.slice().reverse().map(function (s) { return { point: s.point, handleIn: s.handleOut, handleOut: s.handleIn }; });
  }
  // The nearest endpoint within `tol` of `pt` (document units) over the listed records in order: first before last, strict so the earlier wins a tie.
  function nearest(items, pt, tol) {
    var best = null, bestD = tol;
    items.filter(continuable).forEach(function (item) {
      var first = apply(item.matrix, item.segments[0].point), last = apply(item.matrix, item.segments[item.segments.length - 1].point);
      var df = Math.hypot(pt[0] - first[0], pt[1] - first[1]), dl = Math.hypot(pt[0] - last[0], pt[1] - last[1]);
      if (df < bestD) { bestD = df; best = { item: item, end: 'first' }; }
      if (dl < bestD) { bestD = dl; best = { item: item, end: 'last' }; }
    });
    return best;
  }
  function toClient(Mapping, t, p) {
    var m = Mapping.transform({ presentation: t.presentation }, false), b = t.bounds;
    return [b.left + (m[0] * p[0] + m[2] * p[1] + m[4]) * b.width / t.pin.canvasW, b.top + (m[1] * p[0] + m[3] * p[1] + m[5]) * b.height / t.pin.canvasH];
  }
  // The found path as Pen anchors in document space, oriented so the pressed end is last (a press at the first point reverses the path).
  function anchors(Mapping, t, found) {
    var m = found.item.matrix, segs = found.end === 'first' ? reversed(found.item.segments) : found.item.segments;
    return segs.map(function (s) {
      var a = { point: apply(m, s.point), handleIn: linear(m, s.handleIn), handleOut: linear(m, s.handleOut), rec: s };
      a.client = toClient(Mapping, t, a.point); a.was = JSON.stringify([a.point, a.handleIn, a.handleOut]); return a;
    });
  }
  // The gesture's first anchor becomes the path's endpoint (it keeps its identity, so a handle drag in flight still reshapes it); the rest of the path precedes it.
  function merge(Mapping, t, found) {
    var list = anchors(Mapping, t, found), last = list[list.length - 1], g = t.anchors[0];
    var dragged = g.handleIn !== undefined || g.handleOut !== undefined;
    g.point = last.point; g.client = last.client; g.rec = last.rec;
    if (!dragged) { g.handleIn = last.handleIn; g.handleOut = last.handleOut; }
    g.was = JSON.stringify([last.point, last.handleIn, last.handleOut]);
    t.anchors.splice.apply(t.anchors, [0, 0].concat(list.slice(0, -1)));
    t.cont = Object.assign(t.cont, { ready: true, found: found, count: list.length, matrix: found.item.matrix.slice() });
    return t.cont;
  }
  // Starts the search for the path under the first press. Never throws: a project whose listing is unreadable simply starts a new path, as before.
  function begin(host, t, Mapping) {
    var session = t.session, c = { ready: false };
    t.cont = c;
    if (!session || typeof session.readPresentedStrokes !== 'function' || !Mapping || typeof Mapping.transform !== 'function') return null;
    var scale = t.scale || t.bounds.width / t.pin.canvasW, tol = TOLERANCE_PX / scale;
    c.promise = Promise.resolve().then(function () {
      return session.readPresentedStrokes({ contextId: t.context, layerUid: t.layerUid, frame: t.presentation.frame }, t.pin, continuable);
    }).then(function (listing) {
      if (t.cancelled || !listing || listing.presentation.frame !== t.frame) return null;
      var found = nearest(listing.items, t.start, tol);
      return found && t.anchors && t.anchors.length ? merge(Mapping, t, found) : null;
    }).catch(function () { return null; });
    // The commit waits for the search, whatever the Motion pose read is doing.
    var prior = t.pose;
    t.pose = (prior || Promise.resolve(null)).then(function (local) { return c.promise.then(function () { return local; }); });
    return c.promise;
  }
  function touched(a) { return !a.rec || JSON.stringify([a.point, a.handleIn || [0, 0], a.handleOut || [0, 0]]) !== a.was; }
  // The merged anchors in the path's own record space: an untouched stored anchor is passed through unchanged, an edited or new one is mapped by the inverse.
  function geometry(t) {
    var inv = inverse(t.cont.matrix);
    return { kind: 'cubic-path', closed: !!t.closed, coordinateSpace: 'document-world', handleSpace: 'relative', segments: t.anchors.map(function (a) {
      if (!touched(a)) return { point: xy(a.rec.point), handleIn: xy(a.rec.handleIn), handleOut: xy(a.rec.handleOut) };
      return { point: xy(inv.point(a.point)), handleIn: xy(inv.vector(a.handleIn || [0, 0])), handleOut: xy(inv.vector(a.handleOut || [0, 0])) };
    }) };
  }
  // Nothing to write when the press only touched the endpoint: no anchor was added, the path was not closed and no handle moved.
  function unchanged(t) { return !t.closed && t.anchors.length === t.cont.count && !t.anchors.some(touched); }
  // `dict` is the Pen dictionary of the merged anchors in the current style (a tapered ribbon with Taper Ends on). `vector` is the vector client bound to the gesture.
  function commit(host, vector, t, dict) {
    var c = t.cont, target = c.found.item.stableTarget;
    if (unchanged(t)) return Promise.resolve({ status: 'cancelled' });
    var motion = !!host.state && host.state.appMode === 'motion', at = motion && Number.isInteger(target.frameScope && target.frameScope.frame) ? target.frameScope.frame : t.frame;
    var options = Object.assign({ origin: t.pin }, motion ? {} : { keyframe: 'copy' }), step;
    if (dict && dict.isVectorBrush) {
      // finalizePen replaces the path by the tapered ribbon of the current style: a replace of the same stroke id (identity matrix only: the ribbon is stored in document space).
      if (!identity(c.matrix)) { t.refusal = 'Continuing a path with tapered ends is not available on a moved layer.'; return Promise.resolve({ status: 'unavailable', reason: 'taper_moved_layer' }); }
      step = { replace: { strokeId: target.strokeId, dict: dict }, frame: at };
    } else step = { reshape: { stableTarget: target, geometry: geometry(t) }, frame: at };
    return vector.batchStrokes(t.layerUid, t.frame, [step], options);
  }
  return Object.freeze({ TOLERANCE_PX: TOLERANCE_PX, continuable: continuable, nearest: nearest, anchors: anchors, begin: begin, merge: merge, geometry: geometry, unchanged: unchanged, commit: commit });
}));
