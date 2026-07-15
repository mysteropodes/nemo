// ---- LABS PROTOTYPE — Ping-pong cycle (Callipeg cycle/interval assistant) ----
// The existing Cycle button (SM.repeatSelection, timeline.js) repeats a
// selected frame range forward only: A B C A B C. This adds the ping-pong
// variant: A B C B A B C B A — the bread-and-butter of pendulum swings,
// breathing, hair sway... Every appended block skips the shared endpoint
// frame so the turnaround never plays twice.
//
// An ACTION, not a passive toggle — call it from the console with a frame
// range selected in the timeline:
//   SMLabs.pingpongCycle(4)   // append 4 alternating blocks
// Same data-level copy mechanics as repeatSelection (deep-copied frames,
// pushUndoLayers snapshot first, timeline extended as needed) so undo/
// save/export treat the result exactly like the existing Cycle's.
(function () {
  window.SMLabs.register('pingpong-cycle', {
    flag: 'nemo-labs-pingpong-cycle',
    describe: 'Action : SMLabs.pingpongCycle(n) répète la plage sélectionnée en aller-retour (A B C B A...) au lieu de boucler en avant',
  });

  window.SMLabs.pingpongCycle = function (times) {
    if (typeof selBounds !== 'function') return;
    var b = selBounds();
    if (!b) { if (typeof showToast === 'function') showToast('Sélectionne d\'abord une plage de frames dans la timeline'); return; }
    times = Math.max(1, Math.min(50, parseInt(times, 10) || 1));
    var span = b.maxF - b.minF + 1;
    if (span < 2) { if (typeof showToast === 'function') showToast('Il faut au moins 2 frames pour un ping-pong'); return; }
    pushUndoLayers();
    saveAllLayerFrames();
    // Each appended block is span-1 frames (the turnaround frame is shared
    // with the previous block's end, so it is never duplicated).
    var blockLen = span - 1;
    var needed = b.maxF + 1 + blockLen * times;
    if (needed > state.totalFrames) {
      var add = needed - state.totalFrames;
      for (var li = 0; li < state.layers.length; li++) { for (var a = 0; a < add; a++) state.layers[li].frames.push({ strokes: [], isKeyframe: false, isInterpolated: false }); }
      state.totalFrames = needed; window._totalF = needed;
      if (state.waOut < needed - 1) { state.waOut = needed - 1; window._waOut = state.waOut; }
    }
    for (var l = b.minL; l <= b.maxL; l++) {
      if (!state.layers[l] || state.layers[l].symbolId) continue;
      var dst = b.maxF + 1;
      for (var r = 1; r <= times; r++) {
        // Odd blocks run backward (maxF-1 down to minF), even blocks run
        // forward again (minF+1 up to maxF) — A B C | B A | B C | B A ...
        for (var k = 1; k <= blockLen; k++) {
          var srcF = (r % 2 === 1) ? (b.maxF - k) : (b.minF + k);
          var src = state.layers[l].frames[srcF];
          state.layers[l].frames[dst] = { strokes: JSON.parse(JSON.stringify(src.strokes || [])), isKeyframe: !!src.isKeyframe, isInterpolated: !!src.isInterpolated };
          dst++;
        }
      }
    }
    loadFrame(state.currentFrame); renderOS(); renderArcs(); updateUI();
    if (typeof renderTimeline === 'function') renderTimeline();
    if (typeof showToast === 'function') showToast('Ping-pong : plage répétée ' + times + ' fois en aller-retour');
  };
})();
