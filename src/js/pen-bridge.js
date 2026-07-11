// ---- C7 CUTOVER STEP 7: Pen tool on the Rust engine ----
// Same capture-phase-interception architecture as the other bridges. Like
// the Eraser bridge, the Pen tool already mutates a real Paper.js Path
// continuously across the whole multi-click gesture (tools.js's _pen.path
// persists between clicks — each click either starts it, adds an anchor, or
// closes/finalizes it), so this bridge calls the exact same real functions
// (finalizePen, and inline replicas of the anchor-add/close logic) rather
// than deferring to one commit at the end. What's genuinely live-preview-only
// (never touches Paper's real path) is: the dashed rubber-band line from the
// last anchor to the cursor (shown via engine-bridge.js's
// buildPenPreviewItems, driven by setPenPreview here), and suspending
// tick() only for the brief click-and-drag-to-set-tangent-handle portion of
// each click (the multi-click gesture as a whole is NOT one continuous
// suspend — between clicks the tool is idle, tick() runs normally and
// picks up _pen.path's live segments same as any other layer content).
(function () {
  var draggingHandle = false;

  function shouldIntercept() {
    return (
      window.SMEngineBridge && window.SMEngineBridge.isEnabled() &&
      state.tool === 'pen' && !state.playing && !state.layers[state.activeLayerIdx].locked
    );
  }

  function onDown(e) {
    if (!shouldIntercept()) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var pt = new Point(w[0], w[1]);
    var layer = userLayers[state.activeLayerIdx];

    var now = Date.now();
    var isDoubleClick = _pen.path && (now - _pen.lastClickTime < 350) && _pen.lastClickPt && pt.getDistance(_pen.lastClickPt) < 10 / view.zoom;
    _pen.lastClickTime = now;
    _pen.lastClickPt = pt.clone();

    if (isDoubleClick) {
      finalizePen();
      window.SMEngineBridge.renderNow();
      return;
    }
    if (!_pen.path) {
      // Order matters — see draw-bridge.js's commitStroke comment: pushUndo()
      // must snapshot BEFORE ensureKeyframe() promotes the frame, so one
      // undo reverts both the new keyframe and the path together.
      pushUndo();
      ensureKeyframe();
      layer.activate();
      // Starting a click near an existing OPEN path's endpoint continues
      // THAT path (appending, or prepending in reverse order if it's the
      // path's start) instead of always beginning a brand new object —
      // Draw's Freehand-style auto-continuation (draw-bridge.js) already
      // does this for the brush; the Pen tool never had an equivalent, so
      // extending a hand-plotted line meant either re-closing/re-selecting
      // it via Subselect first, or living with two separate objects.
      var tolExt = 10 / view.zoom;
      var bestD = tolExt, bestP = null, bestEnd = null;
      layer.children.forEach(function (p) {
        if (!(p instanceof Path) || p.closed || !p.segments || p.segments.length < 1) return;
        if (p.data && (p.data.isVectorBrush || p.data.isFillShape)) return;
        var df = pt.getDistance(p.firstSegment.point), dl = pt.getDistance(p.lastSegment.point);
        if (df < bestD) { bestD = df; bestP = p; bestEnd = 'first'; }
        if (dl < bestD) { bestD = dl; bestP = p; bestEnd = 'last'; }
      });
      if (bestP) {
        _pen.path = bestP;
        if (bestEnd === 'first') {
          // Prepending: every further click must insert at the FRONT, so
          // flip the segment order once now and continue appending as
          // normal — simplest way to reuse the exact same "add a point at
          // the end" logic below for both directions.
          _pen.path.reverse();
        }
      } else {
        _pen.path = new Path();
        _pen.path.strokeColor = state.strokeColor;
        _pen.path.strokeWidth = state.brushSize;
        _pen.path.strokeCap = state.strokeCap;
        _pen.path.strokeJoin = state.strokeJoin;
        _pen.path.fillColor = null;
        _pen.path.opacity = state.opacity / 100;
        applyStrokeStyle(_pen.path);
        _pen.path.add(pt);
      }
    } else {
      var first = _pen.path.firstSegment.point;
      var tol = 10 / view.zoom;
      if (_pen.path.segments.length > 1 && pt.getDistance(first) < tol) {
        _pen.path.closed = true;
        finalizePen();
        window.SMEngineBridge.renderNow();
        return;
      }
      _pen.path.add(pt);
    }
    draggingHandle = true;
    window.SMEngineBridge.suspend();
    window.SMEngineBridge.renderNow();
  }

  function onMove(e) {
    if (!shouldIntercept()) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    window.SMEngineBridge.setPenPreview(w);
    if (draggingHandle && _pen.path) {
      var seg = _pen.path.lastSegment;
      var pt = new Point(w[0], w[1]);
      var delta = pt.subtract(seg.point);
      seg.handleOut = delta;
      // Alt/Option held: break the handle's symmetry (only the OUT side
      // moves, IN stays wherever it already was) — the standard Illustrator/
      // Figma way to end a smooth curve with a sharp corner right after it,
      // without switching tools. Same idiom subselect-bridge.js already
      // uses for editing handles after the fact; the Pen tool's own live
      // drag never had it.
      if (!e.altKey) seg.handleIn = delta.multiply(-1);
    }
    window.SMEngineBridge.renderNow();
  }

  function onUp(e) {
    // Only swallow this pointerup if OUR OWN handle-drag was actually in
    // progress — checking shouldIntercept() alone (as an earlier version of
    // this guard did) meant ANY pointerup bubbling through #canvas-area
    // while the Pen tool merely happened to be the active tool got
    // stopImmediatePropagation()'d/preventDefault()'d, even ones with
    // nothing to do with a pen drag (e.g. a UI scrub-drag release that
    // happens to land over the canvas) — silently eating events other
    // systems (like ui.js's pointer-capture-based scrub handler) were
    // relying on, and matching exactly the "value keeps changing no matter
    // what I do afterward" bug report: the scrub's own pointerup/endScrub
    // never ran because this handler intercepted it first.
    if (!draggingHandle) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    draggingHandle = false;
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
