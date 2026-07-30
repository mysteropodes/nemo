// ---- Rig tool (2026-07-29) — 3-step: Tracer / Assigner / Déplacer ----
// Redesigned per Cyril's own spec after the first (2-mode) attempt was still
// unclear: "il faut faire plus simple: un bouton icon pour dessiner les
// bones, un bouton d'assignation qui assigne automatiquement les éléments du
// layer sélectionné aux bones comme dans Shapper par rapport à la proximité,
// et après un autre bouton pour déplacer les bones. 3 étapes distinctes. Et
// dans l'assignement automatique tu as les box d'influences comme dans
// Shapper sur les vecteurs que tu peux modifier si l'assignement automatique
// ne fonctionne pas." state.rigSubMode is 'draw' | 'assign' | 'move' — a
// click NEVER means two different things depending on hidden state; which of
// the three it means is a single explicit panel button (timeline.js), not a
// side-effect of what's under the cursor.
//
// Same capture-phase-interception architecture as pen-bridge.js for the
// DRAWING mode (click=corner anchor, click-drag=smooth anchor with symmetric
// tangent handles, double-click/Enter/click-near-first-anchor finishes the
// bone). A finished bone is NEVER inserted into the real Paper layer — it
// would otherwise become ordinary artwork geometry (visible, exported, hit-
// tested by every other tool, saved by saveActiveLayerFrame — exactly
// CLAUDE.md §1's family of bug). `_rigDraw.path` is a live Paper Path used
// only for its own segment/handle math while being drawn (built with
// {insert:false}, matching every other "insert:false by convention" builder
// in this codebase); at finalize time its segments are copied into
// ld.rig.bones[id] (plain JSON) and the Paper object is discarded.
//
// Bare global (not inside the IIFE below), exactly mirroring tools.js's own
// `_pen` — engine-bridge.js's buildRigPreviewItems reads `_rigDraw.path`
// directly (via `typeof _rigDraw !== 'undefined'`) to render the in-progress
// bone WHILE it's being drawn, before finalizeRigBone() copies it into
// ld.rig.bones (plain JSON) and discards this live Paper Path.
var _rigDraw = { path: null, boneId: null, ld: null, draggingHandle: false, lastClickTime: 0, lastClickPt: null };

