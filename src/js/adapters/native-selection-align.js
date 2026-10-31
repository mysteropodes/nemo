/* Native Align and Distribute of the Select selection (#2372, insp-align-distribute). On a native object project the align toolbar's left / centre / right /
 * top / middle / bottom and horizontal / vertical distribute buttons move the selected items as ONE native `object.vector.batch` of record-space reshapes
 * (one revision, one history entry, items of several layers included) - the same route as the canvas drag and the Transform fields (native-select-gesture.js
 * commitMatrix, here with one translation per item) - instead of the legacy alignSelection / distributeSelection, which pushed a history checkpoint a native
 * project refuses and returned without a word. The translations follow the baseline (1065e9d8 tools.js alignSelection / distributeSelection): align
 * measures every item's bounds against the union of the selection's bounds (edge, or centre for centerH / centerV); distribute sorts the items by their current
 * left / top edge, keeps the first and the last where they are and spaces the ones between with EQUAL GAPS (not equal centres); align needs 2 items, distribute 3
 * (anything less did nothing in the baseline, and still does). An item that has no native transform (a bitmap brush, a dab) refuses the whole gesture with the
 * Select gesture's notice and writes nothing. It never throws, holds no document state and reaches no Paper writer. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeSelectionAlign = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var EPS = 1e-6, ALIGN = ['left', 'right', 'centerH', 'top', 'bottom', 'centerV'], DISTRIBUTE = ['horizontal', 'vertical'];
  function notice(root, text) { if (typeof root.showToast === 'function') try { root.showToast(text); } catch (_) {} }
  // The native object project the app is showing, or null (a legacy project keeps the legacy writers).
  function session(root) {
    try { var s = root.NemoNativeObjectProject && root.NemoNativeObjectProject.desktop(root); return s && s.blocksLegacy() === true ? s : null; } catch (_) { return null; }
  }
  function native(root) { return !!session(root); }
  function box(p) { var b = p && p.bounds; return b && [b.left, b.top, b.right, b.bottom].every(Number.isFinite) ? { left: b.left, top: b.top, right: b.right, bottom: b.bottom } : null; }
  function union(boxes) {
    return boxes.reduce(function (u, b) { return u ? { left: Math.min(u.left, b.left), top: Math.min(u.top, b.top), right: Math.max(u.right, b.right), bottom: Math.max(u.bottom, b.bottom) } : b; }, null);
  }
  // Per-item translations [dx, dy] (null where the item stays) for an align `mode`, against the union of the items' bounds. `boxes`: bounds in selection order.
  function alignDeltas(boxes, mode) {
    var u = union(boxes), cx = (u.left + u.right) / 2, cy = (u.top + u.bottom) / 2;
    return boxes.map(function (b) {
      var dx = 0, dy = 0;
      if (mode === 'left') dx = u.left - b.left;
      else if (mode === 'right') dx = u.right - b.right;
      else if (mode === 'centerH') dx = cx - (b.left + b.right) / 2;
      else if (mode === 'top') dy = u.top - b.top;
      else if (mode === 'bottom') dy = u.bottom - b.bottom;
      else if (mode === 'centerV') dy = cy - (b.top + b.bottom) / 2;
      return Math.abs(dx) < EPS && Math.abs(dy) < EPS ? null : [dx, dy];
    });
  }
  // Equal-gap distribution: the first and last item by their current left / top edge are the anchors, the ones between are placed one gap after the other.
  function distributeDeltas(boxes, mode) {
    var h = mode === 'horizontal', start = function (b) { return h ? b.left : b.top; }, size = function (b) { return h ? b.right - b.left : b.bottom - b.top; };
    var order = boxes.map(function (b, i) { return i; }).sort(function (a, c) { return start(boxes[a]) - start(boxes[c]); });
    var first = boxes[order[0]], last = boxes[order[order.length - 1]], spanStart = start(first), spanEnd = h ? last.right : last.bottom;
    var total = order.reduce(function (sum, i) { return sum + size(boxes[i]); }, 0), gap = (spanEnd - spanStart - total) / (order.length - 1);
    var out = boxes.map(function () { return null; }), cursor = spanStart;
    order.forEach(function (at, k) {
      var b = boxes[at];
      if (k === 0) { cursor += size(b); return; }
      if (k === order.length - 1) return;
      var target = cursor + gap, delta = target - start(b);
      cursor = target + size(b);
      if (Math.abs(delta) >= EPS) out[at] = h ? [delta, 0] : [0, delta];
    });
    return out;
  }
  function selected(root) { return (root.selectedPaths || []).filter(function (p) { return p && p.data; }); }
  // Why the selection cannot be aligned here, as a notice; null when it can.
  function refusal(root, paths) {
    var st = root.state, G = root.NemoNativeSelectGesture;
    if (st && st.appMode === 'motion') return 'Aligning the selection is not available in Motion on native projects yet.';
    if (!st || st.tool !== 'select') return 'Switch to the Select tool (V) to align the selection.';
    if (!root.NemoSelectCanvasIntent || !G || typeof G.commitMatrix !== 'function' || !root.NemoNativeObjectLabs) return 'Aligning is not available right now.';
    if (paths.some(function (p) { return !p.data.nativeStableTarget || !Number.isInteger(p.data.nativeLayerIndex); })) return 'Aligning this selection is not available on native projects yet.';
    return null;
  }
  // The shared body: `kind` 'align' | 'distribute', `mode` the button's value. Resolves to 'applied', 'noop', or a refusal ('refused' / 'stale' / 'failed' / 'cancelled' /
  // 'unavailable'); a refusal has already toasted. Never throws.
  // A stroke id is unique within its layer only: a selection spanning layers keys each item by layer and id (as select-canvas-intent does).
  function key(p) { return p.data.nativeLayerIndex + '|' + p.data.strokeId; }
  async function run(root, kind, mode) {
    try {
      var need = kind === 'align' ? 2 : 3, modes = kind === 'align' ? ALIGN : DISTRIBUTE;
      var paths = selected(root);
      if (paths.length < need || modes.indexOf(mode) < 0) return 'noop'; // the baseline did nothing for fewer items
      var why = refusal(root, paths);
      if (why) { notice(root, why); return 'unavailable'; }
      var I = root.NemoSelectCanvasIntent, O = root.NemoNativeSelectOverlay, ctx = I.pickContext(root), at = I.presentedState(root) || await I.presentedSettled(root);
      var boxes = paths.map(box);
      if (!at || boxes.some(function (b) { return !b; })) { notice(root, 'Aligning is not available right now.'); return 'unavailable'; }
      var deltas = kind === 'align' ? alignDeltas(boxes, mode) : distributeDeltas(boxes, mode), byId = {}, moved = 0;
      paths.forEach(function (p, i) { if (deltas[i]) { byId[key(p)] = [1, 0, 0, 1, deltas[i][0], deltas[i][1]]; moved++; } });
      if (!moved) return 'noop';
      var status = await root.NemoNativeSelectGesture.commitMatrix(root, paths, function (p) { return byId[key(p)] || null; }, {
        pin: at.pin, gone: 'The selection changed — nothing was moved.', stale: function () { return I.pickContext(root) !== ctx; } });
      if (status !== 'applied') return status;
      if (O && typeof O.show === 'function') O.show(root);
      if (typeof root.updateUI === 'function') root.updateUI();
      if (root.SMEngineBridge && typeof root.SMEngineBridge.renderNow === 'function') root.SMEngineBridge.renderNow();
      return 'applied';
    } catch (_) { notice(root, 'Aligning is not available right now.'); return 'failed'; }
  }
  return Object.freeze({ native: native, align: function (root, mode) { return run(root, 'align', mode); }, distribute: function (root, mode) { return run(root, 'distribute', mode); },
    alignDeltas: alignDeltas, distributeDeltas: distributeDeltas });
}));
