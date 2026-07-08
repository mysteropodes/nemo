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
  var mode = null; // 'xform-scale' | 'xform-rotate' | 'marquee' | 'move' | 'arc' | null
  var xformDir = null, xformAnchor = null, xformOrigHandlePos = null, xformLastSx = 1, xformLastSy = 1;
  var rotCenter = null, rotStartAngle = 0, rotLastAngle = 0;
  var marqueeStart = null;
  var moveStarted = false;
  var draggingArc = null;
  var ANCHOR_MAP = { nw: 'se', ne: 'sw', sw: 'ne', se: 'nw', n: 's', s: 'n', e: 'w', w: 'e' };

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
    var b = xformSelBounds();
    if (!b) return null;
    var zs = 1 / view.zoom;
    var corners = {
      nw: b.topLeft, ne: b.topRight, sw: b.bottomLeft, se: b.bottomRight,
      n: b.topCenter, s: b.bottomCenter, e: b.rightCenter, w: b.leftCenter,
    };
    var rotPos = b.topCenter.subtract(new Point(0, 20 * zs));
    return { bounds: b, corners: corners, rotPos: rotPos };
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
        rotCenter = h.bounds.center.clone();
        rotStartAngle = Math.atan2(pt.y - rotCenter.y, pt.x - rotCenter.x) * 180 / Math.PI;
        rotLastAngle = 0;
      } else {
        mode = 'xform-scale';
        xformDir = hh.dir;
        xformAnchor = h.corners[ANCHOR_MAP[hh.dir]].clone();
        xformOrigHandlePos = h.corners[hh.dir].clone();
        xformLastSx = 1; xformLastSy = 1;
      }
      window.SMEngineBridge.renderNow();
      return;
    }

    var layer = userLayers[state.activeLayerIdx];
    var hit = layer.hitTest(pt, { stroke: true, fill: true, tolerance: 8 / view.zoom });

    if (!hit) {
      var compHit = hitTestComponentLayers(pt);
      if (compHit) {
        var now2 = Date.now();
        var isDbl = _compClick.layerIdx === compHit.layerIdx && (now2 - _compClick.time < 350);
        _compClick.layerIdx = compHit.layerIdx; _compClick.time = now2;
        if (!e.shiftKey) clearSel();
        state.activeLayerIdx = compHit.layerIdx;
        activateUL(compHit.layerIdx);
        selectedPaths = userLayers[compHit.layerIdx].children.filter(function (c) { return c instanceof Path; });
        state.selectedStrokeIndices = [];
        renderArcs(); updateUI();
        window.SMEngineBridge.renderNow();
        if (isDbl) window.SM.enterSymbol(state.layers[compHit.layerIdx].symbolId);
        mode = selectedPaths.length ? 'move' : null;
        moveStarted = false;
        return;
      }
    }

    if (hit && hit.item instanceof Path) {
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
      _marquee.rect = new Path.Rectangle({ from: pt, to: pt });
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
      var mx1 = Math.min(marqueeStart.x, pt.x), my1 = Math.min(marqueeStart.y, pt.y);
      var mx2 = Math.max(marqueeStart.x, pt.x), my2 = Math.max(marqueeStart.y, pt.y);
      var prevA = project.activeLayer;
      marqueeLayer.activate();
      if (_marquee.rect) _marquee.rect.remove();
      _marquee.rect = new Path.Rectangle({ from: new Point(mx1, my1), to: new Point(mx2, my2) });
      prevA.activate();
    } else if (mode === 'move') {
      if (!moveStarted) { pushUndo(); moveStarted = true; }
      var delta = pt.subtract(lastPt);
      selectedPaths.forEach(function (p) {
        p.position = p.position.add(delta);
        if (p.data && p.data.isVectorBrush && p.data.centerSegments) {
          p.data.centerSegments.forEach(function (s) { s.point = [s.point[0] + delta.x, s.point[1] + delta.y]; });
        }
        if (p.data && p.data.linkedFill && !p.data.linkedFill.removed) p.data.linkedFill.position = p.data.linkedFill.position.add(delta);
      });
    } else if (mode === 'xform-scale') {
      var anchor = xformAnchor, dir = xformDir, sx = 1, sy = 1;
      if (dir === 'nw' || dir === 'ne' || dir === 'sw' || dir === 'se') {
        var origDX = xformOrigHandlePos.x - anchor.x, origDY = xformOrigHandlePos.y - anchor.y;
        var curDX = pt.x - anchor.x, curDY = pt.y - anchor.y;
        sx = origDX !== 0 ? curDX / origDX : 1;
        sy = origDY !== 0 ? curDY / origDY : 1;
      } else if (dir === 'n' || dir === 's') {
        var origDY2 = xformOrigHandlePos.y - anchor.y, curDY2 = pt.y - anchor.y;
        sy = origDY2 !== 0 ? curDY2 / origDY2 : 1;
      } else {
        var origDX2 = xformOrigHandlePos.x - anchor.x, curDX2 = pt.x - anchor.x;
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
    } else if (mode === 'arc') {
      setArcCtrl(draggingArc.fA, draggingArc.fB, draggingArc.matchIdx, draggingArc.ptA, draggingArc.ptB, pt.x, pt.y);
      renderArcs();
    } else if (mode === 'xform-rotate') {
      var curAngle = Math.atan2(pt.y - rotCenter.y, pt.x - rotCenter.x) * 180 / Math.PI;
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
    }
    lastPt = pt;
    window.SMEngineBridge.renderNow();
  }

  function onUp(e) {
    if (!mode) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    if (mode === 'arc') {
      draggingArc = null;
      generateTweens();
    } else if (mode === 'xform-scale' || mode === 'xform-rotate') {
      fillRegenerateLinked(userLayers[state.activeLayerIdx], null);
      saveActiveLayerFrame();
      renderArcs(); updateUI();
    } else if (mode === 'marquee') {
      if (_marquee.rect) {
        var mb = _marquee.rect.bounds;
        var layer2 = userLayers[state.activeLayerIdx];
        layer2.children.forEach(function (c) {
          if (c instanceof Path && c.segments.length > 0 && (c.strokeColor || c.fillColor) && mb.intersects(c.bounds)) {
            if (selectedPaths.indexOf(c) < 0) selectedPaths.push(c);
          }
        });
        state.selectedStrokeIndices = selectedPaths.map(getSI).filter(function (i2) { return i2 >= 0; });
        _marquee.rect.remove(); _marquee.rect = null;
      }
      _marquee.active = false;
      renderArcs(); updateUI();
    } else if (mode === 'move') {
      moveStarted = false;
      fillRegenerateLinked(userLayers[state.activeLayerIdx], null);
      saveActiveLayerFrame();
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
