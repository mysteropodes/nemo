// ---- LABS PROTOTYPE — Interval assistant (Callipeg) ----
// Callipeg's interval assistant places the breakdown/inbetween slots
// between two poses according to a spacing chart (eased, not uniform) —
// the timing half of inbetweening, before any drawing happens. Here:
//
//   1. Select a frame range whose FIRST and LAST frames are keyframes
//   2. SMLabs.intervalAssistant(3)            // 3 eased breakdown slots
//      SMLabs.intervalAssistant(3, 'in')      // ease-in spacing chart
//      ('out' | 'inout' (default) | 'linear')
//
// Each slot becomes a real keyframe pre-filled with a copy of what that
// frame currently shows (its hold content — same freeze as
// insertKeyframeAt), so onion skin and flip-roll immediately work and the
// animator redraws each breakdown in place. Data-level only,
// pushUndoLayers snapshot first, one undo reverts every slot.
(function () {
  var EASES = {
    linear: function (t) { return t; },
    'in': function (t) { return t * t; },
    out: function (t) { return 1 - (1 - t) * (1 - t); },
    inout: function (t) { return t < 0.5 ? 2 * t * t : 1 - 2 * (1 - t) * (1 - t); },
  };

  window.SMLabs.register('interval-assistant', {
    flag: 'nemo-labs-interval',
    describe: 'Action : SMLabs.intervalAssistant(n, ease?) pose n keyframes de breakdown à intervalles éasés entre les 2 bouts de la plage sélectionnée',
  });

  window.SMLabs.intervalAssistant = function (count, ease) {
    if (typeof selBounds !== 'function') return;
    var b = selBounds();
    if (!b) { if (typeof showToast === 'function') showToast('Sélectionne d\'abord une plage de frames dans la timeline'); return; }
    var li = state.activeLayerIdx;
    if (b.minL > li || b.maxL < li) li = b.minL; // work on a layer actually in the selection
    var ld = state.layers[li];
    if (!ld || ld.symbolId) { if (typeof showToast === 'function') showToast('Calque invalide'); return; }
    var span = b.maxF - b.minF;
    count = Math.max(1, Math.min(span - 1, parseInt(count, 10) || 1));
    if (span < 2) { if (typeof showToast === 'function') showToast('Il faut au moins 2 frames d\'écart'); return; }
    var fn = EASES[ease || 'inout'] || EASES.inout;

    // Eased slot positions, deduped and clamped strictly inside the range.
    var slots = [];
    for (var k = 1; k <= count; k++) {
      var f = b.minF + Math.round(fn(k / (count + 1)) * span);
      if (f > b.minF && f < b.maxF && slots.indexOf(f) < 0 && !ld.frames[f].isKeyframe) slots.push(f);
    }
    if (!slots.length) { if (typeof showToast === 'function') showToast('Aucun emplacement libre pour des breakdowns'); return; }

    pushUndoLayers();
    saveAllLayerFrames();
    // Freeze each slot's current hold content BEFORE promoting any of them
    // (resolve-then-mutate — promoting slot A changes what slot B inherits).
    var frozen = {};
    slots.forEach(function (f) { frozen[f] = JSON.parse(JSON.stringify(getEffectiveStrokes(li, f))); });
    slots.forEach(function (f) {
      ld.frames[f] = { strokes: frozen[f], isKeyframe: true, isInterpolated: false };
      if (typeof syncLinkedKeyframeFolder === 'function') syncLinkedKeyframeFolder(li, f);
    });
    loadFrame(state.currentFrame); renderOS(); renderArcs(); updateUI();
    if (typeof renderTimeline === 'function') renderTimeline();
    if (typeof showToast === 'function') showToast(slots.length + ' breakdown(s) posé(s) : frames ' + slots.map(function (f) { return f + 1; }).join(', '));
    return slots;
  };
})();
