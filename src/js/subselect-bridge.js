// ---- C7 CUTOVER: Subselect tool (node/handle editing) on the Rust engine ----
// Same architecture as select-bridge.js/shape-bridge.js: capture-phase
// pointer interception (stopImmediatePropagation blocks Paper's own
// onMouseDown/Drag/Up for the same canvas), engine-bridge.js's tick()
// suspended for the whole gesture, and only Paper.js *data* is mutated
// (segment points/handles, centerSegments) — rendering comes entirely from
// the Rust mirror, which already draws the node/handle overlay from the
// exact same source data (nodeEditTargetPath/nodeEditSegmentsData/_nodeSel,
// see engine-bridge.js's buildNodeHandleItems), so no separate overlay-
// drawing code is needed here: just mutate state, then ask engine-bridge to
// re-render.
//
// Faithful line-for-line port of the 'subselect' branches of
// onMouseDown/onMouseDrag/onMouseUp in tools.js — anchor/handle hit-test
// against the shared `nodeHandles` array (rebuilt by tools.js's own
// renderNodeHandles(), unchanged), single-node drag (point/handleIn/
// handleOut, with Alt breaking handle mirroring), group drag of a multi-
// selected set of anchors (points only, no handles), the node marquee
// (_nmq, anchor-points-only containment test), and vector-brush centerline
// special-casing (mutates data.centerSegments + rebuildVectorBrushOutline
// instead of touching path.segments directly) — all reusing the exact same
// global helpers/state tools.js already defines (nodeHandles, _nodeDrag,
// _nmq, _nodeSel, nodeEditTargetPath, nodeEditSegmentsData,
// renderNodeHandles, rebuildVectorBrushOutline, fillRegenerateLinked,
// clearSel, pushUndo, saveActiveLayerFrame, renderArcs, updateUI) rather
// than reimplementing any of it.
(function () {
  var lastPt = null;

  function shouldIntercept() {
    return window.SMEngineBridge && window.SMEngineBridge.isEnabled() && state.tool === 'subselect' && !state.playing;
  }

  function onDown(e) {
    if (!shouldIntercept()) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var pt = new Point(w[0], w[1]);
    lastPt = pt;
    window.SMEngineBridge.suspend();

    var layer = userLayers[state.activeLayerIdx];

    var bestNh = null, bestNd = 8 / view.zoom;
    for (var ni = 0; ni < nodeHandles.length; ni++) {
      var nh = nodeHandles[ni];
      var nd = pt.getDistance(nh.pos);
      if (nd < bestNd) { bestNd = nd; bestNh = nh; }
    }
    if (bestNh) {
      pushUndo();
      _nodeDrag.active = true; _nodeDrag.path = selectedPaths[0]; _nodeDrag.segIndex = bestNh.segIndex;
      // grabbing one of several marquee-selected anchors drags them all
      if (bestNh.type === 'point' && _nodeSel.indexOf(bestNh.segIndex) >= 0 && _nodeSel.length > 1) {
        _nodeDrag.type = 'group';
      } else {
        _nodeDrag.type = bestNh.type;
        if (bestNh.type === 'point') {
          _nodeSel = e.shiftKey ? _nodeSel.concat([bestNh.segIndex]) : [bestNh.segIndex];
          renderNodeHandles();
        }
      }
      window.SMEngineBridge.renderNow();
      return;
    }
    var subHit = layer.hitTest(pt, { stroke: true, fill: true, tolerance: 8 / view.zoom });
    if (subHit && subHit.item instanceof Path) {
      clearSel();
      selectedPaths.push(subHit.item);
      state.selectedStrokeIndices = selectedPaths.map(getSI).filter(function (i2) { return i2 >= 0; });
    } else if (selectedPaths.length === 1) {
      // empty-space drag with a path selected: marquee over its anchors
      _nmq.active = true; _nmq.start = pt.clone(); _nmq.rect = null;
      window.SMEngineBridge.renderNow();
      return;
    } else {
      clearSel();
    }
    renderArcs(); updateUI();
    window.SMEngineBridge.renderNow();
  }

  function onMove(e) {
    if (!(_nmq.active || _nodeDrag.active)) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var pt = new Point(w[0], w[1]);
    var delta = pt.subtract(lastPt);

    if (_nmq.active) {
      var nx1 = Math.min(_nmq.start.x, pt.x), ny1 = Math.min(_nmq.start.y, pt.y);
      var nx2 = Math.max(_nmq.start.x, pt.x), ny2 = Math.max(_nmq.start.y, pt.y);
      if (_nmq.rect) _nmq.rect.remove();
      var prevA2 = project.activeLayer; marqueeLayer.activate();
      _nmq.rect = new Path.Rectangle({
        from: new Point(nx1, ny1), to: new Point(nx2, ny2),
        strokeColor: 'rgba(255,184,108,.9)', strokeWidth: 1 / view.zoom,
        dashArray: [4 / view.zoom, 3 / view.zoom], fillColor: new Color(1, 0.72, 0.42, 0.08), insert: true,
      });
      prevA2.activate();
    } else if (_nodeDrag.active && _nodeDrag.type === 'group') {
      var gp = _nodeDrag.path;
      if (gp.data && gp.data.isVectorBrush && gp.data.centerSegments) {
        _nodeSel.forEach(function (si) { var cs3 = gp.data.centerSegments[si]; if (cs3) cs3.point = [cs3.point[0] + delta.x, cs3.point[1] + delta.y]; });
        rebuildVectorBrushOutline(gp);
      } else {
        _nodeSel.forEach(function (si) { var sg = gp.segments[si]; if (sg) sg.point = sg.point.add(delta); });
      }
      renderNodeHandles();
      // Live fill follow: without this, a fill linked to this stroke only
      // regenerated at mouseup, so the fill visibly lagged behind the
      // stroke for the whole drag instead of tracking it in real time.
      fillRegenerateLinked(userLayers[state.activeLayerIdx], gp);
    } else if (_nodeDrag.active) {
      var sdp = _nodeDrag.path;
      if (sdp.data && sdp.data.isVectorBrush && sdp.data.centerSegments) {
        var scs = sdp.data.centerSegments[_nodeDrag.segIndex];
        if (_nodeDrag.type === 'point') { scs.point = [scs.point[0] + delta.x, scs.point[1] + delta.y]; }
        else if (_nodeDrag.type === 'handleOut') { var sno = [pt.x - scs.point[0], pt.y - scs.point[1]]; scs.handleOut = sno; if (!e.altKey) scs.handleIn = [-sno[0], -sno[1]]; }
        else if (_nodeDrag.type === 'handleIn') { var sni = [pt.x - scs.point[0], pt.y - scs.point[1]]; scs.handleIn = sni; if (!e.altKey) scs.handleOut = [-sni[0], -sni[1]]; }
        rebuildVectorBrushOutline(sdp);
      } else {
        var sseg = sdp.segments[_nodeDrag.segIndex];
        if (_nodeDrag.type === 'point') sseg.point = sseg.point.add(delta);
        else if (_nodeDrag.type === 'handleOut') { sseg.handleOut = pt.subtract(sseg.point); if (!e.altKey) sseg.handleIn = sseg.handleOut.multiply(-1); }
        else if (_nodeDrag.type === 'handleIn') { sseg.handleIn = pt.subtract(sseg.point); if (!e.altKey) sseg.handleOut = sseg.handleIn.multiply(-1); }
      }
      renderNodeHandles();
      fillRegenerateLinked(userLayers[state.activeLayerIdx], sdp);
    }
    lastPt = pt;
    window.SMEngineBridge.renderNow();
  }

  function onUp(e) {
    if (!(_nmq.active || _nodeDrag.active)) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    if (_nmq.active) {
      if (_nmq.rect) {
        var nmb = _nmq.rect.bounds;
        var npath = nodeEditTargetPath();
        _nodeSel = [];
        if (npath) {
          var nsegs = nodeEditSegmentsData(npath);
          nsegs.forEach(function (s, i) { if (nmb.contains(new Point(s.point[0], s.point[1]))) _nodeSel.push(i); });
        }
        _nmq.rect.remove(); _nmq.rect = null;
        if (!_nodeSel.length) clearSel();
      } else { clearSel(); }
      _nmq.active = false; renderArcs(); updateUI();
    } else if (_nodeDrag.active) {
      var editedPath = _nodeDrag.path; _nodeDrag.active = false; _nodeDrag.path = null;
      fillRegenerateLinked(userLayers[state.activeLayerIdx], editedPath);
      saveActiveLayerFrame(); renderNodeHandles(); updateUI();
    }
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
