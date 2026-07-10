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
  var lastPenPressure = null; // held across a real-pen gesture — same hold-last-value fix as draw-bridge.js's pressureOf(), for the exact same reason (0/missing samples at lift-off otherwise misread as "max pressure")

  function shouldIntercept() {
    return (
      window.SMEngineBridge && window.SMEngineBridge.isEnabled() &&
      state.tool === 'eraser' && !state.playing && !state.layers[state.activeLayerIdx].locked
    );
  }

  // A fixed-radius circle eraser can only ever cut a uniform-width channel
  // — the reference behavior (a natural, hand-drawn-looking mask with
  // varying-width gaps) needs the erase radius to actually respond to
  // stylus pressure the same way the pressure brush's ink width does, not
  // stay pinned to state.eraserSize regardless of how hard you press.
  // Mouse input (no real pressure signal) falls back to the plain nominal
  // radius rather than guessing from cursor speed — unlike the brush tools,
  // an eraser that silently changes width based on how fast you drag would
  // be a worse, more surprising default for a mouse user than just "always
  // the size the panel says".
  function eraseRadiusFor(e) {
    var base = state.eraserSize / 2;
    var p = null;
    if (e.pointerType === 'pen') {
      p = (typeof e.pressure === 'number' && e.pressure > 0) ? Math.min(1, e.pressure) : lastPenPressure;
    }
    // Tauri/WKWebView fallback — same native/webkitForce channel draw-bridge.js
    // now consults: on that webview, many tablet drivers synthesize plain
    // 'mouse' pointerType events (e.pointerType!=='pen' above), so real
    // pressure never reaches the branch above at all in the desktop app,
    // even though it works fine in the browser preview via real pen events.
    if (p == null && typeof _stylus !== 'undefined' && _stylus.force > 0 && Date.now() - _stylus.forceT < 250) {
      p = _stylus.force;
    }
    if (p == null) return base;
    lastPenPressure = p;
    var lo = state.pressureMin / 100, hi = state.pressureMax / 100;
    return base * (lo + (hi - lo) * p);
  }

  function eraseAt(pt, radius) {
    var layer = userLayers[state.activeLayerIdx];
    // Paper.js's hitTest does NOT prioritize a point genuinely INSIDE an
    // item's fill over a merely-within-tolerance proximity match on some
    // OTHER, unrelated item — it returns whichever qualifies first. At low
    // zoom, radius/view.zoom (the eraser's tolerance in world units) can
    // get huge, so a tiny leftover fragment from an earlier bite sitting
    // well outside the visible cursor circle can "steal" the hit away from
    // the actual shape under the cursor — the eraser then bites (and can
    // fully consume in one touch) that unrelated fragment while the real
    // target visually appears untouched, reading as "erases the whole fill
    // in one go" and "stops responding while still dragging" once nearby
    // stray fragments run out. An exact (zero-tolerance) hit — what the
    // user is actually, visually pointing at — always wins first; the
    // radius-derived tolerance is only a fallback for thin strokes the
    // cursor is merely near, not squarely on top of.
    var hit = layer.hitTest(pt, { stroke: true, fill: true, tolerance: 0 });
    if (!hit) hit = layer.hitTest(pt, { stroke: true, fill: true, tolerance: Math.max(8, radius) / view.zoom });
    // instanceof Path AND CompoundPath: eraseAtPoint turns a shape into a
    // CompoundPath the moment a bite creates a hole — an instanceof-Path-only
    // guard silently stopped erasing that same shape any further after the
    // first bite (hit-test still found it, the type check rejected it),
    // matching the reported "sometimes erases, sometimes not".
    if (hit && (hit.item instanceof Path || hit.item instanceof CompoundPath) && (hit.item.strokeColor || hit.item.fillColor || (hit.item.data && hit.item.data.isVectorBrush))) {
      if (!erasing) { pushUndo(); erasing = true; }
      var erasedItem = hit.item;
      eraseAtPoint(erasedItem, pt, radius, lastErasePt);
      lastErasePt = pt.clone();
      // The touched item, not null — fillRegenerateLinked can then skip
      // re-tracing fills whose fillWalls don't involve this stroke at all,
      // instead of unconditionally re-running fillVectorFind (a real
      // boundary walk) on EVERY fill bucket in the layer on every single
      // erase sample during a drag (perf audit finding).
      fillRegenerateLinked(layer, erasedItem);
      saveActiveLayerFrame();
      updateUI();
    }
  }

  function onDown(e) {
    if (!shouldIntercept()) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    ensureKeyframe();
    lastPenPressure = null;
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var radius = eraseRadiusFor(e);
    pointerIsDown = true;
    erasing = false;
    lastErasePt = null;
    window.SMEngineBridge.suspend();
    window.SMEngineBridge.setEraserCursor(w, radius);
    eraseAt(new Point(w[0], w[1]), radius);
    window.SMEngineBridge.renderNow();
  }
  function onMove(e) {
    if (!shouldIntercept()) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var radius = eraseRadiusFor(e);
    window.SMEngineBridge.setEraserCursor(w, radius);
    if (pointerIsDown) eraseAt(new Point(w[0], w[1]), radius);
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
