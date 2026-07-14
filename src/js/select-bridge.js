// ---- C7 CUTOVER STEP 3: Select tool fully on the Rust engine (when enabled) ----
// Same architecture as draw-bridge.js: capture-phase pointer interception
// (stopImmediatePropagation blocks Paper's own onMouseDown/Drag/Up for the
// same canvas), engine-bridge.js's tick() suspended for the whole gesture so
// it can't race the live overlay, and only Paper.js *data* (selectedPaths,
// state.selectedStrokeIndices, the actual path geometry) is mutated —
// rendering comes entirely from the Rust mirror, which already draws the
// transform box/marquee/selection from that same live data (see
// buildTransformBoxItems/buildMarqueeItems in engine-bridge.js), so no
// separate overlay-drawing code is needed here at all: just mutate state,
// then ask engine-bridge to re-render.
//
// This is a faithful line-for-line port of the 'select' branches of
// onMouseDown/onMouseDrag/onMouseUp in tools.js — see the research summary
// in the project plan for the full behavioral inventory this was checked
// against, plus the tween motion-arc handle drag (arcHandles/draggingArc —
// dragging a matched pair's arc-handoff control point), ported after the
// rest since it's a separate, smaller interaction. Click/shift-click toggle
// select, marquee rubber-band with intersection semantics, component-layer
// click-to-select-whole-instance + double-click-to-enter, the 8-handle+
// rotate transform box with opposite-corner anchoring, and group move are
// all ported too.
(function () {
  var mode = null; // 'xform-scale' | 'xform-rotate' | 'marquee' | 'move' | 'arc' | 'nv-drag' | null
  // Native-video footage gesture (experimental branch): drag moves the
  // footage, Shift+drag scales it uniformly (vertical motion), both writing
  // through SMMotion.setLayerValue — static override when the property's
  // stopwatch is off, auto-keyframe at the playhead when it's on, exactly
  // like typing in the Transform panel fields.
  var nvIdx = -1, nvStartPt = null, nvStartPos = null, nvStartScale = null, nvScaleMode = false, nvMoved = false;
  var xformDir = null, xformAnchor = null, xformOrigHandlePos = null, xformLastSx = 1, xformLastSy = 1;
  var xformMap = null; // geometry<->rendered-world mapper when the active layer has a Motion transform
  var rotCenter = null, rotStartAngle = 0, rotLastAngle = 0;
  var marqueeStart = null;
  var moveStarted = false;
  var draggingArc = null;
  var ANCHOR_MAP = { nw: 'se', ne: 'sw', sw: 'ne', se: 'nw', n: 's', s: 'n', e: 'w', w: 'e' };

  // v17: symGestureAccumulate (app.js) folds each move/scale/rotate tick on
  // a component's whole-instance selection into the layer's persistent
  // symMatrix — see that function's comment for why (Paper objects here are
  // rebuilt fresh from getEffectiveStrokes() on every loadFrame(), so a raw
  // segment mutation alone is discarded the moment playback or frame
  // navigation resolves a different internal frame).
  function shouldIntercept() {
    return window.SMEngineBridge && window.SMEngineBridge.isEnabled() && state.tool === 'select' && !state.playing;
  }

  // Same handle-position math as buildTransformBoxItems() in
  // engine-bridge.js and renderTransformHandles() in tools.js — recomputed
  // directly from xformSelBounds()/selectedPaths rather than reading
  // tools.js's own `xformHandles` array, since that array is only kept
  // current by tools.js's own (now-bypassed) onMouseDown/Drag — this way
  // hit-testing never depends on Paper's tool system having run at all.
  function computeHandles() {
    if (!selectedPaths.length) return null;
    // Oriented box (see tools.js orientedSelBox): after a rotation the
    // handles sit on the ROTATED box corners, not the axis-aligned union.
    var box = (typeof orientedSelBox === 'function') ? orientedSelBox() : null;
    if (!box) return null;
    var b = box.b;
    var zs = 1 / view.zoom;
    // Geometry-space corners first (selBoxPt handles the stroke's own
    // boxAngle), THEN the layer's Motion transform ("la box tourne pas
    // avec l'objet" when rotating the panel property) so handles sit on
    // the object where it actually RENDERS. gCorners stay in geometry
    // space for the gesture math (p.rotate/scale/translate mutate raw
    // Paper geometry, never rendered space).
    var map = (window.SMMotion && SMMotion.layerMotionPointMap) ? SMMotion.layerMotionPointMap(state.activeLayerIdx) : null;
    function GP(x, y) { return selBoxPt(x, y, box); }
    function WP(x, y) { var g = GP(x, y); if (!map) return g; var w = map.fwd(g.x, g.y); return new Point(w[0], w[1]); }
    var gCorners = {
      nw: GP(b.left, b.top), ne: GP(b.right, b.top),
      sw: GP(b.left, b.bottom), se: GP(b.right, b.bottom),
      n: GP(b.center.x, b.top), s: GP(b.center.x, b.bottom),
      e: GP(b.right, b.center.y), w: GP(b.left, b.center.y),
    };
    var corners = {
      nw: WP(b.left, b.top), ne: WP(b.right, b.top),
      sw: WP(b.left, b.bottom), se: WP(b.right, b.bottom),
      n: WP(b.center.x, b.top), s: WP(b.center.x, b.bottom),
      e: WP(b.right, b.center.y), w: WP(b.left, b.center.y),
    };
    // The rotate grip hangs off the box's OWN top edge (its up axis), so
    // it swings around with the object instead of hovering above the AABB.
    var rotPos = WP(b.center.x, b.top - 20 * zs);
    return { bounds: b, box: box, corners: corners, gCorners: gCorners, map: map, rotPos: rotPos };
  }

  function hitTestHandles(pt) {
    var h = computeHandles();
    if (!h) return null;
    var tol = 9 / view.zoom;
    var bestD = tol, best = null;
    var dRot = pt.getDistance(h.rotPos);
    if (dRot < bestD) { bestD = dRot; best = { type: 'rotate' }; }
    Object.keys(h.corners).forEach(function (k) {
      var d = pt.getDistance(h.corners[k]);
      if (d < bestD) { bestD = d; best = { type: 'scale', dir: k }; }
    });
    return best;
  }

  // Tween motion-arc handle hit-test: checked FIRST, before the transform
  // box, matching tools.js's own onMouseDown priority order — reads the
  // same `arcHandles` global renderArcs() (in tweens.js) populates, so it
  // stays correct without needing its own separate bookkeeping.
  function hitTestArc(pt) {
    if (typeof arcHandles === 'undefined') return null;
    var tol = 14 / view.zoom;
    for (var i = 0; i < arcHandles.length; i++) {
      if (pt.getDistance(arcHandles[i].handle.position) < tol) return arcHandles[i];
    }
    return null;
  }

  var lastPt = null;
  function onDown(e) {
    if (!shouldIntercept()) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var pt = new Point(w[0], w[1]);
    lastPt = pt;
    window.SMEngineBridge.suspend();

    var ah = hitTestArc(pt);
    if (ah) {
      mode = 'arc';
      draggingArc = ah;
      return;
    }

    var hh = hitTestHandles(pt);
    if (hh) {
      pushUndo();
      var h = computeHandles();
      if (hh.type === 'rotate') {
        mode = 'xform-rotate';
        // Rotation pivots around the redesign's 9-dot anchor widget
        // (tools.js xformAnchorPoint, state.xformAnchorKey) instead of
        // always the bounding-box center — defaults to center so existing
        // behavior is unchanged until the artist actually picks a corner.
        // xformAnchorPoint works in the box's de-rotated space (h.bounds)
        // — map the pivot back to world through the box angle.
        var apr = xformAnchorPoint(h.bounds);
        // Custom pivot (Alt+click) is already world — don't re-rotate it,
        // but DO pull it back to geometry space if a Motion transform is on.
        rotCenter = (h.box && !state.xformAnchorCustom) ? selBoxPt(apr.x, apr.y, h.box) : apr.clone();
        xformMap = h.map;
        if (state.xformAnchorCustom && h.map) { var rcg = h.map.inv(rotCenter.x, rotCenter.y); rotCenter = new Point(rcg[0], rcg[1]); }
        var ptg0 = xformMap ? (function () { var g = xformMap.inv(pt.x, pt.y); return new Point(g[0], g[1]); })() : pt;
        rotStartAngle = Math.atan2(ptg0.y - rotCenter.y, ptg0.x - rotCenter.x) * 180 / Math.PI;
        rotLastAngle = 0;
      } else {
        mode = 'xform-scale';
        xformDir = hh.dir;
        // Geometry-space anchor/handle (gesture math mutates raw geometry).
        xformAnchor = h.gCorners[ANCHOR_MAP[hh.dir]].clone();
        xformOrigHandlePos = h.gCorners[hh.dir].clone();
        xformMap = h.map;
        xformLastSx = 1; xformLastSy = 1;
      }
      window.SMEngineBridge.renderNow();
      return;
    }

    // Alt+click anywhere (not on a handle — already handled above) with an
    // active selection relocates the rotate/scale pivot to that exact point
    // — Illustrator/Figma "Option+click to move the reference point"
    // convention, reported as "avec alt et l'outil de sélection il faudrait
    // pouvoir changer le point d'ancrage de place". Doesn't touch geometry
    // (no pushUndo — this is a UI/pivot preference, not a document edit)
    // and doesn't fall through to select/move/marquee below: the click is
    // entirely consumed by placing the anchor, matching how the reference
    // apps behave (an Alt+click never ALSO reselects or starts a drag).
    if (e.altKey && selectedPaths.length) {
      state.xformAnchorCustom = [pt.x, pt.y];
      if (window.renderXformAnchorGrid) renderXformAnchorGrid();
      window.SMEngineBridge.resume();
      window.SMEngineBridge.renderNow();
      return;
    }

    var layer = userLayers[state.activeLayerIdx];
    var activeLdForLock = state.layers[state.activeLayerIdx];
    // A locked ACTIVE layer's own content must be as untouchable as a locked
    // OTHER layer's already is (see the hitOtherLayerIdx loop right below,
    // which has always skipped ld2.locked) — this hit-test had no such gate,
    // so selecting/dragging/transforming a locked layer's strokes still
    // worked the whole time it happened to be the active one, which is
    // most of the time right after locking it from the layer panel. Forcing
    // a miss here just falls through to the other-layer/component search
    // below, exactly as if this layer had nothing at that point.
    // EXCEPT a component/symbol layer: convertLayerToComponent always sets
    // .locked=true by design (it blocks hand-editing the baked sub-strokes),
    // but the component must still be selectable/movable AS ONE RIGID WHOLE
    // — that's the separate activeLd.symbolId branch a bit further down,
    // which only runs when `hit` comes back truthy. Nulling hit here for
    // every locked layer indiscriminately silently broke selecting a
    // component the instant it was also the active layer (the normal case
    // right after creating one, or an imported video, or whenever the layer
    // panel has it selected).
    var hit = (activeLdForLock.locked && !activeLdForLock.symbolId) ? null : layer.hitTest(pt, { stroke: true, fill: true, tolerance: 8 / view.zoom });
    var hitOtherLayerIdx = -1;
    // If nothing on the active layer, check every OTHER normal (non-
    // component) layer too — clicking a stroke that lives on layer 1 while
    // layer 2 is active must switch to layer 1, same courtesy the
    // component-layer branch right below already gives symbol layers.
    // Topmost-drawn first (project.layers render back-to-front).
    if (!hit) {
      for (var pli = project.layers.length - 1; pli >= 0; pli--) {
        var pl = project.layers[pli];
        var oli = userLayers.indexOf(pl);
        if (oli < 0 || oli === state.activeLayerIdx) continue;
        var ld2 = state.layers[oli];
        if (!ld2 || ld2.locked || !ld2.visible || ld2.symbolId) continue;
        var oh = pl.hitTest(pt, { stroke: true, fill: true, tolerance: 8 / view.zoom });
        if (oh) { hit = oh; hitOtherLayerIdx = oli; break; }
      }
    }

    if (!hit) {
      // Native video footage: click inside a visible video layer's display
      // rect (topmost first) starts a footage transform gesture. Runs only
      // when no stroke was hit — drawings sit ON TOP of footage, so a
      // stroke click must keep selecting the stroke.
      var nvHit = -1;
      if (window.SMNativeVideo && window.SMMotion) {
        for (var nvi = state.layers.length - 1; nvi >= 0; nvi--) {
          var nld = state.layers[nvi];
          if (!nld || !nld.nativeVideo || !nld.visible || nld.locked) continue;
          var nvr = SMNativeVideo.displayRect(nvi);
          if (nvr && pt.x >= nvr.x && pt.x <= nvr.x + nvr.width && pt.y >= nvr.y && pt.y <= nvr.y + nvr.height) { nvHit = nvi; break; }
        }
      }
      if (nvHit >= 0) {
        if (!e.shiftKey) clearSel();
        state.activeLayerIdx = nvHit;
        activateUL(nvHit);
        mode = 'nv-drag';
        nvIdx = nvHit;
        nvStartPt = pt.clone();
        nvScaleMode = !!e.shiftKey;
        nvStartPos = SMMotion.getLayerValue(nvHit, 'position');
        nvStartScale = SMMotion.getLayerValue(nvHit, 'scale');
        nvMoved = false;
        renderArcs(); updateUI();
        window.SMEngineBridge.renderNow();
        return;
      }
      var compHit = hitTestComponentLayers(pt);
      if (compHit) {
        var now2 = Date.now();
        var isDbl = _compClick.layerIdx === compHit.layerIdx && (now2 - _compClick.time < 350);
        _compClick.layerIdx = compHit.layerIdx; _compClick.time = now2;
        if (!e.shiftKey) clearSel();
        state.activeLayerIdx = compHit.layerIdx;
        activateUL(compHit.layerIdx);
        selectedPaths = userLayers[compHit.layerIdx].children.filter(function (c) { return (c instanceof Path || c instanceof Raster) && !(c.data && (c.data.isLinkedFillCompanion || c.data.isBrushTextureCopy)); });
        state.selectedStrokeIndices = [];
        renderArcs(); updateUI();
        window.SMEngineBridge.renderNow();
        if (isDbl) window.SM.enterSymbol(state.layers[compHit.layerIdx].symbolId);
        mode = selectedPaths.length ? 'move' : null;
        moveStarted = false;
        return;
      }
    }

    if (hit && (hit.item instanceof Path || hit.item instanceof Raster)) {
      if (hitOtherLayerIdx >= 0) {
        state.activeLayerIdx = hitOtherLayerIdx;
        activateUL(hitOtherLayerIdx);
      }
      // A component layer must act as one rigid transform group even when
      // it's the ACTIVE layer — the hitTestComponentLayers fallback below
      // only fires when the active layer's OWN hitTest misses, so clicking
      // a component's content while that layer already happens to be
      // active (the common case right after creating one, or whenever it's
      // simply selected in the layer list) fell through to this plain
      // single-path branch instead, selecting just the one clicked child.
      var activeLd = state.layers[state.activeLayerIdx];
      if (activeLd && activeLd.symbolId) {
        var now3 = Date.now();
        var isDbl2 = _compClick.layerIdx === state.activeLayerIdx && (now3 - _compClick.time < 350);
        _compClick.layerIdx = state.activeLayerIdx; _compClick.time = now3;
        if (!e.shiftKey) clearSel();
        selectedPaths = userLayers[state.activeLayerIdx].children.filter(function (c) { return (c instanceof Path || c instanceof Raster) && !(c.data && (c.data.isLinkedFillCompanion || c.data.isBrushTextureCopy)); });
        state.selectedStrokeIndices = [];
        mode = selectedPaths.length ? 'move' : null;
        moveStarted = false;
        renderArcs(); updateUI();
        window.SMEngineBridge.renderNow();
        if (isDbl2) window.SM.enterSymbol(activeLd.symbolId);
        return;
      }
      var p = hit.item;
      var idx2 = selectedPaths.indexOf(p);
      if (e.shiftKey) {
        if (idx2 >= 0) selectedPaths.splice(idx2, 1); else selectedPaths.push(p);
      } else if (idx2 < 0) {
        // Clicking a NEW item without shift replaces the selection — but
        // clicking one already part of a multi-selection must NOT clear the
        // rest of it first, or dragging the group by its body collapses the
        // selection down to just the clicked item before the move-drag
        // even starts (only that one element then moves) — matches the
        // reported "transform works but moving several selected elements
        // doesn't".
        clearSel();
        selectedPaths.push(p);
      }
      state.selectedStrokeIndices = selectedPaths.map(getSI).filter(function (i2) { return i2 >= 0; });
      mode = selectedPaths.length ? 'move' : null;
      moveStarted = false;
    } else {
      if (!e.shiftKey) clearSel();
      mode = 'marquee';
      marqueeStart = pt.clone();
      var prevA = project.activeLayer;
      marqueeLayer.activate();
      if (_marquee.rect) _marquee.rect.remove();
      // Lasso (v19) : Alt+drag sur le vide = selection a main levee (le
      // standard TVPaint/Photoshop), sinon marquee rectangulaire classique.
      _marquee.lasso = !!e.altKey;
      if (_marquee.lasso) {
        _marquee.rect = new Path({ segments: [pt], closed: false });
      } else {
        _marquee.rect = new Path.Rectangle({ from: pt, to: pt });
      }
      _marquee.active = true; _marquee.start = pt.clone();
      prevA.activate();
    }
    renderArcs(); updateUI();
    window.SMEngineBridge.renderNow();
  }

  function onMove(e) {
    if (!mode) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var pt = new Point(w[0], w[1]);

    if (mode === 'marquee') {
      var prevA = project.activeLayer;
      marqueeLayer.activate();
      if (_marquee.lasso) {
        _marquee.rect.add(pt);
      } else {
        var mx1 = Math.min(marqueeStart.x, pt.x), my1 = Math.min(marqueeStart.y, pt.y);
        var mx2 = Math.max(marqueeStart.x, pt.x), my2 = Math.max(marqueeStart.y, pt.y);
        if (_marquee.rect) _marquee.rect.remove();
        _marquee.rect = new Path.Rectangle({ from: new Point(mx1, my1), to: new Point(mx2, my2) });
      }
      prevA.activate();
    } else if (mode === 'nv-drag') {
      if (!nvMoved) { pushUndo(); nvMoved = true; }
      var nvd = pt.subtract(nvStartPt);
      if (nvScaleMode) {
        // Vertical drag scales uniformly around the current pivot: up =
        // bigger. 200 world units per doubling feels right at canvas scale.
        var nvf = Math.max(0.05, 1 - nvd.y / 200);
        SMMotion.setLayerValue(nvIdx, 'scale', [nvStartScale[0] * nvf, nvStartScale[1] * nvf]);
      } else {
        SMMotion.setLayerValue(nvIdx, 'position', [nvStartPos[0] + nvd.x, nvStartPos[1] + nvd.y]);
      }
      window._sceneVersion++;
      window.SMEngineBridge.renderNow();
    } else if (mode === 'move') {
      if (!moveStarted) { pushUndo(); moveStarted = true; }
      var delta = pt.subtract(lastPt);
      // Layer under a Motion transform: the pointer moves in RENDERED
      // space, the geometry lives underneath — pull the delta back
      // (inverse rotate + inverse scale) or the drag drifts/overshoots.
      var mvMap = (window.SMMotion && SMMotion.layerMotionPointMap) ? SMMotion.layerMotionPointMap(state.activeLayerIdx) : null;
      if (mvMap) { var dv = mvMap.invVec(delta.x, delta.y); delta = new Point(dv[0], dv[1]); }
      // translate(delta), not position=position.add(delta) — .position is
      // a bounds-CENTER getter/setter, so a move via .position re-derives
      // bounds on every single tick of the drag (many times per gesture)
      // and writes back a translation computed from that possibly-slightly-
      // imprecise read. The stroke ribbon and its linkedFill backdrop are
      // two DIFFERENT Paper.js objects with different segment counts/
      // geometry, so their bounds-rounding drifts at a different rate each
      // — invisible on one tick, but compounding over a real drag into a
      // visible "parallax" where fill and stroke slowly slide apart,
      // worse the more selected objects/ticks involved. translate() is a
      // direct matrix/segment shift with no bounds round-trip at all, so
      // N ticks of translate(delta) is always bit-identical to one
      // translate(delta*N) — zero accumulated drift by construction.
      selectedPaths.forEach(function (p) {
        p.translate(delta);
        if (p.data && p.data.isVectorBrush && p.data.centerSegments) {
          p.data.centerSegments.forEach(function (s) { s.point = [s.point[0] + delta.x, s.point[1] + delta.y]; });
        }
        if (p.data && p.data.linkedFill && !p.data.linkedFill.removed) p.data.linkedFill.translate(delta);
        if (p.data && p.data.brushCompanions) {
          p.data.brushCompanions.forEach(function (c) { if (!c.removed) c.translate(delta); });
        }
      });
      symGestureAccumulate(new Matrix().translate(delta));
    } else if (mode === 'xform-scale') {
      // Geometry-space pointer (NOT reassigning pt — lastPt at the end of
      // this handler must stay world-space).
      var ptS = pt;
      if (xformMap) { var ptgS = xformMap.inv(pt.x, pt.y); ptS = new Point(ptgS[0], ptgS[1]); }
      var anchor = xformAnchor, dir = xformDir, sx = 1, sy = 1;
      if (dir === 'nw' || dir === 'ne' || dir === 'sw' || dir === 'se') {
        var origDX = xformOrigHandlePos.x - anchor.x, origDY = xformOrigHandlePos.y - anchor.y;
        var curDX = ptS.x - anchor.x, curDY = ptS.y - anchor.y;
        sx = origDX !== 0 ? curDX / origDX : 1;
        sy = origDY !== 0 ? curDY / origDY : 1;
      } else if (dir === 'n' || dir === 's') {
        var origDY2 = xformOrigHandlePos.y - anchor.y, curDY2 = ptS.y - anchor.y;
        sy = origDY2 !== 0 ? curDY2 / origDY2 : 1;
      } else {
        var origDX2 = xformOrigHandlePos.x - anchor.x, curDX2 = ptS.x - anchor.x;
        sx = origDX2 !== 0 ? curDX2 / origDX2 : 1;
      }
      if (Math.abs(sx) < 0.05) sx = sx < 0 ? -0.05 : 0.05;
      if (Math.abs(sy) < 0.05) sy = sy < 0 ? -0.05 : 0.05;
      var stepSx = sx / xformLastSx, stepSy = sy / xformLastSy;
      selectedPaths.forEach(function (p) {
        p.scale(stepSx, stepSy, anchor);
        if (p.data && p.data.isVectorBrush && p.data.centerSegments) {
          scaleCenterSegments(p.data.centerSegments, stepSx, stepSy, anchor.x, anchor.y);
          rebuildVectorBrushOutline(p);
        }
      });
      xformLastSx = sx; xformLastSy = sy;
      symGestureAccumulate(new Matrix().scale(stepSx, stepSy, anchor));
    } else if (mode === 'arc') {
      setArcCtrl(draggingArc.fA, draggingArc.fB, draggingArc.matchIdx, draggingArc.ptA, draggingArc.ptB, pt.x, pt.y);
      renderArcs();
    } else if (mode === 'xform-rotate') {
      var ptR = pt;
      if (xformMap) { var ptgR = xformMap.inv(pt.x, pt.y); ptR = new Point(ptgR[0], ptgR[1]); }
      var curAngle = Math.atan2(ptR.y - rotCenter.y, ptR.x - rotCenter.x) * 180 / Math.PI;
      var deltaFromStart = curAngle - rotStartAngle;
      var stepAngle = deltaFromStart - rotLastAngle;
      selectedPaths.forEach(function (p) {
        p.rotate(stepAngle, rotCenter);
        if (p.data && p.data.isVectorBrush && p.data.centerSegments) {
          rotateCenterSegments(p.data.centerSegments, stepAngle, rotCenter.x, rotCenter.y);
          rebuildVectorBrushOutline(p);
        }
      });
      rotLastAngle = deltaFromStart;
      selectedPaths.forEach(function (p) { if (p) p.data.boxAngle = (((p.data && p.data.boxAngle) || 0) + stepAngle) % 360; }); // orientation lives on the stroke
      symGestureAccumulate(new Matrix().rotate(stepAngle, rotCenter));
    }
    lastPt = pt;
    window.SMEngineBridge.renderNow();
  }

  function onUp(e) {
    if (!mode) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    if (mode === 'nv-drag') {
      mode = null; nvIdx = -1; nvStartPt = null;
      // One panel/timeline refresh at gesture end (not per tick — the
      // Transform fields and Motion rows re-read motionStatic/keys).
      updateUI();
      window.SMEngineBridge.renderNow();
      return;
    }
    if (mode === 'arc') {
      draggingArc = null;
      generateTweens();
    } else if (mode === 'xform-scale' || mode === 'xform-rotate') {
      var xLd = state.layers[state.activeLayerIdx];
      if (xLd && xLd.symbolId) {
        // The persistent symMatrix is already updated (symGestureAccumulate
        // ran every tick) — rebuild the component's Paper objects fresh
        // from it instead of leaving the directly-mutated-in-place ones,
        // which are about to go stale the instant anything re-resolves
        // this layer's content (frame nav, playback, another gesture).
        // loadFrame() creates brand-new Path objects, so selectedPaths'
        // references to the old (now-removed) ones must be re-pointed.
        loadFrame(state.currentFrame);
        selectedPaths = userLayers[state.activeLayerIdx].children.filter(function (c) { return (c instanceof Path || c instanceof Raster) && !(c.data && (c.data.isLinkedFillCompanion || c.data.isBrushTextureCopy)); });
      } else {
        // Team review: a handle drag always means a real transform happened
        // (unlike 'move', which can fire on a plain click) — fork every
        // foreign-owned item in the gesture before it's persisted.
        selectedPaths.forEach(function (p) { forkIfForeignOwner(p); });
        fillRegenerateLinked(userLayers[state.activeLayerIdx], null);
        saveActiveLayerFrame();
      }
      renderArcs(); updateUI();
    } else if (mode === 'marquee') {
      if (_marquee.rect) {
        var mb = _marquee.rect.bounds;
        var lassoPath = null;
        if (_marquee.lasso && _marquee.rect.segments.length > 2) {
          _marquee.rect.closePath();
          lassoPath = _marquee.rect;
        }
        var layer2 = userLayers[state.activeLayerIdx];
        var activeLdForMarqueeLock = state.layers[state.activeLayerIdx];
        // Same lock gate as the click-select hit-test above (and same
        // component exception — a component's own .locked=true must not
        // block marquee-selecting it as a whole either).
        ((activeLdForMarqueeLock.locked && !activeLdForMarqueeLock.symbolId) ? [] : layer2.children).forEach(function (c) {
          // A linkedFill backdrop (c.data.isLinkedFillCompanion) is never
          // its own selectable thing — it always moves as part of its
          // parent ribbon's own selectedPaths entry (see that flag's own
          // comment in draw-bridge.js for the double-translate bug this
          // exclusion fixes). Marquee bounds-intersection would otherwise
          // pick it up as a second, independent hit whenever the box
          // covered both.
          if (((c instanceof Path && c.segments.length > 0 && (c.strokeColor || c.fillColor)) || c instanceof Raster) && mb.intersects(c.bounds) && !(c.data && (c.data.isLinkedFillCompanion || c.data.isBrushTextureCopy))) {
            // Lasso : le test bounds ne suffit pas (le lasso peut serpenter) —
            // l'item doit avoir son centre DANS le trace, ou le croiser.
            if (lassoPath && !(lassoPath.contains(c.position) || (c instanceof Path && lassoPath.intersects(c)))) return;
            if (selectedPaths.indexOf(c) < 0) selectedPaths.push(c);
          }
        });
        state.selectedStrokeIndices = selectedPaths.map(getSI).filter(function (i2) { return i2 >= 0; });
        _marquee.rect.remove(); _marquee.rect = null;
      }
      _marquee.active = false;
      renderArcs(); updateUI();
    } else if (mode === 'move') {
      var didMove = moveStarted;
      moveStarted = false;
      var mLd = state.layers[state.activeLayerIdx];
      if (mLd && mLd.symbolId) {
        loadFrame(state.currentFrame);
        selectedPaths = userLayers[state.activeLayerIdx].children.filter(function (c) { return (c instanceof Path || c instanceof Raster) && !(c.data && (c.data.isLinkedFillCompanion || c.data.isBrushTextureCopy)); });
        state.selectedStrokeIndices = [];
      } else {
        // Same fork-on-real-edit guard as xform above, gated on whether a
        // real drag distance was ever seen (moveStarted flips true lazily
        // in onMove — see its own comment) so a plain click-release on
        // someone else's stroke doesn't spawn a spurious identical-geometry
        // ghost.
        if (didMove) selectedPaths.forEach(function (p) { forkIfForeignOwner(p); });
        fillRegenerateLinked(userLayers[state.activeLayerIdx], null);
        saveActiveLayerFrame();
      }
    }
    mode = null;
    window.SMEngineBridge.renderNow();
    window.SMEngineBridge.resume();
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
