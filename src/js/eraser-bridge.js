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
  var lastErasePt = null; // world Point of the previous erase sample this gesture, or null for the first — fed to eraseAtPoint so it sweeps a continuous capsule instead of a lone circle per move (see eraseAtPoint's own comment for why)
  var lastPenPressure = null; // held across a real-pen gesture — same hold-last-value fix as draw-bridge.js's pressureOf(), for the exact same reason (0/missing samples at lift-off otherwise misread as "max pressure")
  // Alt+drag resize (2026-07, "10 > pourquoi on utilise pas le même
  // système que la brush ?") — draw-bridge.js already has this exact
  // gesture for the pressure brush (Alt+drag = live-preview resize instead
  // of drawing); the eraser only had bracket keys ([/]), an inconsistency
  // the user flagged directly. Same mechanics, reusing the eraser's own
  // hover-circle cursor (setEraserCursor) as the live-size preview instead
  // of draw-bridge's setPressureCursor — visually it's the same "this is
  // the circle you're about to erase with" affordance either way.
  var sizing = false, sizeStartX = 0, sizeStartVal = 0, sizeAnchorW = null;

  // Stabilizer (2026-08-27, feedback #74 — "il faudrait que la gomme
  // puisse se regler comme le pinceau, et se comporter comme le pinceau,
  // comme dans animate"): #tool-opts-sec (Stabilizer/Smooth) was ALREADY
  // shown for the eraser tool (TOOL_OPTS_TOOLS, timeline.js includes
  // 'eraser') — the dropdown was visibly settable and did nothing at all,
  // since nothing here ever read state.stabilizer. Mirrors draw-bridge.js's
  // own stabilizePoint() (same simple moving-average queue, same maxQ
  // steps) rather than sharing that closure-private function directly.
  // Only the moving-average levels (0-3, Off/Low/Medium/High) — the
  // higher "Plume" ink-stroke-modeler levels (4-6) need a stateful
  // upsampling modeler instance threaded through onDown/onMove/onUp,
  // real scope creep for what's fundamentally still a point-erase tool,
  // not a stroke-drawing one; a Plume setting is treated as its own
  // strongest moving-average level here instead of silently no-oping.
  var eraseStabQueue = [];
  function stabilizeErasePoint(w) {
    var stab = Math.min(3, state.stabilizer || 0);
    if (!stab) { eraseStabQueue.length = 0; return w; }
    eraseStabQueue.push(w);
    var maxQ = stab === 1 ? 3 : stab === 2 ? 6 : 10;
    while (eraseStabQueue.length > maxQ) eraseStabQueue.shift();
    var ax = 0, ay = 0;
    for (var i = 0; i < eraseStabQueue.length; i++) { ax += eraseStabQueue[i][0]; ay += eraseStabQueue[i][1]; }
    return [ax / eraseStabQueue.length, ay / eraseStabQueue.length];
  }

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
    // Bitmap Brush texture (bitmap-brush.js): the companion raster sits
    // ABOVE its anchor so it takes the hit — pixel-erase it in place
    // (raster-app semantics; eraseAtPoint's boolean pipeline would lose
    // the anchor's .data linkage, see eraseBite's own comment). The
    // vector-preset dabs need no such branch: they're Paths, the existing
    // flow below already bites them individually.
    if (hit && hit.item instanceof Raster && hit.item.data && hit.item.data.isBitmapBrush && hit.item.data.isBrushTextureCopy) {
      if (window.SMBitmapBrush) {
        SMBitmapBrush.eraseBite(hit.item, pt, radius, lastErasePt);
        lastErasePt = pt.clone();
        updateUI();
      }
      return;
    }
    // instanceof Path AND CompoundPath: eraseAtPoint turns a shape into a
    // CompoundPath the moment a bite creates a hole — an instanceof-Path-only
    // guard silently stopped erasing that same shape any further after the
    // first bite (hit-test still found it, the type check rejected it),
    // matching the reported "sometimes erases, sometimes not".
    if (hit && (hit.item instanceof Path || hit.item instanceof CompoundPath) && (hit.item.strokeColor || hit.item.fillColor || (hit.item.data && hit.item.data.isVectorBrush))) {
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
    if (e.altKey) {
      sizing = true;
      sizeStartX = e.clientX;
      sizeStartVal = state.eraserSize;
      sizeAnchorW = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
      window.SMEngineBridge.suspend();
      window.SMEngineBridge.setEraserCursor(sizeAnchorW, state.eraserSize / 2);
      window.SMEngineBridge.renderNow();
      return;
    }
    // pushUndo() BEFORE ensureKeyframe(), and unconditionally (not lazily
    // on the first actual hit like before) — see draw-bridge.js's
    // commitStroke comment for why the ordering matters: a single undo must
    // revert both the frame's auto-promotion to keyframe AND whatever this
    // gesture erases, as one step. Doing it lazily on first hit also left a
    // miss-only gesture's keyframe promotion permanently un-undoable.
    pushUndo();
    ensureKeyframe();
    lastPenPressure = null;
    eraseStabQueue.length = 0;
    var rawW = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var w = stabilizeErasePoint(rawW);
    var radius = eraseRadiusFor(e);
    pointerIsDown = true;
    erasing = false;
    lastErasePt = null;
    window.SMEngineBridge.suspend();
    // Feedback #93 ("le rond de guide de la gomme est en decalage par
    // rapport à la souris"): the cursor circle used the STABILIZED point
    // (a running average over up to 10 samples — see stabilizeErasePoint
    // above), which is exactly right for the actual erase path but visibly
    // lags the real cursor, worst with a high Stabilizer setting or a fast
    // stroke/stop. The guide now tracks the raw, unsmoothed point — only
    // the erase hit-test itself stays stabilized.
    window.SMEngineBridge.setEraserCursor(rawW, radius);
    eraseAt(new Point(w[0], w[1]), radius);
    window.SMEngineBridge.renderNow();
  }
  function onMove(e) {
    if (sizing) {
      e.stopImmediatePropagation();
      e.preventDefault();
      // Same 0.5px-per-px mapping as draw-bridge.js's brush resize — kept
      // identical on purpose so Alt-drag feels the same across tools.
      var ns = Math.max(2, sizeStartVal + (e.clientX - sizeStartX) * 0.5);
      state.eraserSize = ns;
      var es = document.getElementById('p-erasersize'); if (es) es.value = Math.round(ns);
      window.SMEngineBridge.setEraserCursor(sizeAnchorW, ns / 2);
      window.SMEngineBridge.renderNow();
      return;
    }
    if (!shouldIntercept()) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var rawW = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var w = stabilizeErasePoint(rawW);
    var radius = eraseRadiusFor(e);
    window.SMEngineBridge.setEraserCursor(rawW, radius); // raw point — see onDown's comment on this
    if (pointerIsDown) eraseAt(new Point(w[0], w[1]), radius);
    window.SMEngineBridge.renderNow();
  }
  function onUp(e) {
    if (sizing) {
      e.stopImmediatePropagation();
      e.preventDefault();
      sizing = false;
      window.SMEngineBridge.resume();
      window.SMEngineBridge.renderNow();
      showToast('Taille de la gomme : ' + Math.round(state.eraserSize) + 'px');
      return;
    }
    if (!pointerIsDown) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    pointerIsDown = false;
    erasing = false;
    lastErasePt = null;
    // Bitmap Brush bites deferred their (expensive) data.src refresh to
    // gesture end — flush + persist once here, see eraseBite's comment.
    if (window.SMBitmapBrush) {
      SMBitmapBrush.flushEraseDirty(userLayers[state.activeLayerIdx]);
      saveActiveLayerFrame();
    }
    // Stale-onion-ghost fix (same family as select-bridge.js/subselect-
    // bridge.js) — once per erase GESTURE, not per sample: onionPrevLayer/
    // onionNextLayer are a snapshot cache never rebuilt by an edit commit,
    // only by frame nav/layer changes. Deliberately placed here (onUp) and
    // not in onMove's per-sample saveActiveLayerFrame() above — renderOS()
    // rebuilds by walking every layer's onion-range frames, too expensive
    // to run at stylus sampling rate (same reasoning as this file's own
    // deferred Bitmap Brush refresh just above).
    renderOS();
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
