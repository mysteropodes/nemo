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
  var mode = null; // 'xform-scale' | 'xform-rotate' | 'marquee' | 'move' | 'arc' | 'nv-drag' | 'nv-scale' | 'nv-rotate' | 'null-drag' | 'cornerRadius' | null
  // Animation 2D hover-highlight box (2026-08, feedback: "roll hover n'existe
  // pas sur animation 2D" — Motion mode already has this, see SMMotion's own
  // _hoverLi/onHoverMove/hoverOverlayItems; this is the same idea for a plain
  // shape on the active/other layer). Only tracks the Paper Path itself —
  // the overlay rectangle is built in engine-bridge.js's buildTransformBoxItems
  // (same split as getMultiLayerBox: this module owns hit-testing/state,
  // engine-bridge.js owns turning it into scene-JSON draw items).
  var _hoverPathA2D = null;
  function onHoverMoveA2D(pt) {
    if (state.appMode === 'motion' || state.tool !== 'select' || state.playing) {
      var hadA2D = !!_hoverPathA2D; _hoverPathA2D = null; return hadA2D;
    }
    var hitA2D = null;
    // Active layer first, then every other visible/unlocked normal layer —
    // same order onDown's own click hit-test already uses a few hundred
    // lines down, so hover always finds exactly what a click would select.
    var activeLdA2D = state.layers[state.activeLayerIdx];
    var activeLayerA2D = userLayers[state.activeLayerIdx];
    if (activeLayerA2D && !(activeLdA2D && activeLdA2D.locked && !activeLdA2D.symbolId)) {
      hitA2D = activeLayerA2D.hitTest(pt, { stroke: true, fill: true, tolerance: 8 / view.zoom });
    }
    if (!hitA2D) {
      for (var pliA2D = project.layers.length - 1; pliA2D >= 0; pliA2D--) {
        var plA2D = project.layers[pliA2D];
        var oliA2D = userLayers.indexOf(plA2D);
        if (oliA2D < 0 || oliA2D === state.activeLayerIdx) continue;
        var ld2A2D = state.layers[oliA2D];
        if (!ld2A2D || ld2A2D.locked || !ld2A2D.visible || ld2A2D.symbolId) continue;
        var hA2D = plA2D.hitTest(pt, { stroke: true, fill: true, tolerance: 8 / view.zoom });
        if (hA2D) { hitA2D = hA2D; break; }
      }
    }
    var itemA2D = (hitA2D && hitA2D.item) ? hitA2D.item : null;
    // Don't highlight something already selected — same convention as
    // Motion's hoverOverlayItems (curSel.indexOf check), avoids a redundant
    // hover box drawn right on top of the real selection gizmo.
    if (itemA2D && selectedPaths.indexOf(itemA2D) >= 0) itemA2D = null;
    if (itemA2D === _hoverPathA2D) return false;
    _hoverPathA2D = itemA2D;
    return true;
  }
  // Oriented-box containment scan (2026-08, click-inside-the-hover-box
  // fallback — see the onDown call site's own comment for why this is a
  // fresh scan rather than trusting the last hover result). Topmost child
  // first, same z-order convention as Paper's own hitTest.
  function hitTestOrientedBoxA2D(pt, targetLayer) {
    // The boxes are built from RAW geometry (orientedBoxForPath), so the
    // world click has to come back into the layer's own space first
    // (2026-08-31). Without it, a layer under any Motion transform drew its
    // boxes in one place and tested clicks in another: measured with the
    // layer moved +300/-200, a click dead on the shape landed at world
    // (2269,142) against a box at [1820,301,2060,481] — no hit, so the drag
    // never started and the shape "could be selected but not moved".
    var liOB = userLayers.indexOf(targetLayer);
    if (liOB >= 0 && window.layerLocalPoint) pt = layerLocalPoint(liOB, pt);
    var kids = targetLayer.children;
    for (var i = kids.length - 1; i >= 0; i--) {
      var c = kids[i];
      if (!(c instanceof Path || c instanceof Raster)) continue;
      if (!isSelectablePathChild(c)) continue;
      var ob = orientedBoxForPath(c);
      if (!ob) continue;
      var lp = ob.angle ? pt.rotate(-ob.angle, ob.pivot) : pt;
      if (ob.b.contains(lp)) return c;
    }
    return null;
  }
  // Dynamic shapes phase 3 (2026-08-18) — canvas drag handles for a rect's
  // corner radius, Figma's own interaction (a small grip sitting on each
  // rounded corner's arc, draggable independently). Self-contained state,
  // deliberately NOT sharing computeHandles()/the 8-handle transform box's
  // hit-test — that system is already intricate (oriented boxes, Motion
  // point-maps, distort quads); a parallel, narrowly-scoped check kept this
  // additive instead of risking a regression in code this load-bearing.
  var _cornerDrag = null;
  // Dynamic shapes rework (2026-08-19) — feedback: "la mise en place dans
  // le canvas est pas fluide" turned out to mean Ellipse (pie/donut) and
  // Star/Polygon had NO canvas handles at all, only numeric fields —
  // Rectangle's own corner grips (phase 3, above) were the only kind ever
  // wired up. Reuses that exact hit-test/drag/render plumbing
  // (paramShapeSelectionSingle → *HandleWorldPositions → mode==='cornerRadius'
  // → *RadiusFromDrag, engine-bridge.js's orange-dot overlay) generalized to
  // all three kinds instead of adding a parallel system — same pattern,
  // wider `kind` coverage.
  function paramShapeSelectionSingle() {
    if (selectedPaths.length !== 1) return null;
    var p = selectedPaths[0];
    var k = p.data && p.data.paramShape && p.data.paramShape.kind;
    return (k === 'rect' || k === 'ellipse' || k === 'star') ? p : null;
  }
  // Each handle sits at the rounded corner's own arc MIDPOINT (not the
  // sharp geometric corner) — same visual language as Figma's grip. r=0
  // collapses the (1-k) term to 0, so the handle sits exactly on the sharp
  // corner point until you start dragging it outward, matching "grab the
  // corner to begin rounding it" as a natural gesture rather than needing
  // a pre-existing radius to have something to grab.
  function rectCornerHandleWorldPositions(p) {
    var ps = p.data.paramShape, b = p.bounds, k = 0.70710678;
    var tl = ps.tl || 0, tr = ps.tr || 0, br = ps.br || 0, bl = ps.bl || 0;
    return {
      tl: new Point(b.left + tl * (1 - k), b.top + tl * (1 - k)),
      tr: new Point(b.right - tr * (1 - k), b.top + tr * (1 - k)),
      br: new Point(b.right - br * (1 - k), b.bottom - br * (1 - k)),
      bl: new Point(b.left + bl * (1 - k), b.bottom - bl * (1 - k)),
    };
  }
  var CORNER_POINT = {
    tl: function (b) { return new Point(b.left, b.top); },
    tr: function (b) { return new Point(b.right, b.top); },
    br: function (b) { return new Point(b.right, b.bottom); },
    bl: function (b) { return new Point(b.left, b.bottom); },
  };
  // Radius tracks straight-line distance from the TRUE corner to the
  // pointer — simpler than inverting the arc-midpoint placement above, and
  // reads just as naturally under the hand (drag further from the corner
  // = bigger radius) without needing to match the handle's rest position
  // exactly during the drag itself.
  function cornerRadiusFromDrag(p, corner, pt) {
    var b = p.bounds;
    var maxR = Math.min(b.width, b.height) / 2;
    return Math.max(0, Math.min(maxR, pt.getDistance(CORNER_POINT[corner](b))));
  }
  // Ellipse pie/donut — same "box, not live bounds" source as
  // applyParamShapeEllipse (tools.js) now reads, so a thin pie slice's
  // handles sit where the FULL ellipse would be, not collapsed onto the
  // slice's own tiny rendered bbox.
  function ellipseCenterAndRadii(p) {
    var ps = p.data.paramShape;
    var b = (ps.box) || (function () { var pb = p.bounds; return { x1: pb.left, y1: pb.top, x2: pb.right, y2: pb.bottom }; })();
    return { cx: (b.x1 + b.x2) / 2, cy: (b.y1 + b.y2) / 2, rx: (b.x2 - b.x1) / 2, ry: (b.y2 - b.y1) / 2 };
  }
  function ellipseHandleWorldPositions(p) {
    var ps = p.data.paramShape;
    var g = ellipseCenterAndRadii(p);
    var startA = (ps.startAngle || 0) * Math.PI / 180;
    var endA = ((ps.startAngle || 0) + (ps.sweep !== undefined ? ps.sweep : 359.9)) * Math.PI / 180;
    // Inner-radius grip: always visible along the start-angle ray, at a
    // floor ratio when innerRadius is 0 — same "grab it to start rounding"
    // affordance as a rect's r=0 corner handle sitting exactly on the
    // sharp point until dragged.
    var ir = Math.max(ps.innerRadius || 0, 0.12);
    return {
      sweep: new Point(g.cx + g.rx * Math.cos(endA), g.cy + g.ry * Math.sin(endA)),
      inner: new Point(g.cx + g.rx * ir * Math.cos(startA), g.cy + g.ry * ir * Math.sin(startA)),
    };
  }
  function ellipseSweepFromDrag(p, pt) {
    var ps = p.data.paramShape, g = ellipseCenterAndRadii(p);
    // Un-warp an elliptical (rx != ry) drag point back to a circle before
    // taking its angle — dragging straight "around" a squashed ellipse
    // should still read as a clean angle, not skewed by the aspect ratio.
    var ux = (pt.x - g.cx) / Math.max(g.rx, 0.01), uy = (pt.y - g.cy) / Math.max(g.ry, 0.01);
    var angDeg = Math.atan2(uy, ux) * 180 / Math.PI;
    var sweep = angDeg - (ps.startAngle || 0);
    sweep = ((sweep % 360) + 360) % 360;
    if (sweep < 0.1) sweep = 0.1;
    return Math.min(359.9, sweep);
  }
  function ellipseInnerFromDrag(p, pt) {
    var g = ellipseCenterAndRadii(p);
    var ux = (pt.x - g.cx) / Math.max(g.rx, 0.01), uy = (pt.y - g.cy) / Math.max(g.ry, 0.01);
    var ratio = Math.sqrt(ux * ux + uy * uy);
    return Math.max(0, Math.min(0.95, ratio));
  }
  // Star/Polygon — inner-vertex grip only (outer radius already tracks the
  // shape's own resize handles like any layer; point count has no smooth
  // drag-continuous meaning, stays a numeric field per its own comment in
  // timeline.js's updateStarPanel).
  function starOuterRadius(p) {
    var ps = p.data.paramShape;
    var b = (ps.box) || (function () { var pb = p.bounds; return { x1: pb.left, y1: pb.top, x2: pb.right, y2: pb.bottom }; })();
    return { cx: (b.x1 + b.x2) / 2, cy: (b.y1 + b.y2) / 2, r: Math.min(b.x2 - b.x1, b.y2 - b.y1) / 2 };
  }
  function starHandleWorldPositions(p) {
    var ps = p.data.paramShape;
    var g = starOuterRadius(p);
    var n = Math.max(3, Math.round(ps.pointCount || 5));
    // First INNER vertex's angle in buildStarPolygonPath's own layout
    // (n=pointCount*2 verts, index 1, a=-PI/2 + i*2PI/(2n)).
    var a = -Math.PI / 2 + (2 * Math.PI) / (n * 2);
    var ir = ps.innerRatio !== undefined ? ps.innerRatio : 0.5;
    return { inner: new Point(g.cx + g.r * ir * Math.cos(a), g.cy + g.r * ir * Math.sin(a)) };
  }
  function starInnerFromDrag(p, pt) {
    var g = starOuterRadius(p);
    var ratio = pt.getDistance(new Point(g.cx, g.cy)) / Math.max(g.r, 0.01);
    return Math.max(0.05, Math.min(1, ratio));
  }
  // Unified dispatch — one map of {kind: {positions(p), valueFromDrag(p,handle,pt), commit(p,handle,val)}}
  // instead of a chain of if/else per kind at every call site.
  var PARAM_HANDLE_KINDS = {
    rect: {
      positions: rectCornerHandleWorldPositions,
      names: ['tl', 'tr', 'br', 'bl'],
      valueFromDrag: cornerRadiusFromDrag,
      commit: function (p, handle, val) { p.data.paramShape[handle] = val; window.applyParamShapeRect(p); },
    },
    ellipse: {
      positions: ellipseHandleWorldPositions,
      names: ['sweep', 'inner'],
      valueFromDrag: function (p, handle, pt) { return handle === 'sweep' ? ellipseSweepFromDrag(p, pt) : ellipseInnerFromDrag(p, pt); },
      commit: function (p, handle, val) {
        if (handle === 'sweep') p.data.paramShape.sweep = val; else p.data.paramShape.innerRadius = val;
        window.applyParamShapeEllipse(p);
      },
    },
    star: {
      positions: starHandleWorldPositions,
      names: ['inner'],
      valueFromDrag: starInnerFromDrag,
      commit: function (p, handle, val) { p.data.paramShape.innerRatio = val; window.applyParamShapeStar(p); },
    },
  };
  function paramHandleWorldPositions(p) {
    var kind = p.data.paramShape.kind;
    var def = PARAM_HANDLE_KINDS[kind];
    return def ? def.positions(p) : {};
  }
  window.SMParamShapeHandles = {
    cornerHandleWorldPositions: paramHandleWorldPositions,
    paramShapeSelectionSingle: paramShapeSelectionSingle,
    handleNamesFor: function (p) { var def = PARAM_HANDLE_KINDS[p.data.paramShape.kind]; return def ? def.names : []; },
  };
  // Non-destructive combine groups (2026-07-29) — hit-test confirmatory
  // guard. UNION never has a problem here: the combined visible region is
  // exactly the union of members' own real geometry, so "hit a real
  // member" and "hit something visible" always agree. subtract/intersect/
  // exclude don't: a click can land inside a member's UNMODIFIED geometry
  // that the combine visually cut away — a false-positive "select the
  // group" on what looks like empty canvas. Only runs (computeGroupCombine
  // is not cheap) when the raw hit actually resolved to a non-union
  // combine-group member — never per-frame, only per hit-test.
  function combineVisibleAt(item, pt, layerIdx) {
    var gid = item.data && item.data.groupId;
    if (!gid) return true;
    var ld = state.layers[layerIdx];
    if (!ld || !ld.groups || !ld.groups[gid]) return true;
    var grp = ld.groups[gid];
    if (!grp.combineMode || grp.combineMode === 'none' || grp.combineMode === 'unite') return true;
    if (!window.SMGroup) return true;
    var layer = userLayers[layerIdx];
    var members = SMGroup.resolveGroupMembers(gid, ld, layer);
    if (members.length < 2) return true;
    try {
      var islands = computeGroupCombine(members, grp.combineMode, layer);
      return islands.some(function (isl) { return isl.contains(pt); });
    } catch (e) { return true; }
  }
  function combineHitConfirm(hit, hitPt, layerIdx) {
    if (!hit || !hit.item) return true;
    return combineVisibleAt(hit.item, hitPt, layerIdx);
  }
  // Native-video footage gestures (2026-07, "une vidéo est un objet comme
  // les autres"): clicking a video layer SELECTS it (window._nvSelectedLayer,
  // read by engine-bridge's buildTransformBoxItems to draw the same
  // box+corners+ring gizmo paths get) — drag inside moves it, corner
  // handles scale (uniform), the ring rotates. All three write through
  // SMMotion.setLayerValue — static override when the property's stopwatch
  // is off, auto-keyframe at the playhead when it's on, exactly like
  // typing in the Transform panel fields. (Shift+drag's historical
  // scale-by-vertical-motion gesture is retired: corner handles replace it.)
  var nvIdx = -1, nvStartPt = null, nvStartPos = null, nvStartScale = null, nvScaleMode = false, nvMoved = false;
  var nvPivot = null, nvOrigDist = 1, nvStartAngle = 0, nvOrigRot = 0;
  var nullIdx = -1, nullStartPt = null, nullStartPos = null, nullMoved = false;
  function nvClearSelection() {
    if (window._nvSelectedLayer != null) { window._nvSelectedLayer = null; }
  }
  function syncMotionLayerSelection(li, additive) {
    if (typeof _layerSel === 'undefined') return;
    // Empty-canvas deselection is shared by Animation 2D and Motion now
    // that both modes can expose a layer-level multi-selection box.
    if (li == null) { if (!additive) _layerSel = []; return; }
    // 2026-08 fix (feedback: "j'avait effet de select, dans le canvas j'ai
    // select l'élément du layer 2, effet n'a pas totalement été deselect")
    // — this used to return here for Animation 2D, leaving _layerSel (and
    // therefore both the layer-list row's 'sel' class, timeline.js's
    // renderLayerList, AND multiLayerSelectionBox below) stuck on whatever
    // layer was selected BEFORE this canvas click. Picking a layer THROUGH
    // its own row (addLayer/addEffectLayer/the row's click handler) already
    // updates _layerSel directly — canvas clicks routing through here were
    // the one path that didn't, in either mode.
    if (additive) {
      if (_layerSel.indexOf(li) < 0) _layerSel.push(li);
    } else _layerSel = [li];
  }
  // Animation 2D counterpart of Motion's layer-selection box (feedback
  // #54). Keep this separate from selectedPaths: that array and
  // state.selectedStrokeIndices intentionally describe ONE active layer at
  // dozens of call sites. A dedicated layer-level selection preserves that
  // invariant while still making Cmd/Shift-selected timeline rows one
  // transformable canvas target.
  function multiLayerSelectionBox() {
    if (state.appMode === 'motion' || state.tool !== 'select' || typeof _layerSel === 'undefined' || _layerSel.length < 2) return null;
    var targets = [], left = Infinity, top = Infinity, right = -Infinity, bottom = -Infinity;
    _layerSel.forEach(function (li) {
      var ld = state.layers[li], layer = userLayers[li];
      if (!ld || !layer || ld.locked || !ld.visible || ld.symbolId || ld.nativeVideo || ld.isNullLayer || ld.isEffectLayer || ld.isGuideLayer || ld.isWidgetLayer) return;
      var paths = layer.children.filter(function (c) { return (c instanceof Path || c instanceof Raster) && isSelectablePathChild(c); });
      if (!paths.length) return;
      var b = layer.bounds;
      if (!b || !isFinite(b.width) || !isFinite(b.height)) return;
      left = Math.min(left, b.left); top = Math.min(top, b.top); right = Math.max(right, b.right); bottom = Math.max(bottom, b.bottom);
      targets.push({ li: li, layer: layer, paths: paths });
    });
    if (targets.length < 2 || left === Infinity) return null;
    var bounds = new Rectangle(new Point(left, top), new Point(right, bottom));
    var zs = 1 / Math.max(0.0001, view.zoom);
    return { targets: targets, bounds: bounds, pivot: bounds.center, ringRadius: Math.min(36 * zs, Math.max(bounds.width, bounds.height) * 0.3) };
  }
  function multiLayerHit(box, pt) {
    if (!box) return null;
    var zs = 1 / Math.max(0.0001, view.zoom), b = box.bounds;
    if (Math.abs(pt.getDistance(box.pivot) - box.ringRadius) < 7 * zs) return { type: 'rotate' };
    var corners = { nw: b.topLeft, ne: b.topRight, se: b.bottomRight, sw: b.bottomLeft };
    var hit = null;
    Object.keys(corners).forEach(function (k) { if (!hit && pt.getDistance(corners[k]) < 9 * zs) hit = { type: 'scale', dir: k, point: corners[k] }; });
    if (hit) return hit;
    return b.contains(pt) ? { type: 'move' } : null;
  }
  function forEachMultiPath(drag, fn) {
    drag.targets.forEach(function (target) { target.paths.forEach(function (p) { if (p && !p.removed) fn(p, target); }); });
  }
  function multiTranslatePath(p, delta) {
    p.translate(delta);
    if (window.syncParamShapeBoxOnTranslate) window.syncParamShapeBoxOnTranslate(p, delta.x, delta.y);
    transformFillGradient(p, function (gp) { return gp.add(delta); });
    if (p.data && p.data.isVectorBrush && p.data.centerSegments) p.data.centerSegments.forEach(function (s) { s.point = [s.point[0] + delta.x, s.point[1] + delta.y]; });
    if (p.data && p.data.linkedFill && !p.data.linkedFill.removed) p.data.linkedFill.translate(delta);
    if (p.data && p.data.brushCompanions) p.data.brushCompanions.forEach(function (c) { if (!c.removed) c.translate(delta); });
    if (p.data && p.data.xformAnchorCustom) p.data.xformAnchorCustom = [p.data.xformAnchorCustom[0] + delta.x, p.data.xformAnchorCustom[1] + delta.y];
  }
  var _multiLayerDrag = null;
  var xformDir = null, xformAnchor = null, xformOrigHandlePos = null, xformLastSx = 1, xformLastSy = 1;
  var xformMap = null; // geometry<->rendered-world mapper when the active layer has a Motion transform
  // Alt-held-during-scale-drag = scale from the box's own center instead of
  // the opposite corner/edge (Illustrator/Photoshop Free Transform
  // convention, 2026-09 — "Alt pour redimensionner depuis le centre").
  // xformCenterAnchor is captured once at gesture start (same geometry
  // space as xformAnchor/gCorners); xformActiveAnchor records which of the
  // two the LAST onMove tick actually scaled around, so onUp's text-field
  // resync (anchorTopLeft) uses the anchor that really applied instead of
  // always assuming the corner — toggling Alt mid-drag is legal (matches
  // Shift's own per-tick re-evaluation right below) and would otherwise
  // desync vector-text metadata from the geometry it just scaled.
  var xformCenterAnchor = null, xformActiveAnchor = null;
  // Skew-on-hover (2026-08, Graphite-style — user reference: the edge
  // midpoints of the transform box, past the plain resize handle's own
  // hit radius, grab a single-axis shear instead of a resize). Shear
  // composes ADDITIVELY for a fixed axis+anchor (unlike scale, which is
  // multiplicative) — xformLastShx/Shy track the cumulative shear applied
  // so far so onMove can apply just the per-tick DELTA, same incremental-
  // step pattern xformLastSx/Sy already use for scale.
  var xformSkewEdge = null, xformSkewPerp = 1, xformLastShx = 0, xformLastShy = 0;
  // Ctrl+drag corner = free-transform DISTORT pins (2026-07, "avec l'outil
  // de sélection si on fait ctrl il ne faudrait pas le menu droit mais des
  // pin de transformation libre pour modifier la sélection" — confirmed
  // with the user: Photoshop/AE-style corner-pin distort, the dragged
  // corner moves independently while the other 3 stay put, producing a
  // genuine perspective quad instead of a rectangle). Unlike xform-scale
  // (which mutates geometry incrementally, tick-relative-to-last-tick),
  // distort needs the ORIGINAL (pre-gesture) point positions every tick —
  // a projective map isn't composable step-by-step the way a uniform
  // scale factor is — so distortSegs snapshots every segment's point/
  // handleIn/handleOut once at gesture start and every subsequent tick
  // re-derives the full transform from that same fixed snapshot.
  var distortDir = null, distortSrcQuad = null, distortSegs = null;
  // Live quad during an active drag (2026-07 feedback: "la bounding box ne
  // reflete pas cette transformation") — engine-bridge.js's
  // buildTransformBoxItems reads this via SMSelectBridge.getDistortState()
  // to draw the ACTUAL warped quad instead of the static pre-distort
  // rectangle while dragging.
  var distortDstQuad = null;
  var rotCenter = null, rotStartAngle = 0, rotLastAngle = 0;
  var marqueeStart = null;
  var moveStarted = false;
  // Shift axis-lock (2026-09, Illustrator-style: hold Shift while dragging
  // an object to constrain it to horizontal/vertical). moveGestureOrigin is
  // the world point the CURRENT move gesture started from; moveAppliedTotal
  // is the running sum of every delta actually applied since then. Snapping
  // the TOTAL displacement (gestureOrigin -> current pointer) to an axis
  // each tick — rather than just zeroing a per-tick delta's off-axis
  // component — is what keeps the object exactly on the axis line even
  // though real hand movement never travels in a perfectly straight line;
  // zeroing only the per-tick component would let off-axis drift from
  // ticks before Shift was pressed (or from sub-pixel hand wobble) survive
  // uncorrected. Toggling Shift mid-drag is seamless: moveAppliedTotal is
  // kept up to date every tick regardless of Shift state, so re-engaging
  // the lock later in the same gesture still measures from the true origin.
  var moveGestureOrigin = null, moveAppliedTotal = null;
  // Set at pointerdown by a Shift-click on an already-selected item, applied
  // at pointerup only when the gesture turned out to be a click — see its
  // write site for the ambiguity this resolves (#802).
  var _pendingShiftToggle = null;
  // The grabbed point, in the LAYER's own space, tracked through the whole
  // move gesture (2026-08-31). Needed because a layer's Motion pivot is
  // userLayers[i].bounds.center — DERIVED FROM THE CONTENT — so translating
  // a shape moves the pivot, which rotates/scales the whole layer around a
  // new centre and lands the dragged shape somewhere other than under the
  // cursor. Measured with the layer at rotation 25°: a geometry delta of
  // (240,42) drifted the pivot by (154,21) and moved the shape (223,76) on
  // screen where (200,139) was asked for. Knowing where the grab point
  // WENT lets the move correct itself against where it should be.
  var moveGrabLocal = null;
  var draggingArc = null;
  var arcDragCache = null;
  var ANCHOR_MAP = { nw: 'se', ne: 'sw', sw: 'ne', se: 'nw', n: 's', s: 'n', e: 'w', w: 'e' };
  // Cursor feedback on handle hover (2026-07) — Figma/Illustrator/tldraw all
  // swap the cursor for the handle under the pointer BEFORE the user commits
  // to a drag, so the drag's effect is legible ahead of time. Nemo's canvas
  // cursor was previously a static per-tool value (see timeline.js's `cc`
  // map) with no per-handle feedback at all.
  var HANDLE_CURSORS = { nw: 'nwse-resize', se: 'nwse-resize', ne: 'nesw-resize', sw: 'nesw-resize', n: 'ns-resize', s: 'ns-resize', e: 'ew-resize', w: 'ew-resize' };
  // No native CSS rotate cursor exists — a small curved-arrow glyph, drawn
  // white-on-black for contrast against either a light or dark canvas
  // background, with the hotspot at its visual center (11,11 of a 22x22 svg).
  var ROTATE_CURSOR = "url(\"data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='22' height='22' viewBox='0 0 22 22'><path d='M4 11a7 7 0 1 1 2.1 5' fill='none' stroke='black' stroke-width='3.2' stroke-linecap='round'/><path d='M4 11a7 7 0 1 1 2.1 5' fill='none' stroke='white' stroke-width='1.6' stroke-linecap='round'/><path d='M3 15.5 4 11l4 2.2Z' fill='black'/><path d='M3.6 15 4.3 11.6l3 1.7Z' fill='white'/></svg>\") 11 11, grab";

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
    // World-space position of the anchor/pivot crosshair engine-bridge.js
    // already DRAWS (buildTransformBoxItems, "AE-style anchor point") —
    // mirrors that exact same custom-vs-preset mapping so the hit-test
    // below always agrees with what's actually rendered.
    var ap0 = (typeof xformAnchorPoint === 'function') ? xformAnchorPoint(b) : null;
    var anchorPos = ap0 ? (state.xformAnchorCustom ? ap0.clone() : WP(ap0.x, ap0.y)) : null;
    // Rotate RING (2026-07, replacing the old tiny offset stem+dot handle) —
    // live feedback: a single small grip above the box was easy to miss and
    // didn't read as "rotate" the way a full ring around the selection does
    // (Godot/Blender-style 2D gizmo). Centered on the anchor/pivot (so it
    // stays correct once the anchor's been moved off-center, not just the
    // box's own middle) — draggable from ANYWHERE along its circumference,
    // not one small fixed point. Small and mostly size-INDEPENDENT (a fixed
    // screen-space radius, like the corner handles' own 3.5*zs), per user
    // mockup: a small ring tucked near the pivot, not one that grows to
    // enclose the whole selection — the box/half-dimension-based radius
    // tried first was still "gigantesque" on anything but a small object.
    // Shrinks below that fixed size only for a genuinely small selection,
    // so it never grows past the corner-scale handles' own distance.
    var ringCenter = anchorPos || WP(b.center.x, b.center.y);
    var ringRadius = Math.min(36 * zs, Math.max(b.width, b.height) * 0.3);
    return { bounds: b, box: box, corners: corners, gCorners: gCorners, map: map, ringCenter: ringCenter, ringRadius: ringRadius, anchorPos: anchorPos };
  }

  // Shift+Alt-drag anchor snapping (2026-07, "ça pourrait snap sur les
  // corners du bounding box ?") — reuses the SAME 9-point preset grid the
  // Properties panel's anchor widget already offers (tl/tc/tr/ml/mc/mr/bl/
  // bc/br, tools.js XFORM_ANCHOR_PROP), not a separate corners-only special
  // case, so a Shift-snapped anchor behaves exactly like picking that same
  // preset from the panel — including rotating/scaling WITH the box
  // afterward, unlike a free (Alt-drag-without-Shift) custom anchor, which
  // is a fixed world point by design (see xformAnchorPoint's own comment).
  function presetAnchorWorldPoints(h) {
    function WP(x, y) { var g = selBoxPt(x, y, h.box); if (!h.map) return g; var w = h.map.fwd(g.x, g.y); return new Point(w[0], w[1]); }
    var pts = {};
    Object.keys(XFORM_ANCHOR_PROP).forEach(function (k) {
      var local = h.bounds[XFORM_ANCHOR_PROP[k]];
      pts[k] = WP(local.x, local.y);
    });
    return pts;
  }
  function nearestAnchorPresetKey(pt) {
    var h = computeHandles();
    if (!h) return null;
    var pts = presetAnchorWorldPoints(h);
    var bestK = null, bestD = Infinity;
    Object.keys(pts).forEach(function (k) {
      var d = pt.getDistance(pts[k]);
      if (d < bestD) { bestD = d; bestK = k; }
    });
    return bestK;
  }
  // Shared by both anchor-drag entry points (onDown's Alt+click-anywhere
  // and onMove's continued 'xform-anchor-drag') so Shift behaves
  // identically whether it was already held at mousedown or pressed mid-drag.
  // Also stamps the choice onto every selected stroke's own data (2026-07,
  // "la position du point d'ancrage n'est pas mise en mémoire si je
  // désélectionne et resélectionne l'élément") — state.xformAnchorKey/
  // Custom alone is session UI state, wiped by clearSel() on every new
  // selection; persisting per-stroke (serP/desP, same pattern as boxAngle)
  // lets it survive a deselect+reselect (restored in timeline.js's
  // updateSelPropsPanel). The actual saveActiveLayerFrame() happens once,
  // at onUp — cheap field writes here, no need to persist mid-drag.
  function placeAnchorAt(pt, snapToPreset) {
    if (snapToPreset) {
      var k = nearestAnchorPresetKey(pt);
      if (k) {
        state.xformAnchorKey = k; state.xformAnchorCustom = null;
        selectedPaths.forEach(function (p) { if (p && p.data) { p.data.xformAnchorKey = k; delete p.data.xformAnchorCustom; } });
        return;
      }
    }
    state.xformAnchorCustom = [pt.x, pt.y];
    selectedPaths.forEach(function (p) { if (p && p.data) { p.data.xformAnchorCustom = [pt.x, pt.y]; delete p.data.xformAnchorKey; } });
  }

  function hitTestHandles(pt, altHeld) {
    var h = computeHandles();
    if (!h) return null;
    // NOTE (2026-08-31): `pt` stays in WORLD space here on purpose.
    // computeHandles already maps its corners/ring/anchor through the
    // layer's Motion transform (its own `WP` helper), so both sides are
    // world-space and agree. Converting the point here — as
    // hitTestOrientedBoxA2D genuinely must, since orientedBoxForPath is
    // raw geometry — was tried and broke the drag outright: with a
    // selection live, the mismatched point matched a handle, onDown took
    // the handle branch and never reached the branch that sets mode
    // 'move', so a shape could be selected and then not moved at all.
    var tol = 9 / view.zoom;
    // Anchor crosshair — checked FIRST/exclusively, but ONLY while Alt is
    // held (live feedback 2026-07: "ça peut être confusant quand il faut
    // déplacer un petit élément" — a small object's own body can fall
    // within the anchor's hit tolerance, so an unconditional grab there
    // silently moved the PIVOT instead of the object with no way to tell
    // which one just happened). Without Alt, a click in that same spot now
    // falls through to the normal move/marquee logic below — Alt+drag is
    // otherwise free on the Select tool (viewtools-bridge.js's global
    // Alt-drag-rotate never reaches here anyway: this file's onDown always
    // stopImmediatePropagation()s first while the Select tool is active),
    // so repurposing it for "grab the anchor" doesn't collide with
    // anything. A default (center) anchor sits nowhere near a resize
    // handle so this never shadows them in the common case; when a preset
    // corner anchor DOES coincide with its own resize handle, grabbing the
    // anchor (Alt held) is what the user is more likely reaching for right
    // there, so it still wins that specific tie.
    if (h.anchorPos && altHeld) {
      var dAnchor = pt.getDistance(h.anchorPos);
      if (dAnchor < tol) return { type: 'anchor' };
    }
    // Ring band test — anywhere within ~7px of the circumference counts,
    // not just a single point, checked before the corners since the 16px
    // margin baked into ringRadius already keeps it clear of them.
    var ringTol = 7 / view.zoom;
    if (Math.abs(pt.getDistance(h.ringCenter) - h.ringRadius) < ringTol) return { type: 'rotate' };
    var bestD = tol, best = null;
    Object.keys(h.corners).forEach(function (k) {
      var d = pt.getDistance(h.corners[k]);
      if (d < bestD) { bestD = d; best = { type: 'scale', dir: k }; }
    });
    if (best) return best;
    // Skew zones (2026-08) — checked only once no corner/edge scale handle
    // matched, so a resize handle always wins a tie. Two small hit-zones
    // flank each edge's midpoint handle, running ALONG that edge, starting
    // just past the scale handle's own tolerance (SKEW_INNER) so the two
    // never overlap. Mirrors buildTransformBoxItems' (engine-bridge.js) own
    // tick-mark geometry exactly — the two must agree, or a mark could be
    // drawn somewhere a click here won't recognize.
    var skewHit = skewZoneHitTest(h, pt);
    if (skewHit) return skewHit;
    return null;
  }
  var SKEW_MIN_EDGE_PX = 48, SKEW_INNER_PX = 9, SKEW_OUTER_PX = 20;
  var EDGE_ENDPOINTS = { n: ['nw', 'ne'], s: ['sw', 'se'], w: ['nw', 'sw'], e: ['ne', 'se'] };
  function skewZoneHitTest(h, pt) {
    var zs = 1 / view.zoom;
    var found = null;
    Object.keys(EDGE_ENDPOINTS).forEach(function (k) {
      if (found) return;
      var ep = EDGE_ENDPOINTS[k];
      var a = h.corners[ep[0]], b2 = h.corners[ep[1]];
      var edgeVec = b2.subtract(a);
      var edgeLenPx = edgeVec.length * view.zoom;
      if (edgeLenPx < SKEW_MIN_EDGE_PX) return;
      var dir = edgeVec.normalize();
      var mid = h.corners[k];
      [-1, 1].forEach(function (side) {
        if (found) return;
        var base = mid.add(dir.multiply(side * (SKEW_INNER_PX + SKEW_OUTER_PX) / 2 * zs));
        var ext = pt.subtract(base);
        var along = ext.dot(dir);
        var perp = ext.subtract(dir.multiply(along)).length;
        if (Math.abs(along) <= (SKEW_OUTER_PX - SKEW_INNER_PX) / 2 * zs && perp <= SKEW_INNER_PX * zs) {
          found = { type: 'skew', edge: k };
        }
      });
    });
    return found;
  }

  // ---- Free-transform distort (Ctrl+drag a corner pin) ----
  // Inverts P = nw + u*ex + v*ey for (u,v) — valid because the SOURCE box
  // is always a plain (possibly rotated) rectangle, i.e. an affine image of
  // the unit square, never itself distorted yet.
  function rectUVSolver(nw, ne, sw) {
    var exx = ne.x - nw.x, exy = ne.y - nw.y;
    var eyx = sw.x - nw.x, eyy = sw.y - nw.y;
    var det = exx * eyy - eyx * exy;
    if (Math.abs(det) < 1e-9) det = det < 0 ? -1e-9 : 1e-9;
    return function (x, y) {
      var px = x - nw.x, py = y - nw.y;
      return { u: (px * eyy - py * eyx) / det, v: (exx * py - exy * px) / det };
    };
  }
  // Classic Heckbert (1989) unit-square -> general-quad projective mapping:
  // (0,0)->p0, (1,0)->p1, (1,1)->p2, (0,1)->p3. The destination corner the
  // user is dragging can make this quad genuinely non-planar/non-convex-
  // rectangular (that's the whole point of a distort), so this is a real
  // perspective divide, not a bilinear/affine shortcut.
  function unitSquareToQuad(p0, p1, p2, p3) {
    var x0 = p0.x, y0 = p0.y, x1 = p1.x, y1 = p1.y, x2 = p2.x, y2 = p2.y, x3 = p3.x, y3 = p3.y;
    var dx1 = x1 - x2, dx2 = x3 - x2, dx3 = x0 - x1 + x2 - x3;
    var dy1 = y1 - y2, dy2 = y3 - y2, dy3 = y0 - y1 + y2 - y3;
    var a, b, c, d, e, f, g, hh;
    if (Math.abs(dx3) < 1e-9 && Math.abs(dy3) < 1e-9) {
      a = x1 - x0; b = x2 - x1; c = x0;
      d = y1 - y0; e = y2 - y1; f = y0;
      g = 0; hh = 0;
    } else {
      var den = dx1 * dy2 - dx2 * dy1;
      if (Math.abs(den) < 1e-9) den = den < 0 ? -1e-9 : 1e-9;
      g = (dx3 * dy2 - dx2 * dy3) / den;
      hh = (dx1 * dy3 - dx3 * dy1) / den;
      a = x1 - x0 + g * x1; b = x3 - x0 + hh * x3; c = x0;
      d = y1 - y0 + g * y1; e = y3 - y0 + hh * y3; f = y0;
    }
    return function (u, v) {
      var den2 = g * u + hh * v + 1;
      if (Math.abs(den2) < 1e-9) den2 = den2 < 0 ? -1e-9 : 1e-9;
      return { x: (a * u + b * v + c) / den2, y: (d * u + e * v + f) / den2 };
    };
  }
  // Snapshot every segment's point/handleIn/handleOut ONCE at gesture start
  // (recurses into CompoundPath children — see CLAUDE.md §1, a boolean
  // result or an erase can leave a CompoundPath in the selection and it is
  // NOT a Path subclass, so `.segments` isn't there directly).
  function collectDistortSegs(item, out) {
    if (item.segments) {
      item.segments.forEach(function (seg) {
        out.push({
          seg: seg, pt: [seg.point.x, seg.point.y],
          hi: seg.handleIn ? [seg.handleIn.x, seg.handleIn.y] : null,
          ho: seg.handleOut ? [seg.handleOut.x, seg.handleOut.y] : null,
        });
      });
    } else if (item.children) {
      item.children.forEach(function (c) { collectDistortSegs(c, out); });
    }
  }
  // Only a corner pin on a plain (non-Motion, non-Component) selection is
  // eligible — a Component instance's whole-rigid-body placement folds into
  // symMatrix (symGestureAccumulate), an affine-only accumulator that can't
  // represent a perspective distort, and Motion mode never touches raw
  // geometry at all (see the 'move'/'xform-scale' Motion-mode early-returns
  // elsewhere in this file) — both are out of scope for this gesture rather
  // than silently producing a wrong result.
  function distortEligibleCornerAt(pt) {
    if (!selectedPaths.length || state.appMode === 'motion') return null;
    var ld = state.layers[state.activeLayerIdx];
    if (ld && ld.symbolId) return null;
    var hh = hitTestHandles(pt, false);
    if (hh && hh.type === 'scale' && (hh.dir === 'nw' || hh.dir === 'ne' || hh.dir === 'sw' || hh.dir === 'se')) return hh.dir;
    return null;
  }
  function beginDistort(dir) {
    pushUndo();
    ensureKeyframe();
    selectedPaths = state.selectedStrokeIndices.map(function (i) { return userLayers[state.activeLayerIdx].children[i]; }).filter(Boolean);
    var h = computeHandles();
    if (!h) return false;
    distortDir = dir;
    distortSrcQuad = { nw: h.gCorners.nw.clone(), ne: h.gCorners.ne.clone(), se: h.gCorners.se.clone(), sw: h.gCorners.sw.clone() };
    distortDstQuad = distortSrcQuad; // no drag yet — starts equal to the source quad
    xformMap = h.map;
    distortSegs = [];
    selectedPaths.forEach(function (p) { collectDistortSegs(p, distortSegs); });
    mode = 'xform-distort';
    window.SMEngineBridge.renderNow();
    return true;
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
    // Right/middle-click never drives select/marquee/move/Motion-drag — a
    // pre-existing gap (no button check at all) that used to go unnoticed
    // since a right-click did nothing visible either way; surfaced once
    // onContext (below) gave right-click a real, DIFFERENT job. Without
    // this, a right-click landing just off the shape would run the normal
    // click-miss path (clearSel() + start a marquee) a split second before
    // 'contextmenu' fires, wiping the very selection the context menu was
    // about to act on. Left as a no-op here (no stopPropagation) so
    // 'contextmenu' fires completely normally afterward.
    if (e.button !== undefined && e.button !== 0) return;
    // One-shot guard (2026-08 fix, "je select une forme dans le groupe...
    // si j'essaie de bouger la forme dans le canvas alors ça select le
    // groupe") — set by motion.js's selectShapesByStrokeIds (Elements panel
    // click) right before this mousedown, consumed here regardless of which
    // branch below ends up handling the click so it only ever protects the
    // ONE gesture immediately following a panel pick. See its own comment
    // for why selectedPaths alone can't distinguish "deliberately narrowed
    // via the panel" from "Subselect left one member selected", the other
    // case the two group-widening sites below (bodyHandle shortcut, idx2>=0
    // click) exist for.
    var skipGroupWiden = !!window._skipGroupWidenOnce;
    window._skipGroupWidenOnce = false;
    // Motion mode's position-keyframe/spatial-handle canvas dragging
    // (motion.js's onDown/onDrag/onUp — the bezier-handle motion path,
    // same gizmo pattern as the camera layer) was originally wired ONLY
    // into tools.js's own Paper-Tool onMouseDown/Drag/Up — dead code the
    // moment the Rust engine is on (the default): this file's own onDown
    // stopImmediatePropagation()s at CAPTURE phase, which never lets
    // Paper's Tool system (and therefore tools.js's handler) see the event
    // at all. Found live (2026-07, "ajoute des bezier de controle comme
    // pour le calque caméra au motion path de position" — the feature
    // already existed, just never actually reachable). Checked first, tool-
    // agnostic like tools.js's own placement of this same check — only
    // consumes the event (returns true) when the click actually lands on a
    // motion handle/keyframe dot; otherwise falls through unchanged into
    // this file's own shouldIntercept()-gated logic below.
    if (state.appMode === 'motion' && window.SMMotion) {
      var w0 = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
      if (SMMotion.onDown({ point: new Point(w0[0], w0[1]), altKey: e.altKey })) {
        e.stopImmediatePropagation(); e.preventDefault();
        return;
      }
      // Leaving per-element isolation (2026-08-30: "si je clic ailleurs ça ne
      // sors pas et reviens pas en box normal"). Animation 2D has always had
      // this — its own _shapeEnteredId reset a few hundred lines down — but
      // the Motion equivalent was never written, so once a double-click had
      // isolated a shape there was NO way back to the layer box short of
      // switching modes.
      //
      // Placed right after SMMotion.onDown declined the gesture: anything it
      // took (a handle, the ring, the anchor, or the element body itself) is
      // work INSIDE the isolation and must not cancel it. What's left here is
      // a click that hit none of that — empty canvas or another shape — which
      // is exactly the "clic ailleurs" that should drop back out.
      // The `_perObjBoxes` half of the condition is what covers entering the
      // group WITHOUT targeting a shape (2026-08-31, feedback #176): that
      // state leaves _motionExpandedElement null, and Motion cannot fall
      // through to the Animation 2D reset further down because
      // shouldIntercept() is false here — Motion's active tool is 'draw', not
      // 'select'. So without this the Motion side could be entered and never
      // left. Found by driving; reading the two branches side by side does
      // not show it, since each looks complete on its own.
      if (window._motionExpandedElement != null || window._perObjBoxes === state.activeLayerIdx) {
        var lyrEx = userLayers[state.activeLayerIdx];
        var ptEx = new Point(w0[0], w0[1]);
        // Hit-test in the LAYER's own space (2026-08-31): its geometry never
        // moves, the Motion transform is applied at render, so testing the
        // raw world point misses every shape as soon as the layer is
        // animated. perObjectUnionBounds below is already world-space and
        // keeps ptEx. Measured with the layer moved/rotated/scaled: clicking
        // a shape dead-centre targeted nothing.
        // hitTestPosed undoes the layer's transform AND each element's own,
        // per element — a plain layer.hitTest tests one point against
        // geometry that several independent transforms have moved, so once
        // two elements had been dragged apart inside the group, clicking
        // either of them at its visible position matched nothing.
        var hitEx = (lyrEx && window.hitTestPosed) ? hitTestPosed(state.activeLayerIdx, ptEx, 6 / view.zoom) : null;
        var itEx = hitEx && hitEx.item;
        while (itEx && itEx.parent && itEx.parent !== lyrEx) itEx = itEx.parent;
        // Route through resolveBrushAnchor (tools.js) before reading strokeId
        // (2026-08-31 follow-up) — a vector brush's linked-fill companion
        // (data.isLinkedFillCompanion) is a separate live Paper item with its
        // OWN strokeId, and topmost z-order sometimes puts it in front of the
        // anchor at the exact pixel clicked (same "des fois ça sélectionne le
        // tout des fois juste le fill" inconsistency resolveBrushAnchor was
        // already written to fix for every OTHER click path — this isolation
        // block had its own separate itEx.data.strokeId read that never went
        // through it). Per-element Motion is only ever recorded under the
        // ANCHOR's strokeId (motion.js's layerElements folds the companion
        // out of the Elements list) — without this, a click that happened to
        // land on the companion created a SECOND, independent
        // ld.elementMotion entry keyed by the companion's own strokeId, and
        // now that a click-drag actually moves things (the fix above this
        // block), dragging it split the fill away from its own outline.
        // Found live: after two drags the companion's motionStatic.position
        // was [-96.9,192.1] while the anchor's was [-101.5,338.8] — same
        // stroke, two different places, reading as two separate wavy shapes.
        itEx = (window.resolveBrushAnchor && lyrEx) ? resolveBrushAnchor(itEx, lyrEx) : itEx;
        var sidEx = itEx && itEx.data && itEx.data.strokeId;
        // Clicking a SIBLING hands the isolation over to it rather than
        // ending it — same continuity Animation 2D's per-object mode has, and
        // the thing that makes a multi-shape layer workable. Clicking empty
        // space still INSIDE the group box drops back to "all boxes, none
        // targeted" (you are still in the group, Illustrator-style); only a
        // click outside the box leaves it.
        var sortiDuGroupe = false;
        if (sidEx && sidEx !== window._motionExpandedElement) {
          window._motionExpandedElement = sidEx;
          window._perObjBoxes = state.activeLayerIdx;
          if (SMMotion.selectShapesByStrokeIds) SMMotion.selectShapesByStrokeIds(state.activeLayerIdx, [sidEx]);
          // Re-run onDown now that the target has actually switched to the
          // sibling under the cursor (2026-08-31 follow-up, "je déplace un
          // éléments et que dans le même groupe je déplace un autre élément
          // l'autre éléments ne déplace que la box et pas la shape"). The
          // SMMotion.onDown call above (this file's own top-of-onDown, a few
          // lines up) ran BEFORE _motionExpandedElement was updated, so its
          // body-hit-test always compared the click against the OLD target's
          // box — a click on a sibling is never inside that box, so it fell
          // through to plain re-targeting every time and this whole block
          // just consumed the event. A click-and-drag done as one continuous
          // gesture (mousedown then move without releasing — the normal way
          // to select-and-drag in any design tool) therefore relocated the
          // box to the sibling's real, UNMOVED position and then dragged
          // nothing: no _motionDrag was ever created, so the following
          // pointermoves had nothing to act on and setValue was never
          // called — confirmed live, ld.elementMotion[sidEx] stayed `{}`
          // after a full press-drag-release. One retry with the NOW-current
          // target lets onDown's own body-hit-test (bottom of the function)
          // match this same click and start 'elementMove' properly, so the
          // single gesture both selects AND drags the sibling, like every
          // other grab in this file already does for the FIRST element.
          SMMotion.onDown({ point: ptEx, altKey: e.altKey });
        } else if (sidEx === window._motionExpandedElement) {
          // feedback #216 investigation (2026-08-31): re-clicking the SAME
          // shape that is already the isolation target hit neither branch
          // above (not a sibling to retarget to, not empty space to exit
          // through) — sortiDuGroupe stayed false and the click below got
          // swallowed for nothing: no retarget happened, no exit happened,
          // the gesture just vanished. That silently blocked every OTHER
          // canvas tool (Subselect vertex-dragging chief among them, but
          // also a plain Select-tool re-drag) from ever reaching its own
          // hit-test once a single element had been isolated via the
          // Elements panel (selectShapesByStrokeIds sets
          // _motionExpandedElement) — the exact state a user is in right
          // after arming a shape's "Path" vertex Motion group, whose row
          // only appears for an isolated single element. Nothing to change
          // about the isolation itself here; just let a click this block
          // has no work for fall through instead of eating it.
          sortiDuGroupe = true;
        } else if (!sidEx) {
          var uEx = (lyrEx && window.perObjectUnionBounds) ? perObjectUnionBounds(lyrEx) : null;
          window._motionExpandedElement = null;
          if (!uEx || !uEx.contains(ptEx)) {
            window._perObjBoxes = null;
            sortiDuGroupe = true;
            // _motionExpandedLayer is a THIRD flag (tools.js's double-click
            // group entry sets it alongside _perObjBoxes, motion.js:5409) —
            // activeMotionTarget() prefers it over state.activeLayerIdx
            // whenever it's non-null, so leaving it behind here meant the
            // canvas gizmo kept drawing THIS layer's box forever after
            // exiting, even once a different layer had genuinely become
            // active (row highlight, Layer Properties panel, selectedPaths
            // all correctly showed the new layer — only the canvas box and
            // the timeline's own row highlight, which reads this same flag,
            // stayed stuck). Found live: exit Layer 2's group, click Layer
            // 1's shape — state.activeLayerIdx became 0 right away, but
            // window._motionExpandedLayer stayed 1, so the gizmo kept
            // hugging Layer 2 no matter what got selected afterward. Scoped
            // to the layer actually being exited, matching _perObjBoxes'
            // own scoping just above.
            if (window._motionExpandedLayer === state.activeLayerIdx) window._motionExpandedLayer = null;
          }
        }
        window._sceneVersion = (window._sceneVersion || 0) + 1;
        if (window.renderLayerList) renderLayerList();
        if (window.renderTimeline) renderTimeline();
        window.SMEngineBridge.renderNow();
        // Leaving the group must NOT eat the click (2026-08-31, feedback en
        // Motion: "je ressort en cliquant ailleurs ... j'essaye de clic sur
        // l'autre groupe dans l'autre layer alors il select le premier
        // groupe"). Exiting is a side effect of clicking somewhere else, not
        // the point of the gesture: the click still means "select whatever
        // is here", which may well be a shape on another layer. Consuming it
        // forced a second click, and the first one appeared to do nothing —
        // or worse, left the old group selected. Every other outcome of this
        // block (targeting a sibling, dropping the target while staying in
        // the group) IS the whole gesture, so those still consume it.
        if (!sortiDuGroupe) { e.stopImmediatePropagation(); e.preventDefault(); return; }
      }
    }
    if (!shouldIntercept()) return;
    // Leave the entered group when the click lands OUTSIDE its box
    // (2026-08-31, feedback #176: "si je clic en dehors alors on revient sur
    // la premiere bounding box de groupe"). Keyed on _perObjBoxes, not on
    // _shapeEnteredId: since the double-click can now enter the group
    // WITHOUT targeting a shape, _shapeEnteredId is null in that state and
    // the older reset — which required it — could never fire, so the mode
    // had no way out at all. Found by driving, not by reading.
    // A click still INSIDE the union keeps the mode: that is how you pick
    // one element after another without re-entering each time.
    if (window._perObjBoxes === state.activeLayerIdx && window.perObjectUnionBounds) {
      var _pw = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
      var _pu = perObjectUnionBounds(userLayers[state.activeLayerIdx]);
      if (!_pu || !_pu.contains(new Point(_pw[0], _pw[1]))) {
        window._perObjBoxes = null;
        window._shapeEnteredId = null;
        window._sceneVersion = (window._sceneVersion || 0) + 1;
      }
    }
    // Every gesture starts from a clean slate (2026-07-26). onDown has a
    // dozen branches and several of them `return` without ever assigning
    // `mode`, so the PREVIOUS gesture's mode could survive into the new one
    // and onMove would act on it. Defensive, not a reported bug: the only
    // reproduction found was through synthetic pointer+mouse events firing
    // the same gesture into two handlers at once, which real input never
    // does — re-checked against real drags and it does NOT occur. Kept
    // anyway because a branch should have to opt IN to a mode rather than
    // every branch having to remember to opt out of the last one.
    mode = null;
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var pt = new Point(w[0], w[1]);
    lastPt = pt;
    window.SMEngineBridge.suspend();

    var mlBox = multiLayerSelectionBox();
    var mlHit = multiLayerHit(mlBox, pt);
    if (mlBox && mlHit) {
      // Materialize held frames for every participating layer before taking
      // object references; _insertKeyframeCore/loadFrame rebuilds all Paper
      // items and would otherwise leave the snapshot stale.
      saveAllLayerFrames(); pushUndoLayers(true);
      var promoted = false;
      mlBox.targets.forEach(function (target) {
        var f = state.layers[target.li].frames[state.currentFrame];
        if (f && !f.isKeyframe && !f.isInterpolated) { _insertKeyframeCore(target.li, state.currentFrame); promoted = true; }
      });
      if (promoted) loadFrame(state.currentFrame);
      mlBox = multiLayerSelectionBox();
      if (!mlBox) { window.SMEngineBridge.resume(); return; }
      mode = 'layer-multi-' + mlHit.type;
      _multiLayerDrag = { targets: mlBox.targets, pivot: mlBox.pivot, start: pt.clone(), last: pt.clone(), lastSx: 1, lastSy: 1, lastAngle: 0 };
      if (mlHit.type === 'scale') {
        _multiLayerDrag.dir = mlHit.dir;
        _multiLayerDrag.orig = mlHit.point.subtract(mlBox.pivot);
      } else if (mlHit.type === 'rotate') {
        _multiLayerDrag.startAngle = Math.atan2(pt.y - mlBox.pivot.y, pt.x - mlBox.pivot.x) * 180 / Math.PI;
      }
      window.SMEngineBridge.renderNow();
      return;
    }

    var pshp0 = paramShapeSelectionSingle();
    if (pshp0) {
      var hp0 = paramHandleWorldPositions(pshp0);
      var HIT_R0 = 10 / view.zoom, hitCorner0 = null;
      window.SMParamShapeHandles.handleNamesFor(pshp0).forEach(function (c) { if (!hitCorner0 && pt.getDistance(hp0[c]) < HIT_R0) hitCorner0 = c; });
      if (hitCorner0) {
        mode = 'cornerRadius';
        pushUndo();
        _cornerDrag = { path: pshp0, corner: hitCorner0 };
        return;
      }
    }
    var ah = hitTestArc(pt);
    if (ah) {
      mode = 'arc';
      // One undo entry per arc-handle GESTURE (2026-09 QA sweep): dragging
      // a motion-arc handle rewrote state.motionArcs with no snapshot at
      // all, so Cmd+Z jumped straight past it to the previous action and
      // the bent trajectory stayed bent. Pushed at grab time, before the
      // first setArcHandle, like every other drag in this file.
      if (typeof pushUndo === 'function') pushUndo();
      draggingArc = ah;
      // Perf fix 2026-07: compute the (expensive, O(n³) autoMatch) stroke
      // pairing ONCE here instead of on every pointermove — only the
      // dragged handle's own position changes during the drag, matches()
      // is re-decided fresh on drag-end anyway (generateTweens(), onUp).
      arcDragCache = computeArcMatchState();
      return;
    }

    // Motion mode has its own COMPLETE, parallel box/ring/anchor hit-test
    // (motion.js's hitMotionBoxHandle/hitAnchorPoint, tried first via
    // SMMotion.onDown above) — this one is Animation 2D's, and its own
    // overlay is already hidden in Motion mode (buildTransformBoxItems,
    // engine-bridge.js, 2026-08-21). Without this gate the two systems'
    // hit-zones could still overlap invisibly: SMMotion.onDown returning
    // false (a Motion click that missed every Motion-specific target)
    // let a hidden Animation 2D handle still grab the click underneath —
    // confirmed live as the direct trigger for a real crash a few lines
    // down (this block's own 2026-08-21 fix comment).
    var hh = (state.appMode === 'motion') ? null : hitTestHandles(pt, e.altKey);
    if (hh && hh.type === 'anchor') {
      // Direct drag of the anchor crosshair — same UI-preference-not-
      // document-edit reasoning as the Alt+click path a bit further down
      // (no pushUndo, no ensureKeyframe): only state.xformAnchorCustom
      // changes, geometry is untouched.
      mode = 'xform-anchor-drag';
      return;
    }
    if (hh && e.ctrlKey && hh.type === 'scale' && (hh.dir === 'nw' || hh.dir === 'ne' || hh.dir === 'sw' || hh.dir === 'se') && distortEligibleCornerAt(pt) === hh.dir) {
      // If beginDistort() fails (computeHandles() returned null), `mode`
      // stays null — onUp()'s `if (!mode) return;` would then skip the
      // resume() at its tail entirely, leaving the engine suspended forever
      // (suspend() already ran a few lines above, unconditionally).
      if (!beginDistort(hh.dir)) window.SMEngineBridge.resume();
      return;
    }
    if (hh) {
      pushUndo();
      // ensureKeyframe() BEFORE any geometry mutation — live feedback
      // 2026-07: "si on est sur une frame d'une keyframe prolongée et que
      // l'on déplace un objet celui-ci revient en place". Root cause:
      // saveActiveLayerFrame() (app.js) is a hard no-op on a plain held
      // frame (`if(!f.isKeyframe&&!f.isInterpolated)return;`) — the drag
      // visibly moved the LIVE Paper geometry the whole time, but nothing
      // ever persisted it, so the very next loadFrame() (any frame nav,
      // even just scrubbing away and back) silently rebuilt the object at
      // its old inherited position. draw-bridge.js and every other bridge
      // already call this before their own first edit; select-bridge.js's
      // transform gestures never did. ensureKeyframe() calls loadFrame()
      // internally when it actually promotes the frame, which rebuilds
      // EVERY Paper item fresh — selectedPaths' object references go stale
      // the instant that happens, so it must be re-hydrated from
      // state.selectedStrokeIndices (index-based, survives the rebuild
      // since loadFrame() reconstructs children in the same stroke order)
      // before anything below reads/mutates selectedPaths. Skipped in
      // Motion mode — same reasoning as the 'move' grab below: this
      // gesture never touches ld.frames content.
      // 2026-08-21 fix ("Cannot read properties of null (reading
      // 'gCorners')"): the re-hydration line right below is ONLY needed
      // to recover from ensureKeyframe()'s object rebuild just above —
      // pointless (and actively harmful) when that call didn't run. Motion
      // mode never populates state.selectedStrokeIndices the way
      // Animation 2D does (selectLayerFromGrid sets _layerSel instead,
      // motion.js) — mapping over it here silently emptied a perfectly
      // valid Motion selectedPaths (2 elements from a Component) down to
      // [], so computeHandles() on the very next line returned null and
      // h.gCorners/h.map crashed a few lines down. Confirmed live: any
      // scale/rotate-handle grab in Motion mode reliably crashed here.
      if (state.appMode !== 'motion') {
        ensureKeyframe();
        selectedPaths = state.selectedStrokeIndices.map(function (i) { return userLayers[state.activeLayerIdx].children[i]; }).filter(Boolean);
      }
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
      } else if (hh.type === 'skew') {
        mode = 'xform-skew';
        xformSkewEdge = hh.edge;
        // Same opposite-edge anchor convention as scale's ANCHOR_MAP (the
        // edge you DIDN'T grab stays fixed) — geometry-space, since the
        // gesture mutates raw Paper geometry directly.
        xformAnchor = h.gCorners[ANCHOR_MAP[hh.edge]].clone();
        xformOrigHandlePos = h.gCorners[hh.edge].clone();
        xformMap = h.map;
        // Perpendicular box dimension normalizes the shear amount (Graphite
        // convention) so the visual skew angle stays consistent regardless
        // of box size — a drag of N px always skews by the same ANGLE, not
        // the same absolute offset.
        xformSkewPerp = Math.abs((hh.edge === 'n' || hh.edge === 's') ? h.bounds.height : h.bounds.width) || 1;
        xformLastShx = 0; xformLastShy = 0;
      } else {
        mode = 'xform-scale';
        xformDir = hh.dir;
        // Geometry-space anchor/handle (gesture math mutates raw geometry).
        xformAnchor = h.gCorners[ANCHOR_MAP[hh.dir]].clone();
        xformOrigHandlePos = h.gCorners[hh.dir].clone();
        // Same GP(x,y)=selBoxPt(x,y,box) mapping computeHandles used to
        // build gCorners itself, applied to the box's own local center —
        // lands in the identical geometry space as xformAnchor.
        xformCenterAnchor = selBoxPt(h.bounds.center.x, h.bounds.center.y, h.box).clone();
        xformActiveAnchor = xformAnchor;
        xformMap = h.map;
        xformLastSx = 1; xformLastSy = 1;
      }
      window.SMEngineBridge.renderNow();
      return;
    }

    // Maps a world point into the current selection's own (possibly
    // rotated) box space, same transform computeHandles()'s callers already
    // apply — hoisted out so the Alt+click disambiguation just below and
    // the body-hit-test right after it don't each carry their own copy.
    function pointInSelectionBody(worldPt, handle) {
      if (!handle) return false;
      var p = worldPt;
      if (handle.map) { var g = handle.map.inv(p.x, p.y); p = new Point(g[0], g[1]); }
      if (handle.box && handle.box.angle) p = p.rotate(-handle.box.angle, handle.box.pivot);
      return handle.bounds.contains(p);
    }
    // Alt+click with an active selection is two DIFFERENT things depending
    // on WHERE it lands (2026-09 — "je veux dupliquer avec alt en glissant
    // comme sur illustrator", but Alt+click was already Illustrator/Figma's
    // own "Option+click to move the reference point" convention, built
    // earlier per "avec alt et l'outil de sélection il faudrait pouvoir
    // changer le point d'ancrage de place" — the two conventions collide on
    // the same modifier, so the click's own position is what disambiguates,
    // confirmed with Cyril): landing INSIDE the current selection's own box
    // duplicates it and drags the copy, matching every reference app's
    // Alt/Option-drag; landing OUTSIDE it (empty canvas, or repositioning
    // the pivot somewhere away from the shape) keeps the original
    // anchor-placement behavior untouched.
    if (e.altKey && selectedPaths.length && pointInSelectionBody(pt, computeHandles())) {
      // Same clone pipeline as duplicateSelection()/pasteSelection's
      // "same place" branch (tools.js) — offset 0 so the copy starts
      // exactly where the original sits, THEN falls through to the body-
      // hit-test right below (now re-evaluated against these fresh
      // clones, at the same position, so it finds the click inside them
      // too) to set mode='move' and drag them for the rest of this same
      // gesture. The originals are left completely untouched underneath.
      pushUndo();
      var dupLayer = userLayers[state.activeLayerIdx];
      var dupSnaps = selectedPaths.map(_snapshotForClone).filter(Boolean);
      if (dupSnaps.length) {
        var dupClones = _materializeClones(dupSnaps, dupLayer, 0, (state.layers[state.activeLayerIdx] || {}).groups);
        selectedPaths = dupClones.filter(isSelectablePathChild);
        state.selectedStrokeIndices = selectedPaths.map(getSI).filter(function (i2) { return i2 >= 0; });
        saveActiveLayerFrame();
      }
      // Deliberately no `return` — falls through to the body-hit-test
      // below, same as an ordinary (non-Alt) click on the selection body.
    } else if (e.altKey && selectedPaths.length) {
      placeAnchorAt(pt, e.shiftKey);
      if (window.renderXformAnchorGrid) renderXformAnchorGrid();
      window.SMEngineBridge.resume();
      window.SMEngineBridge.renderNow();
      // Continue as a live drag (2026-07 fix, "alt+glisser le point
      // d'ancrage fait tourner le canvas"): this branch used to teleport
      // the anchor once and return with `mode` still null — onMove's
      // no-active-mode path never stopPropagation()s, so every following
      // pointermove (mouse still down) fell through untouched to
      // viewtools-bridge.js's own global "Alt+drag = rotate the canvas
      // view" shortcut instead. Setting mode here makes the rest of the
      // gesture go through the SAME 'xform-anchor-drag' handling as
      // starting exactly on the crosshair (onMove line ~490ish), which
      // does stop propagation — the anchor now follows the pointer for
      // the whole drag instead of only snapping once at mousedown.
      mode = 'xform-anchor-drag';
      return;
    }

    // The transform box itself is a draggable surface, including its empty
    // center. This matters especially for Ghost All where the distribution
    // box can span large gaps with no actual path under the pointer.
    if (selectedPaths.length) {
      var bodyHandle = computeHandles();
      if (bodyHandle) {
        if (pointInSelectionBody(pt, bodyHandle)) {
          // Widen to the clicked item's full group/combine-group BEFORE
          // starting the move (2026-07-29, QA-confirmed): this shortcut
          // fires and returns on ANY click inside the current selection's
          // own box — which a lone group member's body-click always is,
          // since its own box IS that member's bounds. Without this, the
          // idx2 group-widening logic a bit further below (for the case
          // where `hit` misses this early shortcut) never even gets a
          // chance to run: a Subselect edit leaving just one member
          // selected meant clicking that member again to grab "the whole
          // group" only ever dragged the one member.
          if (window.SMGroup && !skipGroupWiden) {
            var bodyLd = state.layers[state.activeLayerIdx];
            if (!(bodyLd && bodyLd.locked && !bodyLd.symbolId)) {
              var bodyLayer = userLayers[state.activeLayerIdx];
              var bodyHit = bodyLayer.hitTest(pt, { stroke: true, fill: true, tolerance: 8 / view.zoom });
              if (bodyHit && (bodyHit.item instanceof Path || bodyHit.item instanceof Raster) && combineHitConfirm(bodyHit, pt, state.activeLayerIdx)) {
                var bodyP = resolveBrushAnchor(bodyHit.item, bodyLayer);
                SMGroup.membersOf(bodyP, bodyLayer).forEach(function (m) { if (selectedPaths.indexOf(m) < 0) selectedPaths.push(m); });
                state.selectedStrokeIndices = selectedPaths.map(getSI).filter(function (i2) { return i2 >= 0; });
              }
            }
          }
          // Shift inside the selection's own box (2026-09, #802): this
          // shortcut returns before the hit branch below, so its Shift
          // handling never ran here — a Shift-click on an already-selected
          // item could not remove it, and a Shift-click on an unselected
          // one sitting inside the box could not add it. Same click-vs-drag
          // split as down there: a toggle-off is DEFERRED to pointerup (so
          // Shift+drag stays a constrained move of the whole selection),
          // an add applies immediately (it can only grow what gets moved).
          if (e.shiftKey) {
            var shLayer = userLayers[state.activeLayerIdx];
            var shHit = shLayer.hitTest(pt, { stroke: true, fill: true, tolerance: 8 / view.zoom });
            if (shHit && (shHit.item instanceof Path || shHit.item instanceof Raster)) {
              var shP = resolveBrushAnchor(shHit.item, shLayer);
              var shSet = window.SMGroup ? SMGroup.membersOf(shP, shLayer) : [shP];
              if (selectedPaths.indexOf(shP) >= 0) _pendingShiftToggle = shSet.slice();
              else {
                shSet.forEach(function (m) { if (selectedPaths.indexOf(m) < 0) selectedPaths.push(m); });
                state.selectedStrokeIndices = selectedPaths.map(getSI).filter(function (i2) { return i2 >= 0; });
                renderArcs(); updateUI();
              }
            }
          }
          mode = 'move';
          moveStarted = false;
          return;
        }
      }
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
    // Motion mode: the layer's VISIBLE position/rotation/scale is a
    // render-time-only transform (motion.js's computeMotionMat, applied
    // exclusively inside buildSceneJson — the raw Paper.js geometry
    // underneath is NEVER moved, by design, so a save can't bake it in).
    // Hit-testing with the raw pointer therefore missed the shape the
    // moment any key made its rendered position diverge from where it was
    // actually drawn — found live (2026-07, "impossible d'ajouter une
    // troisième keyframe en bougeant le calque dans le canvas"): with 2
    // keys already offsetting the layer, clicking the shape where it
    // VISIBLY sits hit nothing; clicking its invisible original (frame-0)
    // position worked. Map the pointer back through the layer's own Motion
    // transform first — same inverse already used by the transform-box
    // handles above (xformMap) — so testing against geometry that never
    // actually moved uses a point in the space it still lives in.
    var hitPt = pt;
    // No longer gated on Motion mode (2026-08-31): buildSceneJson applies a
    // layer's motionMat in EITHER mode (CLAUDE.md §8), so an animated layer
    // renders moved in Animation 2D too — and there the click was still
    // tested against the un-moved geometry, so nothing could be selected
    // where it visibly sits. Both call sites (left click and right click)
    // get it, or the two would disagree about what is under the cursor.
    if (window.SMMotion) {
      var hitMap = SMMotion.layerMotionPointMap(state.activeLayerIdx);
      // 3D layers (2026-07-29 fix) — layerMotionPointMap returns null for a
      // 3D-toggled layer even with real rotationX/rotationY set (it only
      // recognizes the base 2D properties); layerMotion3DPointMap is the
      // dedicated perspective-correct counterpart — see its own header
      // comment in motion.js for the ray-plane-intersection math.
      if (!hitMap && SMMotion.layerMotion3DPointMap) hitMap = SMMotion.layerMotion3DPointMap(state.activeLayerIdx);
      if (hitMap) { var hg = hitMap.inv(pt.x, pt.y); hitPt = new Point(hg[0], hg[1]); }
    }
    // Per-ELEMENT transforms on top of the layer's (2026-08-31): hitTestPosed
    // undoes each child's own Motion for that child alone, which a single
    // layer.hitTest cannot do — it tests one point against geometry that
    // several independent transforms have moved. Measured in Animation 2D
    // with the layer transformed AND the two elements dragged apart:
    // clicking either at its visible position selected nothing. hitPt
    // (layer-space) stays the fallback, and the posed test hands back the
    // point in the element's own space for combineHitConfirm.
    var posedDispo = !!window.hitTestPosed && !(activeLdForLock.locked && !activeLdForLock.symbolId);
    var posedHit = posedDispo ? hitTestPosed(state.activeLayerIdx, pt, 8 / view.zoom) : null;
    // No raw fallback once the posed test has spoken (2026-08-31): it tests
    // the shapes where they are DRAWN, so "no hit" means the cursor really is
    // over nothing on this layer. Falling back to layer.hitTest(hitPt) then
    // re-tested the same click against the layer's UN-posed geometry, which
    // for a transformed layer sits somewhere else entirely — a click on
    // another layer's shape matched a shape of the active layer and selected
    // it instead. That is the "il select le premier groupe" report. The raw
    // test only remains for the case where the posed one could not run.
    var hit = posedDispo ? posedHit : ((activeLdForLock.locked && !activeLdForLock.symbolId) ? null : layer.hitTest(hitPt, { stroke: true, fill: true, tolerance: 8 / view.zoom }));
    var confirmPt = (posedHit && posedHit.localPoint) ? posedHit.localPoint : hitPt;
    if (hit && !combineHitConfirm(hit, confirmPt, state.activeLayerIdx)) hit = null;
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
        // Same inverse-motion mapping the ACTIVE layer already gets above
        // (hitPt) — without this, clicking a NON-active layer's VISIBLE
        // (Motion-keyed) position missed entirely the moment it had any
        // Position/Rotation/Scale of its own, since its raw Paper.js
        // geometry never actually moves (2026-08 fix, feedback:
        // "impossible de select le layer 1 en cliquant dessus ssi le null
        // est select" — reproducible with ANY other layer active, not
        // Null-specific, but a Null is what the report happened to test:
        // Layer 1 had Motion keys of its own, so its clickable geometry
        // and its visible position had already diverged).
        var oPt = pt;
        if (state.appMode === 'motion' && window.SMMotion) {
          var oMap = SMMotion.layerMotionPointMap(oli);
          if (!oMap && SMMotion.layerMotion3DPointMap) oMap = SMMotion.layerMotion3DPointMap(oli);
          if (oMap) { var og = oMap.inv(pt.x, pt.y); oPt = new Point(og[0], og[1]); }
        }
        var oh = pl.hitTest(oPt, { stroke: true, fill: true, tolerance: 8 / view.zoom });
        if (oh) { hit = oh; hitOtherLayerIdx = oli; break; }
      }
    }

    // Click-inside-the-visible-hover-box fallback (2026-08, feedback: "peu
    // importe où on clic si celle-ci [la box de hover] est affichée ça doit
    // select l'objet"). The hover box (getHoverBounds/buildHoverBoxItems,
    // engine-bridge.js) is the shape's ORIENTED bounding box, which for a
    // rotated shape is generally a bit larger than its exact stroke/fill —
    // the precise hitTest above can miss near a corner while the box is
    // still visibly on screen there. Deliberately does its OWN box-containment
    // scan (mirroring the two precise-hitTest passes just above: active
    // layer first, then every other visible/unlocked layer, topmost child
    // first) rather than trusting _hoverPathA2D — that flag reflects the
    // LAST pointermove's precise hit, which for a real drag-into-the-gap
    // gesture is often already null by the time onDown fires (mousemove
    // re-runs the same precise hitTest at ~the same coordinates a moment
    // before mousedown), so relying on it would silently do nothing for
    // exactly the corner-click case this is meant to fix. Motion mode has
    // its own separate hit-testing (SMMotion.onDown, tried first above) and
    // never reaches this far with anything left to do here.
    if (!hit && state.appMode !== 'motion') {
      var boxHitLd = state.layers[state.activeLayerIdx];
      if (!(boxHitLd && boxHitLd.locked && !boxHitLd.symbolId)) {
        var boxHit = hitTestOrientedBoxA2D(hitPt, layer);
        if (boxHit) { hit = { item: boxHit }; }
      }
      if (!hit) {
        for (var bpli = project.layers.length - 1; bpli >= 0; bpli--) {
          var bpl = project.layers[bpli];
          var boli = userLayers.indexOf(bpl);
          if (boli < 0 || boli === state.activeLayerIdx) continue;
          var bld2 = state.layers[boli];
          if (!bld2 || bld2.locked || !bld2.visible || bld2.symbolId) continue;
          var bh = hitTestOrientedBoxA2D(pt, bpl);
          if (bh) { hit = { item: bh }; hitOtherLayerIdx = boli; break; }
        }
      }
    }
    // Motion's own version of the fallback just above (2026-08-31 follow-up
    // to the same feedback that built it — "peu importe où on clic dans la
    // boîte de hover si celle-ci est affiché ça doit select le group"). The
    // gate above assumed SMMotion.onDown() already covered every Motion
    // click, but onDown only grabs handles/dots/anchors on the CURRENT
    // target; a plain click elsewhere within a layer's own shown box
    // (layerWorldBoundsUnion — the exact bounds hoverOverlayItems draws,
    // motion.js) still fell through to the precise per-child hitTest above,
    // which misses in the gap between two elements or near a corner the box
    // covers but no ink reaches. Found live chasing "impossible de
    // reselect parfois, j'arrive pas à savoir quand": once two elements
    // inside a group have been dragged apart, their combined box has a lot
    // of empty middle that LOOKS selectable (it's the box actually drawn)
    // but wasn't. Null/native-video/Component layers are excluded — each
    // already has its own dedicated, better-tailored hit-test a few lines
    // below (nullHit/nvHit/compHit), which only runs `if (!hit)`; matching
    // them here first would silently skip that specialized handling.
    // Set when the fallback below is the ONLY reason `hit` is truthy — read
    // much further down (mode = ... 'move') to keep this a pure SELECT, not
    // also a drag start. See that read site's own comment for why.
    var motionBoxFallbackOnly = false;
    if (!hit && state.appMode === 'motion' && window.SMMotion && window.SMMotion.layerWorldBoundsUnion) {
      var motionLayerBoxHit = function (li) {
        var mld = state.layers[li];
        if (!mld || mld.locked || !mld.visible || mld.threeD || mld.symbolId || mld.isNullLayer || mld.nativeVideo) return null;
        var mb = SMMotion.layerWorldBoundsUnion([li], state.currentFrame);
        if (!mb || pt.x < mb.x || pt.x > mb.x + mb.w || pt.y < mb.y || pt.y > mb.y + mb.h) return null;
        var mul = userLayers[li];
        if (!mul) return null;
        for (var mci = mul.children.length - 1; mci >= 0; mci--) {
          var mc = mul.children[mci];
          if (!(mc instanceof Path || mc instanceof Raster)) continue;
          if (mc.data && (mc.data.isLinkedFillCompanion || mc.data.isBrushTextureCopy || mc.data.isDuplicatorCopy)) continue;
          return mc;
        }
        return null;
      };
      var mActiveHit = motionLayerBoxHit(state.activeLayerIdx);
      if (mActiveHit) {
        hit = { item: mActiveHit };
        motionBoxFallbackOnly = true;
      } else {
        for (var mpli = project.layers.length - 1; mpli >= 0; mpli--) {
          var mpl = project.layers[mpli];
          var moli = userLayers.indexOf(mpl);
          if (moli < 0 || moli === state.activeLayerIdx) continue;
          var mItemHit = motionLayerBoxHit(moli);
          if (mItemHit) { hit = { item: mItemHit }; hitOtherLayerIdx = moli; motionBoxFallbackOnly = true; break; }
        }
      }
    }

    if (!hit) {
      // Selected video's transform handles FIRST (2026-07, full gizmo —
      // corners scale, ring rotates), before any body hit-test: the ring
      // sits OUTSIDE the display rect, so the point-in-rect walk below
      // would never reach it. Geometry comes from the same
      // SMNativeVideo.transformBox engine-bridge draws from, so grabbed ==
      // drawn by construction.
      if (window._nvSelectedLayer != null && window.SMNativeVideo && window.SMMotion) {
        var selLd = state.layers[window._nvSelectedLayer];
        var tb = (selLd && selLd.nativeVideo) ? SMNativeVideo.transformBox(window._nvSelectedLayer) : null;
        if (tb) {
          var nvTol = 9 / view.zoom, nvRingTol = 7 / view.zoom;
          var hh2 = null;
          if (Math.abs(pt.getDistance(new Point(tb.ringCenter.x, tb.ringCenter.y)) - tb.ringRadius) < nvRingTol) hh2 = { type: 'rotate' };
          if (!hh2) {
            var bestD2 = nvTol;
            Object.keys(tb.corners).forEach(function (k) {
              var d2 = pt.getDistance(new Point(tb.corners[k].x, tb.corners[k].y));
              if (d2 < bestD2) { bestD2 = d2; hh2 = { type: 'scale' }; }
            });
          }
          if (hh2) {
            pushUndo();
            nvIdx = window._nvSelectedLayer;
            nvPivot = new Point(tb.center.x, tb.center.y);
            if (hh2.type === 'rotate') {
              mode = 'nv-rotate';
              nvStartAngle = Math.atan2(pt.y - nvPivot.y, pt.x - nvPivot.x) * 180 / Math.PI;
              nvOrigRot = SMMotion.getLayerValue(nvIdx, 'rotation')[0];
            } else {
              mode = 'nv-scale';
              nvOrigDist = Math.max(1e-6, pt.getDistance(nvPivot));
              nvStartScale = SMMotion.getLayerValue(nvIdx, 'scale');
            }
            return;
          }
        }
      }
      // Native video footage: click inside a visible video layer's display
      // rect (topmost first) selects it + starts a move gesture. Runs only
      // when no stroke was hit — drawings sit ON TOP of footage, so a
      // stroke click must keep selecting the stroke. Rotation-aware: the
      // point is spun BACK around the rect center by the rect's own
      // rotation before the axis-aligned containment check (the rect
      // renders rotated since the image items grew a rotation field).
      var nvHit = -1;
      if (window.SMNativeVideo && window.SMMotion) {
        for (var nvi = state.layers.length - 1; nvi >= 0; nvi--) {
          var nld = state.layers[nvi];
          if (!nld || !nld.nativeVideo || !nld.visible || nld.locked) continue;
          var nvr = SMNativeVideo.displayRect(nvi);
          if (!nvr) continue;
          var tpx = pt.x, tpy = pt.y;
          if (nvr.rotation) {
            var ncx = nvr.x + nvr.width / 2, ncy = nvr.y + nvr.height / 2;
            var na = -nvr.rotation * Math.PI / 180, nc = Math.cos(na), ns = Math.sin(na);
            var ndx = pt.x - ncx, ndy = pt.y - ncy;
            tpx = ncx + ndx * nc - ndy * ns; tpy = ncy + ndx * ns + ndy * nc;
          }
          if (tpx >= nvr.x && tpx <= nvr.x + nvr.width && tpy >= nvr.y && tpy <= nvr.y + nvr.height) { nvHit = nvi; break; }
        }
      }
      if (nvHit >= 0) {
        if (!e.shiftKey) clearSel();
        state.activeLayerIdx = nvHit;
        activateUL(nvHit);
        syncMotionLayerSelection(nvHit, e.shiftKey);
        window._nvSelectedLayer = nvHit; // gizmo drawn by buildTransformBoxItems' nv branch
        mode = 'nv-drag';
        nvIdx = nvHit;
        nvStartPt = pt.clone();
        nvScaleMode = false; // corner handles replaced the historical Shift+drag scale gesture
        nvStartPos = SMMotion.getLayerValue(nvHit, 'position');
        nvStartScale = SMMotion.getLayerValue(nvHit, 'scale');
        nvMoved = false;
        renderArcs(); updateUI();
        window.SMEngineBridge.renderNow();
        return;
      }
      // Null layer markers (2026-08, feedback #59 — a Null had zero canvas
      // presence before this). Hit-test against the SAME resolved screen
      // position buildNullLayerItems (engine-bridge.js) draws its marker
      // at — nullPos composed through the layer's own Motion transform +
      // full parent chain, mirroring buildNullLayerItems exactly so what's
      // clickable always matches what's drawn.
      var nullHit = -1;
      if (window.SMMotion) {
        var nullTol = 12 / view.zoom;
        for (var nli = state.layers.length - 1; nli >= 0; nli--) {
          var nlld = state.layers[nli];
          if (!nlld || !nlld.isNullLayer || !nlld.visible || nlld.locked) continue;
          var nBase = nlld.nullPos || [state.canvasW / 2, state.canvasH / 2];
          var nOwnMat = SMMotion.layerMotionAt(nli, state.currentFrame);
          var nPt = [{ point: [nBase[0] + (nOwnMat ? nOwnMat.dx : 0), nBase[1] + (nOwnMat ? nOwnMat.dy : 0)], handleIn: [0, 0], handleOut: [0, 0] }];
          var nChain = SMMotion.parentChainMats(nli, state.currentFrame);
          for (var npc = 0; npc < nChain.length; npc++) nPt = SMMotion.transformSegments(nPt, nChain[npc].pivot, nChain[npc].mat);
          var ncx = nPt[0].point[0], ncy = nPt[0].point[1];
          if (pt.getDistance(new Point(ncx, ncy)) <= nullTol) { nullHit = nli; break; }
        }
      }
      if (nullHit >= 0) {
        nvClearSelection();
        if (!e.shiftKey) clearSel();
        if (window.SMMotion) SMMotion.setMotionCanvasEmptyClick(false);
        state.activeLayerIdx = nullHit;
        activateUL(nullHit);
        _layerSel = [nullHit]; _layerSelAnchor = nullHit;
        syncMotionLayerSelection(nullHit, e.shiftKey);
        mode = 'null-drag';
        nullIdx = nullHit;
        nullStartPt = pt.clone();
        nullStartPos = SMMotion.getLayerValue(nullHit, 'position');
        nullMoved = false;
        renderArcs(); updateUI();
        window.SMEngineBridge.renderNow();
        return;
      }
      nvClearSelection(); // clicked empty canvas/another target — video deselects like any object would
      var compHit = hitTestComponentLayers(pt);
      if (compHit) {
        var now2 = Date.now();
        var isDbl = _compClick.layerIdx === compHit.layerIdx && (now2 - _compClick.time < 350);
        _compClick.layerIdx = compHit.layerIdx; _compClick.time = now2;
        if (!e.shiftKey) clearSel();
        if (window.SMMotion) SMMotion.setMotionCanvasEmptyClick(false);
        state.activeLayerIdx = compHit.layerIdx;
        activateUL(compHit.layerIdx);
        syncMotionLayerSelection(compHit.layerIdx, e.shiftKey);
        selectedPaths = userLayers[compHit.layerIdx].children.filter(function (c) { return (c instanceof Path || c instanceof Raster) && isSelectablePathChild(c); });
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
      nvClearSelection(); // selecting a stroke/image deselects any selected video, like any selection change
      if (window.SMMotion) SMMotion.setMotionCanvasEmptyClick(false);
      if (hitOtherLayerIdx >= 0) {
        state.activeLayerIdx = hitOtherLayerIdx;
        activateUL(hitOtherLayerIdx);
      }
      syncMotionLayerSelection(state.activeLayerIdx, e.shiftKey);
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
        selectedPaths = userLayers[state.activeLayerIdx].children.filter(function (c) { return (c instanceof Path || c instanceof Raster) && isSelectablePathChild(c); });
        state.selectedStrokeIndices = [];
        mode = selectedPaths.length ? 'move' : null;
        moveStarted = false;
        renderArcs(); updateUI();
        window.SMEngineBridge.renderNow();
        if (isDbl2) window.SM.enterSymbol(activeLd.symbolId);
        return;
      }
      // A click landing on a brush-texture companion (vector dab, or Bitmap
      // Brush v2's raster texture — the anchor under it is stroke-
      // camouflaged, so the companion is often the only hittable thing)
      // selects the real anchor, not the companion; moving/deleting a
      // companion alone would silently desync it from its group.
      var p = resolveBrushAnchor(hit.item, userLayers[state.activeLayerIdx]);
      // Group (group-bridge.js, 2026-07): clicking any ONE member selects
      // every sibling sharing its groupId — membersOf returns `[p]`
      // unchanged when it isn't grouped, so this is a no-op widening for
      // the (overwhelmingly common) ungrouped case.
      var clickedSet = window.SMGroup ? SMGroup.membersOf(p, userLayers[state.activeLayerIdx]) : [p];
      var idx2 = selectedPaths.indexOf(p);
      if (e.shiftKey) {
        // Shift on an ALREADY-selected item is ambiguous: it means "remove
        // me from the selection" for a CLICK, and "constrain this move to
        // one axis" for a DRAG (Illustrator/Figma both read it that way).
        // Removing it here settled the ambiguity at pointerdown, in favour
        // of the click — so Shift+drag deselected the thing being dragged
        // and the axis lock never got a selection to move (2026-09,
        // feedback #802: "le shift + drag un objet ne le contraint pas en
        // x ou y" — in Motion nothing moved at all, since the layer-body
        // drag needs a non-empty selectedPaths to arm). Deferred to
        // pointerup instead, and applied only if the gesture never moved.
        if (idx2 >= 0) _pendingShiftToggle = clickedSet.slice();
        else clickedSet.forEach(function (m) { if (selectedPaths.indexOf(m) < 0) selectedPaths.push(m); });
      } else if (idx2 < 0) {
        // Clicking a NEW item without shift replaces the selection — but
        // clicking one already part of a multi-selection must NOT clear the
        // rest of it first, or dragging the group by its body collapses the
        // selection down to just the clicked item before the move-drag
        // even starts (only that one element then moves) — matches the
        // reported "transform works but moving several selected elements
        // doesn't".
        clearSel();
        clickedSet.forEach(function (m) { selectedPaths.push(m); });
        // _groupEnteredGid (tools.js, group double-click) tracks "did the
        // LAST thing that happened enter this exact group via double-click,
        // with nothing unrelated since" — but nothing ever reset it back to
        // null, so it stayed set to whatever group was last double-clicked
        // for the rest of the SESSION. A plain single click here landing on
        // a DIFFERENT (or ungrouped) target means that continuity is over —
        // without this, double-clicking a group you'd entered earlier (even
        // much earlier, after touching plenty of unrelated shapes since)
        // skipped straight to Subselect on what the user experiences as a
        // fresh first double-click (2026-07-30 fix, Cyril: "un bug pourtant
        // relevé très souvent — si je double clic [sur un groupe déjà
        // touché] ça switch sur subselect alors que ça devrait rester
        // select"). Clicking back onto the SAME still-entered group is a
        // no-op here (condition short-circuits), so the genuine "double-
        // click, then immediately double-click again" fast path into
        // Subselect is untouched — see tools.js's onViewDoubleClick.
        if (window._groupEnteredGid && (!p.data || p.data.groupId !== window._groupEnteredGid)) window._groupEnteredGid = null;
        // Same reset for the ungrouped-shape flag (2026-08-30, feedback #165).
        // Deliberately written the same way rather than folded in with the
        // line above: the two track different things and a shape can carry
        // both a strokeId and a groupId. Clicking away from the entered shape
        // ends its continuity, so coming back to it is a fresh first
        // double-click — which is exactly the "je clic ailleurs, je reviens
        // sur la bounding box globale" half of the request. Without it this
        // would reproduce the July bug quoted just above, one level down.
        if (window._shapeEnteredId && (!p.data || p.data.strokeId !== window._shapeEnteredId)) {
          // Per-object boxes (2026-08-30): while that mode is on, clicking a
          // SIBLING shape on the same layer hands the isolation over to it
          // instead of ending it — that is the mode's whole point ("je peux
          // transformer tranquillement mes shapes avec leur propre bounding
          // box"), and it keeps the next double-click on the newly clicked
          // shape reaching Subselect, since alreadyIsolated (tools.js) then
          // sees ITS id. Clicking a grouped shape or one on another layer
          // still ends the continuity exactly as before.
          if (window._perObjBoxes === state.activeLayerIdx && p.data && p.data.strokeId && !p.data.groupId) {
            window._shapeEnteredId = p.data.strokeId;
          } else {
            window._shapeEnteredId = null;
            window._perObjBoxes = null;
          }
        }
      } else {
        // p is ALREADY selected (idx2>=0) — but its own group siblings might
        // not be (2026-07-29 fix, QA-confirmed): right after a Subselect
        // vertex-edit leaves just ONE member selected, clicking that same
        // member again (to grab "the whole group") left selectedPaths at
        // just the one path, since neither branch above ever ran. ADD any
        // missing sibling rather than replacing the selection outright — an
        // existing broader multi-selection (e.g. from a marquee spanning
        // several unrelated shapes, one of which happens to be `p`) must
        // still survive a click-to-drag on one of its members, same
        // reasoning the idx2>=0 short-circuit existed for in the first place.
        if (!skipGroupWiden) clickedSet.forEach(function (m) { if (selectedPaths.indexOf(m) < 0) selectedPaths.push(m); });
      }
      state.selectedStrokeIndices = selectedPaths.map(getSI).filter(function (i2) { return i2 >= 0; });
      // motionBoxFallbackOnly (set above): this hit came ONLY from clicking
      // inside a layer's shown box, not from landing on real ink — the
      // fallback's own job is SELECT ("ça doit select le group"), same as
      // its A2D precedent. Letting it also arm a drag turned every
      // near-miss inside (or just after leaving) a group into a silent
      // WHOLE-LAYER move: found live chasing "entrée double clic >
      // impossible de déplacer les éléments" — a click aimed at an element
      // but landing outside its own small box exits the group (previous
      // commit) and, with mode='move' armed here, immediately started
      // dragging the entire layer on the very same gesture instead of
      // either grabbing the element or doing nothing. ld.elementMotion for
      // both elements stayed `{}` while ld.motionStatic.position picked up
      // every one of these misses — moving together is not the same bug as
      // not moving at all, but it read exactly like "impossible de
      // déplacer les éléments" from the canvas. A precise hit (this flag
      // false) is unaffected — that path already worked and still starts a
      // drag immediately, matching every other grab in this file.
      mode = (selectedPaths.length && !(state.appMode === 'motion' && motionBoxFallbackOnly)) ? 'move' : null;
      moveStarted = false;
      _multiLayerDrag = null;
    } else {
      if (!e.shiftKey) clearSel();
      syncMotionLayerSelection(null, e.shiftKey);
      // Empty-canvas deselect (2026-08 fix, feedback: "impossible de tout
      // déselect dans le canvas") — see setMotionCanvasEmptyClick's own
      // comment (motion.js): only additive-free (no shift) clicks actually
      // mean "deselect", matching clearSel's own shiftKey gate just above.
      if (state.appMode === 'motion' && window.SMMotion && !e.shiftKey) SMMotion.setMotionCanvasEmptyClick(true);
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
    // See onDown's comment — SMMotion.onDrag no-ops (returns false) unless
    // its own onDown just started a handle/dot/anchor drag, so this is safe
    // to probe unconditionally without any extra state of our own.
    if (state.appMode === 'motion' && window.SMMotion) {
      var w1 = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
      // shiftKey travels with the point (2026-09, #802): Motion's own drags
      // (element move, multi-layer move) apply the axis lock themselves, and
      // this forwarded event was the only place that state could come from.
      if (SMMotion.onDrag({ point: new Point(w1[0], w1[1]), shiftKey: e.shiftKey })) {
        e.stopImmediatePropagation(); e.preventDefault();
        return;
      }
      // Hover highlight (2026-08, feedback: "quand on roll over un élément
      // dans le canvas un rec de bounding box de l'élément doit apparaitre
      // comme dans after effects") — only while idle (onDrag above already
      // returned false, so nothing is being dragged), passive read like
      // Animation 2D's own hover-only pass just below.
      if (window.SMMotion.onHoverMove({ x: w1[0], y: w1[1] })) window.SMEngineBridge.renderNow();
    }
    if (mode === 'cornerRadius' && _cornerDrag) {
      e.stopImmediatePropagation(); e.preventDefault();
      var wc = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
      var ptc = new Point(wc[0], wc[1]);
      var kindDef = PARAM_HANDLE_KINDS[_cornerDrag.path.data.paramShape.kind];
      var rc = kindDef.valueFromDrag(_cornerDrag.path, _cornerDrag.corner, ptc);
      kindDef.commit(_cornerDrag.path, _cornerDrag.corner, rc);
      if (window.updateCornersPanel) updateCornersPanel();
      if (window.updateEllipseArcPanel) updateEllipseArcPanel();
      if (window.updateStarPanel) updateStarPanel();
      window.SMEngineBridge.renderNow();
      return;
    }
    if (!mode) {
      // Shape hover highlight (2026-08, feedback: "roll hover n'existe pas
      // sur animation 2D") — runs regardless of whether anything is already
      // selected, unlike the anchor/ring hover block below which only makes
      // sense once selectedPaths.length.
      if (shouldIntercept()) {
        var whA2D = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
        if (onHoverMoveA2D(new Point(whA2D[0], whA2D[1]))) {
          // 2026-08 tween-reassign hover badge ("au roll hover les id de
          // forme en couleur verte") — piggybacks on this SAME hitTest pass
          // (only fires when the hover TARGET actually changed, exactly
          // like the highlight box above) instead of tweens.js running its
          // own hitTest on every mousemove.
          if (window.updateReassignBadgeHover) window.updateReassignBadgeHover(_hoverPathA2D);
          window.SMEngineBridge.renderNow();
        }
      } else if (_hoverPathA2D) {
        _hoverPathA2D = null;
        if (window.updateReassignBadgeHover) window.updateReassignBadgeHover(null);
      }
      // Hover-only pass (not dragging anything) — tracks whether the
      // pointer sits over the anchor crosshair so engine-bridge.js can draw
      // it slightly larger, live UX feedback requested 2026-07 ("un petit
      // hover visible léger scale serait pas mal"). Deliberately does NOT
      // stopPropagation/preventDefault: this is a passive read, other
      // tools/listeners must keep working normally while the Select tool
      // merely hovers with nothing being dragged.
      if (shouldIntercept() && selectedPaths.length) {
        var wh = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
        var hpt = new Point(wh[0], wh[1]);
        // Alt-gated, matching hitTestHandles' own new requirement — showing
        // the "grabbable" grow effect without Alt held would visually
        // promise a drag that onDown won't actually honor.
        //
        // computeHandles() used to run BEFORE that gate even though e.altKey
        // is its only consumer here (hh2 is read on the next line and
        // nowhere else). It goes through orientedSelBox(), which deep-clones
        // every selected path when the selection box is rotated — paid on
        // every pointermove of a plain hover, with no Alt held and nothing
        // being dragged. Same shape as registerRasterIfNeeded's canvas read:
        // expensive work in front of the cheap check that discards it.
        var hh2 = e.altKey ? computeHandles() : null;
        var isHover = !!(e.altKey && hh2 && hh2.anchorPos && hpt.getDistance(hh2.anchorPos) < 9 / view.zoom);
        if (isHover !== state.xformAnchorHovered) {
          state.xformAnchorHovered = isHover;
          window.SMEngineBridge.renderNow();
        }
        // Cursor feedback: skipped while Alt is held (that's the anchor-hover
        // state above; hitTestHandles would otherwise report a coincident
        // resize/rotate handle underneath it and show the wrong cursor).
        var hoverHit = e.altKey ? null : hitTestHandles(hpt, false);
        var nextCursor = hoverHit && hoverHit.type === 'scale' ? (HANDLE_CURSORS[hoverHit.dir] || 'default')
          : hoverHit && hoverHit.type === 'rotate' ? ROTATE_CURSOR
          // Skew cursor (2026-08): top/bottom edge shears horizontally (the
          // handle slides left/right) so it gets the EW cursor; left/right
          // shears vertically so it gets NS — matches Graphite's own mapping.
          : hoverHit && hoverHit.type === 'skew' ? ((hoverHit.edge === 'n' || hoverHit.edge === 's') ? 'ew-resize' : 'ns-resize')
          : 'default';
        if (canvasEl.dataset.xformCursor !== nextCursor) {
          canvasEl.style.cursor = nextCursor;
          canvasEl.dataset.xformCursor = nextCursor;
        }
        // Skew tick-mark hover highlight (2026-08) — same passive-hover
        // pattern as xformDistortHoverDir just below: engine-bridge.js's
        // buildTransformBoxItems reads this to recolor the hovered edge's
        // ticks before any drag starts.
        var skewHoverEdge = (hoverHit && hoverHit.type === 'skew') ? hoverHit.edge : null;
        if (skewHoverEdge !== (state.xformSkewHoverEdge || null)) {
          state.xformSkewHoverEdge = skewHoverEdge;
          window.SMEngineBridge.renderNow();
        }
        // Rotate-ring hover grow (2026-07, "le rond de rotation peut un peu
        // grossir au roll hover") — same light "you can grab this" pattern
        // as the anchor crosshair's own xformAnchorHovered just above.
        var isRingHover = !!(hoverHit && hoverHit.type === 'rotate');
        if (isRingHover !== state.xformRingHovered) {
          state.xformRingHovered = isRingHover;
          window.SMEngineBridge.renderNow();
        }
        // Ctrl-hover corner-pin affordance (2026-07 feedback: "qd ctrl est
        // appuyé voir une différence visuelle sur les corner") — passive,
        // same pattern as xformAnchorHovered/xformRingHovered above: lets
        // engine-bridge.js's buildTransformBoxItems recolor the ONE corner
        // that a Ctrl+drag from here would actually distort, before any
        // drag starts.
        var distortHoverDir = e.ctrlKey ? distortEligibleCornerAt(hpt) : null;
        if (distortHoverDir !== (state.xformDistortHoverDir || null)) {
          state.xformDistortHoverDir = distortHoverDir;
          window.SMEngineBridge.renderNow();
        }
      } else {
        if (canvasEl.dataset.xformCursor) {
          canvasEl.style.cursor = 'default';
          delete canvasEl.dataset.xformCursor;
        }
        if (state.xformDistortHoverDir) { state.xformDistortHoverDir = null; window.SMEngineBridge.renderNow(); }
        if (state.xformSkewHoverEdge) { state.xformSkewHoverEdge = null; window.SMEngineBridge.renderNow(); }
      }
      return;
    }
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var pt = new Point(w[0], w[1]);
    // Snap-to-guides (rulers-bridge.js, "comme dans tout bon soft") —
    // scoped to a plain single-selection MOVE drag in Animation 2D only,
    // not the multi-layer/Motion-mode branches below (those aren't the
    // canvas-guide use case guides were built for, and mvMap's inverse-
    // rotate math a few lines down expects the RAW pointer delta, not a
    // magnetically-adjusted one). SMRulers.snapPoint no-ops (returns pt
    // unchanged) whenever rulers/snap are off or nothing's close enough.
    if (mode === 'move' && state.appMode !== 'motion' && window.SMRulers) pt = window.SMRulers.snapPoint(pt, 8);
    // Snap-to-pixel-grid — same scope as the guide-snap right above,
    // independent toggle (state.pixelGridSnap, rulers-bridge.js). Applied
    // AFTER guide-snap so a guide near a pixel boundary still wins the tie
    // (guide placement is a deliberate, precise choice; the pixel grid is
    // an always-available fallback).
    if (mode === 'move' && state.appMode !== 'motion' && window.SMRulers && window.SMRulers.snapToPixelGrid) pt = window.SMRulers.snapToPixelGrid(pt);

    if (_multiLayerDrag && mode.indexOf('layer-multi-') === 0) {
      if (mode === 'layer-multi-move') {
        var md = pt.subtract(_multiLayerDrag.last);
        forEachMultiPath(_multiLayerDrag, function (p) { multiTranslatePath(p, md); });
        _multiLayerDrag.last = pt.clone();
      } else if (mode === 'layer-multi-scale') {
        var o = _multiLayerDrag.orig;
        var cur = pt.subtract(_multiLayerDrag.pivot);
        var sx = Math.abs(o.x) > 1e-6 ? cur.x / o.x : 1;
        var sy = Math.abs(o.y) > 1e-6 ? cur.y / o.y : 1;
        if (e.shiftKey) {
          var ratio = o.length > 1e-6 ? cur.length / o.length : 1;
          sx = ratio * (sx < 0 ? -1 : 1); sy = ratio * (sy < 0 ? -1 : 1);
        }
        if (Math.abs(sx) < 0.05) sx = sx < 0 ? -0.05 : 0.05;
        if (Math.abs(sy) < 0.05) sy = sy < 0 ? -0.05 : 0.05;
        var stepX = sx / _multiLayerDrag.lastSx, stepY = sy / _multiLayerDrag.lastSy, pivot = _multiLayerDrag.pivot;
        forEachMultiPath(_multiLayerDrag, function (p) {
          p.scale(stepX, stepY, pivot);
          if (window.syncParamShapeBoxOnScale) window.syncParamShapeBoxOnScale(p, stepX, stepY, pivot);
          transformFillGradient(p, function (gp) { return new Point(pivot.x + (gp.x - pivot.x) * stepX, pivot.y + (gp.y - pivot.y) * stepY); });
          if (p.data && p.data.isVectorBrush && p.data.centerSegments) { scaleCenterSegments(p.data.centerSegments, stepX, stepY, pivot.x, pivot.y); rebuildVectorBrushOutline(p); }
          if (p.data && p.data.brushCompanions) p.data.brushCompanions.forEach(function (c) { if (!c.removed) c.scale(stepX, stepY, pivot); });
        });
        _multiLayerDrag.lastSx = sx; _multiLayerDrag.lastSy = sy;
      } else if (mode === 'layer-multi-rotate') {
        var ang = Math.atan2(pt.y - _multiLayerDrag.pivot.y, pt.x - _multiLayerDrag.pivot.x) * 180 / Math.PI;
        var total = ang - _multiLayerDrag.startAngle;
        if (e.shiftKey) total = Math.round(total / 15) * 15;
        var step = total - _multiLayerDrag.lastAngle, rp = _multiLayerDrag.pivot;
        forEachMultiPath(_multiLayerDrag, function (p) {
          p.rotate(step, rp);
          transformFillGradient(p, function (gp) { return gp.rotate(step, rp); });
          if (p.data && p.data.isVectorBrush && p.data.centerSegments) { rotateCenterSegments(p.data.centerSegments, step, rp.x, rp.y); rebuildVectorBrushOutline(p); }
          if (p.data && p.data.brushCompanions) p.data.brushCompanions.forEach(function (c) { if (!c.removed) c.rotate(step, rp); });
          if (p.data) p.data.boxAngle = (((p.data.boxAngle || 0) + step) % 360);
        });
        _multiLayerDrag.lastAngle = total;
      }
      lastPt = pt;
      window.SMEngineBridge.renderNow();
      return;
    }

    if (mode === 'xform-anchor-drag') {
      // Live-follows the pointer — Shift held snaps to the nearest of the
      // 9 preset points (tl/tc/tr/ml/mc/mr/bl/bc/br) instead of a free
      // custom point, checked continuously so toggling Shift mid-drag
      // switches modes immediately, matching every other Shift-to-snap
      // drag convention in the app.
      placeAnchorAt(pt, e.shiftKey);
      if (window.renderXformAnchorGrid) window.renderXformAnchorGrid();
    } else if (mode === 'marquee') {
      var prevA = project.activeLayer;
      marqueeLayer.activate();
      // Null-guarded like the rectangle branch just below already is —
      // that branch's own guard shows a null rect here was already
      // considered reachable; the lasso branch simply never got the same
      // treatment. Degrade to doing nothing, never throw mid-drag.
      if (!_marquee.rect) {
        // nothing to extend — the gesture was never properly started
      } else if (_marquee.lasso) {
        _marquee.rect.add(pt);
      } else {
        var mx1 = Math.min(marqueeStart.x, pt.x), my1 = Math.min(marqueeStart.y, pt.y);
        var mx2 = Math.max(marqueeStart.x, pt.x), my2 = Math.max(marqueeStart.y, pt.y);
        if (_marquee.rect) _marquee.rect.remove();
        _marquee.rect = new Path.Rectangle({ from: new Point(mx1, my1), to: new Point(mx2, my2) });
      }
      prevA.activate();
    } else if (mode === 'null-drag') {
      // nullPos (set once at creation, see addNullLayer) is the Null's REST
      // base — canvas drag writes through the standard Position track
      // instead, exactly like nv-drag/mv-drag below (2026-08 fix, feedback:
      // dragging a Null never moved its children — legacyParentChainMats
      // only reads a parent's Motion position, and writing straight to
      // nullPos here meant computeMotionMat(null) stayed null forever, so
      // the Null contributed NOTHING — not even a no-op — to its own parent
      // chain; see motion.js's isNullLayer branch there).
      if (!nullMoved) { pushUndo(); nullMoved = true; }
      var nulld = pt.subtract(nullStartPt);
      SMMotion.setLayerValue(nullIdx, 'position', [nullStartPos[0] + nulld.x, nullStartPos[1] + nulld.y]);
      window._sceneVersion++;
      // feedback #211 — same cheap in-place field sync as the layer-body
      // drag below, needed here too since this writes Position through the
      // identical setLayerValue path.
      if (window.SMMotion && SMMotion.liveRefreshVisiblePropertyFields) SMMotion.liveRefreshVisiblePropertyFields();
      window.SMEngineBridge.renderNow();
    } else if (mode === 'nv-drag') {
      if (!nvMoved) { pushUndo(); nvMoved = true; }
      var nvd = pt.subtract(nvStartPt);
      SMMotion.setLayerValue(nvIdx, 'position', [nvStartPos[0] + nvd.x, nvStartPos[1] + nvd.y]);
      window._sceneVersion++;
      // feedback #211 — same cheap in-place field sync as the layer-body
      // drag below, needed here too since this writes Position through the
      // identical setLayerValue path.
      if (window.SMMotion && SMMotion.liveRefreshVisiblePropertyFields) SMMotion.liveRefreshVisiblePropertyFields();
      window.SMEngineBridge.renderNow();
    } else if (mode === 'nv-scale') {
      // Uniform corner scale around the box center — same ratio-of-
      // distances math as Motion mode's motionScale (recomputed from the
      // FIXED drag-start baseline every tick; setLayerValue writes an
      // absolute value, so no compounding drift).
      var nvRatio = pt.getDistance(nvPivot) / nvOrigDist;
      SMMotion.setLayerValue(nvIdx, 'scale', [nvStartScale[0] * nvRatio, nvStartScale[1] * nvRatio]);
      window._sceneVersion++;
      // feedback #211 — same cheap in-place field sync as the layer-body
      // drag below, needed here too since this writes Scale through the
      // identical setLayerValue path.
      if (window.SMMotion && SMMotion.liveRefreshVisiblePropertyFields) SMMotion.liveRefreshVisiblePropertyFields();
      window.SMEngineBridge.renderNow();
    } else if (mode === 'nv-rotate') {
      var nvAng = Math.atan2(pt.y - nvPivot.y, pt.x - nvPivot.x) * 180 / Math.PI;
      SMMotion.setLayerValue(nvIdx, 'rotation', [nvOrigRot + (nvAng - nvStartAngle)]);
      window._sceneVersion++;
      // feedback #211 — same cheap in-place field sync as the layer-body
      // drag below, needed here too since this writes Rotation through the
      // identical setLayerValue path.
      if (window.SMMotion && SMMotion.liveRefreshVisiblePropertyFields) SMMotion.liveRefreshVisiblePropertyFields();
      window.SMEngineBridge.renderNow();
    } else if (mode === 'move') {
      // Same ensureKeyframe()+reselect as the scale/rotate grab above (see
      // its comment) — a plain object-body drag needs it just as much: a
      // held frame's move was silently discarded the exact same way.
      // Skipped in Motion mode: this drag never touches ld.frames content
      // (only ld.motion, below), so promoting the current frame to a real
      // keyframe here would be a pure unrelated side effect — same
      // reasoning as the onUp guards for all three modes.
      if (!moveStarted) {
        // Cross-layer selection (2026-08, feedback #46 follow-on: a
        // marquee spanning several shapes INSIDE a component selects
        // across multiple Paper layers — see the marquee-finalize comment
        // above). ensureKeyframe() below only ever promotes/rebuilds the
        // ACTIVE layer's own frame (tools.js) — per loadFrame's identity-
        // based reuse (CLAUDE.md §5quater), every OTHER involved layer
        // keeps its existing Paper object refs untouched, so only the
        // active layer's own members need the re-grab below; the rest are
        // stashed by strokeId (survives a rebuild unchanged, same
        // established invariant CLAUDE.md §5ter/§1 already rely on) and
        // re-added afterward. A no-op for every ordinary (single-layer)
        // selection, since none of its members live outside the active
        // layer to begin with.
        var crossLayerOthers = selectedPaths
          .filter(function (p) { return p.layer !== userLayers[state.activeLayerIdx]; })
          .map(function (p) { return { layer: p.layer, strokeId: p.data && p.data.strokeId }; });
        pushUndo();
        if (state.appMode !== 'motion') ensureKeyframe();
        // Re-grab by index because ensureKeyframe() just above can rebuild
        // this layer's children, orphaning the Paper objects picked at
        // pointerdown. A COMPONENT selection has no indices though — its
        // branch in onDown selects the whole layer and leaves
        // selectedStrokeIndices empty on purpose — so the map produced an
        // EMPTY array and every remaining tick translated nothing. symMatrix
        // kept accumulating (it doesn't read selectedPaths), so the data
        // moved while the picture didn't, and the component jumped to its
        // new spot only on release (2026-07-27: "des component que l'on
        // bouge et qui ne bouge pas reel time avec le drag"; measured:
        // symMatrix.tx 755→831→907→982→1058→1133 across the ticks with
        // userLayers[0].bounds.x pinned at 963.8, then 1341.5 at pointerup).
        // Falling back to the same whole-layer grab that branch made keeps
        // the orphan fix for ordinary selections and restores the live
        // preview for components. Translating their geometry is purely
        // visual: saveActiveLayerFrame returns early on ld.symbolId, and
        // loadFrame re-derives the layer from the symbol + symMatrix, so it
        // can never be baked or double-applied.
        selectedPaths = state.selectedStrokeIndices.length
          ? state.selectedStrokeIndices.map(function (i) { return userLayers[state.activeLayerIdx].children[i]; }).filter(Boolean)
          : userLayers[state.activeLayerIdx].children.filter(function (c) { return (c instanceof Path || c instanceof Raster) && isSelectablePathChild(c); });
        crossLayerOthers.forEach(function (o) {
          if (!o.strokeId) return;
          o.layer.children.forEach(function (c) { if (c.data && c.data.strokeId === o.strokeId && selectedPaths.indexOf(c) < 0) selectedPaths.push(c); });
        });
        moveStarted = true;
        moveGrabLocal = (window.layerLocalPoint && state.appMode !== 'motion')
          ? layerLocalPoint(state.activeLayerIdx, lastPt) : null;
        // lastPt is still the pointerdown position here (this is the first
        // move tick) — the true baseline the Shift axis-lock measures from.
        moveGestureOrigin = lastPt.clone();
        moveAppliedTotal = new Point(0, 0);
        // Smart-guide targets are per-gesture (see resetSmartTargets).
        if (window.SMRulers && SMRulers.resetSmartTargets) SMRulers.resetSmartTargets();
      }
      var delta = pt.subtract(lastPt);
      // Shift axis-lock (Illustrator-style) — snap the TOTAL displacement
      // since the gesture began onto whichever axis it's closer to, then
      // emit only the corrective delta needed this tick to land exactly on
      // that snapped total. See moveGestureOrigin's own declaration for why
      // this measures from the gesture origin rather than clamping each
      // tick's own (already-tiny, noisy) delta independently.
      if (moveGestureOrigin) {
        if (e.shiftKey) {
          var totalRaw = pt.subtract(moveGestureOrigin);
          var totalSnapped = Math.abs(totalRaw.x) >= Math.abs(totalRaw.y) ? new Point(totalRaw.x, 0) : new Point(0, totalRaw.y);
          delta = totalSnapped.subtract(moveAppliedTotal);
        }
        // Smart alignment guides (2026-09, #747) — correct this tick's
        // delta so the selection's own edges/centres land exactly on a
        // neighbour's, and hand the overlay the lines to draw while the
        // magnet holds. Skipped under Shift: the axis lock right above is
        // an explicit constraint and a diagonal correction would fight it.
        if (!e.shiftKey && state.appMode !== 'motion' && window.SMRulers && SMRulers.smartAlignDelta) {
          var _sg = SMRulers.smartAlignDelta(selectedPaths, delta, 8);
          if (_sg && _sg.fix && (_sg.fix.x || _sg.fix.y)) delta = delta.add(_sg.fix);
          window._smartGuideLines = _sg ? _sg.lines : null;
        } else if (window._smartGuideLines) window._smartGuideLines = null;
        moveAppliedTotal = moveAppliedTotal.add(delta);
      }
      // Layer under a Motion transform: the pointer moves in RENDERED
      // space, the geometry lives underneath — pull the delta back
      // (inverse rotate + inverse scale) or the drag drifts/overshoots.
      // ONLY for the raw-geometry translate below (Animation 2D's own
      // selectedPaths.forEach(p.translate(...))) — Motion mode's own
      // `position` write further down must NOT go through this inverse.
      // 2026-08-21 fix ("si je le rotationne et après déplace tout le
      // group ça déplace bizarrement comme si le x et y était inversé"):
      // confirmed live — at rotation=90°, mvMap.invVec(50,0) returned
      // (~0,-50), a pure horizontal drag turned vertical. computeMotionMat's
      // own header comment is the reason why that's wrong for `position`
      // specifically: "Position's dx/dy is a plain translation applied
      // independently on top" of rotate/scale, i.e. position already
      // lives in the layer's own POST-rotation output space — treating a
      // position delta like a geometry delta (which DOES need inverse-
      // rotating, since raw Paper.js points live in PRE-rotation local
      // space) silently rotated every drag by the layer's own current
      // rotation. Invisible until now because invVec is the identity at
      // the default rotation=0/scale=100%, which is what every previous
      // Motion-mode move drag happened to be tested at.
      var mvMap = (window.SMMotion && SMMotion.layerMotionPointMap) ? SMMotion.layerMotionPointMap(state.activeLayerIdx) : null;
      var geomDelta = delta;
      if (mvMap) { var dv = mvMap.invVec(delta.x, delta.y); geomDelta = new Point(dv[0], dv[1]); }
      // Motion mode (2026-07-17, "quand on modifie ces properties dans le
      // canvas ça ne modifie ou ne créer pas de nouvelle clés" — a real
      // regression from the transform-box fix a few commits back: making
      // Select genuinely interceptable in Motion mode meant this drag
      // handler ran too, but it always mutated raw Paper geometry — Motion
      // mode's whole point is a KEYFRAMED transform on top of untouched
      // geometry, so a canvas drag here must write into ld.motion instead,
      // never touch selectedPaths directly (that would double-move: once
      // via the written key's rendered motionMat, once via the geometry
      // edit) and must skip symGestureAccumulate below (that's symMatrix,
      // a component's PLACEMENT transform — a separate, non-keyframed
      // mechanism; folding this drag into it too would double-apply for a
      // converted-in-Motion component). Incremental per-tick add (read the
      // CURRENT value fresh, add this tick's delta, write back) mirrors the
      // existing geometry code's own per-tick translate(delta) exactly, so
      // it needs no separate gesture-start baseline to track.
      if (state.appMode === 'motion') {
        var mvLi = state.activeLayerIdx;
        var mvCur = SMMotion.getLayerValue(mvLi, 'position');
        // Parent chain (2026-08 fix, feedback: "si je bouge un élément
        // parenté... à droite il va en bas") — delta above is raw WORLD
        // space; a parented layer's Position lives just BEFORE the parent
        // chain is applied (same space computeMotionMat's own dx/dy does),
        // so a rotated/scaled parent needs this undone or the drag axes
        // silently skew/invert, same bug class as mvMap/invVec right above
        // already fixed for the layer's OWN rotation.
        var mvPd = (window.SMMotion && SMMotion.invertVectorThroughParentChain) ? SMMotion.invertVectorThroughParentChain(mvLi, state.currentFrame, delta.x, delta.y) : [delta.x, delta.y];
        SMMotion.setLayerValue(mvLi, 'position', [mvCur[0] + mvPd[0], mvCur[1] + mvPd[1]]);
        window._sceneVersion++;
        lastPt = pt;
        // feedback #211, "les valeurs de propriété... ne changent pas en
        // temps réel pendant les modifications dans le canvas" — same
        // cheap in-place field sync motion.js's own onDrag now does after
        // every tick, needed here too since this layer-body drag writes
        // Position through the identical setLayerValue path.
        if (window.SMMotion && SMMotion.liveRefreshVisiblePropertyFields) SMMotion.liveRefreshVisiblePropertyFields();
        window.SMEngineBridge.renderNow();
        return;
      }
      // Past this point: Animation 2D's raw-geometry path only (Motion
      // mode always returned above) — this DOES need the inverse-rotated/
      // scaled delta (real Paper.js points live in pre-rotation local
      // space), so switch to it here rather than touching every one of
      // the several `delta` reads below individually.
      delta = geomDelta;
      // Applied through a function so the residual correction below can run
      // the exact same work a second time — every companion, gradient,
      // centerline and anchor has to move with it, and a correction that
      // skipped any of them would tear the drawing apart.
      var appliquerDelta = function (d) {
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
        p.translate(d);
        if (window.syncParamShapeBoxOnTranslate) window.syncParamShapeBoxOnTranslate(p, d.x, d.y);
        transformFillGradient(p, function (gp) { return gp.add(d); });
        if (p.data && p.data.isVectorBrush && p.data.centerSegments) {
          p.data.centerSegments.forEach(function (s) { s.point = [s.point[0] + d.x, s.point[1] + d.y]; });
        }
        if (p.data && p.data.linkedFill && !p.data.linkedFill.removed) p.data.linkedFill.translate(d);
        if (p.data && p.data.brushCompanions) {
          p.data.brushCompanions.forEach(function (c) { if (!c.removed) c.translate(d); });
        }
        // Custom anchor point (2026-07: "on déplace un point d'ancrage avec
        // alt et l'on bouge l'objet après ... celui-ci ne se déplace pas
        // avec l'objet") — xformAnchorCustom is stored as an ABSOLUTE
        // world-space [x,y] (placeAnchorAt, above), not derived from the
        // shape's bounds like the 9-dot preset anchor is, so it must be
        // translated explicitly here or it's left stranded at its old
        // position the moment the shape moves out from under it.
        if (p.data && p.data.xformAnchorCustom) {
          p.data.xformAnchorCustom = [p.data.xformAnchorCustom[0] + d.x, p.data.xformAnchorCustom[1] + d.y];
        }
      });
      if (state.xformAnchorCustom) {
        state.xformAnchorCustom = [state.xformAnchorCustom[0] + d.x, state.xformAnchorCustom[1] + d.y];
      }
      symGestureAccumulate(new Matrix().translate(d));
      if (moveGrabLocal) moveGrabLocal = moveGrabLocal.add(d);
      };
      appliquerDelta(delta);
      // Residual correction: aim at the RESULT ON SCREEN, not at the
      // geometry. Only when the layer actually rotates or scales — with a
      // pure translation the pivot drift cancels out exactly (the maths
      // reduce to screen delta == geometry delta), which is why a moved
      // layer always dragged correctly and a rotated one never did.
      // Two passes are enough: each one removes the pivot drift caused by
      // the previous, and the drift is a fraction of the delta, so it
      // converges immediately. Bailing out under half a pixel keeps a
      // normal drag at exactly one pass.
      if (moveGrabLocal && window.SMMotion && SMMotion.layerMotionPointMap) {
        for (var passe = 0; passe < 2; passe++) {
          var mapNow = SMMotion.layerMotionPointMap(state.activeLayerIdx);
          if (!mapNow || !mapNow.fwd || !mapNow.invVec) break;
          var atterri = mapNow.fwd(moveGrabLocal.x, moveGrabLocal.y);
          var ax = (atterri.x !== undefined ? atterri.x : atterri[0]);
          var ay = (atterri.y !== undefined ? atterri.y : atterri[1]);
          var rx = pt.x - ax, ry = pt.y - ay;
          if (Math.abs(rx) < 0.5 && Math.abs(ry) < 0.5) break;
          var cv2 = mapNow.invVec(rx, ry);
          appliquerDelta(new Point(cv2[0], cv2[1]));
        }
      }
    } else if (mode === 'xform-scale') {
      // Geometry-space pointer (NOT reassigning pt — lastPt at the end of
      // this handler must stay world-space).
      var ptS = pt;
      if (xformMap) { var ptgS = xformMap.inv(pt.x, pt.y); ptS = new Point(ptgS[0], ptgS[1]); }
      // Alt = scale from the box's own center instead of the opposite
      // corner/edge — re-evaluated every tick like Shift's proportional
      // lock just below, so pressing/releasing Alt mid-drag re-centers
      // immediately rather than needing a fresh gesture.
      var anchor = (e.altKey && xformCenterAnchor) ? xformCenterAnchor : xformAnchor, dir = xformDir, sx = 1, sy = 1;
      xformActiveAnchor = anchor;
      if (dir === 'nw' || dir === 'ne' || dir === 'sw' || dir === 'se') {
        var origDX = xformOrigHandlePos.x - anchor.x, origDY = xformOrigHandlePos.y - anchor.y;
        var curDX = ptS.x - anchor.x, curDY = ptS.y - anchor.y;
        sx = origDX !== 0 ? curDX / origDX : 1;
        sy = origDY !== 0 ? curDY / origDY : 1;
        // Shift = proportional/aspect-locked scale (UI/UX audit, 2026-07)
        // — Illustrator/Figma/Photoshop convention on a CORNER handle,
        // absent entirely before this. Uses the diagonal distance ratio
        // (direction-agnostic, unlike averaging sx/sy) so it works
        // identically whichever corner or drag direction; each axis keeps
        // its own already-computed sign so dragging a corner PAST the
        // anchor (a legal flip) still flips correctly under lock.
        if (e.shiftKey) {
          var origDiag = Math.sqrt(origDX * origDX + origDY * origDY);
          var curDiag = Math.sqrt(curDX * curDX + curDY * curDY);
          var uniform = origDiag !== 0 ? curDiag / origDiag : 1;
          sx = uniform * (sx < 0 ? -1 : 1);
          sy = uniform * (sy < 0 ? -1 : 1);
        }
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
      // Motion mode: same reasoning as the 'move' branch above — write the
      // per-tick scale STEP into ld.motion.scale instead of the raw
      // geometry. Pivot note: the render transform (computeMotionMat)
      // always scales around bounds-center + the Motion Anchor Point,
      // never around whichever corner/custom pivot this handle drag used
      // — matches AE (a layer's Scale always pivots on its own Anchor
      // Point; repositioning the pivot means moving the anchor, not
      // grabbing a different handle), so the magnitude here is right even
      // though xformAnchor/anchor above isn't the pivot that actually ends
      // up rendering.
      if (state.appMode === 'motion') {
        xformLastSx = sx; xformLastSy = sy;
        var msLi = state.activeLayerIdx;
        var msCur = SMMotion.getLayerValue(msLi, 'scale');
        SMMotion.setLayerValue(msLi, 'scale', [msCur[0] * stepSx, msCur[1] * stepSy]);
        window._sceneVersion++;
        lastPt = pt;
        // feedback #211 — same cheap in-place field sync as the layer-body
        // drag above, needed here too since this writes Scale through the
        // identical setLayerValue path.
        if (window.SMMotion && SMMotion.liveRefreshVisiblePropertyFields) SMMotion.liveRefreshVisiblePropertyFields();
        window.SMEngineBridge.renderNow();
        return;
      }
      selectedPaths.forEach(function (p) {
        p.scale(stepSx, stepSy, anchor);
        if (window.syncParamShapeBoxOnScale) window.syncParamShapeBoxOnScale(p, stepSx, stepSy, anchor);
        transformFillGradient(p, function (gp) {
          return new Point(anchor.x + (gp.x - anchor.x) * stepSx, anchor.y + (gp.y - anchor.y) * stepSy);
        });
        if (p.data && p.data.isVectorBrush && p.data.centerSegments) {
          scaleCenterSegments(p.data.centerSegments, stepSx, stepSy, anchor.x, anchor.y);
          rebuildVectorBrushOutline(p);
        }
        // Texture companions (vector-preset dabs OR Bitmap Brush's raster)
        // never moved with a scale/rotate handle drag before this — only
        // the plain 'move' translate handler touched them at all. Scaling
        // them geometrically with the SAME matrix keeps them visually
        // attached during the drag (cheap); bitmap anchors get a full
        // crisp re-bake at gesture end (onUp below), same "cheap live,
        // crisp on release" precedent as the subselect node-drag path.
        if (p.data && p.data.brushCompanions) {
          p.data.brushCompanions.forEach(function (c) { if (!c.removed) c.scale(stepSx, stepSy, anchor); });
        }
      });
      xformLastSx = sx; xformLastSy = sy;
      symGestureAccumulate(new Matrix().scale(stepSx, stepSy, anchor));
    } else if (mode === 'xform-skew') {
      // Graphite-style skew (2026-08): grabbing an edge midpoint's skew
      // zone shears the box along that edge's own direction, anchored at
      // the OPPOSITE edge (which stays fixed) — top/bottom shear
      // horizontally (x as a function of y), left/right shear vertically
      // (y as a function of x). Shear composes ADDITIVELY for a fixed
      // axis+anchor (unlike scale's multiplicative ratio), so the total
      // amount is recomputed fresh from the drag start every tick (same
      // "total-from-anchor, then diff against last tick" shape as scale's
      // sx/sy) and only the per-tick DELTA is actually applied.
      var ptK = pt;
      if (xformMap) { var ptgK = xformMap.inv(pt.x, pt.y); ptK = new Point(ptgK[0], ptgK[1]); }
      var edgeK = xformSkewEdge;
      var parallelToX = (edgeK === 'n' || edgeK === 's');
      // Dragging the TOP/LEFT edge needs the opposite sign from BOTTOM/
      // RIGHT for the same drag direction to read as the same visual skew
      // (matches Graphite's own sign convention — verified against its
      // source, not re-derived from scratch).
      var signK = (edgeK === 'n' || edgeK === 'w') ? -1 : 1;
      var dxK = ptK.x - xformOrigHandlePos.x, dyK = ptK.y - xformOrigHandlePos.y;
      var shxTotal = 0, shyTotal = 0;
      if (parallelToX) {
        shxTotal = (signK * dxK) / xformSkewPerp;
        // Ctrl = free/biaxial movement (Graphite convention): also lets the
        // OTHER axis shear a little, instead of a pure single-axis shear.
        if (e.ctrlKey) shyTotal = (signK * dyK) / xformSkewPerp;
      } else {
        shyTotal = (signK * dyK) / xformSkewPerp;
        if (e.ctrlKey) shxTotal = (signK * dxK) / xformSkewPerp;
      }
      var stepShx = shxTotal - xformLastShx, stepShy = shyTotal - xformLastShy;
      var anchorK = xformAnchor;
      // Motion mode never reaches 'xform-skew' — hitTestHandles() (its only
      // producer) is only consulted in Animation 2D (see onDown's `hh` gate
      // above), so there's no per-mode branch to write here, unlike scale/
      // rotate which are reachable from Motion's own separate hit-test.
      selectedPaths.forEach(function (p) {
        p.shear(stepShx, stepShy, anchorK);
        transformFillGradient(p, function (gp) {
          var lx = gp.x - anchorK.x, ly = gp.y - anchorK.y;
          return new Point(anchorK.x + lx + stepShx * ly, anchorK.y + ly + stepShy * lx);
        });
        if (p.data && p.data.isVectorBrush && p.data.centerSegments) {
          shearCenterSegments(p.data.centerSegments, stepShx, stepShy, anchorK.x, anchorK.y);
          rebuildVectorBrushOutline(p);
        }
        if (p.data && p.data.brushCompanions) {
          p.data.brushCompanions.forEach(function (c) { if (!c.removed) c.shear(stepShx, stepShy, anchorK); });
        }
      });
      symGestureAccumulate(new Matrix().shear(stepShx, stepShy, anchorK));
      xformLastShx = shxTotal; xformLastShy = shyTotal;
    } else if (mode === 'xform-distort') {
      var ptD = pt;
      if (xformMap) { var ptgD = xformMap.inv(pt.x, pt.y); ptD = new Point(ptgD[0], ptgD[1]); }
      var dstQuad = { nw: distortSrcQuad.nw, ne: distortSrcQuad.ne, se: distortSrcQuad.se, sw: distortSrcQuad.sw };
      dstQuad[distortDir] = ptD;
      distortDstQuad = dstQuad;
      var srcUV = rectUVSolver(distortSrcQuad.nw, distortSrcQuad.ne, distortSrcQuad.sw);
      var dstFwd = unitSquareToQuad(dstQuad.nw, dstQuad.ne, dstQuad.se, dstQuad.sw);
      distortSegs.forEach(function (rec) {
        var uv = srcUV(rec.pt[0], rec.pt[1]);
        var np = dstFwd(uv.u, uv.v);
        rec.seg.point = new Point(np.x, np.y);
        if (rec.hi) {
          var uvHi = srcUV(rec.pt[0] + rec.hi[0], rec.pt[1] + rec.hi[1]);
          var nhi = dstFwd(uvHi.u, uvHi.v);
          rec.seg.handleIn = new Point(nhi.x - np.x, nhi.y - np.y);
        }
        if (rec.ho) {
          var uvHo = srcUV(rec.pt[0] + rec.ho[0], rec.pt[1] + rec.ho[1]);
          var nho = dstFwd(uvHo.u, uvHo.v);
          rec.seg.handleOut = new Point(nho.x - np.x, nho.y - np.y);
        }
      });
      window.SMEngineBridge.renderNow();
    } else if (mode === 'arc') {
      setArcHandle(draggingArc.fA, draggingArc.fB, draggingArc.matchIdx, draggingArc.which, draggingArc.ptA, draggingArc.ptB, pt.x, pt.y);
      renderArcs(arcDragCache);
    } else if (mode === 'xform-rotate') {
      var ptR = pt;
      if (xformMap) { var ptgR = xformMap.inv(pt.x, pt.y); ptR = new Point(ptgR[0], ptgR[1]); }
      var curAngle = Math.atan2(ptR.y - rotCenter.y, ptR.x - rotCenter.x) * 180 / Math.PI;
      var deltaFromStart = curAngle - rotStartAngle;
      // Shift = snap to 15° increments of TOTAL rotation from where the
      // drag started (UI/UX audit, 2026-07) — Illustrator/Figma
      // convention, absent before this. Snapping deltaFromStart (not the
      // raw angle) is what makes it land on clean values relative to the
      // shape's own starting orientation, not clean values in absolute
      // canvas space.
      if (e.shiftKey) deltaFromStart = Math.round(deltaFromStart / 15) * 15;
      var stepAngle = deltaFromStart - rotLastAngle;
      // Motion mode: same reasoning as 'move'/'xform-scale' above — the
      // per-tick angle STEP goes into ld.motion.rotation, raw geometry
      // untouched. Same pivot note as scale: renders around bounds-center
      // + Motion Anchor Point regardless of which corner/custom pivot this
      // particular drag rotated around.
      if (state.appMode === 'motion') {
        rotLastAngle = deltaFromStart;
        var mrLi = state.activeLayerIdx;
        var mrCur = SMMotion.getLayerValue(mrLi, 'rotation');
        SMMotion.setLayerValue(mrLi, 'rotation', [mrCur[0] + stepAngle]);
        window._sceneVersion++;
        lastPt = pt;
        // feedback #211 — same cheap in-place field sync as the layer-body
        // drag above, needed here too since this writes Rotation through the
        // identical setLayerValue path.
        if (window.SMMotion && SMMotion.liveRefreshVisiblePropertyFields) SMMotion.liveRefreshVisiblePropertyFields();
        window.SMEngineBridge.renderNow();
        return;
      }
      selectedPaths.forEach(function (p) {
        p.rotate(stepAngle, rotCenter);
        transformFillGradient(p, function (gp) { return gp.rotate(stepAngle, rotCenter); });
        if (p.data && p.data.isVectorBrush && p.data.centerSegments) {
          rotateCenterSegments(p.data.centerSegments, stepAngle, rotCenter.x, rotCenter.y);
          rebuildVectorBrushOutline(p);
        }
        // See the identical companion-transform comment in xform-scale above.
        if (p.data && p.data.brushCompanions) {
          p.data.brushCompanions.forEach(function (c) { if (!c.removed) c.rotate(stepAngle, rotCenter); });
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
    // See onDown's comment — clears _motionDrag if a motion handle/dot/
    // anchor drag was in progress; no-ops otherwise. Must run even though
    // this file's own `mode` stays null for a motion-path drag (onDown
    // never touched it), or _motionDrag would never get released.
    if (state.appMode === 'motion' && window.SMMotion && SMMotion.onUp()) {
      e.stopImmediatePropagation(); e.preventDefault();
      return;
    }
    // A deferred Shift-toggle (#802) whose gesture never armed a move —
    // Motion's box-fallback leaves mode null, and a pending toggle left
    // behind would fire on some later, unrelated pointerup.
    if (_pendingShiftToggle && mode !== 'move') {
      var _pstIdle = _pendingShiftToggle; _pendingShiftToggle = null;
      _pstIdle.forEach(function (m) { var mi = selectedPaths.indexOf(m); if (mi >= 0) selectedPaths.splice(mi, 1); });
      state.selectedStrokeIndices = selectedPaths.map(getSI).filter(function (i2) { return i2 >= 0; });
      renderArcs(); updateUI();
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
    }
    if (!mode) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    if (_multiLayerDrag && mode.indexOf('layer-multi-') === 0) {
      forEachMultiPath(_multiLayerDrag, function (p, target) {
        forkIfForeignOwner(p); fsUnlinkFillRegen(p); fillRegenerateLinked(target.layer, p);
        if (window.SMBitmapBrush && p.data && p.data.bitmapBrushSpec) SMBitmapBrush.regenerate(p, target.layer);
      });
      saveAllLayerFrames();
      _multiLayerDrag = null; mode = null;
      renderOS(); renderArcs(); updateUI();
      window.SMEngineBridge.resume(); window.SMEngineBridge.renderNow();
      return;
    }
    if (mode === 'nv-drag' || mode === 'nv-scale' || mode === 'nv-rotate') {
      mode = null; nvIdx = -1; nvStartPt = null; nvPivot = null;
      // One panel/timeline refresh at gesture end (not per tick — the
      // Transform fields and Motion rows re-read motionStatic/keys).
      updateUI();
      window.SMEngineBridge.renderNow();
      return;
    }
    if (mode === 'null-drag') {
      mode = null; nullIdx = -1; nullStartPt = null;
      updateUI();
      window.SMEngineBridge.renderNow();
      return;
    }
    if (mode === 'xform-anchor-drag') {
      // Geometry itself is untouched (state.xformAnchorCustom/Key is a UI
      // preference, same as ever) — but placeAnchorAt() now also stamps
      // the choice onto each selected stroke's OWN data (2026-07, so it
      // survives a deselect+reselect), and that per-stroke write needs one
      // saveActiveLayerFrame() to actually reach the frame's persisted
      // JSON, or it would only live on the in-memory Paper object until
      // the next loadFrame() silently discarded it.
      saveActiveLayerFrame();
      mode = null;
      window.SMEngineBridge.resume();
      updateUI();
      window.SMEngineBridge.renderNow();
      return;
    }
    if (mode === 'cornerRadius') {
      _cornerDrag = null;
      mode = null;
      saveActiveLayerFrame();
      window.SMEngineBridge.resume();
      updateUI();
      window.SMEngineBridge.renderNow();
      return;
    }
    if (mode === 'arc') {
      draggingArc = null;
      arcDragCache = null;
      generateTweens(undefined,true);
    } else if (mode === 'xform-scale' || mode === 'xform-rotate' || mode === 'xform-skew') {
      // Motion mode: geometry was never touched during this gesture (see
      // onMove's early-return) — none of the fork/regenerate/save-frame
      // work below applies to anything this drag actually changed, and
      // running it anyway risked corrupting whatever frame the playhead
      // happened to be sitting on (a tween in-between's re-serialized
      // content can come back byte-different from what's stored even with
      // nothing genuinely edited — see saveActiveLayerFrame's
      // _maybePromoteInterpolated — silently flipping it from a generated
      // inbetween to a real keyframe as a side effect of an unrelated
      // Motion drag).
      if (state.appMode === 'motion') {
        mode = null;
        updateUI();
        window.SMEngineBridge.renderNow();
        window.SMEngineBridge.resume();
        return;
      }
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
        selectedPaths = userLayers[state.activeLayerIdx].children.filter(function (c) { return (c instanceof Path || c instanceof Raster) && isSelectablePathChild(c); });
      } else {
        // Team review: a handle drag always means a real transform happened
        // (unlike 'move', which can fire on a plain click) — fork every
        // foreign-owned item in the gesture before it's persisted.
        selectedPaths.forEach(function (p) { forkIfForeignOwner(p); });
        // A fill-bucket result the user just scaled/rotated/distorted THEMSELVES
        // is exactly the "manual edit must stay put" case fsUnlinkFillRegen's
        // own header comment already describes — without this, the very next
        // line's unrestricted fillRegenerateLinked(…,null) re-traces this same
        // path from its OLD, now-stale fillSeed/fillWalls (the walls the user
        // moved may not even be part of them) and silently snaps it back.
        selectedPaths.forEach(function (p) { fsUnlinkFillRegen(p); });
        // touchedPath=null used to mean "check every fill in the layer,
        // unconditionally" — sounds thorough but is actually WEAKER than
        // passing the real path: fillRegenerateLinked's own retry ladder
        // (re-trace from the fill's old seed, then its current interior
        // point, THEN touchedPath's interior point as a last resort) only
        // reaches that last rung when touchedPath is real. A wall stroke
        // dragged just far enough that the fill's old seed no longer lands
        // inside anything hit both earlier rungs and returned null, then
        // silently left the fill exactly where it was — confirmed live: a
        // moved wall circle's bounds updated, the petal fill it used to
        // bound did not budge into the gap that opened up. One call per
        // moved path (not once for the whole gesture) gives every one of
        // them a real shot at being the successful last-resort seed.
        selectedPaths.forEach(function (p) { fillRegenerateLinked(userLayers[state.activeLayerIdx], p); });
        // Bitmap Brush anchors: the live drag above only scaled/rotated the
        // EXISTING raster companion in place (cheap, matches its geometry
        // during the drag but stays at its original bake resolution/
        // stamp density) — re-bake crisp from the anchor's final geometry
        // once, at gesture end, same "cheap live, crisp on release"
        // precedent as liveRestamp/regenerate for subselect node edits.
        if (window.SMBitmapBrush) {
          selectedPaths.forEach(function (p) {
            if (p.data && p.data.bitmapBrushSpec) SMBitmapBrush.regenerate(p, userLayers[state.activeLayerIdx]);
          });
        }
        // Vector text: sync the group's own font metadata after a resize
        // handle drag (feedback #79, "pas de resize de la bounding box").
        // p.scale() above already transformed every glyph's raw geometry —
        // generic, text-agnostic code — but a text group's actual FONT
        // SIZE lives in root.data.size/fixedWidth/letterSpacing/
        // wordSpacing, entirely separate from that geometry. Left unsynced,
        // the resize looked like it worked (glyphs visibly bigger) but any
        // LATER typography-panel edit (rebuildVectorTextFromPopover)
        // rebuilds from the STALE original size, silently snapping back —
        // reproduced live: d.size stayed 48 after a visible drag-scale to
        // ~124% height. anchorTopLeft (this file's own recent fix, same
        // feedback ticket) must go through the identical pivot+scale the
        // glyphs themselves just did, or the NEXT re-edit re-anchors from
        // a stale pre-scale point.
        if (mode === 'xform-scale' && (Math.abs(xformLastSx - 1) > 1e-6 || Math.abs(xformLastSy - 1) > 1e-6)) {
          var _syncedTextRoots = {};
          selectedPaths.forEach(function (p) {
            if (!p.data || !p.data.isVectorText || !p.data.groupId) return;
            var gid = p.data.groupId;
            if (_syncedTextRoots[gid]) return;
            _syncedTextRoots[gid] = true;
            var troot = userLayers[state.activeLayerIdx].children.filter(function (c) { return c.data && c.data.groupId === gid && c.data.isTextRoot; })[0];
            if (!troot) return;
            var avgScale = (Math.abs(xformLastSx) + Math.abs(xformLastSy)) / 2;
            troot.data.size = Math.max(1, (troot.data.size || 48) * avgScale);
            if (troot.data.fixedWidth) troot.data.fixedWidth = troot.data.fixedWidth * Math.abs(xformLastSx);
            if (troot.data.letterSpacing) troot.data.letterSpacing *= Math.abs(xformLastSx);
            if (troot.data.wordSpacing) troot.data.wordSpacing *= Math.abs(xformLastSx);
            if (troot.data.anchorTopLeft) {
              // xformActiveAnchor (not xformAnchor) — the drag may have
              // scaled around the box CENTER on its last tick (Alt held),
              // not the corner captured at gesture start; see its own
              // declaration comment.
              var scaleAnchor = xformActiveAnchor || xformAnchor;
              var apt = new Point(troot.data.anchorTopLeft.x, troot.data.anchorTopLeft.y);
              apt = new Point(scaleAnchor.x + (apt.x - scaleAnchor.x) * xformLastSx, scaleAnchor.y + (apt.y - scaleAnchor.y) * xformLastSy);
              troot.data.anchorTopLeft = { x: apt.x, y: apt.y };
            }
          });
        }
        saveActiveLayerFrame();
        // Reported "trace fantôme" bug (root cause #2, distinct from the
        // team-review fork gated above): onionPrevLayer/onionNextLayer
        // (tweens.js renderOS()) are a snapshot cache, only ever rebuilt on
        // frame nav/layer/project changes — never on a select-tool commit.
        // A held (non-keyframe) frame's onion ghost is generated from
        // getEffectiveStrokes(), which falls back to THIS frame's content
        // when nothing overrides it — so scaling/rotating an object here
        // left every onion-visible neighbor frame showing it at its
        // PRE-drag position/size, indistinguishable from a real duplicate
        // at reduced opacity. Reproduced on a brand-new project (onion skin
        // defaults on) with a single keyframe: drag once, a desaturated
        // copy of the object remains at the old spot until some unrelated
        // action (frame nav, toggling onion) happens to call renderOS().
        renderOS();
      }
      renderArcs(); updateUI();
    } else if (mode === 'xform-distort') {
      selectedPaths.forEach(function (p) { forkIfForeignOwner(p); });
      // See the xform-scale/xform-rotate branch's own comment above — same
      // "manual edit must stay put" unlink, same reason.
      selectedPaths.forEach(function (p) { fsUnlinkFillRegen(p); });
      // Per-path calls, not one null call — see the xform-scale/xform-rotate
      // branch's own comment above for why.
      selectedPaths.forEach(function (p) { fillRegenerateLinked(userLayers[state.activeLayerIdx], p); });
      // Same re-bake as the xform-scale/xform-rotate tail above — a corner-pin
      // distort warps the path's own segments live, but never touched the
      // bitmap-brush raster companion (not a simple scale/rotate, so onMove
      // above leaves it untouched during the drag); without this it stays
      // frozen at its pre-distort shape forever on that stroke.
      if (window.SMBitmapBrush) {
        selectedPaths.forEach(function (p) {
          if (p.data && p.data.bitmapBrushSpec) SMBitmapBrush.regenerate(p, userLayers[state.activeLayerIdx]);
        });
      }
      saveActiveLayerFrame();
      renderOS();
      renderArcs(); updateUI();
      distortDir = null; distortSrcQuad = null; distortSegs = null; distortDstQuad = null;
    } else if (mode === 'marquee') {
      if (_marquee.rect) {
        var mb = _marquee.rect.bounds;
        var lassoPath = null;
        if (_marquee.lasso && _marquee.rect.segments.length > 2) {
          _marquee.rect.closePath();
          lassoPath = _marquee.rect;
        }
        var scanLayerIdx = function (li, allowLockException) {
          var ld = state.layers[li], pl = userLayers[li];
          if (!ld || !pl) return;
          if (ld.locked && !(allowLockException && ld.symbolId)) return;
          if (li !== state.activeLayerIdx && !ld.visible) return; // sibling layers: skip hidden ones, matches the click hit-test's own visible check
          pl.children.forEach(function (c) {
            // A linkedFill backdrop (c.data.isLinkedFillCompanion) is never
            // its own selectable thing — it always moves as part of its
            // parent ribbon's own selectedPaths entry (see that flag's own
            // comment in draw-bridge.js for the double-translate bug this
            // exclusion fixes). Marquee bounds-intersection would otherwise
            // pick it up as a second, independent hit whenever the box
            // covered both.
            if (((c instanceof Path && c.segments.length > 0 && (c.strokeColor || c.fillColor)) || c instanceof Raster) && mb.intersects(c.bounds) && isSelectablePathChild(c)) {
              // Lasso : le test bounds ne suffit pas (le lasso peut serpenter) —
              // l'item doit avoir son centre DANS le trace, ou le croiser.
              if (lassoPath && !(lassoPath.contains(c.position) || (c instanceof Path && lassoPath.intersects(c)))) return;
              // Non-destructive combine groups (2026-07-29): a bounds-only
              // intersection is correct/expected for a REAL marquee drag (a
              // rectangle surrounding a donut's own bounding box legitimately
              // selects it, hole included — standard marquee semantics in
              // every design tool) but wrongly over-selects when onDown's own
              // precise hitTest already missed and this degenerated into a
              // near-zero-size rect — i.e. what was actually just a CLICK on
              // a visually cut-away region. Only that degenerate case gets
              // the extra precision check; a genuine drag is untouched.
              if (!lassoPath && mb.width < 3 / view.zoom && mb.height < 3 / view.zoom && !combineVisibleAt(c, mb.center, li)) return;
              if (selectedPaths.indexOf(c) < 0) selectedPaths.push(c);
            }
          });
        };
        var activeLdForMarqueeLock = state.layers[state.activeLayerIdx];
        // Same lock gate as the click-select hit-test above (and same
        // component exception — a component's own .locked=true must not
        // block marquee-selecting it as a whole either).
        if (!(activeLdForMarqueeLock.locked && !activeLdForMarqueeLock.symbolId)) scanLayerIdx(state.activeLayerIdx, true);
        // Cross-layer marquee INSIDE a component (2026-08, feedback #46:
        // "dans un composant il est impossible de select plusieurs éléments
        // en même temps") — entering a Component auto-splits it into one
        // layer per shape (CLAUDE.md §8, splitLayerIntoElementsCore), so a
        // marquee scoped to only the active layer can structurally never
        // grab more than one shape at a time in there, unlike Animation 2D's
        // ordinary single-layer-holds-many-shapes case this scoping was
        // built for. state.layers IS the symbol's own small layer array
        // while activeSymbolId is set (enterSymbol aliasing) — "every other
        // layer" here means "every other shape of this one component", not
        // the whole outer scene, so this stays bounded to exactly the
        // component being edited.
        if (state.activeSymbolId) {
          state.layers.forEach(function (ld, li) { if (li !== state.activeLayerIdx) scanLayerIdx(li, false); });
        }
        // A selection spanning more than one Paper.js layer has no single
        // per-layer index list to report — same "cross-layer selection ⇒
        // clear the index cache" precedent the component-layer branch above
        // already takes (line ~2090: selectedStrokeIndices=[]).
        var spansLayers = state.activeSymbolId && selectedPaths.some(function (p) { return p.layer !== userLayers[state.activeLayerIdx]; });
        state.selectedStrokeIndices = spansLayers ? [] : selectedPaths.map(getSI).filter(function (i2) { return i2 >= 0; });
        // Footage in a marquee (2026-09, feedback #778: "pblm de selection
        // des footage dans le canvas"). A video layer draws no Paper item at
        // all — getEffectiveStrokes returns [] for it (app.js) and the
        // picture is engine-side — so the children walk above can never see
        // one, and a rubber band drawn right around a video selected
        // nothing while every stroke and image around it got picked up.
        // Same priority rule the click hit-test already states: drawings sit
        // ON TOP of footage, so a marquee that caught any stroke keeps that
        // selection and only an otherwise-empty one falls through to the
        // topmost video it covers. Rotation-aware through the rect's own
        // four corners, matching the click path's own spun-back test.
        if (!selectedPaths.length && window.SMNativeVideo && window.SMMotion) {
          var nvMarq = -1;
          for (var nvj = state.layers.length - 1; nvj >= 0; nvj--) {
            var nvld = state.layers[nvj];
            if (!nvld || !nvld.nativeVideo || !nvld.visible || nvld.locked) continue;
            var nvr2 = SMNativeVideo.displayRect(nvj);
            if (!nvr2) continue;
            var ncx2 = nvr2.x + nvr2.width / 2, ncy2 = nvr2.y + nvr2.height / 2;
            var na2 = (nvr2.rotation || 0) * Math.PI / 180, nc2 = Math.cos(na2), ns2 = Math.sin(na2);
            var corners = [[nvr2.x, nvr2.y], [nvr2.x + nvr2.width, nvr2.y], [nvr2.x + nvr2.width, nvr2.y + nvr2.height], [nvr2.x, nvr2.y + nvr2.height]].map(function (c2) {
              var dx2 = c2[0] - ncx2, dy2 = c2[1] - ncy2;
              return new Point(ncx2 + dx2 * nc2 - dy2 * ns2, ncy2 + dx2 * ns2 + dy2 * nc2);
            });
            var quad = new Path({ segments: corners, closed: true, insert: false });
            var hitMarq = lassoPath
              ? (lassoPath.contains(quad.position) || lassoPath.intersects(quad))
              : mb.intersects(quad.bounds);
            if (hitMarq) { nvMarq = nvj; break; }
          }
          if (nvMarq >= 0) {
            state.activeLayerIdx = nvMarq;
            activateUL(nvMarq);
            syncMotionLayerSelection(nvMarq, false);
            window._nvSelectedLayer = nvMarq; // same gizmo the click path arms
          }
        }
        _marquee.rect.remove(); _marquee.rect = null;
      }
      _marquee.active = false;
      renderArcs(); updateUI();
    } else if (mode === 'move') {
      window._smartGuideLines = null; // the magnet's lines only exist during the drag (#747)
      var didMove = moveStarted;
      moveStarted = false;
      moveGestureOrigin = null; moveAppliedTotal = null;
      // A Shift-click that never became a drag: NOW it means "remove from
      // the selection" (#802). A Shift-DRAG keeps everything selected — the
      // axis lock above has just moved it.
      if (_pendingShiftToggle) {
        var _pst = _pendingShiftToggle; _pendingShiftToggle = null;
        if (!didMove) {
          _pst.forEach(function (m) { var mi = selectedPaths.indexOf(m); if (mi >= 0) selectedPaths.splice(mi, 1); });
          state.selectedStrokeIndices = selectedPaths.map(getSI).filter(function (i2) { return i2 >= 0; });
          renderArcs(); updateUI();
          if (window.SMEngineBridge) SMEngineBridge.renderNow();
        }
      }
      // Motion mode: same reasoning as the xform-scale/xform-rotate guard
      // above — this gesture never touched geometry (onMove's early
      // return), so re-loading/re-saving frame content here would be pure
      // unrelated side effect risk, not a no-op.
      if (state.appMode === 'motion') {
        mode = null;
        updateUI();
        window.SMEngineBridge.renderNow();
        window.SMEngineBridge.resume();
        return;
      }
      var mLd = state.layers[state.activeLayerIdx];
      if (mLd && mLd.symbolId) {
        loadFrame(state.currentFrame);
        selectedPaths = userLayers[state.activeLayerIdx].children.filter(function (c) { return (c instanceof Path || c instanceof Raster) && isSelectablePathChild(c); });
        state.selectedStrokeIndices = [];
      } else {
        // Same fork-on-real-edit guard as xform above, gated on whether a
        // real drag distance was ever seen (moveStarted flips true lazily
        // in onMove — see its own comment) so a plain click-release on
        // someone else's stroke doesn't spawn a spurious identical-geometry
        // ghost.
        if (didMove) selectedPaths.forEach(function (p) { forkIfForeignOwner(p); });
        // Same "manual edit must stay put" unlink as the xform branches above
        // — a fill-bucket result the user just DRAGGED must not have the next
        // line's unrestricted fillRegenerateLinked(…,null) re-trace it from
        // its old fillSeed/fillWalls and silently snap it back to where it
        // was before the drag (confirmed live: bounds moved, the very next
        // render put it right back). Same didMove gate as the fork above —
        // a plain click-release never touched geometry, nothing to unlink.
        if (didMove) selectedPaths.forEach(function (p) { fsUnlinkFillRegen(p); });
        // Per-path calls (not one null call) when something actually moved —
        // see the xform-scale/xform-rotate branch's own comment above for
        // why touchedPath=null can't reach fillRegenerateLinked's own last-
        // resort retry rung. A plain click-release (didMove false) never
        // touched geometry, so the cheap unconditional null call is still
        // fine there — nothing to re-trace from a real touched path anyway.
        if (didMove) selectedPaths.forEach(function (p) { fillRegenerateLinked(userLayers[state.activeLayerIdx], p); });
        else fillRegenerateLinked(userLayers[state.activeLayerIdx], null);
        saveActiveLayerFrame();
        // Same stale-onion-ghost fix as the xform-scale/xform-rotate branch
        // above — a plain move commits through this branch too.
        if (didMove) renderOS();
        // Every sibling commit branch in this handler (xform-scale/rotate,
        // xform-distort, marquee-select) ends with renderArcs()+updateUI() —
        // this one didn't, so the right panel's Position/Size numeric fields
        // (updateSelPropsPanel, timeline.js) kept showing the PRE-drag
        // bounds until some unrelated action happened to trigger a refresh.
        // Confirmed live: dragged a shape, bounds genuinely moved (checked
        // layer.children directly), panel still read the old numbers.
        renderArcs(); updateUI();
      }
    }
    mode = null;
    window.SMEngineBridge.renderNow();
    window.SMEngineBridge.resume();
  }

  // Right-click menu on a canvas object (2026-07) — previously nonexistent:
  // right-clicking a shape did nothing (no listener anywhere on
  // #canvas-area/#drawing-canvas), so the OS/browser's own menu showed
  // instead. Reuses window.showContextMenu (ui.js), the same builder every
  // other right-click menu in the app (layer rows, frame grid, keyframes)
  // already goes through — same {label,shortcut,action,sep} item shape,
  // same flat-list-with-dividers convention Figma/Rive/AE all use for an
  // object-level menu (see UX research: Illustrator's flyout-heavy approach
  // only pays off past ~10 items, not warranted here yet).
  function onContext(e) {
    if (!shouldIntercept()) return;
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var pt = new Point(w[0], w[1]);
    // Ctrl+click free-transform distort (see beginDistort's own comment) —
    // on macOS/WebKit a Ctrl+click is the OS-level secondary-click
    // affordance, so the browser dispatches 'contextmenu' for it same as an
    // actual right-click; there is no separate JS-visible signal to hook
    // earlier than this. Checked independently of onDown's own ctrl+corner
    // branch (not just a flag it sets) because the exact mousedown/
    // contextmenu firing order for a Ctrl+click varies by platform/webview
    // version — this guard covers the gesture whether or not a pointerdown
    // ever reached onDown first.
    if (mode === 'xform-distort') { e.preventDefault(); e.stopImmediatePropagation(); return; }
    if (e.ctrlKey) {
      var distortDirAt = distortEligibleCornerAt(pt);
      if (distortDirAt) {
        e.preventDefault(); e.stopImmediatePropagation();
        window.SMEngineBridge.suspend();
        // Same guard as onDown's ctrl+corner branch — resume immediately on
        // failure instead of relying on an onUp that will never fire (this
        // is a synthetic-mode contextmenu path, no real onUp gesture follows).
        if (!beginDistort(distortDirAt)) window.SMEngineBridge.resume();
        return;
      }
    }
    var layer = userLayers[state.activeLayerIdx];
    var activeLdForLock = state.layers[state.activeLayerIdx];
    // Same hit-testing as onDown (motion-transform-aware point, component-
    // layer whole-instance click, brush-anchor resolution) so right-click
    // selects exactly what a left-click would.
    var hitPt = pt;
    // No longer gated on Motion mode (2026-08-31): buildSceneJson applies a
    // layer's motionMat in EITHER mode (CLAUDE.md §8), so an animated layer
    // renders moved in Animation 2D too — and there the click was still
    // tested against the un-moved geometry, so nothing could be selected
    // where it visibly sits. Both call sites (left click and right click)
    // get it, or the two would disagree about what is under the cursor.
    if (window.SMMotion) {
      var hitMap = SMMotion.layerMotionPointMap(state.activeLayerIdx);
      // 3D layers (2026-07-29 fix) — layerMotionPointMap returns null for a
      // 3D-toggled layer even with real rotationX/rotationY set (it only
      // recognizes the base 2D properties); layerMotion3DPointMap is the
      // dedicated perspective-correct counterpart — see its own header
      // comment in motion.js for the ray-plane-intersection math.
      if (!hitMap && SMMotion.layerMotion3DPointMap) hitMap = SMMotion.layerMotion3DPointMap(state.activeLayerIdx);
      if (hitMap) { var hg = hitMap.inv(pt.x, pt.y); hitPt = new Point(hg[0], hg[1]); }
    }
    // Per-ELEMENT transforms on top of the layer's (2026-08-31): hitTestPosed
    // undoes each child's own Motion for that child alone, which a single
    // layer.hitTest cannot do — it tests one point against geometry that
    // several independent transforms have moved. Measured in Animation 2D
    // with the layer transformed AND the two elements dragged apart:
    // clicking either at its visible position selected nothing. hitPt
    // (layer-space) stays the fallback, and the posed test hands back the
    // point in the element's own space for combineHitConfirm.
    var posedDispo = !!window.hitTestPosed && !(activeLdForLock.locked && !activeLdForLock.symbolId);
    var posedHit = posedDispo ? hitTestPosed(state.activeLayerIdx, pt, 8 / view.zoom) : null;
    // No raw fallback once the posed test has spoken (2026-08-31): it tests
    // the shapes where they are DRAWN, so "no hit" means the cursor really is
    // over nothing on this layer. Falling back to layer.hitTest(hitPt) then
    // re-tested the same click against the layer's UN-posed geometry, which
    // for a transformed layer sits somewhere else entirely — a click on
    // another layer's shape matched a shape of the active layer and selected
    // it instead. That is the "il select le premier groupe" report. The raw
    // test only remains for the case where the posed one could not run.
    var hit = posedDispo ? posedHit : ((activeLdForLock.locked && !activeLdForLock.symbolId) ? null : layer.hitTest(hitPt, { stroke: true, fill: true, tolerance: 8 / view.zoom }));
    var confirmPt = (posedHit && posedHit.localPoint) ? posedHit.localPoint : hitPt;
    if (hit && !combineHitConfirm(hit, confirmPt, state.activeLayerIdx)) hit = null;
    var clickedPath = null;
    if (hit && (hit.item instanceof Path || hit.item instanceof Raster)) {
      clickedPath = resolveBrushAnchor(hit.item, layer);
    } else {
      var compHit = hitTestComponentLayers(pt);
      if (compHit) {
        state.activeLayerIdx = compHit.layerIdx; activateUL(compHit.layerIdx);
        selectedPaths = userLayers[compHit.layerIdx].children.filter(function (c) { return (c instanceof Path || c instanceof Raster) && isSelectablePathChild(c); });
        state.selectedStrokeIndices = selectedPaths.map(getSI).filter(function (i) { return i >= 0; });
        renderArcs(); updateUI(); window.SMEngineBridge.renderNow();
      }
    }
    // Right-click an item already part of the current multi-selection keeps
    // the whole selection (matches Figma/Illustrator); right-clicking
    // anything else replaces it with just that item, same as a plain click.
    if (clickedPath && selectedPaths.indexOf(clickedPath) < 0) {
      clearSel();
      var rcSet = window.SMGroup ? SMGroup.membersOf(clickedPath, layer) : [clickedPath];
      rcSet.forEach(function (m) { selectedPaths.push(m); });
      state.selectedStrokeIndices = selectedPaths.map(getSI).filter(function (i) { return i >= 0; });
      renderArcs(); updateUI(); window.SMEngineBridge.renderNow();
    }
    if (!selectedPaths.length) {
      // Empty canvas: nothing to duplicate/delete, but a right-click here is
      // still the standard place to offer Paste (2026-07, "vérifie que
      // copier/couper/coller existe pour tous les éléments") — only shown
      // when there's actually something in the canvas clipboard, otherwise
      // fall through to the native menu exactly as before.
      var emptyItems = [];
      if (_canvasClip && _canvasClip.snaps && _canvasClip.snaps.length) {
        emptyItems.push({ label: 'Coller', shortcut: '⌘V', action: function () { pasteSelection(); } });
      }
      // Tracked-role proximity suggestion (2026-08-29, feedback #151, design
      // point 4 — the secondary assist on top of the core tag/attach
      // mechanic above). Offered from the empty-canvas menu specifically
      // BECAUSE nothing needs to be pre-selected here: it exists to help
      // the user FIND which shape on this frame is the role's occupant
      // when it isn't obvious, not to act on an already-chosen shape.
      // Advisory only — selects the candidate (visible via the normal
      // selection highlight) and asks for a confirming action; never tags
      // anything on its own.
      var roleSuggestions = collectRoleSuggestions();
      if (roleSuggestions.length) {
        if (emptyItems.length) emptyItems.push({ sep: true });
        roleSuggestions.forEach(function (s) {
          emptyItems.push({
            label: 'Suggérer la forme pour « ' + s.name + ' »…',
            action: function () { applyRoleSuggestion(s); }
          });
        });
      }
      if (!emptyItems.length) return;
      e.preventDefault(); e.stopImmediatePropagation();
      window.showContextMenu(e.clientX, e.clientY, emptyItems);
      return;
    }
    e.preventDefault(); e.stopImmediatePropagation();
    var multi = selectedPaths.length > 1;
    var p0 = selectedPaths[0];
    var isDeleteGhost = !multi && p0.data && p0.data.isRevisionGhost && p0.data.revisionAction === 'delete';
    var isActiveRevision = !multi && p0.data && p0.data.revisionParentId && !p0.data.isRevisionGhost;
    var isGrouped = !multi && p0.data && p0.data.groupId;
    var hasCanvasClip = !!(_canvasClip && _canvasClip.snaps && _canvasClip.snaps.length);
    var items = [
      { label: 'Copier', shortcut: '⌘C', action: function () { copySelection(); } },
      { label: 'Couper', shortcut: '⌘X', action: function () { cutSelection(); } },
      { label: 'Coller', shortcut: '⌘V', disabled: !hasCanvasClip, action: function () { pasteSelection(); } },
      { label: 'Dupliquer', shortcut: '⌘D', action: function () { duplicateSelection(); } },
      { label: multi ? 'Supprimer la sélection' : 'Supprimer', shortcut: 'Suppr', action: function () { window.SM.deleteSelStrokes(); } },
      { sep: true },
    ];
    if (multi || isGrouped) {
      items.push({ label: isGrouped ? 'Dissocier' : 'Grouper', shortcut: isGrouped ? '⇧⌘G' : '⌘G', action: function () { if (window.SMGroup) { if (isGrouped) SMGroup.ungroupSelection(); else SMGroup.groupSelection(); } } });
      items.push({ sep: true });
    }
    // Non-destructive combine groups (2026-07-29) — "Combiner (Union)" is
    // the ONE flat entry offered here (discoverability over a 4-way
    // submenu — this app's context menu is flat-list only, and Union is by
    // far the common case; the other 3 modes stay reachable via Alt+click
    // on their own toolbar icon). If the current selection already IS one
    // active combine-group's full membership, offer changing its mode/
    // removing a member/flattening instead of creating a new one.
    var _cbLayer = userLayers[state.activeLayerIdx], _cbLd = state.layers[state.activeLayerIdx];
    var _cbGid = null;
    if (window.SMGroup && _cbLd && _cbLd.groups) {
      if (!multi && p0.data && p0.data.groupId && _cbLd.groups[p0.data.groupId]) _cbGid = p0.data.groupId;
      else if (multi) {
        var _cbFirst = selectedPaths[0].data && selectedPaths[0].data.groupId;
        if (_cbFirst && _cbLd.groups[_cbFirst]) {
          var _cbMembers = SMGroup.resolveGroupMembers(_cbFirst, _cbLd, _cbLayer);
          if (_cbMembers.length === selectedPaths.length && _cbMembers.every(function (m) { return selectedPaths.indexOf(m) >= 0; })) _cbGid = _cbFirst;
        }
      }
    }
    var _cbActive = (_cbGid && _cbLd.groups[_cbGid].combineMode !== 'none') ? _cbGid : null;
    if (window.SMGroup && multi && !_cbActive) {
      items.push({ label: 'Combiner (Union) — non destructif', action: function () { SMGroup.combineSelection('unite'); } });
      items.push({ sep: true });
    }
    if (_cbActive) {
      var _cbCurMode = _cbLd.groups[_cbActive].combineMode;
      var _cbLabels = { unite: 'Union', subtract: 'Soustraction', intersect: 'Intersection', exclude: 'Exclusion' };
      ['unite', 'subtract', 'intersect', 'exclude'].forEach(function (m) {
        items.push({ label: (m === _cbCurMode ? '✓ ' : '') + _cbLabels[m], action: function () { SMGroup.setGroupCombineMode(_cbActive, _cbLd, m); } });
      });
      items.push({ sep: true });
      if (!multi) items.push({ label: 'Sortir du groupe', action: function () { SMGroup.removeMemberFromGroup(p0, _cbLd, _cbLayer); } });
      items.push({ label: 'Aplatir', action: function () { SMGroup.flattenGroup(_cbActive, _cbLd, _cbLayer); } });
      items.push({ sep: true });
    }
    items = items.concat([
      {
        label: 'Premier plan', action: function () {
          pushUndo();
          selectedPaths.forEach(function (p) { p.bringToFront(); });
          saveActiveLayerFrame(); window.SMEngineBridge.renderNow();
        }
      },
      {
        label: 'Arrière-plan', action: function () {
          pushUndo();
          // Reverse order so the visual stacking order among the selected
          // items themselves is preserved once they're all sent to the back.
          for (var i = selectedPaths.length - 1; i >= 0; i--) selectedPaths[i].sendToBack();
          saveActiveLayerFrame(); window.SMEngineBridge.renderNow();
        }
      },
      { sep: true },
      // Quick reset for the rotate/scale pivot (2026-07, "comment change
      // t'on l'anchor point de place ?" — the drag gesture itself is
      // Alt+drag the anchor crosshair, or Alt+click anywhere on the
      // selection to relocate it there; this menu item is the fast way
      // back to the default without having to Alt+click precisely on the
      // shape's own center).
      {
        label: 'Centrer le point d\'ancrage', action: function () {
          state.xformAnchorCustom = null; state.xformAnchorKey = 'mc';
          selectedPaths.forEach(function (p) { if (p && p.data) { p.data.xformAnchorKey = 'mc'; delete p.data.xformAnchorCustom; } });
          saveActiveLayerFrame();
          if (window.renderXformAnchorGrid) renderXformAnchorGrid();
          updateUI(); window.SMEngineBridge.renderNow();
        }
      },
      // Image mesh (2026-08-30) — the same two toggles the Image mesh panel
      // section carries, offered where you are already looking when you want
      // them: on the image itself. Both route through SMImageMeshUI, which
      // drives the panel's own toggleMesh/`editing`, so the menu and the
      // checkboxes can never drift apart. The whole block is skipped unless
      // exactly one imported image is selected — canMesh() is the panel's own
      // singleRaster() guard, so the menu offers this in precisely the cases
      // the panel appears in, and never on a shape or a multi-selection.
      // Mask an image with a shape you drew (2026-08-30) — a DIFFERENT
      // selection shape from the block below (one image + one path, vs
      // exactly one image), so it is its own guard rather than another entry
      // inside that one.
      ...(window.SMImageMeshUI && SMImageMeshUI.canMaskWithShape() ? [
        { sep: true },
        {
          label: SM.t('ctxMaskImageWithShape'),
          action: function () { SMImageMeshUI.maskImageWithShape(); },
        },
      ] : []),
      ...(window.SMImageMeshUI && SMImageMeshUI.canMesh() ? [
        { sep: true },
        {
          label: (SMImageMeshUI.hasMesh() ? '✓ ' : '') + SM.t('ctxImageMesh'),
          action: function () { SMImageMeshUI.toggleMesh(); },
        },
        {
          label: (SMImageMeshUI.isEditing() ? '✓ ' : '') + SM.t('ctxImageMeshEditPoints'),
          disabled: !SMImageMeshUI.hasMesh(),
          action: function () { SMImageMeshUI.toggleEditing(); },
        },
      ] : []),
      { sep: true },
      {
        // 2026-07 feedback ("tween seulement des éléments select... avec le
        // clic droit sur les éléments select" — corrected after an inverted
        // first attempt, "le but n'était pas d'empêcher au clic droit le
        // tween... mais de justement tween la sélection"): per-element
        // tween OPT-IN, toggled here rather than a stopwatch/keyframe-level
        // setting since several elements of the SAME keyframe can be
        // tweened or not, independently. This does NOT change the default
        // behavior anywhere else — a keyframe pair only enters "manual"
        // mode the first time this is used on it, and reverts to fully
        // automatic once no element in it is flagged anymore (see
        // toggleTweenOnForSelection). Label reflects current state.
        label: selectedPaths.every(function (p) { return p.data && p.data.tweenOn; }) ? 'Retirer du tween' : 'Tweener la sélection',
        action: function () { toggleTweenOnForSelection(); }
      },
    ]);
    // Tracked-role attach (2026-08-29, feedback #151) — hand-drawn
    // Animation 2D "attach an object to another that has no stable identity
    // across frames" workflow (a ball redrawn fresh every frame, a hat
    // riding along) — see state.trackRoles' own comment (app.js) for the
    // full confirmed design. Single-shape only: tagging/attaching targets
    // exactly one stroke, offered here the same way per-element tween
    // opt-in is just above. Path only for v1 (not Raster/CompoundPath) —
    // matches the confirmed scenario (a hand-drawn vector stroke), a
    // deliberate scope cut, not an oversight.
    if (!multi && p0 instanceof Path) {
      items.push({ sep: true });
      items.push({
        label: (p0.data && p0.data.trackRoleId) ? ('Rôle suivi : « ' + p0.data.trackRoleId + ' » (renommer…)') : 'Marquer comme rôle suivi…',
        action: function () { tagSelectionAsRole(p0); }
      });
      items.push({
        label: (p0.data && p0.data.attachedToRoleId) ? ('Attaché à « ' + p0.data.attachedToRoleId + ' » (changer…)') : 'Attacher à un rôle…',
        action: function () { openAttachRolePicker(p0, e.clientX, e.clientY); }
      });
    }
    if (isActiveRevision || isDeleteGhost) {
      // Same actions as the Properties-panel Accept/Reject buttons
      // (timeline.js updateRevisionPanel) — surfacing them here too so a
      // reviewer doesn't have to hunt for the panel just to resolve a
      // correction they just right-clicked.
      items.push({ sep: true });
      items.push({
        label: 'Accepter la correction', action: function () {
          pushUndo();
          if (isDeleteGhost) acceptDeleteRevision(p0); else acceptRevision(p0, userLayers[state.activeLayerIdx]);
          clearSel(); saveActiveLayerFrame(); updateUI();
        }
      });
      items.push({
        label: 'Rejeter la correction', action: function () {
          pushUndo();
          if (isDeleteGhost) rejectDeleteRevision(p0); else rejectRevision(p0, userLayers[state.activeLayerIdx]);
          clearSel(); saveActiveLayerFrame(); updateUI();
        }
      });
    }
    // Stroke profiles (Sander van Dijk 6.2). Offered on any selection that
    // holds at least one open path: a profile turns a flat stroke into a
    // variable-width filled ribbon, which is also what makes a gradient run
    // ALONG the stroke rather than across it (his 6.3, free once the stroke
    // is a fill).
    var profilable = (window.selectedPaths || []).some(function (p) { return p && p.segments && p.segments.length >= 2; });
    if (profilable) {
      items.push({ sep: true });
      // Feedback #102 ("comme dans Animate... convertir les lines en fill,
      // pour le confort de dessin"): the underlying conversion already
      // existed as the "even" stroke profile below (a constant-width
      // profile turns a stroke into a same-width filled ribbon, in place —
      // same machinery, zero new geometry code), but nothing in the menu
      // read as "convert to fill" to someone looking for that specific,
      // named Animate command. Adding the Animate-style label as its own
      // top-level entry rather than renaming the profile option — a
      // constant-width profile and "convert to fill" are the same operation
      // here, but users coming from the tapering submenu still want that
      // wording, so both stay.
      items.push({ label: 'Convertir le trait en remplissage (Fill)', action: function () { window.SM.applyStrokeProfile('even'); } });
      items.push({ label: 'Profil de contour :', disabled: true, action: function () {} });
      [['taper-both', '   • effilé aux deux bouts'],
       ['taper-in', '   • effilé au début'],
       ['taper-out', '   • effilé à la fin'],
       ['bulge', '   • renflé au centre'],
       ['even', '   • épaisseur constante (ruban)']].forEach(function (o) {
        items.push({ label: o[1], action: function () { window.SM.applyStrokeProfile(o[0]); } });
      });
    }
    // Text Animator (2026-08-17, text-animator.js) — right-click access
    // straight on the canvas, same shortcut every other single-item
    // action here already gets, instead of only being reachable through
    // the right-panel text section. Same eligibility check as that panel
    // (groupIdForItem): a vector-text glyph, or an already-split raster
    // character — a whole not-yet-split raster block still has nothing
    // to group into units.
    if (selectedPaths.length === 1 && window.SMTextAnimator) {
      var animGid = window.SMTextAnimator.groupIdForItem(p0);
      if (animGid) {
        items.push({ sep: true });
        items.push({ label: 'Animer le texte…', action: function () { window.SMTextAnimator.openPanel(state.activeLayerIdx, animGid); } });
      }
    }
    // Component exposed properties (2026-08-18, "réutilisation dynamique de
    // component... modifier des properties au dessus" — Figma Component
    // Properties + AE Master Properties synthesis, confirmed with Cyril).
    // Only makes sense while EDITING INSIDE a symbol (state.activeSymbolId —
    // enterSymbol/enterComponentLayer set it, exitToScene clears it) on a
    // single shape with a real strokeId (multi-select or a not-yet-tagged
    // item has nothing stable to bind to). v1 scope: opacity (number) and
    // visibility (boolean, stored 0/100 like every other Motion scalar) —
    // color is a natural v2 (needs its own swatch row; these two reuse the
    // fully generic Transform row renderer with zero new UI code, see
    // propsFor's own comment, motion.js).
    if (state.activeSymbolId && selectedPaths.length === 1 && p0.data && p0.data.strokeId && window.SM && window.SM.exposeSymbolProperty) {
      items.push({ sep: true });
      items.push({
        label: 'Exposer l\'opacité comme propriété de Component…', action: function () {
          var label = prompt('Nom de cette propriété (visible dans le panneau Motion de chaque instance) :', 'Opacité');
          if (!label) return;
          pushUndo();
          window.SM.exposeSymbolProperty(state.activeSymbolId, p0.data.strokeId, 'opacity', label, Math.round((p0.opacity !== undefined ? p0.opacity : 1) * 100));
          showToast('Propriété "' + label + '" exposée sur ce Component.');
        }
      });
      items.push({
        label: 'Exposer la visibilité comme propriété de Component…', action: function () {
          var label = prompt('Nom de cette propriété (visible dans le panneau Motion de chaque instance) :', 'Visible');
          if (!label) return;
          pushUndo();
          window.SM.exposeSymbolProperty(state.activeSymbolId, p0.data.strokeId, '__visible', label, 100);
          showToast('Propriété "' + label + '" exposée sur ce Component.');
        }
      });
    }
    window.showContextMenu(e.clientX, e.clientY, items);
  }

  // 2026-07 feedback ("tween seulement des éléments select... avec le clic
  // droit sur les éléments select et laisser les autres non tween...
  // plusieurs éléments d'une même keyframe peuvent être tween ou pas") —
  // per-element tween OPT-IN, wired to the context-menu item above. Sets
  // data.tweenOn (persisted via serP/desP, app.js) on the live selection,
  // propagates the SAME flag value to the nearest keyframe on each side
  // sharing the same data.strokeId (without this, splitTweenables,
  // tweens.js, would only see the flag on ONE side of the pair, leaving
  // its counterpart to fade in/out as an unmatched stroke instead of
  // staying frozen), then flips ld.frames[<keyframe>].tweenManualMode ON
  // for any bracketing keyframe that now has at least one flagged stroke,
  // or back OFF if none do anymore — this is what keeps the feature "à
  // part, activable au besoin" (per the user's explicit choice): a
  // keyframe pair nobody ever right-clicks here behaves EXACTLY as before,
  // full auto-match, zero change in behavior.
  function toggleTweenOnForSelection() {
    if (!selectedPaths.length) return;
    pushUndo();
    var newVal = !selectedPaths.every(function (p) { return p.data && p.data.tweenOn; });
    var li = state.activeLayerIdx, ld = state.layers[li], cf = state.currentFrame;
    var ids = [];
    selectedPaths.forEach(function (p) {
      if (!(p instanceof Path)) return;
      ensureStrokeId(p);
      if (newVal) p.data.tweenOn = true; else delete p.data.tweenOn;
      if (p.data && p.data.strokeId) ids.push(p.data.strokeId);
    });
    saveActiveLayerFrame();
    function recomputeManualMode(fi) {
      var fr = ld.frames[fi];
      if (!fr || !fr.isKeyframe) return;
      fr.tweenManualMode = !!(fr.strokes && fr.strokes.some(function (sd) { return sd.tweenOn; }));
    }
    function propagate(dir) {
      if (!ids.length) return;
      for (var fi = cf + dir; fi >= 0 && fi < state.totalFrames; fi += dir) {
        var fr = ld.frames[fi];
        if (!fr) continue;
        if (fr.strokes && fr.strokes.length) {
          fr.strokes.forEach(function (sd) {
            if (sd.strokeId && ids.indexOf(sd.strokeId) >= 0) { if (newVal) sd.tweenOn = true; else delete sd.tweenOn; }
          });
        }
        if (fr.isKeyframe) { recomputeManualMode(fi); break; } // stop at the first keyframe reached — that's the pair boundary
      }
    }
    recomputeManualMode(cf); // cf itself, if it's a keyframe (the ids we just set live there)
    propagate(-1); propagate(1);
    if (window.SM && window.SM.generateTweens) window.SM.generateTweens();
    updateUI(); window.SMEngineBridge.renderNow();
  }

  // ---- TRACKED-ROLE ATTACH (2026-08-29, feedback #151) ----
  // Hand-drawn Animation 2D "attach an object to another that has no stable
  // identity across frames" — see state.trackRoles' own comment (app.js)
  // for the full confirmed design (points 1-4 of the feature brief). Three
  // pieces: tagSelectionAsRole (point 1+3 — mark a shape as this frame's
  // role occupant, which ALSO triggers the one-time reposition of every
  // attached shape), attachSelectionToRole (point 2 — freeze an offset to
  // an existing role's CURRENT occupant), and the suggestion pair
  // (collectRoleSuggestions/applyRoleSuggestion, point 4 — advisory only).

  // Tags `p0` as the current-frame occupant of a named role. Un-tags
  // whatever OTHER stroke (any layer, this frame) currently carries that
  // role — "a role has one occupant per frame" is enforced this way rather
  // than blocked outright (confirmed design: "tagging a new stroke with a
  // role that's already occupied un-tags the previous occupant"). Then
  // repositions every stroke (any layer) whose data.attachedToRoleId
  // matches, by its own frozen data.attachOffset relative to p0's NEW
  // bounds center — a ONE-TIME bake into this frame's geometry, not a
  // live link (confirmed design point 3): once done, the attached shape is
  // plain geometry the artist can hand-adjust with nothing re-triggering.
  // A "held" (non-keyframe, non-tween) frame reads the nearest PRIOR
  // keyframe's stroke array BY REFERENCE (getEffectiveStrokes, app.js) — a
  // live edit made while sitting on one has nowhere of its own to persist
  // into, and saveActiveLayerFrame/saveAllLayerFrames both skip a frame
  // that's neither isKeyframe nor isInterpolated outright. Promoting it
  // first (same idea as _maybePromoteInterpolated's tween-in-between
  // promotion, tweens.js, just applied to a plain hold too) is what makes
  // the reposition/tag actually stick past the next frame navigation
  // instead of visually moving for one frame and silently reverting.
  function _ensureLiveFrameIsKeyframe(li) {
    var fr = state.layers[li] && state.layers[li].frames && state.layers[li].frames[state.currentFrame];
    if (fr && !fr.isKeyframe) { fr.isKeyframe = true; fr.isInterpolated = false; }
  }
  function tagSelectionAsRole(p0) {
    if (!(p0 instanceof Path)) return;
    var current = (p0.data && p0.data.trackRoleId) || '';
    var name = prompt('Nom du rôle suivi (ex. « Balle ») — réutilise un nom existant pour continuer ce rôle :', current);
    if (name == null) return; // cancelled
    name = name.trim();
    if (!name) return;
    var roleId = name; // name IS the id — see state.trackRoles' own comment
    pushUndo(); // flushes every layer's live geometry to stored frame data + snapshots pre-state
    if (!state.trackRoles) state.trackRoles = {};
    if (!state.trackRoles[roleId]) state.trackRoles[roleId] = { name: name };
    var newCenter = p0.bounds.center;
    var attachedCount = 0;
    for (var i = 0; i < state.layers.length; i++) {
      if (!layerIsEffectivelyVisible(i)) continue;
      var lyr = userLayers[i];
      if (!lyr) continue;
      (function (li) {
        lyr.children.slice().forEach(function (c) {
          if (c === p0) return;
          if (c.data && c.data.trackRoleId === roleId) delete c.data.trackRoleId;
          if (c instanceof Path && c.data && c.data.attachedToRoleId === roleId && c.data.attachOffset) {
            var target = newCenter.add(new Point(c.data.attachOffset.x, c.data.attachOffset.y));
            var delta = target.subtract(c.bounds.center);
            if (Math.abs(delta.x) > 0.01 || Math.abs(delta.y) > 0.01) {
              _ensureLiveFrameIsKeyframe(li);
              c.translate(delta);
            }
            attachedCount++;
          }
        });
      })(i);
    }
    // Safety net for the edge case of tagging a shape that's currently
    // sitting on a HELD (non-keyframe) frame of its own layer — see
    // _ensureLiveFrameIsKeyframe's own comment. The confirmed core scenario
    // (redrawn fresh every frame) already makes p0's frame a real keyframe
    // by the time it's drawn, so this is a robustness net, not the common
    // path.
    _ensureLiveFrameIsKeyframe(userLayers.indexOf(p0.layer));
    if (!p0.data) p0.data = {};
    p0.data.trackRoleId = roleId;
    saveAllLayerFrames();
    updateUI(); renderArcs();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    showToast('Rôle « ' + name + ' » marqué sur cette frame' + (attachedCount ? (' — ' + attachedCount + ' objet(s) repositionné(s)') : '') + '.');
  }

  // Opens a flat picker (reusing showContextMenu the same way a submenu
  // would) listing every defined role — clicking one attaches p0 to it.
  function openAttachRolePicker(p0, x, y) {
    var roles = state.trackRoles || {};
    var roleIds = Object.keys(roles);
    if (!roleIds.length) { showToast(SM.t('hsNoTrackedRole')); return; }
    roleIds.sort(function (a, b) { return (roles[a].name || a).localeCompare(roles[b].name || b); });
    var pickItems = roleIds.map(function (rid) {
      var isCurrent = !!(p0.data && p0.data.attachedToRoleId === rid);
      return { label: (isCurrent ? '✓ ' : '') + (roles[rid].name || rid), action: function () { attachSelectionToRole(p0, rid); } };
    });
    window.showContextMenu(x, y, pickItems);
  }

  // Freezes offset = p0.bounds.center - roleShape.bounds.center at THIS
  // instant (confirmed design point 2). Errors clearly if the role has no
  // occupant in the CURRENT frame — "you can't attach to a role that isn't
  // present right now". Cross-layer by design: the role's shape and the
  // attached shape can be on the same layer or different ones (both
  // confirmed cases), so this scans every effectively-visible layer's
  // live children, not just the active layer.
  function attachSelectionToRole(p0, roleId) {
    if (!(p0 instanceof Path)) return;
    var roleShape = null;
    for (var i = 0; i < state.layers.length && !roleShape; i++) {
      if (!layerIsEffectivelyVisible(i)) continue;
      var lyr = userLayers[i];
      if (!lyr) continue;
      for (var j = 0; j < lyr.children.length; j++) {
        var c = lyr.children[j];
        if (c !== p0 && c.data && c.data.trackRoleId === roleId) { roleShape = c; break; }
      }
    }
    var roleName = (state.trackRoles && state.trackRoles[roleId] && state.trackRoles[roleId].name) || roleId;
    if (!roleShape) {
      showToast('Le rôle « ' + roleName + ' » n\'a pas d\'occupant sur cette frame — impossible d\'attacher ici.');
      return;
    }
    pushUndo();
    // Safety net (see _ensureLiveFrameIsKeyframe's own comment) for
    // attaching while sitting on a HELD frame of p0's own layer.
    _ensureLiveFrameIsKeyframe(userLayers.indexOf(p0.layer));
    var off = p0.bounds.center.subtract(roleShape.bounds.center);
    if (!p0.data) p0.data = {};
    p0.data.attachedToRoleId = roleId;
    p0.data.attachOffset = { x: off.x, y: off.y };
    saveAllLayerFrames();
    updateUI(); renderArcs();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    showToast('Attaché au rôle « ' + roleName + ' ».');
  }

  // ---- Proximity-suggestion assist (design point 4, secondary) ----
  // Approximate center from a STORED stroke dict's on-curve points (no live
  // Path needed — this only ever looks at PAST frames, which aren't
  // materialized). Good enough for a "closest candidate" heuristic; the
  // actual reposition math above always uses real .bounds.center on live
  // geometry, never this approximation.
  function _strokeDictCenter(sd) {
    if (!sd || !sd.segments || !sd.segments.length) return null;
    var minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    sd.segments.forEach(function (s) {
      var x = s.point[0], y = s.point[1];
      if (x < minX) minX = x; if (x > maxX) maxX = x;
      if (y < minY) minY = y; if (y > maxY) maxY = y;
    });
    if (minX > maxX) return null;
    return new Point((minX + maxX) / 2, (minY + maxY) / 2);
  }
  // Nearest PRIOR frame (any layer) that had an occupant for this role,
  // searching backwards from the frame just before the current one.
  function _findPriorRoleCenter(roleId) {
    for (var fi = state.currentFrame - 1; fi >= 0; fi--) {
      for (var li = 0; li < state.layers.length; li++) {
        var ld = state.layers[li];
        var fr = ld && ld.frames && ld.frames[fi];
        if (!fr || !fr.strokes) continue;
        for (var k = 0; k < fr.strokes.length; k++) {
          if (fr.strokes[k].trackRoleId === roleId) {
            var c = _strokeDictCenter(fr.strokes[k]);
            if (c) return c;
          }
        }
      }
    }
    return null;
  }
  // Every defined role that has NO occupant on the CURRENT frame yet, but
  // DOES have one on some earlier frame — i.e. every role worth suggesting
  // for right now.
  function collectRoleSuggestions() {
    var out = [];
    var roles = state.trackRoles || {};
    Object.keys(roles).forEach(function (rid) {
      var hasNow = false;
      for (var i = 0; i < state.layers.length && !hasNow; i++) {
        if (!layerIsEffectivelyVisible(i)) continue;
        var lyr = userLayers[i];
        if (!lyr) continue;
        hasNow = lyr.children.some(function (c) { return c.data && c.data.trackRoleId === rid; });
      }
      if (hasNow) return;
      var priorCenter = _findPriorRoleCenter(rid);
      if (!priorCenter) return;
      out.push({ roleId: rid, name: roles[rid].name || rid, center: priorCenter });
    });
    return out;
  }
  // Closest LIVE Path (any effectively-visible layer, current frame) to a
  // world-space point.
  function _findClosestPathToPoint(pt) {
    var best = null, bestD = Infinity;
    for (var i = 0; i < state.layers.length; i++) {
      if (!layerIsEffectivelyVisible(i)) continue;
      var lyr = userLayers[i];
      if (!lyr) continue;
      lyr.children.forEach(function (c) {
        if (!(c instanceof Path) || !c.segments.length) return;
        var d = c.bounds.center.getDistance(pt);
        if (d < bestD) { bestD = d; best = c; }
      });
    }
    return best;
  }
  // Advisory only — selects the closest candidate (visible via the normal
  // selection highlight) and prompts the user to confirm via the usual
  // "Marquer comme rôle suivi…" action. NEVER tags anything by itself.
  function applyRoleSuggestion(s) {
    var cand = _findClosestPathToPoint(s.center);
    if (!cand) { showToast(SM.t('hsNoShapeOnFrame')); return; }
    clearSel();
    selectedPaths = [cand];
    state.selectedStrokeIndices = selectedPaths.map(getSI).filter(function (i2) { return i2 >= 0; });
    renderArcs(); updateUI(); window.SMEngineBridge.renderNow();
    showToast('Forme suggérée pour « ' + s.name + ' » sélectionnée — confirme avec « Marquer comme rôle suivi… ».');
  }

  // Read-only peek for engine-bridge.js's buildTransformBoxItems (2026-07
  // feedback: "la bounding box ne reflete pas cette transformation") — lets
  // the gizmo draw the ACTUAL live-distorted quad instead of the static
  // pre-distort rectangle while a corner-pin drag is in progress.
  window.SMSelectBridge = {
    refreshAfterDocumentRestore: function () {
      // Undo/redo rebuilds every Paper item. Any gesture-local object
      // reference therefore points at removed geometry after the restore.
      mode = null;
      draggingArc = null;
      arcDragCache = null;
      moveStarted = false;
      distortDir = null; distortSrcQuad = null; distortSegs = null; distortDstQuad = null;
      selectedPaths = (state.selectedStrokeIndices || []).map(function (i) {
        return userLayers[state.activeLayerIdx] && userLayers[state.activeLayerIdx].children[i];
      }).filter(function (p) { return p && !p.removed; });
      state.selectedStrokeIndices = selectedPaths.map(getSI).filter(function (i) { return i >= 0; });
    },
    getDistortState: function () {
      return mode === 'xform-distort' ? { dir: distortDir, quad: distortDstQuad } : null;
    },
    getMultiLayerBox: multiLayerSelectionBox,
    getHoverBounds: function () {
      // Oriented (rotated) box, not the raw axis-aligned strokeBounds (2026-08
      // fix — see orientedBoxForPath's own comment, tools.js): a rotated
      // shape's AABB is bigger than and doesn't match its actual outline.
      return (_hoverPathA2D && !_hoverPathA2D.removed) ? orientedBoxForPath(_hoverPathA2D) : null;
    },
    // Switching tools mid-marquee-drag (2026-07-29, QA-confirmed) used to
    // leave a stuck ghost selection rectangle: onUp's own finalization
    // (removing the rect, folding it into selectedPaths) only ever runs on
    // THIS tool's own pointerup, which never comes once another tool is
    // picked mid-drag — and onMove doesn't gate on state.tool==='select' at
    // all once `mode` is already set (only the hover-only, no-mode branch
    // checks shouldIntercept()), so the rect kept following the pointer and
    // even kept queuing itself into the next pointerup's marquee-select
    // logic under whatever tool got picked next. Scoped to marquee only —
    // it's pure UI/selection state with no live document geometry to leave
    // half-mutated, unlike move/scale/rotate/distort (their own onUp
    // already handles committing what THEY changed; this isn't a general
    // abandon-any-gesture hook).
    cancelMarquee: function () {
      if (mode !== 'marquee') return;
      if (_marquee.rect) { _marquee.rect.remove(); _marquee.rect = null; }
      _marquee.active = false;
      mode = null;
      if (window.SMEngineBridge) window.SMEngineBridge.resume();
    },
    toggleTweenOnForSelection: toggleTweenOnForSelection,
  };

  function init() {
    var target = document.getElementById('canvas-area') || document.getElementById('drawing-canvas');
    target.addEventListener('pointerdown', onDown, { capture: true });
    target.addEventListener('pointermove', onMove, { capture: true });
    target.addEventListener('pointerup', onUp, { capture: true });
    target.addEventListener('pointercancel', onUp, { capture: true });
    target.addEventListener('contextmenu', onContext, { capture: true });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
