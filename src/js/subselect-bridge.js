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
  // Click-vs-drag disambiguation for the Alt+click "toggle tangent" gesture
  // below — onDown already always starts a normal 'point' drag on hit, so
  // the only way to tell "user clicked to toggle" from "user dragged the
  // anchor a tiny bit" is comparing screen-space start/end position in onUp.
  var _downClientX = 0, _downClientY = 0, _downAlt = false;

  function shouldIntercept() {
    return window.SMEngineBridge && window.SMEngineBridge.isEnabled() && state.tool === 'subselect' && !state.playing;
  }

  // Illustrator/Figma "Convert Anchor Point" convention, ported to a plain
  // Alt+click (no drag) instead of a dedicated tool: a point WITH tangent
  // handles loses them (becomes a sharp corner); a point with NONE gets a
  // symmetric pair computed from its neighbors (Catmull-Rom-ish — direction
  // toward the far neighbor, length 1/3 of the distance to each adjacent
  // point) instead of requiring a drag to define them, since this is a
  // single click, not a drag-to-shape gesture.
  function toggleTangentAtSegments(segs, closed, idx) {
    var s = segs[idx];
    var hasTangent = (Math.abs(s.handleIn[0]) > 0.01 || Math.abs(s.handleIn[1]) > 0.01 || Math.abs(s.handleOut[0]) > 0.01 || Math.abs(s.handleOut[1]) > 0.01);
    if (hasTangent) { s.handleIn = [0, 0]; s.handleOut = [0, 0]; return; }
    var n = segs.length;
    var prev = closed ? segs[(idx - 1 + n) % n] : (idx > 0 ? segs[idx - 1] : null);
    var next = closed ? segs[(idx + 1) % n] : (idx < n - 1 ? segs[idx + 1] : null);
    var dir, distOut = 40, distIn = 40;
    var px = s.point[0], py = s.point[1];
    if (prev && next) {
      var dx = next.point[0] - prev.point[0], dy = next.point[1] - prev.point[1];
      var dl = Math.hypot(dx, dy) || 1; dir = [dx / dl, dy / dl];
    } else if (next) {
      var dx2 = next.point[0] - px, dy2 = next.point[1] - py;
      var dl2 = Math.hypot(dx2, dy2) || 1; dir = [dx2 / dl2, dy2 / dl2];
    } else if (prev) {
      var dx3 = px - prev.point[0], dy3 = py - prev.point[1];
      var dl3 = Math.hypot(dx3, dy3) || 1; dir = [dx3 / dl3, dy3 / dl3];
    } else { dir = [1, 0]; }
    if (next) distOut = Math.hypot(next.point[0] - px, next.point[1] - py);
    if (prev) distIn = Math.hypot(px - prev.point[0], py - prev.point[1]);
    s.handleOut = [dir[0] * distOut / 3, dir[1] * distOut / 3];
    s.handleIn = [-dir[0] * distIn / 3, -dir[1] * distIn / 3];
  }
  function toggleTangentAt(path, idx) {
    if (path.data && path.data.isVectorBrush && path.data.centerSegments) {
      toggleTangentAtSegments(path.data.centerSegments, !!path.closed, idx);
      rebuildVectorBrushOutline(path);
    } else {
      var seg = path.segments[idx];
      var hasTangent = seg.handleIn.length > 0.01 || seg.handleOut.length > 0.01;
      if (hasTangent) { seg.handleIn = new Point(0, 0); seg.handleOut = new Point(0, 0); return; }
      var segs = path.segments.map(function (s) { return { point: [s.point.x, s.point.y], handleIn: [s.handleIn.x, s.handleIn.y], handleOut: [s.handleOut.x, s.handleOut.y] }; });
      toggleTangentAtSegments(segs, !!path.closed, idx);
      seg.handleOut = new Point(segs[idx].handleOut[0], segs[idx].handleOut[1]);
      seg.handleIn = new Point(segs[idx].handleIn[0], segs[idx].handleIn[1]);
    }
  }

  function onDown(e) {
    if (!shouldIntercept()) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var pt = new Point(w[0], w[1]);
    lastPt = pt;
    _downClientX = e.clientX; _downClientY = e.clientY; _downAlt = e.altKey;
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
      var subTarget = resolveBrushAnchor(subHit.item, layer);
      selectedPaths.push(subTarget);
      state.selectedStrokeIndices = selectedPaths.map(getSI).filter(function (i2) { return i2 >= 0; });
      // The Rust mirror redraws the handle overlay fresh from
      // nodeEditTargetPath() every frame regardless, so this looked correct
      // visually — but the NEXT click's hit-test reads the plain `nodeHandles`
      // JS array, which only tools.js's renderNodeHandles() populates. Left
      // stale (still describing whatever was selected before, or empty on
      // the very first click), so a click that looked like "grab this
      // tangent handle" silently missed and fell through to reselecting the
      // path or starting a marquee — the actual "can't edit tangents" bug.
      renderNodeHandles();
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
      var editedPath = _nodeDrag.path; var editedType = _nodeDrag.type; var editedSegIndex = _nodeDrag.segIndex;
      _nodeDrag.active = false; _nodeDrag.path = null;
      // Alt+click (not drag) on an anchor toggles its tangent handles —
      // Illustrator/Figma "Convert Anchor Point" convention, reported as
      // "avec alt enfoncé il faudrait que ça referme les tangentes et si
      // il n'y a pas de tangente alors ça les affiche". onDown always
      // starts a normal 'point' drag on hit (no separate gesture to grab
      // here), so this only fires when Alt was held at mousedown AND the
      // pointer barely moved — a real drag still reshapes/repositions.
      var movedPx = Math.hypot(e.clientX - _downClientX, e.clientY - _downClientY);
      if (editedType === 'point' && _downAlt && movedPx < 4) {
        // No pushUndo() here — onDown already snapshotted state before any
        // mutation (line ~76: pushUndo() right before starting the drag),
        // a second one here would just double up the undo stack for one
        // visual change.
        toggleTangentAt(editedPath, editedSegIndex);
      }
      // Team review: reshaping someone else's stroke forks it — see
      // forkIfForeignOwner's own comment. Must run BEFORE fillRegenerateLinked/
      // regenerateBrushTexture below, which read the CURRENT strokeId to
      // re-associate fills/texture — forking assigns editedPath a fresh id.
      forkIfForeignOwner(editedPath);
      fillRegenerateLinked(userLayers[state.activeLayerIdx], editedPath);
      regenerateBrushTexture(editedPath, userLayers[state.activeLayerIdx]);
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
