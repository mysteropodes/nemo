/* Native Select pivot (#2372 A-select-skew-anchor-lasso): the point a rotation turns about. Baseline (1065e9d8): select-bridge.js placeAnchorAt
 * (:576-587), Alt+click / Alt+drag on the canvas (:1209-1228, mode xform-anchor-drag, onMove ~1981), the anchor crosshair drawn by engine-bridge.js
 * buildTransformBoxItems, and tools.js xformAnchorPoint. The pivot is `state.xformAnchorKey` (one of the 9 preset points of the selection box:
 * tl tc tr ml mc mr bl bc br) or, once Alt+clicked somewhere, `state.xformAnchorCustom` (a WORLD point, [x, y]) which overrides every preset. The
 * numeric Rotate field and the anchor grid of the inspector already read these two state members, so the canvas rotation, the rotate ring and the
 * fields all turn about the same point. Alt+click (or Alt+drag, following the pointer) outside the selection's box places it; Shift snaps to the nearest of
 * the 9 presets; Alt held over the crosshair grabs it. Moving the pivot is a tool preference, not a document edit: it writes no revision and no history
 * entry. Like the baseline's per-stroke `xformAnchorKey` / `xformAnchorCustom`, the choice is remembered per picked stroke, so a deselect and reselect
 * (or another item and back) brings the same pivot back; a stroke never given one reads the centre. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeSelectPivot = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var NS = 'http://www.w3.org/2000/svg', BLUE = 'rgb(74,158,255)', GRAB_PX = 9, memory = new WeakMap();
  // Box corners are [nw, ne, se, sw]; handles [nw, n, ne, e, se, s, sw, w].
  var PRESETS = { tl: 0, tc: 1, tr: 2, mr: 3, br: 4, bc: 5, bl: 6, ml: 7, mc: -1 };
  function presetPoint(box, key) {
    var at = PRESETS[key];
    return at < 0 ? { x: box.center.x, y: box.center.y } : { x: box.handles[at][0], y: box.handles[at][1] };
  }
  // The world point a rotation turns about for the selection box `box`.
  function pointOf(root, box) {
    var st = root.state || {}, c = st.xformAnchorCustom;
    if (Array.isArray(c) && c.length === 2 && c.every(Number.isFinite)) return { x: c[0], y: c[1] };
    return presetPoint(box, Object.prototype.hasOwnProperty.call(PRESETS, st.xformAnchorKey) ? st.xformAnchorKey : 'mc');
  }
  function nearestPreset(box, p) {
    var best = null, bestD = Infinity;
    Object.keys(PRESETS).forEach(function (k) { var q = presetPoint(box, k), d = Math.hypot(p.x - q.x, p.y - q.y); if (d < bestD) { bestD = d; best = k; } });
    return best;
  }
  function store(root) { var m = memory.get(root); if (!m) memory.set(root, m = {}); return m; }
  function paintGrid(root) { if (typeof root.renderXformAnchorGrid === 'function') try { root.renderXformAnchorGrid(); } catch (_) { /* presentation only */ } }
  // The state as it is now, remembered for every picked stroke (the inspector's anchor grid calls this after a preset pick).
  function remember(root) {
    var st = root.state, m = store(root), value = st.xformAnchorCustom ? { custom: st.xformAnchorCustom.slice() } : { key: st.xformAnchorKey || 'mc' };
    (root.selectedPaths || []).forEach(function (p) { if (p && p.data && p.data.strokeId) m[p.data.strokeId] = value; });
  }
  // Place the pivot at the world point `p` (Shift: the nearest preset of `box`), for the picked strokes.
  function place(root, p, snap, box) {
    var st = root.state, key = snap && box ? nearestPreset(box, p) : null;
    if (key) { st.xformAnchorKey = key; st.xformAnchorCustom = null; }
    else st.xformAnchorCustom = [p.x, p.y];
    remember(root); paintGrid(root);
  }
  // A new selection: the first picked stroke's remembered pivot comes back; one never given a pivot resets any custom point (the baseline's clearSel).
  function restore(root) {
    var st = root.state, first = (root.selectedPaths || []).find(function (p) { return p && p.data && p.data.strokeId; });
    if (!st || !first) return;
    var kept = store(root)[first.data.strokeId];
    if (kept && kept.custom) { st.xformAnchorCustom = kept.custom.slice(); }
    else if (kept) { st.xformAnchorKey = kept.key; st.xformAnchorCustom = null; }
    else st.xformAnchorCustom = null;
    paintGrid(root);
  }
  // The menu's "Centrer le point d'ancrage" (baseline select-bridge.js :3112): the pivot goes back to the centre of the selection, for every picked stroke.
  function center(root) {
    var st = root.state; st.xformAnchorCustom = null; st.xformAnchorKey = 'mc';
    remember(root); paintGrid(root);
    if (root.NemoNativeSelectOverlay) root.NemoNativeSelectOverlay.show(root);
    if (typeof root.updateUI === 'function') root.updateUI();
    if (root.SMEngineBridge && typeof root.SMEngineBridge.renderNow === 'function') root.SMEngineBridge.renderNow();
  }
  // True when the world point `p` is on the crosshair (`wpp` world units per screen pixel).
  function grabs(root, box, p, wpp) { var c = pointOf(root, box); return Math.hypot(p.x - c.x, p.y - c.y) < GRAB_PX * wpp; }
  // The crosshair, a ringed cross at the pivot (8 screen px, 10 while hovered), into the overlay's svg.
  function draw(root, svg, c, box) {
    var pv = pointOf(root, box), k = c.pin.canvasW / c.bounds.width, V = c.V, r = 8 * k;
    var x = V[0] * pv.x + V[2] * pv.y + V[4], y = V[1] * pv.x + V[3] * pv.y + V[5];
    var make = function (tag, attrs) { var n = root.document.createElementNS(NS, tag); Object.keys(attrs).forEach(function (a) { n.setAttribute(a, attrs[a]); }); svg.appendChild(n); };
    var line = { fill: 'none', stroke: BLUE, 'stroke-width': 1, 'vector-effect': 'non-scaling-stroke' };
    make('circle', Object.assign({ cx: x, cy: y, r: r }, line, { 'stroke-width': 1.2 }));
    make('line', Object.assign({ x1: x - r, y1: y, x2: x + r, y2: y }, line));
    make('line', Object.assign({ x1: x, y1: y - r, x2: x, y2: y + r }, line));
  }
  return Object.freeze({ pointOf: pointOf, place: place, center: center, restore: restore, remember: remember, grabs: grabs, draw: draw, nearestPreset: nearestPreset, presetPoint: presetPoint });
}));