(function () {
  var _posing = null; // {ld, boneId, vi} while dragging an existing bone anchor (Déplacer mode)
  var _radiusDrag = null; // {ld, boneId} while dragging an influence circle's edge (Assigner mode)

  function shouldIntercept() {
    return (
      window.SMEngineBridge && window.SMEngineBridge.isEnabled() &&
      state.tool === 'rig' && !state.playing
    );
  }

  // A layer's Motion Position/Rotation/Scale is, by design, applied ONLY at
  // render time (engine-bridge.js's buildSceneJson composes it into a
  // pathTransform matrix — see motion.js's computeMotionMat/layerMotionAt
  // header comment) and is NEVER baked into the Paper.js document's own
  // segments — same contract subselect-bridge.js's own toLocalPoint (2026-07-
  // 29 fix) and select-bridge.js's hitPt already honor. This file never did:
  // every bone anchor/segment/influence-circle-center lives in bone.segments'
  // RAW coordinates (ensureLayerRig, applyRigDeform), but onDown/onMove fed
  // them the pointer's RENDERED/world position unmapped. Invisible at
  // scale=1 (identity map, which is why every earlier live test in this
  // session passed), but on any Component instance with an active Motion
  // Scale/Rotation/Position — "un calque qui change avec le zoom" — a click
  // exactly on a visually-scaled anchor missed it, and posing snapped the
  // bone to whatever raw coordinate happened to be under the RENDERED
  // cursor position instead of the equivalent local one, visibly corrupting
  // the shape (confirmed live, QA-confirmed 2026-07-29: dragging at a
  // correctly-hit raw point on a 2x-scaled layer collapsed the shape's
  // bounds from 40x160 to ~141x131 instead of following the drag 1:1).
  function toLocalPoint(pt, layerIdx) {
    if (!window.SMMotion) return pt;
    var map = SMMotion.layerMotionPointMap ? SMMotion.layerMotionPointMap(layerIdx) : null;
    // 3D layers (2026-07-29 fix) — layerMotionPointMap only recognizes the
    // base 2D properties and returns null for a 3D-toggled layer even with
    // real rotationX/rotationY set; layerMotion3DPointMap is the dedicated
    // (perspective-correct, not affine) counterpart for that case — see its
    // own header comment in motion.js for the ray-plane-intersection math.
    if (!map && SMMotion.layerMotion3DPointMap) map = SMMotion.layerMotion3DPointMap(layerIdx);
    if (!map) return pt;
    var lp = map.inv(pt.x, pt.y);
    return new Point(lp[0], lp[1]);
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
  // Anchor OR tangent-handle hit-test for Déplacer mode (2026-07-30 fix,
  // "il manque pas mal de chose les tangents"): the handles ARE drawn for
  // every finished bone (engine-bridge.js's pushHandles, unconditional, one
  // small square per non-zero handleIn/handleOut) but hitBoneAnchor above
  // never tested them and onMove only ever wrote .point — a real, visible
  // false affordance: squares that look grabbable and aren't. Returns the
  // single CLOSEST target across anchors and handles of every bone (not
  // "anchor always wins ties") so a tight curve's handle, when it sits
  // closer to the cursor than the anchor itself, is the one that grabs.
  // `kind` is 'anchor'|'handleIn'|'handleOut' — onDown/onMove below use it
  // to know which field of segments[vi] to write.
  function hitBoneAnchorOrHandle(pt) {
    var ld = state.layers[state.activeLayerIdx];
    if (!ld || !ld.rig) return null;
    var tol = 9 / view.zoom;
    var bones = ld.rig.bones, bestD = tol, best = null;
    Object.keys(bones).forEach(function (bid) {
      var segs = bones[bid].segments;
      for (var i = 0; i < segs.length; i++) {
        var s = segs[i];
        var d = pt.getDistance(new Point(s.point[0], s.point[1]));
        if (d < bestD) { bestD = d; best = { boneId: bid, vi: i, kind: 'anchor' }; }
        ['handleIn', 'handleOut'].forEach(function (which) {
          var h = s[which];
          // A near-zero handle (straight segment, the common case for a
          // fresh bone) has no meaningful on-screen target — skip it rather
          // than let it silently steal anchor-hit priority right at the
          // anchor's own position.
          if (!h || (Math.abs(h[0]) < 0.5 && Math.abs(h[1]) < 0.5)) return;
          var hp = new Point(s.point[0] + h[0], s.point[1] + h[1]);
          var dh = pt.getDistance(hp);
          if (dh < bestD) { bestD = dh; best = { boneId: bid, vi: i, kind: which }; }
        });
      }
    });
    return best;
  }

  // IK chain auto-detection (2026-07-30, wiring rigSetIK/rigDragIKEnd,
  // app.js — a complete, correct 2-bone law-of-cosines solver that existed
  // with zero callers anywhere reachable from the UI: "il manque pas mal
  // de chose" included half the rig engine simply never being invoked).
  // No parentBoneId or explicit chain-authoring UI exists in this data
  // model — two bones read as a chain purely by ONE bone's near end
  // exactly coinciding with another bone's own endpoint, the topology a
  // plain branch-click (onDown's Tracer auto-continuation, above) already
  // produces for free when drawing a 2-segment limb tip-to-tip. Given the
  // bone+vi of the anchor about to be dragged, returns the chain if (and
  // only if) that anchor is one bone's own FAR end (index 0 or the last
  // index — the "wrist") and its NEAR end coincides with some OTHER bone's
  // endpoint (the "elbow"); that other bone's own opposite end is the
  // "shoulder", a fixed pivot the drag rotates everything around.
  function otherEnd(bone, idx) { return idx === 0 ? bone.segments.length - 1 : 0; }
  function findIKChain(ld, boneId, vi) {
    var rig = ld.rig;
    var endBone = rig.bones[boneId];
    if (!endBone || endBone.closed || endBone.segments.length < 2) return null;
    var lastIdx = endBone.segments.length - 1;
    if (vi !== 0 && vi !== lastIdx) return null; // an interior vertex has no "other end" to be a chain at all
    var nearIdx = otherEnd(endBone, vi);
    var nearPt = new Point(endBone.segments[nearIdx].point[0], endBone.segments[nearIdx].point[1]);
    var tol = 2; // exact-enough bone-drawing coincidence, not a screen hit-test radius
    var rootBoneId = null, rootJointIdx = null;
    Object.keys(rig.bones).forEach(function (bid) {
      if (bid === boneId || rootBoneId) return;
      var b = rig.bones[bid];
      if (b.closed || b.segments.length < 2) return;
      var bLast = b.segments.length - 1;
      [0, bLast].forEach(function (i) {
        if (rootBoneId) return;
        var p = new Point(b.segments[i].point[0], b.segments[i].point[1]);
        if (p.getDistance(nearPt) < tol) { rootBoneId = bid; rootJointIdx = i; }
      });
    });
    if (!rootBoneId) return null;
    return {
      root: { boneId: rootBoneId, vi: otherEnd(rig.bones[rootBoneId], rootJointIdx) },
      joint: { boneId: rootBoneId, vi: rootJointIdx },
      // Mirrored joint point on the END bone's own near end — rigDragIKEnd
      // (app.js) only ever writes chain.joint (the ROOT bone's side); this
      // codebase has no parent-child bone link to keep the two coincident
      // points together automatically, so onMove below copies the solved
      // joint position over to this one after every solve, by hand.
      endNear: { boneId: boneId, vi: nearIdx },
      end: { boneId: boneId, vi: vi },
    };
  }

  // A bone's influence-circle CENTER (Assigner mode) — the geometric center
  // of its own bounding box, a stable, easy-to-reason-about point that
  // doesn't depend on which end of the bone was drawn first.
  function boneCircleCenter(bone) {
    var bp = _boneSegsToPath(bone.segments, bone.closed);
    var c = bp.bounds.center;
    bp.remove();
    return new Point(c.x, c.y);
  }
  function boneRadiusOf(bone, defaultRadius) { return bone.radius || defaultRadius; }
  function panelDefaultRadius() { return parseFloat((document.getElementById('rig-weight-radius') || {}).value) || 200; }
  function panelRotate() { return !!(document.getElementById('rig-rotate-mode') || {}).checked; }

  // Influence-circle EDGE hit-test (Shapper-style, Assigner mode) — the
  // handle you grab to resize a bone's own falloff radius. Tolerance is
  // generous (10/zoom) since it's a thin ring, not a filled target.
  function hitRadiusHandle(pt) {
    var ld = state.layers[state.activeLayerIdx];
    if (!ld || !ld.rig) return null;
    var def = panelDefaultRadius();
    var tol = 10 / view.zoom, best = null, bestD = tol;
    Object.keys(ld.rig.bones).forEach(function (bid) {
      var bone = ld.rig.bones[bid];
      var c = boneCircleCenter(bone);
      var r = boneRadiusOf(bone, def);
      var d = Math.abs(pt.getDistance(c) - r);
      if (d < bestD) { bestD = d; best = { boneId: bid, center: c }; }
    });
    return best;
  }

  function onDown(e) {
    if (!shouldIntercept()) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var pt = toLocalPoint(new Point(w[0], w[1]), state.activeLayerIdx);
    var ld = state.layers[state.activeLayerIdx];
    var mode = state.rigSubMode || 'draw';

    // ---- Assigner: drag an influence circle's edge to resize it. Nothing
    // else is interactive here — assignment itself runs from the panel's
    // "Assigner automatiquement" button, not from a canvas click, so a miss
    // is a silent no-op rather than accidentally starting a new bone.
    if (mode === 'assign') {
      var rh = hitRadiusHandle(pt);
      if (rh && ld && ld.rig) {
        pushUndo();
        _radiusDrag = { ld: ld, boneId: rh.boneId, center: rh.center };
        window.SMEngineBridge.suspend();
      }
      return;
    }

    // ---- Déplacer: pose-drag an EXISTING bone anchor OR tangent handle. No
    // drawing, no binding — a click that misses every target here does
    // nothing, on purpose (mirrors Assigner's own "one gesture, one
    // meaning" rule). Handle dragging (2026-07-30) reuses this exact same
    // pose machinery (_posing, applyRigDeform on every move) rather than a
    // separate mode: a handle edit IS a pose, in the same sense an anchor
    // move is — applyRigDeform already drives rotation-aware skinning off
    // the LIVE tangent vs the REST tangent (getLocationAt on bp.cur vs
    // bp.rest), and a bone's tangent at any point depends on its handles
    // just as much as its anchor positions, so reshaping a curve responds
    // through the identical mechanism with zero new code in applyRigDeform.
    if (mode === 'move') {
      var anchorHit = hitBoneAnchorOrHandle(pt);
      if (anchorHit) {
        // Undo checkpoint (2026-07-29 fix, QA-confirmed: undo after a pose-
        // drag + Commit left the bone's OWN segments still posed while the
        // shape reverted to unposed — a visible mismatch between the rig
        // overlay and the actual geometry). This mode was the only one of
        // the three missing a pushUndo before its own mutation (Assigner's
        // radius-drag already has one a few lines up; Tracer's new-bone-
        // start already has one below). Placed here, at drag START, not in
        // rigCommitFrame (app.js) — by the time Commit runs, bone.segments
        // has ALREADY been live-mutated by this drag, so a checkpoint taken
        // there captures that already-posed rig paired with the frame's
        // still-unbaked (pre-Commit) strokes: an internally inconsistent
        // snapshot. _rigPoseUndoPushed (below) tells rigCommitFrame this
        // checkpoint already exists so it doesn't ALSO push its own bad one.
        pushUndo();
        // Alt+drag an ANCHOR (not a handle — that's already Alt's OTHER
        // meaning here, breaking a tangent) that's the far tip of a
        // 2-bone chain solves IK instead of a plain FK point-move: pulling
        // a "hand" bends the "elbow" automatically. A first-pass gesture
        // choice (Cyril hasn't specified how he wants IK invoked) — plain
        // drag keeps ordinary FK on every anchor exactly as before, so
        // this is purely additive.
        var ikChain = (anchorHit.kind === 'anchor' && e.altKey) ? findIKChain(ld, anchorHit.boneId, anchorHit.vi) : null;
        if (ikChain) {
          rigSetIK(ld, ikChain.root, ikChain.joint, ikChain.end, false);
        }
        _posing = { ld: ld, boneId: anchorHit.boneId, vi: anchorHit.vi, kind: anchorHit.kind, ik: ikChain };
        // Marks the pose as live-but-uncommitted for saveActiveLayerFrame/
        // saveAllLayerFrames (app.js) — cleared only by rigCommitFrame/
        // rigResetPose, deliberately NOT on pointerup here, since a pose can
        // sit live far longer than one drag gesture (see those functions'
        // own comment for why this matters: the periodic autosave timer).
        ld._rigPoseLive = true;
        ld._rigPoseUndoPushed = true;
        window.SMEngineBridge.suspend();
      }
      return;
    }

    // ---- Tracer: pure Pen-style bone drawing, no posing (Déplacer owns
    // that now) — a click on an existing anchor here just starts a new bone
    // from that point instead (still useful: branching a skeleton), never
    // grabs/moves it.
    var now = Date.now();
    var isDoubleClick = _rigDraw.path && (now - _rigDraw.lastClickTime < 350) && _rigDraw.lastClickPt && pt.getDistance(_rigDraw.lastClickPt) < 10 / view.zoom;
    _rigDraw.lastClickTime = now;
    _rigDraw.lastClickPt = pt.clone();

    if (isDoubleClick) { finalizeRigBone(); window.SMEngineBridge.renderNow(); return; }

    if (!_rigDraw.path) {
      // Undo checkpoint (2026-07-29 fix, QA-confirmed: no pushUndo existed
      // ANYWHERE in this file — a finished bone could never be undone at
      // all). Placed here, once per NEW bone, mirroring exactly where
      // pen-bridge.js's sibling tools.js onMouseDown pushes for a fresh Pen
      // path (`if(!_pen.path){pushUndo();...}`) — one undo removes the
      // whole bone, not one per anchor click.
      pushUndo();
      ensureLayerRig(ld);
      // Remembered so finalizeRigBone() targets the layer the bone was
      // actually STARTED on, not whichever layer happens to be active when
      // the bone is finished — see that function's own comment for the bug
      // this fixes (crash when the active layer changes mid-draw).
      _rigDraw.ld = ld;
      // Proximity to an existing OPEN bone's endpoint (same trick
      // pen-bridge.js uses for ordinary paths) — two DIFFERENT things
      // depending on Alt, both starting at that exact tip:
      //  - plain click (2026-07-30 fix, "il manque pas mal de chose"): a
      //    genuine BRANCH — a brand-new bone, its own id, coincidentally
      //    starting at the same point. This is what a skeleton actually
      //    needs (spine->arm->forearm is a tree, not one ever-longer path)
      //    and what rigDragIKEnd's chain-of-two-bones math (app.js) is
      //    built to walk — it needs two SEPARATE bones sharing a joint,
      //    not one 3-point bone, to have a joint to bend at all.
      //  - Alt+click: the OLD default — reopen and EXTEND the same bone
      //    (same id), for the rarer case of continuing a curve you
      //    finished a segment early on. The comment here used to claim
      //    "branch" while the code actually always did this — now true
      //    either way, spelled out instead of contradicting the code.
      var tolExt = 10 / view.zoom;
      var bestD = tolExt, bestBoneId = null, bestEnd = null, bestPt = null;
      Object.keys(ld.rig.bones).forEach(function (bid) {
        var bone = ld.rig.bones[bid];
        if (bone.closed) return;
        var segs = bone.segments;
        var first = new Point(segs[0].point[0], segs[0].point[1]);
        var last = new Point(segs[segs.length - 1].point[0], segs[segs.length - 1].point[1]);
        var df = pt.getDistance(first), dl = pt.getDistance(last);
        if (df < bestD) { bestD = df; bestBoneId = bid; bestEnd = 'first'; bestPt = first; }
        if (dl < bestD) { bestD = dl; bestBoneId = bid; bestEnd = 'last'; bestPt = last; }
      });
      if (bestBoneId && e.altKey) {
        _rigDraw.boneId = bestBoneId;
        _rigDraw.path = _boneSegsToPath(ld.rig.bones[bestBoneId].segments, false);
        if (bestEnd === 'first') _rigDraw.path.reverse();
      } else {
        _rigDraw.boneId = rigFreshId(ld.rig, 'bone');
        _rigDraw.path = new Path({ insert: false });
        // Snap exactly onto the existing bone's tip when branching, not the
        // raw (slightly-off) click point — a branch that's 1-2px shy of a
        // true coincidence would silently fail the tip-proximity chain
        // detection a future IK/parent-chain feature walks by exact-enough
        // distance, not by an explicit link.
        _rigDraw.path.add(bestBoneId ? bestPt : pt);
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
    // Local (raw document-space) point for anything that reads/writes
    // bone.segments or a raw-space center — see toLocalPoint's own comment.
    var localPt = toLocalPoint(new Point(w[0], w[1]), state.activeLayerIdx);
    if (_radiusDrag) {
      var bone = _radiusDrag.ld.rig.bones[_radiusDrag.boneId];
      bone.radius = Math.max(1, localPt.getDistance(_radiusDrag.center));
      window.SMEngineBridge.renderNow();
      return;
    }
    if (_posing) {
      if (_posing.ik) {
        var chain = _posing.ik;
        var endKey = chain.end.boneId + ':' + chain.end.vi;
        rigDragIKEnd(_posing.ld, endKey, localPt.x, localPt.y);
        // rigDragIKEnd only writes chain.joint (the ROOT bone's own end) —
        // copy that solved position onto the END bone's coincident near
        // end too (see findIKChain's own comment: no parent-child link
        // exists to do this automatically), or the two bones visibly pull
        // apart at the "elbow" the moment the chain bends.
        var jointS = _posing.ld.rig.bones[chain.joint.boneId].segments[chain.joint.vi];
        _posing.ld.rig.bones[chain.endNear.boneId].segments[chain.endNear.vi].point = jointS.point.slice();
        applyRigDeform(_posing.ld);
        window.SMEngineBridge.renderNow();
        return;
      }
      var poseBone = _posing.ld.rig.bones[_posing.boneId];
      var poseSeg = poseBone.segments[_posing.vi];
      if (_posing.kind === 'handleIn' || _posing.kind === 'handleOut') {
        // Handles are stored as vectors RELATIVE to the anchor (see
        // pushHandles, engine-bridge.js), not absolute points.
        var hVec = localPt.subtract(new Point(poseSeg.point[0], poseSeg.point[1]));
        poseSeg[_posing.kind] = [hVec.x, hVec.y];
        // Symmetric tangent by default (same Alt-to-break convention as
        // Tracer's own handle-drag a few lines up, and as Pen's — already
        // documented in the Rig status-bar help before this fix existed to
        // make it true): dragging one handle mirrors the OTHER to keep the
        // curve smooth through this anchor, unless Alt asks for a corner.
        var other = _posing.kind === 'handleOut' ? 'handleIn' : 'handleOut';
        if (!e.altKey) poseSeg[other] = [-hVec.x, -hVec.y];
      } else {
        poseSeg.point = [localPt.x, localPt.y];
      }
      applyRigDeform(_posing.ld);
      window.SMEngineBridge.renderNow();
      return;
    }
    // Only Tracer has an in-progress bone to rubber-band toward. Kept in
    // RENDERED/world space (w, not localPt) to match buildRigPreviewItems'
    // own forward-mapped bone segments (engine-bridge.js, 2026-07-29 fix) —
    // the rubber-band line's two endpoints must live in the SAME space.
    if ((state.rigSubMode || 'draw') === 'draw' && window.SMEngineBridge.setRigPreview) window.SMEngineBridge.setRigPreview(w);
    if (_rigDraw.draggingHandle && _rigDraw.path) {
      var seg = _rigDraw.path.lastSegment;
      var delta = localPt.subtract(seg.point);
      seg.handleOut = delta;
      if (!e.altKey) seg.handleIn = delta.multiply(-1);
    }
    window.SMEngineBridge.renderNow();
  }

  function onUp(e) {
    if (_radiusDrag) {
      e.stopImmediatePropagation(); e.preventDefault();
      // Re-assign on release (2026-07-29) — the whole point of a Shapper-
      // style influence circle is seeing the deformation follow the radius
      // you just set, not needing a second trip to the "Assigner
      // automatiquement" button to find out if it helped.
      if (window.rigAutoAssignLayer) {
        var li = state.layers.indexOf(_radiusDrag.ld);
        if (li >= 0) rigAutoAssignLayer(_radiusDrag.ld, userLayers[li], panelDefaultRadius(), panelRotate());
      }
      applyRigDeform(_radiusDrag.ld);
      _radiusDrag = null;
      window.SMEngineBridge.resume();
      window.SMEngineBridge.renderNow();
      return;
    }
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
    // Bug found live (2026-07-29 QA sweep): using state.activeLayerIdx here
    // crashed (`Cannot read properties of undefined (reading 'bones')`)
    // whenever the active layer changed (or a new project loaded) while a
    // bone was still mid-draw — this used to grab whatever layer/project is
    // CURRENT at finalize time, not the one the bone actually belongs to.
    // _rigDraw.ld (set in onDown) is the layer the bone was started on; if
    // that layer no longer exists (e.g. a brand-new project), fall back to
    // the current active layer rather than crash.
    var ld = _rigDraw.ld || state.layers[state.activeLayerIdx];
    if (_rigDraw.path.segments.length < 2) { _rigDraw.path.remove(); _rigDraw.path = null; _rigDraw.boneId = null; _rigDraw.ld = null; return; }
    if (!ld) { _rigDraw.path.remove(); _rigDraw.path = null; _rigDraw.boneId = null; _rigDraw.ld = null; return; }
    ensureLayerRig(ld);
    var segsOut = _rigDraw.path.segments.map(function (s) {
      return { point: [s.point.x, s.point.y], handleIn: [s.handleIn.x, s.handleIn.y], handleOut: [s.handleOut.x, s.handleOut.y] };
    });
    var closed = _rigDraw.path.closed;
    // Preserve a radius already set on this bone (branching/re-drawing an
    // existing open bone reuses its id, see the auto-continuation block
    // above) — only a genuinely NEW bone starts without one (falls back to
    // the panel default, boneRadiusOf/rigBindStroke).
    var prevRadius = ld.rig.bones[_rigDraw.boneId] && ld.rig.bones[_rigDraw.boneId].radius;
    ld.rig.bones[_rigDraw.boneId] = { segments: segsOut, restSegments: JSON.parse(JSON.stringify(segsOut)), closed: closed, radius: prevRadius };
    _rigDraw.path.remove();
    _rigDraw.path = null; _rigDraw.boneId = null; _rigDraw.ld = null; _rigDraw.draggingHandle = false;
    if (window.renderLayerList) renderLayerList();
    if (window.renderRigModeUI) renderRigModeUI();
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
    hitBoneAnchorOrHandle: hitBoneAnchorOrHandle,
    boneCircleCenter: boneCircleCenter,
    boneRadiusOf: boneRadiusOf,
  };
})();
