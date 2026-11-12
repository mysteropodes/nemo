/* Motion on a native object project: the box of a TEXT layer (#2520/#2525, C-motion-canvas-4). The native `layerBounds` read answers vector, image and rig
 * geometry; the glyph-run bounds live in the render-scene text join, which a property read cannot import, so the read refuses a text layer. The hit-test
 * listing of the same layer already carries them: a text item's `bounds` are the glyph contours' extent mapped back through the layer's chain, i.e. the
 * layer's own bounds BEFORE its Motion, which is exactly what the box is drawn about (baseline motion.js@1065e9d8 motionBoxGeom takes `userLayers[li].bounds`).
 * This is a fallback taken only after that refusal, only for an UNPARENTED layer (the listing's pose folds the ancestors in, so with a parent the outer
 * affine could not be told apart), and only when every text item is untouched by a block turn or mirror (its matrix is then the layer's pose; a turned block
 * lists its own frame, which is not the layer's). Anything else stays refused, with the notice the caller already shows. No document state, no writes. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeMotionTextBounds = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var IDENTITY = [1, 0, 0, 1, 0, 0], PINS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  function near(a, b) { return Array.isArray(a) && Array.isArray(b) && a.length === 6 && b.length === 6 && a.every(function (v, i) { return Math.abs(v - b[i]) < 1e-6 * (1 + Math.abs(v)); }); }
  // { bounds: [x0, y0, x1, y1], parentAffine } for the layer, or null when this fallback does not apply. `proof` is the strict presentation proof of the canvas.
  async function read(win, proof, layer) {
    var I = win.NemoSelectCanvasIntent;
    if (!I || !layer || layer.parentLayerUid || typeof I.presented !== 'function' || typeof I.layerList !== 'function') return null;
    var index = I.layerList(win, proof.info).findIndex(function (l) { return l && l.layerUid === layer.layerUid; });
    if (index < 0) return null;
    var shown = await I.presented(win, index, function () { return false; }, 0, false);
    if (!shown || shown.frame !== proof.selector.frame || !PINS.every(function (k) { return shown.pin[k] === proof.info[k]; })) return null;
    var texts = shown.items.filter(function (item) { return item.role === 'text' && item.stableTarget && item.stableTarget.layerUid === layer.layerUid; });
    if (!texts.length || shown.items.some(function (item) { return item.role !== 'text'; })) return null;
    var pose = shown.layerMatrix || IDENTITY;
    if (!texts.every(function (item) { return near(item.matrix, pose) && Array.isArray(item.bounds) && item.bounds.length === 4 && item.bounds.every(Number.isFinite); })) return null;
    var b = texts.reduce(function (u, item) { return [Math.min(u[0], item.bounds[0]), Math.min(u[1], item.bounds[1]), Math.max(u[2], item.bounds[2]), Math.max(u[3], item.bounds[3])]; }, [Infinity, Infinity, -Infinity, -Infinity]);
    return b.every(Number.isFinite) && b[2] > b[0] && b[3] > b[1] ? { bounds: b, parentAffine: IDENTITY.slice() } : null;
  }
  return Object.freeze({ read: read });
}));
