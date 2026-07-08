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
  var samples = []; // [x,y,width]
  var lastMoveT = 0, lastWorldPt = null;
  var lastPenPressure = null; // held across a real-pen gesture, see pressureOf()
  var extendTarget = null; // {path, end:'first'|'last'} — set at pointerdown when starting near an open path's endpoint

  // Graphite's Freehand tool auto-continues an open path when you start a
  // new stroke near one of its endpoints, instead of always creating a
  // fresh object. Only the plain constant-width stroke mode (not vector
  // brush / fillbrush, whose centerline data isn't a simple point list to
  // splice) supports this — same tolerance convention as the Pen tool's
  // own close-path snap (10px screen radius, tools.js/pen-bridge.js).
  function findOpenEndpointNear(worldPt) {
    if (state.vectorBrush || isFillBrush()) return null;
    var layer = userLayers[state.activeLayerIdx];
    if (!layer || layer.locked) return null;
    var tol = 10 / view.zoom;
    var pt = new Point(worldPt[0], worldPt[1]);
    var best = null, bestD = tol;
    layer.children.forEach(function (p) {
      if (!(p instanceof Path) || p.closed || !p.segments || p.segments.length < 2) return;
      if (p.data && (p.data.isVectorBrush || p.data.isFillShape)) return;
      var df = pt.getDistance(p.firstSegment.point), dl = pt.getDistance(p.lastSegment.point);
      if (df < bestD) { bestD = df; best = { path: p, end: 'first' }; }
      if (dl < bestD) { bestD = dl; best = { path: p, end: 'last' }; }
    });
    return best;
  }

  function shouldIntercept() {
    return (
      window.SMEngineBridge && window.SMEngineBridge.isEnabled() &&
      (state.tool === 'draw' || state.tool === 'fillbrush') && !state.playing &&
      !state.layers[state.activeLayerIdx].locked
    );
  }
  function isFillBrush() { return state.tool === 'fillbrush'; }

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
    var lo = state.pressureMin / 100, hi = state.pressureMax / 100;
    return state.brushSize * (lo + (hi - lo) * p);
  }
  function hexToRgba(css, opacityPct) {
    if (!css) return null;
    var h = css.replace('#', '');
    var r = parseInt(h.substr(0, 2), 16), g = parseInt(h.substr(2, 2), 16), b = parseInt(h.substr(4, 2), 16);
    return [r, g, b, Math.round(255 * (opacityPct !== undefined ? opacityPct / 100 : 1))];
  }

  function overlayItem() {
    if (isFillBrush()) {
      return { centerline: samples, fillColor: hexToRgba(state.fillColor, state.opacity) };
    }
    if (state.vectorBrush) {
      var ribbon = { centerline: samples, fillColor: hexToRgba(state.strokeColor, state.opacity) };
      // Pressure-brush strokes are a filled ribbon painted with the STROKE
      // color — state.fillColor was never used in this mode, so with Fill
      // enabled the user saw "only the stroke" while drawing (the exact
      // reported bug for stylus users). With Fill on, also paint the region
      // enclosed by the centerline underneath the ribbon, same live-fill
      // behavior the plain-stroke mode already has.
      if (state.fillEnabled) {
        return [
          {
            segments: samples.map(function (s) { return { point: [s[0], s[1]], handleIn: [0, 0], handleOut: [0, 0] }; }),
            closed: true,
            fillColor: hexToRgba(state.fillColor, state.opacity),
          },
          ribbon,
        ];
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
      strokeColor: hexToRgba(state.strokeColor, state.opacity),
      strokeWidth: state.brushSize,
      strokeCap: state.strokeCap,
      strokeJoin: state.strokeJoin,
      miterLimit: state.miterLimit,
      dashPattern: state.strokeStyle === 'dashed' ? [state.brushSize * 2.5, state.brushSize * 1.8] : state.strokeStyle === 'dotted' ? [state.brushSize * 0.5, state.brushSize * 1.2] : undefined,
      dashOffset: state.dashOffset,
      paintOrder: state.paintOrder,
    };
    if (state.fillEnabled) item.fillColor = hexToRgba(state.fillColor, state.opacity);
    // Endpoint markers (first/last point), shown live while drawing — same
    // small square handles Graphite's Freehand tool overlays on its path
    // endpoints. World-space half-size scaled by zoom so they read as a
    // constant on-screen size regardless of canvas zoom level.
    var hs = 4 / view.zoom;
    function markerSquare(x, y) {
      return {
        segments: [[-hs, -hs], [hs, -hs], [hs, hs], [-hs, hs]].map(function (o) { return { point: [x + o[0], y + o[1]], handleIn: [0, 0], handleOut: [0, 0] }; }),
        closed: true,
        fillColor: [255, 255, 255, 255],
        strokeColor: [74, 158, 255, 255],
        strokeWidth: 1.5 / view.zoom,
      };
    }
    var items = [item];
    if (samples.length) {
      items.push(markerSquare(samples[0][0], samples[0][1]));
      var last = samples[samples.length - 1];
      if (samples.length > 1) items.push(markerSquare(last[0], last[1]));
    }
    return items;
  }

  function onDown(e) {
    if (!shouldIntercept()) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    dragging = true;
    samples = [];
    lastMoveT = 0; lastWorldPt = null; lastPenPressure = null;
    var w0 = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    extendTarget = findOpenEndpointNear(w0);
    // engine-bridge.js's own tick() loop keeps running unconditionally in
    // the background (it's not started/stopped per-drag) — without
    // suspending it here, it races renderWithOverlayItem below on every
    // single pointermove and erases the live overlay a frame after it's
    // drawn (see the `suspended` comment in engine-bridge.js for the full
    // story — this was the actual cause of "the stroke isn't visible while
    // drawing", not a resize/canvas-size issue).
    window.SMEngineBridge.suspend();
    // Snap the very first sample onto the target endpoint (not the raw
    // click point) so the extended path has no visible gap/jump at the
    // seam — mirrors Graphite's should_extend behavior.
    var w = extendTarget ? [extendTarget.path[extendTarget.end === 'first' ? 'firstSegment' : 'lastSegment'].point.x, extendTarget.path[extendTarget.end === 'first' ? 'firstSegment' : 'lastSegment'].point.y] : w0;
    var pressure = (state.vectorBrush || isFillBrush()) ? pressureOf(e, w) : 1;
    samples.push([w[0], w[1], widthFor(pressure)]);
    if (state.vectorBrush) window.SMEngineBridge.setPressureCursor(w, widthFor(pressure) / 2);
  }
  function onMove(e) {
    if (!dragging) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var pressure = (state.vectorBrush || isFillBrush()) ? pressureOf(e, w) : 1;
    samples.push([w[0], w[1], widthFor(pressure)]);
    if (state.vectorBrush) window.SMEngineBridge.setPressureCursor(w, widthFor(pressure) / 2);
    window.SMEngineBridge.renderWithOverlayItem(overlayItem());
  }
  function onUp(e) {
    if (!dragging) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    dragging = false;
    window.SMEngineBridge.resume();
    commitStroke();
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
    if (samples.length < 2) return;
    ensureKeyframe();
    pushUndo();
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
      rebuildVectorBrushOutline(path);
      // Placement (Above/Below/Merge) — see applyFillBrushPlacement's own
      // comment; replaces the old unconditional "always at the back".
      applyFillBrushPlacement(path, userLayers[state.activeLayerIdx]);
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
    } else if (extendTarget) {
      // Auto-continue: splice the new samples onto the existing open path's
      // endpoint instead of creating a separate object.
      path = extendTarget.path;
      var newPts = samples.map(function (s) { return new Segment(new Point(s[0], s[1])); });
      if (extendTarget.end === 'last') {
        newPts.slice(1).forEach(function (seg) { path.add(seg); });
      } else {
        // Prepending onto the start: insert in reverse order (excluding the
        // duplicate anchor sample) so the new points read start-to-end.
        newPts.slice(1).reverse().forEach(function (seg) { path.insertSegments(0, [seg]); });
      }
      path.fillColor = state.fillEnabled ? state.fillColor : null;
      path.simplify(state.smoothing);
    } else {
      path = new Path();
      path.strokeColor = state.strokeColor;
      path.strokeWidth = state.brushSize;
      path.strokeCap = state.strokeCap;
      path.strokeJoin = state.strokeJoin;
      path.fillColor = state.fillEnabled ? state.fillColor : null;
      path.opacity = state.opacity / 100;
      applyStrokeStyle(path);
      samples.forEach(function (s) { path.add(new Point(s[0], s[1])); });
      path.simplify(state.smoothing);
      if (state.drawMode === 'behind') userLayers[state.activeLayerIdx].insertChild(0, path);
    }
    saveActiveLayerFrame();
    updateUI();
    samples = [];
    extendTarget = null;
  }

  // Escape aborts the in-progress stroke without committing it — same
  // affordance Pen already has (tools.js/timeline.js Escape handler), Draw
  // never had one since it always committed on pointerup with no way to
  // back out mid-drag.
  function onKeyDown(e) {
    if (e.key !== 'Escape' || !dragging) return;
    dragging = false;
    samples = [];
    extendTarget = null;
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
