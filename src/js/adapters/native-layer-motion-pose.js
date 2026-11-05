/* Native Draw / Pen / shape / Fill Brush on a layer that carries Motion (#2372, R-lane-a-fixes). Baseline (1065e9d8 draw-bridge.js, shape-bridge.js,
 * pen-bridge.js `toLocalPoint`, feedback #135; motion.js `layerMotionPointMap`): a layer's Motion Position / Rotation / Scale is applied only at render
 * time and is never baked into the stored geometry, so a pointer on the canvas must be mapped through the INVERSE of the layer's pose at the current
 * frame before the shape is stored. Otherwise the render applies the Motion a second time on top and the fresh shape jumps away from the cursor.
 * The pose is not computed here: the engine reports it with the hit-test listing of the layer (`layerMatrix`, the affine its own Motion and parent chain
 * apply to everything the layer holds, identity for a layer with nothing to pivot on) and the layer row says whether it is needed at all (`hasMotion`).
 * The preview keeps following the cursor in world space; only what is committed is mapped (points through the inverse, handles through its linear part).
 * No document state is held; a pose that cannot be read is a typed refusal, never a shape stored at the unmapped position. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeLayerMotionPose = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var PINS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  var WHY = 'The layer’s Motion transform could not be read — nothing was drawn.';
  function same(a, b) { return !!a && !!b && PINS.every(function (k) { return a[k] === b[k]; }); }
  // The point map and the vector map of the inverse of the 2x3 pose `m` (x' = m0 x + m2 y + m4, y' = m1 x + m3 y + m5).
  function inverse(m) {
    if (!Array.isArray(m) || m.length !== 6 || !m.every(Number.isFinite)) throw new Error(WHY);
    var det = m[0] * m[3] - m[1] * m[2];
    if (!Number.isFinite(det) || Math.abs(det) < 1e-12) throw new Error(WHY);
    function vector(v) { return [(m[3] * v[0] - m[2] * v[1]) / det, (-m[1] * v[0] + m[0] * v[1]) / det]; }
    return Object.freeze({ vector: vector, point: function (p) { return vector([p[0] - m[4], p[1] - m[5]]); }, matrix: m.slice() });
  }
  // True when the layer row says its own Motion is not the identity (the engine's `hasMotion` fact).
  function needs(layer) { return !!layer && layer.hasMotion === true; }
  // The inverse pose of layer `idx` at the frame the gesture began on, read against the pin it began on. Rejects when the listing is not that one.
  function read(root, idx, pin, frame) {
    var I = root.NemoSelectCanvasIntent;
    var done = Promise.resolve().then(function () {
      if (!I || typeof I.layerPaths !== 'function') throw new Error(WHY);
      return I.layerPaths(root, function () { return false; }, false, idx);
    }).then(function (listing) {
      if (!listing || !same(listing.pin, pin) || listing.frame !== frame || !Array.isArray(listing.layerMatrix)) throw new Error(WHY);
      return inverse(listing.layerMatrix);
    });
    done.catch(function () { /* the commit that waits on it reports the refusal */ });
    return done;
  }
  // A copy of the gesture `t` whose start, Pen anchors (points and handles) and freehand samples (x, y; the width stays) are in the layer's own space,
  // and the end point likewise. With no `local` the gesture is returned as it is.
  function localize(t, end, local) {
    if (!local) return { t: t, end: end };
    var u = Object.assign({}, t, { start: local.point(t.start) });
    if (t.anchors) u.anchors = t.anchors.map(function (a) {
      return Object.assign({}, a, { point: local.point(a.point), handleIn: a.handleIn && local.vector(a.handleIn), handleOut: a.handleOut && local.vector(a.handleOut) }); });
    if (t.samples) u.samples = t.samples.map(function (s) { return local.point([s[0], s[1]]).concat(s.slice(2)); });
    return { t: u, end: end && local.point(end) };
  }
  return Object.freeze({ needs: needs, read: read, inverse: inverse, localize: localize, message: WHY });
}));
