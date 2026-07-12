// ---- LABS PROTOTYPE — Boiling line / vector noise (Moho) ----
// Moho's animated vector noise, a.k.a. the hand-drawn "boil": the line
// wobbles slightly from frame to frame even when nothing moves, keeping a
// held drawing alive. Action form — bake N jittered variants of the
// current frame's drawing onto the following frames:
//
//   SMLabs.boil()                          — 3 variants on the next 3 frames
//   SMLabs.boil({frames: 5, amplitude: 3, seedStep: 1})
//
// Each generated frame is a real keyframe holding a deep-copied, seeded-
// jitter variant (deterministic per frame — replaying produces the same
// boil, same reason dab stamping uses a seeded RNG per CLAUDE.md). Jitter
// displaces segment POINTS only, not handles, so curve character survives;
// amplitude is world px. One pushUndoLayers reverts the whole bake.
(function () {
  function seededRng(seed) {
    var s = seed >>> 0;
    return function () {
      s = (s * 1664525 + 1013904223) >>> 0;
      return s / 4294967296;
    };
  }

  window.SMLabs.boil = function (opts) {
    opts = opts || {};
    var li = state.activeLayerIdx;
    var ld = state.layers[li];
    if (!ld || ld.locked || ld.symbolId) { if (typeof showToast === 'function') showToast('Calque invalide/verrouillé'); return 0; }
    var nFrames = Math.max(1, Math.min(24, opts.frames || 3));
    var amp = opts.amplitude !== undefined ? Math.max(0.2, +opts.amplitude) : 2.5;
    var cf = state.currentFrame;
    var src = getEffectiveStrokes(li, cf);
    if (!src.length) { if (typeof showToast === 'function') showToast('Rien à faire bouillir sur cette frame'); return 0; }

    pushUndoLayers();
    saveAllLayerFrames();
    var needed = cf + nFrames + 1;
    if (needed > state.totalFrames) {
      var add = needed - state.totalFrames;
      for (var l = 0; l < state.layers.length; l++) { for (var a = 0; a < add; a++) state.layers[l].frames.push({ strokes: [], isKeyframe: false, isInterpolated: false }); }
      state.totalFrames = needed; window._totalF = needed;
      if (state.waOut < needed - 1) { state.waOut = needed - 1; window._waOut = state.waOut; }
    }
    for (var k = 1; k <= nFrames; k++) {
      var rng = seededRng(0xB011 + k * (opts.seedStep || 1));
      var copy = JSON.parse(JSON.stringify(src));
      copy.forEach(function (sd) {
        if (sd.isRaster || !sd.segments) return;
        // Dab stamps follow their anchor's jitter for free on reload via
        // relink; standalone jitter of hundreds of dabs would boil the
        // TEXTURE (explicitly what the seeded-RNG rule exists to avoid).
        if (sd.isBrushTextureCopy) return;
        sd.segments.forEach(function (seg) {
          seg.point[0] += (rng() - 0.5) * 2 * amp;
          seg.point[1] += (rng() - 0.5) * 2 * amp;
        });
      });
      ld.frames[cf + k] = { strokes: copy, isKeyframe: true, isInterpolated: false };
    }
    loadFrame(state.currentFrame); renderOS(); renderArcs(); updateUI();
    if (typeof renderTimeline === 'function') renderTimeline();
    if (typeof showToast === 'function') showToast('Boil : ' + nFrames + ' variante(s) tremblée(s) posée(s) après la frame ' + (cf + 1));
    return nFrames;
  };

  window.SMLabs.register('boil-effect', {
    flag: 'nemo-labs-boil',
    describe: 'Ligne bouillante (Moho vector noise) : SMLabs.boil({frames,amplitude}) pose N keyframes-variantes tremblées du dessin courant sur les frames suivantes',
  });
})();
