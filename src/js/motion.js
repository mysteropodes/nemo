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
  // 3D layer (2026-07-28, After-Effects-style "3D layer" toggle,
  // ld.threeD) — three EXTRA scalar properties, revealed only when the
  // layer/holder has 3D on: positionZ (depth), rotationX/rotationY (the
  // existing 'rotation' stays exactly as-is, now read as Z-rotation).
  // Deliberately NOT a 3rd dimension bolted onto 'position' itself (which
  // would ripple through every PROP_DIM-keyed call site in this file) —
  // three independent dim-1 properties reuse the exact same generic
  // track/keyframe/interpolation machinery 'rotation'/'opacity' already
  // do, zero changes needed to that machinery.
  var PROPS_WITH_3D = ['position', 'positionZ', 'anchor', 'rotation', 'rotationX', 'rotationY', 'scale', 'opacity'];
  // Mograph duplicator (2026-07-29, ld.duplicator) — four EXTRA keyframable
  // per-copy DELTAS (each copy k gets k× the delta, or a seeded random in
  // ±delta — see applyLayerDuplicator, app.js), revealed only when the
  // layer has a duplicator. Same "extra ordinary properties, zero new
  // machinery" approach as PROPS_WITH_3D above. The structural config
  // (grid/radial/path mode, counts, seed) is deliberately NOT here — it's
  // static per-layer state edited in its own panel (#duplicator-sec), like
  // a Component instance's own placement fields.
  var PROPS_DUP_EXTRA = ['dupOffsetPos', 'dupOffsetRot', 'dupOffsetScale', 'dupOffsetOpacity'];
  // Time Remap (AE, 2026-07-25) is an EXTRA row, not a 6th transform: it
  // never feeds computeMotionMat — it drives which internal frame a
  // component instance shows (resolveSymbolFrameIdx, app.js). Both the
  // panel and the grid must iterate the same list or their rows desync,
  // which is the alignment invariant ROW_H's header comment is about — so
  // there is exactly ONE function that decides, and both sides call it.
  function propsFor(holder) {
    var list = (holder && holder.threeD) ? PROPS_WITH_3D : PROPS;
    if (holder && holder.duplicator) list = list.concat(PROPS_DUP_EXTRA);
    if (holder && holder.timeRemap) return list.concat(['timeRemap']);
    return list;
  }
  // THE track resolver (2026-07-26). Every transform property's track lives
  // in holder.motion[prop] — except timeRemap, which predates its own row
  // here and lives at ld.timeRemap (enableTimeRemap; consumed by app.js's
  // resolveSymbolFrameIdx, exported/imported as its own layer field). Found
  // live: the Time Remap row rendered with ZERO keys — every panel/grid
  // site resolved `holder.motion['timeRemap']` (undefined), so the two keys
  // enableTimeRemap creates were invisible, unclickable, undraggable. Any
  // code that can meet prop === 'timeRemap' (everything inside a propsFor
  // loop, every _motionKeySel consumer) MUST resolve through this — reading
  // holder.motion[prop] directly is only safe in PROPS-only loops.
  function trackFor(holder, prop) {
    if (!holder) return null;
    if (prop === 'timeRemap') return holder.timeRemap || null;
    return (holder.motion && holder.motion[prop]) || null;
  }
  var PROP_LABEL = { position: 'Position', anchor: 'Anchor Point', rotation: 'Rotation', scale: 'Scale', opacity: 'Opacity', timeRemap: 'Time Remap', positionZ: 'Position Z', rotationX: 'Rotation X', rotationY: 'Rotation Y', dupOffsetPos: 'Dup. Offset', dupOffsetRot: 'Dup. Rotation', dupOffsetScale: 'Dup. Scale', dupOffsetOpacity: 'Dup. Opacity' };
  var PROP_DIM = { position: 2, anchor: 2, rotation: 1, scale: 2, opacity: 1, timeRemap: 1, positionZ: 1, rotationX: 1, rotationY: 1, dupOffsetPos: 2, dupOffsetRot: 1, dupOffsetScale: 2, dupOffsetOpacity: 1 };
  var PROP_UNIT = { position: 'px', anchor: 'px', rotation: '°', scale: '%', opacity: '%', timeRemap: 'f', positionZ: 'px', rotationX: '°', rotationY: '°', dupOffsetPos: 'px', dupOffsetRot: '°', dupOffsetScale: '%', dupOffsetOpacity: '%' };
  var PROP_DEFAULT = { position: [0, 0], anchor: [0, 0], rotation: [0], scale: [100, 100], opacity: [100], timeRemap: [0], positionZ: [0], rotationX: [0], rotationY: [0], dupOffsetPos: [0, 0], dupOffsetRot: [0], dupOffsetScale: [0, 0], dupOffsetOpacity: [0] };
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
  // "Filter Properties" — hides every property row with no keyframes at
  // all (both animated-with-a-track AND a plain non-default static
  // override still count as "has something to show"; only a completely
  // untouched default-value property gets hidden). Global toggle (not
  // per-layer/per-group) — one button affects everything.
  var _hideUnanimated = false;
  function propHasContent(holder, prop) { return isAnimated(holder, prop) || !!(holder.motionStatic && holder.motionStatic[prop]); }
  function isPropFiltered(prop) { return !!_propFilter && _propFilter.indexOf(prop) < 0; }
  // The "TRANSFORM" header earns its row when it groups a list you're
  // scanning. Once a shortcut has narrowed the view to one property it groups
  // nothing — you asked for Position, so show Position (2026-07-27: "quand on
  // utilise les raccourcis pour afficher les propriétés pas la peine
  // d'afficher le transform"). AE does the same: P gives you the one line.
  // ONE predicate for both renderers — the panel builds this header and the
  // grid builds a blank spacer to match it, and a row that exists on one side
  // only is the alignment bug CLAUDE.md §11 opens with.
  function showsGroupHeader() { return !_propFilter; }
  // Which layers a property shortcut acts on: the selection, or EVERY layer
  // when nothing is selected (2026-07-27: "si je fais 'p' alors ça affiche
  // seulement toutes les prop position de tous les calques ou ceux de la
  // sélection") — AE's own rule.
  function shortcutTargets() {
    if (_layerSel.length) return _layerSel.slice();
    return state.layers.map(function (_l, i) { return i; });
  }
  function handlePropShortcut(key, shiftKey) {
    var prop = PROP_SHORTCUT[(key || '').toLowerCase()];
    if (!prop) return false;
    // Filtering to a property is only half of AE's gesture — the layers have
    // to be OPEN for that row to exist at all. Since selecting no longer
    // expands anything, P on a fresh timeline used to filter a set of rows
    // that were all still folded away, i.e. show nothing. Reveal the targets
    // here, and let a second press of the same key fold them back, so the
    // shortcut is a toggle rather than a one-way trip.
    var targets = shortcutTargets();
    var alreadyShowing = !shiftKey && _propFilter && _propFilter.length === 1 && _propFilter[0] === prop &&
      window._motionRevealedLayers && targets.every(function (li) { return window._motionRevealedLayers.indexOf(li) >= 0; });
    if (alreadyShowing) {
      _propFilter = null;
      window._motionRevealedLayers = [];
      window._motionExpandedLayer = null;
      renderLayerList(); renderTimeline();
      return true;
    }
    window._motionRevealedLayers = targets;
    window._motionExpandedLayer = null; // the reveal set replaces the single-row accordion
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
  // E / M — the siblings of U (Van Dijk 5.1: "e = reveal Effect, m = reveal
  // Masks"). Same reveal-set mechanism, only the predicate changes: instead
  // of "layers with animated properties", the layers that actually carry an
  // effects stack, or a matte. Both narrow the LIST rather than filtering
  // properties, because in Nemo effects and mattes are per-layer, not
  // per-property.
  function revealByLayerPredicate(pred, emptyMsg) {
    if (state.appMode !== 'motion') return false;
    var pool = (window._layerSel && window._layerSel.length) ? window._layerSel.slice() : state.layers.map(function (_l, i) { return i; });
    var targets = pool.filter(function (li) { var ld = state.layers[li]; return ld && pred(ld); });
    if (!targets.length) { if (window.showToast) showToast(emptyMsg); return true; }
    window._motionRevealedLayers = targets;
    // Deliberately NOT _hideUnanimated: you want to SEE the properties of
    // the effect-bearing layers, not only the ones that happen to be keyed.
    _hideUnanimated = false;
    renderLayerList(); renderTimeline();
    if (window.showToast) showToast(targets.length + ' calque(s) révélé(s)');
    return true;
  }
  function handleRevealEffectsShortcut() {
    return revealByLayerPredicate(
      function (ld) { return (ld.effects && ld.effects.length) || ld.effectsFrom || ld.isEffectLayer; },
      'Aucun calque ne porte d\u2019effet');
  }
  function handleRevealMattesShortcut() {
    return revealByLayerPredicate(
      function (ld) { return ld.matteMode && ld.matteMode !== 'none'; },
      'Aucun calque n\u2019utilise de matte');
  }

  // ---- easing math: N-point on-curve-waypoint model, deliberate copy of
  // ui.js's shared curve editor (Catmull-Rom tangents -> per-segment cubic
  // Bezier, Newton-with-bisection-fallback solve) — 2026-07, switched from
  // the earlier 2-handle bezier per explicit request to reuse the SAME
  // widget/model the Tween feature already has, just scoped per motion
  // segment instead of one curve applying everywhere. See CLAUDE.md §3 on
  // why this stays a small separate copy rather than a shared import. ----
  // Every motion keyframe's default ease. These are ON-CURVE WAYPOINTS (the
  // model this file and ui.js share — see MOTION_DEFAULT_CURVE's comment
  // there), NOT bezier control handles.
  //
  // That distinction was the bug (2026-07-25). The old value was
  // [{0,0},{.42,0},{.58,1},{1,1}] — the four numbers of CSS
  // `cubic-bezier(.42,0,.58,1)`, i.e. `ease-in-out`, where .42/.58 are
  // CONTROL HANDLES the curve merely leans toward. Fed to evalCurvePoints as
  // waypoints, they became points the curve is forced THROUGH: pinned to 0
  // until 42% of the segment and to 1 from 58% on. Measured on two position
  // keys 12 frames apart, x going 40 -> 120:
  //
  //   legacy       40 40 40 40 40 40 80 120 120 120 120 120 120
  //   linear       40 47 53 60 67 73 80  87  93 100 107 113 120
  //   this curve   40 43 47 52 61 70 80  90  99 108 113 117 120
  //
  // Every default keyframe pair in Motion held still, snapped across in two
  // frames, and held still again — a step, not an ease.
  //
  // Same intent as before (a gentle ease-in-out), expressed correctly for
  // this model: these are the samples of smoothstep 3t^2-2t^3 at quarter
  // points, so the waypoint interpretation reproduces the curve that was
  // meant all along. Legacy keys carrying the exact old array are rewritten
  // by migrateLegacyCurves below.
  var DEFAULT_CURVE = [{ x: 0, y: 0 }, { x: 0.25, y: 0.156 }, { x: 0.5, y: 0.5 }, { x: 0.75, y: 0.844 }, { x: 1, y: 1 }];
  var LEGACY_STEP_CURVE = [{ x: 0, y: 0 }, { x: 0.42, y: 0 }, { x: 0.58, y: 1 }, { x: 1, y: 1 }];
  // A key still carrying the broken default bit-for-bit was never touched by
  // hand, so replacing it restores the intended motion without overwriting
  // any real edit. Anything dragged even slightly fails the equality test and
  // is left exactly as the user tuned it.
  function isLegacyStepCurve(pts) {
    if (!pts || pts.length !== LEGACY_STEP_CURVE.length) return false;
    for (var i = 0; i < pts.length; i++) {
      if (typeof pts[i].tx === 'number') return false; // hand-dragged tangent
      if (Math.abs(pts[i].x - LEGACY_STEP_CURVE[i].x) > 1e-9) return false;
      if (Math.abs(pts[i].y - LEGACY_STEP_CURVE[i].y) > 1e-9) return false;
    }
    return true;
  }
  // ---- keyframe interpolation presets (After Effects parity) ----
  // All three are expressed in the same on-curve-waypoint model as
  // DEFAULT_CURVE, so they round-trip through the existing ease editor and
  // through save/load with no new field. LINEAR is two waypoints (constant
  // velocity); EASE_BOTH is the smoothstep default; the one-sided pair eases
  // only the end it names and stays straight at the other, which is what AE's
  // Easy Ease In / Easy Ease Out do.
  var CURVE_LINEAR = [{ x: 0, y: 0 }, { x: 1, y: 1 }];
  // F9's ease is deliberately STRONGER than DEFAULT_CURVE. AE can make Easy
  // Ease identical to "the ease" because its own default is linear, so F9
  // always changes something. This app's default is already a gentle
  // ease-in-out (the original intent, kept), so an F9 that applied the same
  // shape would be a silent no-op on every fresh keyframe — which is exactly
  // how it first tested. A more pronounced curve keeps F9 meaningful and
  // still reads as "ease this", matching AE's own fairly strong 33% influence.
  var CURVE_EASE = [{ x: 0, y: 0 }, { x: 0.25, y: 0.09 }, { x: 0.5, y: 0.5 }, { x: 0.75, y: 0.91 }, { x: 1, y: 1 }];
  // One-sided: straight through the half it does not name, eased at the half
  // it does. easeIn decelerates ARRIVING at the key, easeOut accelerates
  // leaving it — AE's Easy Ease In / Easy Ease Out.
  var CURVE_EASE_IN = [{ x: 0, y: 0 }, { x: 0.25, y: 0.25 }, { x: 0.5, y: 0.5 }, { x: 0.75, y: 0.91 }, { x: 1, y: 1 }];
  var CURVE_EASE_OUT = [{ x: 0, y: 0 }, { x: 0.25, y: 0.09 }, { x: 0.5, y: 0.5 }, { x: 0.75, y: 0.75 }, { x: 1, y: 1 }];
  // A key's curvePoints govern the segment LEAVING it (valueAtFrame reads the
  // left key's curve), so "ease at this key" means: ease the outgoing segment,
  // and ease the incoming one by touching the previous key's curve. Applying
  // to both is what makes F9 on a middle key feel symmetric, exactly as in AE.
  function prevKeyOf(track, key) {
    var i = track.keys.indexOf(key);
    return i > 0 ? track.keys[i - 1] : null;
  }
  function applyCurveToSelection(kind) {
    var sel = _motionKeySel;
    if (!sel || !sel.length) return 0;
    pushUndo();
    var n = 0;
    sel.forEach(function (s) {
      var track = trackFor(s.holder, s.prop);
      if (!track) return;
      if (kind === 'hold') { s.key.hold = !s.key.hold; n++; return; }
      var outPts = kind === 'linear' ? CURVE_LINEAR : (kind === 'easeIn' ? CURVE_EASE_IN : (kind === 'easeOut' ? CURVE_EASE_OUT : CURVE_EASE));
      // easeIn shapes the segment ARRIVING at this key, so it belongs on the
      // previous key; the others shape the outgoing segment.
      if (kind === 'easeIn') {
        var pk = prevKeyOf(track, s.key);
        if (pk) { pk.curvePoints = cloneCurvePts(CURVE_EASE_IN); n++; }
      } else if (kind === 'easeOut') {
        s.key.curvePoints = cloneCurvePts(CURVE_EASE_OUT); n++;
      } else {
        s.key.curvePoints = cloneCurvePts(outPts);
        var pk2 = prevKeyOf(track, s.key);
        if (pk2) pk2.curvePoints = cloneCurvePts(outPts);
        n++;
      }
    });
    if (n) { renderLayerList(); renderTimeline(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); }
    return n;
  }
  function setKeyInterp(kind) { return applyCurveToSelection(kind); }
  function applyEasyEase(which) { return applyCurveToSelection(which || 'ease'); }
  // Shift every selected keyframe in time. Guards the whole move against
  // collisions and against running off frame 0 BEFORE applying any of it, so a
  // nudge either happens for the whole selection or not at all — a partial
  // shift would silently change the spacing the user was preserving.
  function nudgeSelectedKeys(delta) {
    var sel = _motionKeySel;
    if (!sel || !sel.length || !delta) return 0;
    var groups = {};
    sel.forEach(function (s) {
      var track = trackFor(s.holder, s.prop);
      if (!track) return;
      var id = (s.holder.uid || state.layers.indexOf(s.holder)) + '|' + s.prop;
      (groups[id] || (groups[id] = { track: track, keys: [] })).keys.push(s.key);
    });
    var ok = true;
    Object.keys(groups).forEach(function (id) {
      var g = groups[id];
      g.keys.forEach(function (k) {
        var nf = k.frame + delta;
        if (nf < 0 || nf > state.totalFrames - 1) ok = false;
        var occupant = g.track.keys.find(function (o) { return o.frame === nf; });
        if (occupant && g.keys.indexOf(occupant) < 0) ok = false;
      });
    });
    if (!ok) return 0;
    pushUndo();
    var n = 0;
    Object.keys(groups).forEach(function (id) {
      var g = groups[id];
      g.keys.forEach(function (k) { k.frame += delta; n++; });
      sortKeys(g.track);
    });
    renderLayerList(); renderTimeline();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    return n;
  }
  function deleteSelectedKeys() {
    var sel = _motionKeySel;
    if (!sel || !sel.length) return 0;
    pushUndo();
    var n = 0;
    sel.forEach(function (s) {
      var track = trackFor(s.holder, s.prop);
      if (!track) return;
      var i = track.keys.indexOf(s.key);
      if (i >= 0) { track.keys.splice(i, 1); n++; }
    });
    if (n) { setKeySel([]); renderLayerList(); renderTimeline(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); }
    return n;
  }
  // Copy/paste keeps frames RELATIVE to the earliest key copied, so pasting at
  // the playhead reproduces the selection's own rhythm wherever it lands —
  // AE's behaviour, and the reason this stores offsets rather than frames.
  var _keyClip = null;
  function copySelectedKeys() {
    var sel = _motionKeySel;
    if (!sel || !sel.length) return 0;
    var base = Math.min.apply(null, sel.map(function (s) { return s.key.frame; }));
    _keyClip = sel.map(function (s) {
      return { prop: s.prop, dt: s.key.frame - base, v: s.key.v.slice(), hold: !!s.key.hold, curvePoints: cloneCurvePts(s.key.curvePoints || DEFAULT_CURVE) };
    });
    return _keyClip.length;
  }
  function pasteKeys() {
    if (!_keyClip || !_keyClip.length) return 0;
    var ld = state.layers[state.activeLayerIdx];
    if (!ld) return 0;
    pushUndo();
    var n = 0;
    _keyClip.forEach(function (c) {
      var f = state.currentFrame + c.dt;
      if (f < 0 || f > state.totalFrames - 1) return;
      var track = ensureTrack(ld, c.prop);
      var ex = track.keys.find(function (k) { return k.frame === f; });
      if (ex) { ex.v = c.v.slice(); ex.hold = c.hold; ex.curvePoints = cloneCurvePts(c.curvePoints); }
      else track.keys.push({ frame: f, v: c.v.slice(), hold: c.hold, curvePoints: cloneCurvePts(c.curvePoints), hOut: [0, 0], hIn: [0, 0] });
      sortKeys(track); n++;
    });
    if (n) { renderLayerList(); renderTimeline(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); }
    return n;
  }
  function hasKeySelection() { return !!(_motionKeySel && _motionKeySel.length); }
  function hasKeyClipboard() { return !!(_keyClip && _keyClip.length); }
  function migrateLegacyCurves() {
    var n = 0;
    (state.layers || []).forEach(function (ld) {
      if (!ld || !ld.motion) return;
      Object.keys(ld.motion).forEach(function (prop) {
        var trk = ld.motion[prop];
        if (!trk || !trk.keys) return;
        trk.keys.forEach(function (k) {
          if (isLegacyStepCurve(k.curvePoints)) { k.curvePoints = cloneCurvePts(DEFAULT_CURVE); n++; }
        });
      });
    });
    return n;
  }
  // tx/ty preserved: a point's manual tangent override (draggable Alt-
  // handles in the shared curve editor, ui.js) — stripping them here
  // reset hand-tuned tangents on every key clone.
  function cloneCurvePts(pts) { return pts.map(function (p) { var o = { x: p.x, y: p.y }; if (typeof p.tx === 'number') { o.tx = p.tx; o.ty = p.ty || 0; } return o; }); }
  function curveCubicAt(t, a, b, c, d) { var u = 1 - t; return u * u * u * a + 3 * u * u * t * b + 3 * u * t * t * c + t * t * t * d; }
  function curveCubicDerivAt(t, a, b, c, d) { var u = 1 - t; return 3 * u * u * (b - a) + 6 * u * t * (c - b) + 3 * t * t * (d - c); }
  // Manual tangent override (tx/ty) or derived monotone-limited tangent —
  // duplicated from ui.js's tangentAt (CLAUDE.md §3 pure-math pair, keep
  // in sync — see that copy's comment for the Fritsch–Carlson rationale).
  function curveTangentAt(pts, i) {
    var p = pts[i];
    if (typeof p.tx === 'number') return { x: p.tx, y: p.ty || 0 };
    var prev = pts[i - 1] || p, next = pts[i + 1] || p;
    var tx = (next.x - prev.x) / 2;
    var dx0 = p.x - prev.x, dx1 = next.x - p.x;
    var s0 = dx0 > 1e-9 ? (p.y - prev.y) / dx0 : 0, s1 = dx1 > 1e-9 ? (next.y - p.y) / dx1 : 0;
    var m;
    if (prev === p) m = s1;
    else if (next === p) m = s0;
    else if (s0 * s1 <= 0) m = 0;
    else {
      m = (s0 + s1) / 2;
      var lim = 3 * Math.min(Math.abs(s0), Math.abs(s1));
      if (Math.abs(m) > lim) m = (m > 0 ? 1 : -1) * lim;
    }
    return { x: tx, y: m * tx };
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
    // timeRemap's track is NOT in ld.motion (see trackFor) — creating
    // ld.motion.timeRemap here would make a shadow track no evaluator reads.
    if (prop === 'timeRemap') { if (!ld.timeRemap) ld.timeRemap = { keys: [] }; return ld.timeRemap; }
    if (!ld.motion) ld.motion = {};
    if (!ld.motion[prop]) ld.motion[prop] = { keys: [] };
    return ld.motion[prop];
  }
  function hasKeys(ld, prop) { var t = trackFor(ld, prop); return !!(t && t.keys && t.keys.length); }
  function isAnimated(ld, prop) { return hasKeys(ld, prop); }
  function sortKeys(track) { track.keys.sort(function (a, b) { return a.frame - b.frame; }); }
  function keyAt(track, frame) { return track.keys.find(function (k) { return k.frame === frame; }) || null; }
  function staticValue(ld, prop) {
    var st = ld.motionStatic && ld.motionStatic[prop];
    if (st) return st.slice();
    // Per-vertex Path properties (prop = 'vtx0','vtx1',... — see
    // applyPathVertexOffsets below) aren't in the fixed PROPS list, so they
    // have no PROP_DEFAULT entry; they're offsets on top of the base
    // geometry, so [0,0] (no offset) is the correct neutral default,
    // exactly like PROP_DEFAULT.position would be if it existed there.
    var def = PROP_DEFAULT[prop];
    return def ? def.slice() : [0, 0];
  }

  // The value of `prop` on layer `ld` at `frame` — exact key, interpolated,
  // clamped outside the keyed range, the static override, or the neutral
  // default. Always returns an array (length 1 or 2, per PROP_DIM).
  // Generic single-number track evaluator (2026-07-25, keyable effect
  // parameters). Same key shape as a layer property track — {keys:[{frame, v,
  // curvePoints, hold}]} — evaluated with the SAME curve code and the SAME
  // hold semantics, so an effect's blur radius eases exactly like a Position
  // key and a hold behaves identically. Deliberately shared rather than
  // reimplemented next to the effects panel: a second copy of this maths would
  // drift the first time either side gained a curve type (CLAUDE.md §3's
  // duplicated-pair hazard, applied before it exists).
  function evalTrack(track, frame, fallback) {
    if (!track || !track.keys || !track.keys.length) return fallback;
    var ks = track.keys;
    if (frame <= ks[0].frame) return ks[0].v[0];
    var last = ks[ks.length - 1];
    if (frame >= last.frame) return last.v[0];
    for (var i = 0; i < ks.length - 1; i++) {
      var a = ks[i], b = ks[i + 1];
      if (frame >= a.frame && frame < b.frame) {
        if (a.hold) return a.v[0];
        var t = (frame - a.frame) / (b.frame - a.frame);
        var y = evalCurvePoints(a.curvePoints || DEFAULT_CURVE, t);
        return a.v[0] + (b.v[0] - a.v[0]) * y;
      }
    }
    return last.v[0];
  }
  function rawValueAtFrame(ld, prop, frame) {
    var track = trackFor(ld, prop);
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

  // ---- Expression engine (2026-07) — a modernized take on AE's per-
  // property expressions. Same "opt-in extended mode on the SAME holder"
  // principle as everything else in this file (a holder's .expressions is
  // a THIRD mode next to keyframed/static, not a parallel system) — see
  // this session's audit for the full AE-comparison rationale. Key
  // differences from AE, all deliberate:
  //   - Sandbox: `new Function` with an EXPLICIT, closed parameter list
  //     (time/frame/value/layer/wiggle/loopOut) — no access to window/
  //     document/state, unlike AE's unrestricted ExtendScript.
  //   - Stable references: layer(uid) takes the SAME layerUid this
  //     session's parenting feature already introduced — never a display
  //     name, so renaming a layer can never silently break an expression
  //     (AE's #1 real-world footgun).
  //   - Deterministic wiggle: seeded per-holder-per-property (ensureExprSeed
  //     below), never raw Math.random() — same discipline this codebase's
  //     seededRng (tools.js) already applies to brush-texture dabs, so a
  //     given frame renders identically every time (preview AND export),
  //     unlike AE's wiggle() which can re-seed unpredictably.
  //   - Errors never break the render: a throwing expression falls back to
  //     the underlying keyframed/static value (computed BEFORE the
  //     expression runs, passed in as `value`) and records `lastError` for
  //     the UI to show as a small badge on just that property row — never
  //     a whole-scene failure the way AE's red expression icon can cascade.
  //   - Compiled once per (holder,prop), cached until the code string
  //     changes — not re-parsed every frame.
  function ensureExpr(holder, prop) {
    if (!holder.expressions) holder.expressions = {};
    if (!holder.expressions[prop]) holder.expressions[prop] = { code: '', enabled: false, lastError: null };
    return holder.expressions[prop];
  }
  function hasExpr(holder, prop) { return !!(holder.expressions && holder.expressions[prop] && holder.expressions[prop].enabled && holder.expressions[prop].code); }
  // Stable per-holder random seed for wiggle() — NOT persisted (deliberately
  // absent from serP/serR's field list, see app.js), so it's only stable
  // WITHIN a session; a reload reseeds. A fully save-stable seed would need
  // threading through serP/desP same as strokeId, a reasonable follow-up if
  // "wiggle looks different after reopening the project" is ever reported,
  // but not needed for this MVP (the shape of the motion is what matters,
  // not bit-for-bit identical noise across sessions).
  function ensureExprSeed(holder) {
    if (holder._exprSeed === undefined) holder._exprSeed = Math.floor(Math.random() * 1e9);
    return holder._exprSeed;
  }
  // Tiny deterministic hash noise (not cryptographic, doesn't need to be) —
  // same value for the same (seed, x) every time, smoothly interpolated so
  // wiggle() reads as continuous motion rather than a stepped random walk.
  function hashNoise1D(seed, x) {
    var i = Math.floor(x), f = x - i;
    function h(n) { var v = Math.sin(n * 12.9898 + seed * 78.233) * 43758.5453; return v - Math.floor(v); }
    var a = h(i), b = h(i + 1);
    var t = f * f * (3 - 2 * f); // smoothstep
    return a + (b - a) * t;
  }
  function makeWiggle(seed, dim) {
    // Independent noise per dimension (offsetting the seed) so a 2D
    // property's X/Y wiggle don't move in lockstep along a diagonal line.
    return function (freqPerSec, amp, octaves) {
      var t = (state.currentFrame / (state.fps || 24));
      var n = 0, amp2 = 1, freq2 = 1, norm = 0;
      octaves = Math.max(1, octaves || 1);
      for (var o = 0; o < octaves; o++) {
        n += (hashNoise1D(seed + dim * 101 + o * 977, t * freqPerSec * freq2) - 0.5) * 2 * amp2;
        norm += amp2; amp2 *= 0.5; freq2 *= 2;
      }
      return (n / norm) * amp;
    };
  }
  // loopOut() — cycles `frame` back into this SAME property's own keyed
  // range once playback runs past its last key, AE's loopOut('cycle')
  // equivalent. No-op (returns the un-looped raw value) if the property
  // has fewer than 2 keys — nothing to loop.
  function loopOutRaw(holder, prop, frame) {
    var track = trackFor(holder, prop);
    if (!track || track.keys.length < 2) return rawValueAtFrame(holder, prop, frame);
    var first = track.keys[0].frame, last = track.keys[track.keys.length - 1].frame;
    var span = last - first;
    if (span <= 0 || frame <= last) return rawValueAtFrame(holder, prop, frame);
    var wrapped = first + ((frame - first) % span);
    return rawValueAtFrame(holder, prop, wrapped);
  }
  // Read-only snapshot of another layer's CURRENT effective values, keyed
  // by its stable layerUid (never a display name) — the only inter-item
  // reference an expression can make in this MVP (element-level references
  // are a natural follow-up once this proves out).
  function layerSnapshot(uid, frame) {
    var idx = findLayerIndexByUid(uid);
    if (idx < 0) return null;
    var ld2 = state.layers[idx];
    return {
      position: valueAtFrame(ld2, 'position', frame),
      anchor: valueAtFrame(ld2, 'anchor', frame),
      rotation: valueAtFrame(ld2, 'rotation', frame)[0],
      scale: valueAtFrame(ld2, 'scale', frame),
      opacity: valueAtFrame(ld2, 'opacity', frame)[0],
    };
  }
  // Project-wide expression preamble (Van Dijk 7.2, "set global variables":
  // define something once and use it from every expression instead of
  // retyping it). Plain statements — `var k = 12;` — evaluated in the same
  // scope as the expression body, so anything it declares is in scope.
  // Cached against BOTH the expression's code and the preamble, so editing
  // the preamble recompiles every expression rather than silently leaving
  // stale ones behind.
  function exprGlobals() { return state.exprGlobals || ''; }
  function compiledFnFor(holder, prop) {
    var ex = holder.expressions[prop];
    var pre = exprGlobals();
    if (holder._exprCompiled && holder._exprCompiled[prop] && holder._exprCompiled[prop].code === ex.code
        && holder._exprCompiled[prop].pre === pre) {
      return holder._exprCompiled[prop].fn;
    }
    var fn;
    try {
      // eslint-disable-next-line no-new-func
      fn = new Function('time', 'frame', 'value', 'layer', 'wiggle', 'loopOut',
        '"use strict";\n' + (pre ? pre + '\n' : '') + 'return (\n' + ex.code + '\n);');
      ex.lastError = null;
    } catch (e) {
      fn = null;
      ex.lastError = 'Erreur de syntaxe : ' + e.message + (pre ? ' (variables globales incluses)' : '');
    }
    if (!holder._exprCompiled) holder._exprCompiled = {};
    holder._exprCompiled[prop] = { code: ex.code, pre: pre, fn: fn };
    return fn;
  }
  // Normalizes an expression's return value to the array shape PROP_DIM
  // expects — a 1D property (rotation/opacity) may return a bare number,
  // a 2D one (position/anchor/scale) a bare [x,y] array or, forgivingly, a
  // bare number applied to both dimensions.
  function normalizeExprResult(result, prop) {
    var dim = PROP_DIM[prop];
    if (Array.isArray(result)) {
      if (result.length >= dim) return result.slice(0, dim);
      if (result.length === 1 && dim === 2) return [result[0], result[0]];
      return null;
    }
    if (typeof result === 'number' && !isNaN(result)) return dim === 1 ? [result] : [result, result];
    return null;
  }
  function evalExpressionFor(holder, prop, frame, rawValue) {
    var ex = holder.expressions[prop];
    var fn = compiledFnFor(holder, prop);
    if (!fn) return null;
    var seed = ensureExprSeed(holder);
    try {
      var result = fn(
        frame / (state.fps || 24),
        frame,
        rawValue.length === 1 ? rawValue[0] : rawValue.slice(),
        function (uid) { return layerSnapshot(uid, frame); },
        makeWiggle(seed, prop === 'rotation' || prop === 'opacity' ? 0 : 0),
        function () { return loopOutRaw(holder, prop, frame); }
      );
      var normalized = normalizeExprResult(result, prop);
      if (normalized === null) { ex.lastError = 'L’expression doit retourner un nombre' + (PROP_DIM[prop] === 2 ? ' ou un tableau [x,y]' : '') + '.'; return null; }
      ex.lastError = null;
      return normalized;
    } catch (e) {
      ex.lastError = 'Erreur : ' + e.message;
      return null;
    }
  }
  // Public wrapper — the ONE new branch on top of the pre-existing
  // rawValueAtFrame, checked first so an enabled-with-error expression
  // still falls through to the exact keyframed/static value it would have
  // shown before expressions existed (never a blank/NaN/frozen property).
  function valueAtFrame(ld, prop, frame) {
    var raw = rawValueAtFrame(ld, prop, frame);
    if (hasExpr(ld, prop)) {
      var evaluated = evalExpressionFor(ld, prop, frame, raw);
      if (evaluated !== null) return evaluated;
    }
    return raw;
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
    var track = trackFor(ld, prop);
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
    var track = trackFor(ld, prop);
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
    // The Time Remap row's stopwatch IS the remap switch (AE behavior) —
    // there is no "static timeRemap" fallback to freeze into, the feature
    // is either on (track exists) or off (field deleted, default playback).
    if (prop === 'timeRemap') {
      var tli = state.layers.indexOf(ld);
      if (tli < 0) return;
      ld.timeRemap ? disableTimeRemap(tli) : enableTimeRemap(tli);
      return;
    }
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
  // ---- 3D LAYERS (2026-07-28, After-Effects-style, "chantier sécurisé")
  // ----
  // A 3D-enabled layer's own content renders through the EXACT SAME 2D
  // pipeline every layer already uses (see engine-bridge.js's quad3d
  // branch) — the isolation strategy is to render it normally into its own
  // texture, then composite that texture as a perspective-correct quad.
  // The ONLY new math is here: where do this layer's 4 canvas-sized
  // corners land after a real 3D transform + camera projection. See
  // engine.rs's Quad3DIn doc comment for the clip-space-w trick that makes
  // the GPU do the actual foreshortening from this data — nothing 3D-aware
  // happens on the Rust side, only here.
  //
  // Fixed pinhole camera looking straight down +Z at the canvas center —
  // no explicit 3D Camera layer type yet (this is exactly AE's own
  // behavior when a comp has no Camera layer: a default camera is
  // synthesized). CAMERA_DISTANCE is chosen so z=0 needs ZERO change from
  // today's picture (screen = world exactly at z=0, the critical
  // correctness invariant: toggling threeD on with every 3D value still at
  // its default must reproduce the SAME pixels as before the toggle).
  var CAMERA_DISTANCE = 2000; // world units — same pixel space as canvasW/H
  function degToRad(d) { return d * Math.PI / 180; }
  // Rotates a point around the ORIGIN, X axis then Y then Z (Z = the same
  // rotation the existing 2D 'rotation' property already applies) — callers
  // translate to/from the pivot themselves, same pivot-then-rotate-then-
  // translate shape transformSegments already uses for the 2D-only case.
  function rotate3D(x, y, z, rx, ry, rz) {
    var a, c, s, nx, ny, nz;
    if (rx) { a = degToRad(rx); c = Math.cos(a); s = Math.sin(a); ny = y * c - z * s; nz = y * s + z * c; y = ny; z = nz; }
    if (ry) { a = degToRad(ry); c = Math.cos(a); s = Math.sin(a); nx = x * c + z * s; nz = -x * s + z * c; x = nx; z = nz; }
    if (rz) { a = degToRad(rz); c = Math.cos(a); s = Math.sin(a); nx = x * c - y * s; ny = x * s + y * c; x = nx; y = ny; }
    return [x, y, z];
  }
  // ---- Grease-Pencil-style vertex projection (2026-07-28, revised) ----
  // First cut of this feature rendered a 3D layer's content to an isolated
  // texture and warped the WHOLE TEXTURE as a perspective quad (a new Rust
  // pipeline, since reverted) — technically correct perspective, but it
  // meant stroke WIDTH got warped right along with everything else (thinner
  // on the foreshortened side). Explicit user correction: "comme pour
  // grease pencil, l'épaisseur du trait n'est jamais aplatie, c'est
  // seulement les vecteurs qui sont mis en 3D" — only the PATH GEOMETRY
  // (vertex positions) should follow the 3D transform; stroke thickness
  // stays constant in screen space, exactly like Blender's Grease Pencil
  // (or TVPaint's 3D layer stacking) treats a 2D stroke positioned in 3D
  // space. This is also simpler and needs ZERO Rust changes: a projected
  // vertex is just an ordinary 2D point, so the existing per-item pipeline
  // (gradients, per-element effects, dash patterns, retained paths for
  // non-3D layers, etc.) needs nothing new — only ONE more segment
  // transform step, slotted in exactly where the (now-suppressed)
  // motionMat's own transformSegments call already lived, engine-bridge.js.
  function project3DToScreenPoint(px, py, pz, canvasW, canvasH) {
    var p = project3DToScreen(px, py, pz, canvasW, canvasH);
    return p;
  }
  // One projector function per layer per frame — captures the layer's
  // current position/anchor/rotation(XYZ)/scale ONCE (matching
  // computeMotionMat's own read-once-per-frame contract) and returns a
  // plain (px,py) -> {x,y} closure, reused across every vertex of every
  // item in the layer so they all move together as one rigid (but
  // perspective-projected) plane.
  function make3DProjector(ld, bounds, frameIdx, canvasW, canvasH) {
    var pos = valueAtFrame(ld, 'position', frameIdx);
    var anc = valueAtFrame(ld, 'anchor', frameIdx);
    var rot = valueAtFrame(ld, 'rotation', frameIdx)[0];
    var scl = valueAtFrame(ld, 'scale', frameIdx);
    var posZ = valueAtFrame(ld, 'positionZ', frameIdx)[0];
    var rotX = valueAtFrame(ld, 'rotationX', frameIdx)[0];
    var rotY = valueAtFrame(ld, 'rotationY', frameIdx)[0];
    var sx = scl[0] / 100, sy = scl[1] / 100;
    var pivotX = bounds.x + bounds.width / 2 + anc[0];
    var pivotY = bounds.y + bounds.height / 2 + anc[1];
    return function (px, py) {
      var lx = (px - pivotX) * sx, ly = (py - pivotY) * sy;
      var r = rotate3D(lx, ly, 0, rotX, rotY, rot);
      var wx = pivotX + r[0] + pos[0] - canvasW / 2;
      var wy = pivotY + r[1] + pos[1] - canvasH / 2;
      var wz = r[2] + posZ;
      return project3DToScreenPoint(wx, wy, wz, canvasW, canvasH);
    };
  }
  // Projects every vertex of a stroke's segments through `projector` —
  // handles (bezier control tangents) are RELATIVE vectors, and a true
  // perspective-correct transform of a cubic curve isn't itself a cubic (a
  // known limitation of perspective-warping bezier curves in general) — a
  // nearby-sample-then-subtract approximation is standard practice here
  // (handles are typically much smaller than the shape itself, so the
  // local-linearization error stays visually negligible even under strong
  // rotation — the SAME reasoning applies to any nonlinear path warp, not
  // specific to this feature).
  function project3DSegments(segments, projector) {
    return segments.map(function (s) {
      var p = projector(s.point[0], s.point[1]);
      var out = { point: [p.x, p.y] };
      if (s.handleIn) {
        var hip = projector(s.point[0] + s.handleIn[0], s.point[1] + s.handleIn[1]);
        out.handleIn = [hip.x - p.x, hip.y - p.y];
      }
      if (s.handleOut) {
        var hop = projector(s.point[0] + s.handleOut[0], s.point[1] + s.handleOut[1]);
        out.handleOut = [hop.x - p.x, hop.y - p.y];
      }
      return out;
    });
  }
  function toggleLayer3D(li) {
    var ld = state.layers[li];
    if (!ld) return;
    ld.threeD = !ld.threeD;
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    if (window.renderLayerList) renderLayerList();
  }
  // ---- MOGRAPH DUPLICATOR (2026-07-29) ----
  // Per-layer opt-in grid/radial/path array duplication (AE shape Repeater
  // family). The multiplication itself happens in applyLayerDuplicator /
  // getEffectiveStrokesRendered (app.js) at loadFrame time — NEVER by
  // mutating the stored frames. The layer is force-LOCKED while the
  // duplicator is active: what loadFrame materializes is exactly what
  // saveActiveLayerFrame reads back into ld.frames on the next frame
  // navigation, so an editable multiplied layer would permanently bake N
  // copies into the drawing on the first scrub (CLAUDE.md family-of-bug
  // n°1, destructive variant). Editing the seed shape goes through the
  // panel's dedicated "edit source" toggle (ld._dupEditSource, transient,
  // never persisted), which suspends the multiplication so normal editing
  // and saving apply to the single real shape again.
  function toggleLayerDuplicator(li) {
    var ld = state.layers[li];
    if (!ld) return;
    if (ld.duplicator) {
      ld.duplicator = null;
      ld._dupEditSource = false;
      ld.locked = false;
    } else {
      ld.duplicator = {
        mode: 'grid',
        rows: 2, cols: 3,
        spacingX: 150, spacingY: 150,
        count: 8,
        radius: 200, startAngle: 0, endAngle: null, radialOrient: false,
        pathLayerUid: null, pathAlignTangent: true,
        seed: Math.floor(Math.random() * 1e6),
        staggerRandom: { position: false, rotation: false, scale: false, opacity: false },
      };
      ld.locked = true;
    }
    loadFrame(state.currentFrame);
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    if (window.renderLayerList) renderLayerList();
    if (window.updateDuplicatorPanel) updateDuplicatorPanel();
  }
  function setDuplicatorEditSource(li, on) {
    var ld = state.layers[li];
    if (!ld || !ld.duplicator) return;
    // Leaving edit mode: commit the live (single-seed) layer into stored
    // frame data BEFORE flipping the flag — saveActiveLayerFrame's
    // duplicator guard only lets the save through while _dupEditSource is
    // still true, and the loadFrame below rebuilds from stored data.
    if (!on && ld._dupEditSource && li === state.activeLayerIdx) saveActiveLayerFrame();
    ld._dupEditSource = !!on;
    ld.locked = !on;
    loadFrame(state.currentFrame);
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    if (window.renderLayerList) renderLayerList();
    if (window.updateDuplicatorPanel) updateDuplicatorPanel();
  }
  // ---- 3D GIZMO ("un beau gizmo", 2026-07-28) ----
  // AE's own convention: 3 colored axis arrows (X red/Y green/Z blue) for
  // Position, 3 colored rings for Rotation. Both are projected through the
  // EXACT SAME camera model compute3DCorners uses, then emitted as
  // ordinary 2D path/line overlay primitives via buildOverlayItems below —
  // exactly how the existing anchor-crosshair/scale-rotate-box overlay
  // already works (plain {segments,...} objects fed into the SAME
  // Rust/vello scene-JSON pipeline as real artwork). No Rust changes
  // needed for the gizmo itself — only the LAYER's own content goes
  // through the quad3d path; the gizmo is drawn like any other 2D overlay.
  var GIZMO_AXIS_LEN = 180; // world units — a fixed size, independent of the layer's own bounds (AE's gizmo is a fixed screen size too)
  var GIZMO_RING_RADIUS = 130;
  var GIZMO_COLORS = { x: [235, 70, 70, 255], y: [90, 200, 100, 255], z: [70, 130, 235, 255] };
  function project3DToScreen(wx, wy, wz, canvasW, canvasH) {
    var depthW = (CAMERA_DISTANCE + wz) / CAMERA_DISTANCE;
    if (depthW < 0.05) depthW = 0.05;
    return { x: canvasW / 2 + wx / depthW, y: canvasH / 2 + wy / depthW };
  }
  // One layer's current 3D pose — resolved ONCE per draw/hit-test call so
  // the gizmo's drawn position and its own hit-test always agree exactly
  // (both read the SAME valueAtFrame calls, never two separate paths that
  // could drift, CLAUDE.md §3).
  function gizmo3DPose(t) {
    var holder = t.holder;
    var pos = valueAtFrame(holder, 'position', state.currentFrame);
    var anc = valueAtFrame(holder, 'anchor', state.currentFrame);
    var rot = valueAtFrame(holder, 'rotation', state.currentFrame)[0];
    var posZ = valueAtFrame(holder, 'positionZ', state.currentFrame)[0];
    var rotX = valueAtFrame(holder, 'rotationX', state.currentFrame)[0];
    var rotY = valueAtFrame(holder, 'rotationY', state.currentFrame)[0];
    var pivotX = t.boundsCenter.x + anc[0], pivotY = t.boundsCenter.y + anc[1];
    return {
      pos: pos, rot: rot, posZ: posZ, rotX: rotX, rotY: rotY,
      // Anchor's WORLD 3D position, canvas-center-relative — same
      // convention compute3DCorners' own pivot handling uses.
      originWX: pivotX + pos[0] - state.canvasW / 2,
      originWY: pivotY + pos[1] - state.canvasH / 2,
      originWZ: posZ,
    };
  }
  function gizmo3DOriginScreen(pose) { return project3DToScreen(pose.originWX, pose.originWY, pose.originWZ, state.canvasW, state.canvasH); }
  // Local axes (tilt WITH the layer's current rotation — AE's "Local Axis
  // Mode", more informative than fixed world-aligned arrows once a layer
  // is already rotated).
  function gizmo3DAxisScreenPoints(pose) {
    var dirs = { x: [1, 0, 0], y: [0, 1, 0], z: [0, 0, 1] };
    var origin = gizmo3DOriginScreen(pose);
    var out = {};
    ['x', 'y', 'z'].forEach(function (axis) {
      var d = dirs[axis];
      var r = rotate3D(d[0] * GIZMO_AXIS_LEN, d[1] * GIZMO_AXIS_LEN, d[2] * GIZMO_AXIS_LEN, pose.rotX, pose.rotY, pose.rot);
      out[axis] = { origin: origin, tip: project3DToScreen(pose.originWX + r[0], pose.originWY + r[1], pose.originWZ + r[2], state.canvasW, state.canvasH) };
    });
    return out;
  }
  // Each ring lies in the plane PERPENDICULAR to its own rotation axis
  // (e.g. the X ring, which rotates around X, is drawn in the local Y-Z
  // plane) — sampled as N screen points and connected; a circle in 3D
  // projects to an ellipse-ish 2D shape naturally, no separate math needed.
  function gizmo3DRingScreenPoints(pose) {
    var N = 32;
    var planes = {
      x: function (a) { return [0, Math.cos(a) * GIZMO_RING_RADIUS, Math.sin(a) * GIZMO_RING_RADIUS]; },
      y: function (a) { return [Math.sin(a) * GIZMO_RING_RADIUS, 0, Math.cos(a) * GIZMO_RING_RADIUS]; },
      z: function (a) { return [Math.cos(a) * GIZMO_RING_RADIUS, Math.sin(a) * GIZMO_RING_RADIUS, 0]; },
    };
    var out = {};
    ['x', 'y', 'z'].forEach(function (axis) {
      var pts = [];
      for (var i = 0; i <= N; i++) {
        var a = (i / N) * Math.PI * 2;
        var p = planes[axis](a);
        var r = rotate3D(p[0], p[1], p[2], pose.rotX, pose.rotY, pose.rot);
        pts.push(project3DToScreen(pose.originWX + r[0], pose.originWY + r[1], pose.originWZ + r[2], state.canvasW, state.canvasH));
      }
      out[axis] = pts;
    });
    return out;
  }
  function distToSegment(pt, a, b) {
    var dx = b.x - a.x, dy = b.y - a.y, len2 = dx * dx + dy * dy;
    var tt = len2 ? Math.max(0, Math.min(1, ((pt.x - a.x) * dx + (pt.y - a.y) * dy) / len2)) : 0;
    return Math.hypot(pt.x - (a.x + tt * dx), pt.y - (a.y + tt * dy));
  }
  // Gizmo hit-tests are LAYER-target only (t.strokeId unset) — 3D is a
  // layer-level-only feature in this pass (ld.threeD), matching
  // make3DProjector's own scope. Both return the actual hit DISTANCE
  // (not just axis/pose) — with 6 controls (3 arrows + 3 rings) sharing
  // one small screen region, "arrows always win" turned out wrong in
  // testing (a ring sample point that happens to fall near an arrow's own
  // line stole the grab even when the ring was the closer, more obviously-
  // intended target) — onDown below picks whichever of the two hits is
  // NUMERICALLY CLOSER instead of a fixed type priority.
  function hit3DGizmoAxis(pt, t) {
    if (!t || t.strokeId || !state.layers[t.li] || !state.layers[t.li].threeD) return null;
    var pose = gizmo3DPose(t);
    var axisPts = gizmo3DAxisScreenPoints(pose);
    var tol = 8 / view.zoom;
    var axes = ['x', 'y', 'z'];
    var best = null;
    for (var i = 0; i < axes.length; i++) {
      var seg = axisPts[axes[i]];
      var o = outerWorldPoint(t, seg.origin), tp = outerWorldPoint(t, seg.tip);
      var d = distToSegment(pt, o, tp);
      if (d < tol && (!best || d < best.dist)) best = { axis: axes[i], pose: pose, dist: d };
    }
    return best;
  }
  function hit3DGizmoRing(pt, t) {
    if (!t || t.strokeId || !state.layers[t.li] || !state.layers[t.li].threeD) return null;
    var pose = gizmo3DPose(t);
    var ringPts = gizmo3DRingScreenPoints(pose);
    var tol = 7 / view.zoom;
    var axes = ['x', 'y', 'z'];
    var best = null;
    for (var ai = 0; ai < axes.length; ai++) {
      var pts = ringPts[axes[ai]];
      for (var i = 0; i < pts.length - 1; i++) {
        var a = outerWorldPoint(t, pts[i]), b = outerWorldPoint(t, pts[i + 1]);
        var d = distToSegment(pt, a, b);
        if (d < tol && (!best || d < best.dist)) best = { axis: axes[ai], pose: pose, dist: d };
      }
    }
    return best;
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
  // ---- LAYER PARENTING (2026-07, "parentage de calque comme dans After
  // Effects, changeable en properties d'animation") ----
  // A parent reference is stored by a STABLE uid (ld.parentLayerUid), never
  // a raw array index — state.layers gets spliced on reorder/delete/
  // duplicate (reorderLayer, app.js), which would silently repoint a
  // parent at the WRONG layer (or itself) if indices were stored directly.
  // ensureLayerUid lazily assigns a uid to layers saved before this
  // feature existed, same lazy-assign contract layerElements() already
  // has for strokeId.
  function ensureLayerUid(ld) {
    if (!ld.layerUid) ld.layerUid = 'ly_' + Date.now().toString(36) + '_' + Math.floor(Math.random() * 1e6);
    return ld.layerUid;
  }
  function findLayerIndexByUid(uid) {
    if (!uid) return -1;
    for (var i = 0; i < state.layers.length; i++) if (state.layers[i].layerUid === uid) return i;
    return -1;
  }
  function setLayerParent(li, parentUid) {
    var ld = state.layers[li];
    if (!ld) return;
    if (parentUid) {
      // Refuse to create a cycle (A parents B parents A) — walk the
      // CANDIDATE parent's own chain; if `li` (by uid) appears in it,
      // this assignment would loop. Same visited-set guard as
      // parentChainMats below, applied up front instead of just capping
      // depth at read time, so the UI never silently accepts a cycle.
      var uid = ensureLayerUid(ld);
      var cur = parentUid, visited = {}, guard = 0;
      while (cur && !visited[cur] && guard++ < 256) {
        if (cur === uid) { if (window.showToast) showToast('Parentage refusé : créerait une boucle'); return; }
        visited[cur] = true;
        var idx = findLayerIndexByUid(cur);
        cur = idx >= 0 ? state.layers[idx].parentLayerUid : null;
      }
    }
    ld.parentLayerUid = parentUid || null;
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  // Every ANCESTOR's own layer-level Motion transform, immediate parent
  // first — engine-bridge.js/export.js apply the layer's OWN motionMat,
  // THEN walk this chain applying each ancestor's transform on top, each
  // pivoted around ITS OWN bounds+anchor (exactly how layerMotionAt's own
  // pivot already works, just one level per ancestor) — composes exactly
  // like AE's parent chain: a child inherits its parent's WHOLE effective
  // transform, not just a copy of one property. visited-by-index guards
  // against a cycle that predates setLayerParent's own refusal (e.g. a
  // project file edited by hand, or a parent later deleted and its uid
  // reused by coincidence — extremely unlikely but a hard guard costs
  // nothing here).
  function parentChainMats(li, frameIdx) {
    var mats = [];
    var ld = state.layers[li];
    if (!ld) return mats;
    var visited = {};
    visited[li] = true;
    var curUid = ld.parentLayerUid;
    var guard = 0;
    while (curUid && guard++ < 64) {
      var idx = findLayerIndexByUid(curUid);
      if (idx < 0 || visited[idx]) break;
      visited[idx] = true;
      var m = computeMotionMat(state.layers[idx], frameIdx);
      if (m && userLayers[idx] && userLayers[idx].bounds) {
        mats.push({ mat: m, pivot: { x: userLayers[idx].bounds.center.x + m.ax, y: userLayers[idx].bounds.center.y + m.ay } });
      }
      curUid = state.layers[idx].parentLayerUid;
    }
    return mats;
  }
  function applyParentChainToSegments(segments, li, frameIdx) {
    var chain = parentChainMats(li, frameIdx);
    for (var k = 0; k < chain.length; k++) segments = transformSegments(segments, chain[k].pivot, chain[k].mat);
    return segments;
  }
  // Returns {rect, opacityMul} — opacity composes multiplicatively across
  // the whole chain same as engine-bridge.js already does for elMat/
  // motionMat on one item.
  function applyParentChainToImageRect(rect, li, frameIdx) {
    var chain = parentChainMats(li, frameIdx);
    var opMul = 1;
    for (var k = 0; k < chain.length; k++) { rect = transformImageRect(rect, chain[k].pivot, chain[k].mat); opMul *= chain[k].mat.op; }
    return { rect: rect, opacityMul: opMul };
  }
  // Nested INSIDE the layer transform (engine-bridge.js/export.js apply this
  // FIRST, pivoted around the item's own bounds, THEN the layer transform on
  // top) — matches AE composing a shape group's transform inside its parent
  // layer's transform. Used to unconditionally return null for a Component
  // layer (`ld.symbolId`) — element Motion was inert the instant a layer
  // auto-converted, silently discarding whatever per-shape keys already
  // existed on `ld.elementMotion`. Lifted 2026-07 ("precomp par calque"):
  // getEffectiveStrokes' ld.symbolId branch (app.js) now applies this
  // per-stroke, same nesting order as the plain-layer case above.
  function elementMotionAt(li, strokeId, frameIdx) {
    var ld = state.layers[li];
    if (!ld || !ld.elementMotion) return null;
    return computeMotionMat(ld.elementMotion[strokeId], frameIdx);
  }
  // Extended per-shape property: Fill color (2026-07 — audit gap
  // "propriétés étendues par forme... reste un chantier futur"). First
  // slice of that "chantier": a shape's own `fillColor` becomes just
  // another 'fillColor' track on its element holder, [r,g,b,a] 0-255 (the
  // SAME array shape buildSceneJson's cssColorToRgba already produces —
  // no conversion needed at render time, just an override), reusing every
  // bit of the existing generic keyframe/interpolation machinery (a color
  // track is dimension-agnostic to valueAtFrame/setKeyAtCurrentFrame —
  // confirmed already true for vtxN's 2D offsets, equally true for 4
  // channels). Returns null (never overriding the item's own painted
  // color) unless the user has actually keyed or set a static override —
  // same "opt-in, never automatic" contract as vtxN.
  function elementFillColorAt(li, strokeId, frameIdx) {
    var ld = state.layers[li];
    if (!ld || !ld.elementMotion) return null;
    var holder = ld.elementMotion[strokeId];
    if (!holder) return null;
    if (!hasKeys(holder, 'fillColor') && !(holder.motionStatic && holder.motionStatic.fillColor)) return null;
    return valueAtFrame(holder, 'fillColor', frameIdx);
  }
  // Path property, per-vertex (2026-07, "les properties de path dans motion
  // dont les vertices peuvent être animé séparément"): reuses the EXACT
  // same holder/track machinery as the 5 base Transform properties
  // (valueAtFrame/isAnimated/toggleAnimated/setKeyAtCurrentFrame/
  // trackRowHtml, all already generic over any `prop` string, not hardcoded
  // to PROPS) — a vertex is just a dynamically-named prop ('vtx0','vtx1',…)
  // on the SAME element holder (ld.elementMotion[strokeId]) a shape's own
  // Transform group already lives on, so it's one more sub-track dict,
  // never a parallel system. Each vtxN track holds a 2D [dx,dy] OFFSET
  // added to that vertex's base point — NOT an absolute position — so an
  // un-keyed vertex (the overwhelmingly common case) costs one cheap
  // isAnimated/motionStatic lookup returning [0,0], never touching the
  // segment. See staticValue's [0,0] fallback for props with no
  // PROP_DEFAULT entry (vtxN never has one, by design).
  function hasPathVertexMotion(holder) {
    if (!holder) return false;
    var k;
    if (holder.motion) for (k in holder.motion) if (k.indexOf('vtx') === 0 && holder.motion[k].keys.length) return true;
    if (holder.motionStatic) for (k in holder.motionStatic) if (k.indexOf('vtx') === 0) return true;
    return false;
  }
  // Applied to an item's already-serialized segments (engine-bridge.js's
  // buildSceneJson AND export.js's exportBuildFrame both call this) BEFORE
  // elMat/motionMat/parentChain — per-vertex geometry is the innermost
  // layer of the composition stack, exactly like a shape's own path data
  // is the innermost input to any AE-style transform chain built on top of
  // it. `holder` is the element's own holder (ensureElementHolder result);
  // null/no-vertex-motion is the fast, zero-cost common path.
  function applyPathVertexOffsets(segments, holder, frameIdx) {
    if (!hasPathVertexMotion(holder)) return segments;
    return segments.map(function (s, i) {
      var off = valueAtFrame(holder, 'vtx' + i, frameIdx);
      if (!off[0] && !off[1]) return s;
      return { point: [s.point[0] + off[0], s.point[1] + off[1]], handleIn: s.handleIn, handleOut: s.handleOut };
    });
  }
  // Resolves the element holder from (li, strokeId) itself, same
  // encapsulation elementMotionAt already gives callers — engine-bridge.js/
  // export.js never need to know ld.elementMotion is where this lives.
  function applyPathVertexOffsetsFor(li, strokeId, segments, frameIdx) {
    var ld = state.layers[li];
    if (!ld || !ld.elementMotion || !strokeId) return segments;
    return applyPathVertexOffsets(segments, ld.elementMotion[strokeId], frameIdx);
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
  // Transforms a raster item's on-canvas rect. Full transform since
  // 2026-07 ("une vidéo ou image est un objet comme les autres") — the
  // renderer's image item now carries a `rotation` field (degrees around
  // the rect's own center, engine.rs ImageRef), so this composes rotation
  // too instead of the old scale+translate-only v1: the rect's center
  // ORBITS the pivot by m.rot (same rotPt math transformSegments uses for
  // vector points) and the item's own spin accumulates into .rotation.
  // Input rb may itself carry a rotation (imported image already rotated,
  // or chained element+layer Motion passes) — composed additively, the
  // 2D-rotation group being commutative in angle.
  function transformImageRect(rb, pivot, m) {
    var cx = rb.x + rb.width / 2, cy = rb.y + rb.height / 2;
    var ncx = pivot.x + (cx - pivot.x) * m.sx + m.dx, ncy = pivot.y + (cy - pivot.y) * m.sy + m.dy;
    if (m.rot) {
      // Orbit around the pivot — dx/dy translation applies AFTER the spin,
      // matching transformSegments' own order (scale, rotate, translate).
      var orbited = rotPt(ncx - m.dx, ncy - m.dy, pivot.x, pivot.y, m.rot);
      ncx = orbited[0] + m.dx; ncy = orbited[1] + m.dy;
    }
    var w = rb.width * m.sx, h = rb.height * m.sy;
    return { x: ncx - w / 2, y: ncy - h / 2, width: w, height: h, rotation: (rb.rotation || 0) + (m.rot || 0) };
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
  function applyMotionPoint(p, pivot, m) {
    var x=pivot.x+(p.x-pivot.x)*m.sx,y=pivot.y+(p.y-pivot.y)*m.sy;
    var q=rotPt(x,y,pivot.x,pivot.y,m.rot);
    return{x:q[0]+m.dx,y:q[1]+m.dy};
  }
  function invertMotionPoint(p,pivot,m){
    var q=rotPt(p.x-m.dx,p.y-m.dy,pivot.x,pivot.y,-m.rot);
    return{x:pivot.x+(q[0]-pivot.x)/(m.sx||1),y:pivot.y+(q[1]-pivot.y)/(m.sy||1)};
  }
  // Element overlays start in the element's own world geometry, then must
  // pass through the containing layer and every parent exactly like the
  // rendered path. Keeping the inverse alongside it makes canvas dragging
  // write the element-local key value even when a parent is rotated/scaled.
  function outerMotionMaps(t){
    if(!t||!t.strokeId)return[];
    var out=[],lm=layerMotionAt(t.li,state.currentFrame);
    if(lm&&userLayers[t.li]){
      out.push({mat:lm,pivot:{x:userLayers[t.li].bounds.center.x+lm.ax,y:userLayers[t.li].bounds.center.y+lm.ay}});
    }
    return out.concat(parentChainMats(t.li,state.currentFrame));
  }
  function outerWorldPoint(t,p){
    outerMotionMaps(t).forEach(function(x){p=applyMotionPoint(p,x.pivot,x.mat);});
    return p;
  }
  function outerLocalPoint(t,p){
    var maps=outerMotionMaps(t);
    for(var i=maps.length-1;i>=0;i--)p=invertMotionPoint(p,maps[i].pivot,maps[i].mat);
    return p;
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
    var aw=outerWorldPoint(t,{x:ax,y:ay});
    var ancCol = [80, 220, 140, 255];
    items.push({ segments: [{ point: [aw.x - 9 * zs, aw.y] }, { point: [aw.x + 9 * zs, aw.y] }], closed: false, fillColor: null, strokeColor: ancCol, strokeWidth: 1.5 * zs });
    items.push({ segments: [{ point: [aw.x, aw.y - 9 * zs] }, { point: [aw.x, aw.y + 9 * zs] }], closed: false, fillColor: null, strokeColor: ancCol, strokeWidth: 1.5 * zs });
    items.push({ segments: circleSegs(aw.x, aw.y, 6 * zs), closed: true, fillColor: null, strokeColor: ancCol, strokeWidth: 1.5 * zs });
    // 3D gizmo (2026-07-28) — layer-target only (not per-element), only
    // when this layer has 3D on. Position arrows first, then rotation
    // rings on top (rings are the more precise/deliberate grab, matching
    // the box-handles-before-position-dots priority already established
    // in onDown below).
    if (t.li != null && state.layers[t.li] && state.layers[t.li].threeD && !t.strokeId) {
      var pose3D = gizmo3DPose(t);
      var axisPts3D = gizmo3DAxisScreenPoints(pose3D);
      ['x', 'y', 'z'].forEach(function (axis) {
        var seg = axisPts3D[axis];
        var o = outerWorldPoint(t, seg.origin), tp = outerWorldPoint(t, seg.tip);
        var col = GIZMO_COLORS[axis];
        items.push({ segments: [{ point: [o.x, o.y] }, { point: [tp.x, tp.y] }], closed: false, fillColor: null, strokeColor: col, strokeWidth: 2.2 * zs });
        var dx = tp.x - o.x, dy = tp.y - o.y, len = Math.hypot(dx, dy) || 1;
        var ux = dx / len, uy = dy / len, px = -uy, py = ux, ah = 9 * zs, aw2 = 4 * zs;
        items.push({
          segments: [
            { point: [tp.x, tp.y] },
            { point: [tp.x - ux * ah + px * aw2, tp.y - uy * ah + py * aw2] },
            { point: [tp.x - ux * ah - px * aw2, tp.y - uy * ah - py * aw2] },
          ], closed: true, fillColor: col, strokeColor: null,
        });
      });
      var ringPts3D = gizmo3DRingScreenPoints(pose3D);
      ['x', 'y', 'z'].forEach(function (axis) {
        var col = GIZMO_COLORS[axis];
        var segs = ringPts3D[axis].map(function (p) { var w = outerWorldPoint(t, p); return { point: [w.x, w.y] }; });
        items.push({ segments: segs, closed: true, fillColor: null, strokeColor: [col[0], col[1], col[2], 150], strokeWidth: 1.4 * zs });
      });
    }
    // Scale/rotate transform box (2026-07) — Motion mode previously had NO
    // on-canvas affordance for Scale/Rotation at all, only this anchor
    // crosshair and the position motion-path dots below: scaling/rotating a
    // layer required typing into the Properties panel's numeric fields with
    // no visual handle equivalent, a real inconsistency with Animation 2D's
    // full 8-handle box (bounding-box UX audit, 2026-07). Reuses the exact
    // same corner-square/rotate-stem visual language and blue accent color
    // as engine-bridge.js's buildTransformBoxItems() for cross-mode
    // coherence — same box, drawn via motionHandlePositions()/motionBoxGeom()
    // (which fold in the layer's OWN current position/anchor/rotation/scale,
    // so the box always sits exactly where the object renders THIS frame).
    // 3D gizmo REPLACES this box entirely for a 3D layer (explicit user
    // request) — showing both at once would be two overlapping, partially-
    // redundant control systems (the box's own rotate ring is 2D-only Z
    // rotation, which the 3D gizmo's blue ring already covers).
    var is3DTargetForBox = t.li != null && state.layers[t.li] && state.layers[t.li].threeD && !t.strokeId;
    var mh = is3DTargetForBox ? null : motionHandlePositions(t);
    if (mh) {
      var boxCol = [74, 158, 255, 204];
      var lb = mh.g.bounds;
      var bc1 = mh.g.fwd(lb.left, lb.top), bc2 = mh.g.fwd(lb.right, lb.top), bc3 = mh.g.fwd(lb.right, lb.bottom), bc4 = mh.g.fwd(lb.left, lb.bottom);
      [[bc1, bc2], [bc2, bc3], [bc3, bc4], [bc4, bc1]].forEach(function (seg) {
        items.push({ segments: [{ point: [seg[0].x, seg[0].y] }, { point: [seg[1].x, seg[1].y] }], closed: false, fillColor: null, strokeColor: boxCol, strokeWidth: 1 * zs });
      });
      [mh.corners.nw, mh.corners.ne, mh.corners.se, mh.corners.sw].forEach(function (p) {
        var hs = 3.5 * zs;
        items.push({ segments: [{ point: [p.x - hs, p.y - hs] }, { point: [p.x + hs, p.y - hs] }, { point: [p.x + hs, p.y + hs] }, { point: [p.x - hs, p.y + hs] }], closed: true, fillColor: [255, 255, 255, 255], strokeColor: [74, 158, 255, 255], strokeWidth: 1.2 * zs });
      });
      // Rotate ring (see motionHandlePositions) — full circumference is the
      // grab target, not one small point.
      items.push({ segments: circleSegs(mh.ringCenter.x, mh.ringCenter.y, mh.ringRadius), closed: true, fillColor: null, strokeColor: [74, 158, 255, 160], strokeWidth: 1 * zs });
    }
    // Per-vertex handles (2026-07 — closes the actual gap CLAUDE.md §8
    // flagged: the "Path" accordion (renderPathVertexGroup) already let you
    // ARM a vertex's stopwatch, but nothing ever let you actually MOVE a
    // vertex and have that register as its vtxN track value — arming an
    // untouched vertex just keyed the same [0,0] offset forever, so nothing
    // ever appeared to morph. Only drawn while that element's Path group is
    // expanded (window._motionExpandedPathHolder — same single-accordion
    // state renderPathVertexGroup itself reads), and only for an ELEMENT
    // target (t.strokeId set) — a whole LAYER has no single segments array
    // to offer vertices from.
    if (t.strokeId && window._motionExpandedPathHolder === holder) {
      var vItem = findElementItem(t.li, t.strokeId);
      if (vItem && vItem.segments && mh) {
        var vg = mh.g, vertCol = [190, 130, 240, 255];
        vItem.segments.forEach(function (seg, vi) {
          var voff = valueAtFrame(holder, 'vtx' + vi, state.currentFrame);
          var localX = seg.point.x + (voff[0] || 0), localY = seg.point.y + (voff[1] || 0);
          var wp = vg.fwd(localX, localY);
          var vhs = 4.5 * zs;
          items.push({
            segments: [{ point: [wp.x - vhs, wp.y] }, { point: [wp.x, wp.y - vhs] }, { point: [wp.x + vhs, wp.y] }, { point: [wp.x, wp.y + vhs] }],
            closed: true, fillColor: vertCol, strokeColor: [30, 30, 30, 255], strokeWidth: 1 * zs,
          });
        });
      }
    }
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
        var rawPathPoint={
          x:
            pvx + v * v * v * a.v[0] + 3 * v * v * t * (a.v[0] + ho[0]) + 3 * v * t * t * (b.v[0] + hi[0]) + t * t * t * b.v[0],
          y:pvy + v * v * v * a.v[1] + 3 * v * v * t * (a.v[1] + ho[1]) + 3 * v * t * t * (b.v[1] + hi[1]) + t * t * t * b.v[1]
        };
        var worldPathPoint=outerWorldPoint(t,rawPathPoint);
        pts.push({point:[worldPathPoint.x,worldPathPoint.y]});
      }
      items.push({ segments: pts, closed: false, fillColor: null, strokeColor: pathCol, strokeWidth: 1.5 * zs, dashPattern: [5 * zs, 4 * zs] });
    }
    ks.forEach(function (k, ki) {
      var isCur = k.frame === state.currentFrame;
      var rawKey={x:pvx+k.v[0],y:pvy+k.v[1]},worldKey=outerWorldPoint(t,rawKey);
      var kx=worldKey.x,ky=worldKey.y;
      items.push({ segments: circleSegs(kx, ky, (isCur ? 6 : 4.5) * zs), closed: true, fillColor: isCur ? [255, 170, 40, 255] : [230, 230, 230, 255], strokeColor: [30, 30, 30, 255], strokeWidth: 1.2 * zs });
      // Spatial handles, same discoverable small-dot pattern as camera.js
      var hs = [];
      if (ki < ks.length - 1) hs.push(k.hOut || [0, 0]);
      if (ki > 0) hs.push(k.hIn || [0, 0]);
      hs.forEach(function (h) {
        if (!h[0] && !h[1]) return;
        var worldHandle=outerWorldPoint(t,{x:rawKey.x+h[0],y:rawKey.y+h[1]});
        var hx=worldHandle.x,hy=worldHandle.y;
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
    return outerWorldPoint(t,{ x: t.boundsCenter.x + anc[0], y: t.boundsCenter.y + anc[1] });
  }
  // Geometry-space bounds + the target's own current position/anchor/
  // rotation/scale, folded into a single fwd(x,y) mapper — same formula as
  // layerMotionPointMap's `fwd` (and computeMotionMat's semantics: Rotation/
  // Scale pivot around bounds-center+anchor, Position is a plain translation
  // added on top) but usable even when the target has no motion tracks at
  // all yet (valueAtFrame/staticValue already return the neutral defaults in
  // that case) — layerMotionPointMap itself returns null until at least one
  // property is non-default, which would make the box invisible on a
  // perfectly ordinary not-yet-animated layer.
  // A Component instance's Motion box must stay the SAME size/position for
  // its whole duration (2026-07-17, "le bounding box du layer component
  // doit être le même pour toute la durée du calque... prendre en compte
  // tous les éléments dans la durée pour avoir les bounding box max") —
  // without this, userLayers[li].bounds (below) only ever reflects
  // whatever ONE frame's content getEffectiveStrokes/loadFrame happened to
  // load, so the box visibly jumped/resized every time the scrub crossed a
  // keyframe with different shapes. Unions every frame's effective strokes
  // across the layer's own visible range (layerInPoint..layerOutPoint —
  // the SAME range the layer is ever shown on, app.js), building bounds
  // per stroke the same way getEffectiveStrokes' own elementMotionAt
  // branch already does (a throwaway Path so curved segments' real extent
  // counts, not just anchor points), reduced with unionBounds (tweens.js,
  // a plain generic min/max reducer already used for per-frame feature
  // bounds — reused here across frames instead).
  //
  // This originally shipped with no cache, on the reasoning that it was
  // "Motion-overlay code for whichever ONE layer is currently selected, not
  // the hot per-frame render pipeline". That reasoning was wrong: the Motion
  // overlay IS rebuilt by every render, and a drag renders once per
  // pointermove — so "one selected layer" turned out to mean "every frame of
  // the drag you are trying to do". Hence the cache below.
  //
  // The union is a FIXED geometric reference: it deliberately does not depend
  // on the current frame (that is the whole point — the gizmo's pivot must
  // not jump as the scrub crosses keyframes). Computing it walks every frame
  // of the instance and rebuilds a temporary Paper Path per stroke just to
  // read its bounds — on a real 120-frame component that is ~1.8M Segment/
  // Point allocations. buildOverlayItems calls it TWICE per render (once via
  // activeMotionTarget, once via motionBoxGeom), and a Motion drag renders
  // once per pointermove: measured 326ms + 316ms of a 642ms overlay build,
  // i.e. the entire reason dragging a component in Motion was unusable while
  // the same drag in Animation 2D — where a plain layer uses Paper's own
  // cached bounds — stayed fluid (2026-07-28).
  //
  // Cached per (symbol, in, out). Invalidated only when frame content is
  // written (saveActiveLayerFrame / saveAllLayerFrames), which happens at
  // gesture ends, never per pointermove — so a drag holds the cache for its
  // whole duration, which is exactly the case being fixed.
  var _symUnionCache = new Map();
  function invalidateSymbolUnionBounds() { _symUnionCache.clear(); }
  function symbolUnionBounds(li) {
    var ld = state.layers[li];
    if (!ld || !ld.symbolId) return null;
    var inF = layerInPoint(ld), outF = layerOutPoint(ld);
    var ck = ld.symbolId + '|' + inF + '|' + outF;
    var hit = _symUnionCache.get(ck);
    if (hit !== undefined) return hit;
    var feats = [];
    for (var f = inF; f <= outF; f++) {
      getEffectiveStrokes(li, f).forEach(function (sd) {
        if (sd.isRaster) { feats.push({ bounds: { x: sd.x - sd.width / 2, y: sd.y - sd.height / 2, w: sd.width, h: sd.height } }); return; }
        if (!sd.segments || !sd.segments.length) return;
        var tmp = new Path({ insert: false });
        for (var si = 0; si < sd.segments.length; si++) { var s = sd.segments[si]; tmp.add(new Segment(new Point(s.point[0], s.point[1]), new Point(s.handleIn[0], s.handleIn[1]), new Point(s.handleOut[0], s.handleOut[1]))); }
        if (sd.closed) tmp.closed = true;
        var b = tmp.bounds;
        feats.push({ bounds: { x: b.x, y: b.y, w: b.width, h: b.height } });
      });
    }
    if (!feats.length) { _symUnionCache.set(ck, null); return null; }
    var u = unionBounds(feats);
    var rect = new Rectangle(u.x, u.y, u.w, u.h);
    _symUnionCache.set(ck, rect);
    return rect;
  }
  function motionBoxGeom(t) {
    var ld = state.layers[t.li];
    var lb = (ld && ld.symbolId) ? symbolUnionBounds(t.li) : (userLayers[t.li] && userLayers[t.li].bounds);
    if (!lb) return null;
    var anc = valueAtFrame(t.holder, 'anchor', state.currentFrame);
    var pos = valueAtFrame(t.holder, 'position', state.currentFrame);
    var rot = valueAtFrame(t.holder, 'rotation', state.currentFrame)[0];
    var scl = valueAtFrame(t.holder, 'scale', state.currentFrame);
    var px = t.boundsCenter.x + anc[0], py = t.boundsCenter.y + anc[1];
    var r = rot * Math.PI / 180, c = Math.cos(r), s = Math.sin(r);
    var sx = scl[0] / 100, sy = scl[1] / 100;
    function fwd(x, y) {
      var lx = (x - px) * sx, ly = (y - py) * sy;
      return { x: px + lx * c - ly * s + pos[0], y: py + lx * s + ly * c + pos[1] };
    }
    // Inverse of fwd — world point back to the shape's own local space.
    // [c -s; s c] is a pure rotation matrix, so its inverse is just its
    // transpose ([c s; -s c]); added for vertex dragging (buildOverlayItems'
    // vertex dots / onDown/onDrag's 'vertex' mode below), which needs to go
    // world->local to recover the vertex's LOCAL offset from wherever the
    // mouse currently is in world space.
    function inv(wx, wy) {
      var ux = wx - pos[0] - px, uy = wy - pos[1] - py;
      var lx = ux * c + uy * s, ly = -ux * s + uy * c;
      return { x: lx / sx + px, y: ly / sy + py };
    }
    return { bounds: lb, pivot: fwd(px, py), rot: rot, scl: scl, fwd: fwd, inv: inv };
  }
  // World-space positions of the scale-box corners + the rotate handle
  // (offset outward from the box's own top-center along whatever direction
  // that edge currently renders at, so it swings around with rotation
  // instead of always hovering "above" in screen space) — shared by the
  // overlay drawer and the hit-tester so they can never silently disagree.
  function motionHandlePositions(t) {
    var g = motionBoxGeom(t);
    if (!g) return null;
    var b = g.bounds;
    var corners = { nw: g.fwd(b.left, b.top), ne: g.fwd(b.right, b.top), se: g.fwd(b.right, b.bottom), sw: g.fwd(b.left, b.bottom) };
    var zs = 1 / Math.max(0.0001, view.zoom);
    // Rotate RING (2026-07, replacing the tiny offset stem+dot — same
    // change/formula as select-bridge.js's computeHandles and
    // engine-bridge.js's buildTransformBoxItems, mirrored here for visual/
    // interaction coherence between Animation 2D and Motion). Centered on
    // the pivot. Small and mostly size-independent (per user mockup),
    // capped relative to the box's scaled larger half-dimension (scaled by
    // the layer's own current Scale, since rotation alone doesn't change
    // size) only so it shrinks gracefully on a genuinely small selection.
    var ringRadius = Math.min(36 * zs, Math.max(b.width * (g.scl[0] / 100), b.height * (g.scl[1] / 100)) * 0.3);
    return { g: g, corners: corners, ringCenter: g.pivot, ringRadius: ringRadius };
  }
  // Ring band test first (anywhere within ~7px of the circumference, not
  // one fixed point), then nearest-wins over the corners — same convention
  // as select-bridge.js's hitTestHandles. Kept as a scoped v1 to UNIFORM
  // corner-scale only (no per-axis edge handles yet), the highest-value
  // slice of the gap since it's what was completely missing before.
  function hitMotionBoxHandle(pt, t) {
    var h = motionHandlePositions(t);
    if (!h) return null;
    var zs = 1 / Math.max(0.0001, view.zoom);
    var ringTol = 7 * zs;
    if (Math.abs(Math.hypot(pt.x - h.ringCenter.x, pt.y - h.ringCenter.y) - h.ringRadius) < ringTol) return { type: 'rotate' };
    var tol = 9 * zs, bestD = tol, best = null;
    Object.keys(h.corners).forEach(function (k) {
      var d = Math.hypot(pt.x - h.corners[k].x, pt.y - h.corners[k].y);
      if (d < bestD) { bestD = d; best = { type: 'scale', dir: k }; }
    });
    return best;
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
  function hitPositionHandle(pt, ks, pv, target) {
    var tol = 10 / view.zoom;
    for (var i = 0; i < ks.length; i++) {
      var k = ks[i], hs = [];
      if (i < ks.length - 1) hs.push(['hOut', k.hOut || [0, 0]]);
      if (i > 0) hs.push(['hIn', k.hIn || [0, 0]]);
      for (var j = 0; j < hs.length; j++) {
        var wh=outerWorldPoint(target,{x:pv.x+k.v[0]+hs[j][1][0],y:pv.y+k.v[1]+hs[j][1][1]});
        var hx=wh.x,hy=wh.y;
        if (Math.hypot(pt.x - hx, pt.y - hy) < tol) return { key: k, which: hs[j][0] };
      }
    }
    return null;
  }
  function hitPositionDot(pt, ks, pv, target) {
    var tol = 8 / view.zoom;
    for (var i = 0; i < ks.length; i++){var wp=outerWorldPoint(target,{x:pv.x+ks[i].v[0],y:pv.y+ks[i].v[1]});if(Math.hypot(pt.x-wp.x,pt.y-wp.y)<tol)return ks[i];}
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
    // comment) — per-ELEMENT sub-targeting on a Component layer now works
    // too (renderElementsList lifted its symbolId guard 2026-07-17, so a
    // shape inside a placed instance can be animated straight from the
    // Scene view). Its pivot still comes from the LIVE item's own current
    // bounds (item.bounds.center below) — only the whole-LAYER case
    // (return at the bottom) needed the fixed-across-duration union, since
    // a single element's own gizmo is expected to hug whatever it looks
    // like THIS frame, same as any element's box always has.
    if (window._motionExpandedLayer != null && window._motionExpandedElement != null) {
      var item = findElementItem(li, window._motionExpandedElement);
      if (item) return { li: li, strokeId: window._motionExpandedElement, holder: ensureElementHolder(ld, window._motionExpandedElement), boundsCenter: item.bounds.center };
      // Element no longer present at this frame (drawing changed) — fall
      // back to the layer rather than silently drawing nothing.
    }
    // Component layers use symbolUnionBounds' fixed-for-the-whole-duration
    // center here too, so the gizmo's PIVOT doesn't jump around alongside
    // its box (motionBoxGeom, above) as the scrub crosses different
    // keyframes.
    var ub = ld.symbolId ? symbolUnionBounds(li) : null;
    return { li: li, strokeId: null, holder: ld, boundsCenter: (ub || userLayers[li].bounds).center };
  }
  function activePositionKeys() {
    var t = activeMotionTarget();
    if (!t || !hasKeys(t.holder, 'position')) return null;
    return t.holder.motion.position.keys;
  }
  function hitAnchorPoint(pt, t) {
    var tol = 9 / view.zoom;
    var anc = valueAtFrame(t.holder, 'anchor', state.currentFrame);
    var aw=outerWorldPoint(t,{x:t.boundsCenter.x+anc[0],y:t.boundsCenter.y+anc[1]});
    var ax=aw.x,ay=aw.y;
    return Math.hypot(pt.x - ax, pt.y - ay) < tol ? { holder: t.holder, bc: t.boundsCenter, target:t } : null;
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
    // Vertex handles (2026-07) checked FIRST, before the box/position/anchor
    // hit-tests below — once the Path group is expanded the user's whole
    // focus is on a specific vertex, so a vertex dot must win any incidental
    // overlap with the (usually much larger) scale/rotate box.
    if (t && t.strokeId && window._motionExpandedPathHolder === t.holder) {
      var vItem2 = findElementItem(t.li, t.strokeId);
      var vg2 = motionBoxGeom(t);
      if (vItem2 && vItem2.segments && vg2) {
        var vTol = 9 / view.zoom;
        for (var vi2 = 0; vi2 < vItem2.segments.length; vi2++) {
          var seg2 = vItem2.segments[vi2];
          var voff2 = valueAtFrame(t.holder, 'vtx' + vi2, state.currentFrame);
          var wp2 = vg2.fwd(seg2.point.x + (voff2[0] || 0), seg2.point.y + (voff2[1] || 0));
          if (Math.hypot(event.point.x - wp2.x, event.point.y - wp2.y) < vTol) {
            pushUndo();
            _motionDrag = { mode: 'vertex', t: t, vi: vi2, basePt: { x: seg2.point.x, y: seg2.point.y } };
            return true;
          }
        }
      }
    }
    // Position keys/handles checked BEFORE the anchor point (2026-07-17
    // motion-path-at-anchor fix made this ordering matter): a key at its
    // default [0,0] delta now draws its dot exactly ON the anchor
    // crosshair (motionPivotOf) — dragging keyframes is the far more
    // common gesture, so it wins the overlap; the anchor stays reachable
    // once a key has been moved away from it (the ordinary case) or by
    // starting the drag from a few px off-center.
    if (t) {
      // 3D gizmo (2026-07-28) checked BEFORE the 2D scale/rotate box below —
      // when a layer has 3D on, its own axis arrows/rotation rings are the
      // deliberate, precise controls for it, same "precise grab wins"
      // priority the box-handles-before-position-dots ordering already
      // established for the 2D case. Between the two 3D control types
      // (arrows vs rings), whichever is NUMERICALLY CLOSER to the click
      // wins — found by testing that a fixed "arrows always win" priority
      // let an arrow's line steal a click clearly aimed at a nearby ring
      // sample point.
      var axisHit3D = hit3DGizmoAxis(event.point, t);
      var ringHit3D = hit3DGizmoRing(event.point, t);
      if (axisHit3D && (!ringHit3D || axisHit3D.dist <= ringHit3D.dist)) {
        pushUndo();
        var axisPts3D_ = gizmo3DAxisScreenPoints(axisHit3D.pose);
        var o3d = outerWorldPoint(t, axisPts3D_[axisHit3D.axis].origin), tp3d = outerWorldPoint(t, axisPts3D_[axisHit3D.axis].tip);
        var dx3d = tp3d.x - o3d.x, dy3d = tp3d.y - o3d.y, dl3d = Math.hypot(dx3d, dy3d) || 1;
        _motionDrag = {
          mode: 'axis3d', t: t, axis: axisHit3D.axis,
          dirX: dx3d / dl3d, dirY: dy3d / dl3d,
          startPt: { x: event.point.x, y: event.point.y },
          baseline: axisHit3D.axis === 'z' ? axisHit3D.pose.posZ : axisHit3D.pose.pos[axisHit3D.axis === 'x' ? 0 : 1],
        };
        return true;
      }
      if (ringHit3D) {
        pushUndo();
        var center3d = outerWorldPoint(t, gizmo3DOriginScreen(ringHit3D.pose));
        var startAngle3D = Math.atan2(event.point.y - center3d.y, event.point.x - center3d.x) * 180 / Math.PI;
        _motionDrag = {
          mode: 'ring3d', t: t, axis: ringHit3D.axis, center: center3d, startAngle: startAngle3D,
          baseline: ringHit3D.axis === 'x' ? ringHit3D.pose.rotX : (ringHit3D.axis === 'y' ? ringHit3D.pose.rotY : ringHit3D.pose.rot),
        };
        return true;
      }
      // Scale/rotate box handles checked FIRST — same priority order as
      // Animation 2D's own hitTestHandles (select-bridge.js): a corner/
      // rotate grab is a deliberate, precise action, so it should win any
      // rare overlap with a position dot/anchor rather than the reverse.
      // Skipped entirely for a 3D layer — the box isn't drawn there (see
      // buildOverlayItems' is3DTargetForBox), so it must not still be a
      // live (invisible) hit-target either.
      var boxHit = (t.li != null && state.layers[t.li] && state.layers[t.li].threeD && !t.strokeId) ? null : hitMotionBoxHandle(event.point, t);
      if (boxHit) {
        pushUndo();
        var g = motionBoxGeom(t);
        if (boxHit.type === 'rotate') {
          var startAngle = Math.atan2(event.point.y - g.pivot.y, event.point.x - g.pivot.x) * 180 / Math.PI;
          _motionDrag = { mode: 'motionRotate', t: t, pivot: g.pivot, startAngle: startAngle, origRot: g.rot };
        } else {
          var corner = motionHandlePositions(t).corners[boxHit.dir];
          var origDist = Math.hypot(corner.x - g.pivot.x, corner.y - g.pivot.y) || 1;
          _motionDrag = { mode: 'motionScale', t: t, pivot: g.pivot, origDist: origDist, origScale: g.scl.slice() };
        }
        return true;
      }
      var ks = activePositionKeys();
      if (ks) {
        var anc2=valueAtFrame(t.holder,'anchor',state.currentFrame);
        var pv={x:t.boundsCenter.x+anc2[0],y:t.boundsCenter.y+anc2[1]};
        var hp = hitPositionHandle(event.point, ks, pv,t);
        if (hp) { pushUndo(); _motionDrag = { mode: 'handle', key: hp.key, which: hp.which, pv: pv,t:t }; return true; }
        var pk = hitPositionDot(event.point, ks, pv,t);
        if (pk) { pushUndo(); _motionDrag = { mode: 'point', key: pk, pv: pv,t:t }; return true; }
      }
      var ap = hitAnchorPoint(event.point, t);
      if (ap) { pushUndo(); _motionDrag = { mode: 'anchor', holder: ap.holder, bc: ap.bc, t:ap.target }; return true; }
    }
    return false;
  }
  function onDrag(event) {
    if (!_motionDrag) return false;
    if (_motionDrag.mode === 'vertex') {
      // World -> local via the SAME position/rotation/scale pipeline the
      // vertex dot itself was drawn through (motionBoxGeom's fwd/inv are
      // exact inverses) — recomputed fresh each tick since the shape's own
      // position/rotation/scale could themselves be scrubbing concurrently
      // (dragging a vertex while the playhead moves, or a running preview).
      var vg = motionBoxGeom(_motionDrag.t);
      if (vg) {
        var local = vg.inv(event.point.x, event.point.y);
        var dx = local.x - _motionDrag.basePt.x, dy = local.y - _motionDrag.basePt.y;
        setValue(_motionDrag.t.holder, 'vtx' + _motionDrag.vi, [dx, dy]);
      }
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
      return true;
    }
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
      var localAnchor=outerLocalPoint(_motionDrag.t,{x:event.point.x,y:event.point.y});
      setValue(_motionDrag.holder, 'anchor', [localAnchor.x - _motionDrag.bc.x, localAnchor.y - _motionDrag.bc.y]);
    } else if (_motionDrag.mode === 'motionRotate') {
      // Recomputed from the FIXED drag-start baseline every tick (not
      // incrementally accumulated) — setValue always writes an absolute
      // value, so there's no risk of compounding drift the way raw Paper.js
      // geometry mutation would need to guard against.
      var ang = Math.atan2(event.point.y - _motionDrag.pivot.y, event.point.x - _motionDrag.pivot.x) * 180 / Math.PI;
      setValue(_motionDrag.t.holder, 'rotation', [_motionDrag.origRot + (ang - _motionDrag.startAngle)]);
    } else if (_motionDrag.mode === 'axis3d') {
      // Drag projected onto the arrow's OWN screen-space direction (a
      // standard simplification for lightweight 3D gizmos — full ray/plane
      // unprojection isn't needed to get a natural-feeling drag along one
      // axis) — recomputed from the FIXED drag-start baseline every tick,
      // same "absolute, not accumulated" convention as motionRotate above.
      var ddx = event.point.x - _motionDrag.startPt.x, ddy = event.point.y - _motionDrag.startPt.y;
      var along = ddx * _motionDrag.dirX + ddy * _motionDrag.dirY;
      var newVal3D = _motionDrag.baseline + along;
      if (_motionDrag.axis === 'z') setValue(_motionDrag.t.holder, 'positionZ', [newVal3D]);
      else {
        var curPos3D = valueAtFrame(_motionDrag.t.holder, 'position', state.currentFrame);
        setValue(_motionDrag.t.holder, 'position', [_motionDrag.axis === 'x' ? newVal3D : curPos3D[0], _motionDrag.axis === 'y' ? newVal3D : curPos3D[1]]);
      }
    } else if (_motionDrag.mode === 'ring3d') {
      var ang3D = Math.atan2(event.point.y - _motionDrag.center.y, event.point.x - _motionDrag.center.x) * 180 / Math.PI;
      var newRot3D = _motionDrag.baseline + (ang3D - _motionDrag.startAngle);
      if (_motionDrag.axis === 'x') setValue(_motionDrag.t.holder, 'rotationX', [newRot3D]);
      else if (_motionDrag.axis === 'y') setValue(_motionDrag.t.holder, 'rotationY', [newRot3D]);
      else setValue(_motionDrag.t.holder, 'rotation', [newRot3D]);
    } else if (_motionDrag.mode === 'motionScale') {
      // v1 scope: uniform scale only (both axes move by the same ratio) —
      // no per-edge single-axis handles yet, matching this increment's
      // corner-only hit-test.
      var dist = Math.hypot(event.point.x - _motionDrag.pivot.x, event.point.y - _motionDrag.pivot.y);
      var ratio = dist / _motionDrag.origDist;
      setValue(_motionDrag.t.holder, 'scale', [_motionDrag.origScale[0] * ratio, _motionDrag.origScale[1] * ratio]);
    } else {
      var k = _motionDrag.key, pv = _motionDrag.pv;
      var localPointer=outerLocalPoint(_motionDrag.t,{x:event.point.x,y:event.point.y});
      // Both branches now resolve against the SAME pivot buildOverlayItems
      // draws from (motionPivotOf — bounds center + anchor offset), so a
      // key.v of [0,0] drags from exactly where its dot is drawn, matching
      // computeMotionMat's delta semantics instead of raw world coords.
      if (_motionDrag.mode === 'handle') k[_motionDrag.which] = [localPointer.x - (pv.x + k.v[0]), localPointer.y - (pv.y + k.v[1])];
      else { k.v[0] = localPointer.x - pv.x; k.v[1] = localPointer.y - pv.y; }
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
  // (StoryBoard/Animation2D/Motion architecture diagram, CLAUDE.md §8),
  // reversed the same day in favor of splitLayerIntoElements ("Release to
  // Layers"), then RE-reversed 2026-07-17 ("montage des éléments dans le
  // component") once a Component layer could actually carry working
  // per-element Motion (elementMotionAt no longer forces null for
  // ld.symbolId, see app.js's getEffectiveStrokes). Two rendering attempts
  // followed: first a lightweight parallel "focus" mechanism (own state +
  // own tab, dropped the same day for enterSymbol reuse — "un component et
  // une precomp c'est la même chose"), then a nested "Transform (instance)
  // > Éléments > Forme N (own Transform)" montage view — ALSO dropped
  // ("j'ai pas... plusieurs calques séparés... comme avant", pointing at a
  // screenshot of splitLayerIntoElements' own flat "Layer 1 — Forme N"
  // result). Landed on: double-click just calls enterSymbol (real tab,
  // real "Scene" back-button, zero new state), then SILENTLY auto-runs
  // splitLayerIntoElementsCore on every qualifying layer inside the
  // entered symbol — turning the single merged "Layer 1" into N real
  // separate layers, each with its own real timeline bar. From there the
  // NORMAL Motion layer list/timeline (unmodified) already renders exactly
  // the wanted result; no montage-specific rendering code needed at all.
  // Only gated on `ld.symbolId`: a plain layer keeps the old
  // Release-to-Layers split, since there's no "inside" yet.
  function enterComponentLayer(li) {
    var ld = state.layers[li]; if (!ld || !ld.symbolId) return;
    var sym = state.symbols[ld.symbolId];
    // enterSymbol (app.js) always resets currentFrame to 0 — fine for its
    // other caller (Animation2D's own dblclick-to-enter-symbol), but here
    // it silently hid every shape whenever the symbol's OWN frame 0
    // happens to be blank (bug found live, "je ne vois plus le montage...
    // comme avant" — a component that starts drawing partway through its
    // timeline is a completely normal case, not an edge case). Resolve
    // which inner frame the instance was ALREADY showing at the outer
    // playhead (same resolveSymbolFrameIdx mapping getEffectiveStrokes
    // uses to render it) and jump there right after entering.
    var targetFrame = sym ? resolveSymbolFrameIdx(sym, ld, state.currentFrame) : 0;
    if (window.SM && window.SM.enterSymbol) window.SM.enterSymbol(ld.symbolId);
    if (sym) goToFrame(Math.max(0, Math.min(sym.totalFrames - 1, targetFrame)));
    // Auto-split every entered layer that still bundles 2+ shapes — highest
    // index first so each splice (replacing 1 layer with N) never shifts
    // an index this loop hasn't visited yet.
    if (window.SM && window.SM.splitLayerIntoElementsCore) {
      // Record ONE undo entry covering the whole auto-split before touching
      // anything (2026-07-25, "impossible de revenir qu'à un seul calque
      // après"): each core call runs silent, and `silent` also skips
      // pushUndo — so entering a component to LOOK at it used to rewrite
      // its layer structure permanently, with no undo step to walk back
      // and no merge command to do it by hand. Pre-counted rather than
      // pushed unconditionally so browsing an already-split component (the
      // idempotent re-entry case) doesn't spam the undo stack with no-ops.
      var willSplit = 0;
      for (var q = 0; q < state.layers.length; q++) {
        var qd = state.layers[q];
        if (!qd || qd.symbolId) continue;
        var qe = layerElements(q, qd);
        if (qe && qe.length >= 2) willSplit++;
      }
      if (willSplit) pushUndo();
      for (var i = state.layers.length - 1; i >= 0; i--) window.SM.splitLayerIntoElementsCore(i, { silent: true });
      // The way back is not obvious from the result (N rows where there was
      // one) — say it once, with the exact gesture.
      if (willSplit) showToast('Éclaté en calques — clic droit › « Fusionner les calques sélectionnés » pour revenir en arrière');
    }
  }
  function renderLayerListMotion(list) {
    if (!list._motionEmptySelectBound) {
      list._motionEmptySelectBound = true;
      list.addEventListener('pointerdown', function (e) {
        if (e.target !== list || state.appMode !== 'motion') return;
        _layerSel = [];
        setKeySel([]);
        renderLayerList(); renderTimeline();
      });
    }
    // Inside a Component (state.activeSymbolId, entered via
    // enterComponentLayer's dblclick below OR Animation2D's own
    // dblclick-to-enter-symbol): state.layers IS the symbol's own layers
    // now — enterComponentLayer already auto-split any multi-shape layer
    // into N real separate ones, so the normal per-layer rendering below
    // (unmodified) already shows exactly the wanted result, no special
    // case needed here.
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
      // Motion paints the SELECTION and nothing else — no fallback
      // highlight on the merely-active layer (2026-07-27: "ce n'est pas
      // possible de deselect tout les calque, y en a toujours 1 de
      // select"). activeLayerIdx is always a valid index by design (the
      // drawing tools need a target), so lighting it up made an empty
      // selection indistinguishable from a one-layer selection and put a
      // floor of one under every deselect. AE shows nothing selected when
      // nothing is; Animation 2D keeps its own .act cue, where it means
      // "this is where the brush draws" and is genuinely useful.
      row.className = 'lrow' + (_layerSel.indexOf(li) >= 0 ? ' act motion-selected' : '');
      row.dataset.layer = li;
      if (isComponent) row.title = 'Composant — Position/Anchor/Rotation/Scale/Opacity animent l\'instance entière (le contenu interne s\'édite via "Éditer le composant…")';
      var arrow = document.createElement('div'); arrow.className = 'lico larrow'; arrow.textContent = expanded ? '▾' : '▸';
      // The twirl-down is now the ONLY way to open a layer's properties, so
      // it needs its own handler — it used to be decoration on a row whose
      // click did the expanding. stopPropagation keeps opening a layer from
      // also changing what's selected, the way AE's twirl behaves.
      arrow.addEventListener('click', function (e) {
        e.stopPropagation();
        if (window._motionRevealedLayers) {
          var ri2 = window._motionRevealedLayers.indexOf(li);
          if (ri2 >= 0) window._motionRevealedLayers.splice(ri2, 1);
        }
        window._motionExpandedLayer = isLayerExpanded(li) ? null : li;
        window._motionExpandedElement = null;
        _propFilter = null; // a hand-opened layer shows all its properties
        renderLayerList(); renderTimeline();
      });
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
      var eye = document.createElement('div'); eye.className = 'lico' + (ld.visible ? '' : ' off'); eye.title = SM.t('layerEyeTitle'); eye.innerHTML = ld.visible ? ICO_EYE : ICO_EYE_CLOSED;
      eye.addEventListener('click', function (e) { e.stopPropagation(); window.SM.toggleLayerVis(li); });
      row.appendChild(eye);
      var lock = document.createElement('div'); lock.className = 'lico' + (ld.locked ? '' : ' off'); lock.title = SM.t('layerLockTitle'); lock.innerHTML = ld.locked ? ICO_LOCK : ICO_UNLOCK;
      lock.addEventListener('click', function (e) { e.stopPropagation(); window.SM.toggleLayerLock(li); });
      row.appendChild(lock);
      var solo = document.createElement('div'); solo.className = 'lico solo-btn' + (ld.solo ? ' on' : ' off'); solo.title = SM.t('layerSoloTitle'); solo.textContent = 'S';
      solo.addEventListener('click', function (e) { e.stopPropagation(); window.SM.toggleLayerSolo(li); });
      row.appendChild(solo);
      // 3D layer toggle (2026-07-28) — same icon/button as Animation 2D's
      // own layer list (timeline.js renderLayerList), shown here too since
      // this is precisely where the Position Z/Rotation X/Y properties it
      // reveals actually live and get keyframed.
      var d3 = document.createElement('div'); d3.className = 'lico' + (ld.threeD ? '' : ' off'); d3.title = '3D Layer'; d3.innerHTML = ICO_3D;
      d3.addEventListener('click', function (e) { e.stopPropagation(); toggleLayer3D(li); renderLayerList(); });
      row.appendChild(d3);
      // Mograph duplicator toggle — shown here too since the dupOffset*
      // properties it reveals live/get keyframed in this list (same
      // reasoning as the 3D toggle above).
      var ddup = document.createElement('div'); ddup.className = 'lico' + (ld.duplicator ? '' : ' off'); ddup.title = 'Duplicator (grille / radial / chemin)'; ddup.innerHTML = ICO_DUP;
      ddup.addEventListener('click', function (e) { e.stopPropagation(); toggleLayerDuplicator(li); });
      row.appendChild(ddup);
      // Same badge as Animation 2D's rows, from the same decider — Motion is
      // a different VIEW of these layers, not a different set, so it must not
      // describe them differently.
      var kind = window.SMLayerKind ? SMLayerKind.of(ld) : null;
      if (kind && kind.key !== 'draw') {
        var kb = document.createElement('div'); kb.className = 'lkind lkind-' + kind.key;
        kb.title = kind.label; kb.innerHTML = kind.icon;
        row.appendChild(kb);
      }
      var nm = document.createElement('div'); nm.className = 'lnm'; nm.textContent = ld.name || ('Layer ' + (li + 1));
      row.appendChild(nm);
      // Parent column — the SAME buildParentCell Animation 2D's rows use
      // (2026-07-25, "tu n'as pas porté le parentage là-bas"). Motion had
      // parenting only as a dropdown buried in the right-hand properties
      // panel, which shows the ACTIVE layer alone: you could not see which
      // layers were parented, compare two of them, or re-parent without
      // first selecting. AE puts it in the timeline for exactly that reason,
      // and Animation 2D already had it here. Same function, so the cycle
      // refusal, the descendant greying and the new pickwhip come along for
      // free rather than being reimplemented (and drifting) per timeline.
      if (typeof buildParentCell === 'function') buildParentCell(row, ld, li);
      row.addEventListener('click', function (e) {
        // A completed drag-drop still fires a trailing native 'click' on
        // mouseup — same guard timeline.js's own layer rows use (see its
        // comment there) so releasing a reorder drag doesn't ALSO toggle
        // this row's Transform-group expansion.
        if (window._layerDragJustEnded) { window._layerDragJustEnded = false; return; }
        // Cmd/Ctrl-click and Shift-click multi-select — same convention as
        // Animation 2D's own layer rows (timeline.js renderLayerList), added
        // here specifically so 2+ layers can be selected DIRECTLY (no need
        // to expand tracks or marquee individual keyframe diamonds first)
        // to trigger the stagger box below. Feedback: "je vois pas trop ce
        // que ça fait ton truc pas aussi intuitif que skew" — the reference
        // stagger gesture starts from selecting whole LAYERS, not keys.
        if (e.metaKey || e.ctrlKey) {
          if (_layerSel.indexOf(state.activeLayerIdx) < 0) _layerSel.push(state.activeLayerIdx);
          var p = _layerSel.indexOf(li); if (p >= 0) _layerSel.splice(p, 1); else _layerSel.push(li);
          window.SM.setActiveLayer(li);
          renderLayerList(); renderTimeline();
          return;
        }
        // Shift-click used to require a PRE-EXISTING selection (`&&
        // _layerSel.length`), and Motion's plain click a few lines down
        // cleared it to [] — so the reflex gesture (click a row, shift-click
        // another) selected nothing at all here, and the only way to reach
        // any multi-layer command was to know about Cmd-click. Animation 2D
        // never had the problem because ITS plain click sets [idx]; Motion
        // now does the same, and the anchor falls back to the active layer
        // so the very first Shift-click works from a cold start.
        if (e.shiftKey) {
          var anchor = _layerSel.length ? _layerSel[0] : state.activeLayerIdx;
          _layerSel = [];
          for (var l = Math.min(anchor, li); l <= Math.max(anchor, li); l++) _layerSel.push(l);
          window.SM.setActiveLayer(li);
          renderLayerList(); renderTimeline();
          return;
        }
        _layerSel = [li];
        // A row can be open via the single-accordion state OR via U's
        // reveal set (or both) — always drop it from the reveal set on
        // click, but only touch the single-accordion value if THIS row is
        // the one holding it, so clicking a U-revealed row never collapses
        // some unrelated row that's separately accordion-open.
        if (window._motionRevealedLayers) {
          var ri = window._motionRevealedLayers.indexOf(li);
          if (ri >= 0) window._motionRevealedLayers.splice(ri, 1);
        }
        // Selecting no longer EXPANDS (2026-07-27: "quand on select un calque
        // cela ne doit pas ouvrir son dropdown de property, on doit le faire
        // manuellement"). AE separates the two: click the name to select,
        // click the twirl-down to open. The arrow below owns expansion now,
        // so selecting several layers no longer unfolds a wall of property
        // rows you then have to close one by one.
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
        // Re-reversed 2026-07-17 ("montage des éléments dans le
        // component") — a Component layer (already converted via its
        // first Position/etc keyframe, see maybeAutoConvertToComponent)
        // now DOES have an "enter as precomp" double-click again, but only
        // once it's a Component: a plain layer with several unrelated
        // shapes still gets the old Release-to-Layers split, since there's
        // no "inside" to browse before that first key exists.
        if (ld.symbolId) { enterComponentLayer(li); return; }
        splitLayerIntoElements(li);
      });
      // Discoverability: the Cmd/Ctrl-click multi-select + drag-the-handle
      // gesture above has no visible affordance until you already know
      // about it — feedback: "je vois rien... rien n'indique visuellement
      // qu'il faut Cmd-clic". A right-click offers the same result as a
      // typed number, same UX convention layer-inout.js's own staggerBars
      // context-menu entry already uses for layer in/out bars.
      row.addEventListener('contextmenu', function (e) {
        if (!window.showContextMenu) return;
        e.preventDefault(); e.stopPropagation();
        // Used to open ONLY for a 2+ layer selection (it held a single
        // stagger entry) — so right-clicking one layer in Motion did
        // nothing at all. Now always opens: the split/merge pair below is
        // the documented way back from Motion's own double-click-to-split
        // (2026-07-25), and it has to be reachable from the row you just
        // split, which is by definition where you look for it.
        var multi = _layerSel.length >= 2 && _layerSel.indexOf(li) >= 0;
        window.showContextMenu(e.clientX, e.clientY, [
          { label: 'Éclater en calques (une forme par calque)', disabled: !!ld.symbolId || !!ld.lfsGroup, action: function () { window.SM.splitLayerIntoElements(li); } },
          { label: 'Couper au niveau de la tête de lecture  (⌘⇧D)', action: function () { window.SM.splitLayerAtPlayhead(li); } },
          { label: ld.shy ? 'Retirer le marquage « shy »' : 'Marquer comme « shy »', action: function () { window.SM.toggleLayerShy(li); } },
          { label: ld.motionBlur ? 'Désactiver le flou de mouvement' : 'Activer le flou de mouvement', action: function () { window.SM.toggleLayerMotionBlur(li); } },
          { label: ld.effectsFrom ? 'Ne plus hériter des effets' : 'Hériter des effets d\u2019un calque…', action: function () {
            if (ld.effectsFrom) { pushUndo(); delete ld.effectsFrom; renderLayerList(); renderTimeline(); if (window.SMEngineBridge) SMEngineBridge.renderNow(); return; }
            var items = [];
            state.layers.forEach(function (other, oi) {
              if (oi === li || !other.effects || !other.effects.length) return;
              items.push({ label: (other.name || ('Layer ' + (oi + 1))) + '  (' + other.effects.length + ')', action: function () {
                pushUndo();
                ld.effectsFrom = ensureLayerUid(other);
                renderLayerList(); renderTimeline();
                if (window.SMEngineBridge) SMEngineBridge.renderNow();
                if (window.showToast) showToast('Effets hérités de « ' + (other.name || ('Layer ' + (oi + 1))) + ' » — ils suivent leurs propres keyframes');
              } });
            });
            if (!items.length) { if (window.showToast) showToast('Aucun autre calque ne porte d\u2019effets'); return; }
            window.showContextMenu(e.clientX + 8, e.clientY + 8, items);
          } },
          { sep: true },
          // showContextMenu has no submenus — a disabled row is the honest
          // way to title a group rather than a button that does nothing.
          { label: 'Verrouiller les keyframes sur :', disabled: true, action: function () {} },
          { label: '   • le point d\u2019entrée' + (ld.keyLock === 'in' ? '  ✓' : ''), action: function () { window.SM.setLayerKeyLock(li, ld.keyLock === 'in' ? null : 'in'); } },
          { label: '   • le point de sortie' + (ld.keyLock === 'out' ? '  ✓' : ''), action: function () { window.SM.setLayerKeyLock(li, ld.keyLock === 'out' ? null : 'out'); } },
          { label: '   • le calque entier' + (ld.keyLock === 'layer' ? '  ✓' : ''), action: function () { window.SM.setLayerKeyLock(li, ld.keyLock === 'layer' ? null : 'layer'); } },
          { label: 'Ajouter un repère sur ce calque', action: function () { if (window.SMMarkers) SMMarkers.addLayerMarker(li, state.currentFrame, ''); } },
          { label: ld.timeRemap ? 'Désactiver le remappage temporel' : 'Activer le remappage temporel', disabled: !ld.symbolId,
            action: function () { ld.timeRemap ? disableTimeRemap(li) : enableTimeRemap(li); } },
          { label: 'Fusionner les calques sélectionnés', disabled: !multi, action: function () { window.SM.mergeLayersIntoOne(_layerSel.slice()); } },
          { sep: true },
          { label: 'Échelonner les calques sélectionnés…', disabled: !multi, action: function () {
            var v = prompt('Décalage entre calques (frames)', '2');
            var step = parseInt(v, 10);
            if (!isNaN(step) && step !== 0) staggerSelectedLayers(step);
          } },
        ]);
      });
      list.appendChild(row);
      if (!expanded) return;
      renderTransformGroup(list, ld, 'Transform');
      // Per-element sub-list used to be component-exclusive ("a symbol
      // instance's actual strokes live inside the SYMBOL's own sub-layer,
      // not addressable as elements of this outer layer") — true only
      // while elementMotionAt forced null for ld.symbolId. Since that's
      // lifted (getEffectiveStrokes' ld.symbolId branch now applies
      // per-shape elementMotion in addition to the instance's own
      // placement, 2026-07-17 "precomp par calque"), layerElements/
      // ensureElementHolder work correctly for a Component instance too —
      // showing Éléments here lets a single shape inside a placed
      // instance be animated right from the Scene view, without needing
      // to double-click all the way into the component first.
      renderElementsList(list, li, ld);
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
    if (!ld) { nameRow.textContent = SM.t('noLayerSelected'); body.appendChild(nameRow); return; }
    // Component instances get the same Transform group as any layer now
    // (see renderLayerListMotion's comment) — just a label suffix so it
    // reads clearly as "the whole instance", not its internal content.
    nameRow.textContent = (ld.name || ('Layer ' + (state.activeLayerIdx + 1))) + (ld.symbolId ? ' (composant)' : '');
    body.appendChild(nameRow);
    renderParentRow(body, ld, state.activeLayerIdx);
    renderTimeLinkRow(body, ld, state.activeLayerIdx);
    renderTransformGroup(body, ld, 'Transform');
  }
  // Layer parenting (2026-07, "gestion de parentage de calque dans motion
  // comme dans after, avec la possibilité de changer de parent en
  // properties d'animation"): a single dropdown in the properties panel,
  // AE's own "Parent & Link" column reimagined as a row here since Motion's
  // right panel is already per-property, not per-layer-columns. Writes
  // through setLayerParent (cycle-checked) so this is the ONLY UI entry
  // point — the data model (ld.parentLayerUid) + composition math
  // (parentChainMats, wired into engine-bridge.js/export.js/
  // native-video-bridge.js) already existed before this row, but were
  // otherwise completely inert from the user's side.
  function renderParentRow(body, ld, li) {
    var row = document.createElement('div'); row.className = 'lrow motion-prop-row';
    var label = document.createElement('span'); label.textContent = 'Parent'; label.style.minWidth = '70px';
    row.appendChild(label);
    var sel = document.createElement('select'); sel.className = 'motion-parent-select'; sel.style.flex = '1';
    var noneOpt = document.createElement('option'); noneOpt.value = ''; noneOpt.textContent = 'Aucun (parentage libre)';
    sel.appendChild(noneOpt);
    state.layers.forEach(function (other, oi) {
      if (oi === li) return; // can't parent a layer to itself
      var opt = document.createElement('option');
      opt.value = ensureLayerUid(other);
      opt.textContent = other.name || ('Layer ' + (oi + 1));
      if (ld.parentLayerUid && ld.parentLayerUid === opt.value) opt.selected = true;
      sel.appendChild(opt);
    });
    if (!ld.parentLayerUid) noneOpt.selected = true;
    sel.addEventListener('mousedown', function (e) { e.stopPropagation(); });
    sel.addEventListener('click', function (e) { e.stopPropagation(); });
    sel.addEventListener('change', function (e) {
      e.stopPropagation(); pushUndo();
      setLayerParent(li, sel.value || null);
      renderLayerList(); renderTimeline();
    });
    row.appendChild(sel);
    body.appendChild(row);
  }
  // ---- PARENT IN TIME (Van Dijk 2.1) ---------------------------------
  // The spatial Parent row's counterpart: instead of "whose transform do I
  // follow", "whose TIME do I follow". Same pickwhip idiom as parenting and
  // as the expression whip, dropped on another layer's row; the offsets are
  // plain frame fields (scrub-enabled per CLAUDE.md §10).
  function renderTimeLinkRow(body, ld, li) {
    var row = document.createElement('div'); row.className = 'lrow motion-prop-row';
    var label = document.createElement('span'); label.textContent = 'Temps'; label.style.minWidth = '70px';
    row.appendChild(label);

    var whip = document.createElement('span');
    whip.className = 'lpick';
    whip.title = 'Glisser sur un calque : ses points d\u2019entrée/sortie pilotent ceux-ci';
    whip.addEventListener('mousedown', function (e) { startTimeLinkPickwhip(li, whip, e); });
    row.appendChild(whip);

    var srcIdx = -1;
    if (ld.timeLink && ld.timeLink.uid) {
      state.layers.forEach(function (o, oi) { if (o !== ld && o.layerUid === ld.timeLink.uid) srcIdx = oi; });
    }
    var name = document.createElement('span');
    name.className = 'lparent' + (srcIdx < 0 ? ' none' : '');
    name.style.marginLeft = '4px';
    name.textContent = srcIdx >= 0 ? (state.layers[srcIdx].name || ('Layer ' + (srcIdx + 1)))
      : (ld.timeLink ? 'source introuvable' : '—');
    name.title = srcIdx >= 0 ? 'Cliquer pour délier' : 'Aucun lien temporel';
    name.addEventListener('click', function (e) {
      e.stopPropagation();
      if (!ld.timeLink) return;
      pushUndo(); delete ld.timeLink;
      renderLayerList(); renderTimeline();
      if (window.loadFrame) loadFrame(state.currentFrame);
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
      if (window.showToast) showToast('Lien temporel retiré');
    });
    row.appendChild(name);

    if (ld.timeLink) {
      // Which edges follow, and by how much.
      var sel = document.createElement('select'); sel.className = 'motion-parent-select'; sel.style.marginLeft = '6px';
      [['both', 'entrée + sortie'], ['in', 'entrée seule'], ['out', 'sortie seule']].forEach(function (o) {
        var op = document.createElement('option'); op.value = o[0]; op.textContent = o[1];
        if ((ld.timeLink.mode || 'both') === o[0]) op.selected = true;
        sel.appendChild(op);
      });
      sel.addEventListener('mousedown', function (e) { e.stopPropagation(); });
      sel.addEventListener('change', function () {
        pushUndo(); ld.timeLink.mode = sel.value;
        renderLayerList(); renderTimeline();
        if (window.loadFrame) loadFrame(state.currentFrame);
        if (window.SMEngineBridge) SMEngineBridge.renderNow();
      });
      row.appendChild(sel);
      [['inOffset', 'décalage entrée'], ['outOffset', 'décalage sortie']].forEach(function (o) {
        var f = document.createElement('input');
        f.type = 'number'; f.className = 'pi scrub'; f.dataset.step = '1';
        f.style.width = '46px'; f.style.marginLeft = '4px';
        f.title = o[1] + ' (frames)';
        f.value = ld.timeLink[o[0]] || 0;
        function commitOff() {
          var v = parseInt(f.value, 10) || 0;
          if (v === (ld.timeLink[o[0]] || 0)) return;
          pushUndo(); ld.timeLink[o[0]] = v;
          renderLayerList(); renderTimeline();
          if (window.loadFrame) loadFrame(state.currentFrame);
          if (window.SMEngineBridge) SMEngineBridge.renderNow();
        }
        f.addEventListener('change', commitOff);
        f.addEventListener('blur', commitOff);
        f.addEventListener('mousedown', function (e) { e.stopPropagation(); });
        row.appendChild(f);
      });
    }
    body.appendChild(row);
  }
  // Cycle refusal lives HERE (the only place a link is created), mirroring
  // setLayerParent's own contract for spatial parenting: resolveLinkedTime
  // additionally degrades safely if a cycle ever appears through some other
  // route, but a cycle should never be creatable in the first place.
  function timeLinkWouldCycle(li, targetIdx) {
    var seen = {}, cur = targetIdx, guard = 0;
    while (cur >= 0 && guard++ < 64) {
      if (cur === li) return true;
      if (seen[cur]) return false;
      seen[cur] = 1;
      var l = state.layers[cur];
      if (!l || !l.timeLink || !l.timeLink.uid) return false;
      var next = -1;
      state.layers.forEach(function (o, oi) { if (o.layerUid === l.timeLink.uid) next = oi; });
      cur = next;
    }
    return false;
  }
  function startTimeLinkPickwhip(li, fromEl, ev) {
    ev.stopPropagation(); ev.preventDefault();
    var r0 = fromEl.getBoundingClientRect();
    var ox = r0.left + r0.width / 2, oy = r0.top + r0.height / 2;
    var line = document.createElement('div'); line.className = 'lpick-line';
    document.body.appendChild(line);
    var hover = null;
    function paint(x, y) {
      var dx = x - ox, dy = y - oy;
      line.style.left = ox + 'px'; line.style.top = oy + 'px';
      line.style.width = Math.sqrt(dx * dx + dy * dy) + 'px';
      line.style.transform = 'rotate(' + Math.atan2(dy, dx) + 'rad)';
    }
    function rowUnder(x, y) {
      var el = document.elementFromPoint(x, y); if (!el || !el.closest) return null;
      var row = el.closest('.lrow[data-layer], .frow[data-layer]');
      if (!row) return null;
      var idx = parseInt(row.dataset.layer, 10);
      if (isNaN(idx) || idx === li || timeLinkWouldCycle(li, idx)) return null;
      return { row: row, idx: idx };
    }
    function onMove(e) {
      paint(e.clientX, e.clientY);
      var t = rowUnder(e.clientX, e.clientY);
      if (hover && (!t || t.row !== hover.row)) hover.row.classList.remove('pick-target');
      if (t && (!hover || t.row !== hover.row)) t.row.classList.add('pick-target');
      hover = t;
    }
    function cleanup() {
      document.removeEventListener('mousemove', onMove, true);
      document.removeEventListener('mouseup', onUp, true);
      document.removeEventListener('keydown', onKey, true);
      if (hover) hover.row.classList.remove('pick-target');
      line.remove();
    }
    function onUp(e) {
      var t = rowUnder(e.clientX, e.clientY);
      cleanup();
      if (t == null) return;
      var ld = state.layers[li], src = state.layers[t.idx];
      pushUndo();
      // Seed the offsets from the CURRENT gap, so linking never makes the
      // layer jump: it stays exactly where it is and only starts following.
      var myIn = layerInPoint(ld), myOut = layerOutPoint(ld);
      ld.timeLink = {
        uid: ensureLayerUid(src),
        inOffset: myIn - layerInPoint(src),
        outOffset: myOut - layerOutPoint(src),
        mode: 'both',
      };
      renderLayerList(); renderTimeline();
      if (window.loadFrame) loadFrame(state.currentFrame);
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
      if (window.showToast) showToast('Temps lié à « ' + (src.name || ('Layer ' + (t.idx + 1))) + ' »');
    }
    function onKey(e) { if (e.key === 'Escape') cleanup(); }
    document.addEventListener('mousemove', onMove, true);
    document.addEventListener('mouseup', onUp, true);
    document.addEventListener('keydown', onKey, true);
    paint(ev.clientX, ev.clientY);
  }
  // Shared by all three stopwatch icons (Transform group properties, fill
  // color row, vertex row) — same three-state title logic, only the "off"
  // wording differs per target (property/fill/vertex).
  function stopwatchTitle(offKey, swOn, hasKeyHere) {
    return !swOn ? SM.t(offKey) : (hasKeyHere ? SM.t('motionKeyRemove') : SM.t('motionKeyAdd'));
  }
  // Shared by the layer's own Transform group AND each element's — both are
  // just "a holder with .motion/.motionStatic", see the header comment on
  // ensureElementHolder. `refreshDeep` is called after any structural change
  // (stopwatch toggle) since that can affect which rows/tracks exist;
  // scrubbing a value only needs the timeline (track content) + canvas.
  function renderTransformGroup(list, holder, groupLabel) {
    if (showsGroupHeader()) renderTransformHeader(list, groupLabel);
    renderTransformProps(list, holder);
  }
  function renderTransformHeader(list, groupLabel) {
    var grp = document.createElement('div'); grp.className = 'lrow motion-group-row';
    var grpLabel = document.createElement('span'); grpLabel.textContent = groupLabel;
    grp.appendChild(grpLabel);
    var filterBtn = document.createElement('span'); filterBtn.className = 'motion-filter-btn' + (_hideUnanimated ? ' on' : '');
    filterBtn.title = SM.t(_hideUnanimated ? 'motionFilterShowAll' : 'motionFilterHideUnanimated');
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
  }
  function renderTransformProps(list, holder) {
    propsFor(holder).forEach(function (prop) {
      if (isPropFiltered(prop) || (_hideUnanimated && !propHasContent(holder, prop))) return;
      var pr = document.createElement('div'); pr.className = 'lrow motion-prop-row';
      // Same identity tags the grid's track rows carry (renderTracksFor) —
      // without them the expression pickwhip could only be dropped on the
      // grid side, which is the half you are NOT looking at while writing
      // an expression in the panel.
      pr._smHolder = holder; pr._smProp = prop;
      var sw = document.createElement('div');
      var swOn = isAnimated(holder, prop);
      var hasKeyHere = swOn && !!keyAt(trackFor(holder, prop), state.currentFrame);
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
      sw.title = stopwatchTitle('motionAnimateProp', swOn, hasKeyHere);
      sw.innerHTML = '<svg viewBox="0 0 24 24" width="12" height="12"><path d="M12 3l9 9-9 9-9-9z" fill="' + (hasKeyHere ? 'currentColor' : 'none') + '" stroke="currentColor" stroke-width="2"/></svg>';
      sw.addEventListener('click', function (e) {
        e.stopPropagation(); pushUndo();
        if (!swOn) {
          toggleAnimated(holder, prop); // OFF->ON: first key at the current frame (see toggleAnimated's own comment)
        } else if (prop === 'timeRemap') {
          // No static-freeze fallback for timeRemap (see toggleAnimated) —
          // with a key here, removing down to one key is fine, but killing
          // the LAST pair means "turn remapping off", which has its own
          // proper off-switch.
          if (hasKeyHere && trackFor(holder, prop).keys.length > 1) removeKeyAtCurrentFrame(holder, prop);
          else if (hasKeyHere) toggleAnimated(holder, prop);
          else setKeyAtCurrentFrame(holder, prop, valueAtFrame(holder, prop, state.currentFrame));
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
      var exprOn = holder.expressions && holder.expressions[prop] && holder.expressions[prop].enabled;
      var exprErr = holder.expressions && holder.expressions[prop] && holder.expressions[prop].lastError;
      var exprBtn = document.createElement('div');
      exprBtn.className = 'lico motion-expr-btn' + (exprOn ? ' on' : '') + (exprErr ? ' err' : '');
      exprBtn.title = exprErr ? ('Expression en erreur : ' + exprErr) : (exprOn ? 'Expression active — clic pour éditer' : 'Ajouter une expression sur cette propriété');
      exprBtn.innerHTML = '<svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" stroke-width="2"><path d="M8 4c-2 0-3 1.2-3 3v10c0 2-1 3-3 3M16 4c2 0 3 1.2 3 3v10c0 2 1 3 3 3M9 12h6"/></svg>';
      exprBtn.addEventListener('click', function (e) {
        e.stopPropagation();
        var already = window._exprEditorOpen && window._exprEditorOpen.holder === holder && window._exprEditorOpen.prop === prop;
        window._exprEditorOpen = already ? null : { holder: holder, prop: prop };
        // Bug found live (2026-07 — "pas compris où est ce qu'elle sont
        // écrite"): both containers this row can render into (#layer-list's
        // bottom Transform group, #motion-props-body's right-panel mirror)
        // are small fixed-height scrolling panels with no visible scrollbar
        // affordance — clicking ƒx opened the editor exactly as designed,
        // but the newly-inserted textarea routinely landed BELOW the
        // already-scrolled viewport of that panel, reading as "nothing
        // happened". Scroll the container the user actually clicked in (not
        // its sibling mirror, which may not even be visible) so the editor
        // is guaranteed on-screen the instant it opens.
        var container = exprBtn.closest('#layer-list, #motion-props-body');
        renderLayerList(); renderTimeline();
        if (container && !already) {
          var editorRow = container.querySelector('.motion-expr-editor');
          if (editorRow) editorRow.scrollIntoView({ block: 'nearest' });
        }
      });
      pr.appendChild(sw); pr.appendChild(exprBtn); pr.appendChild(pnm);
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
      if (window._exprEditorOpen && window._exprEditorOpen.holder === holder && window._exprEditorOpen.prop === prop) {
        list.appendChild(buildExprEditorRow(holder, prop));
      }
    });
  }
  // Inline expression editor — appended right after its property's row when
  // toggled open via the ƒx button, same single-accordion convention as
  // _motionExpandedLayer/_motionExpandedElement/_motionExpandedPathHolder
  // elsewhere in this file (only one open at a time, tracked on window so a
  // full re-render can restore which one).
  function buildExprEditorRow(holder, prop) {
    var expr = ensureExpr(holder, prop);
    var row = document.createElement('div'); row.className = 'lrow motion-expr-editor';
    var head = document.createElement('label'); head.className = 'motion-expr-toggle';
    var cb = document.createElement('input'); cb.type = 'checkbox'; cb.checked = !!expr.enabled;
    cb.addEventListener('change', function () {
      pushUndo();
      expr.enabled = cb.checked;
      if (typeof saveActiveLayerFrame === 'function') saveActiveLayerFrame();
      renderLayerList(); renderTimeline();
      if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    });
    head.appendChild(cb);
    head.appendChild(document.createTextNode(' Activer l’expression'));
    row.appendChild(head);
    // The value the property WOULD have without the expression (Van Dijk
    // 7.4). With an expression on, the field above shows the RESULT, and
    // the underlying keyframed value becomes invisible — you end up
    // disabling the expression just to see what you are driving.
    var rawWrap = document.createElement('span');
    rawWrap.className = 'motion-expr-raw';
    var raw = rawValueAtFrame(holder, prop, state.currentFrame);
    rawWrap.textContent = 'sans expression : ' + raw.map(function (n) { return Math.round(n * 100) / 100; }).join(', ') + ' ' + (PROP_UNIT[prop] || '');
    rawWrap.title = 'Valeur de la propriété avant application de l’expression, à la frame courante';
    row.appendChild(rawWrap);
    // Project-wide preamble (7.2) — reachable from the same place you write
    // the expression that uses it, rather than a settings panel away.
    var glob = document.createElement('button');
    glob.className = 'motion-expr-glob';
    glob.textContent = exprGlobals() ? 'Variables globales ✓' : 'Variables globales…';
    glob.title = 'Code exécuté avant CHAQUE expression du projet — déclare ici ce que tu réutilises partout';
    glob.addEventListener('click', function (e) {
      e.preventDefault(); e.stopPropagation();
      var v = prompt('Variables globales — exécutées avant chaque expression\nex : var beat = 60 / 120 * ' + (state.fps || 24) + ';', exprGlobals());
      if (v === null) return;
      pushUndo();
      window.SMMotion.setExprGlobals(v);
    });
    row.appendChild(glob);
    // ---- code pane (Van Dijk 7.5) ------------------------------------
    // A 3-row bare textarea was fine when an expression was one line; it
    // stops being fine the moment people actually write in it, which is his
    // whole point ("expressions... have taken on a more dominant role").
    // A gutter of line numbers scrolled in lockstep with the textarea, the
    // failing line highlighted, and a drag-to-resize grip — no external
    // editor dependency, which would be a lot of weight for this.
    var pane = document.createElement('div'); pane.className = 'motion-expr-pane';
    var gutter = document.createElement('div'); gutter.className = 'motion-expr-gutter';
    var ta = document.createElement('textarea'); ta.className = 'motion-expr-code';
    ta.value = expr.code; ta.spellcheck = false;
    ta.placeholder = 'value + wiggle(2, 10)';
    pane.appendChild(gutter); pane.appendChild(ta);
    // The line number the error points at, when the message carries one.
    // new Function() reports positions against the wrapper we build in
    // compiledFnFor, not against the user's own text, so only a number we
    // can actually trust is used — no guessing at an offset.
    function errorLine() {
      if (!expr.lastError) return -1;
      var mm = /(?:ligne|line)\s*(\d+)/i.exec(expr.lastError);
      return mm ? parseInt(mm[1], 10) : -1;
    }
    function paintGutter() {
      var n = (ta.value.split('\n').length) || 1;
      var el = errorLine();
      var html = '';
      for (var i = 1; i <= n; i++) html += '<span' + (i === el ? ' class="err"' : '') + '>' + i + '</span>';
      gutter.innerHTML = html;
      gutter.scrollTop = ta.scrollTop;
    }
    ta.addEventListener('scroll', function () { gutter.scrollTop = ta.scrollTop; });
    ta.addEventListener('input', paintGutter);
    function commit() {
      if (ta.value === expr.code) return;
      pushUndo();
      expr.code = ta.value;
      expr.lastError = null;
      if (holder._exprCompiled) delete holder._exprCompiled[prop];
      if (typeof saveActiveLayerFrame === 'function') saveActiveLayerFrame();
      if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    }
    ta.addEventListener('blur', commit);
    ta.addEventListener('keydown', function (e) {
      if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') { commit(); e.preventDefault(); ta.blur(); }
      if (e.key === 'Escape') { ta.value = expr.code; paintGutter(); ta.blur(); }
      // Tab indents instead of leaving the field — in a code box, losing
      // focus on Tab is never what you meant.
      if (e.key === 'Tab') {
        e.preventDefault();
        var s = ta.selectionStart, en = ta.selectionEnd;
        ta.value = ta.value.slice(0, s) + '  ' + ta.value.slice(en);
        ta.selectionStart = ta.selectionEnd = s + 2;
        paintGutter();
      }
    });
    // Resize grip. CRITICAL (CLAUDE.md §11): the frame grid mirrors this
    // row's REAL height to keep its own rows aligned — so every height
    // change has to re-render the timeline, or the two panels drift apart
    // by exactly the amount the editor grew.
    var grip = document.createElement('div'); grip.className = 'motion-expr-grip';
    grip.title = 'Glisser pour redimensionner l\u2019éditeur';
    grip.addEventListener('mousedown', function (e) {
      e.preventDefault(); e.stopPropagation();
      var startY = e.clientY, startH = ta.offsetHeight;
      function mv(ev) {
        ta.style.height = Math.max(38, Math.min(420, startH + (ev.clientY - startY))) + 'px';
        gutter.style.height = ta.style.height;
      }
      function up() {
        document.removeEventListener('mousemove', mv);
        document.removeEventListener('mouseup', up);
        expr.editorHeight = parseInt(ta.style.height, 10) || undefined;
        renderTimeline(); // re-reserve the matching height on the grid side
      }
      document.addEventListener('mousemove', mv);
      document.addEventListener('mouseup', up);
    });
    pane.appendChild(grip);
    if (expr.editorHeight) { ta.style.height = expr.editorHeight + 'px'; gutter.style.height = expr.editorHeight + 'px'; }
    row.appendChild(pane);
    paintGutter();
    if (expr.lastError) {
      var errEl = document.createElement('div'); errEl.className = 'motion-expr-error'; errEl.textContent = expr.lastError;
      row.appendChild(errEl);
    }
    // ---- pickwhip (Van Dijk 7.3) -------------------------------------
    // Drag onto another property row to write its reference into the code;
    // Alt-drag CLONES that property's own expression instead — his exact
    // request ("clone the Expression itself rather than the value it
    // returns"), which is what stops duplicated code from having to be
    // updated in N places.
    var whip = document.createElement('span');
    whip.className = 'motion-expr-whip';
    whip.title = 'Glisser sur une autre propriété : insère sa référence.\nAlt+glisser : clone SON expression.';
    whip.addEventListener('mousedown', function (e) { startExprPickwhip(holder, prop, ta, commit, whip, e); });
    head.appendChild(whip);
    return row;
  }
  // Rubber-band pickwhip for expressions. Deliberately the same shape as
  // startParentPickwhip (timeline.js) — same line, same highlighted target,
  // same Escape-cancels — so the two gestures feel like one idea; only the
  // drop target differs (property rows here, layer rows there).
  function startExprPickwhip(holder, prop, ta, commit, fromEl, ev) {
    ev.stopPropagation(); ev.preventDefault();
    var r0 = fromEl.getBoundingClientRect();
    var ox = r0.left + r0.width / 2, oy = r0.top + r0.height / 2;
    var line = document.createElement('div'); line.className = 'lpick-line';
    document.body.appendChild(line);
    var hover = null;
    function paint(x, y) {
      var dx = x - ox, dy = y - oy;
      line.style.left = ox + 'px'; line.style.top = oy + 'px';
      line.style.width = Math.sqrt(dx * dx + dy * dy) + 'px';
      line.style.transform = 'rotate(' + Math.atan2(dy, dx) + 'rad)';
    }
    // A property row knows its holder/prop through the same _smHolder/_smProp
    // tags the grid rows carry; the PANEL rows carry them too (set below in
    // renderTransformGroup) so either side can be dropped on.
    function targetUnder(x, y) {
      var el = document.elementFromPoint(x, y); if (!el || !el.closest) return null;
      var row = el.closest('.motion-prop-row, .motion-track-row');
      if (!row || !row._smHolder || !row._smProp) return null;
      if (row._smHolder === holder && row._smProp === prop) return null; // itself
      return { row: row, holder: row._smHolder, prop: row._smProp };
    }
    function onMove(e) {
      paint(e.clientX, e.clientY);
      var t = targetUnder(e.clientX, e.clientY);
      if (hover && (!t || t.row !== hover.row)) hover.row.classList.remove('pick-target');
      if (t && (!hover || t.row !== hover.row)) t.row.classList.add('pick-target');
      hover = t;
    }
    function cleanup() {
      document.removeEventListener('mousemove', onMove, true);
      document.removeEventListener('mouseup', onUp, true);
      document.removeEventListener('keydown', onKey, true);
      if (hover) hover.row.classList.remove('pick-target');
      line.remove();
    }
    function onUp(e) {
      var t = targetUnder(e.clientX, e.clientY);
      var alt = !!e.altKey;
      cleanup();
      if (!t) return;
      var text;
      if (alt) {
        // Clone the source's OWN expression. Nothing to clone is worth
        // saying out loud rather than silently inserting an empty string.
        var srcEx = t.holder.expressions && t.holder.expressions[t.prop];
        if (!srcEx || !srcEx.code) { if (window.showToast) showToast('Cette propriété n\u2019a pas d\u2019expression à cloner'); return; }
        text = srcEx.code;
      } else {
        var li = state.layers.indexOf(t.holder);
        // A per-element holder isn't in state.layers — reference its OWNER
        // layer, which is what the sandbox's layer() can actually resolve.
        if (li < 0) state.layers.forEach(function (ld2, i2) {
          if (ld2.elementMotion) Object.keys(ld2.elementMotion).forEach(function (k) { if (ld2.elementMotion[k] === t.holder) li = i2; });
        });
        if (li < 0) { if (window.showToast) showToast('Propriété non référençable'); return; }
        text = 'layer("' + ensureLayerUid(state.layers[li]) + '").' + t.prop;
      }
      // Insert at the caret rather than replacing: a pickwhip is usually
      // used mid-expression (`value + <here>`), not on an empty box.
      var s = ta.selectionStart != null ? ta.selectionStart : ta.value.length;
      var en = ta.selectionEnd != null ? ta.selectionEnd : s;
      ta.value = ta.value.slice(0, s) + text + ta.value.slice(en);
      ta.selectionStart = ta.selectionEnd = s + text.length;
      ta.dispatchEvent(new Event('input'));
      commit();
      renderLayerList(); renderTimeline();
      if (window.showToast) showToast(alt ? 'Expression clonée' : 'Référence insérée');
    }
    function onKey(e) { if (e.key === 'Escape') cleanup(); }
    document.addEventListener('mousemove', onMove, true);
    document.addEventListener('mouseup', onUp, true);
    document.addEventListener('keydown', onKey, true);
    paint(ev.clientX, ev.clientY);
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
      // Path property (2026-07): opt-in extended property, hidden unless
      // the element actually has vertex geometry (a Raster/image entry
      // never does) — same "hidden by default, opt-in" convention CLAUDE.md
      // §8 documents for fill/stroke/brush extended properties.
      if (!entry.sd.isRaster && entry.sd.segments && entry.sd.segments.length) {
        renderPathVertexGroup(list, ensureElementHolder(ld, entry.strokeId), entry.sd.segments.length);
      }
      // Fill color (2026-07): opt-in extended property, hidden unless the
      // element actually has a fill — same convention as Path above.
      if (entry.sd.fillColor) {
        renderFillColorRow(list, ensureElementHolder(ld, entry.strokeId), entry.sd.fillColor);
      }
    });
  }
  // Reads a hex6/hex8 string into an [r,g,b,a] 0-255 array — the SAME shape
  // buildSceneJson's cssColorToRgba already produces for the live render,
  // so a keyed fillColor value needs zero conversion at render time.
  function hexToRgba255(hex) {
    var h = (hex || '#000000').replace('#', '');
    var r = parseInt(h.substr(0, 2), 16) || 0, g = parseInt(h.substr(2, 2), 16) || 0, b = parseInt(h.substr(4, 2), 16) || 0;
    var a = h.length >= 8 ? parseInt(h.substr(6, 2), 16) : 255;
    return [r, g, b, isNaN(a) ? 255 : a];
  }
  function rgba255ToHex(v) {
    function h2(n) { return Math.max(0, Math.min(255, Math.round(n || 0))).toString(16).padStart(2, '0'); }
    return '#' + h2(v[0]) + h2(v[1]) + h2(v[2]) + h2(v[3] !== undefined ? v[3] : 255);
  }
  // Single row (not an accordion group — one color, nothing to expand),
  // same stopwatch/keying contract as renderVertexRow but with a color
  // swatch instead of numeric scrub fields. Opens the SAME color-picker
  // popover the layer-color dot uses elsewhere (timeline.js).
  function renderFillColorRow(list, holder, currentFillColorHex) {
    var prop = 'fillColor';
    var row = document.createElement('div'); row.className = 'lrow motion-prop-row';
    var sw = document.createElement('div');
    var swOn = isAnimated(holder, prop);
    var hasKeyHere = swOn && !!keyAt(holder.motion[prop], state.currentFrame);
    sw.className = 'lico motion-stopwatch' + (swOn ? ' on' : '');
    sw.title = stopwatchTitle('motionAnimateFill', swOn, hasKeyHere);
    sw.innerHTML = '<svg viewBox="0 0 24 24" width="12" height="12"><path d="M12 3l9 9-9 9-9-9z" fill="' + (hasKeyHere ? 'currentColor' : 'none') + '" stroke="currentColor" stroke-width="2"/></svg>';
    function currentRgba() {
      if (hasKeys(holder, prop)) return valueAtFrame(holder, prop, state.currentFrame);
      if (holder.motionStatic && holder.motionStatic[prop]) return holder.motionStatic[prop];
      return hexToRgba255(currentFillColorHex);
    }
    sw.addEventListener('click', function (e) {
      e.stopPropagation(); pushUndo();
      if (!swOn) {
        if (!holder.motionStatic) holder.motionStatic = {};
        if (!holder.motionStatic[prop]) holder.motionStatic[prop] = hexToRgba255(currentFillColorHex);
        toggleAnimated(holder, prop);
      } else if (hasKeyHere) {
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
    var nm = document.createElement('div'); nm.className = 'lnm'; nm.textContent = 'Fill';
    var swatch = document.createElement('div');
    swatch.style.cssText = 'width:16px;height:16px;border-radius:3px;border:1px solid var(--border2);cursor:pointer;margin-left:auto;';
    var rgba = currentRgba();
    swatch.style.background = 'rgba(' + rgba[0] + ',' + rgba[1] + ',' + rgba[2] + ',' + ((rgba[3] !== undefined ? rgba[3] : 255) / 255) + ')';
    swatch.title = 'Couleur de fill de cette forme';
    swatch.addEventListener('click', function (e) {
      e.stopPropagation();
      if (!window.openLayerColorSwatches) return;
      openLayerColorSwatches(swatch, rgba255ToHex(currentRgba()), function (hex) {
        pushUndo();
        var v = hexToRgba255(hex);
        setValue(holder, prop, v);
        renderLayerList(); renderTimeline();
        if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
      });
    });
    row.appendChild(sw); row.appendChild(nm); row.appendChild(swatch);
    list.appendChild(row);
  }
  // "Path" group — one row per vertex, each independently keyable (2026-07,
  // "les properties de path dont les vertices peuvent être animé
  // séparément on doit voir cette propriété"). Single-accordion per holder
  // (window._motionExpandedPathHolder), same pattern as
  // window._motionExpandedElement one level up. Mirrored in
  // renderTimelineMotion below — MUST stay in exact sync (same expand
  // condition, same vertex count) or the panel/grid rows desync (ROW_H's
  // own header comment already warns about this class of bug for the base
  // Transform rows).
  function isPathGroupExpanded(holder) { return window._motionExpandedPathHolder === holder; }
  function renderPathVertexGroup(list, holder, vertexCount) {
    var grp = document.createElement('div'); grp.className = 'lrow motion-group-row';
    var arrow = document.createElement('span'); arrow.className = 'lico larrow'; arrow.textContent = isPathGroupExpanded(holder) ? '▾' : '▸';
    var label = document.createElement('span'); label.textContent = 'Path';
    grp.appendChild(arrow); grp.appendChild(label);
    grp.addEventListener('click', function (e) {
      e.stopPropagation();
      window._motionExpandedPathHolder = isPathGroupExpanded(holder) ? null : holder;
      renderLayerList(); renderTimeline();
    });
    list.appendChild(grp);
    if (!isPathGroupExpanded(holder)) return;
    for (var vi = 0; vi < vertexCount; vi++) renderVertexRow(list, holder, vi);
  }
  // A vertex row is a lean version of renderTransformGroup's own per-prop
  // row — single stopwatch (same 3-state convention: hollow/blue-outline/
  // solid-blue) driving the SAME generic toggleAnimated/setKeyAtCurrentFrame/
  // removeKeyAtCurrentFrame machinery the 5 base properties use, just with
  // prop='vtx'+vi instead of one of PROPS. No numeric value fields here
  // (unlike Position/Scale/etc.) — a vertex offset is set by DRAGGING the
  // vertex on canvas (select-bridge, not built here), the stopwatch only
  // arms/disarms keyframing and jumps to/removes a key at the playhead.
  function renderVertexRow(list, holder, vi) {
    var prop = 'vtx' + vi;
    var row = document.createElement('div'); row.className = 'lrow motion-prop-row motion-vertex-row';
    var sw = document.createElement('div');
    var swOn = isAnimated(holder, prop);
    var hasKeyHere = swOn && !!keyAt(holder.motion[prop], state.currentFrame);
    sw.className = 'lico motion-stopwatch' + (swOn ? ' on' : '');
    sw.title = stopwatchTitle('motionAnimateVertex', swOn, hasKeyHere);
    sw.innerHTML = '<svg viewBox="0 0 24 24" width="12" height="12"><path d="M12 3l9 9-9 9-9-9z" fill="' + (hasKeyHere ? 'currentColor' : 'none') + '" stroke="currentColor" stroke-width="2"/></svg>';
    sw.addEventListener('click', function (e) {
      e.stopPropagation(); pushUndo();
      if (!swOn) {
        toggleAnimated(holder, prop);
      } else if (hasKeyHere) {
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
    row.appendChild(sw);
    var nm = document.createElement('div'); nm.className = 'lnm'; nm.textContent = 'Vertex ' + (vi + 1);
    row.appendChild(nm);
    list.appendChild(row);
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
    var track = trackFor(ld, prop);
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
      // Frame-duration block behind each key (Van Dijk 3.3): the diamond is
      // centred on the frame's START, which at high zoom makes it hard to
      // see how much time one frame actually occupies — and whether a key
      // lines up with a layer's out point. Drawn only when a frame is wide
      // enough for the block to mean anything (his "closest three zoom
      // levels"), otherwise it degrades into a smear.
      if (FC >= 18) {
        track.keys.forEach(function (k) {
          var d = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
          d.setAttribute('x', k.frame * FC); d.setAttribute('y', 0);
          d.setAttribute('width', FC); d.setAttribute('height', ROW_H);
          d.setAttribute('fill', k.color || 'var(--accent)'); d.setAttribute('opacity', '0.13');
          svg.appendChild(d);
        });
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
        // Per-key colour (Van Dijk 3.4: "sometimes you have so many
        // keyframes it becomes difficult to know what does what — like
        // layers, we could color keyframes to highlight a group"). Only a
        // paint job: nothing reads k.color at evaluation time.
        if (k.color) { dia.classList.add('tinted'); dia.style.setProperty('--key-color', k.color); }
        // Velocity read-out (3.5): the ease actually applied out of this key,
        // as a percentage, shown on the SELECTED key instead of living in a
        // dialog. Derived from the same curvePoints the interpolator uses,
        // so it can never disagree with what the animation does.
        if (isKeySelected(ld, prop, k)) {
          var vel = easeOutPercent(k);
          if (vel != null) {
            var vl = document.createElement('span');
            vl.className = 'motion-key-vel';
            vl.textContent = vel + '%';
            dia.appendChild(vl);
          }
        }
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
            // convention below). Alt+drag instead SKEWS (bottom-edge
            // semantics: top row anchored, bottom row slides the full drag
            // distance, in-between rows interpolate — see startSkewDrag),
            // kept as a shortcut alongside the selection box's own edges.
            pushUndo();
            if (isKeySelected(ld, prop, key)) {
              var skewRows = e.altKey && _motionKeySel.length >= 2 ? buildKeyRows() : null;
              if (skewRows && skewRows.length >= 2) {
                window._motionSkewDrag = { startX: e.clientX, mode: 'bottom', rows: skewRows };
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
          var track = trackFor(ld, prop);
          var menu = [
            key
              ? { label: 'Supprimer cette clé', action: function () { pushUndo(); var tr = trackFor(ld, prop); tr.keys.splice(tr.keys.indexOf(key), 1); renderTimeline(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); } }
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
          // Interpolation presets, mirroring the keyboard layer added
          // alongside them (timeline.js onKeyDown) so neither is the only way
          // in — the keyboard is faster once known, the menu is how it gets
          // known. They act on the whole selection, so right-clicking a key
          // that isn't selected yet selects it first (the mousedown handler
          // above already did that by the time this fires).
          if (_motionKeySel.length) {
            menu.push({ sep: true });
            menu.push({ label: 'Easy Ease  (F9)', action: function () { applyEasyEase('ease'); } });
            menu.push({ label: 'Easy Ease In  (Maj+F9)', action: function () { applyEasyEase('easeIn'); } });
            menu.push({ label: 'Easy Ease Out  (Cmd+Maj+F9)', action: function () { applyEasyEase('easeOut'); } });
            menu.push({ label: 'Linéaire  (Cmd+Alt+K)', action: function () { setKeyInterp('linear'); } });
          }
          // Batch ops act on the WHOLE current multi-selection, offered
          // regardless of which key/cell was right-clicked. No Align here
          // (unlike layer bars) — a keyframe has no duration, "align"
          // doesn't map onto a single point.
          if (_motionKeySel.length >= 2) {
            menu.push({ sep: true });
            menu.push({ label: 'Distribuer uniformément', action: distributeKeys });
            menu.push({ label: 'Inverser l’ordre (flip)', action: flipKeys });
            menu.push({ label: 'Subdiviser (clé à mi-chemin)', action: subdivideKeys });
            menu.push({ label: 'Colorer les clés…', action: function () {
              var palette = ['#e8b64c', '#4ea9ff', '#59d38a', '#ff6b8b', '#b98cff', '#ffffff'];
              var names = ['Ambre', 'Bleu', 'Vert', 'Rose', 'Violet', 'Blanc'];
              window.showContextMenu(e.clientX + 8, e.clientY + 8,
                names.map(function (n, i) { return { label: n, action: function () { colorSelectedKeys(palette[i]); } }; })
                  .concat([{ sep: true }, { label: 'Retirer la couleur', action: function () { colorSelectedKeys(null); } }]));
            } });
            menu.push({ label: 'Sélectionner 1 sur 2', action: function () { selectEveryNthKey(2); } });
            menu.push({ label: 'Sélectionner 1 sur N…', action: function () {
              var v = prompt('Garder une clé sur combien ?', '3');
              if (v !== null) selectEveryNthKey(v);
            } });
            menu.push({ label: 'Garder au hasard…', action: function () {
              var v = prompt('Garder quel pourcentage de la sélection ?', '50');
              if (v !== null) grabRandomKeys(v);
            } });
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
    // Bug found live (2026-07 — "problème d'alignement de clé par rapport
    // aux properties"): renderTransformGroup (the #layer-list/#motion-props-
    // body panel) inserts an EXTRA row right here when this prop's ƒx
    // expression editor is open (buildExprEditorRow) — this grid side never
    // had an equivalent row, so every row below the open editor silently
    // shifted down by one relative to its own keyframe track, exactly the
    // "same alignment-invariant" class of bug ROW_H's header comment warns
    // about. The editor is a variable-height textarea (not a fixed ROW_H
    // row like everything else here), so instead of hardcoding a height,
    // read the ACTUAL rendered height off #layer-list's copy — it always
    // exists whenever this flag is set (renderTransformGroup renders it
    // into every container it's called for, panel AND mirror alike), and
    // renderLayerList() always runs before renderTimeline() in every call
    // site in this file, so it's guaranteed already up to date by now.
    if (window._exprEditorOpen && window._exprEditorOpen.holder === holder && window._exprEditorOpen.prop === prop) {
      var refRow = document.querySelector('#layer-list .motion-expr-editor');
      var spacer = document.createElement('div'); spacer.className = 'frow motion-track-spacer';
      spacer.style.height = (refRow ? refRow.getBoundingClientRect().height : 90) + 'px';
      grid.appendChild(spacer);
    }
  }
  // Selecting a layer from the GRID half of the timeline (2026-07-27:
  // "impossible de select un layer en clicquand de ce côté de la timeline").
  // Same selection the layer-list row click makes, MINUS the accordion
  // toggle: on the left the row IS the name, so expanding it on click is the
  // obvious reading; on the right the row is the layer's in/out bar, whose
  // own gesture is dragging timing — collapsing or expanding the whole
  // timeline under a click that was aimed at the bar is not what you meant.
  function selectLayerFromGrid(li) {
    if (state.appMode !== 'motion' || !state.layers[li]) return;
    _layerSel = [li];
    setKeySel([]);
    window.SM.setActiveLayer(li);
    renderLayerList(); renderTimeline();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }

  function renderTimelineMotion(grid) {
    var order = (typeof computeLayerRenderOrder === 'function') ? computeLayerRenderOrder() : state.layers.map(function (_l, i) { return { type: 'layer', idx: i }; });
    order.forEach(function (entry) {
      if (entry.type !== 'layer' || entry.hidden) return;
      var li = entry.idx, ld = state.layers[li];
      var expanded = isLayerExpanded(li);
      // SAME class string renderLayerListMotion puts on this layer's row —
      // the grid half was left plain, so a selected layer lit up on the left
      // and stayed dark on the right ("cette partie n'est pas hightlight
      // quand select") even though it is one row across both panels.
      var spacer = document.createElement('div');
      spacer.className = 'frow' + (_layerSel.indexOf(li) >= 0 ? ' act motion-selected' : '');
      spacer.dataset.layer = li;
      if (window.SMLayerInOut) SMLayerInOut.buildBar(spacer, li);
      grid.appendChild(spacer);
      if (!expanded) return;
      if (showsGroupHeader()) {
        var grpSpacer = document.createElement('div'); grpSpacer.className = 'frow';
        grid.appendChild(grpSpacer);
      }
      propsFor(ld).forEach(function (prop) { renderTracksFor(grid, ld, prop); });
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
          var elHolder = ensureElementHolder(ld, entry.strokeId);
          // Bug found live (2026-07 — "problème d'alignement de clé par
          // rapport aux properties"): renderElementsList calls
          // renderTransformGroup(list, elHolder, 'Transform (élément)') here,
          // which appends its OWN group-header row before looping PROPS —
          // this grid side went straight into PROPS.forEach with no matching
          // header spacer, so every row of THIS element (and everything
          // after it) was permanently off by one from its own keyframe
          // track. Confirmed via a single-snapshot DOM measurement: grid's
          // row at this position had class 'motion-track-row' (a real
          // track) while list's had 'motion-group-row' (a header) — same
          // "extra row on one side only" bug class as the Fill-row fix
          // above, just at the element level instead of the shape level.
          if (showsGroupHeader()) {
            var elGrpSpacer = document.createElement('div'); elGrpSpacer.className = 'frow';
            grid.appendChild(elGrpSpacer);
          }
          PROPS.forEach(function (prop) { renderTracksFor(grid, elHolder, prop); });
          // Path group (mirrors renderElementsList's renderPathVertexGroup
          // exactly — same expand condition, same vertex count, same
          // spacer-then-rows shape as the Transform group just above).
          if (!entry.sd.isRaster && entry.sd.segments && entry.sd.segments.length) {
            var pathHdrSpacer = document.createElement('div'); pathHdrSpacer.className = 'frow';
            grid.appendChild(pathHdrSpacer);
            if (isPathGroupExpanded(elHolder)) {
              for (var vi = 0; vi < entry.sd.segments.length; vi++) renderTracksFor(grid, elHolder, 'vtx' + vi);
            }
          }
          // Fill color (mirrors renderElementsList's renderFillColorRow call
          // exactly — same condition). Bug found live (2026-07 —
          // "problème d'alignement de clé par rapport aux properties"):
          // this call was missing entirely, so #layer-list had one MORE row
          // than #frame-grid for any shape with a fill — every row further
          // down (a later Forme's own Transform/Path/Fill rows) drifted out
          // of alignment with its own keyframe track from that point on.
          if (entry.sd.fillColor) renderTracksFor(grid, elHolder, 'fillColor');
        });
      }
    });
    // Re-measure the selection box against the freshly-rebuilt grid —
    // covers every path that changes _motionKeySel without going through
    // setKeySel (selectEveryNthKey/invertKeySelection assign it directly),
    // plus keeps the box tracking selected diamonds that just moved
    // horizontally (stagger drag calls renderTimeline() every tick).
    updateKeySelectionBox();
    updateLayerStaggerBox();
  }
  // ---- multi-select (marquee rectangle) + group drag ----
  // AE convention: drag a selection rectangle over the keyframe
  // grid to select several keys across one or more property tracks at
  // once (position/rotation/scale/... — layer OR element tracks, doesn't
  // matter which), then drag any ONE of the selected diamonds to retime
  // the whole group together by the same frame delta.
  var _motionKeySel = []; // [{holder, prop, key}]
  function isKeySelected(holder, prop, key) {
    return _motionKeySel.some(function (s) { return s.holder === holder && s.prop === prop && s.key === key; });
  }
  function setKeySel(sel) { _motionKeySel = sel; updateKeySelectionBox(); }
  // ---- batch operations on the current keyframe selection (Distribute/
  // Flip/Select Every/Invert Selection — no Align here, unlike layer bars:
  // a keyframe has no duration, "align" doesn't map onto a single point).
  // Grouped PER TRACK (holder+prop) first: two
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
      sortKeys(trackFor(g.holder, g.prop));
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
      sortKeys(trackFor(g.holder, g.prop));
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
  // Subdivide: insert a key HALFWAY between each consecutive pair of
  // selected keys on the same track (2026-07-25, Skew Pro's "Subdivide").
  // The inserted key takes the value the curve already produces at that
  // frame, so the animation is bit-for-bit unchanged the moment it lands —
  // it exists to give you a handle to grab, which is the whole point. The
  // new keys are added to the selection so a subdivide → drag → subdivide
  // loop works without re-selecting, and a pair only one frame apart is
  // skipped (no room for a key between them) rather than silently
  // overwriting one of its own endpoints.
  // How much ease leaves this key, as a percentage. 0% = linear out, 100% =
  // fully eased out. Read off the curve's FIRST span: with the on-curve
  // waypoint model used everywhere here (see DEFAULT_CURVE), a key that
  // leaves linearly has its first waypoint on the diagonal, and the further
  // that point sits below the diagonal the slower the start.
  function easeOutPercent(k) {
    var pts = k && k.curvePoints;
    if (!pts || pts.length < 2) return null;
    var p = pts[1];
    if (!p || !p.x) return 0;
    var lag = Math.max(0, Math.min(1, 1 - (p.y / p.x)));
    return Math.round(lag * 100);
  }
  function colorSelectedKeys(color) {
    if (!_motionKeySel.length) { if (window.showToast) showToast('Aucune clé sélectionnée'); return; }
    pushUndo();
    _motionKeySel.forEach(function (s) { if (color) s.key.color = color; else delete s.key.color; });
    renderTimeline();
    if (window.showToast) showToast(color ? (_motionKeySel.length + ' clé(s) colorée(s)') : 'Couleur retirée');
  }
  function subdivideKeys() {
    if (_motionKeySel.length < 2) { if (window.showToast) showToast('Sélectionne au moins 2 clés'); return; }
    pushUndo();
    var added = 0, skipped = 0, fresh = [];
    _groupKeySelByTrack().forEach(function (g) {
      if (g.items.length < 2) return;
      var track = trackFor(g.holder, g.prop);
      if (!track) return;
      var sorted = g.items.slice().sort(function (a, b) { return a.frame - b.frame; });
      for (var i = 0; i < sorted.length - 1; i++) {
        var a = sorted[i], b = sorted[i + 1];
        var mid = Math.round((a.frame + b.frame) / 2);
        if (mid <= a.frame || mid >= b.frame) { skipped++; continue; }
        if (keyAt(track, mid)) { skipped++; continue; }
        // Read the value BEFORE inserting — rawValueAtFrame walks this same
        // track, so inserting first would make the new key sample itself.
        var v = rawValueAtFrame(g.holder, g.prop, mid);
        var nk = { frame: mid, v: v.slice(), curvePoints: cloneCurvePts(a.curvePoints || DEFAULT_CURVE), hOut: [0, 0], hIn: [0, 0] };
        track.keys.push(nk); added++;
        fresh.push({ holder: g.holder, prop: g.prop, key: nk });
      }
      sortKeys(track);
    });
    setKeySel(_motionKeySel.concat(fresh));
    renderLayerList(); renderTimeline();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    if (window.showToast) showToast(added + ' clé(s) insérée(s)' + (skipped ? ' — ' + skipped + ' intervalle(s) trop court(s)' : ''));
  }
  // Keep a random subset of the current selection (Skew Pro's "Grab
  // Randomly"): the fast way to make a uniform batch of layers/keys feel
  // hand-made. Guarantees at least one key survives, so it can't silently
  // empty the selection on a small one.
  function grabRandomKeys(percent) {
    var p = Math.max(1, Math.min(100, parseInt(percent, 10) || 50)) / 100;
    if (!_motionKeySel.length) { if (window.showToast) showToast('Aucune clé sélectionnée'); return; }
    var pool = _motionKeySel.slice();
    var want = Math.max(1, Math.round(pool.length * p));
    var out = [];
    while (out.length < want && pool.length) out.push(pool.splice(Math.floor(Math.random() * pool.length), 1)[0]);
    setKeySel(out);
    renderTimeline();
    if (window.showToast) showToast(out.length + ' clé(s) gardée(s) au hasard');
  }
  // Inverts within whatever tracks are CURRENTLY RENDERED (the same
  // universe the marquee itself draws over).
  function invertKeySelection() {
    var all = [], prevSel = _motionKeySel;
    document.querySelectorAll('.motion-track-row').forEach(function (rowEl) {
      var holder = rowEl._smHolder, prop = rowEl._smProp;
      var track = trackFor(holder, prop);
      if (!track) return;
      track.keys.forEach(function (k) { all.push({ holder: holder, prop: prop, key: k }); });
    });
    _motionKeySel = all.filter(function (s) {
      return !prevSel.some(function (s2) { return s2.holder === s.holder && s2.prop === s.prop && s2.key === s.key; });
    });
    renderTimeline();
  }
  var _motionMarquee = null; // {startX, startY, rectEl, moved, layer}
  function startMarquee(e) {
    var rect = document.createElement('div'); rect.className = 'motion-marquee-rect';
    document.body.appendChild(rect);
    // Which layer's row the press landed on, if any — read HERE because by
    // mouseup the pointer has usually left it. null means empty grid space,
    // which is what endMarquee treats as "deselect".
    var rowEl = e.target && e.target.closest && e.target.closest('.frow[data-layer]');
    var li = rowEl ? parseInt(rowEl.dataset.layer, 10) : NaN;
    _motionMarquee = { startX: e.clientX, startY: e.clientY, rectEl: rect, moved: false, layer: isNaN(li) ? null : li };
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
        var track = trackFor(holder, prop);
        var key = track ? keyAt(track, frame) : null;
        if (key) sel.push({ holder: holder, prop: prop, key: key });
      });
    });
    setKeySel(sel); // also refreshes the selection box (updateKeySelectionBox below)
  }
  // ---- visible selection box: skew edges + move fill (2026-07) ----
  // Feedback history on this ONE feature: a hidden Alt+drag gesture wasn't
  // discoverable; a small centered handle was unclickable AND wrong; full-
  // width edges with a rank-cascade were closer but still not the
  // reference's actual SKEW semantics (screenshots provided): dragging the
  // TOP edge slides the top row by the full drag distance while the BOTTOM
  // row stays anchored (and vice versa), intermediate rows interpolating
  // linearly — the keys form the diagonal seen in the screenshots. The
  // interior is a hand-cursor surface that moves everything uniformly.
  // All three gestures now go through ONE skew-drag engine (fraction per
  // row: move=1 everywhere, top=(n-1-r)/(n-1), bottom=r/(n-1)) computing
  // absolute per-row offsets from each key's ORIGINAL frame at mousedown —
  // no incremental re-baselining, so fractional rows can't drift.
  function addStaggerEdges(boxEl, onStart) {
    ['top', 'bottom'].forEach(function (pos) {
      var edge = document.createElement('div');
      edge.className = 'motion-keysel-edge motion-keysel-edge-' + pos;
      edge.title = 'Glisser horizontalement pour skewer les clés (ce bord bouge, l\'autre reste ancré)';
      edge.addEventListener('mousedown', function (e) { e.stopPropagation(); e.preventDefault(); onStart(e, pos); });
      boxEl.appendChild(edge);
    });
  }
  // Side edges = "Space": drag one to spread the selection out along the
  // timeline (or squeeze it in), the opposite edge staying put. Distinct
  // from the top/bottom skew edges, which slide whole ROWS; this one
  // rescales the selection's own timing. Needs 2+ distinct frames to mean
  // anything, hence the guard in the caller.
  function addSpaceEdges(boxEl, onStart) {
    ['left', 'right'].forEach(function (pos) {
      var edge = document.createElement('div');
      edge.className = 'motion-keysel-edge motion-keysel-edge-' + pos;
      edge.title = 'Glisser pour espacer / resserrer les clés dans le temps (le bord opposé reste ancré)';
      edge.addEventListener('mousedown', function (e) { e.stopPropagation(); e.preventDefault(); onStart(e, pos); });
      boxEl.appendChild(edge);
    });
  }
  function addMoveFill(boxEl, onStart) {
    var fill = document.createElement('div');
    fill.className = 'motion-keysel-fill';
    fill.title = 'Glisser pour déplacer toutes les clés sélectionnées ensemble'
      + '\n⌘/Ctrl + glisser : liquify — les clés proches du curseur suivent plus que les lointaines';
    fill.addEventListener('mousedown', function (e) {
      e.stopPropagation(); e.preventDefault();
      // Skew Pro's "Liquify": same drag, but the influence falls off with
      // distance from where you grabbed, so a block of keys deforms instead
      // of sliding rigidly. Cmd/Ctrl picks it because the plain drag (move)
      // and both edge drags (skew, space) already own the unmodified
      // gestures.
      onStart(e, (e.metaKey || e.ctrlKey) ? 'liquify' : 'move');
    });
    boxEl.appendChild(fill);
  }
  // rows: array (ordered top→bottom visually) of arrays of {track, key,
  // orig}. mode: 'move' | 'top' | 'bottom'.
  function startSkewDrag(rows, mode, e) {
    rows = rows.filter(function (r) { return r.length || true; });
    if (!rows.length) return;
    pushUndo();
    // Selection extent along TIME, captured once at mousedown — the Space
    // gestures (left/right edges) need it to place each key between the
    // anchored edge and the dragged one, and recomputing it mid-drag would
    // move the anchor under the cursor as the keys spread.
    var fMin = Infinity, fMax = -Infinity;
    rows.forEach(function (row) { row.forEach(function (en) { fMin = Math.min(fMin, en.orig); fMax = Math.max(fMax, en.orig); }); });
    // Liquify needs to know WHERE along the timeline you grabbed — the
    // falloff is centred there, not on the selection's middle.
    var grid = document.getElementById('frame-grid');
    var grabFrame = grid ? ((e.clientX - grid.getBoundingClientRect().left) / FC) : (fMin + fMax) / 2;
    window._motionSkewDrag = { startX: e.clientX, mode: mode, rows: rows, fMin: fMin, fMax: fMax, grabFrame: grabFrame };
  }
  // Key box rows = one row per PROPERTY TRACK holding selected keys, in
  // rendered (document) order — matches the reference where each visible
  // track row skews as its own step of the diagonal.
  function buildKeyRows() {
    var rows = [];
    document.querySelectorAll('#frame-grid .motion-track-row').forEach(function (rowEl) {
      var holder = rowEl._smHolder, prop = rowEl._smProp;
      var bkTrack = trackFor(holder, prop);
      if (!bkTrack) return;
      var entries = [];
      _motionKeySel.forEach(function (s) {
        if (s.holder === holder && s.prop === prop) entries.push({ track: bkTrack, key: s.key, orig: s.key.frame });
      });
      if (entries.length) rows.push(entries);
    });
    return rows;
  }
  var _keySelBoxEl = null;
  function removeKeySelectionBox() {
    if (_keySelBoxEl) { _keySelBoxEl.remove(); _keySelBoxEl = null; }
  }
  function updateKeySelectionBox() {
    // A direct multi-layer selection (see below) takes priority — no point
    // showing both boxes at once, and the layer-level one is the more
    // direct gesture per the reference ("select layers, drag").
    if (_layerSel.length >= 2) { removeKeySelectionBox(); return; }
    var dias = Array.from(document.querySelectorAll('#frame-grid .motion-key.sel'));
    if (dias.length < 2) { removeKeySelectionBox(); return; }
    // The box only earns its place on keys stacked across SEVERAL property
    // tracks (2026-07-25: "quand on select sur une seule propriété des
    // keyframes pas besoin de la box c'est pour plusieurs keyframes les
    // unes en dessous des autres"). Its whole point is the cross-row
    // gestures — skew is literally undefined on one row (its own handler
    // refuses under 2 rows), so on a single property the box was pure
    // overlay: it covered the keys, hid the diamonds it was drawn around,
    // and intercepted clicks meant for them. Keys on one track are already
    // draggable as a group by grabbing any one of them.
    var rowsHit = [];
    dias.forEach(function (d) {
      var row = d.closest('.motion-track-row');
      if (row && rowsHit.indexOf(row) < 0) rowsHit.push(row);
    });
    if (rowsHit.length < 2) { removeKeySelectionBox(); return; }
    var x0 = Infinity, x1 = -Infinity, y0 = Infinity, y1 = -Infinity;
    dias.forEach(function (d) {
      var b = d.getBoundingClientRect();
      x0 = Math.min(x0, b.left); x1 = Math.max(x1, b.right);
      y0 = Math.min(y0, b.top); y1 = Math.max(y1, b.bottom);
    });
    if (!_keySelBoxEl) {
      _keySelBoxEl = document.createElement('div'); _keySelBoxEl.className = 'motion-keysel-box';
      document.body.appendChild(_keySelBoxEl);
      // The box's own surfaces (fill + edges) are pointer-events:auto so
      // they can be dragged — which also means they swallow right-clicks on
      // the keys UNDERNEATH, and every batch op (Distribuer, Flip,
      // Subdiviser, Easy Ease…) lives in that cell context menu. Found
      // 2026-07-25: as soon as the box appeared, the menu became
      // unreachable for exactly the selection it was meant to act on.
      // Forward instead of duplicating the menu: blank out the box for one
      // hit-test and re-dispatch to whatever is really under the cursor.
      _keySelBoxEl.addEventListener('contextmenu', function (e) {
        var prev = _keySelBoxEl.style.pointerEvents;
        _keySelBoxEl.style.pointerEvents = 'none';
        var kids = Array.prototype.slice.call(_keySelBoxEl.children);
        var prevKids = kids.map(function (c) { var p = c.style.pointerEvents; c.style.pointerEvents = 'none'; return p; });
        var under = document.elementFromPoint(e.clientX, e.clientY);
        _keySelBoxEl.style.pointerEvents = prev;
        kids.forEach(function (c, i) { c.style.pointerEvents = prevKids[i]; });
        if (!under) return;
        e.preventDefault(); e.stopPropagation();
        under.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: e.clientX, clientY: e.clientY }));
      });
      addMoveFill(_keySelBoxEl, function (e, mode) { startSkewDrag(buildKeyRows(), mode, e); });
      addSpaceEdges(_keySelBoxEl, function (e, mode) {
        var rows = buildKeyRows();
        var f0 = Infinity, f1 = -Infinity;
        rows.forEach(function (r) { r.forEach(function (en) { f0 = Math.min(f0, en.orig); f1 = Math.max(f1, en.orig); }); });
        if (!(f1 > f0)) { if (window.showToast) showToast('Sélectionne des clés sur 2 frames différentes pour les espacer'); return; }
        startSkewDrag(rows, mode, e);
      });
      addStaggerEdges(_keySelBoxEl, function (e, mode) {
        var rows = buildKeyRows();
        if (rows.length < 2) { if (window.showToast) showToast('Sélectionne des clés sur 2 pistes ou plus pour skewer'); return; }
        startSkewDrag(rows, mode, e);
      });
    }
    _keySelBoxEl.style.left = x0 + 'px'; _keySelBoxEl.style.top = y0 + 'px';
    _keySelBoxEl.style.width = (x1 - x0) + 'px'; _keySelBoxEl.style.height = (y1 - y0) + 'px';
  }
  // ---- layer-level stagger (2026-07) ----
  // Feedback: "je vois pas trop ce que ça fait ton truc pas aussi intuitif
  // que skew" + a reference GIF showing the real gesture — select whole
  // LAYERS (Cmd/Shift-click on the rows, wired above), no need to expand
  // any Transform group or marquee individual keyframe diamonds first. The
  // handle then staggers EVERY animated property's EVERY key for each
  // selected layer together (not just one property/track), matching the
  // reference where a layer's whole keyframe set shifts as one block.
  var _layerStaggerBoxEl = null;
  function removeLayerStaggerBox() {
    if (_layerStaggerBoxEl) { _layerStaggerBoxEl.remove(); _layerStaggerBoxEl = null; }
  }
  // Layer box rows = one row per selected LAYER, ordered by ACTUAL
  // on-screen vertical position of each layer's timeline row (not by
  // state.layers index — the layer list renders newest-on-top, so index
  // order is visually REVERSED; 'top'/'bottom' edge semantics must match
  // what the user sees, same reason buildKeyRows uses document order).
  // Each row carries EVERY key of EVERY animated property of that layer —
  // the whole layer skews as one step. Layers with no Motion keys still
  // occupy a row so the interpolation fractions match the visual spacing.
  function buildLayerRows() {
    var order = _layerSel.slice().sort(function (a, b) {
      var ra = document.querySelector('#frame-grid .frow[data-layer="' + a + '"]');
      var rb = document.querySelector('#frame-grid .frow[data-layer="' + b + '"]');
      if (!ra || !rb) return a - b;
      return ra.getBoundingClientRect().top - rb.getBoundingClientRect().top;
    });
    var rows = [];
    order.forEach(function (li) {
      var h = state.layers[li]; if (!h) return;
      var entries = [];
      propsFor(h).forEach(function (prop) {
        var track = trackFor(h, prop); if (!track) return;
        track.keys.forEach(function (k) { entries.push({ track: track, key: k, orig: k.frame }); });
      });
      rows.push(entries);
    });
    return rows;
  }
  // Hands the gesture to layer-inout.js's existing group-bar drag instead of
  // teaching the skew engine a second entry type: re-dispatching the press on
  // the first selected layer's own bar means this is literally the same code
  // path as marquee-selecting those bars and dragging one, so clamping,
  // keyframe carry-along, time-link reconciliation and undo all behave
  // identically rather than being reimplemented here and drifting.
  // Skew and Space genuinely need keys — there is no bar equivalent of
  // "spread these apart in time by their own frames". They used to return
  // silently, which reads exactly like the tool being broken; say why.
  function noKeysGuard(rows) {
    if (rows.some(function (r) { return r.length; })) return true;
    if (window.showToast) showToast('Aucune clé sur ces calques — pose des clés, ou glisse le centre de la boîte pour décaler les calques dans le temps');
    return false;
  }

  function dragLayerBars(e) {
    if (!window.SMLayerInOut || !SMLayerInOut.setBarSelection || !_layerSel.length) return;
    SMLayerInOut.setBarSelection(_layerSel.map(function (li) { return { li: li, part: 'both' }; }));
    var bar = document.querySelector('#frame-grid .frow[data-layer="' + _layerSel[0] + '"] .layer-inout-bar');
    if (!bar) return;
    bar.dispatchEvent(new MouseEvent('mousedown', {
      bubbles: true, cancelable: true, button: 0, clientX: e.clientX, clientY: e.clientY,
    }));
  }

  // ---- layer skew (box top/bottom edge) ----
  // Absolute from a start-of-gesture snapshot, never incremental: a drag
  // ticks dozens of times and rounding each tick's own delta to whole frames
  // would accumulate, while re-deriving every position from the snapshot
  // cannot drift — the same reason startSkewDrag works this way for keys.
  var _layerSkew = null;
  function startLayerSkewDrag(mode, e) {
    // Visual top-to-bottom order, not state.layers order: the list renders
    // newest-first, so index order is upside down and 'top'/'bottom' would
    // anchor the wrong end (buildLayerRows sorts the same way, for the same
    // reason).
    var order = _layerSel.slice().sort(function (a, b) {
      var ra = document.querySelector('#frame-grid .frow[data-layer="' + a + '"]');
      var rb = document.querySelector('#frame-grid .frow[data-layer="' + b + '"]');
      if (!ra || !rb) return a - b;
      return ra.getBoundingClientRect().top - rb.getBoundingClientRect().top;
    });
    if (window.pushUndo) pushUndo();
    _layerSkew = {
      mode: mode, startX: e.clientX, applied: {},
      rows: order.map(function (li) {
        var ld = state.layers[li];
        return { li: li, origIn: SMLayerInOut.inPointOf(ld), origOut: SMLayerInOut.outPointOf(ld) };
      }),
    };
  }
  function onLayerSkewMove(e) {
    if (!_layerSkew) return false;
    var total = (e.clientX - _layerSkew.startX) / FC;
    var n = _layerSkew.rows.length;
    _layerSkew.rows.forEach(function (r, i) {
      var f = n < 2 ? 1 : (_layerSkew.mode === 'top' ? (n - 1 - i) / (n - 1) : i / (n - 1));
      var dx = Math.round(total * f);
      var w = r.origOut - r.origIn;
      var ni = Math.max(0, r.origIn + dx);
      var ld = state.layers[r.li];
      ld.inPoint = ni; ld.outPoint = ni + w;
      _layerSkew.applied[r.li] = ni - r.origIn;
    });
    renderTimeline();
    return true;
  }
  function onLayerSkewUp() {
    if (!_layerSkew) return false;
    // Content and property keys follow ONCE, at drop — same as a bar drag,
    // which deliberately doesn't re-shift ld.frames on every mousemove.
    var plan = Object.keys(_layerSkew.applied).map(function (li) {
      return { li: +li, dx: _layerSkew.applied[li] };
    });
    _layerSkew = null;
    if (window.SMLayerInOut && SMLayerInOut.retimeLayers) SMLayerInOut.retimeLayers(plan);
    renderTimeline(); renderLayerList();
    if (window.loadFrame) loadFrame(state.currentFrame);
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    return true;
  }

  function updateLayerStaggerBox() {
    if (state.appMode !== 'motion' || _layerSel.length < 2) { removeLayerStaggerBox(); return; }
    var spacers = _layerSel.map(function (li) { return document.querySelector('#frame-grid .frow[data-layer="' + li + '"]'); }).filter(Boolean);
    if (spacers.length < 2) { removeLayerStaggerBox(); return; }
    var gridEl = document.getElementById('frame-grid'); if (!gridEl) { removeLayerStaggerBox(); return; }
    var y0 = Infinity, y1 = -Infinity;
    spacers.forEach(function (s) { var b = s.getBoundingClientRect(); y0 = Math.min(y0, b.top); y1 = Math.max(y1, b.bottom); });
    // Full grid width, not just a one-frame-wide sliver at the playhead —
    // "j'ai juste à glisser la box... peu importe où je suis" — the whole
    // top/bottom edge (addStaggerEdges) needs a genuinely wide box to grab
    // along, not a narrow strip that's itself hard to land on.
    // Clamped to what is actually ON SCREEN. The box used to span the grid's
    // whole CONTENT (measured: 3600px against a 979px viewport), so most of
    // it — and most of its skew edges — sat thousands of pixels past the
    // right edge where no pointer can reach, and the two Space edges had to
    // be re-pinned by hand further down to compensate. The visible rect is
    // the only part a gesture can ever start in.
    var gbFull = gridEl.getBoundingClientRect();
    var wrapEl2 = document.getElementById('fg-wrap');
    var wr2 = wrapEl2 ? wrapEl2.getBoundingClientRect() : gbFull;
    var gbLeft = Math.max(gbFull.left, wr2.left);
    var gbRight = Math.min(gbFull.right, wr2.right);
    var gb = { left: gbLeft, width: Math.max(0, gbRight - gbLeft) };
    if (!_layerStaggerBoxEl) {
      _layerStaggerBoxEl = document.createElement('div'); _layerStaggerBoxEl.className = 'motion-keysel-box';
      document.body.appendChild(_layerStaggerBoxEl);
      // The fill covers LAYER rows, so dragging it moves the LAYERS in time —
      // bars and their keys together, which is what AE does when you drag
      // selected layers in its timeline (2026-07-27: "la box d'alignement…
      // ne permet pas encore de bien déplacé les calques dans le temps").
      // It used to move keys ALONE whenever the selection had any, which is
      // a different operation living in a different place: dragging keys.
      // Nothing is lost — the key box (two or more property tracks selected)
      // still owns move/skew/space/liquify on keys, and the edges of THIS box
      // still skew and space them. Routing through layer-inout.js's group bar
      // drag also means clamping, keyframe carry-along and undo behave
      // exactly as they do when you drag one of those bars by hand.
      addMoveFill(_layerStaggerBoxEl, function (e) { dragLayerBars(e); });
      // Space on the LAYER box, which is where the reference actually
      // demonstrates it ("select layers across multiple rows, drag from the
      // right edge to space them out"): spreads the selected layers' keys
      // apart in time with the opposite edge anchored. Same per-key factor
      // as the key box — a layer's keys sit at their own frames, so a layer
      // early in the span moves little and a late one moves a lot, which is
      // exactly the spread.
      addSpaceEdges(_layerStaggerBoxEl, function (e, mode) {
        var rows = buildLayerRows();
        if (!noKeysGuard(rows)) return;
        var f0 = Infinity, f1 = -Infinity;
        rows.forEach(function (r) { r.forEach(function (en) { f0 = Math.min(f0, en.orig); f1 = Math.max(f1, en.orig); }); });
        if (!(f1 > f0)) { if (window.showToast) showToast('Il faut des clés sur au moins 2 frames différentes pour les espacer'); return; }
        startSkewDrag(rows, mode, e);
      });
      // Top/bottom edge on a LAYER selection staggers the LAYERS in time —
      // the counterpart of what the same edge does to keys, and what the box
      // sitting over layer rows implies (2026-07-27: "le haut et le bas de la
      // box sur une selection de calque ne les déplace actuellement pas comme
      // ça peut déplacé des clé en skew"). Same diagonal as the key skew: the
      // dragged edge's row travels the full distance, the opposite row stays
      // anchored, the rows between interpolate.
      addStaggerEdges(_layerStaggerBoxEl, function (e, mode) {
        if (_layerSel.length < 2) return;
        startLayerSkewDrag(mode, e);
      });
    }
    _layerStaggerBoxEl.style.left = gb.left + 'px'; _layerStaggerBoxEl.style.top = y0 + 'px';
    _layerStaggerBoxEl.style.width = gb.width + 'px'; _layerStaggerBoxEl.style.height = (y1 - y0) + 'px';
    // The box deliberately spans the FULL grid width so the skew edges can
    // be grabbed anywhere along the row ("j'ai juste à glisser la box…
    // peu importe où je suis"). That puts its own left/right extremities
    // thousands of pixels off-screen, which would make the Space handles
    // unreachable in practice (measured: right edge at x≈2926 on an 800px
    // view). So pin them to the selection's KEY SPAN instead of the box's
    // geometry — which is also where they belong conceptually: the handles
    // sit exactly where the content starts and ends, like the key box's do.
    var el = _layerStaggerBoxEl.querySelector('.motion-keysel-edge-left');
    var er = _layerStaggerBoxEl.querySelector('.motion-keysel-edge-right');
    if (el && er) {
      var sf0 = Infinity, sf1 = -Infinity;
      _layerSel.forEach(function (li) {
        var h = state.layers[li];
        if (!h) return;
        propsFor(h).forEach(function (prop) {
          var t = trackFor(h, prop); if (!t) return;
          t.keys.forEach(function (k) { sf0 = Math.min(sf0, k.frame); sf1 = Math.max(sf1, k.frame); });
        });
      });
      if (sf1 > sf0) {
        el.style.display = ''; er.style.display = '';
        el.style.left = (sf0 * FC - 7) + 'px'; el.style.right = 'auto';
        er.style.left = (sf1 * FC + FC - 7) + 'px'; er.style.right = 'auto';
      } else {
        // One frame (or no keys at all) — nothing to spread, and a handle
        // that silently does nothing is worse than no handle.
        el.style.display = 'none'; er.style.display = 'none';
      }
    }
  }
  // ---- TIME REMAP (2026-07-25) ----------------------------------------
  // AE's Time Remapping, for component-instance layers: instead of the
  // instance playing at a fixed speed/loop mode, a keyframed curve says
  // WHICH internal frame to show at each main-timeline frame. Freeze, hold,
  // reverse, ramp and ping-pong all fall out of the same curve, with the
  // existing easing/graph editor working on it unchanged because it is a
  // normal motion track (same shape as position/scale/...).
  //
  // Enabling seeds two keys — internal frame 0 at the layer's in point,
  // last internal frame at its out point — which is exactly AE's behaviour
  // and, crucially, means turning it on changes nothing on screen: the
  // linear ramp between those two keys reproduces the default playback.
  function enableTimeRemap(li) {
    var ld = state.layers[li];
    if (!ld) return false;
    if (!ld.symbolId) { if (window.showToast) showToast('Le remappage temporel s\u2019applique aux calques composants'); return false; }
    var sym = state.symbols[ld.symbolId];
    if (!sym) return false;
    if (ld.timeRemap) { if (window.showToast) showToast('Remappage temporel déjà actif'); return false; }
    pushUndo();
    var inF = window.layerInPoint ? layerInPoint(ld) : (ld.inPoint != null ? ld.inPoint : 0);
    var outF = window.layerOutPoint ? layerOutPoint(ld) : (ld.outPoint != null ? ld.outPoint : state.totalFrames - 1);
    var last = Math.max(0, (sym.totalFrames || 1) - 1);
    ld.timeRemap = { keys: [
      { frame: inF, v: [0], curvePoints: cloneCurvePts(CURVE_LINEAR), hOut: [0, 0], hIn: [0, 0] },
      { frame: Math.max(inF + 1, outF), v: [last], curvePoints: cloneCurvePts(CURVE_LINEAR), hOut: [0, 0], hIn: [0, 0] },
    ] };
    renderLayerList(); renderTimeline();
    if (window.loadFrame) loadFrame(state.currentFrame);
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    if (window.showToast) showToast('Remappage temporel activé — 0 → ' + last);
    return true;
  }
  function disableTimeRemap(li) {
    var ld = state.layers[li];
    if (!ld || !ld.timeRemap) return false;
    pushUndo();
    delete ld.timeRemap;
    renderLayerList(); renderTimeline();
    if (window.loadFrame) loadFrame(state.currentFrame);
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    if (window.showToast) showToast('Remappage temporel désactivé');
    return true;
  }
  // The internal frame this instance should show at `frame`, or null when
  // the layer isn't remapped. app.js's resolveSymbolFrameIdx is the single
  // consumer — one chokepoint, so nothing else can disagree about it.
  function timeRemapValue(ld, frame) {
    var t = ld && ld.timeRemap;
    if (!t || !t.keys || !t.keys.length) return null;
    return evalTrack(t, frame, 0);
  }
  // Retime every keyframe a layer owns by dx frames. The counterpart to
  // SM.shiftLayerFrames (timeline.js), which moves only ld.frames — the
  // DRAWN content. Found 2026-07-25 while testing the in/out handles: a bar
  // drag in Motion moved the artwork and the visibility window but left the
  // Position/Rotation/Scale/Opacity keys exactly where they were, so the
  // layer arrived somewhere new still animating on the old schedule. In
  // Motion "the keyframes" means these, so "déplacer le calque avec ses
  // keyframes" was only ever half true.
  //
  // Covers all three holders a layer can carry keys in — its own motion,
  // each per-element holder, and each effect's parameter tracks (the
  // effects panel keeps them in eff.keys[param].keys, see
  // effects-panel.js's ensureParamTrack). Missing any one of them is the
  // CLAUDE.md §1 shape of bug: retimed in one reader, stale in the others.
  function shiftLayerMotionKeys(li, dx) {
    var ld = state.layers[li];
    if (!ld || !dx) return false;
    var total = state.totalFrames, touched = false;
    // A key pushed past either end is CLAMPED, not dropped (dropping would
    // silently destroy animation the user can't see going). Clamping can
    // land two keys on the same frame, so collapse exact duplicates
    // afterwards, keeping the earliest-listed one.
    function shiftTrack(t) {
      if (!t || !t.keys || !t.keys.length) return;
      t.keys.forEach(function (k) { k.frame = Math.max(0, Math.min(total - 1, k.frame + dx)); });
      t.keys.sort(function (a, b) { return a.frame - b.frame; });
      for (var i = t.keys.length - 1; i > 0; i--) if (t.keys[i].frame === t.keys[i - 1].frame) t.keys.splice(i, 1);
      touched = true;
    }
    function shiftHolder(h) {
      if (!h || !h.motion) return;
      PROPS.forEach(function (prop) { shiftTrack(h.motion[prop]); });
    }
    shiftHolder(ld);
    // timeRemap lives OUTSIDE ld.motion (see trackFor) — without this, a
    // bar drag on a remapped component layer retimed the drawings and every
    // transform key but left the remap curve at the old frames (the exact
    // §1 shape this function's own header comment warns about).
    shiftTrack(ld.timeRemap);
    if (ld.elementMotion) Object.keys(ld.elementMotion).forEach(function (id) { shiftHolder(ld.elementMotion[id]); });
    if (ld.effects) ld.effects.forEach(function (eff) {
      if (eff && eff.keys) Object.keys(eff.keys).forEach(function (p) { shiftTrack(eff.keys[p]); });
    });
    return touched;
  }
  // Right-click alternative to dragging the handle — same rank-0-anchor,
  // every-property-together semantics, applied instantly for a typed step
  // instead of a live drag. Ordered by layer index (same as the handle's
  // own mousedown), so both routes to the same result agree.
  function staggerSelectedLayers(step) {
    var order = _layerSel.slice().sort(function (a, b) { return a - b; });
    var holders = order.map(function (li) { return state.layers[li]; }).filter(Boolean);
    if (holders.length < 2) return;
    pushUndo();
    holders.forEach(function (h, rank) {
      if (rank === 0) return;
      propsFor(h).forEach(function (prop) {
        var track = trackFor(h, prop); if (!track) return;
        track.keys.forEach(function (k) { k.frame = Math.max(0, Math.min(state.totalFrames - 1, k.frame + step * rank)); });
        sortKeys(track);
      });
    });
    renderLayerList(); renderTimeline();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }
  function updateMarquee(e) {
    if (!_motionMarquee) return;
    var dx = e.clientX - _motionMarquee.startX, dy = e.clientY - _motionMarquee.startY;
    if (Math.abs(dx) > 3 || Math.abs(dy) > 3) _motionMarquee.moved = true;
    var x0 = Math.min(_motionMarquee.startX, e.clientX), y0 = Math.min(_motionMarquee.startY, e.clientY);
    var w = Math.abs(dx), h = Math.abs(dy);
    var r = _motionMarquee.rectEl;
    r.style.left = x0 + 'px'; r.style.top = y0 + 'px'; r.style.width = w + 'px'; r.style.height = h + 'px';
    if (_motionMarquee.moved) {
      applyMarqueeSelection(x0, y0, x0 + w, y0 + h);
      // ...and the layer BARS in the same sweep. This marquee is registered
      // on #fg-wrap in the CAPTURE phase and stops propagation, so it fires
      // before layer-inout.js's own row/wrap listeners can start their bar
      // marquee — which meant in/out point selection was simply dead in
      // Motion, the only mode where the bars exist at all (verified live
      // 2026-07-25: a drag across three bars' left halves left
      // getBarSelection() empty). layer-inout.js already forwards to
      // SMMotion.marqueeSelect in the other direction; this is the missing
      // half, so one rectangle now picks up keys AND in/out points together.
      if (window.SMLayerInOut && SMLayerInOut.marqueeSelect) SMLayerInOut.marqueeSelect(x0, y0, x0 + w, y0 + h);
    }
  }
  function endMarquee() {
    if (!_motionMarquee) return;
    var moved = _motionMarquee.moved;
    var downLayer = _motionMarquee.layer;
    _motionMarquee.rectEl.remove();
    _motionMarquee = null;
    // A plain click that landed ON a layer's own grid row SELECTS that layer
    // rather than clearing — the grid half of a row is still that row.
    if (!moved && downLayer != null) { selectLayerFromGrid(downLayer); return; }
    // A plain click on empty grid space (no drag) clears the selection,
    // same "click empty = deselect" convention as the canvas's own
    // marquee/selection tools elsewhere in this app.
    // A plain click on empty grid space clears the BAR selection too, not
    // just the key one — they are now made by the same gesture, so leaving
    // one of them behind would be the same desync in reverse.
    if (!moved) {
      setKeySel([]);
      if (window.SMLayerInOut && SMLayerInOut.clearSelection) SMLayerInOut.clearSelection();
      // ...and the LAYER selection — the third thing this gesture owns, and
      // the one it was leaving behind (2026-07-27: "dans la timeline si on
      // clic à côté ou y a pas de calque il faut aussi pouvoir deselect").
      // #layer-list's empty area already dropped it, so the same click
      // deselected or not depending on which half of the timeline it landed
      // in. renderLayerList is the left-hand half; renderTimeline below only
      // repaints the grid.
      var droppedLayers = _layerSel.length > 0;
      if (droppedLayers) _layerSel = [];
      renderTimeline();
      if (droppedLayers) renderLayerList();
    }
  }

  // Drag-to-retime a keyframe (mousemove/up delegated from ui.js's global
  // pointer handlers via SMMotion.onDragMove/onDragUp, same pattern as the
  // span-end/keyframe drag handlers already in timeline.js).
  function onDragMove(e) {
    if (onLayerSkewMove(e)) return;
    updateMarquee(e);
    var sk = window._motionSkewDrag;
    if (sk) {
      // One engine for all three box gestures (see startSkewDrag's header):
      // per-row fraction × TOTAL drag distance, applied to each key's
      // ORIGINAL frame captured at mousedown — absolute, not incremental,
      // so fractional middle rows land exactly on round(fraction × total)
      // with zero drift, and dragging back to the start restores every key
      // to its exact original frame.
      var total = (e.clientX - sk.startX) / FC;
      var n = sk.rows.length;
      var dragged = [];
      sk.rows.forEach(function (row) { row.forEach(function (en) { dragged.push(en.key); }); });
      var plan = [];
      var ok3 = sk.rows.every(function (row, r) {
        // Row-based factor for the SKEW gestures (top/bottom edges): the
        // dragged edge's row moves the full distance, the opposite row
        // stays anchored, rows in between interpolate — the diagonal.
        var rowF = sk.mode === 'move' || n < 2 ? 1
          : (sk.mode === 'top' ? (n - 1 - r) / (n - 1) : r / (n - 1));
        return row.every(function (en) {
          // ...and a TIME-based factor for the SPACE gestures (left/right
          // edges, 2026-07-25, Skew Pro's "Space" lesson): spread or
          // compress the selection along the timeline with the opposite
          // edge anchored. The factor has to be per-KEY here, not per-row —
          // it depends on where the key sits between the selection's first
          // and last frame, not on which track it lives in. Same absolute-
          // from-original arithmetic as the skew, so it inherits the
          // no-drift and drag-back-to-restore properties unchanged.
          var f = rowF;
          if (sk.mode === 'liquify') {
            // Gaussian falloff around the grab point. Radius scales with the
            // selection's own span so the same gesture feels the same on a
            // 10-frame and a 200-frame selection; a single-frame selection
            // falls back to a fixed radius rather than dividing by zero.
            var span2 = Math.max(1, sk.fMax - sk.fMin);
            var radius = Math.max(2, span2 * 0.35);
            var dist = (en.orig - sk.grabFrame) / radius;
            f = Math.exp(-dist * dist);
          } else if (sk.mode === 'left' || sk.mode === 'right') {
            var span = sk.fMax - sk.fMin;
            f = span <= 0 ? 0 // every key on one frame — nothing to spread
              : (sk.mode === 'right' ? (en.orig - sk.fMin) / span : (sk.fMax - en.orig) / span);
          }
          var nf = en.orig + Math.round(total * f);
          if (nf < 0 || nf >= state.totalFrames) return false;
          var existing = keyAt(en.track, nf);
          // Only an UNdragged key blocks — another dragged key sitting at
          // nf mid-drag isn't a real collision (it's about to move too).
          if (existing && existing !== en.key && dragged.indexOf(existing) < 0) return false;
          plan.push({ key: en.key, track: en.track, nf: nf });
          return true;
        });
      });
      if (!ok3) return;
      var touched = [];
      plan.forEach(function (p) {
        p.key.frame = p.nf;
        if (touched.indexOf(p.track) < 0) touched.push(p.track);
      });
      touched.forEach(sortKeys);
      renderTimeline();
      // Live stage feedback (2026-07: "un component ne bouge pas en temps
      // réel quand on drag" — the timeline diamond tracked the cursor via
      // renderTimeline() above, but the interpolated value AT THE CURRENT
      // PLAYHEAD only changed on mouseup, since nothing re-rendered the
      // stage until onDragUp's own renderNow(). Every sibling drag
      // (onDrag's canvas gizmo, layer-inout.js's bar drag) already
      // re-renders on every move — this was the one gap.
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
      return;
    }
    var d = window._motionKeyDrag; if (!d) return;
    var deltaFrames = Math.round((e.clientX - d.startX) / FC);
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
        var existing = keyAt(trackFor(s.holder, s.prop), nf);
        return !existing || existing === s.key;
      });
      if (!ok) return;
      d.keys.forEach(function (s) { s.key.frame += deltaFrames; sortKeys(trackFor(s.holder, s.prop)); });
      d.startX = e.clientX; // re-baseline so the next move is a fresh delta from here
      renderTimeline();
      if (window.SMEngineBridge) SMEngineBridge.renderNow(); // live stage feedback — see skew-drag branch's own comment above
      return;
    }
    var nf = Math.max(0, Math.min(state.totalFrames - 1, d.startFrame + deltaFrames));
    if (nf === d.key.frame) return;
    var track = trackFor(d.ld, d.prop);
    if (keyAt(track, nf)) return; // don't stomp an existing key
    d.key.frame = nf; sortKeys(track);
    renderTimeline();
    if (window.SMEngineBridge) SMEngineBridge.renderNow(); // live stage feedback — see skew-drag branch's own comment above
  }
  function onDragUp() {
    if (onLayerSkewUp()) return;
    endMarquee();
    if (window._motionSkewDrag) { window._motionSkewDrag = null; if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); return; }
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
      removeKeySelectionBox();
      removeLayerStaggerBox();
      // The graph editor hides #frame-grid while it's open — leaving Motion
      // with it still on would strand the 2D timeline invisible.
      if (window.SMMotionGraph && SMMotionGraph.isOn()) SMMotionGraph.toggle(false);
    }
    state.appMode = mode;
    document.querySelectorAll('.app-mode-btn').forEach(function (b) { b.classList.toggle('active', b.dataset.mode === mode); });
    document.body.classList.toggle('mode-motion', mode === 'motion');
    document.body.classList.toggle('mode-storyboard', mode === 'storyboard');
    var mgBtn = document.getElementById('btn-mgraph');
    if (mgBtn) mgBtn.style.display = (mode === 'motion') ? '' : 'none';
    var shyBtn = document.getElementById('btn-shy');
    if (shyBtn) shyBtn.style.display = (mode === 'motion') ? '' : 'none';
    var mbBtn = document.getElementById('btn-mblur');
    if (mbBtn) mbBtn.style.display = (mode === 'motion') ? '' : 'none';
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
    // The 2D-tool sections (Fill/Stroke/Tool Options/Effects/Document) stay
    // visible via updatePropsContext()'s own inline display:block unless
    // re-run right now — without this, switching into Motion still shows
    // whatever tool context was active in 2D mode, burying the section
    // above (which contains the expression buttons) far down the panel.
    if (window.updatePropsContext) window.updatePropsContext();
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

  // Motion mode: the key-selection marquee starts from ANYWHERE in the
  // timeline grid — "je ne peux drag que à partir des lignes où y a des
  // keyframes... dans le canvas je peux drag à partir de n'importe où".
  // The full-width layer in/out bars used to swallow the mousedown on
  // their rows (bar-move drag), and the group/element spacer rows had no
  // handler at all. Capture-phase interception on #fg-wrap (static
  // element, never rebuilt) fires BEFORE any descendant's own handler, so
  // one listener covers every row type at once — except the genuinely
  // interactive targets that keep their own gesture:
  //   .fc.motion-fc        — track cells (own key-drag/marquee + goToFrame)
  //   .layer-inout-handle  — the in/out resize brackets
  //   .layer-inout-key     — per-key ticks on the bar
  //   #frame-hdr           — the numbered ruler (click/drag-to-scrub,
  //                          ui.js) — bug found 2026-07 (feedback:
  //                          "impossible de scrubber le curseur de temps"
  //                          in Motion): the ruler was never in this
  //                          exemption list, so THIS capture listener
  //                          stopPropagation()'d every ruler click before
  //                          it ever reached ui.js's own #frame-hdr
  //                          mousedown handler — scrubbing silently did
  //                          nothing in Motion mode specifically (Animation
  //                          2D never runs this listener at all, the
  //                          state.appMode!=='motion' guard above skips it
  //                          entirely, which is exactly why the ruler
  //                          worked fine there and nowhere else).
  //   #playhead-flag        — the draggable playhead handle (ui.js) has
  //                          this same "never exempted" bug — its own
  //                          mousedown handler lives in the BUBBLE phase,
  //                          strictly later than this capture listener, so
  //                          it was just as silently swallowed.
  //   #bars-row             — work-area (#wa-bar/.wa-handle) and onion
  //                          skin range (.onion-marker/#onion-bar) both
  //                          have their own drag gestures (ui.js) that hit
  //                          the exact same capture-order problem — none
  //                          of those elements were exempted either.
  //   .layer-inout-bar       — REVERSED 2026-07 (feedback: "impossible de
  //                          déplacer/drag un calque dans le temps"): this
  //                          used to be a deliberate exclusion (bar-BODY
  //                          drag intentionally lost to the marquee, only
  //                          its handles kept their own gesture — see the
  //                          old note below, kept for history). Explicit
  //                          follow-up feedback wants moving a layer's
  //                          whole timing back — same "keeps its own
  //                          gesture" treatment as its own handles now.
  // (historical) Deliberate trade-off: dragging a bar's BODY no longer
  // moved the layer range while in Motion — same priority call as the
  // canvas, where drag-anywhere marquee wins over object-move unless you
  // grab an actual object. Superseded by the entry above.
  function initGridMarquee() {
    var wrap = document.getElementById('fg-wrap');
    if (!wrap) return;
    wrap.addEventListener('mousedown', function (e) {
      if (state.appMode !== 'motion' || e.button !== 0) return;
      if (e.target.closest('.fc.motion-fc, .layer-inout-handle, .layer-inout-key, .layer-inout-bar, #frame-hdr, #playhead-flag, #bars-row')) return;
      // Scrollbar clicks land on the wrap itself but outside its client
      // area — intercepting them would break scrollbar dragging.
      var r = wrap.getBoundingClientRect();
      if (e.clientX > r.left + wrap.clientWidth || e.clientY > r.top + wrap.clientHeight) return;
      e.stopPropagation(); e.preventDefault();
      startMarquee(e);
    }, true);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', initGridMarquee); else initGridMarquee();

  window.SMMotion = {
    valueAtFrame: valueAtFrame,
    // 3D layers (2026-07-28) — see make3DProjector/project3DSegments' own
    // doc comment (Grease-Pencil-style: vertices move in 3D, stroke width
    // never scales).
    make3DProjector: make3DProjector,
    project3DSegments: project3DSegments,
    toggleLayer3D: toggleLayer3D,
    toggleLayerDuplicator: toggleLayerDuplicator,
    setDuplicatorEditSource: setDuplicatorEditSource,
    // 3D gizmo diagnostics — exposed so its projection/hit-test math can be
    // verified directly (screenshots/pixel-probing alone can't confirm
    // WHICH handle a screen point resolves to).
    activeMotionTarget: activeMotionTarget,
    gizmo3DPose: gizmo3DPose,
    gizmo3DAxisScreenPoints: gizmo3DAxisScreenPoints,
    gizmo3DRingScreenPoints: gizmo3DRingScreenPoints,
    hit3DGizmoAxis: hit3DGizmoAxis,
    hit3DGizmoRing: hit3DGizmoRing,
    debugMotionDrag: function () { return _motionDrag; },
    // Generic track evaluator — effects-panel.js keys its parameters with it
    // so an effect eases exactly like a layer property (see evalTrack).
    evalTrack: evalTrack,
    DEFAULT_CURVE: function () { return JSON.parse(JSON.stringify(DEFAULT_CURVE)); },
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
    elementFillColorAt: elementFillColorAt,
    transformSegments: transformSegments,
    transformImageRect: transformImageRect,
    ensureLayerUid: ensureLayerUid,
    findLayerIndexByUid: findLayerIndexByUid,
    setLayerParent: setLayerParent,
    shiftLayerMotionKeys: shiftLayerMotionKeys,
    exprGlobals: exprGlobals,
    setExprGlobals: function (code) {
      state.exprGlobals = code || '';
      // Drop every cached compile: they were built with the OLD preamble.
      state.layers.forEach(function (l) {
        delete l._exprCompiled;
        if (l.elementMotion) Object.keys(l.elementMotion).forEach(function (k) { delete l.elementMotion[k]._exprCompiled; });
      });
      renderLayerList(); renderTimeline();
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
    },
    // The value a property WOULD have without its expression (Van Dijk 7.4,
    // "show the unaffected value of the property"). rawValueAtFrame is the
    // pre-expression path by construction — valueAtFrame is the one that
    // layers the expression on top — so this cannot drift from it.
    rawValueAtFrame: function (holder, prop, frame) { return rawValueAtFrame(holder, prop, frame); },
    enableTimeRemap: enableTimeRemap,
    disableTimeRemap: disableTimeRemap,
    timeRemapValue: timeRemapValue,
    parentChainMats: parentChainMats,
    applyParentChainToSegments: applyParentChainToSegments,
    applyParentChainToImageRect: applyParentChainToImageRect,
    applyPathVertexOffsetsFor: applyPathVertexOffsetsFor,
    buildOverlayItems: buildOverlayItems,
    renderLayerListMotion: renderLayerListMotion,
    renderTimelineMotion: renderTimelineMotion,
    setAppMode: setAppMode,
    onDown: onDown,
    onDrag: onDrag,
    onUp: onUp,
    handlePropShortcut: handlePropShortcut,
    revealAnimated: handleRevealAnimatedShortcut,
    revealEffects: handleRevealEffectsShortcut,
    revealMattes: handleRevealMattesShortcut,
    // layer-inout.js's own marquee (which covers the layer bar rows AND the
    // empty grid space below them) forwards its rectangle here in Motion
    // mode so keyframes get selected from a drag started ANYWHERE in the
    // timeline, not only from inside a property track's own cells.
    marqueeSelect: applyMarqueeSelection,
    clearKeySelection: function () { setKeySel([]); },
    // Cheap predicate for engine-bridge's retained-path fast path: per-vertex
    // offsets are the ONE part of the element-Motion stack that is not an
    // affine, so a shape carrying them can never ride a stored path.
    hasPathVertexMotionFor: function (li, strokeId) {
      var ld = state.layers[li];
      if (!ld || !ld.elementMotion || !strokeId) return false;
      return hasPathVertexMotion(ld.elementMotion[strokeId]);
    },
    // Called by the two frame-content writers in app.js — the union is
    // derived from that content, so it must not outlive an edit.
    invalidateSymbolUnionBounds: invalidateSymbolUnionBounds,
    // layer-inout.js calls this when a bar press turned out to be a plain
    // click rather than a retime drag — the bar covers most of the grid half
    // of a layer's row, so without it that whole strip stayed unclickable.
    selectLayerFromGrid: selectLayerFromGrid,
    layerElements: layerElements,
    elementLabel: elementLabel,
    distributeKeys: distributeKeys, flipKeys: flipKeys, selectEveryNthKey: selectEveryNthKey, invertKeySelection: invertKeySelection,
    getKeySelection: function () { return _motionKeySel.slice(); },
    // Move an explicit set of keys by dx frames — used by layer-inout.js so
    // that dragging a layer's in/out point carries the SELECTED keyframes
    // with it (2026-07-25: "il faut pouvoir bouger les in/out point de
    // calque avec les keyframes selectionnées aussi"). Takes the selection
    // captured at drag START, not the live one: the drag itself re-renders
    // the grid, and a re-render rebuilds the diamonds.
    shiftKeySelection: function (sel, dx) {
      if (!sel || !sel.length || !dx) return false;
      var total = state.totalFrames, tracks = [];
      sel.forEach(function (s) {
        if (!s || !s.key) return;
        s.key.frame = Math.max(0, Math.min(total - 1, s.key.frame + dx));
        var t = trackFor(s.holder, s.prop);
        if (t && tracks.indexOf(t) < 0) tracks.push(t);
      });
      // Clamping at the edges can stack two keys on one frame — same
      // collapse rule as shiftLayerMotionKeys, keeping the earliest.
      tracks.forEach(function (t) {
        sortKeys(t);
        for (var i = t.keys.length - 1; i > 0; i--) if (t.keys[i].frame === t.keys[i - 1].frame) t.keys.splice(i, 1);
      });
      return true;
    },
    // Lets the graph editor drive the SAME selection the track view uses, so
    // clicking a point on a curve lights up its diamond and makes F9 / Delete
    // / copy behave identically from either view. Without this the two views
    // would each hold their own idea of what is selected.
    selectKeys: function (sel) { setKeySel(sel || []); },
    // ui.js's shared curve widget calls this after a motion segment's
    // curvePoints change (drag/preset/add/delete point) — the canvas needs
    // a repaint since the eased value at the current frame may have
    // changed; the panel's scrub field too, if the playhead sits inside
    // the segment being reshaped.
    onEaseSegChanged: function () { renderLayerList(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); },
    migrateLegacyCurves: migrateLegacyCurves,
    setKeyInterp: setKeyInterp,
    applyEasyEase: applyEasyEase,
    nudgeSelectedKeys: nudgeSelectedKeys,
    deleteSelectedKeys: deleteSelectedKeys,
    copySelectedKeys: copySelectedKeys,
    pasteKeys: pasteKeys,
    hasKeySelection: hasKeySelection,
    hasKeyClipboard: hasKeyClipboard,
  };
})();
