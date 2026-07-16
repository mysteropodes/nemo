// ---- MOTION MODE (v1, 2026-07) ----
// A second animation paradigm alongside Animation 2D's frame-by-frame
// drawing: After-Effects-style property keyframing (Position/Rotation/
// Scale/Opacity per layer), reusing the SAME layers ("on doit retrouver
// nos éléments dedans" — the user's own requirement) so the two modes stay
// two VIEWS of one project, not two separate documents.
//
// Engine: position gets real motion-path curvature via spatial hOut/hIn
// handles, generalized from camera.js's framing keys; the TIMING/velocity
// curve between two keys (rotation/scale/opacity are scalar lerps along the
// same eased t) uses the Tween feature's own N-point on-curve-waypoint
// model (Catmull-Rom tangents), not camera.js's simpler 2-handle bezier —
// explicit request to reuse the SAME curve widget/math the Tween panel
// already has, just scoped PER SEGMENT (key.curvePoints) instead of one
// curve applying globally. evalCurvePoints below is a deliberate small copy
// of ui.js's evalPointsCurve (CLAUDE.md §3's "small stable pure-math pairs
// that must stay in sync" pattern), not a shared import.
//
// CRITICAL save-safety constraint (CLAUDE.md's "family of bug #1" — a new
// per-frame effect that mutates the LIVE Paper.js layer would get baked
// into saveActiveLayerFrame()/serP() on the very next save, permanently
// corrupting the original drawing with the motion offset): the motion
// transform is applied ONLY inside buildSceneJson()'s own item-serialization
// pass (engine-bridge.js hook below), never to userLayers[i] itself. The
// live Paper.js layer stays exactly as drawn; only the JSON handed to the
// Rust renderer (and, symmetrically, export.js's own frame builder) gets
// translated/rotated/scaled/faded.
(function () {
  // 'anchor' is AE's Anchor Point: an OFFSET from the layer's auto-computed
  // bounds center, default [0,0] (== exactly today's behavior, so existing
  // projects/keys are untouched). It doesn't move the artwork itself — it
  // shifts WHERE Rotation/Scale pivot around, independent of Position's own
  // translation. Order matters: it must sit right after 'position' so it
  // reads naturally in the panel (AE's own Position/Anchor Point ordering),
  // and R/S/P/T shortcuts (see PROP_SHORTCUT below) map to it as "A".
  // Motion mode's own row height — deliberately shorter than Animation 2D's
  // 34px .lrow/.frow default (5 properties × N elements adds up fast; a
  // 34px-per-row list read as needlessly sparse). Scoped via body.mode-motion
  // in CSS (style.css), but the SAME number drives every SVG-positioned
  // element BELOW in JS (the connection-bar rect, keyframe-cell centering)
  // so the two can never drift apart — the exact alignment bug class this
  // constant exists to prevent (per explicit feedback: "fait attention au
  // alignement des keyframes aux properties").
  var ROW_H = 22;
  var PROPS = ['position', 'anchor', 'rotation', 'scale', 'opacity'];
  var PROP_LABEL = { position: 'Position', anchor: 'Anchor Point', rotation: 'Rotation', scale: 'Scale', opacity: 'Opacity' };
  var PROP_DIM = { position: 2, anchor: 2, rotation: 1, scale: 2, opacity: 1 };
  var PROP_UNIT = { position: 'px', anchor: 'px', rotation: '°', scale: '%', opacity: '%' };
  var PROP_DEFAULT = { position: [0, 0], anchor: [0, 0], rotation: [0], scale: [100, 100], opacity: [100] };
  // AE's own shortcuts: P/A/R/S/T reveal just that property's row. Kept as
  // a lookup table (not hardcoded in the keydown handler) so the property
  // list and its shortcuts can't silently drift apart.
  var PROP_SHORTCUT = { p: 'position', a: 'anchor', r: 'rotation', s: 'scale', t: 'opacity' };
  // null = show every Transform property (default). A non-null array is an
  // AE-style "revealed properties" filter: plain P/A/R/S/T replaces it with
  // just that one property; Shift+key adds/removes it from the current set
  // (AE's own "shift-click a shortcut to show several at once" convention).
  // Reset to null (show all) whenever a different layer gets expanded — see
  // the row-click handler in renderLayerListMotion.
  var _propFilter = null;
  // Skew Pro's "Filter Properties" — hides every property row with no
  // keyframes at all (both animated-with-a-track AND a plain non-default
  // static override still count as "has something to show"; only a
  // completely untouched default-value property gets hidden). Global
  // toggle (not per-layer/per-group), same one-button-affects-everything
  // convention as the reference.
  var _hideUnanimated = false;
  function propHasContent(holder, prop) { return isAnimated(holder, prop) || !!(holder.motionStatic && holder.motionStatic[prop]); }
  function isPropFiltered(prop) { return !!_propFilter && _propFilter.indexOf(prop) < 0; }
  function handlePropShortcut(key, shiftKey) {
    var prop = PROP_SHORTCUT[(key || '').toLowerCase()];
    if (!prop) return false;
    if (shiftKey) {
      if (!_propFilter) _propFilter = PROPS.slice(); // shift on a fresh/all-shown state starts from "all", then removes
      var i = _propFilter.indexOf(prop);
      if (i >= 0) _propFilter.splice(i, 1); else _propFilter.push(prop);
      if (_propFilter.length === PROPS.length) _propFilter = null; // back to "all" once everything is re-added
    } else {
      _propFilter = [prop];
    }
    renderLayerList(); renderTimeline();
    return true;
  }
  // AE's "U" — reveal animated properties on the selected layer(s), or on
  // EVERY layer if none is selected, per explicit request. Reuses
  // _hideUnanimated (the existing "Filter Properties" toggle/button just
  // above) for the "only keyframed/touched properties show" part rather
  // than inventing a second near-identical filter; the new part is
  // _motionRevealedLayers, which lets several layers' Transform groups be
  // open AT ONCE (window._motionExpandedLayer, the older single-row
  // accordion state used by plain row clicks, only ever holds ONE layer —
  // additive here, doesn't touch that existing click-to-expand behavior).
  // isLayerExpanded is the single source of truth both render passes
  // (renderLayerListMotion's left list AND renderTimelineMotion's right
  // track grid) must agree on — see CLAUDE.md §3 on why a duplicated
  // predicate would be a bug waiting to happen.
  function isLayerExpanded(li) {
    return window._motionExpandedLayer === li || (window._motionRevealedLayers && window._motionRevealedLayers.indexOf(li) >= 0);
  }
  function handleRevealAnimatedShortcut() {
    if (state.appMode !== 'motion') return false;
    var targets = (window._layerSel && window._layerSel.length) ? window._layerSel.slice() : state.layers.map(function (_l, i) { return i; });
    window._motionRevealedLayers = targets;
    _hideUnanimated = true;
    renderLayerList(); renderTimeline();
    return true;
  }

  // ---- easing math: N-point on-curve-waypoint model, deliberate copy of
  // ui.js's shared curve editor (Catmull-Rom tangents -> per-segment cubic
  // Bezier, Newton-with-bisection-fallback solve) — 2026-07, switched from
  // the earlier 2-handle bezier per explicit request to reuse the SAME
  // widget/model the Tween feature already has, just scoped per motion
  // segment instead of one curve applying everywhere. See CLAUDE.md §3 on
  // why this stays a small separate copy rather than a shared import. ----
  var DEFAULT_CURVE = [{ x: 0, y: 0 }, { x: 0.42, y: 0 }, { x: 0.58, y: 1 }, { x: 1, y: 1 }];
  // tx/ty preserved: a point's manual tangent override (draggable Alt-
  // handles in the shared curve editor, ui.js) — stripping them here
  // reset hand-tuned tangents on every key clone.
  function cloneCurvePts(pts) { return pts.map(function (p) { var o = { x: p.x, y: p.y }; if (typeof p.tx === 'number') { o.tx = p.tx; o.ty = p.ty || 0; } return o; }); }
  function curveCubicAt(t, a, b, c, d) { var u = 1 - t; return u * u * u * a + 3 * u * u * t * b + 3 * u * t * t * c + t * t * t * d; }
  function curveCubicDerivAt(t, a, b, c, d) { var u = 1 - t; return 3 * u * u * (b - a) + 6 * u * t * (c - b) + 3 * t * t * (d - c); }
  // Manual tangent override (tx/ty) or derived Catmull-Rom — duplicated
  // from ui.js's tangentAt (CLAUDE.md §3 pure-math pair, keep in sync).
  function curveTangentAt(pts, i) {
    var p = pts[i];
    if (typeof p.tx === 'number') return { x: p.tx, y: p.ty || 0 };
    var prev = pts[i - 1] || p, next = pts[i + 1] || p;
    return { x: (next.x - prev.x) / 2, y: (next.y - prev.y) / 2 };
  }
  function curveSegCtrl(pts, i) {
    var p0 = pts[i], p3 = pts[i + 1];
    var t1 = curveTangentAt(pts, i), t2 = curveTangentAt(pts, i + 1);
    return { c1: { x: p0.x + t1.x / 3, y: p0.y + t1.y / 3 }, c2: { x: p3.x - t2.x / 3, y: p3.y - t2.y / 3 } };
  }
  function curveSegFor(pts, x) { var i = 0; while (i < pts.length - 2 && pts[i + 1].x < x) i++; return i; }
  function evalCurvePoints(pts, x) {
    if (!pts || pts.length < 2) return x;
    x = Math.max(0, Math.min(1, x));
    var i = curveSegFor(pts, x), p0 = pts[i], p3 = pts[i + 1], ctrl = curveSegCtrl(pts, i);
    var span = p3.x - p0.x, t = span > 1e-6 ? (x - p0.x) / span : 0;
    for (var k = 0; k < 8; k++) {
      var ex = curveCubicAt(t, p0.x, ctrl.c1.x, ctrl.c2.x, p3.x) - x;
      var dx = curveCubicDerivAt(t, p0.x, ctrl.c1.x, ctrl.c2.x, p3.x);
      if (Math.abs(dx) < 1e-6) break;
      t -= ex / dx; t = Math.max(0, Math.min(1, t));
    }
    return curveCubicAt(t, p0.y, ctrl.c1.y, ctrl.c2.y, p3.y);
  }

  // ---- data model: state.layers[i].motion = {position:{keys:[...]}, ...},
  // state.layers[i].motionStatic = {position:[x,y], ...} for a property
  // that has a non-default value but ISN'T keyframed (stopwatch off) ----
  function ensureTrack(ld, prop) {
    if (!ld.motion) ld.motion = {};
    if (!ld.motion[prop]) ld.motion[prop] = { keys: [] };
    return ld.motion[prop];
  }
  function hasKeys(ld, prop) { return !!(ld.motion && ld.motion[prop] && ld.motion[prop].keys.length); }
  function isAnimated(ld, prop) { return hasKeys(ld, prop); }
  function sortKeys(track) { track.keys.sort(function (a, b) { return a.frame - b.frame; }); }
  function keyAt(track, frame) { return track.keys.find(function (k) { return k.frame === frame; }) || null; }
  function staticValue(ld, prop) {
    var st = ld.motionStatic && ld.motionStatic[prop];
    return st ? st.slice() : PROP_DEFAULT[prop].slice();
  }

  // The value of `prop` on layer `ld` at `frame` — exact key, interpolated,
  // clamped outside the keyed range, the static override, or the neutral
  // default. Always returns an array (length 1 or 2, per PROP_DIM).
  function valueAtFrame(ld, prop, frame) {
    var track = ld.motion && ld.motion[prop];
    if (!track || !track.keys.length) return staticValue(ld, prop);
    var ks = track.keys;
    if (frame <= ks[0].frame) return ks[0].v.slice();
    var last = ks[ks.length - 1];
    if (frame >= last.frame) return last.v.slice();
    for (var i = 0; i < ks.length - 1; i++) {
      var a = ks[i], b = ks[i + 1];
      if (frame >= a.frame && frame < b.frame) {
        // Hold keyframe (Caddis/AE convention, 2026-07): the value stays
        // pinned to `a` for the whole segment, then jumps to `b` the
        // instant frame reaches b.frame — no interpolation. Flag lives on
        // the LEFT key (a), matching AE's own model (hold is a property of
        // the OUTGOING keyframe, not the segment).
        if (a.hold) return a.v.slice();
        var t = (frame - a.frame) / (b.frame - a.frame);
        var y = evalCurvePoints(a.curvePoints || DEFAULT_CURVE, t);
        // Position gets real spatial-bezier curvature through its
        // hOut/hIn handles (same construction as camera.js's motion
        // path) whenever either handle is non-zero — a straight [0,0]
        // handle collapses to the plain linear-in-eased-t case below.
        if (prop === 'position' && ((a.hOut && (a.hOut[0] || a.hOut[1])) || (b.hIn && (b.hIn[0] || b.hIn[1])))) {
          var ho = a.hOut || [0, 0], hi = b.hIn || [0, 0];
          var p1x = a.v[0] + ho[0], p1y = a.v[1] + ho[1], p2x = b.v[0] + hi[0], p2y = b.v[1] + hi[1];
          var v = 1 - y;
          var px = v * v * v * a.v[0] + 3 * v * v * y * p1x + 3 * v * y * y * p2x + y * y * y * b.v[0];
          var py = v * v * v * a.v[1] + 3 * v * v * y * p1y + 3 * v * y * y * p2y + y * y * y * b.v[1];
          return [px, py];
        }
        var out = [];
        for (var d = 0; d < a.v.length; d++) out.push(a.v[d] + (b.v[d] - a.v[d]) * y);
        return out;
      }
    }
    return last.v.slice();
  }
  // The segment whose ease governs `frame` (its LEFT key) — same contract
  // as camera.js's segmentLeftKey, generalized to any track.
  function segmentLeftKey(track, frame) {
    var ks = track.keys;
    if (ks.length < 2) return null;
    if (frame >= ks[ks.length - 1].frame) return ks[ks.length - 2];
    for (var i = ks.length - 2; i >= 0; i--) if (frame >= ks[i].frame) return ks[i];
    return ks[0];
  }

  // Opens the shared curve widget in its points-based mode
  // (window._curveEditor.editMotionSeg) — the SAME on-curve-waypoint model
  // and rendering the Tween feature's own curve already uses (explicit
  // request: reuse it rather than camera.js's simpler 2-handle bezier), just
  // scoped to this ONE segment's own `key.curvePoints` instead of the single
  // global tween curve. Dragging/adding/deleting points in the widget
  // mutates `seg.curvePoints` in place.
  function openMotionEaseEditor(ld, prop) {
    var track = ld.motion && ld.motion[prop];
    if (!track || !track.keys.length) { if (window.showToast) showToast('Anime d’abord ' + PROP_LABEL[prop] + ' (icône chrono) pour avoir une courbe'); return; }
    // Auto-create the missing second key — explicit request ("créer les
    // clés manquantes si il le faut"): a single-key track has nothing to
    // ease BETWEEN yet, but the user shouldn't have to manually add a
    // placeholder key first just to open the curve editor. Same value as
    // the existing key (a flat segment, editing the curve then shapes the
    // actual motion) at a nearby frame — the current frame if it differs
    // from the only key's, otherwise +12 frames (or -12 if that overflows).
    if (track.keys.length === 1) {
      var only = track.keys[0];
      var nf = state.currentFrame !== only.frame ? state.currentFrame : Math.min(state.totalFrames - 1, only.frame + 12);
      if (nf === only.frame) nf = Math.max(0, only.frame - 12);
      if (nf !== only.frame) {
        track.keys.push({ frame: nf, v: only.v.slice(), curvePoints: cloneCurvePts(DEFAULT_CURVE), hOut: [0, 0], hIn: [0, 0] });
        sortKeys(track);
        renderLayerList(); renderTimeline();
      }
    }
    var seg = segmentLeftKey(track, state.currentFrame) || track.keys[0];
    var idx = track.keys.indexOf(seg);
    var next = track.keys[idx + 1] || track.keys[idx - 1];
    if (!next) { if (window.showToast) showToast('Impossible de créer un segment éditable'); return; }
    if (window._curveEditor) window._curveEditor.editMotionSeg(seg, PROP_LABEL[prop] + ' : clé ' + (seg.frame + 1) + ' → ' + (next.frame + 1));
  }

  function setKeyAtCurrentFrame(ld, prop, values) {
    var track = ensureTrack(ld, prop);
    var k = keyAt(track, state.currentFrame);
    if (k) { k.v = values.slice(); }
    else {
      track.keys.push({ frame: state.currentFrame, v: values.slice(), curvePoints: cloneCurvePts(DEFAULT_CURVE), hOut: [0, 0], hIn: [0, 0] });
      sortKeys(track);
    }
    return keyAt(track, state.currentFrame);
  }
  function removeKeyAtCurrentFrame(ld, prop) {
    var track = ld.motion && ld.motion[prop];
    if (!track) return;
    var i = track.keys.findIndex(function (k) { return k.frame === state.currentFrame; });
    if (i >= 0) track.keys.splice(i, 1);
  }
  // Stopwatch toggle: OFF→ON starts animating from the CURRENT effective
  // value (matches AE: turning on keyframing never jumps the value); ON→OFF
  // freezes at the current interpolated value as a static override, same
  // "manual edit wins" principle CLAUDE.md documents for fill-merge —
  // switching modes must never silently snap a layer back to its neutral
  // default.
  // Auto-convert to a Component on the FIRST layer-level property edit.
  // Originally (2026-07-17) scoped to layers with 2+ elements ("un calque
  // animé dans motion si il contient plusieurs élément devient
  // automatiquement un component") — widened 2026-07 to ANY layer,
  // including a single shape: StoryBoard exclusively works with Components
  // (§8 CLAUDE.md, "StoryBoard ne manipule QUE des Components"), so a
  // single-element layer that never converted could animate fine in Motion
  // but could never be placed in a StoryBoard montage — a real dead end
  // found live ("j'ai essayé avec une shape dessinné et ça ne créer pas de
  // component"). The friction this used to avoid (locking a trivial single
  // shape into a symbol) is mitigated by double-clicking the shape ON THE
  // CANVAS, which already enters the symbol for editing (enterSymbol,
  // wired in select-bridge.js — a separate, pre-existing mechanism from
  // Motion's own layer-row double-click, see splitLayerIntoElements below),
  // so editing the shape after conversion is one extra click, not a dead
  // end — and convertComponentToLayer stays available to reverse it.
  // Guards: only for a genuine LAYER target (state.layers.indexOf finds
  // it; a per-element holder from ensureElementHolder is a bare {} never
  // in that array), not already a component. convertLayerToComponent
  // (app.js) mutates `ld` IN PLACE (sets ld.symbolId, clears ld.frames)
  // rather than replacing the object, so whatever the caller attaches
  // right after this call still lands on the correct (now-converted)
  // layer. Shared by BOTH entry points that can start animating a property
  // — the stopwatch toggle (toggleAnimated) AND a direct canvas drag
  // (setValue, wired up by select-bridge.js's Motion-mode drag handling).
  // Idempotent (guarded by `!ld.symbolId`), so calling it on every drag
  // tick is safe — it only ever actually converts once.
  function maybeAutoConvertToComponent(ld) {
    var li = state.layers.indexOf(ld);
    if (li >= 0 && !ld.symbolId && userLayers[li]) {
      var elCount = userLayers[li].children.filter(function (c) { return (c instanceof Path || c instanceof Raster) && isSelectablePathChild(c); }).length;
      if (elCount >= 1) convertLayerToComponent(li);
    }
  }
  function toggleAnimated(ld, prop) {
    if (isAnimated(ld, prop)) {
      var v = valueAtFrame(ld, prop, state.currentFrame);
      if (!ld.motion) ld.motion = {};
      ld.motion[prop] = { keys: [] };
      if (!ld.motionStatic) ld.motionStatic = {};
      ld.motionStatic[prop] = v;
    } else {
      maybeAutoConvertToComponent(ld);
      var cur = staticValue(ld, prop);
      ensureTrack(ld, prop).keys = [{ frame: state.currentFrame, v: cur, curvePoints: cloneCurvePts(DEFAULT_CURVE), hOut: [0, 0], hIn: [0, 0] }];
    }
  }
  // Editing a value field: if animated, this is a scrub at the CURRENT
  // frame — auto-adds/updates a key there (AE convention). If not
  // animated, it's just the static override.
  function setValue(ld, prop, values) {
    if (isAnimated(ld, prop)) setKeyAtCurrentFrame(ld, prop, values);
    else { maybeAutoConvertToComponent(ld); if (!ld.motionStatic) ld.motionStatic = {}; ld.motionStatic[prop] = values.slice(); }
  }

  // ---- render-time transform (engine-bridge.js hook — see header
  // comment: NEVER applied to the live userLayers[i], only to the JSON
  // items handed to the renderer, so a save can never bake this in) ----
  function rotPt(px, py, cx, cy, deg) {
    if (!deg) return [px, py];
    var a = deg * Math.PI / 180, c = Math.cos(a), s = Math.sin(a);
    var dx = px - cx, dy = py - cy;
    return [cx + dx * c - dy * s, cy + dx * s + dy * c];
  }
  function rotVec(dx, dy, deg) {
    if (!deg) return [dx, dy];
    var a = deg * Math.PI / 180, c = Math.cos(a), s = Math.sin(a);
    return [dx * c - dy * s, dx * s + dy * c];
  }
  // ---- LAYER vs ELEMENT targets (2026-07, "shape layer" feedback) ----
  // A Nemo "layer" is a loose bag of independent strokes/shapes drawn across
  // the timeline — nothing like AE's one-cohesive-graphic layer. Pivoting
  // Rotation/Scale around the whole layer's bounds.center falls apart the
  // moment a layer holds two unrelated shapes in opposite corners: the
  // pivot lands in empty space between them. Fix: Motion can now ALSO target
  // one specific element (stroke, identified by its stable data.strokeId —
  // same id fillWalls/team-review already rely on) INSIDE a layer, with its
  // OWN independent Position/Anchor/Rotation/Scale/Opacity, nested inside
  // the layer's own transform — exactly AE's shape-group-inside-a-shape-
  // layer model. Storage: ld.elementMotion[strokeId] = {motion,motionStatic},
  // the identical shape `ld` itself already has — every function below
  // (valueAtFrame, isAnimated, setValue, toggleAnimated, etc.) only ever
  // reads/writes `.motion`/`.motionStatic` on whatever object it's handed,
  // so passing an element holder instead of `ld` just works, zero duplicated
  // logic needed for the per-element case.
  function ensureElementHolder(ld, strokeId) {
    if (!ld.elementMotion) ld.elementMotion = {};
    if (!ld.elementMotion[strokeId]) ld.elementMotion[strokeId] = {};
    return ld.elementMotion[strokeId];
  }
  function elementHolder(ld, strokeId) { return ld.elementMotion ? ld.elementMotion[strokeId] : null; }
  function elementHasMotion(ld, strokeId) {
    var h = elementHolder(ld, strokeId);
    return !!(h && (h.motion || h.motionStatic));
  }
  // Shared by layerMotionAt/elementMotionAt below — `holder` is either `ld`
  // itself (layer target) or `ld.elementMotion[strokeId]` (element target).
  // Null when there's no motion at all this frame (the overwhelmingly common
  // case) — callers skip the per-item transform pass entirely then.
  function computeMotionMat(holder, frameIdx) {
    if (!holder || (!holder.motion && !holder.motionStatic)) return null;
    var pos = valueAtFrame(holder, 'position', frameIdx);
    var anc = valueAtFrame(holder, 'anchor', frameIdx);
    var rot = valueAtFrame(holder, 'rotation', frameIdx)[0];
    var scl = valueAtFrame(holder, 'scale', frameIdx);
    var op = valueAtFrame(holder, 'opacity', frameIdx)[0];
    if (!pos[0] && !pos[1] && !anc[0] && !anc[1] && !rot && scl[0] === 100 && scl[1] === 100 && op === 100) return null;
    // ax/ay: the pivot offset callers (engine-bridge.js, export.js) add to
    // the target's own bounds.center before scaling/rotating around it —
    // Rotation/Scale pivot around this point, Position's dx/dy is a plain
    // translation applied independently on top (matches AE: moving the
    // Anchor Point doesn't move the artwork, only where it spins/scales
    // from).
    return { dx: pos[0], dy: pos[1], rot: rot, sx: scl[0] / 100, sy: scl[1] / 100, op: Math.max(0, op / 100), ax: anc[0], ay: anc[1] };
  }
  function layerMotionAt(li, frameIdx) {
    var ld = state.layers[li];
    if (!ld) return null;
    // Component instances CAN carry a layer-level Motion track (2026-07-17,
    // "un calque animé dans motion... devient automatiquement un
    // component") — verified this composes correctly with symMatrix, not
    // "double-applying" the same transform: getEffectiveStrokes (app.js)
    // bakes symMatrix into the STROKE DATA at frame-load time (the
    // instance's placement, AE's "where you dropped the precomp"); this
    // motionMat is a SEPARATE transform applied by buildSceneJson on top
    // of the already-placed Paper items at render time (AE's "keyframed
    // Transform properties ON that precomp layer") — two different,
    // composable transforms, exactly AE's precomp-layer model. Confirmed
    // live: rotating/scaling a converted component pivots around its own
    // bounds+anchor correctly, matches an ordinary layer.
    return computeMotionMat(ld, frameIdx);
  }
  // Nested INSIDE the layer transform (engine-bridge.js/export.js apply this
  // FIRST, pivoted around the item's own bounds, THEN the layer transform on
  // top) — matches AE composing a shape group's transform inside its parent
  // layer's transform.
  function elementMotionAt(li, strokeId, frameIdx) {
    var ld = state.layers[li];
    if (!ld || ld.symbolId || !ld.elementMotion) return null;
    return computeMotionMat(ld.elementMotion[strokeId], frameIdx);
  }
  // Transforms one item's already-built segments array (engine-bridge.js's
  // {point,handleIn,handleOut} triples, handles as RELATIVE offsets — see
  // serP/lottieShapeValue's shared convention) around `pivot`. Scale+rotate
  // happen in the pivot's local frame, translate last — matches the order
  // camera.js's own applyToExportLayer uses (scale, rotate, then reposition).
  function transformSegments(segments, pivot, m) {
    return segments.map(function (s) {
      var lx = (s.point[0] - pivot.x) * m.sx, ly = (s.point[1] - pivot.y) * m.sy;
      var r = rotPt(pivot.x + lx, pivot.y + ly, pivot.x, pivot.y, m.rot);
      var out = { point: [r[0] + m.dx, r[1] + m.dy] };
      if (s.handleIn) { var hi = rotVec(s.handleIn[0] * m.sx, s.handleIn[1] * m.sy, m.rot); out.handleIn = hi; }
      if (s.handleOut) { var ho = rotVec(s.handleOut[0] * m.sx, s.handleOut[1] * m.sy, m.rot); out.handleOut = ho; }
      return out;
    });
  }
  // Transforms a raster item's on-canvas rect. v1 scope: scale + translate
  // only (skips rotation for images — the axis-aligned {x,y,width,height}
  // the renderer expects has no rotation field; rare case since almost all
  // Nemo content is vector strokes, noted as a known v1 limitation).
  function transformImageRect(rb, pivot, m) {
    var cx = rb.x + rb.width / 2, cy = rb.y + rb.height / 2;
    var ncx = pivot.x + (cx - pivot.x) * m.sx + m.dx, ncy = pivot.y + (cy - pivot.y) * m.sy + m.dy;
    var w = rb.width * m.sx, h = rb.height * m.sy;
    return { x: ncx - w / 2, y: ncy - h / 2, width: w, height: h };
  }

  // ---- canvas overlay: the position motion path for the layer(s)
  // currently expanded in the Motion panel — same dashed-bezier + handle
  // visual language as camera.js's own trajectory overlay. ----
  function circleSegs(cx, cy, r) {
    var k = r * 0.5523;
    return [
      { point: [cx + r, cy], handleIn: [0, k], handleOut: [0, -k] },
      { point: [cx, cy - r], handleIn: [k, 0], handleOut: [-k, 0] },
      { point: [cx - r, cy], handleIn: [0, -k], handleOut: [0, k] },
      { point: [cx, cy + r], handleIn: [-k, 0], handleOut: [k, 0] },
    ];
  }
  // ---- Unified motion path for a multi-element canvas selection
  // (2026-07-16, "si on sélectionne plusieurs éléments qui ont
  // actuellement un motion path individuel alors ils auront un motion
  // path unifié à partir de la sélection") : with 2+ elements of the
  // active layer selected on canvas, each carrying its OWN animated
  // position, the overlay stops drawing N separate paths' worth of
  // clutter and shows ONE path — selection centroid + the AVERAGE of the
  // elements' position offsets, one dot per frame keyed on ANY of them.
  // Dragging a dot moves every selected element's key at that frame by
  // the same delta (created on the fly at its interpolated value for an
  // element that has no key there yet), so the group's trajectory is
  // edited as a single object without merging/destroying the individual
  // tracks. ----
  function unifiedMotionTargets() {
    if (state.appMode !== 'motion') return null;
    var sel = (typeof selectedPaths !== 'undefined') ? selectedPaths : null;
    if (!sel || sel.length < 2) return null;
    var li = state.activeLayerIdx, ld = state.layers[li];
    if (!ld || ld.symbolId || !userLayers[li]) return null;
    var seen = {}, out = [], cx = 0, cy = 0, n = 0;
    sel.forEach(function (p) {
      if (!p || !p.data || p.parent !== userLayers[li]) return;
      cx += p.bounds.center.x; cy += p.bounds.center.y; n++;
      var sid = p.data.strokeId;
      if (!sid || seen[sid]) return;
      seen[sid] = true;
      var h = ld.elementMotion && ld.elementMotion[sid];
      if (h && h.motion && h.motion.position && h.motion.position.keys.length) out.push({ strokeId: sid, holder: h });
    });
    if (out.length < 2 || !n) return null;
    return { targets: out, centroid: { x: cx / n, y: cy / n } };
  }
  function unifiedFrames(targets) {
    var set = {};
    targets.forEach(function (t) { t.holder.motion.position.keys.forEach(function (k) { set[k.frame] = true; }); });
    return Object.keys(set).map(Number).sort(function (a, b) { return a - b; });
  }
  function unifiedPointAt(u, frame) {
    var sx = 0, sy = 0;
    u.targets.forEach(function (t) { var v = valueAtFrame(t.holder, 'position', frame); sx += v[0]; sy += v[1]; });
    return { x: u.centroid.x + sx / u.targets.length, y: u.centroid.y + sy / u.targets.length };
  }
  function buildUnifiedOverlay(u) {
    var items = [];
    var zs = 1 / Math.max(0.0001, view.zoom);
    var col = [189, 147, 249, 220]; // violet — visibly distinct from the single-target accent-blue path
    var frames = unifiedFrames(u.targets);
    if (frames.length < 1) return items;
    var pts = frames.map(function (f) { return unifiedPointAt(u, f); });
    if (pts.length > 1) {
      items.push({ segments: pts.map(function (p) { return { point: [p.x, p.y] }; }), closed: false, fillColor: null, strokeColor: col, strokeWidth: 1.5 * zs, dashPattern: [5 * zs, 4 * zs] });
    }
    frames.forEach(function (f, i) {
      var isCur = f === state.currentFrame;
      items.push({ segments: circleSegs(pts[i].x, pts[i].y, (isCur ? 6 : 4.5) * zs), closed: true, fillColor: isCur ? [255, 170, 40, 255] : col, strokeColor: [30, 30, 30, 255], strokeWidth: 1.2 * zs });
    });
    return items;
  }
  function buildOverlayItems() {
    var u = unifiedMotionTargets();
    if (u) return buildUnifiedOverlay(u); // multi-selection: the unified path replaces the single-target one entirely
    var t = activeMotionTarget();
    if (!t) return [];
    var holder = t.holder, bc = t.boundsCenter;
    var items = [];
    var zs = 1 / Math.max(0.0001, view.zoom);
    var pathCol = [63, 107, 245, 200]; // --accent
    var handleCol = [255, 170, 40, 220];
    // Anchor point — AE-style crosshair-in-circle, ALWAYS shown while a
    // layer/element is expanded (even with zero keyframes on anything), same
    // as AE shows it on any selected layer/shape group. Pivot = bounds
    // center + anchor offset (see computeMotionMat's header comment);
    // position/rotation/scale are NOT applied to this preview point on
    // purpose — it marks where the pivot sits in the target's OWN unmoved
    // bounds, matching what engine-bridge.js/export.js actually pivot
    // around at frame 0-equivalent (the anchor is a static geometric
    // reference, not itself animated relative to the moving artwork).
    var anc = valueAtFrame(holder, 'anchor', state.currentFrame);
    var ax = bc.x + anc[0], ay = bc.y + anc[1];
    var ancCol = [80, 220, 140, 255];
    items.push({ segments: [{ point: [ax - 9 * zs, ay] }, { point: [ax + 9 * zs, ay] }], closed: false, fillColor: null, strokeColor: ancCol, strokeWidth: 1.5 * zs });
    items.push({ segments: [{ point: [ax, ay - 9 * zs] }, { point: [ax, ay + 9 * zs] }], closed: false, fillColor: null, strokeColor: ancCol, strokeWidth: 1.5 * zs });
    items.push({ segments: circleSegs(ax, ay, 6 * zs), closed: true, fillColor: null, strokeColor: ancCol, strokeWidth: 1.5 * zs });
    if (!hasKeys(holder, 'position')) return items;
    // Position keys store a DELTA translation (computeMotionMat's dx/dy —
    // [0,0] means "no motion", added on top of the artwork's own drawn
    // position), never an absolute world point — but this overlay used to
    // plot the path/dots/handles at the RAW key.v coordinates, i.e. at
    // world (0,0) for an unmoved key instead of at the object itself.
    // Found live (2026-07-17, "le motion path doit être placé au point
    // d'ancrage") : offsetting everything below by the anchor's own
    // absolute position (ax,ay — bounds center + anchor offset, the exact
    // point Rotation/Scale already pivot around) makes a fresh/unmoved key
    // sit exactly ON the anchor crosshair, and the path read as "where the
    // object goes FROM the anchor", matching computeMotionMat's actual math
    // instead of drawing in a different coordinate space than the one that
    // renders. unifiedMotionTargets/buildUnifiedOverlay already summed
    // position deltas onto a centroid the same delta-aware way — this
    // brings the single-target view in line with it.
    var pvx = ax, pvy = ay;
    var track = holder.motion.position;
    var ks = track.keys;
    for (var i = 0; i < ks.length - 1; i++) {
      var a = ks[i], b = ks[i + 1];
      var ho = a.hOut || [0, 0], hi = b.hIn || [0, 0];
      var pts = [];
      var steps = 24;
      for (var s = 0; s <= steps; s++) {
        var t = s / steps, v = 1 - t;
        pts.push({
          point: [
            pvx + v * v * v * a.v[0] + 3 * v * v * t * (a.v[0] + ho[0]) + 3 * v * t * t * (b.v[0] + hi[0]) + t * t * t * b.v[0],
            pvy + v * v * v * a.v[1] + 3 * v * v * t * (a.v[1] + ho[1]) + 3 * v * t * t * (b.v[1] + hi[1]) + t * t * t * b.v[1],
          ],
        });
      }
      items.push({ segments: pts, closed: false, fillColor: null, strokeColor: pathCol, strokeWidth: 1.5 * zs, dashPattern: [5 * zs, 4 * zs] });
    }
    ks.forEach(function (k, ki) {
      var isCur = k.frame === state.currentFrame;
      var kx = pvx + k.v[0], ky = pvy + k.v[1];
      items.push({ segments: circleSegs(kx, ky, (isCur ? 6 : 4.5) * zs), closed: true, fillColor: isCur ? [255, 170, 40, 255] : [230, 230, 230, 255], strokeColor: [30, 30, 30, 255], strokeWidth: 1.2 * zs });
      // Spatial handles, same discoverable small-dot pattern as camera.js
      var hs = [];
      if (ki < ks.length - 1) hs.push(k.hOut || [0, 0]);
      if (ki > 0) hs.push(k.hIn || [0, 0]);
      hs.forEach(function (h) {
        if (!h[0] && !h[1]) return;
        var hx = kx + h[0], hy = ky + h[1];
        items.push({ segments: [{ point: [kx, ky] }, { point: [hx, hy] }], closed: false, fillColor: null, strokeColor: handleCol, strokeWidth: 1 * zs });
        items.push({ segments: circleSegs(hx, hy, 4 * zs), closed: true, fillColor: handleCol, strokeColor: [30, 30, 30, 255], strokeWidth: 1 * zs });
      });
    });
    return items;
  }
  // Shared by the overlay above and the hit-test/drag code below — the
  // exact world point a position delta of [0,0] should render/hit-test at.
  function motionPivotOf(t) {
    var anc = valueAtFrame(t.holder, 'anchor', state.currentFrame);
    return { x: t.boundsCenter.x + anc[0], y: t.boundsCenter.y + anc[1] };
  }

  // ---- canvas drag: position keyframe dots + spatial handles ----
  // Mirrors camera.js's onDown/onDrag/onUp (its hitTrajectoryHandle in
  // particular), but Motion mode is a persistent app-mode, not a `state.tool`
  // value like 'camera' — it must coexist with whatever tool is active, so
  // these are wired as an early INTERCEPT at the top of tools.js's
  // onMouseDown/onMouseDrag/onMouseUp (returns true only when the click
  // actually lands on a handle/dot, so an unrelated click falls through
  // unchanged into Select/Draw/etc. below it).
  var _motionDrag = null; // {mode:'point'|'handle', key, which}
  function hitPositionHandle(pt, ks, pv) {
    var tol = 10 / view.zoom;
    for (var i = 0; i < ks.length; i++) {
      var k = ks[i], hs = [];
      if (i < ks.length - 1) hs.push(['hOut', k.hOut || [0, 0]]);
      if (i > 0) hs.push(['hIn', k.hIn || [0, 0]]);
      for (var j = 0; j < hs.length; j++) {
        var hx = pv.x + k.v[0] + hs[j][1][0], hy = pv.y + k.v[1] + hs[j][1][1];
        if (Math.hypot(pt.x - hx, pt.y - hy) < tol) return { key: k, which: hs[j][0] };
      }
    }
    return null;
  }
  function hitPositionDot(pt, ks, pv) {
    var tol = 8 / view.zoom;
    for (var i = 0; i < ks.length; i++) if (Math.hypot(pt.x - (pv.x + ks[i].v[0]), pt.y - (pv.y + ks[i].v[1])) < tol) return ks[i];
    return null;
  }
  // Finds the LIVE Paper item for a strokeId within a layer (including
  // CompoundPath sub-paths, which share their parent's data.strokeId — see
  // serP/engine-bridge.js's CompoundPath flattening) — needed to read an
  // element's own current bounds for its overlay/pivot.
  function findElementItem(li, strokeId) {
    var kids = userLayers[li] && userLayers[li].children;
    if (!kids) return null;
    for (var i = 0; i < kids.length; i++) {
      if (kids[i].data && kids[i].data.strokeId === strokeId) return kids[i];
    }
    return null;
  }
  // Single source of truth for "what is Motion currently focused on" —
  // the expanded ELEMENT if one is expanded (see renderElementsList),
  // otherwise the expanded LAYER itself. Both the canvas overlay
  // (buildOverlayItems) and the drag handlers below resolve through this so
  // they always agree on the same target and the same pivot-bounds source.
  function activeMotionTarget() {
    if (state.appMode !== 'motion') return null;
    // Bug found 2026-07 ("quand tu select un calque ça select pas dans le
    // canvas"): this used to require window._motionExpandedLayer (a row's
    // Transform group toggled OPEN) — merely clicking a layer row (setting
    // state.activeLayerIdx) showed nothing on canvas at all, unlike AE where
    // clicking a layer immediately shows its anchor/position path. Falling
    // back to activeLayerIdx when nothing is explicitly expanded keeps the
    // expanded-layer/expanded-element priority unchanged (an open Transform
    // group still wins, and still gates element-level focus below) while
    // fixing the plain single-click case.
    var li = window._motionExpandedLayer != null ? window._motionExpandedLayer : state.activeLayerIdx;
    if (li == null) return null;
    var ld = state.layers[li];
    if (!ld || !userLayers[li]) return null;
    // Component instances now allow layer-level Motion (see layerMotionAt's
    // comment) — but per-ELEMENT sub-targeting stays blocked: renderElementsList
    // never runs for a symbolId row (renderLayerListMotion), so
    // window._motionExpandedElement can never legitimately be set while
    // this layer is the expanded one; the `ld.symbolId` guard here would
    // have been redundant with that, not a second independent gate.
    if (window._motionExpandedLayer != null && window._motionExpandedElement != null) {
      var item = findElementItem(li, window._motionExpandedElement);
      if (item) return { li: li, strokeId: window._motionExpandedElement, holder: ensureElementHolder(ld, window._motionExpandedElement), boundsCenter: item.bounds.center };
      // Element no longer present at this frame (drawing changed) — fall
      // back to the layer rather than silently drawing nothing.
    }
    return { li: li, strokeId: null, holder: ld, boundsCenter: userLayers[li].bounds.center };
  }
  function activePositionKeys() {
    var t = activeMotionTarget();
    if (!t || !hasKeys(t.holder, 'position')) return null;
    return t.holder.motion.position.keys;
  }
  function hitAnchorPoint(pt, t) {
    var tol = 9 / view.zoom;
    var anc = valueAtFrame(t.holder, 'anchor', state.currentFrame);
    var ax = t.boundsCenter.x + anc[0], ay = t.boundsCenter.y + anc[1];
    return Math.hypot(pt.x - ax, pt.y - ay) < tol ? { holder: t.holder, bc: t.boundsCenter } : null;
  }
  function onDown(event) {
    // Unified multi-selection path first — while it's active the overlay
    // shows ONLY the unified dots (see buildOverlayItems), so the single-
    // target hit-tests below would grab invisible geometry.
    var u = unifiedMotionTargets();
    if (u) {
      var uFrames = unifiedFrames(u.targets), uTol = 8 / view.zoom;
      for (var ui = 0; ui < uFrames.length; ui++) {
        var upt = unifiedPointAt(u, uFrames[ui]);
        if (Math.hypot(event.point.x - upt.x, event.point.y - upt.y) < uTol) {
          pushUndo();
          _motionDrag = { mode: 'unified', u: u, frame: uFrames[ui], last: { x: event.point.x, y: event.point.y } };
          return true;
        }
      }
      return false;
    }
    var t = activeMotionTarget();
    // Position keys/handles checked BEFORE the anchor point (2026-07-17
    // motion-path-at-anchor fix made this ordering matter): a key at its
    // default [0,0] delta now draws its dot exactly ON the anchor
    // crosshair (motionPivotOf) — dragging keyframes is the far more
    // common gesture, so it wins the overlap; the anchor stays reachable
    // once a key has been moved away from it (the ordinary case) or by
    // starting the drag from a few px off-center.
    if (t) {
      var ks = activePositionKeys();
      if (ks) {
        var pv = motionPivotOf(t);
        var hp = hitPositionHandle(event.point, ks, pv);
        if (hp) { pushUndo(); _motionDrag = { mode: 'handle', key: hp.key, which: hp.which, pv: pv }; return true; }
        var pk = hitPositionDot(event.point, ks, pv);
        if (pk) { pushUndo(); _motionDrag = { mode: 'point', key: pk, pv: pv }; return true; }
      }
      var ap = hitAnchorPoint(event.point, t);
      if (ap) { pushUndo(); _motionDrag = { mode: 'anchor', holder: ap.holder, bc: ap.bc }; return true; }
    }
    return false;
  }
  function onDrag(event) {
    if (!_motionDrag) return false;
    if (_motionDrag.mode === 'unified') {
      var dx = event.point.x - _motionDrag.last.x, dy = event.point.y - _motionDrag.last.y;
      _motionDrag.last = { x: event.point.x, y: event.point.y };
      var uf = _motionDrag.frame;
      _motionDrag.u.targets.forEach(function (t) {
        var track = t.holder.motion.position;
        var k = track.keys.find(function (kk) { return kk.frame === uf; });
        if (!k) {
          // no key here yet on THIS element — freeze its interpolated
          // value as a new key so the group edit doesn't yank its whole
          // curve, then offset like the others.
          k = { frame: uf, v: valueAtFrame(t.holder, 'position', uf), curvePoints: cloneCurvePts(DEFAULT_CURVE), hOut: [0, 0], hIn: [0, 0] };
          track.keys.push(k); sortKeys(track);
        }
        k.v[0] += dx; k.v[1] += dy;
      });
    } else if (_motionDrag.mode === 'anchor') {
      setValue(_motionDrag.holder, 'anchor', [event.point.x - _motionDrag.bc.x, event.point.y - _motionDrag.bc.y]);
    } else {
      var k = _motionDrag.key, pv = _motionDrag.pv;
      // Both branches now resolve against the SAME pivot buildOverlayItems
      // draws from (motionPivotOf — bounds center + anchor offset), so a
      // key.v of [0,0] drags from exactly where its dot is drawn, matching
      // computeMotionMat's delta semantics instead of raw world coords.
      if (_motionDrag.mode === 'handle') k[_motionDrag.which] = [event.point.x - (pv.x + k.v[0]), event.point.y - (pv.y + k.v[1])];
      else { k.v[0] = event.point.x - pv.x; k.v[1] = event.point.y - pv.y; }
    }
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    return true;
  }
  function onUp() {
    if (!_motionDrag) return false;
    _motionDrag = null;
    renderLayerList(); // scrub fields must reflect the dragged position/handle
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    return true;
  }

  // ---- Motion mode UI: layer list (Transform property rows) ----
  function fmtVal(n) { return Math.round(n * 10) / 10; }
  function scrubField(value, onCommit) {
    var inp = document.createElement('input');
    inp.type = 'number'; inp.className = 'pi scrub motion-val'; inp.value = fmtVal(value);
    inp.step = 1;
    inp.addEventListener('change', function () { onCommit(parseFloat(inp.value) || 0); });
    inp.addEventListener('click', function (e) { e.stopPropagation(); });
    inp.addEventListener('mousedown', function (e) { e.stopPropagation(); });
    return inp;
  }
  // ---- Double-click a layer row (2026-07) ----
  // First tried as an "enter layer as precomp" in-place grouped view
  // (StoryBoard/Animation2D/Motion architecture diagram, CLAUDE.md §8) —
  // explicitly reversed the same day: "cette ouverture ne doit pas mettre
  // 2 shape dans un layer mais construite 2 layer séparé avec dans chacune
  // une shape". Double-click now calls splitLayerIntoElements (app.js,
  // "Release to Layers"-style: explodes the layer into N real top-level
  // layers, one per element, each carrying over its own per-element Motion
  // keys as a normal layer-level track) — nothing left to render specially
  // here, the layer list's normal per-layer loop just runs again afterward
  // on the new layers like any other layer change.
  function renderLayerListMotion(list) {
    var order = (typeof computeLayerRenderOrder === 'function') ? computeLayerRenderOrder() : state.layers.map(function (_l, i) { return { type: 'layer', idx: i }; });
    order.forEach(function (entry) {
      if (entry.type !== 'layer' || entry.hidden) return;
      var li = entry.idx, ld = state.layers[li];
      // Component instances DO now get a layer-level Transform group
      // (2026-07-17, "un calque animé dans motion... devient
      // automatiquement un component que l'on retrouvera dans animation
      // 2D") — composes cleanly with their own symMatrix placement (see
      // layerMotionAt's comment: two separate, stacking transforms, same
      // as AE's precomp-layer model), so `isComponent` below no longer
      // blocks EXPANSION, only the per-ELEMENT sub-list a few lines down
      // (renderElementsList) — a symbol instance's actual strokes live
      // inside the symbol's own sub-layer, not addressable as elements of
      // THIS layer, so per-element motion genuinely has no meaning here.
      var isComponent = !!ld.symbolId;
      var expanded = isLayerExpanded(li);
      var row = document.createElement('div');
      row.className = 'lrow' + (li === state.activeLayerIdx ? ' act' : '');
      row.dataset.layer = li;
      if (isComponent) row.title = 'Composant — Position/Anchor/Rotation/Scale/Opacity animent l\'instance entière (le contenu interne s\'édite via "Éditer le composant…")';
      var arrow = document.createElement('div'); arrow.className = 'lico larrow'; arrow.textContent = expanded ? '▾' : '▸';
      row.appendChild(arrow);
      // Same color dot / eye / lock / solo controls as Animation 2D's own
      // layer row (timeline.js's renderLayerList) — Motion mode is a
      // different VIEW of the same layers, not a different set of layers,
      // so these need to be reachable here too, and they're already wired
      // through window.SM.toggleLayerVis/Lock/Solo + layerIsEffectivelyVisible
      // (app.js), the same choke point Motion's own render pipeline already
      // goes through — no extra plumbing needed for the toggles to actually
      // affect what's shown.
      var cdot = document.createElement('div'); cdot.className = 'lico layer-color-dot'; cdot.title = 'Couleur du calque';
      cdot.style.setProperty('--dot-color', ld.color || '#8b8b9e');
      cdot.addEventListener('click', function (e) {
        e.stopPropagation();
        openLayerColorSwatches(cdot, ld.color || '#8b8b9e', function (hex) { ld.color = hex; cdot.style.setProperty('--dot-color', hex); renderTimeline(); });
      });
      row.appendChild(cdot);
      var eye = document.createElement('div'); eye.className = 'lico' + (ld.visible ? '' : ' off'); eye.title = 'Show / hide layer'; eye.innerHTML = ld.visible ? ICO_EYE : ICO_EYE_CLOSED;
      eye.addEventListener('click', function (e) { e.stopPropagation(); window.SM.toggleLayerVis(li); });
      row.appendChild(eye);
      var lock = document.createElement('div'); lock.className = 'lico' + (ld.locked ? '' : ' off'); lock.title = 'Lock / unlock layer'; lock.innerHTML = ld.locked ? ICO_LOCK : ICO_UNLOCK;
      lock.addEventListener('click', function (e) { e.stopPropagation(); window.SM.toggleLayerLock(li); });
      row.appendChild(lock);
      var solo = document.createElement('div'); solo.className = 'lico solo-btn' + (ld.solo ? ' on' : ' off'); solo.title = 'Solo layer (hide all others)'; solo.textContent = 'S';
      solo.addEventListener('click', function (e) { e.stopPropagation(); window.SM.toggleLayerSolo(li); });
      row.appendChild(solo);
      var nm = document.createElement('div'); nm.className = 'lnm'; nm.textContent = ld.name || ('Layer ' + (li + 1));
      row.appendChild(nm);
      row.addEventListener('click', function () {
        // A completed drag-drop still fires a trailing native 'click' on
        // mouseup — same guard timeline.js's own layer rows use (see its
        // comment there) so releasing a reorder drag doesn't ALSO toggle
        // this row's Transform-group expansion.
        if (window._layerDragJustEnded) { window._layerDragJustEnded = false; return; }
        // A row can be open via the single-accordion state OR via U's
        // reveal set (or both) — always drop it from the reveal set on
        // click, but only touch the single-accordion value if THIS row is
        // the one holding it, so clicking a U-revealed row never collapses
        // some unrelated row that's separately accordion-open.
        if (window._motionRevealedLayers) {
          var ri = window._motionRevealedLayers.indexOf(li);
          if (ri >= 0) window._motionRevealedLayers.splice(ri, 1);
        }
        window._motionExpandedLayer = expanded ? (window._motionExpandedLayer === li ? null : window._motionExpandedLayer) : li;
        _propFilter = null; // fresh "show all" every time the expanded layer changes
        window._motionExpandedElement = null;
        setKeySel([]);
        // window.SM.setActiveLayer(li), not a raw state.activeLayerIdx=li —
        // found live (2026-07-17, "on ne voit pas la box de transformation
        // à la selection d'un calque dans motion") : this row set the index
        // directly, bypassing setActiveLayer entirely — its "force Select
        // tool + populate selectedPaths with the layer's content" fix
        // (same commit) never ran for Motion's own layer rows, only
        // Animation 2D's, so clicking a layer here left state.tool at
        // whatever it was (almost never 'select') and selectedPaths empty
        // — buildTransformBoxItems (engine-bridge.js) requires BOTH to
        // show anything, so the box just never appeared.
        window.SM.setActiveLayer(li);
        renderLayerList(); renderTimeline();
        if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
      });
      // Feedback: "impossible de changer l'index des calques comme dans
      // animation 2D, avec drag et réorganisation des calques". Reuses the
      // EXACT same drag machinery Animation 2D's own layer rows use
      // (timeline.js's global _layerDrag + window mousemove/mouseup
      // handlers, further down in this same file's sibling) — that state
      // is a bare top-level var (timeline.js is not IIFE-wrapped), and its
      // handlers only depend on `.lrow` + `data-layer`, both already set
      // above, so no duplicate drag logic needed here at all.
      row.addEventListener('mousedown', function (e) {
        if (e.button !== 0 || e.target.closest('.lico')) return;
        _layerDrag.active = true; _layerDrag.srcIdx = li; _layerDrag.startY = e.clientY; _layerDrag.moved = false;
      });
      row.addEventListener('dblclick', function (e) {
        if (e.target.closest('.lico')) return;
        splitLayerIntoElements(li);
      });
      list.appendChild(row);
      if (!expanded) return;
      renderTransformGroup(list, ld, 'Transform');
      // Per-element sub-list stays component-exclusive: a symbol instance's
      // actual strokes live inside the SYMBOL's own sub-layer (edited via
      // "Éditer le composant…"), not addressable as elements of this outer
      // layer — only the whole-instance Transform group above applies.
      if (!isComponent) renderElementsList(list, li, ld);
    });
    // Right-panel mirror of the active layer's Transform group ("il
    // faudrait afficher les properties d'un calque sélectionné et la
    // possibilité d'ajouter des keyframes dans le panel de droite") —
    // refreshed here because renderLayerListMotion is already the ONE
    // place every relevant change funnels through (layer selection,
    // stopwatch toggles, frame navigation via updateUI → renderLayerList),
    // so the panel can never go stale against the bottom list.
    renderMotionPropsPanel();
  }
  // Populates #motion-props-body (index.html, right panel — hidden outside
  // Motion mode via body:not(.mode-motion) CSS) with the ACTIVE layer's
  // Transform rows. Reuses renderTransformGroup verbatim — the rows are
  // identical to the bottom panel's by construction (same builder, same
  // holder), including the stopwatch and the add-key diamond, so both
  // locations can add/remove keyframes and neither can drift from the
  // other's behavior.
  function renderMotionPropsPanel() {
    var body = document.getElementById('motion-props-body');
    if (!body) return;
    body.innerHTML = '';
    var ld = state.layers[state.activeLayerIdx];
    var nameRow = document.createElement('div');
    nameRow.className = 'motion-props-layername';
    if (!ld) { nameRow.textContent = 'Aucun calque sélectionné'; body.appendChild(nameRow); return; }
    // Component instances get the same Transform group as any layer now
    // (see renderLayerListMotion's comment) — just a label suffix so it
    // reads clearly as "the whole instance", not its internal content.
    nameRow.textContent = (ld.name || ('Layer ' + (state.activeLayerIdx + 1))) + (ld.symbolId ? ' (composant)' : '');
    body.appendChild(nameRow);
    renderTransformGroup(body, ld, 'Transform');
  }
  // Shared by the layer's own Transform group AND each element's — both are
  // just "a holder with .motion/.motionStatic", see the header comment on
  // ensureElementHolder. `refreshDeep` is called after any structural change
  // (stopwatch toggle) since that can affect which rows/tracks exist;
  // scrubbing a value only needs the timeline (track content) + canvas.
  function renderTransformGroup(list, holder, groupLabel) {
    var grp = document.createElement('div'); grp.className = 'lrow motion-group-row';
    var grpLabel = document.createElement('span'); grpLabel.textContent = groupLabel;
    grp.appendChild(grpLabel);
    var filterBtn = document.createElement('span'); filterBtn.className = 'motion-filter-btn' + (_hideUnanimated ? ' on' : '');
    filterBtn.title = _hideUnanimated ? 'Afficher toutes les propriétés' : 'N’afficher que les propriétés animées';
    filterBtn.innerHTML = '<svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 5h18l-7 8v6l-4-2v-4z"/></svg>';
    filterBtn.addEventListener('click', function (e) {
      e.stopPropagation();
      _hideUnanimated = !_hideUnanimated;
      // Both sides — panel AND grid share the exact same skip condition
      // (renderTransformGroup/renderTracksFor); calling only one here would
      // silently desync which rows exist between them, the same alignment
      // bug class ROW_H's own header comment already warns about.
      renderLayerList(); renderTimeline();
    });
    grp.appendChild(filterBtn);
    list.appendChild(grp);
    PROPS.forEach(function (prop) {
      if (isPropFiltered(prop) || (_hideUnanimated && !propHasContent(holder, prop))) return;
      var pr = document.createElement('div'); pr.className = 'lrow motion-prop-row';
      var sw = document.createElement('div');
      var swOn = isAnimated(holder, prop);
      var hasKeyHere = swOn && !!keyAt(holder.motion[prop], state.currentFrame);
      // Single diamond, three states — merges what used to be two separate
      // icons (this stopwatch AND a second .motion-addkey diamond appended
      // after the value fields further down): they always showed the exact
      // same on/off information twice on the same row, reported as a visual
      // duplicate ("supprimé la 2e keyframe qui apparait, ça fait doublon").
      // Not animated at all: hollow, neutral (--text-dark, no .on class).
      // Animated but no key at the current frame: hollow with the BLUE
      // outline (.on gives color:accent to the SVG's currentColor stroke,
      // fill stays none) — signals "this property IS keyframed, just not
      // right here", the one state the old single stopwatch icon couldn't
      // show on its own. Animated AND a key sits exactly here: solid blue
      // fill, same as before.
      sw.className = 'lico motion-stopwatch' + (swOn ? ' on' : '');
      sw.title = !swOn ? 'Activer l’animation de cette propriété' : (hasKeyHere ? 'Retirer la clé à la frame courante' : 'Ajouter une clé à la frame courante');
      sw.innerHTML = '<svg viewBox="0 0 24 24" width="12" height="12"><path d="M12 3l9 9-9 9-9-9z" fill="' + (hasKeyHere ? 'currentColor' : 'none') + '" stroke="currentColor" stroke-width="2"/></svg>';
      sw.addEventListener('click', function (e) {
        e.stopPropagation(); pushUndo();
        if (!swOn) {
          toggleAnimated(holder, prop); // OFF->ON: first key at the current frame (see toggleAnimated's own comment)
        } else if (hasKeyHere) {
          // Removing the LAST key would drop the property back to its
          // neutral default — freeze the current value as a static
          // override instead, exactly like toggleAnimated's own ON->OFF
          // branch ("switching modes must never silently snap a layer
          // back to its neutral default", its header comment).
          if (holder.motion[prop].keys.length === 1) {
            var fv = valueAtFrame(holder, prop, state.currentFrame);
            holder.motion[prop] = { keys: [] };
            if (!holder.motionStatic) holder.motionStatic = {};
            holder.motionStatic[prop] = fv;
          } else {
            removeKeyAtCurrentFrame(holder, prop);
          }
        } else {
          setKeyAtCurrentFrame(holder, prop, valueAtFrame(holder, prop, state.currentFrame));
        }
        renderLayerList(); renderTimeline();
        if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
      });
      var pnm = document.createElement('div'); pnm.className = 'lnm motion-prop-name';
      pnm.innerHTML = '<span>' + PROP_LABEL[prop] + '</span>';
      pr.appendChild(sw); pr.appendChild(pnm);
      var vals = isAnimated(holder, prop) ? valueAtFrame(holder, prop, state.currentFrame) : staticValue(holder, prop);
      var fieldWrap = document.createElement('div'); fieldWrap.className = 'motion-fields';
      for (var d = 0; d < PROP_DIM[prop]; d++) {
        (function (dim) {
          var f = scrubField(vals[dim], function (nv) {
            pushUndo();
            var nvals = isAnimated(holder, prop) ? valueAtFrame(holder, prop, state.currentFrame) : staticValue(holder, prop);
            nvals[dim] = nv;
            setValue(holder, prop, nvals);
            // renderLayerList too (not just the timeline, the original
            // single-panel behavior): these same rows now render in TWO
            // places (bottom Transform group + right-panel mirror,
            // renderMotionPropsPanel) — committing a value in one must
            // refresh the other's copy of the field, or they visibly
            // disagree until the next unrelated refresh.
            renderLayerList(); renderTimeline();
            if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
          });
          fieldWrap.appendChild(f);
        })(d);
      }
      var unit = document.createElement('span'); unit.className = 'motion-unit'; unit.textContent = PROP_UNIT[prop];
      fieldWrap.appendChild(unit);
      pr.appendChild(fieldWrap);
      list.appendChild(pr);
    });
  }
  // Every unique element (stroke) visible in the layer at the CURRENT frame
  // — "Éléments", AE shape-group style, listed under the layer's own
  // Transform group. A layer with a single cohesive drawing shows one
  // element (harmless — nesting an identity transform costs nothing); a
  // layer with several unrelated shapes (the exact case that broke the
  // layer-wide Anchor Point, see computeMotionMat's header comment) can now
  // target each one independently, each pivoting around its OWN bounds.
  function layerElements(li, ld) {
    var strokes = getEffectiveStrokes(li, state.currentFrame) || [];
    var out = [];
    strokes.forEach(function (sd, i) {
      // A brush-texture companion (bitmap raster or vector dab group, both
      // tagged isBrushTextureCopy + a brushGroupId shared with their
      // anchor) is the SAME visual shape as its anchor, not a separate
      // one — reported live (2026-07, screenshot showing a "Forme 1" +
      // "Image 2" pair for one hand-drawn stroke): listing it as its own
      // Elements row let you key it independently of the anchor it's
      // camouflage-glued to, which just desyncs the texture from the
      // shape the moment either one alone gets animated. Folded out here;
      // engine-bridge.js's buildSceneJson resolves the companion's
      // element-motion through its ANCHOR's strokeId instead of its own,
      // so animating "the shape" (the one row left) carries the texture
      // along automatically.
      if (sd.isBrushTextureCopy) return;
      // Lazily stamp a strokeId onto legacy stroke data that predates this
      // feature (or fillWalls/team-review, the other lazy-assign consumers)
      // — getEffectiveStrokes returns the LIVE array reference for a real
      // keyframe (not a clone), so this mutation persists on next save,
      // same lazy-assign contract ensureStrokeId already has for live Paper
      // items (tools.js).
      if (!sd.strokeId) sd.strokeId = 's' + Date.now().toString(36) + '_' + i + '_' + Math.floor(Math.random() * 1e6);
      out.push({ strokeId: sd.strokeId, sd: sd });
    });
    return out;
  }
  function elementLabel(entry, idx) {
    var sd = entry.sd;
    if (sd.isRaster) return 'Image ' + (idx + 1);
    return (sd.fillColor ? 'Forme' : 'Trait') + ' ' + (idx + 1);
  }
  function renderElementsList(list, li, ld) {
    var els = layerElements(li, ld);
    if (!els.length) return;
    var hdr = document.createElement('div'); hdr.className = 'lrow motion-group-row'; hdr.textContent = 'Éléments';
    list.appendChild(hdr);
    els.forEach(function (entry, idx) {
      var expanded = window._motionExpandedElement === entry.strokeId;
      var row = document.createElement('div'); row.className = 'lrow motion-elem-row';
      var swatch = document.createElement('div'); swatch.className = 'motion-elem-swatch';
      swatch.style.background = entry.sd.fillColor || entry.sd.strokeColor || 'transparent';
      if (elementHasMotion(ld, entry.strokeId)) swatch.classList.add('has-motion');
      var arrow = document.createElement('div'); arrow.className = 'lico larrow'; arrow.textContent = expanded ? '▾' : '▸';
      var nm = document.createElement('div'); nm.className = 'lnm'; nm.textContent = elementLabel(entry, idx);
      row.appendChild(arrow); row.appendChild(swatch); row.appendChild(nm);
      row.addEventListener('click', function () {
        window._motionExpandedElement = expanded ? null : entry.strokeId;
        renderLayerList(); renderTimeline();
        if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
      });
      list.appendChild(row);
      if (!expanded) return;
      renderTransformGroup(list, ensureElementHolder(ld, entry.strokeId), 'Transform (élément)');
    });
  }

  // ---- Motion mode UI: keyframe tracks (mirrors the layer list's rows) ----
  // `ld` here is really "the holder" (see ensureElementHolder's header
  // comment) — it's the layer object itself for a layer's own tracks, or
  // an element holder (ld.elementMotion[strokeId]) for an element's, same
  // generic contract as renderTransformGroup/isAnimated/valueAtFrame/etc.
  function trackRowHtml(ld, prop, rowEl) {
    rowEl.innerHTML = '';
    rowEl.style.position = 'relative';
    // Tagged directly on the element (plain JS properties, not a WeakMap —
    // simplest way for the marquee-select code below to recover "which
    // holder/prop does this row belong to" from a DOM hit-test without
    // threading extra state through render calls).
    rowEl._smHolder = ld; rowEl._smProp = prop;
    var track = ld.motion && ld.motion[prop];
    var w = state.totalFrames * FC;
    var svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('width', w); svg.setAttribute('height', ROW_H);
    svg.style.cssText = 'position:absolute;left:0;top:0;pointer-events:none;';
    rowEl.appendChild(svg);
    if (track && track.keys.length) {
      // Connection bar between consecutive keys whose value actually
      // changes — AE-wishlist idea (sandervandijk.tv "Connection"/"Keyframe
      // Duration"): makes it obvious at a glance WHERE movement happens
      // instead of a row of identical-looking diamonds with no context.
      var barH = Math.max(3, Math.round(ROW_H * 0.27)), barY = Math.round((ROW_H - barH) / 2);
      for (var i = 0; i < track.keys.length - 1; i++) {
        var a = track.keys[i], b = track.keys[i + 1];
        var changed = a.v.some(function (v, d) { return v !== b.v[d]; });
        if (!changed) continue;
        var rect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
        rect.setAttribute('x', a.frame * FC + FC / 2); rect.setAttribute('y', barY);
        rect.setAttribute('width', (b.frame - a.frame) * FC); rect.setAttribute('height', barH);
        rect.setAttribute('fill', 'var(--accent)'); rect.setAttribute('opacity', '0.35');
        svg.appendChild(rect);
      }
    }
    for (var fi = 0; fi < state.totalFrames; fi++) {
      var c = document.createElement('div');
      c.className = 'fc motion-fc' + (fi === state.currentFrame ? ' cur' : '');
      c.dataset.frame = fi;
      var k = track ? keyAt(track, fi) : null;
      if (k) {
        var dia = document.createElement('div');
        // Hold keys render as a square, not the usual diamond (AE/Caddis
        // convention — the shape itself communicates "no interpolation
        // out of this key" without needing to open the curve editor).
        dia.className = 'motion-key' + (fi === state.currentFrame ? ' cur' : '') + (isKeySelected(ld, prop, k) ? ' sel' : '') + (k.hold ? ' hold' : '');
        c.appendChild(dia);
      }
      (function (frameIdx, key) {
        c.addEventListener('mousedown', function (e) {
          e.stopPropagation();
          if (key) {
            // Dragging a key that's already part of a multi-selection moves
            // the WHOLE group together, one frame-delta shared by all of
            // them; grabbing an unselected key resets the selection to just
            // that one (matches marquee-select's own "click empty = clear"
            // convention below). Alt+drag instead STAGGERS — Skew Pro's
            // headline "grab and drag to skew" gesture, brought into
            // Motion's own keyframe timeline (staggerBars, layer-inout.js,
            // already covers this for layer in/out bars; this is the
            // missing piece for actual property keyframes). Grouped by
            // HOLDER (layer or element — not by property track), in the
            // order each holder was first encountered in the marquee
            // selection — applyMarqueeSelection above builds that selection
            // by iterating .motion-track-row in DOCUMENT order, i.e. the
            // same top-to-bottom visual order the rows render in, so this
            // naturally matches "top row anchors, lower rows cascade"
            // without needing a separate sort. Only meaningful across 2+
            // holders — a selection confined to one layer's own tracks
            // falls back to the ordinary uniform group move.
            pushUndo();
            if (isKeySelected(ld, prop, key)) {
              if (e.altKey && _motionKeySel.length >= 2) {
                var holderOrder = [];
                _motionKeySel.forEach(function (s) { if (holderOrder.indexOf(s.holder) < 0) holderOrder.push(s.holder); });
                if (holderOrder.length >= 2) {
                  window._motionKeyDrag = { stagger: true, startX: e.clientX, keys: _motionKeySel.slice(), holderOrder: holderOrder };
                } else {
                  window._motionKeyDrag = { group: true, startX: e.clientX, keys: _motionKeySel.slice() };
                }
              } else {
                window._motionKeyDrag = { group: true, startX: e.clientX, keys: _motionKeySel.slice() };
              }
            } else {
              setKeySel([{ holder: ld, prop: prop, key: key }]);
              window._motionKeyDrag = { ld: ld, prop: prop, key: key, startX: e.clientX, startFrame: key.frame };
            }
          } else {
            startMarquee(e);
          }
          goToFrame(frameIdx);
        });
        c.addEventListener('contextmenu', function (e) {
          e.preventDefault(); e.stopPropagation();
          goToFrame(frameIdx);
          var track = ld.motion && ld.motion[prop];
          var menu = [
            key
              ? { label: 'Supprimer cette clé', action: function () { pushUndo(); var tr = ld.motion[prop]; tr.keys.splice(tr.keys.indexOf(key), 1); renderTimeline(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); } }
              : { label: 'Ajouter une clé ici', action: function () { pushUndo(); setKeyAtCurrentFrame(ld, prop, isAnimated(ld, prop) ? valueAtFrame(ld, prop, frameIdx) : staticValue(ld, prop)); renderLayerList(); renderTimeline(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); } },
          ];
          if (track && track.keys.length) {
            // A single-key track auto-creates its missing second key on open
            // (see openMotionEaseEditor's header comment) — no need to
            // require a full segment already existing before offering this.
            menu.push({ label: 'Éditer la courbe d’accélération…', action: function () { pushUndo(); openMotionEaseEditor(ld, prop); } });
          }
          if (key) {
            // Hold keyframe (2026-07): no interpolation out of this key —
            // the value snaps to the NEXT key's value the instant it's
            // reached. Renders as a square (see the .hold class above).
            menu.push({ label: key.hold ? 'Retirer le maintien (hold)' : 'Maintenir (hold)', action: function () { pushUndo(); key.hold = !key.hold; renderLayerList(); renderTimeline(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); } });
          }
          // Batch ops (Skew Pro punch list) act on the WHOLE current
          // multi-selection, offered regardless of which key/cell was
          // right-clicked. No Align here (unlike layer bars) — a keyframe
          // has no duration, "align" doesn't map onto a single point.
          if (_motionKeySel.length >= 2) {
            menu.push({ sep: true });
            menu.push({ label: 'Distribuer uniformément', action: distributeKeys });
            menu.push({ label: 'Inverser l’ordre (flip)', action: flipKeys });
            menu.push({ label: 'Sélectionner 1 sur 2', action: function () { selectEveryNthKey(2); } });
          }
          if (_motionKeySel.length >= 1) menu.push({ label: 'Inverser la sélection', action: invertKeySelection });
          window.showContextMenu(e.clientX, e.clientY, menu);
        });
      })(fi, k);
      rowEl.appendChild(c);
    }
  }
  function renderTracksFor(grid, holder, prop) {
    // Must mirror renderTransformGroup's own skip condition exactly (same
    // alignment-invariant this file's whole panel/grid split depends on —
    // see ROW_H's own header comment).
    if (isPropFiltered(prop) || (_hideUnanimated && !propHasContent(holder, prop))) return;
    var row = document.createElement('div'); row.className = 'frow motion-track-row';
    trackRowHtml(holder, prop, row);
    grid.appendChild(row);
  }
  function renderTimelineMotion(grid) {
    var order = (typeof computeLayerRenderOrder === 'function') ? computeLayerRenderOrder() : state.layers.map(function (_l, i) { return { type: 'layer', idx: i }; });
    order.forEach(function (entry) {
      if (entry.type !== 'layer' || entry.hidden) return;
      var li = entry.idx, ld = state.layers[li];
      var expanded = isLayerExpanded(li);
      var spacer = document.createElement('div'); spacer.className = 'frow';
      if (window.SMLayerInOut) SMLayerInOut.buildBar(spacer, li);
      grid.appendChild(spacer);
      if (!expanded) return;
      var grpSpacer = document.createElement('div'); grpSpacer.className = 'frow';
      grid.appendChild(grpSpacer);
      PROPS.forEach(function (prop) { renderTracksFor(grid, ld, prop); });
      // Mirrors renderElementsList's panel structure: one spacer per
      // element, its own track rows only when that ONE element is expanded
      // (only one element expands at a time, same single-expand contract
      // as layers).
      var els = layerElements(li, ld);
      if (els.length) {
        var elHdrSpacer = document.createElement('div'); elHdrSpacer.className = 'frow';
        grid.appendChild(elHdrSpacer);
        els.forEach(function (entry) {
          var elExpanded = window._motionExpandedElement === entry.strokeId;
          var elSpacer = document.createElement('div'); elSpacer.className = 'frow';
          grid.appendChild(elSpacer);
          if (!elExpanded) return;
          PROPS.forEach(function (prop) { renderTracksFor(grid, ensureElementHolder(ld, entry.strokeId), prop); });
        });
      }
    });
  }
  // ---- multi-select (marquee rectangle) + group drag ----
  // AE/Skew Pro convention: drag a selection rectangle over the keyframe
  // grid to select several keys across one or more property tracks at
  // once (position/rotation/scale/... — layer OR element tracks, doesn't
  // matter which), then drag any ONE of the selected diamonds to retime
  // the whole group together by the same frame delta.
  var _motionKeySel = []; // [{holder, prop, key}]
  function isKeySelected(holder, prop, key) {
    return _motionKeySel.some(function (s) { return s.holder === holder && s.prop === prop && s.key === key; });
  }
  function setKeySel(sel) { _motionKeySel = sel; }
  // ---- batch operations on the current keyframe selection (Skew Pro
  // punch list: Distribute/Flip/Select Every/Invert Selection — no Align
  // here, unlike layer bars: a keyframe has no duration, "align" doesn't
  // map onto a single point). Grouped PER TRACK (holder+prop) first: two
  // selected keys on DIFFERENT properties shouldn't need to avoid
  // colliding with each other, only with other keys on their OWN track.
  function _groupKeySelByTrack() {
    var groups = [];
    _motionKeySel.forEach(function (s) {
      var g = groups.filter(function (g2) { return g2.holder === s.holder && g2.prop === s.prop; })[0];
      if (!g) { g = { holder: s.holder, prop: s.prop, items: [] }; groups.push(g); }
      g.items.push(s.key);
    });
    return groups;
  }
  function distributeKeys() {
    if (_motionKeySel.length < 2) { if (window.showToast) showToast('Sélectionne au moins 2 clés'); return; }
    pushUndo();
    _groupKeySelByTrack().forEach(function (g) {
      if (g.items.length < 2) return;
      var sorted = g.items.slice().sort(function (a, b) { return a.frame - b.frame; });
      var first = sorted[0].frame, last = sorted[sorted.length - 1].frame;
      var step = (last - first) / (sorted.length - 1);
      sorted.forEach(function (k, i) { k.frame = Math.round(first + step * i); });
      sortKeys(g.holder.motion[g.prop]);
    });
    renderTimeline();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  function flipKeys() {
    if (_motionKeySel.length < 2) { if (window.showToast) showToast('Sélectionne au moins 2 clés'); return; }
    pushUndo();
    _groupKeySelByTrack().forEach(function (g) {
      if (g.items.length < 2) return;
      var sorted = g.items.slice().sort(function (a, b) { return a.frame - b.frame; });
      var slots = sorted.map(function (k) { return k.frame; }).reverse();
      sorted.forEach(function (k, i) { k.frame = slots[i]; });
      sortKeys(g.holder.motion[g.prop]);
    });
    renderTimeline();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  function selectEveryNthKey(n) {
    n = Math.max(2, parseInt(n, 10) || 2);
    var sorted = _motionKeySel.slice().sort(function (a, b) { return a.key.frame - b.key.frame; });
    _motionKeySel = sorted.filter(function (_s, i) { return i % n === 0; });
    renderTimeline();
    if (window.showToast) showToast(_motionKeySel.length + ' clé(s) sélectionnée(s)');
  }
  // Inverts within whatever tracks are CURRENTLY RENDERED (the same
  // universe the marquee itself draws over) — matches Skew Pro's own "I"
  // shortcut semantics.
  function invertKeySelection() {
    var all = [], prevSel = _motionKeySel;
    document.querySelectorAll('.motion-track-row').forEach(function (rowEl) {
      var holder = rowEl._smHolder, prop = rowEl._smProp;
      var track = holder && holder.motion && holder.motion[prop];
      if (!track) return;
      track.keys.forEach(function (k) { all.push({ holder: holder, prop: prop, key: k }); });
    });
    _motionKeySel = all.filter(function (s) {
      return !prevSel.some(function (s2) { return s2.holder === s.holder && s2.prop === s.prop && s2.key === s.key; });
    });
    renderTimeline();
  }
  var _motionMarquee = null; // {startX, startY, rectEl, moved}
  function startMarquee(e) {
    var rect = document.createElement('div'); rect.className = 'motion-marquee-rect';
    document.body.appendChild(rect);
    _motionMarquee = { startX: e.clientX, startY: e.clientY, rectEl: rect, moved: false };
  }
  function applyMarqueeSelection(x0, y0, x1, y1) {
    var sel = [];
    document.querySelectorAll('.motion-track-row').forEach(function (rowEl) {
      var holder = rowEl._smHolder, prop = rowEl._smProp;
      if (!holder) return;
      rowEl.querySelectorAll('.motion-key').forEach(function (dia) {
        var b = dia.getBoundingClientRect();
        var cx = b.left + b.width / 2, cy = b.top + b.height / 2;
        var hit = cx >= x0 && cx <= x1 && cy >= y0 && cy <= y1;
        dia.classList.toggle('sel', hit);
        if (!hit) return;
        var cell = dia.parentElement;
        var frame = parseInt(cell.dataset.frame, 10);
        var track = holder.motion && holder.motion[prop];
        var key = track ? keyAt(track, frame) : null;
        if (key) sel.push({ holder: holder, prop: prop, key: key });
      });
    });
    setKeySel(sel);
  }
  function updateMarquee(e) {
    if (!_motionMarquee) return;
    var dx = e.clientX - _motionMarquee.startX, dy = e.clientY - _motionMarquee.startY;
    if (Math.abs(dx) > 3 || Math.abs(dy) > 3) _motionMarquee.moved = true;
    var x0 = Math.min(_motionMarquee.startX, e.clientX), y0 = Math.min(_motionMarquee.startY, e.clientY);
    var w = Math.abs(dx), h = Math.abs(dy);
    var r = _motionMarquee.rectEl;
    r.style.left = x0 + 'px'; r.style.top = y0 + 'px'; r.style.width = w + 'px'; r.style.height = h + 'px';
    if (_motionMarquee.moved) applyMarqueeSelection(x0, y0, x0 + w, y0 + h);
  }
  function endMarquee() {
    if (!_motionMarquee) return;
    var moved = _motionMarquee.moved;
    _motionMarquee.rectEl.remove();
    _motionMarquee = null;
    // A plain click on empty grid space (no drag) clears the selection,
    // same "click empty = deselect" convention as the canvas's own
    // marquee/selection tools elsewhere in this app.
    if (!moved) { setKeySel([]); renderTimeline(); }
  }

  // Drag-to-retime a keyframe (mousemove/up delegated from ui.js's global
  // pointer handlers via SMMotion.onDragMove/onDragUp, same pattern as the
  // span-end/keyframe drag handlers already in timeline.js).
  function onDragMove(e) {
    updateMarquee(e);
    var d = window._motionKeyDrag; if (!d) return;
    var deltaFrames = Math.round((e.clientX - d.startX) / FC);
    if (d.stagger) {
      // Skew Pro's stagger: holder rank 0 (topmost row in the selection)
      // is the anchor and never moves; holder rank i gets i× the raw drag
      // delta. Same incremental-tick/re-baseline shape as the uniform
      // group move below (add THIS tick's small delta, re-baseline startX)
      // so the accumulated per-holder offset telescopes correctly back to
      // rank × total-mouse-delta-from-drag-start, without needing to track
      // each key's original frame separately.
      if (!deltaFrames) return;
      var ok = d.keys.every(function (s) {
        var rank = d.holderOrder.indexOf(s.holder);
        if (rank <= 0) return true; // anchor holder never moves, never collides
        var nf = s.key.frame + deltaFrames * rank;
        if (nf < 0 || nf >= state.totalFrames) return false;
        var existing = keyAt(s.holder.motion[s.prop], nf);
        return !existing || existing === s.key;
      });
      if (!ok) return;
      d.keys.forEach(function (s) {
        var rank = d.holderOrder.indexOf(s.holder);
        if (rank <= 0) return;
        s.key.frame += deltaFrames * rank;
        sortKeys(s.holder.motion[s.prop]);
      });
      d.startX = e.clientX;
      renderTimeline();
      return;
    }
    if (d.group) {
      // Whole-group move: compute the delta from ONE reference key (the
      // first), then check EVERY selected key can land there without
      // colliding with an UNselected key already at the target frame —
      // an all-or-nothing move keeps the group's relative spacing intact
      // rather than silently dropping just the colliding member.
      if (!deltaFrames) return;
      var ok = d.keys.every(function (s) {
        var nf = s.key.frame + deltaFrames;
        if (nf < 0 || nf >= state.totalFrames) return false;
        var existing = keyAt(s.holder.motion[s.prop], nf);
        return !existing || existing === s.key;
      });
      if (!ok) return;
      d.keys.forEach(function (s) { s.key.frame += deltaFrames; sortKeys(s.holder.motion[s.prop]); });
      d.startX = e.clientX; // re-baseline so the next move is a fresh delta from here
      renderTimeline();
      return;
    }
    var nf = Math.max(0, Math.min(state.totalFrames - 1, d.startFrame + deltaFrames));
    if (nf === d.key.frame) return;
    var track = d.ld.motion[d.prop];
    if (keyAt(track, nf)) return; // don't stomp an existing key
    d.key.frame = nf; sortKeys(track);
    renderTimeline();
  }
  function onDragUp() {
    endMarquee();
    if (!window._motionKeyDrag) return;
    window._motionKeyDrag = null;
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }
  document.addEventListener('mousemove', onDragMove);
  document.addEventListener('mouseup', onDragUp);

  // ---- mode switching ----
  function setAppMode(mode) {
    if (state.appMode === mode) return;
    // Leaving Motion mode: the shared ease-curve widget (see
    // openMotionEaseEditor) may still be pointed at a motion key whose row
    // is about to disappear — fall back to the plain tween-curve view, same
    // precedent as camera.js's own exitCameraSeg() call when its tool
    // deactivates.
    if (state.appMode === 'motion' && window._curveEditor) window._curveEditor.exitMotionSeg();
    // U's multi-layer reveal set (handleRevealAnimatedShortcut) is
    // Motion-only transient UI state — clear it on the way out so
    // switching back into Motion later starts fresh rather than reopening
    // whatever U last revealed.
    if (state.appMode === 'motion' && mode !== 'motion') {
      window._motionRevealedLayers = null;
    }
    state.appMode = mode;
    document.querySelectorAll('.app-mode-btn').forEach(function (b) { b.classList.toggle('active', b.dataset.mode === mode); });
    document.body.classList.toggle('mode-motion', mode === 'motion');
    document.body.classList.toggle('mode-storyboard', mode === 'storyboard');
    // StoryBoard swaps the whole timeline area for the node space —
    // renderLayerList/renderTimeline still run (their targets are hidden,
    // harmless) so switching BACK lands on an up-to-date grid.
    if (window.SMStoryboard) SMStoryboard.setVisible(mode === 'storyboard');
    // Every right-panel section starts collapsed by design (ui.js) — fine
    // for 2D's Fill/Stroke (secondary to the canvas), but Motion's own
    // "Propriétés du calque" section IS the entire point of the mode, so
    // requiring a manual click on every mode switch was pure friction.
    // Force it open on every switch INTO Motion (not just the first) —
    // deliberately doesn't remember a mid-session manual collapse, since
    // the whole reason to be in Motion mode is to see this panel.
    if (mode === 'motion') {
      var mpHdr = document.querySelector('#motion-props-sec .phdr');
      var mpBody = document.getElementById('motion-props-body');
      if (mpHdr && mpBody && mpBody.classList.contains('hid')) {
        mpBody.classList.remove('hid');
        mpHdr.classList.remove('closed');
      }
    }
    renderLayerList(); renderTimeline();
    if (window.renderSymbolTabs) renderSymbolTabs();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }
  function initModeSwitch() {
    document.querySelectorAll('.app-mode-btn').forEach(function (b) {
      if (b.disabled) return;
      b.addEventListener('click', function () { setAppMode(b.dataset.mode); });
    });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', initModeSwitch); else initModeSwitch();

  window.SMMotion = {
    valueAtFrame: valueAtFrame,
    isAnimated: isAnimated,
    // Layer-level get/set for external gesture writers (select-bridge's
    // native-video footage drag) — same semantics as the Transform panel
    // fields: reads the effective value at the playhead, writes through
    // setValue (static override when the stopwatch is off, auto-keyframe
    // at the current frame when it's on — the AE convention).
    getLayerValue: function (li, prop) { var ld = state.layers[li]; return ld ? valueAtFrame(ld, prop, state.currentFrame) : null; },
    // Geometry-space <-> rendered-world point mapping for the layer's
    // CURRENT Motion transform ("si on rotate la propriété dans le panel
    // la box tourne pas avec l'objet") — the transform box and its gesture
    // math compose this so the gizmo sits on the object WHERE IT RENDERS,
    // not where its raw Paper geometry lies. fwd = exactly the per-point
    // formula transformSegments applies at render time (scale about pivot,
    // rotate about pivot, then translate); inv is its closed-form inverse.
    // Returns null when the layer has no effective transform (the
    // overwhelmingly common case — callers skip all mapping then).
    layerMotionPointMap: function (li) {
      var mm = layerMotionAt(li, state.currentFrame);
      if (!mm) return null;
      var lb = userLayers[li] && userLayers[li].bounds;
      if (!lb) return null;
      var px = lb.center.x + mm.ax, py = lb.center.y + mm.ay;
      var r = mm.rot * Math.PI / 180, c = Math.cos(r), s = Math.sin(r);
      return {
        mat: mm,
        fwd: function (x, y) {
          var lx = (x - px) * mm.sx, ly = (y - py) * mm.sy;
          return [px + lx * c - ly * s + mm.dx, py + lx * s + ly * c + mm.dy];
        },
        inv: function (x, y) {
          var wx = x - mm.dx - px, wy = y - mm.dy - py;
          var lx = wx * c + wy * s, ly = -wx * s + wy * c;
          return [px + lx / (mm.sx || 1e-6), py + ly / (mm.sy || 1e-6)];
        },
        invVec: function (x, y) {
          var lx = x * c + y * s, ly = -x * s + y * c;
          return [lx / (mm.sx || 1e-6), ly / (mm.sy || 1e-6)];
        },
      };
    },
    setLayerValue: function (li, prop, vals) { var ld = state.layers[li]; if (ld) setValue(ld, prop, vals); },
    layerMotionAt: layerMotionAt,
    elementMotionAt: elementMotionAt,
    transformSegments: transformSegments,
    transformImageRect: transformImageRect,
    buildOverlayItems: buildOverlayItems,
    renderLayerListMotion: renderLayerListMotion,
    renderTimelineMotion: renderTimelineMotion,
    setAppMode: setAppMode,
    onDown: onDown,
    onDrag: onDrag,
    onUp: onUp,
    handlePropShortcut: handlePropShortcut,
    revealAnimated: handleRevealAnimatedShortcut,
    layerElements: layerElements,
    elementLabel: elementLabel,
    distributeKeys: distributeKeys, flipKeys: flipKeys, selectEveryNthKey: selectEveryNthKey, invertKeySelection: invertKeySelection,
    getKeySelection: function () { return _motionKeySel.slice(); },
    // ui.js's shared curve widget calls this after a motion segment's
    // curvePoints change (drag/preset/add/delete point) — the canvas needs
    // a repaint since the eased value at the current frame may have
    // changed; the panel's scrub field too, if the playhead sits inside
    // the segment being reshaped.
    onEaseSegChanged: function () { renderLayerList(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); },
  };
})();
