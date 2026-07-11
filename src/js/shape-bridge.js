// ---- C7 CUTOVER STEP 4: Shape tools (Line/Rect/Ellipse) on the Rust engine ----
// Same architecture as draw-bridge.js: capture-phase pointer interception,
// tick() suspended for the drag, a plain scene-JSON overlay item drives the
// live preview, and only ONE real Paper.js item is created — at pointerup —
// instead of tools.js's original approach of destroying and recreating a
// real Path.Line/Rectangle/Ellipse on every single pointermove. Faithful
// port of the 'line'/'rect'/'ellipse' branches of onMouseDown/Up in
// tools.js: `ensureKeyframe()`/`pushUndo()` still fire at the START of the
// drag (matching the original's timing exactly, not deferred to commit),
// and the <2px-drag-discards-the-shape behavior at the end is preserved.
(function () {
  var dragging = false;
  var shapeStart = null; // world [x,y]
  var shapeTool = null;

  function shouldIntercept() {
    return (
      window.SMEngineBridge && window.SMEngineBridge.isEnabled() &&
      (state.tool === 'line' || state.tool === 'rect' || state.tool === 'ellipse') &&
      !state.playing && !state.layers[state.activeLayerIdx].locked
    );
  }

  function hexToRgba(css, opacityPct) {
    if (!css) return null;
    var h = css.replace('#', '');
    var r = parseInt(h.substr(0, 2), 16), g = parseInt(h.substr(2, 2), 16), b = parseInt(h.substr(4, 2), 16);
    // An 8-digit hex (#rrggbbaa) carries its own alpha byte — multiply it
    // with the object's separate opacity slider rather than ignoring it.
    var hexA = h.length === 8 ? parseInt(h.substr(6, 2), 16) / 255 : 1;
    var op = (opacityPct !== undefined ? opacityPct / 100 : 1) * hexA;
    return [r, g, b, Math.round(255 * op)];
  }

  // Geometry (segments/closed) comes from geometry-wasm's line_segments/
  // rect_segments/ellipse_segments — the SAME Rust functions used for both
  // the live drag overlay below AND the final committed Path in
  // commitShape(), so the shape the user sees while dragging is pixel-for-
  // pixel the shape that gets committed, not two independently-implemented
  // approximations of an ellipse that could theoretically drift apart (the
  // overlay used to hand-roll its own kappa-constant bezier math in JS,
  // while the committed shape came from Paper.js's own separate
  // Path.Ellipse/Rectangle implementation). Falls back to the same JS math
  // if WASM isn't ready — same safety pattern as fill/erase/boolean.
  var KAPPA = 0.5522847498;
  function ellipseGeomJS(x0, y0, x1, y1) {
    var cx = (x0 + x1) / 2, cy = (y0 + y1) / 2;
    var rx = Math.abs(x1 - x0) / 2, ry = Math.abs(y1 - y0) / 2;
    var kx = rx * KAPPA, ky = ry * KAPPA;
    return {
      segments: [
        { point: [cx + rx, cy], handleIn: [0, -ky], handleOut: [0, ky] },
        { point: [cx, cy + ry], handleIn: [kx, 0], handleOut: [-kx, 0] },
        { point: [cx - rx, cy], handleIn: [0, ky], handleOut: [0, -ky] },
        { point: [cx, cy - ry], handleIn: [-kx, 0], handleOut: [kx, 0] },
      ],
      closed: true,
    };
  }
  function rectGeomJS(x0, y0, x1, y1) {
    var l = Math.min(x0, x1), r = Math.max(x0, x1), t = Math.min(y0, y1), b = Math.max(y0, y1);
    return { segments: [{ point: [l, t] }, { point: [r, t] }, { point: [r, b] }, { point: [l, b] }], closed: true };
  }
  function lineGeomJS(x0, y0, x1, y1) {
    return { segments: [{ point: [x0, y0] }, { point: [x1, y1] }], closed: false };
  }
  function shapeGeom(tool, x0, y0, x1, y1) {
    var wasm = window.GeometryWasm;
    if (wasm && wasm.ready) {
      try {
        if (tool === 'line') return JSON.parse(wasm.line_segments(x0, y0, x1, y1));
        if (tool === 'rect') return JSON.parse(wasm.rect_segments(x0, y0, x1, y1));
        return JSON.parse(wasm.ellipse_segments(x0, y0, x1, y1));
      } catch (e) { console.warn('[geometry-wasm] shape_segments failed, falling back to JS', e); }
    }
    if (tool === 'line') return lineGeomJS(x0, y0, x1, y1);
    if (tool === 'rect') return rectGeomJS(x0, y0, x1, y1);
    return ellipseGeomJS(x0, y0, x1, y1);
  }

  function overlayItem(x1, y1) {
    var sc = state.strokeEnabled ? hexToRgba(state.strokeColor, state.opacity) : null;
    var fc = state.fillEnabled ? hexToRgba(state.fillColor, state.opacity) : null;
    var geom = shapeGeom(shapeTool, shapeStart[0], shapeStart[1], x1, y1);
    return {
      segments: geom.segments, closed: geom.closed,
      fillColor: shapeTool === 'line' ? null : fc,
      strokeColor: sc, strokeWidth: state.brushSize,
    };
  }

  function onDown(e) {
    if (!shouldIntercept()) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    // Order matters — see draw-bridge.js's commitStroke comment: pushUndo()
    // must snapshot BEFORE ensureKeyframe() promotes the frame, so one
    // undo reverts both the new keyframe and the shape together.
    pushUndo();
    ensureKeyframe();
    dragging = true;
    shapeTool = state.tool;
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    shapeStart = [w[0], w[1]];
    window.SMEngineBridge.suspend();
    window.SMEngineBridge.renderWithOverlayItem(overlayItem(w[0], w[1]));
  }
  // Perspective-guide snapping (perspective-bridge.js) only makes sense for
  // the Line tool — a straight ruler line drawn roughly toward a vanishing
  // point locks onto it exactly, same as Sketchbook's perspective guide
  // cursor. Rect/Ellipse have no single "direction" to snap. No-ops
  // (returns the point unchanged) when the guide is off or nothing's close
  // enough in angle, so this is always safe to call unconditionally.
  function maybeSnap(wx, wy) {
    if (shapeTool !== 'line' || !window.perspectiveSnapPoint || !shapeStart) return [wx, wy];
    var snapped = window.perspectiveSnapPoint(new Point(shapeStart[0], shapeStart[1]), new Point(wx, wy));
    return [snapped.x, snapped.y];
  }
  function onMove(e) {
    if (!dragging) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var s = maybeSnap(w[0], w[1]);
    window.SMEngineBridge.renderWithOverlayItem(overlayItem(s[0], s[1]));
  }
  function onUp(e) {
    if (!dragging) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    dragging = false;
    window.SMEngineBridge.resume();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var s = maybeSnap(w[0], w[1]);
    commitShape(s[0], s[1]);
  }

  function commitShape(ex, ey) {
    var start = new Point(shapeStart[0], shapeStart[1]);
    var end = new Point(ex, ey);
    var layer = userLayers[state.activeLayerIdx];
    layer.activate();
    var geom = shapeGeom(shapeTool, start.x, start.y, end.x, end.y);
    var path = new Path();
    geom.segments.forEach(function (sd) {
      path.add(new Segment(new Point(sd.point[0], sd.point[1]),
        sd.handleIn ? new Point(sd.handleIn[0], sd.handleIn[1]) : new Point(0, 0),
        sd.handleOut ? new Point(sd.handleOut[0], sd.handleOut[1]) : new Point(0, 0)));
    });
    path.closed = geom.closed;
    path.strokeColor = state.strokeEnabled ? state.strokeColor : null;
    path.strokeWidth = state.brushSize;
    path.opacity = state.opacity / 100;
    if (shapeTool === 'line') {
      path.fillColor = null;
      path.strokeCap = state.strokeCap;
      applyStrokeStyle(path);
    } else {
      path.fillColor = state.fillEnabled ? state.fillColor : null;
    }
    if (start.getDistance(end) < 2) {
      path.remove();
      if (state.undoStack.length) state.undoStack.pop();
    } else {
      if (state.shadowMode) path.data.channelTag = 'shadow';
      tagOwner(path);
    }
    saveActiveLayerFrame();
    updateUI();
    shapeStart = null; shapeTool = null;
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
