// ---- LABS PROTOTYPE — Retime exposure (TVPaint X-sheet ones/twos) ----
// Re-times the keyframes of the selected timeline range to a fixed
// exposure: "on ones" (spacing 1), "on twos" (2), "on threes" (3)...
// The classic X-sheet operation — rough an action on ones, then relax it
// to twos without redrawing; or space out a too-snappy run.
//
//   1. Select a frame range in the timeline
//   2. SMLabs.retimeExposure(2)   // put the range on twos
//
// Data-level only, same deep-copy + pushUndoLayers mechanics as
// SM.repeatSelection: the range's keyframes are collected in order and
// re-placed at minF, minF+n, minF+2n... — every other frame in the new
// span becomes a plain hold. Content after the range is untouched (the
// new span can extend the timeline if the retime is longer than the
// original selection).
(function () {
  window.SMLabs.register('retime-exposure', {
    flag: 'nemo-labs-retime',
    describe: 'Action : SMLabs.retimeExposure(n) re-cale les keyframes de la plage sélectionnée sur des expositions fixes (1=ones, 2=twos, 3=threes...)',
  });

  window.SMLabs.retimeExposure = function (spacing) {
    if (typeof selBounds !== 'function') return;
    var b = selBounds();
    if (!b) { if (typeof showToast === 'function') showToast('Sélectionne d\'abord une plage de frames dans la timeline'); return; }
    spacing = Math.max(1, Math.min(12, parseInt(spacing, 10) || 2));
    pushUndoLayers();
    saveAllLayerFrames();
    var totalNeeded = 0, plans = [];
    for (var l = b.minL; l <= b.maxL; l++) {
      var ld = state.layers[l];
      if (!ld || ld.symbolId) continue;
      // Collect the range's keyframes in order (deep-copied BEFORE any
      // mutation — resolve-then-mutate, same lesson as multiframe-draw).
      var keys = [];
      for (var f = b.minF; f <= b.maxF; f++) {
        var fr = ld.frames[f];
        if (fr && fr.isKeyframe) keys.push(JSON.parse(JSON.stringify(fr)));
      }
      if (!keys.length) continue;
      var endF = b.minF + (keys.length - 1) * spacing;
      totalNeeded = Math.max(totalNeeded, endF + 1);
      plans.push({ l: l, keys: keys, endF: endF });
    }
    if (!plans.length) { if (typeof showToast === 'function') showToast('Aucune keyframe dans la plage'); return; }
    if (totalNeeded > state.totalFrames) {
      var add = totalNeeded - state.totalFrames;
      for (var li = 0; li < state.layers.length; li++) { for (var a = 0; a < add; a++) state.layers[li].frames.push({ strokes: [], isKeyframe: false, isInterpolated: false }); }
      state.totalFrames = totalNeeded; window._totalF = totalNeeded;
      if (state.waOut < totalNeeded - 1) { state.waOut = totalNeeded - 1; window._waOut = state.waOut; }
    }
    plans.forEach(function (p) {
      var ld = state.layers[p.l];
      // Clear the whole rewritten span (original range + any extension) to
      // plain holds, then drop the keyframes at their new positions.
      var wipeEnd = Math.max(b.maxF, p.endF);
      for (var f = b.minF; f <= wipeEnd; f++) ld.frames[f] = { strokes: [], isKeyframe: false, isInterpolated: false };
      p.keys.forEach(function (k, i) { ld.frames[b.minF + i * spacing] = k; });
    });
    loadFrame(state.currentFrame); renderOS(); renderArcs(); updateUI();
    if (typeof renderTimeline === 'function') renderTimeline();
    if (typeof showToast === 'function') showToast('Exposition re-calée sur ' + spacing + ' (' + (spacing === 1 ? 'ones' : spacing === 2 ? 'twos' : spacing === 3 ? 'threes' : spacing) + ')');
  };
})();
