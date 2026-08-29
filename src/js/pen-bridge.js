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
  // Set only by the Alt+drag-on-an-existing-anchor gesture below — tells
  // onMove to reshape THAT stored segment instead of the path's own
  // lastSegment (the normal placement-drag target). Cleared in onUp.
  var reshapingSeg = null;
  var canvasEl = null; // set in init(), read by the cursor-affordance hover check in onMove

  // Motion transform (2026-08-29, feedback #135: "si je dessine dans le
  // layer dont la position a été changé dans motion... le dessin se
  // recale") — same fix/reasoning as draw-bridge.js's toLocalPoint (see its
  // header comment for the full story). Unlike Draw/Shape, the Pen tool
  // mutates a real Paper.js Path (_pen.path, shared with tools.js)
  // continuously across the whole multi-click gesture rather than
  // deferring to one commit at the end — every anchor placement and
  // proximity/tangent check against _pen.path's existing (raw, local-
  // space) segments below must map the pointer's rendered/world position
  // through this FIRST, or a Motion-transformed layer places each new
  // anchor at its raw click position instead of the equivalent local one
  // (same failure mode subselect-bridge.js/rig-bridge.js already document
  // for hit-testing). setPenPreview's rubber-band line is deliberately fed
  // the UNMAPPED world point (w) instead — it's a live overlay, must track
  // the cursor at its true on-screen position.
  function toLocalPoint(pt, layerIdx) {
    if (!window.SMMotion) return pt;
    var map = SMMotion.layerMotionPointMap ? SMMotion.layerMotionPointMap(layerIdx) : null;
    if (!map && SMMotion.layerMotion3DPointMap) map = SMMotion.layerMotion3DPointMap(layerIdx);
    if (!map) return pt;
    var lp = map.inv(pt.x, pt.y);
    return new Point(lp[0], lp[1]);
  }

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
    // Motion transform fix (feedback #135, see toLocalPoint's header
    // comment above) — every anchor placed/hit-tested below must live in
    // the same raw document space as _pen.path's existing segments.
    var pt = toLocalPoint(new Point(w[0], w[1]), state.activeLayerIdx);
    var layer = userLayers[state.activeLayerIdx];

    // Alt+drag directly on an ALREADY-PLACED anchor of the in-progress path
    // reshapes that anchor's outgoing tangent handle without switching to
    // Subselect first — the standard Illustrator/AE Pen convention for
    // going back and adjusting an earlier point's curve mid-path (feedback
    // #38, "on voit les vecteurs et tangentes... [ça devrait fonctionner]
    // comme dans n'importe quel soft de vecto"). Distinct from the
    // pre-existing Alt behavior in onMove (breaking a JUST-dragged handle's
    // symmetry while placing a brand new anchor) — this one targets an
    // anchor by PROXIMITY, gated on _pen.path already existing, so it never
    // fires on the very first click of a fresh path.
    if (_pen.path && e.altKey) {
      var rTol = 10 / view.zoom, rBestD = rTol, rBestSeg = null;
      _pen.path.segments.forEach(function (s) {
        var d = pt.getDistance(s.point);
        if (d < rBestD) { rBestD = d; rBestSeg = s; }
      });
      if (rBestSeg) {
        reshapingSeg = rBestSeg;
        draggingHandle = true;
        window.SMEngineBridge.suspend();
        window.SMEngineBridge.renderNow();
        return;
      }
    }

    var now = Date.now();
    var isDoubleClick = _pen.path && (now - _pen.lastClickTime < 350) && _pen.lastClickPt && pt.getDistance(_pen.lastClickPt) < 10 / view.zoom;
    _pen.lastClickTime = now;
    _pen.lastClickPt = pt.clone();

    if (isDoubleClick) {
      finalizePen();
      if (canvasEl) canvasEl.style.cursor = 'crosshair'; // undo any lingering hover-affordance cursor (pointer/grab) from just before finishing
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
      // Shift-constrain (2026-07, "8/9 marche pas" — same root cause as
      // shape-bridge.js: this bridge, not tools.js's Paper.js-View
      // onMouseDown, is the code path that actually places Pen anchors
      // whenever the Rust engine is enabled — the earlier tools.js fix
      // was dead code here too. Anchors are placed by discrete clicks,
      // not by dragging, so the constrain check belongs at click-time
      // (onDown), against the path's last placed anchor — not in onMove
      // like Rect/Ellipse/Line's continuous drag.
      if (e.shiftKey && _pen.path.segments.length) pt = constrainAngle45(_pen.path.lastSegment.point, pt);
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
      var seg = reshapingSeg || _pen.path.lastSegment;
      // Motion transform fix (feedback #135) — seg.point lives in raw
      // document space; the pointer must be mapped the same way before the
      // handle delta is computed against it.
      var pt = toLocalPoint(new Point(w[0], w[1]), state.activeLayerIdx);
      var delta = pt.subtract(seg.point);
      seg.handleOut = delta;
      // Alt/Option held: break the handle's symmetry (only the OUT side
      // moves, IN stays wherever it already was) — the standard Illustrator/
      // Figma way to end a smooth curve with a sharp corner right after it,
      // without switching tools. Same idiom subselect-bridge.js already
      // uses for editing handles after the fact; the Pen tool's own live
      // drag never had it.
      // reshapingSeg's whole gesture is held under Alt from mousedown (see
      // onDown), so this is always false there — correct: reshaping an
      // already-placed anchor only pulls its OUT tangent, leaving whatever
      // IN handle it already had untouched, rather than yanking a curve
      // that was already committed on the other side.
      if (!e.altKey) seg.handleIn = delta.multiply(-1);
    } else if (_pen.path && canvasEl) {
      // Cursor affordances (feedback #38) — the same two hints every vector
      // app gives before you commit to a click: hovering back near the
      // start of an open, closeable path previews that clicking here closes
      // it (mirrors the actual hit-test in onDown, 10/view.zoom); Alt
      // hovering an existing anchor previews the reshape-drag added above.
      // Both are dynamic overrides of the tool's normal static cursor
      // (SM.setTool's cc['pen']='crosshair'), so anything that doesn't
      // match falls back to that same default rather than getting stuck.
      // Motion transform fix (feedback #135) — _pen.path.segments live in
      // raw document space; map the pointer the same way for these
      // proximity checks.
      var pt = toLocalPoint(new Point(w[0], w[1]), state.activeLayerIdx);
      var tol = 10 / view.zoom;
      var nearAnchor = false;
      if (e.altKey) {
        for (var ci = 0; ci < _pen.path.segments.length; ci++) {
          if (pt.getDistance(_pen.path.segments[ci].point) < tol) { nearAnchor = true; break; }
        }
      }
      var nearStart = !e.altKey && !_pen.path.closed && _pen.path.segments.length > 1 &&
        pt.getDistance(_pen.path.firstSegment.point) < tol;
      canvasEl.style.cursor = nearAnchor ? 'grab' : (nearStart ? 'pointer' : 'crosshair');
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
    reshapingSeg = null;
    window.SMEngineBridge.resume();
    window.SMEngineBridge.renderNow();
  }

  function init() {
    var target = document.getElementById('canvas-area') || document.getElementById('drawing-canvas');
    canvasEl = target;
    target.addEventListener('pointerdown', onDown, { capture: true });
    target.addEventListener('pointermove', onMove, { capture: true });
    target.addEventListener('pointerup', onUp, { capture: true });
    target.addEventListener('pointercancel', onUp, { capture: true });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
