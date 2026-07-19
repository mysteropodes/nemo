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
  var mode = null; // 'xform-scale' | 'xform-rotate' | 'marquee' | 'move' | 'arc' | 'nv-drag' | 'nv-scale' | 'nv-rotate' | null
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
  function nvClearSelection() {
    if (window._nvSelectedLayer != null) { window._nvSelectedLayer = null; }
  }
  var xformDir = null, xformAnchor = null, xformOrigHandlePos = null, xformLastSx = 1, xformLastSy = 1;
  var xformMap = null; // geometry<->rendered-world mapper when the active layer has a Motion transform
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
  var rotCenter = null, rotStartAngle = 0, rotLastAngle = 0;
  var marqueeStart = null;
  var moveStarted = false;
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
    return best;
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
      if (SMMotion.onDown({ point: new Point(w0[0], w0[1]) })) {
        e.stopImmediatePropagation(); e.preventDefault();
        return;
      }
    }
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
      // Perf fix 2026-07: compute the (expensive, O(n³) autoMatch) stroke
      // pairing ONCE here instead of on every pointermove — only the
      // dragged handle's own position changes during the drag, matches()
      // is re-decided fresh on drag-end anyway (generateTweens(), onUp).
      arcDragCache = computeArcMatchState();
      return;
    }

    var hh = hitTestHandles(pt, e.altKey);
    if (hh && hh.type === 'anchor') {
      // Direct drag of the anchor crosshair — same UI-preference-not-
      // document-edit reasoning as the Alt+click path a bit further down
      // (no pushUndo, no ensureKeyframe): only state.xformAnchorCustom
      // changes, geometry is untouched.
      mode = 'xform-anchor-drag';
      return;
    }
    if (hh && e.ctrlKey && hh.type === 'scale' && (hh.dir === 'nw' || hh.dir === 'ne' || hh.dir === 'sw' || hh.dir === 'se') && distortEligibleCornerAt(pt) === hh.dir) {
      beginDistort(hh.dir);
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
      if (state.appMode !== 'motion') ensureKeyframe();
      selectedPaths = state.selectedStrokeIndices.map(function (i) { return userLayers[state.activeLayerIdx].children[i]; }).filter(Boolean);
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
    if (state.appMode === 'motion' && window.SMMotion) {
      var hitMap = SMMotion.layerMotionPointMap(state.activeLayerIdx);
      if (hitMap) { var hg = hitMap.inv(pt.x, pt.y); hitPt = new Point(hg[0], hg[1]); }
    }
    var hit = (activeLdForLock.locked && !activeLdForLock.symbolId) ? null : layer.hitTest(hitPt, { stroke: true, fill: true, tolerance: 8 / view.zoom });
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
      nvClearSelection(); // clicked empty canvas/another target — video deselects like any object would
      var compHit = hitTestComponentLayers(pt);
      if (compHit) {
        var now2 = Date.now();
        var isDbl = _compClick.layerIdx === compHit.layerIdx && (now2 - _compClick.time < 350);
        _compClick.layerIdx = compHit.layerIdx; _compClick.time = now2;
        if (!e.shiftKey) clearSel();
        state.activeLayerIdx = compHit.layerIdx;
        activateUL(compHit.layerIdx);
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
        if (idx2 >= 0) clickedSet.forEach(function (m) { var mi = selectedPaths.indexOf(m); if (mi >= 0) selectedPaths.splice(mi, 1); });
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
    // See onDown's comment — SMMotion.onDrag no-ops (returns false) unless
    // its own onDown just started a handle/dot/anchor drag, so this is safe
    // to probe unconditionally without any extra state of our own.
    if (state.appMode === 'motion' && window.SMMotion) {
      var w1 = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
      if (SMMotion.onDrag({ point: new Point(w1[0], w1[1]) })) {
        e.stopImmediatePropagation(); e.preventDefault();
        return;
      }
    }
    if (!mode) {
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
        var hh2 = computeHandles();
        // Alt-gated, matching hitTestHandles' own new requirement — showing
        // the "grabbable" grow effect without Alt held would visually
        // promise a drag that onDown won't actually honor.
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
          : 'default';
        if (canvasEl.dataset.xformCursor !== nextCursor) {
          canvasEl.style.cursor = nextCursor;
          canvasEl.dataset.xformCursor = nextCursor;
        }
        // Rotate-ring hover grow (2026-07, "le rond de rotation peut un peu
        // grossir au roll hover") — same light "you can grab this" pattern
        // as the anchor crosshair's own xformAnchorHovered just above.
        var isRingHover = !!(hoverHit && hoverHit.type === 'rotate');
        if (isRingHover !== state.xformRingHovered) {
          state.xformRingHovered = isRingHover;
          window.SMEngineBridge.renderNow();
        }
      } else if (canvasEl.dataset.xformCursor) {
        canvasEl.style.cursor = 'default';
        delete canvasEl.dataset.xformCursor;
      }
      return;
    }
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var pt = new Point(w[0], w[1]);

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
      SMMotion.setLayerValue(nvIdx, 'position', [nvStartPos[0] + nvd.x, nvStartPos[1] + nvd.y]);
      window._sceneVersion++;
      window.SMEngineBridge.renderNow();
    } else if (mode === 'nv-scale') {
      // Uniform corner scale around the box center — same ratio-of-
      // distances math as Motion mode's motionScale (recomputed from the
      // FIXED drag-start baseline every tick; setLayerValue writes an
      // absolute value, so no compounding drift).
      var nvRatio = pt.getDistance(nvPivot) / nvOrigDist;
      SMMotion.setLayerValue(nvIdx, 'scale', [nvStartScale[0] * nvRatio, nvStartScale[1] * nvRatio]);
      window._sceneVersion++;
      window.SMEngineBridge.renderNow();
    } else if (mode === 'nv-rotate') {
      var nvAng = Math.atan2(pt.y - nvPivot.y, pt.x - nvPivot.x) * 180 / Math.PI;
      SMMotion.setLayerValue(nvIdx, 'rotation', [nvOrigRot + (nvAng - nvStartAngle)]);
      window._sceneVersion++;
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
        pushUndo();
        if (state.appMode !== 'motion') ensureKeyframe();
        selectedPaths = state.selectedStrokeIndices.map(function (i) { return userLayers[state.activeLayerIdx].children[i]; }).filter(Boolean);
        moveStarted = true;
      }
      var delta = pt.subtract(lastPt);
      // Layer under a Motion transform: the pointer moves in RENDERED
      // space, the geometry lives underneath — pull the delta back
      // (inverse rotate + inverse scale) or the drag drifts/overshoots.
      var mvMap = (window.SMMotion && SMMotion.layerMotionPointMap) ? SMMotion.layerMotionPointMap(state.activeLayerIdx) : null;
      if (mvMap) { var dv = mvMap.invVec(delta.x, delta.y); delta = new Point(dv[0], dv[1]); }
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
        SMMotion.setLayerValue(mvLi, 'position', [mvCur[0] + delta.x, mvCur[1] + delta.y]);
        window._sceneVersion++;
        lastPt = pt;
        window.SMEngineBridge.renderNow();
        return;
      }
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
        // Custom anchor point (2026-07: "on déplace un point d'ancrage avec
        // alt et l'on bouge l'objet après ... celui-ci ne se déplace pas
        // avec l'objet") — xformAnchorCustom is stored as an ABSOLUTE
        // world-space [x,y] (placeAnchorAt, above), not derived from the
        // shape's bounds like the 9-dot preset anchor is, so it must be
        // translated explicitly here or it's left stranded at its old
        // position the moment the shape moves out from under it.
        if (p.data && p.data.xformAnchorCustom) {
          p.data.xformAnchorCustom = [p.data.xformAnchorCustom[0] + delta.x, p.data.xformAnchorCustom[1] + delta.y];
        }
      });
      // Same fix for the session-level anchor (state.xformAnchorCustom) the
      // on-canvas crosshair/gizmo actually reads (tools.js's xformAnchorPoint)
      // — a single global value, translated once per move tick rather than
      // once per selected path.
      if (state.xformAnchorCustom) {
        state.xformAnchorCustom = [state.xformAnchorCustom[0] + delta.x, state.xformAnchorCustom[1] + delta.y];
      }
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
        window.SMEngineBridge.renderNow();
        return;
      }
      selectedPaths.forEach(function (p) {
        p.scale(stepSx, stepSy, anchor);
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
    } else if (mode === 'xform-distort') {
      var ptD = pt;
      if (xformMap) { var ptgD = xformMap.inv(pt.x, pt.y); ptD = new Point(ptgD[0], ptgD[1]); }
      var dstQuad = { nw: distortSrcQuad.nw, ne: distortSrcQuad.ne, se: distortSrcQuad.se, sw: distortSrcQuad.sw };
      dstQuad[distortDir] = ptD;
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
        window.SMEngineBridge.renderNow();
        return;
      }
      selectedPaths.forEach(function (p) {
        p.rotate(stepAngle, rotCenter);
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
    if (!mode) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    if (mode === 'nv-drag' || mode === 'nv-scale' || mode === 'nv-rotate') {
      mode = null; nvIdx = -1; nvStartPt = null; nvPivot = null;
      // One panel/timeline refresh at gesture end (not per tick — the
      // Transform fields and Motion rows re-read motionStatic/keys).
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
    if (mode === 'arc') {
      draggingArc = null;
      arcDragCache = null;
      generateTweens();
    } else if (mode === 'xform-scale' || mode === 'xform-rotate') {
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
        fillRegenerateLinked(userLayers[state.activeLayerIdx], null);
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
      fillRegenerateLinked(userLayers[state.activeLayerIdx], null);
      saveActiveLayerFrame();
      renderOS();
      renderArcs(); updateUI();
      distortDir = null; distortSrcQuad = null; distortSegs = null;
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
          if (((c instanceof Path && c.segments.length > 0 && (c.strokeColor || c.fillColor)) || c instanceof Raster) && mb.intersects(c.bounds) && isSelectablePathChild(c)) {
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
        fillRegenerateLinked(userLayers[state.activeLayerIdx], null);
        saveActiveLayerFrame();
        // Same stale-onion-ghost fix as the xform-scale/xform-rotate branch
        // above — a plain move commits through this branch too.
        if (didMove) renderOS();
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
        beginDistort(distortDirAt);
        return;
      }
    }
    var layer = userLayers[state.activeLayerIdx];
    var activeLdForLock = state.layers[state.activeLayerIdx];
    // Same hit-testing as onDown (motion-transform-aware point, component-
    // layer whole-instance click, brush-anchor resolution) so right-click
    // selects exactly what a left-click would.
    var hitPt = pt;
    if (state.appMode === 'motion' && window.SMMotion) {
      var hitMap = SMMotion.layerMotionPointMap(state.activeLayerIdx);
      if (hitMap) { var hg = hitMap.inv(pt.x, pt.y); hitPt = new Point(hg[0], hg[1]); }
    }
    var hit = (activeLdForLock.locked && !activeLdForLock.symbolId) ? null : layer.hitTest(hitPt, { stroke: true, fill: true, tolerance: 8 / view.zoom });
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
    if (!selectedPaths.length) return; // empty canvas — let the native menu show, nothing to act on yet
    e.preventDefault(); e.stopImmediatePropagation();
    var multi = selectedPaths.length > 1;
    var p0 = selectedPaths[0];
    var isDeleteGhost = !multi && p0.data && p0.data.isRevisionGhost && p0.data.revisionAction === 'delete';
    var isActiveRevision = !multi && p0.data && p0.data.revisionParentId && !p0.data.isRevisionGhost;
    var isGrouped = !multi && p0.data && p0.data.groupId;
    var items = [
      { label: 'Dupliquer', shortcut: '⌘D', action: function () { duplicateSelection(); } },
      { label: multi ? 'Supprimer la sélection' : 'Supprimer', shortcut: 'Suppr', action: function () { window.SM.deleteSelStrokes(); } },
      { sep: true },
    ];
    if (multi || isGrouped) {
      items.push({ label: isGrouped ? 'Dissocier' : 'Grouper', shortcut: isGrouped ? '⇧⌘G' : '⌘G', action: function () { if (window.SMGroup) { if (isGrouped) SMGroup.ungroupSelection(); else SMGroup.groupSelection(); } } });
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
    ]);
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
    window.showContextMenu(e.clientX, e.clientY, items);
  }

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
