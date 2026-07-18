// ---- C7 CUTOVER STEP 2: Draw + Fillbrush tools fully on the Rust engine (when enabled) ----
// Fillbrush (tools.js's 'fillbrush') is mechanically the exact same
// pressure-brush centerline capture as Draw's vector-brush mode (same
// _vb.pts/_vb.widths accumulation, same buildCenterSegmentsFromRawStroke +
// rebuildVectorBrushOutline at commit) — it only differs in painting the
// FILL color instead of the stroke color, skipping the taper-ends option
// entirely, marking the result data.isFillShape, and always inserting at
// the back of the layer (a fill-brush stroke is a background patch, not
// linework drawn on top) rather than only when drawMode==='behind'. Folded
// into this file rather than duplicated into its own, since every other
// line of logic (sampling, pressure, overlay preview, suspend/resume) is
// identical to Draw's own vector-brush path.
// engine-bridge.js's mirror still re-renders Paper.js's OWN scene graph
// changes every frame, so a stroke drawn through Paper.js during an active
// drag makes BOTH engines re-render on every mousemove — the real cost
// behind "ça rame pendant que je dessine". This module removes that for the
// Draw tool specifically: while dragging, pointer events are intercepted in
// the capture phase (stopImmediatePropagation blocks Paper's own handlers
// on the same canvas from ever seeing them), the in-progress stroke lives
// ONLY as a plain samples array rendered via engine.renderWithOverlayItem —
// Paper's scene graph is untouched, so Paper.js does zero work for the
// whole duration of the drag. Only at pointerup is a single real Path
// created in Paper (reusing the exact same centerline-building helpers
// tools.js's own Draw tool uses), so persistence/undo/tweening all still
// work exactly as before — one Paper scene mutation instead of one per
// mousemove.
(function () {
  var dragging = false;
  // Alt+drag brush-resize gesture (feedback #24): horizontal drag scales
  // state.brushSize with a live circle preview at the press point, like
  // every mainstream drawing app. viewtools-bridge.js's global
  // Alt-drag-to-rotate explicitly cedes Alt to the brushes for this.
  var sizing = false, sizeStartX = 0, sizeStartVal = 0, sizeAnchorW = null;
  var samples = []; // [x,y,width]
  // Perspective guide hard-lock (2026-07, perspective-bridge.js's
  // findGuideRayNear/projectOnRay — see their own header comment for why
  // this replaced a pure per-point magnet). Set once at pointerdown, held
  // for the whole gesture, cleared at pointerup.
  var lockedGuideRay = null;
  var lastMoveT = 0, lastWorldPt = null;
  var lastPenPressure = null; // held across a real-pen gesture, see pressureOf()
  // state.stabilizer (position-averaging while drawing) used to only exist
  // in tools.js's Paper-native onMouseDrag mirror — dead code in practice,
  // since this engine-bridge path is what actually runs whenever the Rust
  // engine is on (the default, almost always). Mirrors that same moving-
  // average-of-recent-points technique here, on the real raw pointer
  // stream, so the Stabilizer UI setting (off/low/med/high) finally does
  // something. Reset every stroke so it never drags in a stale point from
  // the previous gesture.
  var stabQueue = [];
  // Stroke-modeler mode (stabilizer values 4/5/6 → levels 1/2/3): the
  // spring-mass-damper input smoother ported from Google's
  // ink-stroke-modeler (stroke-modeler.js JS reference / strokemodeler.rs
  // Rust port, picked automatically by SMStrokeModeler.create). One
  // instance per gesture; onMove feeds it raw input and pushes its
  // (possibly several, upsampled) outputs; onUp runs its end-of-stroke
  // catch-up INSTEAD of the raw-point splice the moving average needs.
  var modeler = null;
  function modelerLevel() {
    return state.stabilizer >= 4 ? state.stabilizer - 3 : 0;
  }
  function stabilizePoint(w) {
    var stab = state.stabilizer;
    if (!stab) { stabQueue.length = 0; return w; }
    stabQueue.push(w);
    var maxQ = stab === 1 ? 3 : stab === 2 ? 6 : 10;
    while (stabQueue.length > maxQ) stabQueue.shift();
    var ax = 0, ay = 0;
    for (var i = 0; i < stabQueue.length; i++) { ax += stabQueue[i][0]; ay += stabQueue[i][1]; }
    return [ax / stabQueue.length, ay / stabQueue.length];
  }
  // Raw per-sample pressure (especially the speed-fallback formula, and
  // some stylus hardware) is noisy enough to visibly bump the variable-
  // width outline's edge — a one-pole low-pass filter (exponential moving
  // average) smooths that out while still tracking a deliberate press/
  // release fast enough to feel responsive, not laggy. Reset to null every
  // stroke so it snaps straight to the first real reading instead of
  // ramping up from 0 (a real pressed-down pen should register full weight
  // immediately, not fade in).
  var smoothedPressure = null;
  var PRESSURE_SMOOTH_ALPHA = 0.45;
  function smoothPressure(p) {
    if (smoothedPressure == null) { smoothedPressure = p; return p; }
    smoothedPressure += (p - smoothedPressure) * PRESSURE_SMOOTH_ALPHA;
    return smoothedPressure;
  }

  function shouldIntercept() {
    return (
      window.SMEngineBridge && window.SMEngineBridge.isEnabled() &&
      (state.tool === 'draw' || state.tool === 'fillbrush') && !state.playing &&
      !state.layers[state.activeLayerIdx].locked
    );
  }
  function isFillBrush() { return state.tool === 'fillbrush'; }
  // Real tablet pressure ("pression de brush avec tablet") was never
  // actually COMPUTED for the plain constant-width Draw branch — only
  // vectorBrush/fillBrush's own width-profile paths called pressureOf() at
  // all, everything else got a hardcoded 1 (full/max width, no
  // sensitivity). Bitmap Brush's live-preview code already multiplied by
  // a `pressureWidthMul` parameter threaded through from here — but it was
  // silently a no-op the whole time since this returned 1 unconditionally
  // for that case. Extending the gate to bitmap-brush mode is what
  // actually turns pressure sensitivity on for it, both live and baked
  // (samples[i][2] = widthFor(pressure) is what bitmap-brush.js's
  // applyBitmapBrushTexture reads to build bitmapPressureProfile).
  function wantsPressure() { return state.vectorBrush || isFillBrush() || (state.bitmapBrushOn && state.strokeEnabled); }

  // Mirrors vbPressureOf/vbWidthFor in tools.js, just reading a plain
  // PointerEvent + our own world-space samples instead of Paper's
  // event/tool-event objects (this stroke never becomes a live Paper item
  // until commit, so there's no Paper event to read pressure from).
  //
  // Unlike tools.js's version (which reads pressure off Paper.js's own
  // synthesized tool events, where a real mouse can end up reporting a
  // constant pressure of exactly 0.5 for spec-compatibility reasons — the
  // `!== 0.5` guard there exists to catch that), THIS function reads the
  // browser's raw PointerEvent directly, whose `pointerType` already tells
  // us definitively whether it's a real stylus. Once that's true, a 0.5
  // reading is a legitimate real pressure sample, not a mouse impostor —
  // rejecting it was a bug, not a safety check. Worse: rejecting it (or a
  // literal 0 reading, which most tablets send for the sample right as
  // contact ends at lift-off) fell through to the mouse-speed heuristic
  // below, which reads the natural deceleration of lifting the pen as
  // "slow movement = max pressure", ballooning a fat round blob exactly
  // where the stroke should taper to a fine point (reported: "un bout
  // comme ça au relâchement du stylet"). Fix: once a real pen sample has
  // been seen this gesture, HOLD that last true reading for any
  // zero/missing sample instead of ever falling back to the speed formula.
  function pressureOf(e, worldPt) {
    if (e.pointerType === 'pen') {
      if (typeof e.pressure === 'number' && e.pressure > 0) {
        lastPenPressure = Math.min(1, e.pressure);
        return lastPenPressure;
      }
      if (lastPenPressure != null) return lastPenPressure;
    }
    // Tauri/WKWebView: many tablet drivers (XP-Pen, Huion…) never surface
    // real pressure through PointerEvent.pressure in that webview — tools.js
    // already built two fallback channels for the vector-brush path
    // (webkitForce on synthesized mouse events, and a native AppKit reading
    // streamed from Rust as 'stylus-pressure') into the shared `_stylus`
    // object. This bridge draws with the Rust/vello engine and never
    // consulted them, so pressure silently degraded to the speed heuristic
    // below ONLY inside the desktop app, never in the browser preview where
    // Chrome's PointerEvent.pressure just works.
    if (typeof _stylus !== 'undefined' && _stylus.force > 0 && Date.now() - _stylus.forceT < 250) {
      lastPenPressure = _stylus.force;
      return _stylus.force;
    }
    var now = Date.now();
    var dt = Math.max(8, now - (lastMoveT || now));
    lastMoveT = now;
    var dist = lastWorldPt ? Math.hypot(worldPt[0] - lastWorldPt[0], worldPt[1] - lastWorldPt[1]) : 0;
    lastWorldPt = worldPt;
    var speed = dist / dt;
    return Math.max(0.15, 1 - Math.min(1, speed / 2.2));
  }
  function widthFor(p) {
    if (state.pressureInvert) p = 1 - p;
    // Response curve BEFORE the range mapping — same order as vbWidthFor
    // (tools.js), where the shared applyPressureCurve is defined; the two
    // pipelines must stay in phase (duplication-hazard convention).
    if (window.applyPressureCurve) p = window.applyPressureCurve(p);
    var lo = state.pressureMin / 100, hi = state.pressureMax / 100;
    // Fill Brush gets its own base size — it was silently sharing
    // state.brushSize with the Draw tool (and, one panel field further,
    // with the Pen tool's stroke width too), so changing either elsewhere
    // silently resized fill-brush strokes with no dedicated control of its
    // own.
    var base = isFillBrush() ? state.fillBrushSize : state.brushSize;
    return base * (lo + (hi - lo) * p);
  }
  // Sticks a freehand sample onto the nearest perspective guide line
  // (perspective-bridge.js) when it's within the magnet tolerance — no-op
  // (returns the point unchanged) whenever the guide is off or nothing's
  // close enough, so this is always safe to call unconditionally on every
  // sample of a Draw/Fillbrush stroke.
  function magnetSnap(w) {
    if (!window.perspectiveSnapPointMagnetic) return w;
    var snapped = window.perspectiveSnapPointMagnetic(new Point(w[0], w[1]));
    return snapped ? [snapped.x, snapped.y] : w;
  }
  // Hard directional lock (2026-07, perspective-bridge.js's
  // findGuideRayNear/projectOnRay) — call this instead of magnetSnap at
  // every point of a Draw/Fillbrush stroke. Locks onto whichever guide ray
  // is closest at the VERY FIRST call of a gesture (pointerdown) and then
  // hard-projects every later point onto that same ray for the rest of the
  // stroke, regardless of distance — falls back to the old per-point
  // magnetSnap when nothing was close enough to lock at the start (so a
  // stroke drawn nowhere near the guide still behaves exactly as before).
  function guideConstrain(w, isStart) {
    if (isStart) lockedGuideRay = window.perspectiveFindGuideRayNear ? window.perspectiveFindGuideRayNear(new Point(w[0], w[1])) : null;
    if (lockedGuideRay && window.perspectiveProjectOnRay) {
      var p = window.perspectiveProjectOnRay(new Point(w[0], w[1]), lockedGuideRay);
      return [p.x, p.y];
    }
    return magnetSnap(w);
  }
  function hexToRgba(css, opacityPct) {
    if (!css) return null;
    var h = css.replace('#', '');
    var r = parseInt(h.substr(0, 2), 16), g = parseInt(h.substr(2, 2), 16), b = parseInt(h.substr(4, 2), 16);
    // An 8-digit hex (#rrggbbaa) carries its own alpha byte — the color's
    // own transparency multiplies with the object's separate opacity slider
    // rather than one silently overriding the other.
    var hexA = h.length === 8 ? parseInt(h.substr(6, 2), 16) / 255 : 1;
    var op = (opacityPct !== undefined ? opacityPct / 100 : 1) * hexA;
    return [r, g, b, Math.round(255 * op)];
  }

  function overlayItem() {
    if (isFillBrush()) {
      return { centerline: samples, fillColor: hexToRgba(state.fillColor, state.opacity) };
    }
    if (state.vectorBrush) {
      var ribbon = { centerline: samples, fillColor: hexToRgba(state.strokeColor, state.opacity) };
      // Stroke eye OFF (left panel): the ribbon IS the stroke — preview only
      // the enclosed region's fill, matching what commitStroke() will
      // actually produce in that mode.
      var regionPreview = {
        segments: samples.map(function (s) { return { point: [s[0], s[1]], handleIn: [0, 0], handleOut: [0, 0] }; }),
        closed: true,
        fillColor: hexToRgba(state.fillColor, state.opacity),
      };
      if (!state.strokeEnabled && state.fillEnabled) return regionPreview;
      // Pressure-brush strokes are a filled ribbon painted with the STROKE
      // color — state.fillColor was never used in this mode, so with Fill
      // enabled the user saw "only the stroke" while drawing (the exact
      // reported bug for stylus users). With Fill on, also paint the region
      // enclosed by the centerline underneath the ribbon, same live-fill
      // behavior the plain-stroke mode already has.
      if (state.fillEnabled) {
        return [regionPreview, ribbon];
      }
      return ribbon;
    }
    // Plain (constant-width) stroke preview: render the raw polyline as a
    // thin stroked path — good enough for a live preview; the committed
    // Path gets Paper's own simplify()/applyStrokeStyle() treatment same
    // as today, so the FINAL result is unaffected by this simplification.
    // When Fill is enabled, Graphite-style tools paint the fill live too
    // (the canvas renderer fills an open path across the implicit
    // start-to-end closing edge, same as every vector app does).
    //
    // Raw pointermove samples land only a pixel or two apart at typical
    // mouse-move rates, producing runs of near-zero-length segments. Found
    // by testing: vello's stroke expansion (scene.stroke in engine.rs)
    // silently renders NOTHING for a many-segment path like that — a
    // 3-4-point test stroke rendered fine, a ~30-point freehand curve
    // showed the fill but no stroke at all, even at a thick width. fill()
    // tolerates the same degenerate geometry fine, which is why only the
    // stroke went missing. Filtering out samples closer than ~2 screen px
    // to the last KEPT point avoids feeding the stroker that degenerate
    // input — same visual fidelity (final commit still runs Paper's own
    // simplify() on the untouched raw `samples`), just fixes the live
    // preview.
    var minGap = 2 / view.zoom;
    var decimated = [samples[0]];
    for (var di = 1; di < samples.length; di++) {
      var last = decimated[decimated.length - 1];
      var s = samples[di];
      if (Math.hypot(s[0] - last[0], s[1] - last[1]) >= minGap) decimated.push(s);
    }
    if (decimated.length < 2 && samples.length > 1) decimated.push(samples[samples.length - 1]);
    var item = {
      segments: decimated.map(function (s) { return { point: [s[0], s[1]], handleIn: [0, 0], handleOut: [0, 0] }; }),
      closed: false,
      strokeColor: state.strokeEnabled ? hexToRgba(state.strokeColor, state.opacity) : null,
      strokeWidth: state.brushSize,
      strokeCap: state.strokeCap,
      strokeJoin: state.strokeJoin,
      miterLimit: state.miterLimit,
      dashPattern: state.strokeStyle === 'dashed' ? [state.brushSize * 2.5, state.brushSize * 1.8] : state.strokeStyle === 'dotted' ? [state.brushSize * 0.5, state.brushSize * 1.2] : undefined,
      dashOffset: state.dashOffset,
      paintOrder: state.paintOrder,
    };
    if (state.fillEnabled) item.fillColor = hexToRgba(state.fillColor, state.opacity);
    // No endpoint marker squares anymore: the Graphite-style white/blue
    // squares shown at the stroke's first/last point while drawing read as
    // visual noise during inking, not as a helpful affordance (feedback
    // #15 — "les carrés au début et à la fin de ligne qu'il faudrait
    // supprimer"). The live stroke preview itself is feedback enough.
    return [item];
  }
  // Labs live-preview hook (2026-07, "il faut que la symétrie soit visible
  // pendant le dessin en direct pas que au relâchement") — symmetry-mirror.js
  // previously only mirrored at commitStroke (the existing hook below),
  // meaning the reflection popped in only when the stroke was released.
  // overlayItem() is a pure function of the module-level `samples` array +
  // state.*, with no side effects — swapping `samples` for a mirrored/
  // rotated copy and calling it again reuses its EXACT shaping logic for
  // all three branches (fill-brush region, pressure-brush ribbon, plain
  // stroke) instead of a parallel geometry function per branch. Restored
  // immediately after, so the real drag state is never touched.
  function overlayItemFor(altSamples) {
    var backup = samples;
    samples = altSamples;
    var r = overlayItem();
    samples = backup;
    return r;
  }

  function onDown(e) {
    if (!shouldIntercept()) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    if (e.altKey) {
      sizing = true;
      sizeStartX = e.clientX;
      sizeStartVal = state.brushSize;
      sizeAnchorW = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
      window.SMEngineBridge.suspend();
      window.SMEngineBridge.setPressureCursor(sizeAnchorW, state.brushSize / 2, true);
      window.SMEngineBridge.renderNow();
      return;
    }
    dragging = true;
    samples = [];
    lastMoveT = 0; lastWorldPt = null; lastPenPressure = null;
    stabQueue = []; smoothedPressure = null;
    var w0 = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    w0 = guideConstrain(w0, true);
    // engine-bridge.js's own tick() loop keeps running unconditionally in
    // the background (it's not started/stopped per-drag) — without
    // suspending it here, it races renderWithOverlayItem below on every
    // single pointermove and erases the live overlay a frame after it's
    // drawn (see the `suspended` comment in engine-bridge.js for the full
    // story — this was the actual cause of "the stroke isn't visible while
    // drawing", not a resize/canvas-size issue).
    window.SMEngineBridge.suspend();
    var w = w0;
    var pressure = smoothPressure(wantsPressure() ? pressureOf(e, w) : 1);
    modeler = null;
    if (modelerLevel() && window.SMStrokeModeler) {
      // unitScale = view.zoom at stroke start: the wobble speed thresholds
      // and end-of-stroke stop distance are calibrated in SCREEN px.
      modeler = window.SMStrokeModeler.create(modelerLevel(), view.zoom);
      modeler.down(w[0], w[1], performance.now() / 1000, pressure);
    }
    samples.push([w[0], w[1], widthFor(pressure)]);
    if (state.vectorBrush) window.SMEngineBridge.setPressureCursor(w, widthFor(pressure) / 2);
    // Bitmap Brush live preview (bitmap-brush.js) — screen-space DOM canvas,
    // independent of the Rust engine's overlay-item JSON path (see that
    // file's header for why). Only for the same plain constant-width case
    // commitStroke's own bitmap-brush branch handles.
    if (state.bitmapBrushOn && state.strokeEnabled && !state.vectorBrush && !isFillBrush() && window.SMBitmapBrush) {
      window.SMBitmapBrush.beginLivePreview();
      window.SMBitmapBrush.livePreviewMove(e.clientX, e.clientY, pressure);
    }
  }
  function onMove(e) {
    if (sizing) {
      e.stopImmediatePropagation();
      e.preventDefault();
      // 0.5px of size per px of drag: p-sw only spans 1-80, a 1:1 mapping
      // would burn the whole range in a wrist flick.
      var ns = Math.max(1, Math.min(80, sizeStartVal + (e.clientX - sizeStartX) * 0.5));
      state.brushSize = ns;
      var sw = document.getElementById('p-sw'); if (sw) sw.value = Math.round(ns);
      window.SMEngineBridge.setPressureCursor(sizeAnchorW, ns / 2, true);
      window.SMEngineBridge.renderNow();
      return;
    }
    if (!dragging) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    w = guideConstrain(w, false);
    // Pressure is read from the RAW point (speed-fallback pressure needs
    // the real, un-averaged motion to feel right) — only the drawn
    // POSITION gets stabilized, matching TVPaint-style "smoothing" where
    // the line itself calms down without pressure lagging behind it.
    var pressure = smoothPressure(wantsPressure() ? pressureOf(e, w) : 1);
    if (modeler) {
      // The modeler upsamples: one raw input can yield several modeled
      // points. Width is computed per modeled point from its own carried
      // (interpolated) pressure, so the ribbon tracks the modeled line.
      var outs = modeler.move(w[0], w[1], performance.now() / 1000, pressure);
      for (var mi = 0; mi < outs.length; mi++) samples.push([outs[mi].x, outs[mi].y, widthFor(outs[mi].p)]);
      var lastOut = outs.length ? outs[outs.length - 1] : null;
      if (state.vectorBrush && lastOut) window.SMEngineBridge.setPressureCursor([lastOut.x, lastOut.y], widthFor(lastOut.p) / 2);
    } else {
      w = stabilizePoint(w);
      samples.push([w[0], w[1], widthFor(pressure)]);
      if (state.vectorBrush) window.SMEngineBridge.setPressureCursor(w, widthFor(pressure) / 2);
    }
    if (state.bitmapBrushOn && state.strokeEnabled && !state.vectorBrush && !isFillBrush() && window.SMBitmapBrush) {
      window.SMBitmapBrush.livePreviewMove(e.clientX, e.clientY, pressure);
    }
    var previewItems = [].concat(overlayItem());
    // Same guarded, no-op-when-absent pattern as the commitStroke hook
    // below — window.SMLabs is only present when Labs prototypes are
    // loaded, and buildDrawPreviewExtras itself no-ops per-prototype
    // unless that prototype is both registered AND currently on.
    if (window.SMLabs && window.SMLabs.buildDrawPreviewExtras) {
      previewItems = previewItems.concat(window.SMLabs.buildDrawPreviewExtras(samples, overlayItemFor));
    }
    // Symmetry guide (symmetry-bridge.js, 2026-07) — same live-preview seam
    // as the SMLabs call just above (reuses overlayItemFor to shape the
    // mirrored/rotated samples exactly like the real stroke), promoted out
    // of Labs into its own guarded, always-present hook since it's a real
    // shipped feature now, not a prototype.
    if (window.SMSymmetry && window.SMSymmetry.onPreview) {
      previewItems = previewItems.concat(window.SMSymmetry.onPreview(samples, overlayItemFor));
    }
    window.SMEngineBridge.renderWithOverlayItem(previewItems);
  }
  function onUp(e) {
    if (sizing) {
      e.stopImmediatePropagation();
      e.preventDefault();
      sizing = false;
      window.SMEngineBridge.setPressureCursor(null, 0);
      window.SMEngineBridge.resume();
      window.SMEngineBridge.renderNow();
      showToast('Taille du pinceau : ' + Math.round(state.brushSize) + 'px');
      return;
    }
    if (!dragging) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    dragging = false;
    // The position stabilizer above is a trailing moving average, so the
    // last sample pushed by onMove sits BEHIND the pen's true position by
    // design — without this, the committed stroke's tail would visibly stop
    // short of wherever the animator actually lifted the pen (the exact
    // opposite of "feels natural"). Push one final RAW, unstabilized point
    // at the true release position so the line always reaches it, same
    // catch-up behavior Photoshop/Clip Studio's stabilized brushes use.
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    w = guideConstrain(w, false);
    lockedGuideRay = null; // stroke over — next pointerdown re-evaluates from scratch
    var pressure = smoothPressure(wantsPressure() ? pressureOf(e, w) : 1);
    if (modeler) {
      // The modeler's own end-of-stroke catch-up (physics iterated until
      // the line converges onto the lift-off point) replaces the raw-point
      // splice below — that splice exists to fix the moving average's
      // trailing lag, which this mode doesn't have.
      var outs = modeler.up(w[0], w[1], performance.now() / 1000, pressure);
      for (var mi = 0; mi < outs.length; mi++) samples.push([outs[mi].x, outs[mi].y, widthFor(outs[mi].p)]);
      modeler = null;
    } else {
      samples.push([w[0], w[1], widthFor(pressure)]);
    }
    window.SMEngineBridge.resume();
    commitStroke();
    // Force an immediate clean render — without this, the endpoint marker
    // squares from the last overlay frame (renderWithOverlayItem) can still
    // be on screen if that rAF already fired before resume() got a chance to
    // cancel it; the ambient tick() loop would eventually clear them on its
    // own next frame, but forcing it here (same pattern as eraser-bridge.js/
    // select-bridge.js's own onUp) removes the race entirely.
    window.SMEngineBridge.renderNow();
  }

  // Hover cursor (not actively drawing): shows the brush at a neutral
  // mid-range pressure so the user can gauge its size/position before the
  // first stroke — same always-on convention as the eraser's cursor circle.
  // A separate bubble-phase listener (not capture, and explicitly skipped
  // while `dragging` so it never fights onMove's live pressure-driven size
  // during an actual stroke).
  function onHoverMove(e) {
    if (dragging || !window.SMEngineBridge || !window.SMEngineBridge.isEnabled()) return;
    if (state.tool !== 'draw' || !state.vectorBrush || state.playing) return;
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    window.SMEngineBridge.setPressureCursor(w, widthFor(0.5) / 2);
    window.SMEngineBridge.renderNow();
  }
  function clearHoverCursor() {
    if (window.SMEngineBridge) { window.SMEngineBridge.setPressureCursor(null, 0); window.SMEngineBridge.renderNow(); }
  }

  function commitStroke() {
    if (window.SMBitmapBrush) window.SMBitmapBrush.endLivePreview(); // clear the screen-space preview — the real baked Raster (if any) takes over below
    if (samples.length < 2) return;
    // Both paint channels switched off via the left-panel eyes — committing
    // would insert a fully invisible path (pollutes the layer, participates
    // in tween matching, un-hit-testable). Tell the user why instead.
    if (!isFillBrush() && !state.strokeEnabled && !state.fillEnabled) { showToast('Stroke et Fill désactivés — rien à dessiner'); return; }
    // pushUndo() BEFORE ensureKeyframe(): ensureKeyframe's auto-promotion of
    // an empty/interpolated frame to a real keyframe is itself a mutation —
    // snapshotting after it ran meant a single Cmd+Z only undid the new
    // stroke and left the frame stuck as an (now empty) keyframe; a SECOND
    // undo was needed to remove that, and it consumed the undo slot that
    // belonged to whatever was drawn before it. One undo must revert both.
    pushUndo();
    ensureKeyframe();
    var layer = userLayers[state.activeLayerIdx];
    layer.activate();
    var path;
    if (isFillBrush()) {
      var fbPts = samples.map(function (s) { return new Point(s[0], s[1]); });
      var fbWidths = samples.map(function (s) { return s[2]; });
      path = new Path();
      path.fillColor = state.fillColor;
      path.strokeColor = null;
      path.opacity = state.opacity / 100;
      path.data.isVectorBrush = true;
      path.data.isFillShape = true;
      // No taper-ends here — tools.js's own fillbrush commit never applies
      // it (taper is a Draw-tool-only option for linework endpoints; a fill
      // patch has no "ends" to taper).
      var fbCs = buildCenterSegmentsFromRawStroke(fbPts, fbWidths, state.smoothing);
      path.data.centerSegments = fbCs;
      path.data.widthProfile = fbCs.widthProfile;
      path.data.isVectorBrush = true; // scaffold flag ONLY for the duration of this call — cleared right below
      rebuildVectorBrushOutline(path);
      // Fill Brush is meant to draw a genuine filled SHAPE, editable like
      // any hand-plotted Pen path — real anchors sitting directly ON the
      // outline, standard node/tangent editing (Subselect). The centerline+
      // width-profile scaffolding above is only how the OUTLINE gets built;
      // once built, drop the isVectorBrush/centerSegments/widthProfile
      // linkage entirely so subselect-bridge.js's node editor takes the
      // ordinary path.segments branch instead of the special vector-brush
      // centerline-editing mode (which would otherwise let you drag a
      // handful of centerline anchors but never touch the outline's own
      // points directly — not what a filled shape's nodes should behave
      // like). isFillShape stays (still needed by pen-bridge.js's own
      // close-path/endpoint-snap exclusion guard).
      delete path.data.isVectorBrush;
      delete path.data.centerSegments;
      delete path.data.widthProfile;
      // Placement (Above/Below/Merge) — see applyFillBrushPlacement's own
      // comment; replaces the old unconditional "always at the back".
      applyFillBrushPlacement(path, userLayers[state.activeLayerIdx]);
    } else if (state.vectorBrush && !state.strokeEnabled) {
      // Stroke eye OFF + Fill ON: the pressure ribbon IS the stroke, so
      // drawing "fill seul" means committing only the region enclosed by
      // the drawn centerline, as an ordinary closed filled shape (no
      // isVectorBrush scaffolding — nothing ribbon-like survives). Mirrors
      // what the plain-brush mode gets for free below (open path + fill
      // renders just the enclosed region once strokeColor is null).
      var foPts = samples.map(function (s) { return new Point(s[0], s[1]); });
      var foWidths = samples.map(function (s) { return s[2]; });
      var foCs = buildCenterSegmentsFromRawStroke(foPts, foWidths, state.smoothing);
      path = new Path();
      foCs.forEach(function (s) { path.add(new Segment(new Point(s.point[0], s.point[1]), new Point(s.handleIn[0], s.handleIn[1]), new Point(s.handleOut[0], s.handleOut[1]))); });
      path.closed = true;
      path.fillColor = state.fillColor;
      path.strokeColor = null;
      path.opacity = state.opacity / 100;
      if (state.drawMode === 'behind') userLayers[state.activeLayerIdx].insertChild(0, path);
    } else if (state.vectorBrush) {
      var pts = samples.map(function (s) { return new Point(s[0], s[1]); });
      var widths = samples.map(function (s) { return s[2]; });
      path = new Path();
      path.fillColor = state.strokeColor;
      path.strokeColor = null;
      path.opacity = state.opacity / 100;
      path.data.isVectorBrush = true;
      var cs = buildCenterSegmentsFromRawStroke(pts, widths, state.smoothing);
      if (state.taperEnds) applyTaperToCenterSegments(cs, 0.15);
      path.data.centerSegments = cs;
      path.data.widthProfile = cs.widthProfile;
      // Fill enabled: also commit the region enclosed by the centerline as a
      // separate closed backdrop path under the ribbon (a single Paper path
      // can't be both the variable-width ribbon outline AND the enclosed-
      // region fill — same two-object precedent as the fillbrush's
      // background patches). LINKED via data.linkedFill (not just built
      // once and left to drift): rebuildVectorBrushOutline() below fills in
      // its segments from the exact same curve fit through the centerline
      // as the ribbon itself (rather than an independently re-simplified
      // copy of the raw points, which could end up a visibly different
      // curve, especially at tapered ends), and every future edit to this
      // stroke (node drag, scale, rotate — all funnel through
      // rebuildVectorBrushOutline; pure drag-to-move is handled at its own
      // two call sites) keeps re-syncing it — fixing the reported "fill
      // pas attaché au stroke" desync on top of the initial mismatch.
      if (state.fillEnabled) {
        var fillPath = new Path();
        fillPath.fillColor = state.fillColor;
        fillPath.strokeColor = null;
        fillPath.opacity = state.opacity / 100;
        fillPath.insertBelow(path);
        path.data.linkedFill = fillPath;
        // Reverse tag — marquee-select (select-bridge.js) iterates every
        // Path in the layer independently and had no way to recognize a
        // linkedFill backdrop as "not its own selectable thing, always
        // moves as part of its parent ribbon" — a marquee box that covered
        // both ended up with BOTH in selectedPaths as separate entries, so
        // the move handler's own "also translate p.data.linkedFill" logic
        // ran a SECOND time on the exact same fill object once the forEach
        // reached its own now-independent entry — translating it 2x delta
        // per tick against the ribbon's 1x, a real, growing divergence
        // ("parallax") between stroke and fill the longer/further a multi-
        // element drag went. This flag is how selectedPaths building code
        // excludes it from ever being added as its own entry.
        fillPath.data.isLinkedFillCompanion = true;
      }
      rebuildVectorBrushOutline(path);
      if (state.drawMode === 'behind') {
        userLayers[state.activeLayerIdx].insertChild(0, path);
        // insertChild moved only the ribbon — the backdrop fill (inserted
        // right below it a few lines up, back when the ribbon was still at
        // its original top-of-stack position) would otherwise get left
        // behind above it. Re-anchor it directly under the ribbon's new spot.
        if (path.data.linkedFill) path.data.linkedFill.insertBelow(path);
      }
      // Texture presets previously only existed for the plain constant-
      // width brush (applyBrushTexture couldn't size dabs off a single
      // baseWidth for a shape whose width varies along its length) — now
      // that it accepts a widthProfile, wire it up here too so a preset
      // actually applies to pressure strokes at draw time.
      if (state.brushPreset && state.brushPreset !== 'none') applyBrushTexture(path, state.brushPreset);
    } else {
      path = new Path();
      // Left-panel stroke eye honored here too (it always was for the
      // shape tools, never for the brush): stroke OFF + fill ON commits a
      // fill-only open path — the renderer paints just the enclosed region.
      path.strokeColor = state.strokeEnabled ? state.strokeColor : null;
      path.strokeWidth = state.brushSize;
      path.strokeCap = state.strokeCap;
      path.strokeJoin = state.strokeJoin;
      path.fillColor = state.fillEnabled ? state.fillColor : null;
      path.opacity = state.opacity / 100;
      applyStrokeStyle(path);
      samples.forEach(function (s) { path.add(new Point(s[0], s[1])); });
      // Paper.js's simplify(t) does `t||2.5` internally — 0 silently falls
      // back to its default tolerance, not a true no-op (tools.js's
      // simplifyIfNeeded, shared across every "Smooth" call site).
      simplifyIfNeeded(path,state.smoothing);
      // Brush preset texture (Chalk/Charcoal/Pencil) — only for this plain
      // constant-width mode, not vector-brush or fill-brush, which have
      // their own width-profile/outline machinery this jittered-copies
      // technique isn't built to coexist with. See applyBrushTexture's own
      // comment (tools.js) for how the "texture" is actually achieved in a
      // pure-vector renderer. Skipped when the stroke channel is off — the
      // dabs REPLACE a visible stroke, there's nothing to texture without one.
      // Bitmap Brush (v2, bitmap-brush.js) takes priority when enabled —
      // same anchor+companion architecture as applyBrushTexture, just one
      // Raster companion instead of many vector dabs; the path committed
      // above (fill included) stays as the real, subselect-editable anchor.
      if (state.strokeEnabled && state.bitmapBrushOn && window.SMBitmapBrush) window.SMBitmapBrush.applyToPath(path, null, samples);
      else if (state.strokeEnabled && state.brushPreset && state.brushPreset !== 'none') applyBrushTexture(path, state.brushPreset);
      if (state.drawMode === 'behind') {
        userLayers[state.activeLayerIdx].insertChild(0, path);
        // Same re-anchor need as the linkedFill case a few lines up in the
        // vector-brush branch — insertChild(0) only moves the primary copy,
        // stranding its texture companions at their old stacking position.
        if (path.data.brushCompanions) {
          path.data.brushCompanions.forEach(function (c) { c.insertBelow(path); });
        }
      }
    }
    if (state.shadowMode && path) path.data.channelTag = 'shadow';
    if (path) tagOwner(path);
    // Labs prototype hook (docs/feature-scouting.md) — no-op unless
    // window.SMLabs is loaded AND the relevant prototype's own flag is on.
    // Deliberately a single guarded call at the one point every
    // commitStroke branch funnels through, rather than a change per-branch
    // — keeps this file's own logic untouched when Labs is off.
    if (window.SMLabs && window.SMLabs.onStrokeCommitted && path) window.SMLabs.onStrokeCommitted(path, userLayers[state.activeLayerIdx]);
    // Symmetry guide (symmetry-bridge.js) — same single guarded call point
    // as the SMLabs hook just above, right before the frame is saved so the
    // mirrored/rotated copies are part of the same undo snapshot as the
    // original stroke.
    if (window.SMSymmetry && window.SMSymmetry.onStrokeCommitted && path) window.SMSymmetry.onStrokeCommitted(path, userLayers[state.activeLayerIdx]);
    saveActiveLayerFrame();
    // Stale-onion-ghost fix (see select-bridge.js's commit paths for the
    // full explanation) — a fresh stroke on a held frame can be exactly
    // what an onion-visible neighbor frame was about to inherit.
    renderOS();
    updateUI();
    samples = [];
  }

  // Escape aborts the in-progress stroke without committing it — same
  // affordance Pen already has (tools.js/timeline.js Escape handler), Draw
  // never had one since it always committed on pointerup with no way to
  // back out mid-drag.
  function onKeyDown(e) {
    if (e.key !== 'Escape' || !dragging) return;
    dragging = false;
    samples = [];
    if (window.SMBitmapBrush) window.SMBitmapBrush.endLivePreview();
    window.SMEngineBridge.resume();
    window.SMEngineBridge.renderNow();
  }

  function init() {
    var target = document.getElementById('canvas-area') || document.getElementById('drawing-canvas');
    // capture:true so this runs BEFORE Paper.js's own bubble-phase handlers
    // bound to the same canvas — stopImmediatePropagation then keeps Paper
    // from ever seeing the event at all while a Rust-side drag is live.
    target.addEventListener('pointerdown', onDown, { capture: true });
    target.addEventListener('pointermove', onMove, { capture: true });
    target.addEventListener('pointerup', onUp, { capture: true });
    target.addEventListener('pointercancel', onUp, { capture: true });
    document.addEventListener('keydown', onKeyDown);
    // Bubble phase (not capture) and gated on `!dragging` inside the handler
    // itself, so this never races the capture-phase onMove above during an
    // actual stroke — it only ever fires for hover moves onMove doesn't
    // already handle.
    target.addEventListener('pointermove', onHoverMove);
    target.addEventListener('pointerleave', clearHoverCursor);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
