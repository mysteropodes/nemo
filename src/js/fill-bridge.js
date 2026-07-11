// ---- C7 CUTOVER STEP 6: Fill (paint-bucket) tool on the Rust engine ----
// Simplest bridge so far: the Fill tool is a single click, not a drag, so
// there's no live-preview overlay to build and no suspend()/resume() span
// (a single click doesn't race tick() the way a drag does — the whole
// mutation + one render happens synchronously between two ticks). Faithful
// port of the 'fill' branch of onMouseDown in tools.js: shift-click removes
// an existing fill (hit-test fill:true, tolerance 12/zoom) instead of
// adding one; a plain click runs fillVectorFind (already tries the wasm
// fill_find path first — see fillVectorFind's own implementation, this
// bridge doesn't change that), inserts the result at the bottom of the
// active layer, and stores the same fillSeed/fillGapPx data
// fillRegenerateLinked relies on later.
(function () {
  function shouldIntercept() {
    return (
      window.SMEngineBridge && window.SMEngineBridge.isEnabled() &&
      state.tool === 'fill' && !state.playing && !state.layers[state.activeLayerIdx].locked
    );
  }

  function onDown(e) {
    if (!shouldIntercept()) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    // Alt+drag: same temporary closing-stroke gesture as tools.js's Paper
    // Tool 'fill' branch (see that file's fillCloseOverlayItems/
    // fillMaterializeTempCloseStrokes comments for the full rationale) —
    // duplicated here because this pointerdown/move/up bridge is the path
    // actually used whenever the Rust engine is intercepting events
    // (shouldIntercept() above), which is the normal running case.
    if (e.altKey) {
      var w0 = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
      _fillCloseDrag = { points: [new Point(w0[0], w0[1])] };
      // A closing stroke is often drawn deliberately past the visible
      // strokes near the canvas edge, more likely than most drags to leave
      // the canvas element's own bounds mid-gesture — without capture, the
      // pointerup landing outside #canvas-area would never reach onUp
      // below, leaving _fillCloseDrag set and the engine suspended forever
      // (breaking not just the fill tool but every other tool's own
      // suspend/resume-gated interaction, e.g. Alt+drag rotate elsewhere).
      try { e.target.setPointerCapture(e.pointerId); } catch (err) {}
      window.SMEngineBridge.suspend();
      window.SMEngineBridge.renderWithOverlayItem(fillCloseOverlayItems());
      return;
    }
    // pushUndo() BEFORE ensureKeyframe(), and unconditionally (not lazily
    // per-branch like before) — see draw-bridge.js's commitStroke comment:
    // a single undo must revert both the frame's auto-promotion to keyframe
    // AND whatever this click does, as one step. Doing it lazily per-branch
    // also left a "Aucune zone fermée ici" miss's keyframe promotion
    // permanently un-undoable.
    pushUndo();
    ensureKeyframe();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var pt = new Point(w[0], w[1]);
    var layer = userLayers[state.activeLayerIdx];

    if (e.shiftKey) {
      var hitRm = layer.hitTest(pt, { fill: true, tolerance: 12 / view.zoom });
      if (hitRm && hitRm.item instanceof Path && hitRm.item.fillColor) {
        hitRm.item.fillColor = null;
        saveActiveLayerFrame();
        updateUI();
        showToast('Fill supprimé');
        window.SMEngineBridge.renderNow();
      }
      return;
    }

    var _tempCloseWalls = fillMaterializeTempCloseStrokes(layer);
    var res = fillVectorFind(pt, layer, null);
    fillRemoveTempCloseStrokes(_tempCloseWalls);
    // Only discards the closing stroke(s) that actually bounded THIS
    // result (via its wallIds) — drawing several closing strokes for
    // different zones and filling them one at a time keeps every other
    // queued stroke around instead of wiping all of them on the first
    // click (reported: "j'aurais dû refaire chaque trait").
    if (res) fillConsumeCloseStrokes(res.wallIds);
    if (!res) {
      // No traceable closed region from the surrounding walls — but if the
      // click landed directly inside an already-filled shape, recolor it in
      // place instead of rejecting (same fallback as tools.js's own 'fill'
      // branch — see its comment for the Animate-parity rationale).
      var hitFill = layer.hitTest(pt, { fill: true, tolerance: 1 / view.zoom });
      if (hitFill && (hitFill.item instanceof Path || hitFill.item instanceof CompoundPath) && hitFill.item.fillColor) {
        hitFill.item.fillColor = state.fillColor;
        hitFill.item.opacity = state.opacity / 100;
        saveActiveLayerFrame();
        updateUI();
        showToast('Couleur remplacée');
        window.SMEngineBridge.renderNow();
        return;
      }
      showToast('Aucune zone fermée ici');
      return;
    }
    // Same Animate-style stacking as tools.js's own 'fill' branch — see
    // fillInsertIndexFor's comment (tools.js): above the topmost fill
    // covering the click point, so the new fill is actually visible.
    layer.insertChild(fillInsertIndexFor(layer, pt, res.path), res.path);
    res.path.fillColor = state.fillColor;
    res.path.strokeColor = null;
    res.path.opacity = state.opacity / 100;
    res.path.data.fillSeed = [pt.x, pt.y];
    res.path.data.fillGapPx = res.gapPx;
    if (res.wallIds && res.wallIds.length) res.path.data.fillWalls = res.wallIds;
    // Animate merge-drawing: fuse with touching same-color fills (see
    // fillMergeSameColor's comment in tools.js — shared logic, both paths).
    fillMergeSameColor(layer, res.path);
    saveActiveLayerFrame();
    updateUI();
    showToast('Fill appliqué');
    window.SMEngineBridge.renderNow();
  }

  function onMove(e) {
    if (!_fillCloseDrag) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    _fillCloseDrag.points.push(new Point(w[0], w[1]));
    window.SMEngineBridge.renderWithOverlayItem(fillCloseOverlayItems());
  }
  function onUp(e) {
    if (!_fillCloseDrag) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    try { e.target.releasePointerCapture(e.pointerId); } catch (err) {}
    if (_fillCloseDrag.points.length >= 2) _fillCloseStrokes.push({ id: 'fc' + Date.now().toString(36) + '_' + (++_fillCloseIdCounter), points: _fillCloseDrag.points.map(function (p) { return [p.x, p.y]; }) });
    _fillCloseDrag = null;
    window.SMEngineBridge.resume();
    window.SMEngineBridge.renderNow();
    showToast(_fillCloseStrokes.length + ' trait(s) de fermeture en attente — clic sans Alt pour remplir');
  }
  // Belt-and-suspenders alongside setPointerCapture in onDown: a bare
  // document-level pointerup ALSO finalizes a stuck drag (bubble phase, so
  // it only ever runs if nothing else already handled/stopped the event —
  // harmless no-op via the `!_fillCloseDrag` guard once the capture-phase
  // onUp above has already done its job normally).
  function onDocUp(e) {
    if (!_fillCloseDrag) return;
    onUp(e);
  }

  // Recovery valve: if a closing-stroke drag ever gets stuck active (pointer
  // capture failing on some platform, a devtools/OS gesture stealing the
  // pointerup, etc.) the engine would stay suspended forever, silently
  // breaking every other suspend/resume-gated tool. Escape always cancels
  // it and resumes rendering.
  function onKeyDown(e) {
    if (e.key === 'Escape' && _fillCloseDrag) {
      _fillCloseDrag = null;
      window.SMEngineBridge.resume();
      window.SMEngineBridge.renderNow();
    }
  }

  function init() {
    var target = document.getElementById('canvas-area') || document.getElementById('drawing-canvas');
    target.addEventListener('pointerdown', onDown, { capture: true });
    target.addEventListener('pointermove', onMove, { capture: true });
    document.addEventListener('keydown', onKeyDown);
    target.addEventListener('pointerup', onUp, { capture: true });
    target.addEventListener('pointercancel', onUp, { capture: true });
    document.addEventListener('pointerup', onDocUp);
    document.addEventListener('pointercancel', onDocUp);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
