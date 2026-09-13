// @ts-check
// Frame-only history entry (P21): the one undo/redo entry kind that captures
// every layer's copy of ONE frame instead of the whole animation. This is the
// entry `pushUndoActiveFrame` (tweens.js) records for the Fill tool and that
// `undo()`/`redo()` apply through their lightweight branch — moved here so the
// capture and the (previously duplicated) apply live in one place with an
// explicit lifecycle. Mapped in H02/#1059; behaviour preserved exactly.
//
// Entry shape, exactly as before:
//   { frame: <index>, layers: [ { strokes, isKeyframe, isInterpolated }, ... ] }
// one element per document layer, in index order, and deliberately NO `type`
// field: `type:'layers'` is the full-animation snapshot the same stacks also
// carry, and the absence of `type` is what routes an entry to this kind.
//
// Lifecycle:
//   capture(doc, frame, cloneStrokes) -> entry     (pure; cloneStrokes is the
//       caller's stroke cloner, tweens.js _cloneStrokesForUndo, which shares
//       the heavy immutable-once-written string fields by reference)
//   apply(doc, entry)                 -> inverse   (writes the entry's frame
//       into doc.layers, moves doc.currentFrame to entry.frame, and returns
//       the entry that undoes this apply — push it on the opposite stack)
//
// Two limitations characterized in H02 are PRESERVED here, not repaired,
// because this leaf is an extraction; each is pinned by a test that names it:
//   1. The inverse entry is built from doc.currentFrame BEFORE the frame is
//      moved to entry.frame (Scenario B): undoing from a different frame
//      records that other frame's content as the redo entry.
//   2. Frame entries carry no symbol/montage context, so no cross-context
//      guard applies to them (the `type:'layers'` guard in undo()/redo()
//      never matches). apply() writes whatever document it is handed.
// Owners of the stacks, labels, the `type:'layers'` path and every UI refresh
// hook remain in tweens.js; this module reads no globals.
var NemoFrameHistoryEntry = (function () {
  'use strict';

  function isFrameEntry(entry) { return !!entry && typeof entry === 'object' && !entry.type; }

  function capture(doc, frame, cloneStrokes) {
    return { frame: frame, layers: doc.layers.map(function (ld) {
      var f = ld.frames[frame] || {};
      return { strokes: cloneStrokes(f.strokes), isKeyframe: f.isKeyframe, isInterpolated: f.isInterpolated };
    }) };
  }

  // Deep copy of the live frame for the inverse entry — the historical apply
  // path used a plain JSON round-trip here (no heavy-field sharing), kept.
  function snapshotFrame(doc, frame) {
    var out = { frame: frame, layers: [] };
    for (var i = 0; i < doc.layers.length; i++) {
      var f = doc.layers[i].frames[frame];
      out.layers.push({ strokes: JSON.parse(JSON.stringify(f.strokes)), isKeyframe: f.isKeyframe, isInterpolated: f.isInterpolated });
    }
    return out;
  }

  function apply(doc, entry) {
    var inverse = snapshotFrame(doc, doc.currentFrame);            // limitation 1: current frame, not entry.frame
    for (var i = 0; i < entry.layers.length && i < doc.layers.length; i++) {
      var tf = doc.layers[i].frames[entry.frame];
      tf.strokes = entry.layers[i].strokes;
      tf.isKeyframe = entry.layers[i].isKeyframe;
      tf.isInterpolated = entry.layers[i].isInterpolated;
    }
    var moved = entry.frame !== doc.currentFrame;
    if (moved) doc.currentFrame = entry.frame;
    return { inverse: inverse, frame: doc.currentFrame, moved: moved };
  }

  return { isFrameEntry: isFrameEntry, capture: capture, apply: apply };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoFrameHistoryEntry;
