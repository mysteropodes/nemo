/* Native Eyedropper (#2372). On a native object project the Paper layer holds no live items, so the baseline layer.hitTest found nothing and the click
 * said nothing. The pick reads the engine's hit-test listing instead (NemoSelectCanvasIntent.pick, the read the Select gesture uses), waits out a busy
 * native queue (NemoNativePresentationWait), and feeds the listed paint to the same controls the baseline filled. READ-ONLY: no document write, no history,
 * no selection, no layer switch; a miss or an unready session says so and never falls back to the Paper layer.
 *   describe(hit)                       -> { stroke, fill, width, preset } from the hit's listed style, or null when it lists nothing to pick
 *   pickAt(root, ports, clientX, clientY) -> Promise<boolean>; ports: { setColor(kind, css), setBrushSize(n), selectPreset(key), toast(text) }
 * Baseline (1065e9d8 viewtools-bridge.js pick): a vector brush ribbon's fill is the stroke colour and its width the brush size; any other item gives its
 * stroke and fill colours and its stroke width; the brush preset is copied (or reset to 'none'); toast 'Color picked'. Colours keep their alpha (#rrggbbaa). */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeEyedropper = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var NOTHING = 'Nothing to pick here', PICKED = 'Color picked', HEX = /^#[0-9a-f]{6}(?:[0-9a-f]{2})?$/i;
  function colour(v) { return typeof v === 'string' && HEX.test(v) ? v : null; }
  function describe(hit) {
    var data = hit && hit.data, st = data && data.nativeStyle;
    if (!st || typeof st !== 'object') return null;
    var role = data.nativeRole || st.role, ribbon = role === 'ribbon' || data.nativeRibbon === true || data.isVectorBrush === true;
    var fill = st.fillEnabled === false ? null : colour(st.fill), stroke = st.strokeEnabled === false ? null : colour(st.stroke);
    var out = { stroke: ribbon ? fill : stroke, fill: ribbon ? null : fill, width: null, preset: 'none' };
    // A ribbon is a filled outline, its listed width is the brush width; any other item has a width only when it strokes. A record that lists none keeps the size.
    if ((ribbon || stroke) && typeof st.width === 'number' && Number.isFinite(st.width) && st.width > 0) out.width = st.width;
    if (typeof st.brushPreset === 'string' && st.brushPreset.length > 0) out.preset = st.brushPreset;
    return out.stroke || out.fill || out.width !== null ? out : null;
  }
  function apply(ports, picked) {
    if (picked.stroke) ports.setColor('stroke', picked.stroke);
    if (picked.fill) ports.setColor('fill', picked.fill);
    if (picked.width !== null) ports.setBrushSize(picked.width);
    ports.selectPreset(picked.preset);
    ports.toast(PICKED);
    return true;
  }
  function pickAt(root, ports, clientX, clientY) {
    var I = root.NemoSelectCanvasIntent, W = root.NemoNativePresentationWait;
    var miss = function () { ports.toast(NOTHING); return false; };
    if (!I || typeof I.pick !== 'function' || !I.objectProject(root)) return Promise.resolve(miss());
    var ctx = I.pickContext(root), read = function () { return I.pick(root, clientX, clientY); };
    var done = function (hit) { var picked = describe(hit); return picked ? apply(ports, picked) : miss(); };
    if (!W) return read().then(function (hit) { return hit === undefined ? miss() : I.pickContext(root) === ctx ? done(hit) : false; }, miss);
    return W.await(root, { read: read, ready: function (v) { return v !== undefined; }, holds: function () { return I.pickContext(root) === ctx; }, notice: NOTHING }, { tries: 17 })
      .then(function (r) { return r.ready ? done(r.value) : r.status === 'refused' ? miss() : false; });
  }
  return Object.freeze({ describe: describe, pickAt: pickAt, NOTHING: NOTHING, PICKED: PICKED });
}));
