// ---- Rig tool (2026-07-29) — bone-path drawing + pose dragging ----
// Same capture-phase-interception architecture as pen-bridge.js, which this
// file mirrors closely for the DRAWING half (click=corner anchor,
// click-drag=smooth anchor with symmetric tangent handles, double-click or
// click-near-first-anchor finishes the bone). Two departures from pen-bridge:
//
// 1. Unified interaction — a single onDown hit-tests every EXISTING bone
//    anchor across the active layer's rig FIRST; a hit starts a POSE drag
//    (moves that anchor, live-deforms every bound shape via
//    app.js's applyRigDeform) instead of drawing. Only a miss falls through
//    to the Pen-style draw-a-new-bone logic below. This is what lets the
//    same tool serve both "build the skeleton" and "puppet it" without a
//    separate mode switch.
// 2. A finished bone is NEVER inserted into the real Paper layer — it would
//    otherwise become ordinary artwork geometry (visible, exported, hit-
//    tested by every other tool, saved by saveActiveLayerFrame — exactly
//    CLAUDE.md §1's family of bug). `_rigDraw.path` is a live Paper Path
//    used only for its own segment/handle math while being drawn (built
//    with {insert:false}, matching every other "insert:false by
//    convention" builder in this codebase); at finalize time its segments
//    are copied into ld.rig.bones[id] (plain JSON) and the Paper object is
//    discarded.
// Bare global (not inside the IIFE below), exactly mirroring tools.js's own
// `_pen` — engine-bridge.js's buildRigPreviewItems reads `_rigDraw.path`
// directly (via `typeof _rigDraw !== 'undefined'`) to render the in-progress
// bone WHILE it's being drawn, before finalizeRigBone() copies it into
// ld.rig.bones (plain JSON) and discards this live Paper Path.
var _rigDraw = { path: null, boneId: null, draggingHandle: false, lastClickTime: 0, lastClickPt: null };

