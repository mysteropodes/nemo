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
        showToast(SM.t('toastFillRemoved'));
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
        showToast(SM.t('toastColorReplaced'));
        window.SMEngineBridge.renderNow();
        return;
      }
      showToast(SM.t('toastNoClosedAreaHere'));
      return;
    }
    // Recolor-in-place (see fillFindExistingMatch's comment, tools.js) —
    // same check as tools.js's own 'fill' branch, duplicated here since
    // this bridge is a separate click handler, not a shared code path.
    var existingMatch = fillFindExistingMatch(layer, res.path);
    if (existingMatch) {
      res.path.remove();
      existingMatch.fillColor = state.fillColor;
      existingMatch.opacity = state.opacity / 100;
      saveActiveLayerFrame();
      updateUI();
      showToast(SM.t('toastColorReplaced'));
      window.SMEngineBridge.renderNow();
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
    showToast(SM.t('toastFillApplied'));
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
    // Committed as a PERSISTENT invisible line in the drawing rather than
    // queued as a single-use hint (Toon Boom Harmony's Close Gap model — see
    // fillCommitCloseLine, tools.js). It therefore survives the fill click,
    // the frame, the save, and is tweened/matched like any other stroke,
    // which is what lets a fill be re-traced on other frames at all. Undoable
    // as one step with whatever fill follows it.
    var committed = null;
    if (_fillCloseDrag.points.length >= 2) {
      pushUndo();
      ensureKeyframe();
      committed = fillCommitCloseLine(userLayers[state.activeLayerIdx], _fillCloseDrag.points);
      if (committed) saveActiveLayerFrame();
    }
    _fillCloseDrag = null;
    window.SMEngineBridge.resume();
    window.SMEngineBridge.renderNow();
    if (committed) showToast(SM.t('toastFillCloseLineAdded'));
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

  // "Propager sur toutes les frames" — re-traces the bucket fill under the
  // cursor (or the only one on the frame) across the whole layer. Explicit
  // rather than automatic on every fill click: it costs ~3.5s on a 120-frame
  // project, which would be punishing when colouring many zones in a row.
  function onPropagateClick() {
    if (state.layers[state.activeLayerIdx].locked) { showToast(SM.t('toastLayerLocked')); return; }
    var layer = userLayers[state.activeLayerIdx];
    var fills = layer.children.filter(function (c) { return c.data && c.data.fillSeed; });
    if (!fills.length) { showToast(SM.t('toastPropagateNoFill')); return; }
    // Most recently added fill = the one just painted, the usual intent.
    var target = fills[fills.length - 1];
    pushUndo();
    var res = null;
    try { res = fillPropagateAcrossFrames(target); } catch (err) { console.warn('[fill] propagate failed', err); }
    updateUI();
    window.SMEngineBridge.renderNow();
    if (!res) { showToast(SM.t('toastPropagateFailed')); return; }
    showToast(SM.t('toastPropagateDone').replace('{filled}', res.filled).replace('{skipped}', res.skipped));
  }

  function init() {
    var target = document.getElementById('canvas-area') || document.getElementById('drawing-canvas');
    target.addEventListener('pointerdown', onDown, { capture: true });
    target.addEventListener('pointermove', onMove, { capture: true });
    document.addEventListener('keydown', onKeyDown);
    var propBtn = document.getElementById('btn-fill-propagate');
    if (propBtn) propBtn.addEventListener('click', onPropagateClick);
    target.addEventListener('pointerup', onUp, { capture: true });
    target.addEventListener('pointercancel', onUp, { capture: true });
    document.addEventListener('pointerup', onDocUp);
    document.addEventListener('pointercancel', onDocUp);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
