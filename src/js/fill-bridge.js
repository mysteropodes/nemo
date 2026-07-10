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
    ensureKeyframe();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var pt = new Point(w[0], w[1]);
    var layer = userLayers[state.activeLayerIdx];

    if (e.shiftKey) {
      var hitRm = layer.hitTest(pt, { fill: true, tolerance: 12 / view.zoom });
      if (hitRm && hitRm.item instanceof Path && hitRm.item.fillColor) {
        pushUndo();
        hitRm.item.fillColor = null;
        saveActiveLayerFrame();
        updateUI();
        showToast('Fill supprimé');
        window.SMEngineBridge.renderNow();
      }
      return;
    }

    var res = fillVectorFind(pt, layer, null);
    if (!res) {
      showToast('Aucune zone fermée ici');
      return;
    }
    pushUndo();
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
    saveActiveLayerFrame();
    updateUI();
    showToast('Fill appliqué');
    window.SMEngineBridge.renderNow();
  }

  function init() {
    var target = document.getElementById('canvas-area') || document.getElementById('drawing-canvas');
    target.addEventListener('pointerdown', onDown, { capture: true });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