(function () {
  var _posing = null; // {ld, boneId, vi} while dragging an existing bone anchor

  function shouldIntercept() {
    return (
      window.SMEngineBridge && window.SMEngineBridge.isEnabled() &&
      state.tool === 'rig' && !state.playing
    );
  }

  // Hit-tests every anchor of every bone on the active layer's rig. Returns
  // {boneId, vi} on a hit, tolerance in the same "N/view.zoom" screen-pixel
  // convention as every other handle hit-test in this codebase (motion.js's
  // hitPositionDot/hitAnchorPoint, tools.js's node handles).
  function hitBoneAnchor(pt) {
    var ld = state.layers[state.activeLayerIdx];
    if (!ld || !ld.rig) return null;
    var tol = 9 / view.zoom;
    var bones = ld.rig.bones, bestD = tol, best = null;
    Object.keys(bones).forEach(function (bid) {
      var segs = bones[bid].segments;
      for (var i = 0; i < segs.length; i++) {
        var d = pt.getDistance(new Point(segs[i].point[0], segs[i].point[1]));
        if (d < bestD) { bestD = d; best = { boneId: bid, vi: i }; }
      }
    });
    return best;
  }

  function onDown(e) {
    if (!shouldIntercept()) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var pt = new Point(w[0], w[1]);
    var ld = state.layers[state.activeLayerIdx];

    var anchorHit = hitBoneAnchor(pt);
    if (anchorHit) {
      _posing = { ld: ld, boneId: anchorHit.boneId, vi: anchorHit.vi };
      window.SMEngineBridge.suspend();
      return;
    }

    var now = Date.now();
    var isDoubleClick = _rigDraw.path && (now - _rigDraw.lastClickTime < 350) && _rigDraw.lastClickPt && pt.getDistance(_rigDraw.lastClickPt) < 10 / view.zoom;
    _rigDraw.lastClickTime = now;
    _rigDraw.lastClickPt = pt.clone();

    if (isDoubleClick) { finalizeRigBone(); window.SMEngineBridge.renderNow(); return; }

    if (!_rigDraw.path) {
      ensureLayerRig(ld);
      // Auto-continuation from an existing OPEN bone's endpoint (same trick
      // pen-bridge.js uses for ordinary paths) — lets a skeleton branch by
      // starting the next bone right at a previous one's tip.
      var tolExt = 10 / view.zoom;
      var bestD = tolExt, bestBoneId = null, bestEnd = null;
      Object.keys(ld.rig.bones).forEach(function (bid) {
        var bone = ld.rig.bones[bid];
        if (bone.closed) return;
        var segs = bone.segments;
        var first = new Point(segs[0].point[0], segs[0].point[1]);
        var last = new Point(segs[segs.length - 1].point[0], segs[segs.length - 1].point[1]);
        var df = pt.getDistance(first), dl = pt.getDistance(last);
        if (df < bestD) { bestD = df; bestBoneId = bid; bestEnd = 'first'; }
        if (dl < bestD) { bestD = dl; bestBoneId = bid; bestEnd = 'last'; }
      });
      if (bestBoneId) {
        _rigDraw.boneId = bestBoneId;
        _rigDraw.path = _boneSegsToPath(ld.rig.bones[bestBoneId].segments, false);
        if (bestEnd === 'first') _rigDraw.path.reverse();
      } else {
        _rigDraw.boneId = rigFreshId(ld.rig, 'bone');
        _rigDraw.path = new Path({ insert: false });
        _rigDraw.path.add(pt);
      }
    } else {
      var first2 = _rigDraw.path.firstSegment.point;
      var tol2 = 10 / view.zoom;
      if (_rigDraw.path.segments.length > 1 && pt.getDistance(first2) < tol2) {
        _rigDraw.path.closed = true;
        finalizeRigBone();
        window.SMEngineBridge.renderNow();
        return;
      }
      if (e.shiftKey && _rigDraw.path.segments.length) pt = constrainAngle45(_rigDraw.path.lastSegment.point, pt);
      _rigDraw.path.add(pt);
    }
    _rigDraw.draggingHandle = true;
    window.SMEngineBridge.suspend();
    window.SMEngineBridge.renderNow();
  }

  function onMove(e) {
    if (!shouldIntercept()) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    if (_posing) {
      var bone = _posing.ld.rig.bones[_posing.boneId];
      bone.segments[_posing.vi].point = [w[0], w[1]];
      applyRigDeform(_posing.ld);
      window.SMEngineBridge.renderNow();
      return;
    }
    if (window.SMEngineBridge.setRigPreview) window.SMEngineBridge.setRigPreview(w);
    if (_rigDraw.draggingHandle && _rigDraw.path) {
      var seg = _rigDraw.path.lastSegment;
      var pt = new Point(w[0], w[1]);
      var delta = pt.subtract(seg.point);
      seg.handleOut = delta;
      if (!e.altKey) seg.handleIn = delta.multiply(-1);
    }
    window.SMEngineBridge.renderNow();
  }

  function onUp(e) {
    if (_posing) {
      e.stopImmediatePropagation(); e.preventDefault();
      _posing = null;
      window.SMEngineBridge.resume();
      window.SMEngineBridge.renderNow();
      return;
    }
    if (!_rigDraw.draggingHandle) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    _rigDraw.draggingHandle = false;
    window.SMEngineBridge.resume();
    window.SMEngineBridge.renderNow();
  }

  // Copies the in-progress Paper Path's segments into ld.rig.bones[id] as
  // plain JSON (point/handleIn/handleOut arrays, matching relinkRigBinds'/
  // applyRigDeform's own expected shape) and discards the Paper object —
  // a bone never becomes real layer.children artwork (see header comment).
  function finalizeRigBone() {
    if (window.SMEngineBridge && window.SMEngineBridge.setRigPreview) window.SMEngineBridge.setRigPreview(null);
    if (!_rigDraw.path) return;
    var ld = state.layers[state.activeLayerIdx];
    if (_rigDraw.path.segments.length < 2) { _rigDraw.path.remove(); _rigDraw.path = null; _rigDraw.boneId = null; return; }
    var segsOut = _rigDraw.path.segments.map(function (s) {
      return { point: [s.point.x, s.point.y], handleIn: [s.handleIn.x, s.handleIn.y], handleOut: [s.handleOut.x, s.handleOut.y] };
    });
    var closed = _rigDraw.path.closed;
    ld.rig.bones[_rigDraw.boneId] = { segments: segsOut, restSegments: JSON.parse(JSON.stringify(segsOut)), closed: closed };
    _rigDraw.path.remove();
    _rigDraw.path = null; _rigDraw.boneId = null; _rigDraw.draggingHandle = false;
    if (window.renderLayerList) renderLayerList();
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

  window.SMRig = {
    finalizeRigBone: finalizeRigBone,
    hitBoneAnchor: hitBoneAnchor,
  };
})();
