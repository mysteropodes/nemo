// ---- LABS PROTOTYPE — Move selection to another layer (Umoupen) ----
// Umoupen lists "move drawings between layers" as a core selection op;
// Nemo has layer reordering but no way to move SELECTED STROKES from one
// layer to another. Action:
//   1. Select tool, select stroke(s)
//   2. SMLabs.moveSelectionToLayer(targetLayerIdx)
//
// Mechanics: pure Paper re-parenting (targetLayer.addChild), then a normal
// saveAllLayerFrames() — persistence reads layer.children, so moving the
// live items IS the move; serP/desP round-trips, data tags, linked fills
// and brush-texture companions all come along because the OBJECTS move,
// nothing is rebuilt. The target layer's current frame is promoted to a
// keyframe first (same freeze-inherited-content dance as ensureKeyframe)
// because saveAllLayerFrames skips frames that are neither keyframe nor
// interpolated — without the promotion the moved strokes would render but
// silently vanish from persisted data (CLAUDE.md §1's worst failure mode).
(function () {
  window.SMLabs.register('move-to-layer', {
    flag: 'nemo-labs-move-to-layer',
    describe: 'Action : SMLabs.moveSelectionToLayer(idx) déplace les traits sélectionnés (outil Sélection) vers le calque idx, même frame',
  });

  window.SMLabs.moveSelectionToLayer = function (targetIdx) {
    if (typeof selectedPaths === 'undefined' || !selectedPaths.length) {
      if (typeof showToast === 'function') showToast('Sélectionne d\'abord des traits (outil Sélection)');
      return 0;
    }
    var tl = state.layers[targetIdx];
    if (!tl || targetIdx === state.activeLayerIdx) { if (typeof showToast === 'function') showToast('Calque cible invalide'); return 0; }
    if (tl.locked) { if (typeof showToast === 'function') showToast('Calque cible verrouillé'); return 0; }
    if (tl.symbolId) { if (typeof showToast === 'function') showToast('Impossible vers un calque composant'); return 0; }

    pushUndo();
    // Promote the target layer's current frame BEFORE re-parenting, so its
    // frozen "inherited" content doesn't accidentally include the strokes
    // being moved in.
    var tf = tl.frames[state.currentFrame];
    if (!tf.isKeyframe && !tf.isInterpolated) {
      tf.strokes = JSON.parse(JSON.stringify(getEffectiveStrokes(targetIdx, state.currentFrame)));
      tf.isKeyframe = true;
      tf.isInterpolated = false;
      if (typeof syncLinkedKeyframeFolder === 'function') syncLinkedKeyframeFolder(targetIdx, state.currentFrame);
    }

    var target = userLayers[targetIdx];
    var moved = 0;
    selectedPaths.slice().forEach(function (p) {
      if (!p || !p.parent) return;
      // Companions travel WITH their primary: a vector-brush ribbon's
      // linked fill, and brush-texture dab copies, are separate Paper items
      // in the same layer — leaving them behind splits the visual object
      // across two layers.
      if (p.data && p.data.linkedFill && p.data.linkedFill.parent) target.addChild(p.data.linkedFill);
      if (p.data && p.data.brushCompanions) p.data.brushCompanions.forEach(function (c) { if (c && c.parent) target.addChild(c); });
      target.addChild(p);
      moved++;
    });
    if (typeof clearSel === 'function') clearSel();
    saveAllLayerFrames();
    loadFrame(state.currentFrame);
    renderOS(); renderArcs(); updateUI();
    if (typeof showToast === 'function') showToast(moved + ' trait(s) déplacé(s) vers « ' + tl.name + ' »');
    return moved;
  };
})();
