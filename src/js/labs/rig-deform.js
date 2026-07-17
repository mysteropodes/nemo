// ---- LABS PROTOTYPE — Weighted control-point rig (Shapper-inspired) ----
// feature-scouting.md's bones entry (#6) recommends capturing rigging via
// the Rive MCP export rather than reimplementing a bone/skeleton system
// natively — that recommendation stands for a TRANSFORM-hierarchy rig
// (parent/child bone matrices, skinning). Shapper (the studio's own AE
// mask-influence tool — see wiki/shapper-docs) is a DIFFERENT model that
// fits a vector-native app naturally: no bone hierarchy at all — a
// handful of movable CONTROL POINTS, each shape VERTEX carries a 0..1
// INFLUENCE weight per control (auto-computed from proximity, editable),
// and moving a control offsets every vertex it influences by
// weight × (control's own delta from its rest position). That is pure
// per-vertex arithmetic on data Nemo already has (segment.point arrays)
// — no skinning matrices, no engine.rs, no new render pipeline.
//
// Ported concepts (see wiki/shapper-docs/docs/*):
//   - core-concepts/how-it-works.md → rest-position + weighted-offset model
//   - mask-influences/set-influences.md → proximity-based auto weights
//   - tools/ik-system.md → 3-point (root/joint/end) 2-bone IK solver
//   - tools/magnet-brush.md → radius + cosine-falloff direct point drag
//   - animation/autosway.md → sine-wave oscillation, here BAKED per frame
//     (Nemo has no runtime expression engine like AE, so autosway can't
//     stay a live formula — it's rendered to real keyframes instead, same
//     tradeoff boil-effect/follow-path already made for their own bakes)
//
// Deformation is LIVE on the Paper scene while dragging a control
// (rigMoveControl), exactly like Shapper's viewer — commitFrame() bakes
// the current deformed state into the active frame as a real keyframe
// (pushUndo first, one undo reverts the whole pose). Nothing is baked
// automatically; you can pose freely and only commit() when happy.
//
//   SMLabs.rigAddControl(x, y)                    → controlId
//   SMLabs.rigBindStroke(path|null, controlIds, radius)
//   SMLabs.rigMoveControl(id, x, y)                 — live, no undo yet
//   SMLabs.rigResetPose()                           — snap back to rest
//   SMLabs.rigCommitFrame()                         — bake + undo point
//   SMLabs.rigSetIK(rootId, jointId, endId, flip?)
//   SMLabs.rigDragIKEnd(endId, x, y)                — live, 2-bone solve
//   SMLabs.rigMagnetBrush(path, cx, cy, radius, dx, dy) — standalone tool,
//     bypasses the control/weight system entirely, own pushUndo per call
//   SMLabs.rigAutosway(controlId, {amplitude,frequency,phase,frameStart,frameEnd})
(function () {
  var controls = {}; // id -> {x,y,restX,restY}
  var binds = []; // [{ path, restPositions:[[x,y]...], weights:[{controlId:w}...] }]
  var ikChains = {}; // endId -> {root,joint,end,l1,l2,flip}
  var nextId = 1;

  function freshId(prefix) { return prefix + (nextId++); }

  window.SMLabs.rigAddControl = function (x, y) {
    var id = freshId('ctl');
    controls[id] = { x: x, y: y, restX: x, restY: y };
    return id;
  };
  window.SMLabs.rigListControls = function () { return JSON.parse(JSON.stringify(controls)); };

  window.SMLabs.rigBindStroke = function (path, controlIds, radius) {
    var target = path || (typeof selectedPaths !== 'undefined' && selectedPaths.length === 1 ? selectedPaths[0] : null);
    if (!target || !(target instanceof Path) || !target.segments.length) { console.warn('[labs] rigBindStroke: aucun trait cible'); return false; }
    radius = radius || 200;
    var rest = target.segments.map(function (s) { return [s.point.x, s.point.y]; });
    var weights = rest.map(function (pt) {
      var w = {};
      controlIds.forEach(function (cid) {
        var c = controls[cid];
        if (!c) return;
        var d = Math.hypot(pt[0] - c.x, pt[1] - c.y);
        var infl = Math.max(0, 1 - d / radius);
        if (infl > 0) w[cid] = infl;
      });
      return w;
    });
    if (!target.data) target.data = {};
    if (typeof ensureStrokeId === 'function') ensureStrokeId(target);
    binds.push({ path: target, rest: rest, weights: weights });
    return true;
  };
  window.SMLabs.rigWeightsFor = function (path) {
    var b = binds.filter(function (b) { return b.path === path; })[0];
    return b ? JSON.parse(JSON.stringify(b.weights)) : null;
  };
  // Manual override — same shape Shapper's Influence Box editing produces,
  // just called directly: set one vertex's weight for one control.
  window.SMLabs.rigSetWeight = function (path, vertexIdx, controlId, weight) {
    var b = binds.filter(function (b) { return b.path === path; })[0];
    if (!b || !b.weights[vertexIdx]) return false;
    b.weights[vertexIdx][controlId] = Math.max(0, Math.min(1, weight));
    applyDeform();
    return true;
  };

  function applyDeform() {
    binds.forEach(function (b) {
      var segs = b.path.segments;
      for (var i = 0; i < segs.length; i++) {
        var rest = b.rest[i], w = b.weights[i];
        var dx = 0, dy = 0;
        for (var cid in w) {
          var c = controls[cid];
          if (!c) continue;
          dx += (c.x - c.restX) * w[cid];
          dy += (c.y - c.restY) * w[cid];
        }
        segs[i].point = new Point(rest[0] + dx, rest[1] + dy);
      }
    });
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }

  window.SMLabs.rigMoveControl = function (id, x, y) {
    var c = controls[id];
    if (!c) { console.warn('[labs] contrôle inconnu:', id); return false; }
    c.x = x; c.y = y;
    applyDeform();
    return true;
  };
  window.SMLabs.rigResetPose = function () {
    Object.keys(controls).forEach(function (id) { controls[id].x = controls[id].restX; controls[id].y = controls[id].restY; });
    applyDeform();
  };

  window.SMLabs.rigCommitFrame = function () {
    if (!binds.length) { if (typeof showToast === 'function') showToast('Aucun trait riggé'); return false; }
    pushUndo();
    ensureKeyframe();
    binds.forEach(function () {}); // deform already live on the Paper items
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    if (typeof showToast === 'function') showToast('Pose du rig figée sur cette frame');
    return true;
  };

  // ---- IK (tools/ik-system.md — 3-point chain, 2-bone law-of-cosines) ----
  window.SMLabs.rigSetIK = function (rootId, jointId, endId, flip) {
    var r = controls[rootId], j = controls[jointId], e = controls[endId];
    if (!r || !j || !e) { console.warn('[labs] rigSetIK: contrôle(s) introuvable(s)'); return false; }
    var l1 = Math.hypot(j.restX - r.restX, j.restY - r.restY);
    var l2 = Math.hypot(e.restX - j.restX, e.restY - j.restY);
    ikChains[endId] = { root: rootId, joint: jointId, end: endId, l1: l1, l2: l2, flip: !!flip };
    return true;
  };
  // Standard 2-bone IK: given root fixed and a target for the end effector,
  // solve the joint angle via the law of cosines, clamping the target
  // distance to [|l1-l2|, l1+l2] so an out-of-reach target still gives a
  // fully-extended (not NaN) pose, exactly like every 2-bone solver does.
  window.SMLabs.rigDragIKEnd = function (endId, x, y) {
    var chain = ikChains[endId];
    if (!chain) { console.warn('[labs] pas de chaîne IK pour', endId); return false; }
    var root = controls[chain.root];
    var dx = x - root.x, dy = y - root.y;
    var dist = Math.hypot(dx, dy);
    var l1 = chain.l1, l2 = chain.l2;
    var maxD = l1 + l2, minD = Math.abs(l1 - l2);
    var clamped = Math.max(minD + 1e-6, Math.min(maxD - 1e-6, dist));
    var baseAngle = Math.atan2(dy, dx);
    // Angle at root between the root→end line and the root→joint bone.
    var cosA = (l1 * l1 + clamped * clamped - l2 * l2) / (2 * l1 * clamped);
    cosA = Math.max(-1, Math.min(1, cosA));
    var a = Math.acos(cosA);
    var jointAngle = baseAngle + (chain.flip ? -a : a);
    var jointX = root.x + l1 * Math.cos(jointAngle);
    var jointY = root.y + l1 * Math.sin(jointAngle);
    window.SMLabs.rigMoveControl(chain.joint, jointX, jointY);
    window.SMLabs.rigMoveControl(endId, x, y);
    return { joint: [jointX, jointY] };
  };

  // ---- Magnet Brush (tools/magnet-brush.md) — standalone, bypasses the
  // control/weight system entirely: direct radius-falloff point drag on
  // ANY path's own vertices. Own pushUndo per call (one gesture = one undo).
  window.SMLabs.rigMagnetBrush = function (path, cx, cy, radius, dx, dy) {
    var target = path || (typeof selectedPaths !== 'undefined' && selectedPaths.length === 1 ? selectedPaths[0] : null);
    if (!target || !(target instanceof Path)) { console.warn('[labs] rigMagnetBrush: aucun trait cible'); return 0; }
    pushUndo();
    ensureKeyframe();
    var moved = 0;
    target.segments.forEach(function (s) {
      var d = s.point.getDistance(new Point(cx, cy));
      if (d > radius) return;
      // Cosine falloff, matching the doc's own description: full movement
      // at center, minimal at the edge, smooth (not linear) in between.
      var falloff = (Math.cos(Math.PI * d / radius) + 1) / 2;
      s.point = s.point.add(new Point(dx * falloff, dy * falloff));
      moved++;
    });
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    return moved;
  };

  // ---- Autosway (animation/autosway.md) — BAKED, not a live AE expression
  // (Nemo has no runtime expression engine): generates real keyframes
  // across [frameStart,frameEnd], one per frame, moving the control along
  // a sine wave and re-deforming + saving each frame in turn. Frame 0 of
  // the range is the rest pose (sin(phase) at t=0 if phase=0 → offset 0).
  window.SMLabs.rigAutosway = function (controlId, opts) {
    opts = opts || {};
    var c = controls[controlId];
    if (!c) { console.warn('[labs] contrôle inconnu:', controlId); return 0; }
    var amp = opts.amplitude !== undefined ? opts.amplitude : 20;
    var freq = opts.frequency !== undefined ? opts.frequency : 1; // cycles per second
    var phase = opts.phase || 0;
    var axis = opts.axis === 'x' ? 'x' : 'y'; // swing direction
    var f0 = opts.frameStart !== undefined ? opts.frameStart : state.currentFrame;
    var f1 = opts.frameEnd !== undefined ? opts.frameEnd : state.totalFrames - 1;
    if (!binds.length) { if (typeof showToast === 'function') showToast('Aucun trait riggé à faire osciller'); return 0; }
    pushUndoLayers();
    saveAllLayerFrames();
    var cf = state.currentFrame;
    var n = 0;
    for (var f = f0; f <= f1; f++) {
      var t = f / Math.max(1, state.fps);
      var off = amp * Math.sin(2 * Math.PI * freq * t + phase);
      if (axis === 'x') { c.x = c.restX + off; c.y = c.restY; }
      else { c.y = c.restY + off; c.x = c.restX; }
      state.currentFrame = f; window._curFrame = f;
      loadFrame(f); // rebuild userLayers[*] children for this frame first
      // re-resolve bind targets to THIS frame's live items (loadFrame
      // rebuilds the layer's children — the old Path references are gone)
      rebindAfterFrameChange();
      applyDeform();
      if (!state.layers[state.activeLayerIdx].frames[f].isKeyframe) {
        state.layers[state.activeLayerIdx].frames[f].isKeyframe = true;
        state.layers[state.activeLayerIdx].frames[f].isInterpolated = false;
      }
      saveActiveLayerFrame();
      n++;
    }
    state.currentFrame = cf; window._curFrame = cf;
    loadFrame(cf); rebindAfterFrameChange();
    renderOS(); renderArcs(); updateUI();
    if (typeof renderTimeline === 'function') renderTimeline();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    if (typeof showToast === 'function') showToast('Autosway : ' + n + ' frame(s) bakées');
    return n;
  };
  // loadFrame() throws away and rebuilds every layer's Paper children —
  // a bind's `path` reference from before the frame change is now a
  // detached, orphaned Path. Re-resolve each bind to the live item at the
  // same layer/child-index (rigAutosway's own commits never reorder
  // children, so index-based lookup is safe here — a general-purpose
  // rebind would need strokeId matching instead).
  function rebindAfterFrameChange() {
    binds.forEach(function (b) {
      if (b.path.layer) return; // still attached, nothing to do
      var sid = b.path.data && b.path.data.strokeId;
      if (!sid) return;
      for (var li = 0; li < userLayers.length; li++) {
        var found = userLayers[li].children.filter(function (c) { return c.data && c.data.strokeId === sid; })[0];
        if (found) { b.path = found; return; }
      }
    });
  }

  window.SMLabs.register('rig-deform', {
    flag: 'nemo-labs-rig',
    describe: 'Rig à contrôles pondérés (inspiré de Shapper/AE — voir wiki/shapper-docs) : rigAddControl/rigBindStroke/rigMoveControl/rigCommitFrame, IK 2-os (rigSetIK/rigDragIKEnd), Magnet Brush, Autosway bakée',
  });
})();
