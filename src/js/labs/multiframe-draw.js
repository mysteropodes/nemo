// ---- LABS PROTOTYPE — Multi-frame draw (Umoupen "multi-frame edit") ----
// With frames selected in the timeline (the existing shift/drag frame
// selection, _sel.frames in timeline.js) and this flag on, a stroke drawn
// on the current frame is ALSO stamped onto every other selected frame of
// the same layer — e.g. add a scar/prop/correction across a whole span of
// keyframes in one gesture instead of redrawing it N times.
//
// Data-level only: the copy is `serP(path)` pushed into each target
// frame's stored strokes array — the exact serialized form
// saveActiveLayerFrame would produce for a hand-drawn duplicate, so
// loadFrame/export/tween all read it back as an ordinary stroke (no new
// data.* tag, per CLAUDE.md §1). Non-keyframe targets are promoted the
// same way ensureKeyframe does it (freeze current effective strokes, then
// add). All inside the committing gesture's own undo snapshot: one Cmd+Z
// reverts every stamped frame at once.
(function () {
  window.SMLabs.register('multiframe-draw', {
    flag: 'nemo-labs-multiframe',
    describe: 'Un trait dessiné est aussi ajouté à toutes les frames sélectionnées dans la timeline (même calque)',
    onStroke: function (path) {
      if (typeof _sel === 'undefined' || !_sel.frames || !_sel.frames.length) return;
      if (typeof serP !== 'function' || typeof getEffectiveStrokes !== 'function') return;
      var li = state.activeLayerIdx;
      var ld = state.layers[li];
      if (!ld || ld.symbolId) return; // component layers store no strokes of their own
      var sd = serP(path);
      var targets = _sel.frames.filter(function (s) {
        return s.layer === li && s.frame !== state.currentFrame && ld.frames[s.frame];
      });
      // TWO passes, resolve-then-mutate: promoting a non-keyframe target
      // freezes its inherited hold content via getEffectiveStrokes — but
      // that inheritance scans back to the previous KEYFRAME, so promoting
      // frame 3 changes what frame 4 "currently shows" before its own
      // freeze runs. One interleaved pass cascaded the stamp (found live:
      // stamping frames 3..6 produced 1,2,3,4 copies instead of 1,1,1,1).
      var frozen = {};
      targets.forEach(function (s) {
        var f = ld.frames[s.frame];
        if (!f.isKeyframe && !f.isInterpolated) frozen[s.frame] = JSON.parse(JSON.stringify(getEffectiveStrokes(li, s.frame)));
      });
      var stamped = 0;
      targets.forEach(function (s) {
        var f = ld.frames[s.frame];
        if (frozen[s.frame] !== undefined) {
          f.strokes = frozen[s.frame];
          f.isKeyframe = true;
          f.isInterpolated = false;
          if (typeof syncLinkedKeyframeFolder === 'function') syncLinkedKeyframeFolder(li, s.frame);
        }
        f.strokes.push(JSON.parse(JSON.stringify(sd)));
        stamped++;
      });
      if (stamped && typeof showToast === 'function') showToast('Labs — trait ajouté sur ' + stamped + ' autre(s) frame(s)');
      // Current frame's own copy is handled by the normal commit; the
      // timeline needs a redraw so newly-promoted keyframes show their dot.
      if (stamped && typeof renderTimeline === 'function') renderTimeline();
    },
  });
})();
