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
      // Promote a HELD frame to a real keyframe before touching geometry.
      // Without this, editing vertices on a frame that merely inherits an
      // earlier keyframe was silently DISCARDED: saveActiveLayerFrame (app.js)
      // returns early when the frame is neither a keyframe nor interpolated,
      // so the edit lived only in the live Paper items until the next
      // loadFrame threw it away. Reported 2026-07-28 ("quand je modifie les
      // vecteurs d'une clé prolongée [...] ça ne modifie pas la clé").
      //
      // This also makes subselect answer the same way the Draw tool already
      // did — drawing on a held frame has always created a keyframe. Same
      // gesture class, same answer, nothing lost either way.
      var _wasKey = (function () {
        var f = state.layers[state.activeLayerIdx].frames[state.currentFrame];
        return !!(f && (f.isKeyframe || f.isInterpolated));
      })();
      ensureKeyframe();
      if (!_wasKey) {
        // ensureKeyframe -> loadFrame rebuilds the layer's children, so every
        // Paper reference captured above is now stale. Re-resolve by INDEX
        // (bestNh.segIndex stays valid — same geometry, new objects), exactly
        // as select-bridge's beginDistort does after its own ensureKeyframe.
        selectedPaths = state.selectedStrokeIndices
          .map(function (i) { return userLayers[state.activeLayerIdx].children[i]; })
          .filter(Boolean);
        if (!selectedPaths.length) return;
        renderNodeHandles();
      }
      _nodeDrag.active = true; _nodeDrag.path = selectedPaths[0]; _nodeDrag.segIndex = bestNh.segIndex;
      _nodeDrag.dragStartPointer = pt.clone(); _nodeDrag.appliedDelta = new Point(0, 0);
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
    var subHit = layer.hitTest(pt, { stroke: true, fill: true, pixel: true, tolerance: 8 / view.zoom });
    // Isolation entered by a Select double-click (tools.js's
    // onViewDoubleClick sets _fsIsolation): only that shape/group is
    // reachable, and a click outside it leaves back to Select. This file is
    // the ENGINE-ON port of the same branch and the engine is on by default,
    // so the guard has to exist in both or the gesture behaves differently
    // depending on the renderer — CLAUDE.md §3's duplicated-pair rule.
    if (window._fsIsolation && subHit) {
      var subAllowed = window._fsIsolation.groupId
        ? !!(subHit.item.data && subHit.item.data.groupId === window._fsIsolation.groupId)
        : subHit.item === window._fsIsolation.path;
      if (!subAllowed) subHit = null;
    }
    if (!subHit && window._fsIsolation) {
      window._fsIsolation = null; clearSel(); window.SM.setTool('select');
      renderArcs(); updateUI(); window.SMEngineBridge.renderNow();
      return;
    }
    // Raster companions count too (Bitmap Brush v2, bitmap-brush.js): the
    // visible texture over a bitmap-brush stroke is ONE Raster tagged
    // isBrushTextureCopy — its anchor path has strokeColor camouflaged to
    // null, so the stroke hit-test can't land on the anchor directly and
    // the texture is often the only thing under the cursor. The vector
    // presets never hit this (their dabs are Paths, already matched);
    // resolveBrushAnchor maps either kind back to the anchor. The
    // `instanceof Path` guard on subTarget keeps a plain imported image
    // (a Raster with NO companion tag, resolved to itself) un-node-editable
    // as before.
    if (subHit && (subHit.item instanceof Path || (subHit.item instanceof Raster && subHit.item.data && subHit.item.data.isBrushTextureCopy))) {
      clearSel();
      var subTarget = resolveBrushAnchor(subHit.item, layer);
      if (!(subTarget instanceof Path)) { window.SMEngineBridge.renderNow(); return; }
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
    } else {
      // "avoir aussi les rect selection box pour cet outil" (2026-07): a
      // marquee already existed here, but ONLY once a path was already
      // the edit target (selectedPaths.length===1) — dragging on empty
      // space with nothing pre-selected fell through to clearSel() and
      // did nothing, unlike Illustrator/Figma's Direct Selection tool,
      // where a marquee box works in one drag with no separate prior
      // click. Now always starts the marquee; onUp below picks the
      // target path itself (whichever path's bounds the box overlaps)
      // when none was already selected, THEN does the same node-
      // containment test as before.
      _nmq.active = true; _nmq.start = pt.clone(); _nmq.rect = null;
      window.SMEngineBridge.renderNow();
      return;
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
    if (_nodeDrag.active && (_nodeDrag.type === 'point' || _nodeDrag.type === 'group')) {
      var desired = pt.subtract(_nodeDrag.dragStartPointer || lastPt);
      if (e.shiftKey) {
        var snapped = constrainAngle45(_nodeDrag.dragStartPointer || lastPt, pt);
        desired = snapped.subtract(_nodeDrag.dragStartPointer || lastPt);
      }
      var already = _nodeDrag.appliedDelta || new Point(0, 0);
      delta = desired.subtract(already);
      _nodeDrag.appliedDelta = desired;
    } else if (_nodeDrag.active && e.shiftKey && (_nodeDrag.type === 'handleIn' || _nodeDrag.type === 'handleOut')) {
      var hs = nodeEditSegmentsData(_nodeDrag.path)[_nodeDrag.segIndex];
      if (hs) pt = constrainAngle45(new Point(hs.point[0], hs.point[1]), pt);
    }

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
      // Perf fix (2026-07, "pas très fluide" with thousands of points):
      // renderNodeHandles() rebuilds EVERY node's Paper.js Path/Circle
      // display objects from scratch — O(segment count) allocations, on
      // every single pointermove of the drag. Wasted work whenever the
      // Rust engine is on (the default): its own overlay
      // (buildNodeHandleItems, engine-bridge.js) re-reads segment
      // positions/_nodeSel directly from the live path on every
      // renderNow() call below, completely independent of nodeLayer's
      // Paper objects — confirmed by reading it before making this
      // change. nodeHandles[].pos (built by renderNodeHandles, used for
      // hit-testing) is only ever read at the START of a NEW click
      // (onDown), never mid-drag, so it doesn't need to stay fresh DURING
      // one either — onUp already refreshes it once the drag ends. Only
      // skipped when the engine is OFF, where nodeLayer's own Paper
      // objects ARE the visible overlay (the Paper-native fallback has no
      // other rendering path).
      if (!window.SMEngineBridge.isEnabled()) renderNodeHandles();
      // Live fill follow: without this, a fill linked to this stroke only
      // regenerated at mouseup, so the fill visibly lagged behind the
      // stroke for the whole drag instead of tracking it in real time.
      fillRegenerateLinked(userLayers[state.activeLayerIdx], gp);
      // Live texture follow for Bitmap Brush anchors — same "at mouseup
      // only, it visibly lags" complaint as the fill line above, fixed the
      // same way (rAF-coalesced + in-place re-upload, see liveRestamp's
      // own comment for why this is cheap where full regenerate isn't).
      if (gp.data && gp.data.bitmapBrushSpec && window.SMBitmapBrush) SMBitmapBrush.liveRestamp(gp, userLayers[state.activeLayerIdx]);
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
      // See the identical perf comment in the group-drag branch above.
      if (!window.SMEngineBridge.isEnabled()) renderNodeHandles();
      fillRegenerateLinked(userLayers[state.activeLayerIdx], sdp);
      // Live texture follow — see the identical call in the group branch.
      if (sdp.data && sdp.data.bitmapBrushSpec && window.SMBitmapBrush) SMBitmapBrush.liveRestamp(sdp, userLayers[state.activeLayerIdx]);
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
        // No path was already the edit target — pick one from what the
        // box actually overlaps (topmost, matching project.layers'
        // back-to-front draw order, same convention select-bridge.js
        // uses for its own topmost-first hit search) instead of leaving
        // the drag a no-op.
        if (!npath) {
          var layerForPick = userLayers[state.activeLayerIdx];
          var candidates = layerForPick ? layerForPick.children.filter(function (c) { return c instanceof Path && c.segments && c.segments.length && isSelectablePathChild(c) && c.bounds.intersects(nmb); }) : [];
          if (candidates.length) {
            var picked = candidates[candidates.length - 1];
            clearSel();
            selectedPaths.push(picked);
            state.selectedStrokeIndices = selectedPaths.map(getSI).filter(function (i2) { return i2 >= 0; });
            npath = nodeEditTargetPath();
          }
        }
        _nodeSel = [];
        if (npath) {
          var nsegs = nodeEditSegmentsData(npath);
          nsegs.forEach(function (s, i) { if (nmb.contains(new Point(s.point[0], s.point[1]))) _nodeSel.push(i); });
        }
        _nmq.rect.remove(); _nmq.rect = null;
        if (!_nodeSel.length) clearSel();
      } else { clearSel(); }
      _nmq.active = false; renderNodeHandles(); renderArcs(); updateUI();
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
      saveActiveLayerFrame();
      // Same stale-onion-ghost fix as select-bridge.js's xform/move commits
      // — onionPrevLayer/onionNextLayer are a snapshot cache never rebuilt
      // by a node-edit commit, so a held neighbor frame kept ghosting this
      // path at its pre-edit shape until an unrelated frame-nav/onion
      // toggle happened to call renderOS().
      renderOS();
      renderNodeHandles(); updateUI();
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
