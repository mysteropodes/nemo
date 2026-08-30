// ---- LABS PROTOTYPE — Vector sculpt: push & smooth (Toon Boom / Umoupen) ----
// The "finger" for vector ink: enable it from the Select/Subselect floating
// panel, then drag over the drawing —
// every stroke point inside the brush radius is PUSHED along the drag
// with a soft falloff (Toon Boom contour nudging / Umoupen warp,
// Illustrator's Warp tool). Hold W+Shift and scrub instead to SMOOTH
// (each point relaxes toward its neighbors' midpoint — Harmony's Smooth
// Editor).
//
//   drag             — push/warp brush
//   Shift + drag     — smooth/relax brush
//   Alt + horizontal drag — resize the on-canvas brush radius
//   SMLabs.setSculptRadius(px)   — SCREEN-px brush radius (default 60)
//
// Interception: document-level capture listeners (they fire BEFORE the
// tool bridges' own #canvas-area capture handlers), active only while the
// Labs toggle is on and Select/Subselect is the current tool. Displaces
// segment POINTS only
// (handles follow their point, curve character survives); vector-brush
// ribbons and fill shapes are skipped (their outlines are rebuilt from
// centerSegments — sculpting the outline would desync, same reason
// predictive-stroke skips them). Fills bounded by sculpted walls are
// regenerated at gesture end via fillRegenerateLinked, like the eraser
// does per bite. One pushUndo per gesture.
(function () {
  var RAD_KEY = 'nemo-labs-sculpt-radius';
  var dragging = false, resizing = false, lastW = null, undoPushed = false, touched = [];
  // #122: snapshot BEFORE the gesture whether the current frame is a
  // generated tween in-between — saveActiveLayerFrame (onUp) promotes it to
  // a real keyframe the instant content actually changes, so by onUp time
  // isInterpolated is already gone; this is the only place that can still
  // tell "this edit just performed the promotion" from "this frame was
  // already a real keyframe" (see harmonizeAfterEdit's own comment, tweens.js).
  var wasInterpolated = false;
  var resizeStartX = 0, resizeStartRadius = 60;
  var cursor = null;
  // Feedback #139 ("le sculpt deform n'est pas en temps réel sur une brush
  // avec un preset"): #129 fixed regenerateBrushTexture() never being called
  // at all, but only wired it into onUp (drag-END) — regenerateBrushTexture's
  // own header comment in tools.js is explicit: "buildBrushDabs is a full
  // rebuild, not cheap per mousemove" (remove every old dab Path + walk the
  // anchor's current geometry stamping up to BRUSH_MAX_DABS=900 fresh Path
  // objects — measured ~14.8ms worst case per full rebuild, see tools.js's
  // BRUSH_MAX_DABS comment). So onMove moved the anchor but the visible dabs
  // stayed put until release — exactly the "not real-time" complaint. Same
  // fix shape as feedback #128 (motion-graph.js, CLAUDE.md §5bis): the DATA
  // write (segment positions, in applyPush/applySmooth) stays synchronous on
  // every move, but the expensive re-stamp is coalesced to one rAF tick —
  // a mouse/stylet fires far more move events per frame than the screen can
  // show anyway, so re-stamping once per SCREEN REFRESH instead of once per
  // input event loses nothing visible. onUp already forces one final
  // synchronous regenerateBrushTexture call (added by #129) so the very last
  // position is never left showing a stale/queued texture.
  var texRafQueued = false, texLayer = null;
  function scheduleTextureUpdate(layer) {
    texLayer = layer;
    if (texRafQueued) return;
    texRafQueued = true;
    requestAnimationFrame(function () {
      texRafQueued = false;
      flushTextureUpdate();
    });
  }
  function flushTextureUpdate() {
    var layer = texLayer;
    if (!layer || !touched.length) return;
    // regenerateBrushTexture no-ops instantly on a path with no
    // brushTexturePreset (its own guard, tools.js) — safe/cheap to call for
    // every touched path regardless of whether it's textured, so a
    // non-textured stroke's sculpt behavior is unaffected by this at all.
    touched.forEach(function (p) { if (typeof regenerateBrushTexture === 'function') regenerateBrushTexture(p, layer); });
    SMEngineBridge.renderNow();
  }

  function isActive() {
    return !!(window.SMLabs && window.SMLabs.isOn('vector-sculpt') &&
      window.state && (state.tool === 'select' || state.tool === 'subselect') &&
      !state.playing);
  }
  function ensureCursor() {
    if (cursor) return cursor;
    cursor = document.createElement('div');
    cursor.id = 'vector-sculpt-cursor';
    cursor.setAttribute('aria-hidden', 'true');
    document.body.appendChild(cursor);
    return cursor;
  }
  // These are DOCUMENT-level capture handlers that swallow the event
  // (stopImmediatePropagation + preventDefault) — so they must refuse
  // anything outside the drawing area, or turning the prototype on kills
  // every click in the app: timeline rows, panel fields, toolbar buttons
  // (found 2026-07-27 while testing the timeline — a pointerdown on
  // #layer-list never reached a single other listener). onMove/onUp only
  // act once a drag started here, so gating the entry point covers all three.
  function inCanvas(e) {
    var area = document.getElementById('canvas-area');
    return !!(area && e.target && area.contains(e.target));
  }
  function updateCursor(e) {
    var c = ensureCursor();
    var visible = isActive() && inCanvas(e);
    c.style.display = visible ? 'block' : 'none';
    if (!visible) return;
    var r = radiusScreen();
    c.style.width = (r * 2) + 'px';
    c.style.height = (r * 2) + 'px';
    c.style.left = (e.clientX - r) + 'px';
    c.style.top = (e.clientY - r) + 'px';
    c.classList.toggle('resizing', resizing);
  }

  function radiusScreen() {
    var n = parseFloat(localStorage.getItem(RAD_KEY) || '60');
    return (isNaN(n) || n < 8) ? 60 : Math.min(400, n);
  }
  window.SMLabs.setSculptRadius = function (n) {
    localStorage.setItem(RAD_KEY, String(Math.max(8, Math.min(400, +n || 60))));
    if (window.renderLabsFloatPanel) window.renderLabsFloatPanel();
    return radiusScreen();
  };

  // A pressure-brush stroke is an OUTLINE generated from a centerline; the
  // centerline (data.centerSegments) is the editable thing, and
  // rebuildVectorBrushOutline regenerates the ribbon — and re-syncs its
  // linked fill backdrop — from it. Same isVB/centerSegments split
  // setPointType and every node-drag already use (tools.js).
  function isRibbon(c) {
    return !!(c.data && c.data.isVectorBrush && c.data.centerSegments && c.data.centerSegments.length >= 2);
  }

  function sculptables(layer) {
    return layer.children.filter(function (c) {
      if (!(c instanceof Path)) return false;
      if (c.data && (c.data.isBrushTextureCopy || c.data.ghostFrame !== undefined)) return false;
      // A linked fill backdrop is REBUILT from its ribbon's centerline, so
      // sculpting it as an independent path is precisely what pushed the
      // fill while the stroke stayed put (2026-07-27: "le sculpt vecto ne
      // prend pas le trait en même temps que le fill surtout quand ils sont
      // attaché l'un à l'autre"). It was the ONE companion tag this filter
      // never listed — CLAUDE.md §1.
      if (c.data && c.data.isLinkedFillCompanion) return false;
      if (isRibbon(c)) return true;
      // Bucket fills are re-traced from their walls at gesture end instead.
      if (c.data && c.data.isFillShape) return false;
      return !!(c.segments && c.segments.length >= 2);
    });
  }

  // Nodes to displace: the centerline for a ribbon, the path's own segments
  // otherwise. Handles are stored relative to their point in both models, so
  // moving a point carries its handles and the curve character survives.
  function nodesOf(p) { return isRibbon(p) ? p.data.centerSegments : p.segments; }

  function centerPathOf(p) {
    var c = new Path({ insert: false });
    p.data.centerSegments.forEach(function (s) {
      var seg = new Segment(new Point(s.point[0], s.point[1]),
        new Point(s.handleIn ? s.handleIn[0] : 0, s.handleIn ? s.handleIn[1] : 0),
        new Point(s.handleOut ? s.handleOut[0] : 0, s.handleOut ? s.handleOut[1] : 0));
      c.add(seg);
      // Ride the width along on the Segment object itself rather than a
      // parallel array — divideAt() splices a new segment in and would shift
      // every index after it.
      c.segments[c.segments.length - 1]._w = s.width;
    });
    return c;
  }

  function writeCenterPath(p, c) {
    p.data.centerSegments = c.segments.map(function (s) {
      return {
        point: [s.point.x, s.point.y],
        handleIn: [s.handleIn.x, s.handleIn.y],
        handleOut: [s.handleOut.x, s.handleOut.y],
        width: s._w,
      };
    });
  }

  // An anchor inserted by divideAt has no width of its own — interpolate it
  // from the nearest neighbours that do, or rebuildVectorBrushOutline reads
  // undefined and the ribbon collapses to NaN width at that point.
  function fillMissingWidths(segs) {
    for (var i = 0; i < segs.length; i++) {
      if (segs[i]._w != null) continue;
      var a = i - 1; while (a >= 0 && segs[a]._w == null) a--;
      var b = i + 1; while (b < segs.length && segs[b]._w == null) b++;
      var wa = a >= 0 ? segs[a]._w : null, wb = b < segs.length ? segs[b]._w : null;
      segs[i]._w = wa == null ? (wb == null ? 1 : wb) : (wb == null ? wa : (wa + wb) / 2);
    }
  }

  // A long simplified stroke can pass straight through the brush with
  // both anchors far outside it (a ruler-straight line has exactly 2) —
  // pushing anchors alone then does nothing (found live). Toon Boom
  // subdivides under the brush: if the CURVE runs within the radius but
  // no anchor is inside, insert anchors at the nearest spot (and once
  // more each side for a bendable span) before displacing.
  function divideUnderBrush(c, center, R) {
    var added = false;
    for (var guard = 0; guard < 6; guard++) {
      var anyInside = c.segments.some(function (seg) { return seg.point.getDistance(center) < R * 0.8; });
      if (anyInside) break;
      var near = c.getNearestLocation(center);
      if (!near || near.point.getDistance(center) >= R) break;
      if (!c.divideAt(near)) break;
      added = true;
    }
    return added;
  }

  function densifyUnderBrush(p, center, R) {
    if (!isRibbon(p)) { divideUnderBrush(p, center, R); return; }
    var c = centerPathOf(p);
    if (divideUnderBrush(c, center, R)) { fillMissingWidths(c.segments); writeCenterPath(p, c); }
    c.remove();
  }

  // ---- Image meshes under the brush (2026-08-30) ----------------------
  // Cyril: "il faudrait que l'outil sculpt déforme aussi les points de mesh
  // quand ils sont actifs". The brush already pushes path vertices with a
  // quadratic falloff; a mesh vertex is the same idea in a different store,
  // so this reuses the SAME radius and the SAME falloff rather than giving
  // meshes a second, subtly-different feel.
  //
  // "quand ils sont actifs" is taken literally: only a raster whose mesh is
  // currently in EDIT mode is sculpted. Otherwise dragging near any meshed
  // image on the layer would silently reshape it while you meant to sculpt a
  // stroke — meshes are attached to images that mostly sit still.
  //
  // Offsets are normalized (0..1 of the image rect), so the world-space delta
  // is converted through the rect rather than added raw; a rotated or scaled
  // image therefore follows the pointer instead of drifting off-axis.
  function meshTargets(layer) {
    if (!window.SMImageMesh || !window.SMImageMeshUI || !SMImageMeshUI.isEditing()) return [];
    if (typeof Raster === 'undefined' || !window.SMEngineBridge || !SMEngineBridge.rasterImageRect) return [];
    var out = [];
    layer.children.forEach(function (c) {
      if (!(c instanceof Raster) || !c.data || !c.data.meshId) return;
      var mesh = SMImageMesh.get(c.data.meshId);
      if (!mesh) return;
      out.push({ id: c.data.meshId, mesh: mesh, rect: SMEngineBridge.rasterImageRect(c) });
    });
    return out;
  }
  function pushMeshes(layer, center, delta, R) {
    var moved = false;
    meshTargets(layer).forEach(function (t) {
      var world = SMImageMesh.worldVerts(t.mesh, t.rect, null);
      if (!world) return;
      // Same conversion the offsets themselves use: a normalized delta is the
      // world delta expressed in the rect's own (possibly rotated) frame.
      var rad = (t.rect.rotation || 0) * Math.PI / 180;
      var cos = Math.cos(rad), sin = Math.sin(rad);
      var lw = (delta.x * cos + delta.y * sin) / (t.rect.width || 1);
      var lh = (-delta.x * sin + delta.y * cos) / (t.rect.height || 1);
      for (var i = 0; i < world.length; i++) {
        // worldVerts returns [x, y] ARRAYS, not Points — same shape the mesh
        // editor's own hit-test reads (rv.pts[i][0]). Reading .x/.y here gave
        // NaN distances, which never tripped the `d >= R` skip, so every
        // vertex got an offset of NaN and the mesh looked untouched. Found by
        // driving a real sculpt drag and counting moved vertices: 0.
        var d = Math.hypot(world[i][0] - center.x, world[i][1] - center.y);
        if (d >= R) continue;
        var w = 1 - d / R; w = w * w;
        var off = (t.mesh.offsets && t.mesh.offsets[i]) || [0, 0];
        SMImageMesh.setOffset(t.id, i, off[0] + lw * w, off[1] + lh * w);
        moved = true;
      }
    });
    return moved;
  }

  function applyPush(layer, center, delta, R) {
    var moved = false;
    sculptables(layer).forEach(function (p) {
      // Cheap reject: brush circle vs stroke bbox.
      if (!p.bounds.expand(2 * R).contains(center)) return;
      densifyUnderBrush(p, center, R);
      var vb = isRibbon(p), hit = false;
      nodesOf(p).forEach(function (seg) {
        var pt = _ptGet(vb, seg);
        var d = pt.getDistance(center);
        if (d >= R) return;
        var w = 1 - d / R; w = w * w; // quadratic falloff
        _ptSet(vb, seg, pt.add(delta.multiply(w)));
        hit = true;
      });
      if (hit) {
        if (vb) rebuildVectorBrushOutline(p); // regenerates the ribbon AND its linked fill
        moved = true;
        if (touched.indexOf(p) < 0) touched.push(p);
      }
    });
    return moved;
  }

  function applySmooth(layer, center, R) {
    var moved = false;
    sculptables(layer).forEach(function (p) {
      if (!p.bounds.expand(2 * R).contains(center)) return;
      var vb = isRibbon(p), segs = nodesOf(p), hit = false;
      // Relax interior points toward their neighbors' midpoint; endpoints
      // stay pinned so the stroke never shrinks off its anchors.
      for (var i = 1; i < segs.length - 1; i++) {
        var pt = _ptGet(vb, segs[i]);
        var d = pt.getDistance(center);
        if (d >= R) continue;
        var w = (1 - d / R); w = w * w * 0.35; // gentler than push
        var mid = _ptGet(vb, segs[i - 1]).add(_ptGet(vb, segs[i + 1])).divide(2);
        _ptSet(vb, segs[i], pt.add(mid.subtract(pt).multiply(w)));
        hit = true;
      }
      if (hit) {
        if (vb) rebuildVectorBrushOutline(p);
        moved = true;
        if (touched.indexOf(p) < 0) touched.push(p);
      }
    });
    return moved;
  }

  function onDown(e) {
    if (!isActive() || !inCanvas(e)) return;
    e.stopImmediatePropagation(); e.preventDefault();
    if (e.altKey) {
      resizing = true;
      resizeStartX = e.clientX;
      resizeStartRadius = radiusScreen();
      updateCursor(e);
      return;
    }
    dragging = true; undoPushed = false; touched = [];
    var curLd = state.layers[state.activeLayerIdx];
    var curF = curLd && curLd.frames[state.currentFrame];
    wasInterpolated = !!(curF && curF.isInterpolated);
    var w = SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    lastW = new Point(w[0], w[1]);
  }
  function onMove(e) {
    updateCursor(e);
    if (!isActive()) return;
    // Feedback #94: "après avoir utilisé sculpt vector j'ai des lags sur
    // l'affichage et les outils" — a pointerup/pointercancel can be missed
    // by the browser (released outside the window, Alt-Tab mid-drag, a
    // right-click interrupting) and this listener never resets dragging.
    // With no safety net, EVERY subsequent mousemove — including ones that
    // have nothing to do with sculpting, as long as Select/Subselect stays
    // the active tool — silently re-ran the full applyPush/applySmooth
    // pass (walk every sculptable path in the layer, distance-test every
    // segment) forever, which reads exactly as "lag after using the tool"
    // since nothing on screen hints sculpting is still armed. e.buttons is
    // the actual live button state (unlike a stored flag it can't go
    // stale) — bit 1 is the primary button; if it's not set the drag
    // already ended somewhere this listener never heard about.
    if ((dragging || resizing) && !(e.buttons & 1)) {
      dragging = false; resizing = false; lastW = null; touched = [];
      updateCursor(e);
      return;
    }
    if (resizing) {
      e.stopImmediatePropagation(); e.preventDefault();
      window.SMLabs.setSculptRadius(resizeStartRadius + (e.clientX - resizeStartX));
      updateCursor(e);
      return;
    }
    if (!dragging) return;
    e.stopImmediatePropagation(); e.preventDefault();
    var w = SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var cur = new Point(w[0], w[1]);
    var layer = userLayers[state.activeLayerIdx];
    if (!layer || state.layers[state.activeLayerIdx].locked) return;
    var R = radiusScreen() / view.zoom;
    if (!undoPushed) { pushUndo(); ensureKeyframe(); undoPushed = true; }
    var delta = cur.subtract(lastW);
    var moved = e.shiftKey ? applySmooth(layer, cur, R) : applyPush(layer, cur, delta, R);
    // Meshes are pushed alongside the paths, not instead of them: one drag
    // over an image sitting on top of a stroke moves both, which is what the
    // brush looks like it is doing.
    if (!e.shiftKey && pushMeshes(layer, cur, delta, R)) moved = true;
    lastW = cur;
    if (moved) {
      saveActiveLayerFrame();
      SMEngineBridge.renderNow();
      // #139: re-stamp textured strokes' visible dabs during the drag too,
      // not just at release — coalesced to one rAF tick (see
      // scheduleTextureUpdate's header comment above).
      scheduleTextureUpdate(layer);
    }
  }
  function onUp(e) {
    if (resizing) {
      e.stopImmediatePropagation(); e.preventDefault();
      resizing = false;
      updateCursor(e);
      return;
    }
    if (!dragging) return;
    e.stopImmediatePropagation(); e.preventDefault();
    dragging = false; lastW = null;
    var layer = userLayers[state.activeLayerIdx];
    // Fills whose walls were sculpted re-trace against the new geometry.
    touched.forEach(function (p) { if (typeof fillRegenerateLinked === 'function') fillRegenerateLinked(layer, p); });
    // Feedback #129 ("le sculpt brush marche pas sur des stroke texturé"): a
    // brush-texture anchor's dabs are disposable stamps computed once at
    // apply-time — they never re-follow an edited anchor on their own (see
    // regenerateBrushTexture's own header comment, tools.js). subselect-
    // bridge.js's node-drag already re-stamps on drag-end; this tool's own
    // push/smooth drag moves the anchor's segments exactly the same way but
    // never called the same re-stamp, so sculpting a textured stroke visibly
    // did nothing — the anchor itself moved (invisible, camouflaged), the
    // dabs stayed put.
    touched.forEach(function (p) { if (typeof regenerateBrushTexture === 'function') regenerateBrushTexture(p, layer); });
    touched = [];
    saveActiveLayerFrame();
    if (wasInterpolated && typeof harmonizeAfterEdit === 'function') harmonizeAfterEdit(state.currentFrame);
    updateUI();
    SMEngineBridge.renderNow();
  }

  document.addEventListener('pointerleave', function (e) {
    if (cursor && e.target === document.documentElement) cursor.style.display = 'none';
  }, true);
  window.addEventListener('blur', function () {
    dragging = false; resizing = false; lastW = null;
    if (cursor) cursor.style.display = 'none';
  });

  // Document-level capture: fires before the tool bridges' #canvas-area
  // capture handlers, so while W is held no bridge or Paper handler ever
  // sees the gesture.
  document.addEventListener('pointerdown', onDown, true);
  document.addEventListener('pointermove', onMove, true);
  document.addEventListener('pointerup', onUp, true);
  document.addEventListener('pointercancel', onUp, true);

  window.SMLabs.register('vector-sculpt', {
    flag: 'nemo-labs-sculpt',
    describe: 'labsDescribeVectorSculpt',
    onDisable: function () {
      dragging = false; resizing = false; lastW = null; touched = [];
      if (cursor) cursor.style.display = 'none';
    },
  });
})();
