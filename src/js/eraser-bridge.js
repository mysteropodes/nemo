// ---- C7 CUTOVER STEP 5: Eraser tool on the Rust engine ----
// Same capture-phase-interception architecture as the other bridges, but
// simpler in one respect: tools.js's eraser already mutates real Paper.js
// geometry continuously during the drag (eraseAtPoint runs a real boolean
// subtract on every pointermove, not just at pointerup like Draw/Shapes/
// Select), so there's no separate "commit at the end" step to add — this
// bridge just calls the exact same real functions tools.js's own eraser
// mousedown/mousemove handlers call, then asks engine-bridge.js to render
// the result immediately (buildSceneJson() picks up the real, now-erased
// geometry on its own). What IS ported is the hover cursor circle
// (eraseUpdateCursor() in tools.js draws it via a real, invisible-under-
// the-rust-canvas Paper guide item) and eliminating the double-render cost
// during the drag via suspend()/resume(), matching the other bridges.
(function () {
  var pointerIsDown = false; // gesture lifecycle (suspend/resume span)
  var erasing = false; // true once the FIRST successful hit of this gesture has fired (gates the one-time pushUndo, matching _eraseDragActive in tools.js)
  var lastErasePt = null; // world Point of the previous erase sample this gesture, or null for the first — fed to eraseAtPoint so it sweeps a continuous capsule instead of a lone circle per move (see eraseAtPoint's own comment for why)

  function shouldIntercept() {
    return (
      window.SMEngineBridge && window.SMEngineBridge.isEnabled() &&
      state.tool === 'eraser' && !state.playing && !state.layers[state.activeLayerIdx].locked
    );
  }

  function eraseAt(pt) {
    var layer = userLayers[state.activeLayerIdx];
    var hit = layer.hitTest(pt, { stroke: true, fill: true, tolerance: Math.max(8, state.eraserSize / 2) / view.zoom });
    // instanceof Path AND CompoundPath: eraseAtPoint turns a shape into a
    // CompoundPath the moment a bite creates a hole — an instanceof-Path-only
    // guard silently stopped erasing that same shape any further after the
    // first bite (hit-test still found it, the type check rejected it),
    // matching the reported "sometimes erases, sometimes not".
    if (hit && (hit.item instanceof Path || hit.item instanceof CompoundPath) && (hit.item.strokeColor || hit.item.fillColor || (hit.item.data && hit.item.data.isVectorBrush))) {
      if (!erasing) { pushUndo(); erasing = true; }
      eraseAtPoint(hit.item, pt, state.eraserSize / 2, lastErasePt);
      lastErasePt = pt.clone();
      fillRegenerateLinked(layer, null);
      saveActiveLayerFrame();
      updateUI();
    }
  }

  function onDown(e) {
    if (!shouldIntercept()) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    ensureKeyframe();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    pointerIsDown = true;
    erasing = false;
    lastErasePt = null;
    window.SMEngineBridge.suspend();
    window.SMEngineBridge.setEraserCursor(w);
    eraseAt(new Point(w[0], w[1]));
    window.SMEngineBridge.renderNow();
  }
  function onMove(e) {
    if (!shouldIntercept()) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    window.SMEngineBridge.setEraserCursor(w);
    if (pointerIsDown) eraseAt(new Point(w[0], w[1]));
    window.SMEngineBridge.renderNow();
  }
  function onUp(e) {
    if (!pointerIsDown) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    pointerIsDown = false;
    erasing = false;
    lastErasePt = null;
    window.SMEngineBridge.resume();
    window.SMEngineBridge.renderNow();
  }

  function init() {
    var target = document.getElementById('canvas-area') || document.getElementById('drawing-canvas');
    target.addEventListener('pointerdown', onDown, { capture: true });
    target.addEventListener('pointermove', onMove, { capture: true });
    target.addEventListener('pointerup', onUp, { capture: true });
    target.addEventListener('pointercancel', onUp, { capture: true });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
