/* Native Alt-drag duplicate of the Select selection (#2372/#1027, A-select-alt-drag-duplicate). Baseline (1065e9d8 select-bridge.js onDown ~1186-1207):
 * Alt+press INSIDE the current selection clones it in place through the Duplicate pipeline and the rest of the gesture drags the clones, as one
 * undo step, the originals left where they were. On a native object project the gesture (native-select-gesture.js) previews the moved copy and, on
 * release, hands the drag's world affine here. This module turns that translation into the record-space distance the engine owner takes and sends
 * ONE object.selection.duplicate with an `offset` through the clipboard module's duplicateAt: one revision, one history entry, the whole compound
 * (a ribbon with its linked fill and centreline, a texture anchor with its dabs, a bitmap brush with its raster, a combine group with fresh ids) cloned by
 * the same engine kernel as Cmd+D, the clones selected afterwards. Nothing is written while the pointer moves; Escape, a lost capture or a document
 * that moved on since the press end the gesture with no write (P25). A selection spanning layers is cloned into the active layer as in the baseline (the clipboard
 * module's duplicate names the other layers' shapes). The baseline's press-time clone is the zero distance of a plain Alt-click: the gesture hands this module the
 * identity, and the copy stays where the original is. No document state is held. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeSelectAltDuplicate = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var EPS = 1e-6, LIMIT = 1e6; // LIMIT is the engine's own bound on a duplicate offset (per axis)
  function notice(root, text) { if (typeof root.showToast === 'function') try { root.showToast(text); } catch (_) {} }
  // The record-space distance of the world move `m` ([1 0 0 1 dx dy]) for every selected item: P^-1 T P is a pure translation, by the inverse linear part of the
  // item's presented matrix P applied to (dx, dy). One request carries one distance, so every item must agree (they do unless a selection spans placements).
  function distance(root, sel, m) {
    var I = root.NemoSelectCanvasIntent, offset = null;
    for (var i = 0; i < sel.length; i++) {
      var r = I.inRecordSpace(sel[i].data.nativeMatrix, m);
      if (Math.abs(r[0] - 1) > EPS || Math.abs(r[3] - 1) > EPS || Math.abs(r[1]) > EPS || Math.abs(r[2]) > EPS) return { refusal: 'Duplicating while dragging is not available for this selection yet.' };
      if (!Number.isFinite(r[4]) || !Number.isFinite(r[5]) || Math.abs(r[4]) > LIMIT || Math.abs(r[5]) > LIMIT) return { refusal: 'That is too far to drag a copy.' };
      if (offset && (Math.abs(offset[0] - r[4]) > EPS || Math.abs(offset[1] - r[5]) > EPS)) return { refusal: 'Shapes placed differently cannot be duplicated in one drag yet.' };
      offset = [r[4], r[5]];
    }
    return offset ? { offset: offset } : { refusal: 'Select items to duplicate them.' };
  }
  // `o`: { pin (the document pin read when the drag began), gone (notice for a document that moved on), stale() (true when the document, frame, layer or selection moved
  // on), cancelled() (Escape between the release and the send), release() (before the send) }. Resolves to 'applied', or the refusal ('refused' / 'stale' / 'failed' /
  // 'cancelled'); a refusal has already toasted.
  async function commit(root, sel, m, o) {
    o = o || {};
    var C = root.NemoNativeSelectionClipboard, gone = o.gone || 'The document changed during the drag — nothing was changed.';
    if (!C || typeof C.duplicateAt !== 'function' || !root.NemoSelectCanvasIntent || C.native(root) !== true) { notice(root, 'Duplicating is not available right now.'); return 'refused'; }
    if (!sel.length || sel.some(function (p) { return !Number.isInteger(p.data.nativeLayerIndex); })) { notice(root, 'Select items to duplicate them.'); return 'refused'; }
    var d;
    try { d = distance(root, sel, m); } catch (_) { d = { refusal: 'Duplicating while dragging is not available for this selection yet.' }; }
    if (d.refusal) { notice(root, d.refusal); return 'refused'; }
    if (o.stale && o.stale()) { notice(root, gone); return 'stale'; }
    var out;
    try { out = await C.duplicateAt(root, d.offset, { pin: o.pin, cancelled: o.cancelled, release: o.release }); } catch (_) { out = null; }
    var status = out && out.status;
    return status === 'applied' ? 'applied' : status === 'cancelled' ? 'cancelled' : status === 'stale' ? 'stale' : status === 'noop' ? 'noop' : 'failed';
  }
  return Object.freeze({ commit: commit, distance: distance });
}));
