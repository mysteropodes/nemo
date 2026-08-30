// ---- IMAGE MESH EDITOR (2026-08-30) ----
//
// The on-canvas half of the image mesh (data model + rendering: image-mesh.js
// and engine.rs's draw_image_mesh). Select an imported image with the
// Selection tool, turn "Mesh" on in the right panel, drag the dots.
//
// Deliberately NOT a 24th toolbar button. The mesh belongs to the image, not
// to a global mode — so it follows the gradient gizmo's pattern
// (gradient-bridge.js): a panel section that appears for the one selected
// item that can have one, plus a capture-phase pointer intercept that only
// arms while that item is selected and the mode is on. Same reason the
// gradient's angle is dragged on canvas rather than typed: position is a
// gesture, not a number.
//
// ---- THE OUTLINE IS THE MASK ----
//
// verts[0 .. outline.length-1] ARE the outline, in order (image-mesh.js's
// index invariant). So dragging one of those vertices reshapes the MASK and
// the deformation at once — there is no separate "edit the mask" mode to
// switch into, which is the whole point of Cyril's "the mask outline is also
// the mesh boundary" rule. The editor says this out loud rather than leaving
// it to be discovered: outline vertices are drawn as squares in the accent
// orange, interior vertices as blue circles, and the outline edge is drawn
// heavier than the interior wireframe.
//
// ---- HANDLE CONVENTIONS ----
//
// Copied from buildNodeHandleItems (engine-bridge.js), not invented here:
// sizes in 1/view.zoom so they stay screen-constant, blue [74,158,255] for
// an idle handle, accent orange [255,184,108] for a selected/special one,
// white 1px stroke, and thin [120,170,255] guide lines. A parallel visual
// language for one feature is how an app starts feeling like several apps.
(function () {
  'use strict';

  var IDLE = [74, 158, 255, 255];
  var ACCENT = [255, 184, 108, 255];
  var WHITE = [255, 255, 255, 255];
  var WIRE = [120, 170, 255, 130];
  var OUTLINE = [255, 184, 108, 220];

  var editing = false;      // the panel's "Mesh" mode toggle
  var dragIdx = -1;         // vertex being dragged, -1 = none
  var dragMoved = false;

  function engineOn() { return window.SMEngineBridge && window.SMEngineBridge.isEnabled() && !state.playing; }

  // The one selected item that can carry a mesh. Same shape as
  // gradient-bridge's singleTarget, restricted to Rasters.
  function singleRaster() {
    if ((state.tool !== 'select' && state.tool !== 'subselect') || !window.selectedPaths || selectedPaths.length !== 1) return null;
    var p = selectedPaths[0];
    if (typeof Raster === 'undefined' || !(p instanceof Raster)) return null;
    // A selection entry can outlive the object it points at: loadFrame,
    // undo/redo and importJSON all rebuild every Paper item, and
    // selectedPaths isn't always rebound in the same tick. A detached
    // Raster still passes `instanceof` and still carries data.meshId, so
    // without this check the panel would offer to edit a ghost — and
    // detach() would delete the live tag while SMImageMesh.propagate
    // silently no-oped (no parent = no layer = no frames to untag),
    // leaving the mesh half-removed: gone on screen, back on the next
    // loadFrame. Found live, driving the real checkbox after an import.
    if (typeof userLayers === 'undefined' || !p.parent || userLayers.indexOf(p.parent) < 0) return null;
    return p;
  }
  function targetMesh() {
    var r = singleRaster();
    if (!r || !r.data || !r.data.meshId || !window.SMImageMesh) return null;
    var mesh = SMImageMesh.get(r.data.meshId);
    return mesh ? { raster: r, mesh: mesh, meshId: r.data.meshId } : null;
  }

  // Vertex positions in RENDERED space. Two mappings are involved and both
  // already exist elsewhere — the raster's own display rect (engine-bridge's
  // rasterImageRect, shared rather than re-derived) and the layer's
  // render-time-only Motion transform (SMMotion.layerMotionPointMap, exactly
  // what buildNodeHandleItems does for path handles, and for the same
  // reason: without it the handles sit at the shape's pre-transform position
  // while the picture is somewhere else).
  function motionMap() {
    if (!window.SMMotion) return null;
    var m = SMMotion.layerMotionPointMap ? SMMotion.layerMotionPointMap(state.activeLayerIdx) : null;
    if (!m && SMMotion.layerMotion3DPointMap) m = SMMotion.layerMotion3DPointMap(state.activeLayerIdx);
    return m;
  }
  // The animated pose for this mesh at the current frame, or null when it
  // has no vertex tracks at all. Identical to what buildSceneJson feeds the
  // renderer (engine-bridge.js) — the handles have to sit on the picture,
  // so both sides read the same thing rather than the overlay quietly
  // showing the rest sculpt while the image renders its animated pose.
  function poseAtFor(t) {
    if (!window.SMMotion || !SMMotion.hasMeshVertexMotionFor) return null;
    var li = state.activeLayerIdx;
    if (!SMMotion.hasMeshVertexMotionFor(li, t.meshId)) return null;
    return function (vi) { return SMMotion.meshVertexOffsetAt(li, t.meshId, vi, state.currentFrame); };
  }
  function renderedVerts(t) {
    if (!window.SMEngineBridge || !SMEngineBridge.rasterImageRect) return null;
    var rect = SMEngineBridge.rasterImageRect(t.raster);
    var pts = SMImageMesh.worldVerts(t.mesh, rect, poseAtFor(t));
    if (!pts) return null;
    var mm = motionMap();
    if (mm) pts = pts.map(function (p) { return mm.fwd(p[0], p[1]); });
    return { pts: pts, rect: rect, mm: mm };
  }

  // ---- overlay ---------------------------------------------------------
  function lineItem(a, b, color, w) {
    return { segments: [{ point: [a[0], a[1]] }, { point: [b[0], b[1]] }], closed: false, fillColor: null, strokeColor: color, strokeWidth: w, strokeCap: 'butt' };
  }
  function dotItem(p, r, fill) {
    // 12-gon, same approximation gradient-bridge uses for its own round
    // handle — a circle here would need bezier handles for no visible gain
    // at handle size.
    var segs = [];
    for (var i = 0; i < 12; i++) { var a = (i / 12) * Math.PI * 2; segs.push({ point: [p[0] + Math.cos(a) * r, p[1] + Math.sin(a) * r] }); }
    return { segments: segs, closed: true, fillColor: fill, strokeColor: WHITE, strokeWidth: r * 0.25 };
  }
  function squareItem(p, r, fill) {
    return {
      segments: [{ point: [p[0] - r, p[1] - r] }, { point: [p[0] + r, p[1] - r] }, { point: [p[0] + r, p[1] + r] }, { point: [p[0] - r, p[1] + r] }],
      closed: true, fillColor: fill, strokeColor: WHITE, strokeWidth: r * 0.25,
    };
  }

  function buildImageMeshOverlayItems() {
    if (!editing || !engineOn()) return [];
    var t = targetMesh();
    if (!t) return [];
    var rv = renderedVerts(t);
    if (!rv) return [];
    var pts = rv.pts, mesh = t.mesh;
    var zs = 1 / view.zoom;
    var items = [];
    // Interior wireframe first, so vertices sit on top of it. Every triangle
    // edge is drawn once per triangle (so shared edges are drawn twice) —
    // deliberate: de-duplicating would need an edge set built per frame for
    // a purely cosmetic gain on a translucent stroke.
    for (var k = 0; k + 2 < mesh.tris.length; k += 3) {
      var a = pts[mesh.tris[k]], b = pts[mesh.tris[k + 1]], c = pts[mesh.tris[k + 2]];
      if (!a || !b || !c) continue;
      items.push(lineItem(a, b, WIRE, 1 * zs));
      items.push(lineItem(b, c, WIRE, 1 * zs));
      items.push(lineItem(c, a, WIRE, 1 * zs));
    }
    // The mask boundary, heavier and in the accent colour — this is the line
    // that decides what is visible, so it should not read as one more
    // wireframe edge.
    for (var o = 0; o < mesh.outline.length; o++) {
      var p0 = pts[o], p1 = pts[(o + 1) % mesh.outline.length];
      if (p0 && p1) items.push(lineItem(p0, p1, OUTLINE, 1.6 * zs));
    }
    for (var i = 0; i < pts.length; i++) {
      var isOutline = SMImageMesh.isOutlineVertex(mesh, i);
      var isDrag = (i === dragIdx);
      var r = (isDrag ? 5 : 3.5) * zs;
      items.push(isOutline ? squareItem(pts[i], r, isDrag ? WHITE : ACCENT)
                           : dotItem(pts[i], r, isDrag ? WHITE : IDLE));
    }
    return items;
  }
  window.buildImageMeshOverlayItems = buildImageMeshOverlayItems;

  // ---- drag interaction ------------------------------------------------
  function onDown(e) {
    if (!editing || !engineOn()) return;
    var t = targetMesh();
    if (!t) return;
    var rv = renderedVerts(t);
    if (!rv) return;
    var w = SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    // Screen-space tolerance, like every other handle hit-test in the app —
    // a world-space one would be unusable when zoomed out.
    var tol = 10 / view.zoom;
    var best = -1, bestD = tol;
    for (var i = 0; i < rv.pts.length; i++) {
      var d = Math.hypot(rv.pts[i][0] - w[0], rv.pts[i][1] - w[1]);
      if (d < bestD) { bestD = d; best = i; }
    }
    if (best < 0) return;
    dragIdx = best; dragMoved = false;
    e.stopImmediatePropagation(); e.preventDefault();
    pushUndo();
    SMEngineBridge.suspend();
  }
  function onMove(e) {
    if (dragIdx < 0) return;
    var t = targetMesh();
    if (!t) { dragIdx = -1; return; }
    e.stopImmediatePropagation(); e.preventDefault();
    var rect = SMEngineBridge.rasterImageRect(t.raster);
    var w = SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    // Back through the layer's Motion transform first, into the raw space
    // the rect itself lives in, then into the rect's normalized space —
    // the exact reverse of renderedVerts' forward chain.
    var mm = motionMap();
    if (mm && mm.inv) { var iv = mm.inv(w[0], w[1]); w = [iv[0], iv[1]]; }
    var uv = SMImageMesh.normalizedOf(rect, w[0], w[1]);
    var rest = t.mesh.verts[dragIdx];
    // Where the drag lands depends on the stopwatch, exactly like every
    // other Motion property: an ANIMATED vertex records a key (or a static
    // override) on its vtxN track, an un-animated one edits the mesh's own
    // rest sculpt. The track value is a deviation ON TOP of the sculpt, so
    // it is measured from rest+sculpt rather than from rest.
    var li = state.activeLayerIdx;
    if (window.SMMotion && SMMotion.isMeshVertexAnimated && SMMotion.isMeshVertexAnimated(li, t.meshId, dragIdx)) {
      var sculpt = (t.mesh.offsets && t.mesh.offsets[dragIdx]) || [0, 0];
      SMMotion.setMeshVertexOffset(li, t.meshId, dragIdx, uv[0] - rest[0] - sculpt[0], uv[1] - rest[1] - sculpt[1]);
    } else {
      SMImageMesh.setOffset(t.meshId, dragIdx, uv[0] - rest[0], uv[1] - rest[1]);
    }
    dragMoved = true;
    SMEngineBridge.renderNow();
  }
  function onUp(e) {
    if (dragIdx < 0) return;
    e.stopImmediatePropagation(); e.preventDefault();
    // A drag that landed on a vtxN track may have created a keyframe, which
    // only shows up once the timeline re-renders — the panel/grid pair is
    // rebuilt wholesale (motion.js), never incrementally, so nothing else
    // would repaint it until the next unrelated UI event.
    var tUp = targetMesh();
    if (dragMoved && tUp && window.SMMotion && SMMotion.isMeshVertexAnimated
        && SMMotion.isMeshVertexAnimated(state.activeLayerIdx, tUp.meshId, dragIdx)
        && typeof updateUI === 'function') updateUI();
    dragIdx = -1;
    SMEngineBridge.resume();
    // The mesh lives in state.imageMeshes, not in the frame's strokes, so
    // there is nothing for saveActiveLayerFrame to write here — the undo
    // snapshot taken in onDown is what makes the edit revertible (see
    // layersSnapshotNow, tweens.js, which now carries imageMeshes), and
    // SMProject.isDirty() picks the change up on its own since it diffs a
    // fresh exportJSON() against the last saved one.
    dragMoved = false;
    SMEngineBridge.renderNow();
  }

  // ---- panel -----------------------------------------------------------
  function el(id) { return document.getElementById(id); }
  function renderImageMeshPanel() {
    var sec = el('p-imagemesh-sec');
    if (!sec) return;
    var r = singleRaster();
    // NOT `editing = false` here. This runs from updateUI, which fires on
    // every frame change — and loadFrame rebuilds the Paper items, so
    // singleRaster() is transiently null in the middle of an ordinary
    // scrub. Clearing the mode there turned edit mode off the instant the
    // playhead moved, which makes animating a mesh (key a vertex, scrub,
    // key it again) impossible. Found live doing exactly that. `editing`
    // is a sticky preference; with no valid target the overlay returns []
    // and the pointer handlers bail on their own, so leaving it on costs
    // nothing. It is cleared only when the mesh is genuinely gone (below).
    if (!r) { sec.style.display = 'none'; return; }
    sec.style.display = '';
    var has = !!(r.data && r.data.meshId && SMImageMesh.get(r.data.meshId));
    if (!has) editing = false;
    var onCb = el('p-imagemesh-on');
    if (onCb) onCb.checked = has;
    var editCb = el('p-imagemesh-edit');
    if (editCb) { editCb.checked = editing; editCb.disabled = !has; }
    var body = el('p-imagemesh-body');
    if (body) body.style.display = has ? '' : 'none';
    if (has) {
      var mesh = SMImageMesh.get(r.data.meshId);
      var cols = el('p-imagemesh-cols'), rows = el('p-imagemesh-rows');
      if (cols) cols.value = mesh.cols;
      if (rows) rows.value = mesh.rows;
      var info = el('p-imagemesh-info');
      if (info) info.textContent = mesh.verts.length + ' / ' + (mesh.tris.length / 3);
    }
  }
  window.renderImageMeshPanel = renderImageMeshPanel;

  function toggleMesh(on) {
    var r = singleRaster();
    if (!r) return;
    pushUndo();
    // attach/detach write BOTH the live Raster's data.meshId and every
    // frame's stored dict (SMImageMesh.propagate), so there is nothing left
    // for loadFrame to bring in — and calling it here was actively harmful:
    // it rebuilds every Paper item, which leaves selectedPaths pointing at
    // the removed Raster, so the very next click on this panel found no
    // valid target and silently did nothing while the checkbox had already
    // flipped. Found live, toggling the real checkbox twice in a row.
    if (on) SMImageMesh.attach(r, { cols: 4, rows: 4 });
    else { SMImageMesh.detach(r); editing = false; }
    renderImageMeshPanel();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  function setDensity() {
    var t = targetMesh();
    if (!t) return;
    var cols = Math.max(1, Math.min(32, parseInt(el('p-imagemesh-cols').value, 10) || 4));
    var rows = Math.max(1, Math.min(32, parseInt(el('p-imagemesh-rows').value, 10) || 4));
    pushUndo();
    t.mesh.cols = cols; t.mesh.rows = rows;
    // Retopology resets the pose — vertex indices are not stable across a
    // rebuild, so carrying the old offsets onto new vertices would scramble
    // the deformation (image-mesh.js's rebuild says the same thing).
    // Rebuilding from the CURRENT outline keeps the mask the user drew.
    SMImageMesh.setOutline(t.meshId, t.mesh.outline, { cols: cols, rows: rows });
    renderImageMeshPanel();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  function resetPose() {
    var t = targetMesh();
    if (!t) return;
    pushUndo();
    SMImageMesh.resetOffsets(t.meshId);
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }

  function init() {
    var onCb = el('p-imagemesh-on');
    if (onCb) onCb.addEventListener('change', function () { toggleMesh(this.checked); });
    var editCb = el('p-imagemesh-edit');
    if (editCb) editCb.addEventListener('change', function () { editing = this.checked; if (window.SMEngineBridge) SMEngineBridge.renderNow(); });
    var cols = el('p-imagemesh-cols'), rows = el('p-imagemesh-rows');
    if (cols) cols.addEventListener('change', setDensity);
    if (rows) rows.addEventListener('change', setDensity);
    var rst = el('btn-imagemesh-reset');
    if (rst) rst.addEventListener('click', resetPose);
    // Capture phase on DOCUMENT, not on #canvas-area.
    //
    // gradient-bridge/perspective-bridge/symmetry-bridge all listen on
    // #canvas-area, and that is enough for them because nothing else
    // competes for the same gesture. It is NOT enough here: motion.js also
    // has a capture-phase pointerdown on #canvas-area (its own canvas drag
    // that moves a layer), and among listeners on the SAME element capture
    // order is registration order — motion.js loads first, so it won.
    // Found live in Motion mode: dragging a mesh vertex silently moved the
    // whole layer instead, leaving ld.motionStatic.position at exactly the
    // drag delta and no key on the vertex.
    //
    // A capture listener on an ANCESTOR always runs before one on a
    // descendant, whatever the registration order — the same trick tweens.js
    // documents for its reassign-click intercept ("registered on document,
    // to steal a click before any per-tool bridge on #canvas-area sees it").
    // The target check keeps this scoped to the canvas exactly as before,
    // and every handler bails without stopping propagation unless it is
    // really taking the gesture.
    function inCanvas(e) {
      var area = document.getElementById('canvas-area');
      return !!(area && e.target && area.contains(e.target));
    }
    document.addEventListener('pointerdown', function (e) { if (inCanvas(e)) onDown(e); }, { capture: true });
    document.addEventListener('pointermove', function (e) { if (dragIdx >= 0) onMove(e); }, { capture: true });
    document.addEventListener('pointerup', function (e) { if (dragIdx >= 0) onUp(e); }, { capture: true });
    document.addEventListener('pointercancel', function (e) { if (dragIdx >= 0) onUp(e); }, { capture: true });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();

  // Exposed so the canvas right-click menu can offer the same two toggles the
  // panel does (2026-08-30, Cyril: "est-ce que ça serait dans élément et au
  // clic droit dans le canvas ?"). Deliberately NOT the whole module: these
  // four are what an outside caller needs to ask "is this offerable, is it on"
  // and to flip it. Everything routes through the same toggleMesh/`editing`
  // the checkboxes use, so the two entry points can never disagree — and
  // renderImageMeshPanel() keeps the checkboxes in step after a menu click.
  // ---- Mask an image with a shape you drew (2026-08-30) --------------
  // Cyril: "comment il fonctionnerait avec un bouton select dans le menu
  // flottant quand on a select une image ? Sur les outils de tracé ?"
  //
  // Answer, and why it is shaped this way: rather than teaching the mesh a
  // drawing mode of its own (which would have to re-implement the Pen, the
  // lasso, the primitives and freehand, one by one), you draw the mask with
  // the tools that already exist, as an ORDINARY shape, and then hand that
  // shape to the image. Every tracing tool works for free, including ones
  // added later, and you can refine the shape with the normal editing tools
  // before committing it.
  //
  // The result is not a second masking system: the shape becomes the mesh's
  // OUTLINE, which already IS the clip silhouette and the triangulation
  // boundary. So the mask stays live (no baking, unlike labs/clip-mask-bake
  // for vectors), its points stay draggable in mesh edit mode afterwards,
  // and because outline points are ordinary mesh vertices, the mask is
  // keyframable in Motion like anything else — an animated mask, for free.
  //
  // Selection is one shape + one image, the convention every app uses for
  // "clip this with that", and unambiguous about which masks which.
  function maskPair() {
    if (typeof Raster === 'undefined' || !window.selectedPaths || selectedPaths.length !== 2) return null;
    var a = selectedPaths[0], b = selectedPaths[1];
    var ras = (a instanceof Raster) ? a : (b instanceof Raster) ? b : null;
    var shape = (ras === a) ? b : a;
    if (!ras || !ras.parent || !shape || !(shape instanceof Path) || !shape.segments || shape.segments.length < 3) return null;
    return { raster: ras, shape: shape };
  }
  function maskImageWithShape() {
    var pair = maskPair();
    if (!pair) return;
    var rect = window.SMEngineBridge && SMEngineBridge.rasterImageRect
      ? SMEngineBridge.rasterImageRect(pair.raster) : null;
    if (!rect) return;
    // flatten() on a CLONE: the outline is a polygon (SMImageMesh triangulates
    // straight edges), so a curve has to be sampled — and the user's own shape
    // must not be modified in the process, since it may survive as art if the
    // caller ever chooses not to consume it.
    var tmp = pair.shape.clone({ insert: false });
    tmp.closed = true;
    tmp.flatten(2);
    var poly = tmp.segments.map(function (sg) {
      return SMImageMesh.normalizedOf(rect, sg.point.x, sg.point.y);
    });
    tmp.remove();
    if (poly.length < 3) return;
    pushUndo();
    var mid = pair.raster.data && pair.raster.data.meshId;
    if (!mid) { SMImageMesh.attach(pair.raster, { cols: 4, rows: 4 }); mid = pair.raster.data && pair.raster.data.meshId; }
    if (!mid) return;
    SMImageMesh.setOutline(mid, poly);
    // The shape has BECOME the mask, so leaving it on the canvas would draw
    // it twice: once as art, once as the silhouette. Removed through the
    // app's own delete path (companions, revision-ghosting, frame save) with
    // the selection narrowed to it, then the image is reselected so the mesh
    // panel stays on the thing you are now editing.
    selectedPaths = [pair.shape];
    if (window.SM && SM.deleteSelStrokes) SM.deleteSelStrokes();
    // Re-RESOLVE the image instead of keeping the pre-delete reference:
    // deleteSelStrokes saves the frame, which rebuilds the Paper items, so
    // the old object is detached by the time we get here. Holding it left
    // selectedPaths pointing at a parentless Raster — singleRaster() then
    // refused it (correctly, that is exactly the ghost its own comment
    // guards against) and the mesh panel went blank the instant the mask
    // landed. Found live, toggling Edit points right after masking. meshId
    // is the key because we just wrote it, and it survives the rebuild.
    var live = null, kids = userLayers[state.activeLayerIdx] && userLayers[state.activeLayerIdx].children;
    for (var i = 0; kids && i < kids.length; i++) {
      if (kids[i] instanceof Raster && kids[i].data && kids[i].data.meshId === mid) { live = kids[i]; break; }
    }
    selectedPaths = live ? [live] : [];
    if (window.updateUI) updateUI(true);
    renderImageMeshPanel();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    if (window.showToast) showToast(SM.t('toastImageMaskedWithShape'));
  }

  window.SMImageMeshUI = {
    canMesh: function () { return !!singleRaster(); },
    hasMesh: function () { var r = singleRaster(); return !!(r && r.data && r.data.meshId); },
    toggleMesh: function () { var r = singleRaster(); if (r) toggleMesh(!(r.data && r.data.meshId)); },
    isEditing: function () { return editing; },
    canMaskWithShape: function () { return !!maskPair(); },
    maskImageWithShape: maskImageWithShape,
    toggleEditing: function () {
      var r = singleRaster();
      if (!r || !(r.data && r.data.meshId)) return;
      editing = !editing;
      renderImageMeshPanel();
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
    },
  };
})();
