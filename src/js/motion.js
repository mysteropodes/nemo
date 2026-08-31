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
  // Order (2026-08, "système pour animer l'id index de calque ou de
  // shape/éléments") — a plain z-index: NOT a paint attribute, it's WHICH
  // POSITION in the render list this holder ends up at. Deliberately a base
  // PROPS entry rather than an opt-in extended one (like Fill/Stroke/Brush)
  // because it applies to literally every holder unconditionally, same as
  // Position/Rotation/Opacity — and since propsFor/PROP_* already serve BOTH
  // a layer holder (ld) and an element holder (ld.elementMotion[id])
  // identically, putting it here is the "même mécanisme" for both
  // granularities in one place, confirmed with Cyril rather than building
  // two parallel systems. Consumed by engine-bridge.js as a STABLE sort key
  // (default 0 ties keep original document order, exactly like CSS
  // z-index) — see layerOrderAt/elementOrderAt below.
  var PROPS = ['position', 'anchor', 'rotation', 'scale', 'opacity', 'order'];
  // 3D layer (2026-07-28, After-Effects-style "3D layer" toggle,
  // ld.threeD) — three EXTRA scalar properties, revealed only when the
  // layer/holder has 3D on: positionZ (depth), rotationX/rotationY (the
  // existing 'rotation' stays exactly as-is, now read as Z-rotation).
  // Deliberately NOT a 3rd dimension bolted onto 'position' itself (which
  // would ripple through every PROP_DIM-keyed call site in this file) —
  // three independent dim-1 properties reuse the exact same generic
  // track/keyframe/interpolation machinery 'rotation'/'opacity' already
  // do, zero changes needed to that machinery.
  var PROPS_WITH_3D = ['position', 'positionZ', 'anchor', 'rotation', 'rotationX', 'rotationY', 'scale', 'opacity', 'order'];
  // Mograph duplicator (2026-07-29, ld.duplicator) — four EXTRA keyframable
  // per-copy DELTAS (each copy k gets k× the delta, or a seeded random in
  // ±delta — see applyLayerDuplicator, app.js), revealed only when the
  // layer has a duplicator. Same "extra ordinary properties, zero new
  // machinery" approach as PROPS_WITH_3D above. The structural config
  // (grid/radial/path mode, counts, seed) is deliberately NOT here — it's
  // static per-layer state edited in its own panel (#duplicator-sec), like
  // a Component instance's own placement fields.
  // Extra Motion properties for a duplicator-enabled layer (per-copy stagger
  // deltas). Unlike every other Motion property (applied purely inside
  // buildSceneJson's own per-item matrix, computeMotionMat), these are read
  // by applyLayerDuplicator (app.js) only when getEffectiveStrokesRendered
  // runs — a plain renderNow() alone would re-serialize stale materialized
  // copies. Fixed centrally in SMEngineBridge.renderNow() (engine-bridge.js:
  // a loadFrame precedes buildSceneJson whenever any layer has a duplicator)
  // rather than patching every commit path here — see that fix's own
  // comment for why (CLAUDE.md §1's whack-a-mole trap).
  // dupOffsetPosZ/RotX/RotY (2026-07-30, "en 3D aussi avec ID de chaque
  // cloner") — same per-copy k×delta/±delta stagger as the original 4, just
  // three more channels so a duplicator's copies can spread/rotate through
  // real depth instead of just the XY plane. Only visibly does anything
  // once the layer ALSO has threeD on (buildSceneJson reads them off each
  // clone's own data.dup3D to build a per-clone 3D projector instead of the
  // single shared layer-wide one — see engine-bridge.js) — shown
  // unconditionally here anyway, same "extra property exists, does nothing
  // until its prerequisite is met" precedent Time Remap already sets for
  // a non-Component layer.
  var PROPS_DUP_EXTRA = ['dupOffsetPos', 'dupOffsetRot', 'dupOffsetScale', 'dupOffsetOpacity', 'dupOffsetPosZ', 'dupOffsetRotX', 'dupOffsetRotY'];
  // Time Remap (AE, 2026-07-25) is an EXTRA row, not a 6th transform: it
  // never feeds computeMotionMat — it drives which internal frame a
  // component instance shows (resolveSymbolFrameIdx, app.js). Both the
  // panel and the grid must iterate the same list or their rows desync,
  // which is the alignment invariant ROW_H's header comment is about — so
  // there is exactly ONE function that decides, and both sides call it.
  // Component exposed properties (2026-08-18, "réutilisation dynamique de
  // component... modifier des properties au dessus", Figma Component
  // Properties + AE Master Properties synthesis, confirmed with Cyril).
  // Declared ONCE on the symbol (state.symbols[id].exposedProps, via the
  // "Exposer…" context menu items in select-bridge.js while editing INSIDE
  // it), surfaced here as EXTRA ordinary Motion properties on every INSTANCE
  // layer — same "extra properties, zero new track/keyframe machinery"
  // precedent as PROPS_WITH_3D/PROPS_DUP_EXTRA above. Registers the row's
  // PROP_LABEL/PROP_DIM/PROP_UNIT/PROP_DEFAULT on every call (idempotent,
  // cheap) rather than once at exposure time — exposure happens in app.js,
  // a project reload never re-runs that code path, and propsFor is the one
  // function EVERY row (panel AND grid, the ROW_H alignment invariant) is
  // guaranteed to call before it can render anything, so self-registering
  // here is the only way that survives both first-use and reload alike.
  // Like registerExposedPropMeta but honours dimension and unit — effector
  // layers need 2D offsets (position/scale) and real units, which the
  // exposed-property registrar deliberately flattens to 1D/no-unit.
  function registerEffectorPropMeta(key, label, defaultVal, dim, unit) {
    PROP_LABEL[key] = label;
    PROP_DIM[key] = dim || 1;
    PROP_UNIT[key] = unit || '';
    PROP_DEFAULT[key] = (dim === 2) ? [defaultVal, defaultVal] : [defaultVal];
  }
  function registerExposedPropMeta(key, label, defaultVal) {
    PROP_LABEL[key] = label; PROP_DIM[key] = 1; PROP_UNIT[key] = '';
    PROP_DEFAULT[key] = [defaultVal];
  }
  // ---- Expression controls (2026-08-30) ---------------------------------
  // Named, typed, KEYFRAMABLE parameters carried by a layer, whose reason to
  // exist is to be read from expressions: one "Wave amount" number on a null
  // layer that ten other layers' rotation reads, so a single value drives a
  // whole rig instead of ten copies of the same magic constant.
  //
  // Built on the SAME mechanism Component exposed properties already use
  // (registerExposedPropMeta + propsFor, just above): a control IS an extra
  // ordinary Motion property, not a parallel store hanging off the side.
  // That one decision is what makes the stopwatch, the keyframe machinery,
  // the graph editor, ease curves, the pickwhip, expression access AND —
  // most importantly — CLAUDE.md §11's panel/grid row-alignment invariant
  // all come along for free, because propsFor is the single function both
  // halves of the timeline call before they can render anything. Anything
  // that stored controls outside the property system would have to re-fight
  // every one of those.
  //
  // Storage: ld.exprControls = [{key, name, type, default}], with a stable
  // generated key (never an array index — same id discipline as layerUid/
  // strokeId elsewhere), so renaming or reordering a control can never
  // orphan the keyframe track that key names inside ld.motion.
  var CONTROL_TYPES = ['number', 'checkbox', 'angle', 'point', 'color'];
  // Colour is genuinely available here, and it isn't a special case bolted
  // on: PROP_DIM has never been limited to 1 and 2 — the per-element
  // 'fillColor' property already runs a 4-channel [r,g,b,a] track through
  // this exact generic keyframe/interpolation machinery (see
  // elementFillColorAt). A control only needs its own FIELD widget (a
  // swatch instead of four number boxes), which renderTransformProps
  // branches on below; nothing under it changes.
  var CONTROL_DIM = { number: 1, checkbox: 1, angle: 1, point: 2, color: 4 };
  var CONTROL_UNIT = { number: '', checkbox: '', angle: '°', point: 'px', color: '' };
  var CONTROL_DEFAULT = { number: [0], checkbox: [0], angle: [0], point: [0, 0], color: [255, 255, 255, 255] };
  // prop key -> control type, so the row builders can ask "is this a
  // control, and which widget does it want" without re-scanning every
  // layer's list on every row.
  var _controlTypeByKey = {};
  function controlsOf(holder) { return (holder && Array.isArray(holder.exprControls)) ? holder.exprControls : []; }
  function registerControlPropMeta(c) {
    if (!c || !c.key) return;
    var type = CONTROL_TYPES.indexOf(c.type) >= 0 ? c.type : 'number';
    PROP_LABEL[c.key] = c.name || SM.t('ctrlUnnamed');
    PROP_DIM[c.key] = CONTROL_DIM[type];
    PROP_UNIT[c.key] = CONTROL_UNIT[type];
    PROP_DEFAULT[c.key] = Array.isArray(c.default) ? c.default.slice() : CONTROL_DEFAULT[type].slice();
    if (type === 'point') PROP_DIM_LABELS[c.key] = ['X', 'Y'];
    _controlTypeByKey[c.key] = type;
  }
  function controlTypeOf(prop) { return _controlTypeByKey[prop] || null; }
  function propsFor(holder) {
    var list = (holder && holder.threeD) ? PROPS_WITH_3D : PROPS;
    if (holder && holder.duplicator) list = list.concat(PROPS_DUP_EXTRA);
    if (holder && holder.symbolId && state.symbols[holder.symbolId] && state.symbols[holder.symbolId].exposedProps && state.symbols[holder.symbolId].exposedProps.length) {
      var epList = state.symbols[holder.symbolId].exposedProps;
      epList.forEach(function (ep) { registerExposedPropMeta(ep.key, ep.label, ep.default); });
      list = list.concat(epList.map(function (ep) { return ep.key; }));
    }
    // Dynamic shapes phase 2 (2026-08-18) — a rect's corner radii, keyable
    // PER SHAPE. `holder` here is an ELEMENT holder (ld.elementMotion[id],
    // see ensureElementHolder), tagged .paramShapeKind at creation time from
    // the live item's data.paramShape.kind — same "extra properties on a
    // tagged holder" shape as the layer-level exposedProps branch just
    // above, one level down (element instead of layer).
    if (holder && holder.paramShapeKind === 'rect') list = list.concat(['cornerTL', 'cornerTR', 'cornerBR', 'cornerBL']);
    if (holder && holder.paramShapeKind === 'ellipse') list = list.concat(['arcStart', 'arcSweep', 'arcInner']);
    if (holder && holder.paramShapeKind === 'star') list = list.concat(['starInner', 'starCorner']);
    // Multi-parent crossfade (2026-07-30, "jouer comme une opacité les
    // parents entre eux") — parentBlend only means anything once a SECOND
    // parent exists (parentLayerUidB); an ordinary single-parent layer
    // shows nothing extra here, same "hidden until its prerequisite is
    // set" precedent Time Remap already establishes for symbolId.
    // Effector layer rows (2026-08-30) — radius/strength/angle plus one
    // offset per duplicator-targetable channel, all ordinary keyframable
    // Motion properties. Listed through propsFor like everything else, so
    // panel and grid agree by construction (§11).
    if (holder && holder.isEffectorLayer && window.SMEffectorLayer) {
      list = list.concat(SMEffectorLayer.propKeysFor(holder));
    }
    if (holder && holder.parentLayerUidB) list = list.concat(['parentBlend']);
    // One weight row per parent beyond A/B (2026-08-30). Self-registered
    // here for the same reason the exposed-property keys below are: a
    // project reload never re-runs whatever code created the parent, and
    // propsFor is the one function BOTH sides call before rendering a row,
    // so registering here is what survives first-use and reload alike.
    // Default 0 so adding a parent never moves the layer until you give it
    // weight — see weightedParentsOf for why that falls out of the model.
    if (holder && holder.parentLayerUidB && holder.parentsMore) {
      for (var pw = 0; pw < holder.parentsMore.length; pw++) {
        var pwe = holder.parentsMore[pw];
        if (!pwe || !pwe.uid) continue;
        var pwIdx = findLayerIndexByUid(pwe.uid);
        var pwName = pwIdx >= 0 ? (state.layers[pwIdx].name || ('Layer ' + (pwIdx + 1))) : '?';
        var pwKey = parentWeightKeyFor(pw);
        registerExposedPropMeta(pwKey, SM.t('propParentWeightPrefix') + ' ' + pwName, 0);
        PROP_UNIT[pwKey] = '%';
        list = list.concat([pwKey]);
      }
    }
    // Matte On (2026-08-30) — same "only once it means something" gate as
    // parentBlend right above: a layer with no matte has nothing to switch
    // on and off, so the row would be a dead control. Threshold semantics
    // (>= 50 is matted) live in matteOnAt, the single reader.
    if (holder && holder.matteMode && holder.matteMode !== 'none') list = list.concat(['matteOn']);
    // Follow Path (2026-08, motion path unifié — see FollowPathEffect
    // research note). Layer-level only for v1 (ld.followPath, not per-
    // element) — same "static config field, extra keyable rows only once
    // it's set" precedent as parentLayerUidB/timeLink just above.
    // pathInfluence only means anything once "Align to path" is on (it
    // blends the path's tangent INTO rotation — position always follows
    // the path at full strength, matching FollowPathEffect's own design).
    if (holder && holder.followPath && holder.followPath.targetLayerUid) {
      list = list.concat(['pathPercent']);
      if (holder.followPath.align) list = list.concat(['pathInfluence']);
    }
    // Parent in Time (Van Dijk 2.1, "Time... In/Out Points become values...
    // linked together with Expressions") — offset rows only appear for
    // whichever edge(s) the current link mode actually drives (mirrors
    // resolveLinkedTime's own which==='out'&&mode==='in' style guard,
    // app.js) so there's never a dead control for an edge that isn't
    // linked. NO stopwatch on these rows (see the exception in
    // renderTransformProps) — layerInPoint/layerOutPoint are read at 13
    // call sites with no frame parameter (confirmed by grep before
    // building this), so true per-frame keyframing has no coherent
    // meaning here without a much bigger rewrite. Expression-only,
    // evaluated live off state.currentFrame — confirmed scope with Cyril.
    if (holder && holder.timeLink) {
      var tlMode = holder.timeLink.mode || 'both';
      if (tlMode !== 'out') list = list.concat(['timeLinkInOffset']);
      if (tlMode !== 'in') list = list.concat(['timeLinkOutOffset']);
    }
    // Expression controls (2026-08-30) — listed LAST so they read as the
    // layer's own parameter block below its transform, and registered on
    // EVERY call for the same reason the exposedProps branch above does it:
    // a project reload never re-runs whatever code created the control, and
    // propsFor is the one function every row on BOTH sides is guaranteed to
    // call before it can render, so self-registering here is what survives
    // first-use and reload alike.
    var ctrls = controlsOf(holder);
    if (ctrls.length) {
      ctrls.forEach(registerControlPropMeta);
      list = list.concat(ctrls.map(function (c) { return c.key; }));
    }
    if (holder && holder.timeRemap) return list.concat(['timeRemap']);
    return list;
  }
  // ---- control CRUD ------------------------------------------------------
  // Names are user-facing strings typed twice — once when creating the
  // control, once inside an expression — so every lookup normalizes the same
  // way (trimmed, case-insensitive) and creation refuses a name that would
  // collide under that same rule. Two controls a user reads as "the same
  // name" resolving to different tracks is a trap, not a feature.
  function normalizeControlName(n) { return String(n == null ? '' : n).trim().toLowerCase(); }
  function findControl(ld, name) {
    var want = normalizeControlName(name);
    if (!want) return null;
    var list = controlsOf(ld);
    for (var i = 0; i < list.length; i++) if (normalizeControlName(list[i].name) === want) return list[i];
    return null;
  }
  function genControlKey() { return 'xc_' + Date.now().toString(36) + '_' + Math.random().toString(36).slice(2, 6); }
  function addExprControl(ld, type, name) {
    if (!ld) return null;
    var t = CONTROL_TYPES.indexOf(type) >= 0 ? type : 'number';
    var nm = String(name == null ? '' : name).trim();
    if (!nm) return null;
    if (findControl(ld, nm)) { if (window.showToast) showToast(SM.t('toastControlNameTaken')); return null; }
    if (!Array.isArray(ld.exprControls)) ld.exprControls = [];
    var c = { key: genControlKey(), name: nm, type: t, default: CONTROL_DEFAULT[t].slice() };
    ld.exprControls.push(c);
    registerControlPropMeta(c);
    return c;
  }
  function renameExprControl(ld, key, name) {
    var nm = String(name == null ? '' : name).trim();
    if (!nm) return false;
    var list = controlsOf(ld), target = null;
    for (var i = 0; i < list.length; i++) if (list[i].key === key) target = list[i];
    if (!target) return false;
    var clash = findControl(ld, nm);
    if (clash && clash !== target) { if (window.showToast) showToast(SM.t('toastControlNameTaken')); return false; }
    target.name = nm;
    registerControlPropMeta(target);
    return true;
  }
  // Deleting a control removes its TRACK too — leaving ld.motion[key] behind
  // would be data nothing can ever reach again (no row lists it, no
  // expression can name it) that still rides along in every save. Any
  // expression still referencing the deleted name degrades on its own: the
  // lookup returns nothing, the engine's ordinary error path shows the
  // message on that one property row and falls back to the raw value.
  function removeExprControl(ld, key) {
    var list = controlsOf(ld), idx = -1;
    for (var i = 0; i < list.length; i++) if (list[i].key === key) idx = i;
    if (idx < 0) return false;
    list.splice(idx, 1);
    if (ld.motion) delete ld.motion[key];
    if (ld.motionStatic) delete ld.motionStatic[key];
    if (ld.expressions) delete ld.expressions[key];
    if (ld._exprCompiled) delete ld._exprCompiled[key];
    if (window._exprEditorOpen && window._exprEditorOpen.holder === ld && window._exprEditorOpen.prop === key) window._exprEditorOpen = null;
    setKeySel(_motionKeySel.filter(function (s) { return !(s.holder === ld && s.prop === key); }));
    if (!list.length) delete ld.exprControls;
    return true;
  }
  function moveExprControl(ld, key, dir) {
    var list = controlsOf(ld), idx = -1;
    for (var i = 0; i < list.length; i++) if (list[i].key === key) idx = i;
    var to = idx + dir;
    if (idx < 0 || to < 0 || to >= list.length) return false;
    var tmp = list[idx]; list[idx] = list[to]; list[to] = tmp;
    return true;
  }
  function afterControlEdit(ld) {
    if (typeof saveActiveLayerFrame === 'function') saveActiveLayerFrame();
    renderLayerList(); renderTimeline();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }
  // Add / rename / delete / reorder, reached from the layer's own
  // right-click menu (and from a right-click on a control row itself).
  // Deliberately NOT a new settings surface: this is the same nested
  // showContextMenu + prompt idiom the layer menu already uses for
  // "Inherit effects from…" and "Stagger…", so there is one place to look
  // for anything about a layer and one interaction to learn.
  function openExprControlsMenu(x, y, ld) {
    if (!window.showContextMenu) return;
    var items = [{ label: SM.t('ctrlMenuAddEllipsis'), action: function () {
      window.showContextMenu(x + 8, y + 8, CONTROL_TYPES.map(function (t) {
        return { label: SM.t('ctrlType_' + t), action: function () {
          var nm = prompt(SM.t('promptControlName'), SM.t('ctrlType_' + t));
          if (nm === null) return;
          pushUndo();
          var c = addExprControl(ld, t, nm);
          if (!c) return;
          afterControlEdit(ld);
          if (window.showToast) showToast(SM.t('toastControlAddedPrefix') + c.name + SM.t('toastControlAddedSuffix'));
        } };
      }));
    } }];
    var list = controlsOf(ld);
    if (list.length) {
      items.push({ sep: true });
      // showContextMenu has no submenus and no group headings — a disabled
      // row is how the rest of this file titles a group (see the layer
      // menu's own "Lock keyframes on:").
      items.push({ label: SM.t('ctrlMenuExistingColon'), disabled: true, action: function () {} });
      list.forEach(function (c, ci) {
        items.push({ label: c.name + '  ·  ' + SM.t('ctrlType_' + c.type), action: function () {
          window.showContextMenu(x + 8, y + 8, [
            { label: SM.t('ctrlMenuRenameEllipsis'), action: function () {
              var nm = prompt(SM.t('promptControlRename'), c.name);
              if (nm === null) return;
              pushUndo();
              if (renameExprControl(ld, c.key, nm)) afterControlEdit(ld);
            } },
            { label: SM.t('ctrlMenuMoveUp'), disabled: ci === 0, action: function () { pushUndo(); moveExprControl(ld, c.key, -1); afterControlEdit(ld); } },
            { label: SM.t('ctrlMenuMoveDown'), disabled: ci === list.length - 1, action: function () { pushUndo(); moveExprControl(ld, c.key, 1); afterControlEdit(ld); } },
            { label: SM.t('ctrlMenuDelete'), action: function () {
              pushUndo();
              if (!removeExprControl(ld, c.key)) return;
              afterControlEdit(ld);
              if (window.showToast) showToast(SM.t('toastControlDeletedPrefix') + c.name + SM.t('toastControlDeletedSuffix'));
            } },
          ]);
        } });
      });
    }
    window.showContextMenu(x, y, items);
  }
  // ---- Collapsible "Duplicator" sub-group (2026-07-30, "créer des sous
  // menu à transform pour des properties comme duplicator afin que ça soit
  // mieux rangés") ----
  // THE visible row sequence of a holder's Transform group. The panel
  // (renderTransformProps) AND the grid (renderTimelineMotion — the layer
  // loop and the element loop both) iterate THIS list, never propsFor
  // directly, so the sub-group's header row and its collapsed/expanded
  // state can never desync the two sides (ROW_H's alignment invariant).
  // propsFor itself stays the data-level truth untouched: every non-render
  // consumer (key collection for stagger/skew/selection-box bounds) keeps
  // seeing dup props regardless of collapse — collapsing is purely visual.
  // The shared filter (property-letter filter + hide-unanimated) lives HERE
  // for the same reason; renderTracksFor re-checks it internally, which is
  // a harmless no-op since it's the identical condition.
  // Entries: {row:'prop', prop} — one ordinary property row;
  //          {row:'dupHeader'} — the Duplicator sub-header (panel: a
  //          chevron header row, grid: one blank spacer .frow).
  // Dup rows are re-grouped AFTER parentBlend/timeLink/timeRemap even
  // though propsFor lists them before — a collapsible unit reads best last,
  // and both sides get the same order from here so alignment holds.
  function transformRowPlan(holder) {
    var rows = [], dupProps = [];
    propsFor(holder).forEach(function (prop) {
      if (isPropFiltered(prop) || (_hideUnanimated && !propHasContent(holder, prop)) || !motionPropMatchesView(holder, prop)) return;
      if (PROPS_DUP_EXTRA.indexOf(prop) >= 0) { dupProps.push(prop); return; }
      rows.push({ row: 'prop', prop: prop });
    });
    if (dupProps.length) {
      rows.push({ row: 'dupHeader' });
      if (isDupGroupExpanded(holder)) dupProps.forEach(function (p) { rows.push({ row: 'prop', prop: p }); });
    }
    return rows;
  }
  // Same single-accordion idiom as _motionExpandedPathHolder one level
  // down — object identity against the live holder dict, which is stable
  // across renders (state.layers entries are mutated in place, never
  // recreated, same assumption the Path group already relies on).
  function isDupGroupExpanded(holder) { return window._motionExpandedDupHolder === holder; }
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
    // Discrete hold-key tracks (feedback #207/#212/#214) — Blend/Parent
    // keys live directly on the layer (ld.blendKeys/ld.parentKeys), not
    // under holder.motion[prop] like every numeric track, because they
    // aren't lerp-able values (see setLayerParent's own header comment on
    // this whole discrete-key pattern). Wrapping the LIVE array in
    // {keys:...} here — not a copy — is what lets every generic
    // selection/drag/delete consumer of trackFor(...).keys (deleteSelectedKeys,
    // onDragMove's group-drag, sortKeys) work on these two props exactly
    // like any numeric one, with zero changes to any of them.
    if (prop === 'blendKeys') return holder.blendKeys ? { keys: holder.blendKeys } : null;
    if (prop === 'parentKeys') return holder.parentKeys ? { keys: holder.parentKeys } : null;
    return (holder.motion && holder.motion[prop]) || null;
  }
  var PROP_LABEL = { position: 'Position', anchor: 'Anchor Point', rotation: 'Rotation', scale: 'Scale', opacity: 'Opacity', order: 'Order', timeRemap: 'Time Remap', positionZ: 'Position Z', rotationX: 'Rotation X', rotationY: 'Rotation Y', dupOffsetPos: 'Dup. Offset', dupOffsetRot: 'Dup. Rotation', dupOffsetScale: 'Dup. Scale', dupOffsetOpacity: 'Dup. Opacity', dupOffsetPosZ: 'Dup. Offset Z', dupOffsetRotX: 'Dup. Rotation X', dupOffsetRotY: 'Dup. Rotation Y', parentBlend: 'Parent Blend', matteOn: 'Matte On', timeLinkInOffset: 'Décalage entrée', timeLinkOutOffset: 'Décalage sortie', cornerTL: 'Coin ↖', cornerTR: 'Coin ↗', cornerBR: 'Coin ↘', cornerBL: 'Coin ↙', arcStart: 'Début (arc)', arcSweep: 'Ouverture (arc)', arcInner: 'Rayon interne', starInner: 'Rayon interne', starCorner: 'Coins', pathPercent: 'Position sur le chemin', pathInfluence: 'Influence (rotation)' };
  var PROP_DIM = { position: 2, anchor: 2, rotation: 1, scale: 2, opacity: 1, order: 1, timeRemap: 1, positionZ: 1, rotationX: 1, rotationY: 1, dupOffsetPos: 2, dupOffsetRot: 1, dupOffsetScale: 2, dupOffsetOpacity: 1, dupOffsetPosZ: 1, dupOffsetRotX: 1, dupOffsetRotY: 1, parentBlend: 1, matteOn: 1, timeLinkInOffset: 1, timeLinkOutOffset: 1, cornerTL: 1, cornerTR: 1, cornerBR: 1, cornerBL: 1, arcStart: 1, arcSweep: 1, arcInner: 1, starInner: 1, starCorner: 1, pathPercent: 1, pathInfluence: 1 };
  var PROP_UNIT = { position: 'px', anchor: 'px', rotation: '°', scale: '%', opacity: '%', order: '', timeRemap: 'f', positionZ: 'px', rotationX: '°', rotationY: '°', dupOffsetPos: 'px', dupOffsetRot: '°', dupOffsetScale: '%', dupOffsetOpacity: '%', dupOffsetPosZ: 'px', dupOffsetRotX: '°', dupOffsetRotY: '°', parentBlend: '%', matteOn: '%', timeLinkInOffset: 'f', timeLinkOutOffset: 'f', cornerTL: 'px', cornerTR: 'px', cornerBR: 'px', cornerBL: 'px', arcStart: '°', arcSweep: '°', arcInner: '%', starInner: '%', starCorner: 'px', pathPercent: '%', pathInfluence: '%' };
  // parentBlend defaults to 0 — "0%" reads as "fully Parent A" (the
  // pre-existing single parent), matching the invariant that assigning a
  // second parent must never itself move anything until the user actually
  // animates the blend (same "adding a feature is a visual no-op until
  // deliberately used" precedent enableTimeRemap's own seeded keys follow).
  var PROP_DEFAULT = { position: [0, 0], anchor: [0, 0], rotation: [0], scale: [100, 100], opacity: [100], order: [0], timeRemap: [0], positionZ: [0], rotationX: [0], rotationY: [0], dupOffsetPos: [0, 0], dupOffsetRot: [0], dupOffsetScale: [0, 0], dupOffsetOpacity: [0], dupOffsetPosZ: [0], dupOffsetRotX: [0], dupOffsetRotY: [0], parentBlend: [0], matteOn: [100], timeLinkInOffset: [0], timeLinkOutOffset: [0],
    // Trim Paths (2026-08, AE parity — "animer les stroke en in et out"):
    // start/end as % of the path's own arc length, offset as a % that
    // shifts the whole [start,end] window — same 3-field shape as AE's own
    // Trim Paths, see applyTrimFor's doc comment for the combine math.
    trimStart: [0], trimEnd: [100], trimOffset: [0],
    // Follow Path (2026-08) — 0% = start of the target path; influence
    // defaults to 100 (rotation fully follows the path's tangent once
    // "Align" is on — matching Friction's own default, see the research
    // note on FollowPathEffect).
    pathPercent: [0], pathInfluence: [100],
    // Dynamic shape corners (2026-08-18) — safety-net fallback only; the
    // REAL per-shape default is each rect's own data.paramShape.tl/tr/br/bl
    // (every shape has its own baked radii, unlike every other prop here
    // which shares one constant across all holders), seeded onto the
    // element holder's motionStatic the moment it's created — see
    // ensureElementHolder's own comment.
    cornerTL: [0], cornerTR: [0], cornerBR: [0], cornerBL: [0],
    arcStart: [0], arcSweep: [359.9], arcInner: [0],
    starInner: [50], starCorner: [0] };
  // Rows with no stopwatch — layerInPoint/layerOutPoint (app.js) are read at
  // 13 call sites with no frame parameter, so a real keyframe track on these
  // would silently only ever reflect state.currentFrame at read time (export
  // included) rather than the frame actually being resolved. Static value +
  // expression only (still fully wired: hasExpr/evalExpressionFor don't care
  // whether a track exists) — confirmed scope with Cyril rather than either
  // hiding this limitation or threading frame through all 13 sites.
  var PROP_NO_STOPWATCH = { timeLinkInOffset: 1, timeLinkOutOffset: 1 };
  // timeLinkInOffset/timeLinkOutOffset feed layerHasTimeRange/
  // getEffectiveStrokes' visible-range check (app.js) — unlike every other
  // Motion property, committing one of these can change WHICH content is
  // materialized for this layer, not just its transform, so it needs the
  // extra loadFrame() reconstruction that renderNow() alone doesn't do.
  // Narrow, explicit exception — called from every commit path that can
  // change these two props (plain value, expression toggle, expression
  // code), not a general property hook.
  function reloadIfTimeLinkOffset(prop) {
    if ((prop === 'timeLinkInOffset' || prop === 'timeLinkOutOffset') && window.loadFrame) loadFrame(state.currentFrame);
  }
  // Small per-dimension labels ("X"/"Y") shown before each multi-dimension
  // property's field, LottieFiles-inspired (2026-07-29) — every 2-dim prop
  // here is an X/Y pair, so one shared default covers them all.
  var PROP_DIM_LABELS = { position: ['X', 'Y'], anchor: ['X', 'Y'], scale: ['X', 'Y'], dupOffsetPos: ['X', 'Y'], dupOffsetScale: ['X', 'Y'] };
  // Which properties a duplicator's per-copy stagger AND an Effector
  // (app.js's applyLayerDuplicator) can drive — the "n'importe quel
  // property" generalization (2026-07-30) is bounded to this curated set
  // (every ordinary transform channel except anchor/timeRemap, which don't
  // make sense per-clone) rather than truly arbitrary properties (fill
  // color etc. aren't PROP_DIM-registered as keyframable at all yet).
  // Exported so app.js's applyLayerDuplicator/Effector UI don't hardcode a
  // second copy of this list (CLAUDE.md §3's duplicated-pair trap).
  var DUP_TARGET_PROPS = ['position', 'positionZ', 'rotation', 'rotationX', 'rotationY', 'scale', 'opacity'];
  var DUP_OFFSET_PROP = { position: 'dupOffsetPos', positionZ: 'dupOffsetPosZ', rotation: 'dupOffsetRot', rotationX: 'dupOffsetRotX', rotationY: 'dupOffsetRotY', scale: 'dupOffsetScale', opacity: 'dupOffsetOpacity' };
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
  // Scale X/Y aspect-ratio lock (feedback #120, "il manque le cadenas pour
  // que le x et y scale soit lié") — per-holder, not persisted with the
  // project (exportJSON's layer serialization, timeline.js, is an explicit
  // field whitelist — this is an editing convenience like _hideUnanimated
  // above, not project data, so a WeakSet avoids needing to add a field
  // there+on import for a purely-UI toggle). Cleared automatically when a
  // holder is garbage-collected, no explicit cleanup needed.
  var _scaleLockedHolders = new WeakSet();
  // Motion timeline workspace filters. They are UI-only: project data stays
  // untouched, while the column/filter preferences follow the workstation.
  var _motionSearch = '';
  var _motionFilterMode = 'all';
  // Default column preset (2026-08-29, Cyril: "3D/mograph par default") — the
  // 3D Layer/Duplicator toggle icons only show under this preset (see
  // style.css's `[data-motion-columns]` rules); 'animation' (the prior
  // default) hid them, which read as "these icons disappeared" until you
  // knew to open the column picker and switch presets.
  var _motionColumnPreset = '3d';
  var _motionSnapEnabled = true;
  try {
    _motionFilterMode = localStorage.getItem('nemo-motion-filter') || 'all';
    _motionColumnPreset = localStorage.getItem('nemo-motion-columns') || '3d';
    _motionSnapEnabled = localStorage.getItem('nemo-motion-snap') !== '0';
  } catch (e) {}
  function propHasContent(holder, prop) { return isAnimated(holder, prop) || !!(holder.motionStatic && holder.motionStatic[prop]); }
  function isPropFiltered(prop) { return !!_propFilter && _propFilter.indexOf(prop) < 0; }
  function propHasExpression(holder, prop) {
    return !!(holder && holder.expressions && holder.expressions[prop] && holder.expressions[prop].enabled);
  }
  function propHasExpressionError(holder, prop) {
    return !!(propHasExpression(holder, prop) && holder.expressions[prop].lastError);
  }
  function motionPropMatchesView(holder, prop) {
    if (_motionSearch && String(PROP_LABEL[prop] || prop).toLowerCase().indexOf(_motionSearch) < 0 &&
        String((holder && holder.name) || '').toLowerCase().indexOf(_motionSearch) < 0) return false;
    if (_motionFilterMode === 'animated') return isAnimated(holder, prop);
    if (_motionFilterMode === 'modified') return propHasContent(holder, prop);
    if (_motionFilterMode === 'expressions') return propHasExpression(holder, prop);
    if (_motionFilterMode === 'errors') return propHasExpressionError(holder, prop);
    return true;
  }
  function layerMatchesMotionView(ld) {
    if (!ld) return false;
    var nameMatch = !_motionSearch || String(ld.name || '').toLowerCase().indexOf(_motionSearch) >= 0;
    if (_motionFilterMode === 'effects') {
      if (!((ld.effects && ld.effects.length) || ld.effectsFrom || ld.isEffectLayer)) return false;
      return nameMatch;
    }
    var props = propsFor(ld);
    var propMatch = props.some(function (prop) { return motionPropMatchesView(ld, prop); });
    return nameMatch ? (_motionFilterMode === 'all' || propMatch) : propMatch;
  }
  function motionViewIsNarrowed() { return !!_motionSearch || _motionFilterMode !== 'all'; }
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
    return window._motionExpandedLayer === li ||
      (window._motionRevealedLayers && window._motionRevealedLayers.indexOf(li) >= 0) ||
      (motionViewIsNarrowed() && layerMatchesMotionView(state.layers[li]));
  }
  // Does ANY per-element holder on this layer carry animated content?
  // Text-animator's per-glyph offsets (opacity/position/scale — see
  // text-animator.js's ALL_PRESET_PROPS) and any hand-keyed per-shape
  // property live ONLY on ld.elementMotion[strokeId], never on the
  // layer's own holder — U's reveal (below) needs this to know whether a
  // target layer's animated content is hiding down in the Elements tree.
  function layerHasAnimatedElements(ld) {
    if (!ld.elementMotion) return false;
    for (var k in ld.elementMotion) {
      var h = ld.elementMotion[k];
      if (h && PROPS.some(function (p) { return propHasContent(h, p); })) return true;
    }
    return false;
  }
  function handleRevealAnimatedShortcut() {
    if (state.appMode !== 'motion') return false;
    var targets = (window._layerSel && window._layerSel.length) ? window._layerSel.slice() : state.layers.map(function (_l, i) { return i; });
    window._motionRevealedLayers = targets;
    // Also open the Elements tree for any target layer whose animated
    // content lives per-element rather than on the layer's own Transform
    // (feedback #145: "le raccourci U ne révèle pas toute les keyframes
    // notamment des text animation") — deliberately only the layers that
    // actually HAVE per-element keys, not every revealed layer: opening it
    // unconditionally would reintroduce feedback #42's original complaint
    // (U cascading into every layer's full per-shape breakdown even when
    // nothing down there is animated).
    window._motionRevealedElementLayers = targets.filter(function (li) {
      var ld = state.layers[li];
      return ld && layerHasAnimatedElements(ld);
    });
    // Un-collapse any folder hiding an ANIMATED target (2026-08-29 audit
    // finding, confirmed live: with the only animated layer inside a
    // collapsed folder, U appeared to do nothing — its row simply isn't
    // rendered while ld.folderCollapsed hides the subtree, so the reveal
    // set pointed at rows that don't exist). Same narrowing discipline as
    // _motionRevealedElementLayers just above: only folders whose subtree
    // actually CARRIES animated content get opened — expanding every
    // folder on every U press would be feedback #42's clutter all over
    // again. One level is the whole tree (no nested folders in v1).
    targets.forEach(function (li) {
      var ld = state.layers[li];
      if (!ld || !ld.parentLayerUid) return;
      var animated = PROPS.some(function (p) { return propHasContent(ld, p); }) || layerHasAnimatedElements(ld);
      if (!animated) return;
      for (var fi = 0; fi < state.layers.length; fi++) {
        var fld = state.layers[fi];
        if (fld && fld.isFolderLayer && fld.folderCollapsed && fld.layerUid === ld.parentLayerUid) fld.folderCollapsed = false;
      }
    });
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
    if (window.showToast) showToast(targets.length + SM.t('toastLayersRevealedSuffix'));
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
  function curveMatchesPreset(points, preset) {
    if (!points || points.length !== preset.length) return false;
    for (var i = 0; i < preset.length; i++) {
      if (Math.abs((points[i].x || 0) - preset[i].x) > 1e-6 || Math.abs((points[i].y || 0) - preset[i].y) > 1e-6) return false;
      if (typeof points[i].tx === 'number' || typeof points[i].ty === 'number') return false;
    }
    return true;
  }
  function keyInterpolationKind(key) {
    if (key && key.hold) return 'hold';
    if (key && curveMatchesPreset(key.curvePoints || DEFAULT_CURVE, CURVE_LINEAR)) return 'linear';
    return 'smooth';
  }
  function keyInterpolationLabel(key) {
    var kind = keyInterpolationKind(key);
    return kind === 'hold' ? 'Bloc / maintien' : (kind === 'linear' ? 'Linéaire' : 'Lissée');
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
      // Switching away from Hold must genuinely restore interpolation; the
      // old code changed curvePoints but left hold=true, so the icon and the
      // evaluated animation remained blocked despite choosing Linear/Ease.
      s.key.hold = false;
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
        var occupant = keyAt(g.track, nf);
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
  // Each copied key remembers WHERE it came from (2026-08-30). Without this,
  // a selection spanning several layers was flattened onto whatever layer
  // happened to be active at paste time, so keys from different layers that
  // share a property name — position, the common case — silently overwrote one
  // another and the rest of the layers got nothing. Reported as: "when you
  // select keyframes from multiple layers and you want to paste them somewhere
  // else in time... it'd be great if we could copy and paste keyframes from
  // different layers."
  //
  // The reference is by layerUid, not index, because layers get reordered; and
  // by the element-holder KEY (a strokeId, or a meshId for an image mesh)
  // rather than the holder object, because loadFrame rebuilds those objects —
  // the same reason rig binds and elementMotion are keyed that way.
  function holderRefOf(holder) {
    var r = resolveHolderLayer(holder);
    if (!r) return null;
    return { uid: r.ld.layerUid || null, elem: r.strokeId || null };
  }
  function holderFromRef(ref) {
    if (!ref) return null;
    var li = ref.uid ? findLayerIndexByUid(ref.uid) : -1;
    var ld = li >= 0 ? state.layers[li] : null;
    if (!ld) return null;
    if (!ref.elem) return ld;
    return (ld.elementMotion && ld.elementMotion[ref.elem]) || ensureElementHolder(ld, ref.elem);
  }
  function copySelectedKeys() {
    var sel = _motionKeySel;
    if (!sel || !sel.length) return 0;
    var base = Math.min.apply(null, sel.map(function (s) { return s.key.frame; }));
    _keyClip = sel.map(function (s) {
      return { prop: s.prop, dt: s.key.frame - base, v: s.key.v.slice(), hold: !!s.key.hold, curvePoints: cloneCurvePts(s.key.curvePoints || DEFAULT_CURVE), src: holderRefOf(s.holder) };
    });
    return _keyClip.length;
  }
  function pasteKeys() {
    if (!_keyClip || !_keyClip.length) return 0;
    var ld = state.layers[state.activeLayerIdx];
    if (!ld) return 0;
    pushUndo();
    var n = 0;
    var touched = [];
    _keyClip.forEach(function (c) {
      var f = state.currentFrame + c.dt;
      if (f < 0 || f > state.totalFrames - 1) return;
      // Back to the layer it came from. Falling back to the active layer keeps
      // the old behaviour for the cases that relied on it: a single-layer copy
      // deliberately re-pasted somewhere else, and a clipboard whose source
      // layer has since been deleted.
      var target = holderFromRef(c.src) || ld;
      if (touched.indexOf(target) < 0) touched.push(target);
      var track = ensureTrack(target, c.prop);
      var ex = keyAt(track, f);
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
  function keyAt(track, frame) {
    if (!track || !track.keys || !track.keys.length) return null;
    var keys = track.keys, lo = 0, hi = keys.length - 1;
    while (lo <= hi) {
      var mid = (lo + hi) >> 1;
      var keyFrame = keys[mid].frame;
      if (keyFrame < frame) lo = mid + 1;
      else if (keyFrame > frame) hi = mid - 1;
      else return keys[mid];
    }
    return null;
  }
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
  // What a property row should actually DISPLAY right now — the ordinary
  // animated/static value for everything, except a LAYER holder's
  // untouched 'order' row (feedback #215 follow-up), which shows the
  // layer's own natural front-to-back rank (layerOrderAt's fallback, the
  // exact value the render pipeline now sorts by) instead of staticValue's
  // flat 0 placeholder. Shared by the panel's initial build and its own
  // live-drag refresh so the two can never show two different numbers for
  // the same untouched layer.
  function displayValueFor(holder, prop) {
    if (isAnimated(holder, prop)) return valueAtFrame(holder, prop, state.currentFrame);
    if (prop === 'order') {
      var li = state.layers.indexOf(holder);
      if (li >= 0) return [layerOrderAt(li, state.currentFrame)];
    }
    return staticValue(holder, prop);
  }

  // Callers clamp before the first/after the last key, so the remaining
  // query is always inside one segment. Motion evaluation runs this lookup
  // for every animated property and element on every rendered frame: a
  // linear walk made long productions progressively slower even though the
  // keys are already sorted. Keep the interpolation itself unchanged and
  // locate only its left key in O(log n).
  function segmentIndexAtFrame(keys, frame) {
    var lo = 0, hi = keys.length - 2;
    while (lo <= hi) {
      var mid = (lo + hi) >> 1;
      if (frame < keys[mid].frame) hi = mid - 1;
      else if (frame >= keys[mid + 1].frame) lo = mid + 1;
      else return mid;
    }
    return Math.max(0, Math.min(keys.length - 2, lo));
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
    var i = segmentIndexAtFrame(ks, frame);
    var a = ks[i], b = ks[i + 1];
    if (a.hold) return a.v[0];
    var t = (frame - a.frame) / (b.frame - a.frame);
    var y = evalCurvePoints(a.curvePoints || DEFAULT_CURVE, t);
    return a.v[0] + (b.v[0] - a.v[0]) * y;
  }
  function rawValueAtFrame(ld, prop, frame) {
    var track = trackFor(ld, prop);
    if (!track || !track.keys.length) return staticValue(ld, prop);
    var ks = track.keys;
    if (frame <= ks[0].frame) return ks[0].v.slice();
    var last = ks[ks.length - 1];
    if (frame >= last.frame) return last.v.slice();
    var i = segmentIndexAtFrame(ks, frame);
    var a = ks[i], b = ks[i + 1];
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

  // ---- Expression engine (2026-07, rebuilt 2026-08-30) ------------------
  // A property can be driven by code instead of (or on top of) its
  // keyframes. Same "opt-in extended mode on the SAME holder" principle as
  // everything else in this file: a holder's `.expressions` is a THIRD mode
  // next to keyframed and static, not a parallel system. The properties that
  // make this engine what it is:
  //   - Sandbox: `new Function` with an EXPLICIT, closed parameter list —
  //     no window, no document, no state. Everything reachable from an
  //     expression is listed in EXPR_ARG_NAMES and nothing else is.
  //   - Ordinary modern JavaScript: the code compiles through the browser's
  //     own engine, so let/const, arrow functions, destructuring, template
  //     literals, spread and the whole current Math/Array surface work with
  //     no dialect of our own layered on top.
  //   - Frame-native vocabulary: every function that names a moment in the
  //     timeline speaks in FRAMES. See the runtime block below.
  //   - Deterministic randomness: seeded per holder and per property
  //     (ensureExprSeed), never raw Math.random() — same discipline
  //     seededRng (tools.js) already applies to brush-texture dabs, so a
  //     given frame renders identically every time, preview and export.
  //   - Errors never break the render: a throwing expression falls back to
  //     the underlying keyframed/static value (computed BEFORE the
  //     expression runs, passed in as `value`) and records `lastError` for
  //     the UI to show on just that property row — never a whole-scene
  //     failure.
  //   - Runaway code can't take the application with it: loops are
  //     instrumented against a wall-clock budget, and an expression that
  //     trips it switches itself off.
  //   - Compiled once per (holder,prop), cached until the code string
  //     changes — not re-parsed every frame.
  function ensureExpr(holder, prop) {
    if (!holder.expressions) holder.expressions = {};
    if (!holder.expressions[prop]) holder.expressions[prop] = { code: '', enabled: false, lastError: null };
    return holder.expressions[prop];
  }
  function hasExpr(holder, prop) { return !!(holder.expressions && holder.expressions[prop] && holder.expressions[prop].enabled && holder.expressions[prop].code); }
  // ==== Expression runtime (2026-08-30 rebuild) =========================
  // Nemo's expression vocabulary is its own, and it is FRAME-NATIVE: every
  // function that takes or returns a moment in the timeline speaks in
  // FRAMES, the unit this whole application is built on. `time` stays
  // available in seconds for ordinary maths (sines, physical speeds), and
  // toFrames()/toSeconds() convert explicitly when you need to cross over.
  // There is deliberately no place where a number silently means seconds.
  //
  // Everything the sandbox exposes lives BELOW as module-level singletons
  // that read one mutable per-evaluation context (`_ectx`), instead of the
  // per-evaluation closures the first version built. Two reasons, in order
  // of importance:
  //   1. CORRECTNESS. The old makeWiggle() read `state.currentFrame` rather
  //      than the `frame` argument evalExpressionFor was handed. That was
  //      invisible while every caller happened to evaluate the current
  //      frame, and flatly wrong the moment anything samples a property at
  //      another time (self.at(), cross-layer reads, rendering a frame
  //      other than the playhead's). The context carries the evaluation
  //      frame; nothing time-dependent may read state.currentFrame again.
  //   2. COST. Expressions evaluate per property per frame. Rebuilding the
  //      whole binding set on every call would be a real regression over
  //      the three closures the old list allocated; module-level functions
  //      and singleton views allocate none. The only per-call allocations
  //      left are the context object and the argument array.
  // Nesting (layer A's expression sampling layer B, whose own expression
  // samples back) saves/restores `_ectx` and is bounded by _exprDepth.
  var _ectx = null;
  var _exprDepth = 0;
  var EXPR_MAX_DEPTH = 8;
  // Expression records the depth cap flagged during the current outermost
  // evaluation — see the success path in evalExpressionFor for why.
  var _depthTripped = [];
  function _exprFps() { return state.fps || 24; }

  // ---- runaway protection -----------------------------------------------
  // An expression is user code on the UI thread, evaluated for every
  // property on every frame. A mistyped `while (true) {}` would otherwise
  // freeze the whole application with no way out but force-quitting it, and
  // JavaScript gives no way to interrupt a running function from outside.
  // So the loops are instrumented on the way in: instrumentLoops() inserts a
  // call to __tick at the top of every loop BODY, and __tick throws once a
  // wall-clock budget is spent. Sampling the clock (every 1024 crossings)
  // rather than reading it each time keeps the added cost off the radar.
  //
  // Honest about the hole: only loops with a BRACED body are instrumented.
  // `while (true);`, `for (;;);` and `while (x) doThing();` — a loop whose
  // body is a single statement rather than a block — are not reached, and
  // neither is unbounded recursion inside the expression itself. Those still
  // hang. Everything with a `{` after the loop header, which is how loops
  // are actually written, is covered.
  //
  // Tripping the budget doesn't just fail the frame: the expression turns
  // ITSELF off (see evalExpressionFor), so the user gets the application
  // back instead of hitting the same wall on the next frame.
  var EXPR_BUDGET_MS = 60;
  var _exprDeadline = 0;
  var _tickCount = 0;
  var EXPR_TIMEOUT_TAG = '__nemo_expr_timeout__';
  function _exprTick() {
    if (((++_tickCount) & 1023) !== 0) return;
    if (Date.now() > _exprDeadline) throw new Error(EXPR_TIMEOUT_TAG);
  }
  // Skips ahead past a string, template literal or comment starting at i.
  // Returns the index just after it, or -1 if i isn't the start of one.
  function _skipLiteral(src, i) {
    var c = src.charAt(i);
    if (c === '/' && src.charAt(i + 1) === '/') {
      var nl = src.indexOf('\n', i);
      return nl < 0 ? src.length : nl;
    }
    if (c === '/' && src.charAt(i + 1) === '*') {
      var end = src.indexOf('*/', i + 2);
      return end < 0 ? src.length : end + 2;
    }
    if (c === '"' || c === "'" || c === '`') {
      var j = i + 1;
      while (j < src.length) {
        var d = src.charAt(j);
        if (d === '\\') { j += 2; continue; }
        if (d === c) return j + 1;
        j++;
      }
      return src.length;
    }
    return -1;
  }
  function _isIdentChar(c) { return /[A-Za-z0-9_$]/.test(c); }
  function instrumentLoops(src) {
    var out = '', i = 0;
    while (i < src.length) {
      var skipped = _skipLiteral(src, i);
      if (skipped >= 0) { out += src.slice(i, skipped); i = skipped; continue; }
      var rest = src.slice(i);
      var m = /^(while|for|do)\b/.exec(rest);
      var prevOk = (i === 0) || !_isIdentChar(src.charAt(i - 1));
      if (!m || !prevOk) { out += src.charAt(i); i++; continue; }
      var kw = m[1];
      out += kw;
      var j = i + kw.length;
      if (kw !== 'do') {
        // Step over the loop header, balancing parentheses (and skipping any
        // string/comment inside it).
        while (j < src.length && src.charAt(j) !== '(') { out += src.charAt(j); j++; }
        if (j >= src.length) { i = j; continue; }
        var depth = 0;
        while (j < src.length) {
          var sk = _skipLiteral(src, j);
          if (sk >= 0) { out += src.slice(j, sk); j = sk; continue; }
          var ch = src.charAt(j);
          out += ch;
          j++;
          if (ch === '(') depth++;
          else if (ch === ')') { depth--; if (depth === 0) break; }
        }
      }
      // Whatever whitespace/comments separate the header from the body.
      while (j < src.length) {
        var sk2 = _skipLiteral(src, j);
        if (sk2 >= 0) { out += src.slice(j, sk2); j = sk2; continue; }
        if (!/\s/.test(src.charAt(j))) break;
        out += src.charAt(j); j++;
      }
      if (src.charAt(j) === '{') { out += '{ __tick();'; j++; }
      i = j;
    }
    return out;
  }

  // ---- error reporting --------------------------------------------------
  // `new Function` reports positions against ITS OWN wrapper, not against
  // the text the user typed, and the size of that wrapper is an engine
  // detail. So it is measured once, at run time, from a probe whose error
  // line is known — no hardcoded guess that silently drifts.
  var _lineOffset = null;
  function _fnLineOffset() {
    if (_lineOffset !== null) return _lineOffset;
    _lineOffset = 2;
    try {
      // eslint-disable-next-line no-new-func
      Function('"use strict";\nthrow new Error("probe");')();
    } catch (e) {
      var mm = /<anonymous>:(\d+):/.exec(e.stack || '');
      if (mm) _lineOffset = parseInt(mm[1], 10) - 2; // probe throws on body line 2
    }
    return _lineOffset;
  }
  // `exprMode` is the bare-expression wrapper (the one that adds a `return (`
  // line); `userLines` is how many lines the user actually typed.
  // Both corrections below were measured, not assumed:
  //   - In the bare-expression wrapper an error on the user's FIRST line is
  //     attributed to the `return (` line we added, landing on 0. Every
  //     other position comes out exact.
  //   - Anything that still falls outside the text the user typed is
  //     reported as "no line" rather than as a number pointing nowhere. A
  //     wrong line is worse than none.
  function _lineFromStack(stack, prefixLines, userLines, exprMode) {
    if (!stack) return -1;
    var mm = /<anonymous>:(\d+):/.exec(stack);
    if (!mm) return -1;
    var line = parseInt(mm[1], 10) - _fnLineOffset() - prefixLines;
    if (line === 0 && exprMode) line = 1;
    if (line < 1) return -1;
    if (userLines && line > userLines) return -1;
    return line;
  }
  function _editDistance(a, b) {
    var m = a.length, n = b.length;
    if (Math.abs(m - n) > 2) return 99;
    var prev = [], cur = [], i, j;
    for (j = 0; j <= n; j++) prev[j] = j;
    for (i = 1; i <= m; i++) {
      cur[0] = i;
      for (j = 1; j <= n; j++) {
        cur[j] = Math.min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (a.charAt(i - 1) === b.charAt(j - 1) ? 0 : 1));
      }
      for (j = 0; j <= n; j++) prev[j] = cur[j];
    }
    return prev[n];
  }
  // Closest documented name to something the expression referenced but that
  // doesn't exist. Only Nemo's own vocabulary is suggested — never the
  // compatibility aliases, which stay invisible.
  function _closestName(name) {
    var best = null, bestD = 3;
    var lower = String(name).toLowerCase();
    for (var i = 0; i < EXPR_PUBLIC_NAMES.length; i++) {
      var cand = EXPR_PUBLIC_NAMES[i];
      var d = _editDistance(lower, cand.toLowerCase());
      if (d < bestD) { bestD = d; best = cand; }
    }
    return best;
  }
  function _formatRuntimeError(e, entry) {
    var msg = SM.t('exprErrorPrefix') + (e && e.message ? e.message : String(e));
    var line = _lineFromStack(e && e.stack, entry ? entry.prefixLines : 0,
      entry ? entry.userLines : 0, entry ? entry.exprMode : false);
    var undef = /(?:^|\s)([A-Za-z_$][A-Za-z0-9_$]*) is not defined/.exec((e && e.message) || '');
    if (undef) {
      var near = _closestName(undef[1]);
      if (near) msg += SM.t('exprErrorDidYouMeanPrefix') + near + SM.t('exprErrorDidYouMeanSuffix');
    }
    if (line > 0) msg += SM.t('exprErrorLineSuffixPrefix') + line + SM.t('exprErrorLineSuffixEnd');
    return { message: msg, line: line };
  }

  // Stable per-holder random seed for wiggle()/random() — NOT persisted
  // (deliberately absent from serP/serR's field list, see app.js), so it's
  // only stable WITHIN a session; a reload reseeds. A fully save-stable seed
  // would need threading through serP/desP same as strokeId, a reasonable
  // follow-up if "the shake looks different after reopening the project" is
  // ever reported, but not needed here (the shape of the motion is what
  // matters, not bit-for-bit identical noise across sessions).
  function ensureExprSeed(holder) {
    if (holder._exprSeed === undefined) holder._exprSeed = Math.floor(Math.random() * 1e9);
    return holder._exprSeed;
  }
  // Per-property offset so two properties on the SAME holder don't draw the
  // same random stream.
  function _propSeedOffset(prop) {
    var h = 0;
    for (var i = 0; i < prop.length; i++) h = (h * 31 + prop.charCodeAt(i)) | 0;
    return h;
  }
  // Tiny deterministic hash (not cryptographic, doesn't need to be) — the
  // same value for the same (seed, n) every time. Extracted from the inner
  // h() hashNoise1D used to define privately, so the smooth noise below and
  // the uniform draw above share ONE definition instead of drifting
  // (CLAUDE.md §3's duplicated-pair hazard).
  function hashUnit(seed, n) {
    var v = Math.sin(n * 12.9898 + seed * 78.233) * 43758.5453;
    return v - Math.floor(v);
  }
  // Smoothly interpolated 1D value noise, so wiggle() reads as continuous
  // motion rather than a stepped random walk.
  function hashNoise1D(seed, x) {
    var i = Math.floor(x), f = x - i;
    var a = hashUnit(seed, i), b = hashUnit(seed, i + 1);
    var t = f * f * (3 - 2 * f); // smoothstep
    return a + (b - a) * t;
  }
  // 2D counterpart — bilinear blend of the same lattice, used by noise([x,y]).
  function hashNoise2D(seed, x, y) {
    var xi = Math.floor(x), yi = Math.floor(y), xf = x - xi, yf = y - yi;
    function g(a, b) { return hashUnit(seed, a + b * 311.7); }
    var u = xf * xf * (3 - 2 * xf), v = yf * yf * (3 - 2 * yf);
    var n00 = g(xi, yi), n10 = g(xi + 1, yi), n01 = g(xi, yi + 1), n11 = g(xi + 1, yi + 1);
    var a = n00 + (n10 - n00) * u, b = n01 + (n11 - n01) * u;
    return a + (b - a) * v;
  }

  // ---- shared value helpers (scalar-or-array) --------------------------
  // Every property is either 1-dimensional (a bare number to user code) or
  // 2-dimensional (an [x, y] array). Each helper below accepts both and
  // gives back the same shape it was handed, so `remap(t, 0, 1, [0,0],
  // [100,50])` and `remap(t, 0, 1, 0, 100)` are both ordinary usage.
  function _vec(v) { return Array.isArray(v) ? v : [Number(v) || 0]; }
  function _num(v) { var n = Number(v); return isFinite(n) ? n : 0; }
  function _vecOp(a, b, f) {
    var A = _vec(a), B = _vec(b), n = Math.max(A.length, B.length), out = [];
    for (var i = 0; i < n; i++) out.push(f(_num(A[i]), _num(B[i])));
    return (!Array.isArray(a) && !Array.isArray(b)) ? out[0] : out;
  }
  function _mixVals(v1, v2, k) {
    if (Array.isArray(v1) || Array.isArray(v2)) {
      var A = _vec(v1), B = _vec(v2), n = Math.max(A.length, B.length), out = [];
      for (var i = 0; i < n; i++) {
        var x = _num(A[i]), y = _num(B[i]);
        out.push(x + (y - x) * k);
      }
      return out;
    }
    return _num(v1) + (_num(v2) - _num(v1)) * k;
  }
  // Both call shapes of the four remap helpers in one place:
  //   remap(v, toLo, toHi)                    — v read on a 0..1 scale
  //   remap(v, fromLo, fromHi, toLo, toHi)    — v rescaled from its own range
  function _remapArgs(a) {
    if (a.length >= 4) return { t: _num(a[0]), tMin: _num(a[1]), tMax: _num(a[2]), v1: a[3], v2: a[4] };
    return { t: _num(a[0]), tMin: 0, tMax: 1, v1: a[1], v2: a[2] };
  }
  function _normT(t, tMin, tMax) {
    if (tMax === tMin) return t <= tMin ? 0 : 1;
    var k = (t - tMin) / (tMax - tMin);
    return k < 0 ? 0 : (k > 1 ? 1 : k);
  }
  // Four blend shapes over the same remap: straight, soft at both ends, soft
  // at the start only, soft at the end only. Input outside [fromLo, fromHi]
  // is clamped, so the result never overshoots the output range.
  function exprRemap() { var p = _remapArgs(arguments); return _mixVals(p.v1, p.v2, _normT(p.t, p.tMin, p.tMax)); }
  function exprRemapEase() { var p = _remapArgs(arguments); var k = _normT(p.t, p.tMin, p.tMax); return _mixVals(p.v1, p.v2, k * k * (3 - 2 * k)); }
  function exprRemapEaseIn() { var p = _remapArgs(arguments); var k = _normT(p.t, p.tMin, p.tMax); return _mixVals(p.v1, p.v2, -k * k * k + 2 * k * k); }
  function exprRemapEaseOut() { var p = _remapArgs(arguments); var k = _normT(p.t, p.tMin, p.tMax); return _mixVals(p.v1, p.v2, -k * k * k + k * k + k); }
  function exprClamp(v, lo, hi) {
    if (Array.isArray(v)) {
      var out = [];
      for (var i = 0; i < v.length; i++) {
        out.push(exprClamp(v[i], Array.isArray(lo) ? lo[i] : lo, Array.isArray(hi) ? hi[i] : hi));
      }
      return out;
    }
    var lo2 = _num(lo), hi2 = _num(hi);
    if (hi2 < lo2) { var s = lo2; lo2 = hi2; hi2 = s; }
    return Math.min(Math.max(_num(v), lo2), hi2);
  }
  function exprRadians(d) { if (Array.isArray(d)) return d.map(exprRadians); return _num(d) * Math.PI / 180; }
  function exprDegrees(r) { if (Array.isArray(r)) return r.map(exprDegrees); return _num(r) * 180 / Math.PI; }

  // ---- vector maths ----------------------------------------------------
  function exprAdd(a, b) { return _vecOp(a, b, function (x, y) { return x + y; }); }
  function exprSub(a, b) { return _vecOp(a, b, function (x, y) { return x - y; }); }
  function exprMul(a, s) {
    if (Array.isArray(s)) return _vecOp(a, s, function (x, y) { return x * y; });
    var A = _vec(a), k = _num(s), out = [];
    for (var i = 0; i < A.length; i++) out.push(_num(A[i]) * k);
    return Array.isArray(a) ? out : out[0];
  }
  function exprDiv(a, s) {
    if (Array.isArray(s)) return _vecOp(a, s, function (x, y) { return y === 0 ? 0 : x / y; });
    var A = _vec(a), k = _num(s), out = [];
    for (var i = 0; i < A.length; i++) out.push(k === 0 ? 0 : _num(A[i]) / k);
    return Array.isArray(a) ? out : out[0];
  }
  function exprDot(a, b) {
    var A = _vec(a), B = _vec(b), n = Math.max(A.length, B.length), s = 0;
    for (var i = 0; i < n; i++) s += _num(A[i]) * _num(B[i]);
    return s;
  }
  // One argument: the vector's own magnitude. Two: the distance between two
  // points — the form that actually gets typed ("how far apart are these").
  function exprLength(a, b) {
    if (b === undefined) {
      var A = _vec(a), s = 0;
      for (var i = 0; i < A.length; i++) s += _num(A[i]) * _num(A[i]);
      return Math.sqrt(s);
    }
    return exprLength(exprSub(a, b));
  }
  function exprNormalize(a) {
    var L = exprLength(a);
    if (!L) { var A = _vec(a), z = []; for (var i = 0; i < A.length; i++) z.push(0); return Array.isArray(a) ? z : 0; }
    return exprDiv(a, L);
  }
  // 3-component inputs give the usual 3-component cross product. Nemo's
  // properties are 2D, and there the cross product only ever has a Z
  // component, so a 2D pair gives back that number directly rather than an
  // [0, 0, z] array nothing in a 2D property could consume.
  function exprCross(a, b) {
    var A = _vec(a), B = _vec(b);
    if (A.length >= 3 || B.length >= 3) {
      return [_num(A[1]) * _num(B[2]) - _num(A[2]) * _num(B[1]),
        _num(A[2]) * _num(B[0]) - _num(A[0]) * _num(B[2]),
        _num(A[0]) * _num(B[1]) - _num(A[1]) * _num(B[0])];
    }
    return _num(A[0]) * _num(B[1]) - _num(A[1]) * _num(B[0]);
  }
  // The angle, IN DEGREES, from one point towards another — the unit
  // Rotation itself uses (PROP_UNIT.rotation === '°'), so the result can be
  // returned straight from a Rotation expression with no conversion.
  function exprAngleTo(from, to) {
    var A = _vec(from), B = _vec(to);
    return Math.atan2(_num(B[1]) - _num(A[1]), _num(B[0]) - _num(A[0])) * 180 / Math.PI;
  }

  // ---- time -------------------------------------------------------------
  // stepTime(n) snaps the evaluation clock to every n FRAMES, in place.
  // Every later call in the SAME expression that depends on the clock —
  // wiggle, noise, random, self.at(), layer(), the loops — then sees the
  // snapped frame, so one call at the top turns a whole expression into
  // stepped motion. n may be fractional.
  // Known limit, stated rather than hidden: the bare `time`/`frame`
  // variables are ordinary function arguments, bound by value before user
  // code runs, so they keep their un-snapped values. The snapped FRAME is
  // returned for exactly that reason — `var f = stepTime(4);` hands you a
  // stepped clock to do arithmetic with.
  function exprStepTime(everyNFrames) {
    var ctx = _ectx;
    if (!ctx) return 0;
    var n = Number(everyNFrames);
    if (!isFinite(n) || n <= 0) return ctx.frame;
    ctx.frame = Math.floor(ctx.frame / n) * n;
    ctx.time = ctx.frame / ctx.fps;
    return ctx.frame;
  }
  function exprToFrames(seconds) {
    var ctx = _ectx;
    var s = (seconds === undefined) ? (ctx ? ctx.time : 0) : _num(seconds);
    return s * (ctx ? ctx.fps : _exprFps());
  }
  function exprToSeconds(frames) {
    var ctx = _ectx;
    var f = (frames === undefined) ? (ctx ? ctx.frame : 0) : _num(frames);
    var r = ctx ? ctx.fps : _exprFps();
    return r === 0 ? 0 : f / r;
  }

  // ---- randomness -------------------------------------------------------
  // The stream is stable per holder AND per property (so two properties
  // never shake in lockstep) unless seed(n) picks an explicit one. random()
  // re-draws every frame; randomFixed() draws once and holds that value for
  // the whole timeline, which is how you give each element of a set its own
  // permanent offset. Same pair for the bell-curve versions.
  function exprSeed(n) {
    var ctx = _ectx;
    if (!ctx) return;
    ctx.rngSeed = _num(n) + 1;
    ctx.rngCounter = 0;
  }
  function _rand01(fixed) {
    _exprTick();
    var ctx = _ectx;
    if (!ctx) return 0;
    var tPart = (fixed || ctx.rngTimeless) ? 0 : Math.round(ctx.frame * 1000) / 1000;
    return hashUnit(ctx.rngSeed, tPart * 1013.13 + (ctx.rngCounter++) * 7919 + 0.5);
  }
  // Bell-shaped counterpart (Box-Muller), centred on 0.5 with a spread that
  // keeps roughly nine draws in ten inside 0..1.
  function _gauss01(fixed) {
    var u1 = Math.max(1e-9, _rand01(fixed)), u2 = _rand01(fixed);
    return 0.5 + 0.304 * Math.sqrt(-2 * Math.log(u1)) * Math.cos(2 * Math.PI * u2);
  }
  // Shared by all four draw functions: no args = the raw 0..1 draw, one arg
  // = 0..max, two = min..max, and either bound may be an array for a
  // per-axis range.
  function _randomWith(gen, a, b) {
    if (a === undefined) return gen();
    if (b === undefined) {
      if (Array.isArray(a)) { var o = []; for (var i = 0; i < a.length; i++) o.push(gen() * _num(a[i])); return o; }
      return gen() * _num(a);
    }
    if (Array.isArray(a) || Array.isArray(b)) {
      var A = _vec(a), B = _vec(b), n = Math.max(A.length, B.length), out = [];
      for (var j = 0; j < n; j++) { var lo = _num(A[j]), hi = _num(B[j]); out.push(lo + gen() * (hi - lo)); }
      return out;
    }
    var l = _num(a), h = _num(b);
    return l + gen() * (h - l);
  }
  function _rndVary() { return _rand01(false); }
  function _rndFixed() { return _rand01(true); }
  function _gaussVary() { return _gauss01(false); }
  function _gaussFixed() { return _gauss01(true); }
  function exprRandom(a, b) { return _randomWith(_rndVary, a, b); }
  function exprRandomFixed(a, b) { return _randomWith(_rndFixed, a, b); }
  function exprRandomGauss(a, b) { return _randomWith(_gaussVary, a, b); }
  function exprRandomGaussFixed(a, b) { return _randomWith(_gaussFixed, a, b); }
  // Smooth noise in -1..1 for a 1D or 2D input — unlike random(), nearby
  // inputs give nearby results, which is what makes it usable as motion.
  function exprNoise(x, y) {
    _exprTick();
    var ctx = _ectx;
    if (!ctx) return 0;
    if (Array.isArray(x)) { y = x[1]; x = x[0]; }
    if (y === undefined) return hashNoise1D(ctx.seed + 7717, _num(x)) * 2 - 1;
    return hashNoise2D(ctx.seed + 7717, _num(x), _num(y)) * 2 - 1;
  }

  // ---- wiggle -----------------------------------------------------------
  // dim is the PROPERTY's PROP_DIM (1 for rotation/opacity, 2 for
  // position/anchor/scale) — NOT a single axis index. A 1D property gets a
  // bare number back (axis 0's own noise stream); a 2D one gets [wx, wy]
  // IN ONE CALL, each axis its own independent noise stream (offsetting the
  // seed by axis, same idea as the octave loop) so X and Y don't move in
  // lockstep along a diagonal line — this is what lets the examples menu
  // write plain `value[0] + wiggle(2,10)[0]` / `[1]` instead of calling
  // wiggle() twice with no way to correlate/decorrelate the axes from user
  // code (bug found via feedback #134: the single-scalar wiggle() this
  // replaced could never produce a valid [x,y] for a 2D property in the
  // first place — `value + wiggle(...)` string-concats an array with a
  // number in JS, it doesn't add per-axis).
  function _wiggleAxis(ctx, axis, freqPerSec, amp, octaves) {
    var t = ctx.time; // the EVALUATION frame's time, never state.currentFrame
    var n = 0, amp2 = 1, freq2 = 1, norm = 0;
    for (var o = 0; o < octaves; o++) {
      n += (hashNoise1D(ctx.seed + axis * 101 + o * 977, t * freqPerSec * freq2) - 0.5) * 2 * amp2;
      norm += amp2; amp2 *= 0.5; freq2 *= 2;
    }
    return (n / norm) * amp;
  }
  function exprWiggle(freqPerSec, amp, octaves) {
    _exprTick();
    var ctx = _ectx;
    if (!ctx) return 0;
    octaves = Math.max(1, octaves || 1);
    if ((PROP_DIM[ctx.prop] || 1) === 2) {
      return [_wiggleAxis(ctx, 0, freqPerSec, amp, octaves), _wiggleAxis(ctx, 1, freqPerSec, amp, octaves)];
    }
    return _wiggleAxis(ctx, 0, freqPerSec, amp, octaves);
  }

  // ---- sampling this property at another frame --------------------------
  // self.at() reads the RAW (pre-expression) track deliberately. That is
  // both what makes "where was I a moment ago" meaningful — sampling the
  // post-expression value would feed the expression its own output — and
  // what makes infinite self-recursion structurally impossible rather than
  // merely guarded against.
  function _rawShaped(holder, prop, frame) {
    var v = rawValueAtFrame(holder, prop, frame);
    return v.length === 1 ? v[0] : v.slice();
  }
  function exprSelfAt(f) {
    _exprTick();
    var ctx = _ectx;
    if (!ctx) return 0;
    return _rawShaped(ctx.holder, ctx.prop, f === undefined ? ctx.frame : _num(f));
  }
  // Rate of change in units per SECOND, as a central difference across one
  // frame. Honest about what it is: a numeric derivative of the sampled
  // track, not an analytic one. Per second rather than per frame because
  // that is what "speed" means once you compare two projects at different
  // frame rates.
  function exprSelfVelocity(f) {
    var ctx = _ectx;
    if (!ctx) return 0;
    var ff = (f === undefined) ? ctx.frame : _num(f);
    var a = rawValueAtFrame(ctx.holder, ctx.prop, ff - 0.5);
    var b = rawValueAtFrame(ctx.holder, ctx.prop, ff + 0.5);
    var out = [];
    for (var i = 0; i < b.length; i++) out.push((b[i] - (a[i] || 0)) * ctx.fps);
    return out.length === 1 ? out[0] : out;
  }
  function exprSelfSpeed(f) {
    var v = exprSelfVelocity(f);
    if (!Array.isArray(v)) return Math.abs(v);
    var s = 0;
    for (var i = 0; i < v.length; i++) s += v[i] * v[i];
    return Math.sqrt(s);
  }

  // ---- looping ----------------------------------------------------------
  // ONE implementation for both directions and all four shapes; loopAfter/
  // loopBefore are thin wrappers over it, so a fix to the wrap maths can
  // never apply to only half of them. "After"/"before" means after the LAST
  // keyframe / before the FIRST one — deliberately not "in"/"out", which in
  // Nemo already mean a layer's in and out points.
  //   'cycle'    — replay the keyed range over and over
  //   'pingpong' — replay it forwards, then backwards, alternating
  //   'offset'   — replay it, each pass starting where the last one ended
  //   'continue' — no replay: keep going in a straight line at the speed the
  //                property had when it ran out of keyframes
  // `keyframes` picks how much of the track repeats: 0 (the default) uses
  // the whole thing, N uses the last (or first) N segments.
  function loopRaw(holder, prop, frame, mode, keyframes, forward) {
    _exprTick();
    var track = trackFor(holder, prop);
    if (!track || !track.keys || track.keys.length < 2) return rawValueAtFrame(holder, prop, frame);
    var keys = track.keys;
    var firstF = keys[0].frame, lastF = keys[keys.length - 1].frame;
    // Inside the keyed range nothing loops — the real keyframes win.
    if (forward ? frame <= lastF : frame >= firstF) return rawValueAtFrame(holder, prop, frame);
    var m = String(mode == null ? 'cycle' : mode).toLowerCase();
    var n = Math.max(0, Math.floor(_num(keyframes)));
    var segStart, segEnd;
    if (forward) {
      segEnd = lastF;
      segStart = (n > 0 && n < keys.length) ? keys[keys.length - 1 - n].frame : firstF;
    } else {
      segStart = firstF;
      segEnd = (n > 0 && n < keys.length) ? keys[n].frame : lastF;
    }
    if (m === 'continue') {
      // Straight-line extrapolation off the outermost keyframe, at the slope
      // the last real frame of animation had.
      var edge = forward ? lastF : firstF;
      var inner = forward ? (lastF - 1) : (firstF + 1);
      var ve = rawValueAtFrame(holder, prop, edge);
      var vi = rawValueAtFrame(holder, prop, inner);
      var dist = forward ? (frame - lastF) : (firstF - frame);
      var outC = [];
      for (var c = 0; c < ve.length; c++) outC.push(ve[c] + (ve[c] - (vi[c] || 0)) * dist);
      return outC;
    }
    var span = segEnd - segStart;
    if (span <= 0) return rawValueAtFrame(holder, prop, frame);
    var d = forward ? (frame - segStart) : (segEnd - frame);
    var cycles = Math.floor(d / span);
    var r = d - cycles * span;
    if (m === 'pingpong' && (cycles % 2) === 1) r = span - r;
    var base = rawValueAtFrame(holder, prop, forward ? (segStart + r) : (segEnd - r));
    if (m !== 'offset') return base;
    var from = rawValueAtFrame(holder, prop, forward ? segStart : segEnd);
    var to = rawValueAtFrame(holder, prop, forward ? segEnd : segStart);
    var outO = [];
    for (var k = 0; k < base.length; k++) outO.push(base[k] + ((to[k] || 0) - (from[k] || 0)) * cycles);
    return outO;
  }
  function exprLoopAfter(mode, keyframes) {
    var ctx = _ectx;
    if (!ctx) return 0;
    var v = loopRaw(ctx.holder, ctx.prop, ctx.frame, mode, keyframes, true);
    return v.length === 1 ? v[0] : v.slice();
  }
  function exprLoopBefore(mode, keyframes) {
    var ctx = _ectx;
    if (!ctx) return 0;
    var v = loopRaw(ctx.holder, ctx.prop, ctx.frame, mode, keyframes, false);
    return v.length === 1 ? v[0] : v.slice();
  }

  // ---- other layers -----------------------------------------------------
  // Read-only snapshot of another layer's effective values.
  // `ref` is whatever the user typed into layer(...) inside an expression —
  // 2026-08-16, Cyril: "agrémenter la library d'expressions... ID layers
  // peut être". The ONLY thing layer() ever accepted was the internal uid
  // string (ly_xxxxx), which is generated at layer-creation time and never
  // shown anywhere in the UI — there is no field to copy it from, so this
  // function was reachable in code but not in practice usable by a human
  // writing an expression. Layer NAME is what's actually visible (the
  // layer-list row, the Motion panel header), so it's tried FIRST; the raw
  // uid is still accepted after, so nothing written before this change
  // (or generated by some future name-independent tool) breaks. First
  // match wins on a duplicate name, the same "good enough, not
  // ambiguity-safe" tradeoff any hand-typed layer("name") reference has.
  function findLayerIndexByRef(ref) {
    if (!ref) return -1;
    for (var i = 0; i < state.layers.length; i++) if (state.layers[i].name === ref) return i;
    return findLayerIndexByUid(ref);
  }
  // Attaches .at(frame) to the ARRAY a 2D property hands back. An array can
  // carry properties without ceasing to be an array, so
  // `layer('Ball').position` keeps working exactly as before while
  // `layer('Ball').position.at(f)` becomes available. 1D properties are bare
  // numbers and cannot carry methods, which is why the snapshot ALSO gets a
  // uniform at(prop, frame) covering every property.
  function _withSampler(arr, ld2, prop) {
    try {
      arr.at = function (f) { return _sampleOther(ld2, prop, _num(f)); };
    } catch (e) { /* frozen array: the plain values still work */ }
    return arr;
  }
  function _sampleOther(ld2, prop, frame) {
    var v = valueAtFrame(ld2, prop, frame);
    return v.length === 1 ? v[0] : v.slice();
  }
  function layerSnapshot(ref, frame) {
    var idx = findLayerIndexByRef(ref);
    if (idx < 0) return null;
    var ld2 = state.layers[idx];
    return {
      position: _withSampler(valueAtFrame(ld2, 'position', frame), ld2, 'position'),
      anchor: _withSampler(valueAtFrame(ld2, 'anchor', frame), ld2, 'anchor'),
      rotation: valueAtFrame(ld2, 'rotation', frame)[0],
      scale: _withSampler(valueAtFrame(ld2, 'scale', frame), ld2, 'scale'),
      opacity: valueAtFrame(ld2, 'opacity', frame)[0],
      name: ld2.name,
      index: idx,
      inPoint: window.layerInPoint ? layerInPoint(ld2) : 0,
      outPoint: window.layerOutPoint ? layerOutPoint(ld2) : (state.totalFrames - 1),
      hasParent: !!ld2.parentLayerUid,
      parent: ld2.parentLayerUid ? (function () {
        var pi = findLayerIndexByUid(ld2.parentLayerUid);
        return pi >= 0 ? { name: state.layers[pi].name, index: pi } : null;
      })() : null,
      // Uniform accessor — the only form that also works for 1D properties.
      at: function (prop, f) { return _sampleOther(ld2, prop, _num(f)); },
      marker: _markerApi(ld2.markers),
      // That layer's own expression controls — the cross-layer half of the
      // rig ("ten layers read one slider"). Defaults to the frame this
      // snapshot was taken at, like every other field here; an explicit
      // second argument samples the control at another frame.
      control: function (name, f) { return _controlValue(ld2, name, f === undefined ? frame : _num(f)); },
    };
  }
  function exprLayer(ref) {
    _exprTick();
    var ctx = _ectx;
    return layerSnapshot(ref, ctx ? ctx.frame : state.currentFrame);
  }

  // ---- expression controls ----------------------------------------------
  // A layer's named parameters, read as `self.control('Wave amount')` for
  // this layer's own and `layer('Ctrl').control('Wave amount')` for another
  // layer's (with a bare `control(...)` as the self shorthand, matching how
  // wiggle/loopAfter are already implicitly about the property they sit on).
  // The name is whatever the user typed in the panel, so it resolves trimmed
  // and case-insensitively — the same rule findControl applies at creation
  // time, so what the panel refuses as a duplicate is exactly what an
  // expression can't ambiguously mean.
  //
  // An unknown name THROWS rather than quietly returning zero: the engine's
  // own degrade contract then does the right thing with it — the message
  // lands on that one property's row and the property falls back to its raw
  // keyframed/static value. Deleting a referenced control therefore produces
  // a visible, located error and a still-rendering scene, never a crash and
  // never a silent zero that looks like a working rig gone limp.
  // Read through valueAtFrame (not rawValueAtFrame) so a control can itself
  // carry an expression; _exprDepth bounds any chain of those exactly as it
  // does for cross-layer property reads.
  function _controlValue(ld, name, frame) {
    var c = findControl(ld, name);
    if (!c) throw new Error(SM.t('exprErrorUnknownControlPrefix') + String(name) + SM.t('exprErrorUnknownControlSuffix'));
    var v = valueAtFrame(ld, c.key, frame);
    return v.length === 1 ? v[0] : v.slice();
  }
  // Controls live on the LAYER, so a per-shape (element) holder's expression
  // reaches its OWNING layer's controls — which is what makes "one slider,
  // every shape inside this layer" work without duplicating the control.
  function exprSelfControl(name, f) {
    _exprTick();
    var ctx = _ectx;
    if (!ctx) return 0;
    var ld = _selfLd();
    if (!ld) throw new Error(SM.t('exprErrorControlNoLayer'));
    return _controlValue(ld, name, f === undefined ? ctx.frame : _num(f));
  }
  // layerControl(uid, name) — the LEAN cross-layer control read
  // (2026-08-30, added with the rig widgets). `layer(uid).control(name)`
  // returns the identical number, but gets there by building a whole
  // layerSnapshot first: a name scan over every layer, SIX valueAtFrame
  // calls (position/anchor/rotation/scale/opacity plus the parent lookup),
  // a _withSampler closure per 2D property and two more closures for
  // at()/marker(). All of it discarded, every read. A rig is MANY reads per
  // scrub tick — every driven property on every driven layer, on the
  // playback hot path (CLAUDE.md §5bis) — so the widget wiring emits this
  // form instead, and so does the expression pickwhip when it lands on a
  // control row.
  //
  // Uid FIRST (findLayerIndexByRef scans names first, which is the wrong
  // order for the form we emit), name second so a hand-typed
  // layerControl("Ctrl", "Turn") still resolves. Unknown layer THROWS, the
  // same contract _controlValue already has for an unknown control name:
  // the engine's degrade path puts the message on that one property row and
  // falls back to its raw value, rather than a silent zero that looks like
  // a working rig gone limp.
  function exprLayerControl(ref, name, f) {
    _exprTick();
    var ctx = _ectx;
    var idx = findLayerIndexByUid(ref);
    if (idx < 0) idx = findLayerIndexByRef(ref);
    if (idx < 0) throw new Error(SM.t('exprErrorUnknownLayerPrefix') + String(ref) + SM.t('exprErrorUnknownLayerSuffix'));
    return _controlValue(state.layers[idx], name, f === undefined ? (ctx ? ctx.frame : state.currentFrame) : _num(f));
  }

  // ---- keyframe introspection ------------------------------------------
  // Scoped to the CURRENT property's own track (the one the expression lives
  // on) — self.keys.at(1) inside a Position expression means Position's
  // first keyframe. 1-indexed, matching how keyframes are counted in the
  // timeline UI rather than the internal 0-based array. Returns null (never
  // throws) for an out-of-range index or an empty track, same "fall through
  // to the safe value" contract as layer() returning null for an unresolved
  // reference.
  function exprKeyAt(holder, prop, i) {
    var track = trackFor(holder, prop);
    if (!track || !track.keys || i < 1 || i > track.keys.length) return null;
    var k = track.keys[i - 1];
    return { frame: k.frame, time: k.frame / _exprFps(), value: k.v.length === 1 ? k.v[0] : k.v.slice(), index: i };
  }
  function exprNumKeys(holder, prop) {
    var track = trackFor(holder, prop);
    return (track && track.keys) ? track.keys.length : 0;
  }
  function nearestKeyIndex(keys, targetFrame) {
    var lo = 0, hi = keys.length;
    while (lo < hi) {
      var mid = (lo + hi) >> 1;
      if (keys[mid].frame < targetFrame) lo = mid + 1;
      else hi = mid;
    }
    if (lo === 0) return 0;
    if (lo === keys.length) return keys.length - 1;
    var prevDistance = targetFrame - keys[lo - 1].frame;
    var nextDistance = keys[lo].frame - targetFrame;
    // Preserve the historical linear scan's tie behavior: because it only
    // replaced the winner on a strictly smaller distance, the earlier key
    // won when the target sat exactly halfway between two keys.
    return nextDistance < prevDistance ? lo : lo - 1;
  }
  // Takes a FRAME (see the frame-native rule at the top of this section).
  function exprNearestKeyAtFrame(holder, prop, frame) {
    var track = trackFor(holder, prop);
    if (!track || !track.keys || !track.keys.length) return null;
    return exprKeyAt(holder, prop, nearestKeyIndex(track.keys, frame) + 1);
  }

  // ---- markers ----------------------------------------------------------
  // Both marker scopes are stored as {frame, name, color} (markers.js).
  // Same count/at/nearest shape as keyframes, so learning one teaches the
  // other. at() also accepts a marker's name, which is usually how you mean
  // to find one.
  function _markerEntry(list, i) {
    if (!list || i < 1 || i > list.length) return null;
    var m = list[i - 1];
    return { frame: m.frame, time: m.frame / _exprFps(), name: m.name || '', color: m.color || null, index: i };
  }
  function _markerApi(list) {
    var arr = list || [];
    return {
      count: arr.length,
      at: function (i) {
        if (typeof i === 'string') {
          for (var j = 0; j < arr.length; j++) if ((arr[j].name || '') === i) return _markerEntry(arr, j + 1);
          return null;
        }
        return _markerEntry(arr, Math.floor(_num(i)));
      },
      nearest: function (frame) {
        if (!arr.length) return null;
        var target = _num(frame);
        var best = 0, bestD = Infinity;
        for (var j = 0; j < arr.length; j++) {
          var d = Math.abs(arr[j].frame - target);
          if (d < bestD) { bestD = d; best = j; }
        }
        return _markerEntry(arr, best + 1);
      },
    };
  }

  // ---- `comp` and `self` views ------------------------------------------
  // Both are module-level SINGLETONS whose accessors read `_ectx`/`state` at
  // the moment they're touched, not per-evaluation objects. That keeps the
  // hot path allocation-free, and the expensive parts (resolving which layer
  // a per-shape holder belongs to; building a marker list) only run if an
  // expression actually reads them.
  function resolveHolderLayer(holder) {
    var li = state.layers.indexOf(holder);
    if (li >= 0) return { ld: holder, index: li, strokeId: null };
    for (var i = 0; i < state.layers.length; i++) {
      var em = state.layers[i].elementMotion;
      if (!em) continue;
      for (var k in em) if (em[k] === holder) return { ld: state.layers[i], index: i, strokeId: k };
    }
    return null;
  }
  // Cached per evaluation: the scan above is O(layers × shapes) and several
  // `self` fields want the same answer.
  function _selfResolved() {
    var ctx = _ectx;
    if (!ctx) return null;
    if (ctx._resolved === undefined) ctx._resolved = resolveHolderLayer(ctx.holder);
    return ctx._resolved;
  }
  function _selfLd() { var r = _selfResolved(); return r ? r.ld : null; }
  var COMP_VIEW = {};
  Object.defineProperty(COMP_VIEW, 'width', { get: function () { return state.canvasW; } });
  Object.defineProperty(COMP_VIEW, 'height', { get: function () { return state.canvasH; } });
  Object.defineProperty(COMP_VIEW, 'fps', { get: function () { return _exprFps(); } });
  Object.defineProperty(COMP_VIEW, 'frames', { get: function () { return state.totalFrames; } });
  Object.defineProperty(COMP_VIEW, 'layers', { get: function () { return state.layers.length; } });
  Object.defineProperty(COMP_VIEW, 'name', {
    get: function () {
      if (state.activeSymbolId && state.symbols && state.symbols[state.activeSymbolId]) {
        return state.symbols[state.activeSymbolId].name || 'Component';
      }
      return 'Scene';
    },
  });
  Object.defineProperty(COMP_VIEW, 'marker', { get: function () { return _markerApi(state.markers); } });
  // self.keys — the current property's own keyframes.
  var SELF_KEYS = {
    at: function (i) { var c = _ectx; return c ? exprKeyAt(c.holder, c.prop, Math.floor(_num(i))) : null; },
    nearest: function (f) { var c = _ectx; return c ? exprNearestKeyAtFrame(c.holder, c.prop, _num(f)) : null; },
  };
  Object.defineProperty(SELF_KEYS, 'count', { get: function () { var c = _ectx; return c ? exprNumKeys(c.holder, c.prop) : 0; } });
  var SELF_VIEW = {
    at: exprSelfAt,
    velocity: exprSelfVelocity,
    speed: exprSelfSpeed,
    keys: SELF_KEYS,
    control: exprSelfControl,
  };
  // ---- Mesh-aware `self` (2026-08-30) -------------------------------
  // Image meshes became animatable per vertex, but an expression had no way
  // to know WHICH vertex it was running on — so the only way to animate a
  // mesh was to key every vertex by hand, which for a 10-column mesh is 75
  // tracks. With the index and the vertex's own rest position in hand, ONE
  // expression describes the whole surface:
  //
  //   var uv = self.vertexUV;
  //   [0, Math.sin(uv[0] * 6 + time * 3) * 8]     // a wave travelling across
  //
  // vertexUV is the REST position (normalized, 0..1 over the image), not the
  // posed one: an expression that read its own output would feed back on
  // itself frame after frame.
  function _selfMesh() {
    var r = _selfResolved();
    if (!r || r.strokeId == null || !window.SMImageMesh) return null;
    return SMImageMesh.get(r.strokeId) || null;
  }
  function _selfVertexIndex() {
    var p = _ectx && _ectx.prop;
    if (!p) return -1;
    var m = /^vtx(\d+)$/.exec(p);
    return m ? parseInt(m[1], 10) : -1;
  }
  Object.defineProperty(SELF_VIEW, 'isMesh', { get: function () { return !!_selfMesh(); } });
  Object.defineProperty(SELF_VIEW, 'vertexIndex', { get: _selfVertexIndex });
  Object.defineProperty(SELF_VIEW, 'vertexCount', {
    get: function () { var m = _selfMesh(); return m ? m.verts.length : 0; },
  });
  Object.defineProperty(SELF_VIEW, 'vertexUV', {
    get: function () {
      var m = _selfMesh(), i = _selfVertexIndex();
      if (!m || i < 0 || !m.verts[i]) return [0, 0];
      return [m.verts[i][0], m.verts[i][1]];
    },
  });
  Object.defineProperty(SELF_VIEW, 'isOutlineVertex', {
    get: function () {
      var m = _selfMesh(), i = _selfVertexIndex();
      return !!(m && i >= 0 && i < m.outline.length);
    },
  });
  Object.defineProperty(SELF_VIEW, 'property', { get: function () { return _ectx ? _ectx.prop : null; } });
  Object.defineProperty(SELF_VIEW, 'name', { get: function () { var ld = _selfLd(); return ld ? ld.name : null; } });
  Object.defineProperty(SELF_VIEW, 'index', { get: function () { var r = _selfResolved(); return r ? r.index : -1; } });
  // A mesh holder is keyed like an element holder but is NOT a shape — it
  // used to answer true here, which made `self.isShape` useless for telling
  // the two apart now that both exist.
  Object.defineProperty(SELF_VIEW, 'isShape', { get: function () { var r = _selfResolved(); return !!(r && r.strokeId != null) && !_selfMesh(); } });
  Object.defineProperty(SELF_VIEW, 'inPoint', {
    get: function () { var ld = _selfLd(); return (ld && window.layerInPoint) ? layerInPoint(ld) : 0; },
  });
  Object.defineProperty(SELF_VIEW, 'outPoint', {
    get: function () { var ld = _selfLd(); return (ld && window.layerOutPoint) ? layerOutPoint(ld) : (state.totalFrames - 1); },
  });
  Object.defineProperty(SELF_VIEW, 'hasParent', { get: function () { var ld = _selfLd(); return !!(ld && ld.parentLayerUid); } });
  Object.defineProperty(SELF_VIEW, 'parent', {
    get: function () {
      var ld = _selfLd();
      if (!ld || !ld.parentLayerUid) return null;
      var pi = findLayerIndexByUid(ld.parentLayerUid);
      if (pi < 0) return null;
      return layerSnapshot(state.layers[pi].name, _ectx ? _ectx.frame : state.currentFrame);
    },
  });
  Object.defineProperty(SELF_VIEW, 'marker', { get: function () { var ld = _selfLd(); return _markerApi(ld && ld.markers); } });

  // Bounding box of what this holder actually draws, as {x, y, width,
  // height} in canvas coordinates.
  // SCOPE, stated rather than faked: Paper.js only materializes geometry for
  // the frame currently loaded, so this reports the CURRENT frame's box
  // whatever frame is being evaluated. Returning plausible-looking numbers
  // for another frame would be worse than saying so.
  function exprContentBox() {
    var r = _selfResolved();
    var b = null;
    if (r && window.userLayers) {
      if (r.strokeId != null && typeof liveItemByStrokeId === 'function') {
        var it = liveItemByStrokeId(r.index, r.strokeId);
        b = it && it.bounds;
      }
      if (!b) b = userLayers[r.index] && userLayers[r.index].bounds;
    }
    if (!b) return { x: 0, y: 0, width: 0, height: 0, top: 0, left: 0 };
    // top/left duplicate y/x so the compatibility alias below needs no
    // separate implementation.
    return { x: b.x, y: b.y, width: b.width, height: b.height, top: b.y, left: b.x };
  }

  // ==== Compatibility aliases ===========================================
  // Undocumented on purpose. These exist for ONE reason: a snippet pasted
  // from another application shouldn't strand the person who pasted it.
  // They are absent from the help tooltip, the examples menu and every
  // user-facing string; Nemo's own vocabulary above is the documented API.
  // Where an alias speaks SECONDS and the Nemo primary speaks FRAMES, the
  // alias converts here — a silent factor-of-fps error would be nasty.
  function aliasPosterizeTime(samplesPerSecond) {
    var ctx = _ectx;
    if (!ctx) return 0;
    var s = Number(samplesPerSecond);
    if (isFinite(s) && s > 0) exprStepTime(ctx.fps / s);
    return ctx.time; // seconds, as the original does
  }
  function aliasValueAtTime(t) { var c = _ectx; return exprSelfAt(_num(t) * (c ? c.fps : _exprFps())); }
  function aliasVelocityAtTime(t) { var c = _ectx; return exprSelfVelocity(_num(t) * (c ? c.fps : _exprFps())); }
  function aliasSpeedAtTime(t) { var c = _ectx; return exprSelfSpeed(_num(t) * (c ? c.fps : _exprFps())); }
  function aliasNearestKey(t) {
    var c = _ectx;
    return c ? exprNearestKeyAtFrame(c.holder, c.prop, _num(t) * c.fps) : null;
  }
  function aliasKey(i) { var c = _ectx; return c ? exprKeyAt(c.holder, c.prop, Math.floor(_num(i))) : null; }
  function aliasSeedRandom(seed, timeless) {
    var c = _ectx;
    exprSeed(seed);
    if (c) c.rngTimeless = !!timeless;
  }
  function aliasTimeToFrames(t, fps) {
    var c = _ectx;
    var tt = (t === undefined) ? (c ? c.time : 0) : _num(t);
    return tt * ((fps === undefined) ? (c ? c.fps : _exprFps()) : _num(fps));
  }
  function aliasFramesToTime(f, fps) {
    var c = _ectx;
    var ff = (f === undefined) ? (c ? c.frame : 0) : _num(f);
    var r = (fps === undefined) ? (c ? c.fps : _exprFps()) : _num(fps);
    return r === 0 ? 0 : ff / r;
  }
  // A comp/layer view shaped the way the pasted snippet expects. Built only
  // when an expression actually names one (see compiledFnFor's usage scan),
  // so the alias layer costs nothing on the hot path.
  function buildAliasComp() {
    return {
      width: COMP_VIEW.width, height: COMP_VIEW.height,
      frameDuration: 1 / _exprFps(),
      duration: (state.totalFrames || 1) / _exprFps(),
      numLayers: state.layers.length,
      name: COMP_VIEW.name,
      marker: _aliasMarkerApi(state.markers),
      layer: exprLayer,
    };
  }
  function buildAliasLayer() {
    var ld = _selfLd(), r = _selfResolved(), fps = _exprFps();
    if (!ld || !r) return null;
    return {
      name: ld.name, index: r.index,
      inPoint: (window.layerInPoint ? layerInPoint(ld) : 0) / fps,
      outPoint: (window.layerOutPoint ? layerOutPoint(ld) : (state.totalFrames - 1)) / fps,
      hasParent: !!ld.parentLayerUid,
      parent: SELF_VIEW.parent,
      marker: _aliasMarkerApi(ld.markers),
    };
  }
  // Same entries, under the numKeys/key/nearestKey names and with
  // nearestKey taking seconds.
  function _aliasMarkerApi(list) {
    var api = _markerApi(list);
    return {
      numKeys: api.count,
      key: api.at,
      nearestKey: function (t) { return api.nearest(_num(t) * _exprFps()); },
    };
  }

  // ---- compilation ------------------------------------------------------
  // Project-wide expression preamble: define something once and use it from
  // every expression instead of retyping it. Plain statements — `var k =
  // 12;` — evaluated in the same scope as the expression body, so anything
  // it declares is in scope. Cached against BOTH the expression's code and
  // the preamble, so editing the preamble recompiles every expression rather
  // than silently leaving stale ones behind.
  function exprGlobals() { return state.exprGlobals || ''; }
  // The sandbox's complete surface. `new Function` with an EXPLICIT, closed
  // parameter list — no window, no document, no state.
  // Nemo's own vocabulary — the documented surface, and the only pool the
  // "did you mean" suggestion draws from.
  var EXPR_PUBLIC_NAMES = [
    'time', 'frame', 'value', 'layer', 'self', 'comp', 'marker',
    'wiggle', 'noise', 'random', 'randomFixed', 'randomGauss', 'randomGaussFixed', 'seed',
    'clamp', 'remap', 'remapEase', 'remapEaseIn', 'remapEaseOut', 'degrees', 'radians',
    'add', 'sub', 'mul', 'div', 'length', 'normalize', 'dot', 'cross', 'angleTo',
    'stepTime', 'loopAfter', 'loopBefore', 'toFrames', 'toSeconds', 'contentBox',
    // Expression controls (2026-08-30) — the bare form is the self
    // shorthand; self.control(...) and layer(...).control(...) are the same
    // function reached through those two views. Listed here (and therefore
    // in the "did you mean" pool) because it IS part of the documented
    // vocabulary, not a compatibility alias.
    'control',
    // Lean cross-layer control read — see exprLayerControl's own comment for
    // why this exists next to layer(...).control(...) rather than instead of
    // it. Documented vocabulary, so it belongs in the "did you mean" pool.
    'layerControl',
  ];
  var EXPR_ARG_NAMES = EXPR_PUBLIC_NAMES.concat([
    // --- compatibility aliases (undocumented, see the block above) ---
    'loopOut', 'loopIn', 'key', 'nearestKey', 'numKeys', 'posterizeTime',
    'linear', 'ease', 'easeIn', 'easeOut', 'degreesToRadians', 'radiansToDegrees',
    'lookAt', 'seedRandom', 'gaussRandom',
    'valueAtTime', 'valueAtFrame', 'velocityAtTime', 'speedAtTime',
    'timeToFrames', 'framesToTime', 'sourceRectAtTime', 'thisComp', 'thisLayer',
    // --- internal: the runaway guard instrumentLoops() injects ---
    '__tick',
  ]);
  function _countLines(s) { return s ? s.split('\n').length : 0; }
  function compiledFnFor(holder, prop) {
    var ex = holder.expressions[prop];
    var pre = exprGlobals();
    if (holder._exprCompiled && holder._exprCompiled[prop] && holder._exprCompiled[prop].code === ex.code
        && holder._exprCompiled[prop].pre === pre) {
      return holder._exprCompiled[prop];
    }
    var fn;
    var args = EXPR_ARG_NAMES;
    // Guarded source first; if instrumentation somehow produced something
    // the parser rejects, fall back to the user's untouched text. A working
    // expression without the loop guard beats a broken one with it — and
    // this is why instrumentLoops may be conservative without risk.
    var preSrc = pre ? instrumentLoops(pre) : '';
    var codeSrc = instrumentLoops(ex.code || '');
    var guarded = true;
    // Body lines that sit BEFORE the user's own line 1, so a reported error
    // line can be translated back into a line of the textarea. Attempt A
    // adds `"use strict";`, the preamble, and `return (`.
    var preLines = pre ? _countLines(pre) : 0;
    var prefixLines = 1 + preLines + 1;
    var exprMode = true; // flipped below if the multi-statement form is used
    var userLines = _countLines(ex.code || '');
    function build(preText, codeText) {
      // eslint-disable-next-line no-new-func
      return Function.apply(null, args.concat(
        '"use strict";\n' + (preText ? preText + '\n' : '') + 'return (\n' + codeText + '\n);'));
    }
    try {
      try {
        fn = build(preSrc, codeSrc);
      } catch (eGuard) {
        if (eGuard instanceof SyntaxError && (preSrc !== pre || codeSrc !== ex.code)) {
          fn = build(pre, ex.code); // un-instrumented retry
          guarded = false;
        } else {
          throw eGuard;
        }
      }
      ex.lastError = null;
    } catch (e) {
      // Multi-statement fallback (2026-08-16, Cyril: "agrémenter la library
      // d'expressions... ID keyframes et layers") — the single-expression
      // wrapper above was the ONLY form ever tried, so `var l = layer(...);
      // return l ? l.position : value;` (the natural way to use layer()'s
      // own null-when-not-found contract) has always been a syntax error:
      // `var`/`if`/multiple statements can't live inside a bare `return
      // (...)` expression. Retried as a plain function BODY instead —
      // requires the user's own explicit `return`, unlike the wrapper above,
      // so this is deliberately a fallback tried second: every existing
      // single-expression project (`value + wiggle(2, 10)`, no `return`)
      // keeps compiling exactly as before through the first attempt, and
      // only code that already failed as a bare expression gets a second
      // interpretation.
      function buildBody(preText, codeText) {
        // eslint-disable-next-line no-new-func
        return Function.apply(null, args.concat(
          '"use strict";\n' + (preText ? preText + '\n' : '') + codeText + '\n'));
      }
      prefixLines = 1 + preLines; // no `return (` line in this form
      exprMode = false;
      try {
        try {
          fn = buildBody(preSrc, codeSrc);
        } catch (eGuard2) {
          if (eGuard2 instanceof SyntaxError && (preSrc !== pre || codeSrc !== ex.code)) {
            fn = buildBody(pre, ex.code);
            guarded = false;
          } else {
            throw eGuard2;
          }
        }
        ex.lastError = null;
      } catch (e2) {
        fn = null;
        // No line number on a syntax error, on purpose: measured, `new
        // Function` gives no usable position for one (its stack points at
        // the Function constructor's own call site, not into the source it
        // was handed). The engine's message usually names the offending
        // token, which is the part that helps; inventing a line here would
        // point at the wrong row of the editor.
        ex.lastError = SM.t('exprErrorSyntaxPrefix') + e2.message
          + (pre ? SM.t('exprErrorSyntaxGlobalsSuffix') : '');
        ex.errorLine = -1;
      }
    }
    // The two alias views are the only bindings that would allocate per
    // evaluation. Detected once, at compile time, from the text that will
    // actually run, so an expression that never names them pays nothing.
    var body = (pre ? pre + '\n' : '') + (ex.code || '');
    var entry = {
      code: ex.code, pre: pre, fn: fn,
      prefixLines: prefixLines,
      userLines: userLines,
      exprMode: exprMode,
      guarded: guarded,
      needsAliasComp: /\bthisComp\b/.test(body),
      needsAliasLayer: /\bthisLayer\b/.test(body),
    };
    if (!holder._exprCompiled) holder._exprCompiled = {};
    holder._exprCompiled[prop] = entry;
    return entry;
  }
  // Normalizes an expression's return value to the array shape PROP_DIM
  // expects — a 1D property (rotation/opacity) may return a bare number,
  // a 2D one (position/anchor/scale) a bare [x,y] array or, forgivingly, a
  // bare number applied to both dimensions.
  // Written against `dim` rather than against the literal cases 1 and 2
  // (2026-08-30): PROP_DIM has never actually been capped at 2 — 'fillColor'
  // is a 4-channel track, and a colour expression control is another — so
  // the old `dim === 2` branches silently produced a 2-element array for a
  // 4-dimension property. Behaviour for 1D/2D properties is unchanged.
  function normalizeExprResult(result, prop) {
    // Per-vertex properties (prop = 'vtx0','vtx1', ...) are created on demand
    // and are deliberately absent from the fixed PROP_DIM table, exactly as
    // they are absent from PROP_DEFAULT — staticValue already answers [0,0]
    // for the same family, for the same reason. Without the same fallback
    // here `dim` was undefined, `result.length >= undefined` is false, and
    // EVERY expression on a vertex returned null: the value was computed
    // correctly and then discarded on the way out. Vertex expressions have
    // never worked, on meshes or on path vertices. A vertex offset is always
    // [du, dv].
    var dim = PROP_DIM[prop];
    if (dim === undefined && prop.indexOf('vtx') === 0) dim = 2;
    function fill(n) { var o = []; for (var i = 0; i < dim; i++) o.push(n); return o; }
    // Every component must be FINITE (2026-08-30). The bare-number branch
    // below already rejected NaN; the array branch did not, and that gap was
    // load-bearing: `[0/0, 100]` is valid JavaScript with no syntax error and
    // no reported expression error, so a NaN went straight through, reached
    // the scene JSON — where JSON.stringify turns NaN and Infinity into
    // `null` — and the Rust engine rejected the payload with "invalid type:
    // null, expected f64". Measured: a position expression of exactly that
    // form threw on every render.
    //
    // Returning null here routes it into the path that already exists for a
    // bad result: the located "must return a number" error on that one
    // property row, that property falling back to its raw value, and the rest
    // of the scene still rendering. This is the same family as the
    // Option<String> incident in CLAUDE.md — one malformed value must not be
    // able to take the whole engine down for the session.
    function allFinite(a) {
      for (var i = 0; i < a.length; i++) if (typeof a[i] !== 'number' || !isFinite(a[i])) return false;
      return true;
    }
    if (Array.isArray(result)) {
      if (result.length >= dim) { var cut = result.slice(0, dim); return allFinite(cut) ? cut : null; }
      // A single number in an array applies to every dimension, same
      // forgiving reading as a bare number below.
      if (result.length === 1 && dim >= 2 && typeof result[0] === 'number' && isFinite(result[0])) return fill(result[0]);
      return null;
    }
    if (typeof result === 'number' && isFinite(result)) return fill(result);
    return null;
  }
  function evalExpressionFor(holder, prop, frame, rawValue) {
    var ex = holder.expressions[prop];
    var entry = compiledFnFor(holder, prop);
    var fn = entry && entry.fn;
    if (!fn) return null;
    // Cross-layer references can form a cycle (A reads B, B reads A). Each
    // hop re-enters here, so a bounded depth turns what would be a stack
    // overflow into the ordinary "fall back to the raw value" outcome.
    if (_exprDepth >= EXPR_MAX_DEPTH) {
      ex.lastError = SM.t('exprErrorTooDeep');
      ex.errorLine = -1;
      _depthTripped.push(ex);
      return null;
    }
    // The wall-clock budget belongs to the OUTERMOST evaluation, so a chain
    // of cross-layer references shares one budget instead of each hop
    // resetting it.
    if (_exprDepth === 0) { _exprDeadline = Date.now() + EXPR_BUDGET_MS; _depthTripped.length = 0; }
    var seed = ensureExprSeed(holder);
    var fps = _exprFps();
    var prev = _ectx;
    _ectx = {
      holder: holder, prop: prop,
      frame: frame, time: frame / fps, fps: fps,
      seed: seed,
      rngSeed: seed + _propSeedOffset(prop), rngTimeless: false, rngCounter: 0,
      _resolved: undefined,
    };
    _exprDepth++;
    try {
      var result = fn(
        // --- Nemo's own vocabulary ---
        _ectx.time, _ectx.frame,
        rawValue.length === 1 ? rawValue[0] : rawValue.slice(),
        exprLayer, SELF_VIEW, COMP_VIEW, COMP_VIEW.marker,
        exprWiggle, exprNoise, exprRandom, exprRandomFixed, exprRandomGauss, exprRandomGaussFixed, exprSeed,
        exprClamp, exprRemap, exprRemapEase, exprRemapEaseIn, exprRemapEaseOut, exprDegrees, exprRadians,
        exprAdd, exprSub, exprMul, exprDiv, exprLength, exprNormalize, exprDot, exprCross, exprAngleTo,
        exprStepTime, exprLoopAfter, exprLoopBefore, exprToFrames, exprToSeconds, exprContentBox,
        exprSelfControl, exprLayerControl,
        // --- compatibility aliases ---
        exprLoopAfter, exprLoopBefore, aliasKey, aliasNearestKey, exprNumKeys(holder, prop), aliasPosterizeTime,
        exprRemap, exprRemapEase, exprRemapEaseIn, exprRemapEaseOut, exprRadians, exprDegrees,
        exprAngleTo, aliasSeedRandom, exprRandomGauss,
        aliasValueAtTime, exprSelfAt, aliasVelocityAtTime, aliasSpeedAtTime,
        aliasTimeToFrames, aliasFramesToTime, exprContentBox,
        entry.needsAliasComp ? buildAliasComp() : null,
        entry.needsAliasLayer ? buildAliasLayer() : null,
        _exprTick
      );
      var normalized = normalizeExprResult(result, prop);
      if (normalized === null) {
        ex.lastError = SM.t(PROP_DIM[prop] >= 2 ? 'exprErrorMustReturnNumberOrXY' : 'exprErrorMustReturnNumber');
        ex.errorLine = -1;
        return null;
      }
      // A reference cycle resolves to SOMETHING (the innermost hop falls back
      // to its raw value and the outer hops then succeed on top of it), so
      // without this the warning the cap raised would be wiped out by the
      // very evaluations it protected, and a cycle would look healthy while
      // producing an arbitrary number. Anything the cap flagged during this
      // outermost evaluation keeps its message.
      if (_depthTripped.indexOf(ex) < 0) { ex.lastError = null; ex.errorLine = -1; }
      return normalized;
    } catch (e) {
      if (e && e.message === EXPR_TIMEOUT_TAG) {
        // Give the application back rather than hitting the same wall on
        // every subsequent frame: the expression switches itself off and
        // says why, exactly once.
        ex.enabled = false;
        ex.lastError = SM.t('exprErrorTimeout');
        ex.errorLine = -1;
        if (holder._exprCompiled) delete holder._exprCompiled[prop];
        if (!_timeoutToastPending) {
          _timeoutToastPending = true;
          setTimeout(function () {
            _timeoutToastPending = false;
            if (window.showToast) showToast(SM.t('exprToastTimeout'));
            try { renderLayerList(); renderTimeline(); } catch (e2) { /* not mounted */ }
          }, 0);
        }
        return null;
      }
      var f = _formatRuntimeError(e, entry);
      ex.lastError = f.message;
      ex.errorLine = f.line;
      return null;
    } finally {
      _ectx = prev;
      _exprDepth--;
    }
  }
  var _timeoutToastPending = false;
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
    if (!track || !track.keys.length) { if (window.showToast) showToast('Anime d’abord ' + PROP_LABEL[prop] + SM.t('toastAnimIconTip')); return; }
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
    if (!next) { if (window.showToast) showToast(SM.t('toastCannotCreateEditableSegment')); return; }
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
  // Arbitrary-frame sibling of setKeyAtCurrentFrame — for callers that
  // build several keys across a range in one pass (Text Animator, below)
  // rather than reacting to the playhead one commit at a time. Same key
  // shape, curvePoints defaulting to DEFAULT_CURVE when omitted.
  function setKeyAtFrame(holder, prop, frame, values, curvePoints) {
    var track = ensureTrack(holder, prop);
    var k = keyAt(track, frame);
    if (k) { k.v = values.slice(); if (curvePoints) k.curvePoints = curvePoints; }
    else {
      track.keys.push({ frame: frame, v: values.slice(), curvePoints: curvePoints || cloneCurvePts(DEFAULT_CURVE), hOut: [0, 0], hIn: [0, 0] });
      sortKeys(track);
    }
    return keyAt(track, frame);
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
  // Component conversion is a MANUAL action (2026-08, feedback: "évite de
  // faire automatiquement des composant dans motion, ça doit être
  // manuelle"). This used to auto-convert a layer to a Component on its
  // FIRST layer-level property edit — originally (2026-07-17) scoped to
  // 2+-element layers, then widened to ANY layer (including a single
  // shape) so it could also be placed in a StoryBoard montage (§8
  // CLAUDE.md, "StoryBoard ne manipule QUE des Components"). Rendering a
  // plain (non-Component) layer's own Motion keys was always correct
  // either way — engine-bridge.js's buildSceneJson applies `motionMat`
  // to every layer's own content regardless of symbolId, entirely
  // independent of this conversion; the auto-convert was purely a UX
  // shortcut (and the StoryBoard-eligibility side effect), never a
  // rendering requirement. The manual entry point already existed before
  // this (timeline.js's layer-row/context-menu action ->
  // convertLayerToComponent/convertLayersToComponent) and still does —
  // this only removes the SILENT trigger from toggleAnimated/setValue
  // below, so keying a plain layer's Position/Rotation/etc. now just
  // keys it, no surprise conversion.
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
      // feedback #215 follow-up: seed the first key with what the field
      // was actually SHOWING (displayValueFor — a layer's untouched
      // 'order' reads as its natural rank, not staticValue's flat 0), so
      // turning the stopwatch on can never snap the layer to a different
      // rank than what was on screen the moment before.
      var cur = displayValueFor(ld, prop);
      ensureTrack(ld, prop).keys = [{ frame: state.currentFrame, v: cur, curvePoints: cloneCurvePts(DEFAULT_CURVE), hOut: [0, 0], hIn: [0, 0] }];
    }
  }
  // Editing a value field: if animated, this is a scrub at the CURRENT
  // frame — auto-adds/updates a key there (AE convention). If not
  // animated, it's just the static override.
  function setValue(ld, prop, values) {
    if (isAnimated(ld, prop)) setKeyAtCurrentFrame(ld, prop, values);
    else { if (!ld.motionStatic) ld.motionStatic = {}; ld.motionStatic[prop] = values.slice(); }
    // Order (feedback #97, "l'order n'a pas l'air de marcher... dans le
    // canvas"): z-stacking is engine-only, same as 3D layers/Motion Blur
    // (see exportHasEngineOnlyMotion's own comment) — the Paper.js fallback
    // canvas (used whenever the Rust/WebGPU engine is off, e.g. WebGPU
    // unavailable, or a prior WASM panic disabled it for the rest of the
    // session, see engine-bridge.js's tick() catch) just draws userLayers in
    // their natural document order and has never known about this property.
    // Without a warning this silently does nothing — a live "the layer
    // won't go behind" report is one confusing symptom of an engine outage
    // that started somewhere else entirely. One-shot per session so a scrub
    // gesture (many setValue calls per drag) doesn't spam toasts.
    if (prop === 'order' && !_orderEngineWarnShown && window.SMEngineBridge && !window.SMEngineBridge.isEnabled()) {
      _orderEngineWarnShown = true;
      if (window.showToast) showToast(SM.t('toastOrderNeedsEngine'));
    }
  }
  var _orderEngineWarnShown = false;

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
    if (!ld.elementMotion[strokeId]) {
      ld.elementMotion[strokeId] = {};
      // Dynamic shapes phase 2 (2026-08-18) — auto-tag + seed from the live
      // item's OWN current radii, found once here rather than re-derived on
      // every propsFor call: unlike Component exposedProps (one shared
      // default per key across every instance), each rect's un-animated
      // corner value is genuinely its OWN (data.paramShape.tl/tr/br/bl),
      // so PROP_DEFAULT can't carry it — motionStatic must, from the start,
      // or clicking the stopwatch for the first time (toggleAnimated's
      // OFF→ON reads valueAtFrame → staticValue → PROP_DEFAULT[0] when
      // nothing else is seeded) would silently snap a 40px corner back to
      // 0 the instant it's keyed.
      var li = state.layers.indexOf(ld);
      var item = li >= 0 ? liveItemByStrokeId(li, strokeId) : null;
      if (item && item.data && item.data.paramShape && item.data.paramShape.kind === 'rect') {
        var ps = item.data.paramShape;
        ld.elementMotion[strokeId].paramShapeKind = 'rect';
        ld.elementMotion[strokeId].motionStatic = { cornerTL: [ps.tl || 0], cornerTR: [ps.tr || 0], cornerBR: [ps.br || 0], cornerBL: [ps.bl || 0] };
      } else if (item && item.data && item.data.paramShape && item.data.paramShape.kind === 'ellipse') {
        var pse = item.data.paramShape;
        ld.elementMotion[strokeId].paramShapeKind = 'ellipse';
        ld.elementMotion[strokeId].motionStatic = { arcStart: [pse.startAngle || 0], arcSweep: [pse.sweep !== undefined ? pse.sweep : 359.9], arcInner: [Math.round((pse.innerRadius || 0) * 100)] };
      } else if (item && item.data && item.data.paramShape && item.data.paramShape.kind === 'star') {
        var pss = item.data.paramShape;
        ld.elementMotion[strokeId].paramShapeKind = 'star';
        ld.elementMotion[strokeId].motionStatic = { starInner: [Math.round((pss.innerRatio !== undefined ? pss.innerRatio : 0.5) * 100)], starCorner: [pss.cornerRadius || 0] };
      }
    }
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
  //
  // 2026-08-29 fix (feedback #150, "les expressions ont l'air d'agir juste
  // sur la bounding box pas les objets"): the early-return only checked
  // holder.motion/motionStatic — a layer that's NEVER been keyframed or
  // given a static override (both genuinely undefined, the common state for
  // a just-drawn layer) bailed out here even with an expression enabled on
  // one of its properties, so the actual render never picked it up. The
  // selection-box overlay (motionBoxGeom, above) has no such guard — it
  // calls valueAtFrame unconditionally, which DOES evaluate expressions
  // regardless of motion/motionStatic — so the box moved with the
  // expression while the object itself stayed put. Now also proceeds when
  // any of the 5 properties this function actually reads has an enabled
  // expression (hasExpr), matching what the box already does.
  function computeMotionMat(holder, frameIdx) {
    var hasAnyExpr = holder && holder.expressions &&
      (hasExpr(holder, 'position') || hasExpr(holder, 'anchor') || hasExpr(holder, 'rotation') ||
       hasExpr(holder, 'scale') || hasExpr(holder, 'opacity'));
    if (!holder || (!holder.motion && !holder.motionStatic && !hasAnyExpr)) return null;
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
  // extraDelta — an optional additive contribution on top of the layer's
  // own base pose, omitted (every call site before 2026-07-30) behaving
  // identically to before this param existed. Two independent producers:
  // (1) a duplicator clone's own positionZ/rotationX/rotationY offset
  // (data.dup3D, app.js — {dz,drx,dry} only, "en 3D aussi avec ID de
  // chaque cloner"), and (2) a layer's blended multi-parent contribution
  // ({dx,dy,drot,dsxPct,dsyPct,dz,drx,dry} — parentChainMats' 2D part
  // PLUS blendedParent3D's 3D part combined, "jouer comme une opacité les
  // parents entre eux" — a 3D layer ignores parentChain entirely
  // (engine-bridge.js), so this is the ONLY way a 3D child ever sees ANY
  // parent contribution, 2D or 3D). Every field defaults to 0/no-op when
  // absent, so either producer can supply just the fields it has.
  function make3DProjector(ld, bounds, frameIdx, canvasW, canvasH, extraDelta) {
    var pos = valueAtFrame(ld, 'position', frameIdx);
    var anc = valueAtFrame(ld, 'anchor', frameIdx);
    var rot = valueAtFrame(ld, 'rotation', frameIdx)[0];
    var scl = valueAtFrame(ld, 'scale', frameIdx);
    if (extraDelta) {
      pos = [pos[0] + (extraDelta.dx || 0), pos[1] + (extraDelta.dy || 0)];
      rot += extraDelta.drot || 0;
      scl = [scl[0] + (extraDelta.dsxPct || 0), scl[1] + (extraDelta.dsyPct || 0)];
    }
    var posZ = valueAtFrame(ld, 'positionZ', frameIdx)[0] + (extraDelta ? (extraDelta.dz || 0) : 0);
    var rotX = valueAtFrame(ld, 'rotationX', frameIdx)[0] + (extraDelta ? (extraDelta.drx || 0) : 0);
    var rotY = valueAtFrame(ld, 'rotationY', frameIdx)[0] + (extraDelta ? (extraDelta.dry || 0) : 0);
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
  // 3D layers (2026-07-29, Cyril: "attaque le chantier" of Select/Subselect/
  // Rig having zero awareness of ld.threeD) — layerMotionPointMap (below)
  // only recognizes the base 2D properties, so it returned null for a
  // 3D-toggled layer (rotationX/rotationY/positionZ never registered as
  // "this layer has a transform"), and every canvas tool built on it
  // (select-bridge.js, subselect-bridge.js, rig-bridge.js) silently fell
  // back to "no mapping", operating at the click's RAW screen position —
  // confirmed live: dragging a rig bone anchor on a 3D-rotated layer
  // snapped it to a wildly wrong position, nowhere near the cursor.
  //
  // Unlike the 2D case, make3DProjector's forward map is a true PERSPECTIVE
  // projection (project3DToScreen: screen = center + world/depthW, depthW
  // depending on each point's own post-rotation Z) — not affine, so
  // inverting it isn't a 2x2 matrix invert. This solves the actual
  // ray-plane intersection instead: the layer's local XY plane, after its
  // 3D rotation, is a plane in 3D — point O (the pivot's world position)
  // spanned by basis vectors e1/e2 (the rotated, scaled local X/Y unit
  // vectors, from the SAME rotate3D the forward projector itself uses, just
  // probed with unit vectors instead of an actual point). The screen point
  // defines a camera ray (the exact inverse of project3DToScreen's own
  // depthW formula: a point at ray-parameter t has wz = CAMERA_DISTANCE*
  // (t-1), which reproduces depthW=t and wx/depthW=ux at every t — i.e.
  // every point on this ray really does project to the same screen point,
  // by construction). Solving where the ray crosses the plane (3 linear
  // equations, 3 unknowns t/lx/ly — Cramer's rule) gives back the exact
  // local (lx,ly) that projected there: the mathematically correct
  // inverse, not a local/linearized approximation.
  function layerMotion3DPointMap(li) {
    var ld = state.layers[li];
    if (!ld || !ld.threeD) return null;
    var lb = userLayers[li] && userLayers[li].bounds;
    if (!lb) return null;
    var frameIdx = state.currentFrame;
    var pos = valueAtFrame(ld, 'position', frameIdx);
    var anc = valueAtFrame(ld, 'anchor', frameIdx);
    var rot = valueAtFrame(ld, 'rotation', frameIdx)[0];
    var scl = valueAtFrame(ld, 'scale', frameIdx);
    var posZ = valueAtFrame(ld, 'positionZ', frameIdx)[0];
    var rotX = valueAtFrame(ld, 'rotationX', frameIdx)[0];
    var rotY = valueAtFrame(ld, 'rotationY', frameIdx)[0];
    var sx = scl[0] / 100 || 1e-6, sy = scl[1] / 100 || 1e-6;
    var pivotX = lb.x + lb.width / 2 + anc[0], pivotY = lb.y + lb.height / 2 + anc[1];
    var projector = make3DProjector(ld, lb, frameIdx, state.canvasW, state.canvasH);
    var Ox = pivotX + pos[0] - state.canvasW / 2, Oy = pivotY + pos[1] - state.canvasH / 2, Oz = posZ;
    var e1 = rotate3D(sx, 0, 0, rotX, rotY, rot);
    var e2 = rotate3D(0, sy, 0, rotX, rotY, rot);
    return {
      fwd: function (x, y) { var p = projector(x, y); return [p.x, p.y]; },
      inv: function (sx2, sy2) {
        var ux = sx2 - state.canvasW / 2, uy = sy2 - state.canvasH / 2;
        // [ux,-e1x,-e2x; uy,-e1y,-e2y; CAMERA_DISTANCE,-e1z,-e2z] * [t,lx,ly]^T = [Ox,Oy,Oz+CAMERA_DISTANCE]^T
        var a11 = ux, a12 = -e1[0], a13 = -e2[0], b1 = Ox;
        var a21 = uy, a22 = -e1[1], a23 = -e2[1], b2 = Oy;
        var a31 = CAMERA_DISTANCE, a32 = -e1[2], a33 = -e2[2], b3 = Oz + CAMERA_DISTANCE;
        var det = a11 * (a22 * a33 - a23 * a32) - a12 * (a21 * a33 - a23 * a31) + a13 * (a21 * a32 - a22 * a31);
        // Degenerate: the view ray is exactly parallel to the rotated plane
        // (a true edge-on 90° view) — falls back to the pivot rather than
        // dividing by ~0; a real user gesture never sustains exactly 90°.
        if (Math.abs(det) < 1e-9) return [pivotX, pivotY];
        var detLx = a11 * (b2 * a33 - a23 * b3) - b1 * (a21 * a33 - a23 * a31) + a13 * (a21 * b3 - b2 * a31);
        var detLy = a11 * (a22 * b3 - b2 * a32) - a12 * (a21 * b3 - b2 * a31) + b1 * (a21 * a32 - a22 * a31);
        var lx = detLx / det, ly = detLy / det;
        return [pivotX + lx / sx, pivotY + ly / sy];
      },
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
  // Image/video 3D projection (2026-07-30, Cyril: "la 3D sur les footage
  // pareil je crois que ça marche pas" — confirmed: engine-bridge.js's own
  // comment on the per-item loop states outright "an image inside a 3D
  // layer renders unprojected", a DELIBERATE scope boundary from when 3D
  // layers first shipped, not a regression).
  //
  // project3DSegments above projects every VERTEX independently because a
  // vector path's STROKE WIDTH must never warp with perspective — Cyril's
  // own explicit correction when this feature was first built ("comme pour
  // grease pencil, l'épaisseur du trait n'est jamais aplatie"), which is
  // why the original whole-texture perspective-quad-warp Rust pipeline was
  // reverted in favor of this per-vertex approach. A raster image/video has
  // no stroke-width concept to protect, so warping the WHOLE picture as a
  // true perspective trapezoid would actually be the semantically correct
  // behavior for it — but engine.rs's ImageRef (the wire shape a rendered
  // image item takes) is a plain axis-aligned rect + single rotation, with
  // no support for 4 independently-positioned corners; making it one would
  // mean a new Rust-side rendering primitive (a texture-mapped quad, real
  // WGPU/vello shader work) — out of scope for a JS-only pass and a
  // meaningfully bigger, separate undertaking.
  //
  // This is the middle ground: probes the projector along the rect's own
  // (possibly already-rotated, e.g. by elMat) local axes instead of a
  // single center point, then rebuilds an axis-aligned-but-rotated rect
  // from the resulting screen-space half-width/half-height vectors — same
  // "probe an offset point, use the delta" technique project3DSegments
  // itself already uses for bezier handles just above. Gives correct
  // position, correct rotationZ-equivalent screen rotation, and a
  // reasonable foreshortening-driven scale approximation for moderate
  // rotationX/rotationY — NOT true perspective (the projected shape is
  // always forced back into a rectangle, never an actual trapezoid), so a
  // steeply-tilted image won't show the trapezoidal keystoning a real 3D
  // plane would. Degrades gracefully (no crash, just a less accurate rect)
  // rather than blowing up as rotation approaches the 90°-edge-on case.
  function project3DImageRect(rect, projector) {
    var cx = rect.x + rect.width / 2, cy = rect.y + rect.height / 2;
    var hw = rect.width / 2, hh = rect.height / 2;
    var rot = (rect.rotation || 0) * Math.PI / 180, cos = Math.cos(rot), sin = Math.sin(rot);
    // Local +X/+Y axis directions in world space, accounting for the rect's
    // OWN existing rotation (e.g. already applied by elMat before this
    // runs) — probing along world-axis-aligned offsets instead would be
    // wrong the moment the input rect isn't already unrotated.
    var axX = cos, axY = sin, ayX = -sin, ayY = cos;
    // Full edge-to-edge probes (both +hw/-hw, both +hh/-hh), not a single
    // center-to-edge probe: under rotationX/rotationY one edge of the plane
    // moves CLOSER to the camera and the other FARTHER, so a one-sided
    // probe reads only that edge's own (magnified or minified) foreshortening
    // instead of a representative average — confirmed live, an EARLIER
    // version of this function probed center-to-edge only and a 45°
    // rotationY produced a rect 70% WIDER than the source, not narrower as
    // real foreshortening should. Averaging both edges' spans cancels that
    // one-sided bias out; the center still comes from the rect's own
    // center point directly (unaffected by this asymmetry either way).
    var pC = projector(cx, cy);
    var pXPos = projector(cx + hw * axX, cy + hw * axY), pXNeg = projector(cx - hw * axX, cy - hw * axY);
    var pYPos = projector(cx + hh * ayX, cy + hh * ayY), pYNeg = projector(cx - hh * ayX, cy - hh * ayY);
    var xVec = { x: (pXPos.x - pXNeg.x) / 2, y: (pXPos.y - pXNeg.y) / 2 };
    var yVec = { x: (pYPos.x - pYNeg.x) / 2, y: (pYPos.y - pYNeg.y) / 2 };
    var newHalfW = Math.hypot(xVec.x, xVec.y), newHalfH = Math.hypot(yVec.x, yVec.y);
    var rotDeg = Math.atan2(xVec.y, xVec.x) * 180 / Math.PI;
    return { x: pC.x - newHalfW, y: pC.y - newHalfH, width: newHalfW * 2, height: newHalfH * 2, rotation: rotDeg };
  }
  function toggleLayer3D(li) {
    var ld = state.layers[li];
    if (!ld) return;
    ld.threeD = !ld.threeD;
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    // renderTimeline too, not just renderLayerList (2026-07-30 fix — Cyril:
    // "encore des problème de calage d'ui... là où j'ai active la
    // duplication"): threeD changes propsFor(ld)'s row count (PROPS_WITH_3D
    // adds 3 rows) exactly like the Duplicator toggle below — the LEFT panel
    // re-rendered with the new count, but #frame-grid kept its stale one
    // until some unrelated later action happened to call renderTimeline(),
    // desyncing every layer row below this one in the meantime.
    if (window.renderLayerList) renderLayerList();
    if (window.renderTimeline) renderTimeline();
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
    // 2026-07-29 fix (QA-confirmed missing checkpoint, same family as the
    // duplicator panel's own fields below) — this also flips ld.locked,
    // an easy-to-regret side effect that must be undoable on its own.
    pushUndo();
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
        // Temporal stagger (2026-07-29, LottieFiles "Animation" tab
        // equivalent) — off by default, see applyLayerDuplicator (app.js).
        timeOffset: { enabled: false, offsetFrames: 1, direction: 'forward' },
      };
      ld.locked = true;
      // UI/UX audit (2026-07-30): enabling this dumps ~15 fields into the
      // panel at once (Mode, grille, radial, chemin, seed, 4 toggles
      // random, décalage temporel, Effectors...) with zero feedback that
      // anything happened — Time Remap's own enable already toasts a
      // one-line summary (enableTimeRemap below); this had nothing.
      if (window.showToast) showToast(SM.t('toastDuplicatorEnabledHint'));
    }
    loadFrame(state.currentFrame);
    invalidateSymbolUnionBounds(); // same cache-staleness fix as dupRefresh (timeline.js)
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    // renderTimeline too (2026-07-30 fix — Cyril: "encore des problème de
    // calage d'ui... là où j'ai active la duplication"): enabling/disabling
    // ld.duplicator changes propsFor(ld)'s row count by 7 (PROPS_DUP_EXTRA)
    // — #layer-list re-rendered with the new count right below, but
    // #frame-grid silently kept its PREVIOUS row count until some unrelated
    // later action happened to call renderTimeline(), so every layer row
    // below this one visibly drifted from its own keyframe track in the
    // meantime. Same alignment-invariant class ROW_H's header comment warns
    // about — this was simply a call site that never got the memo.
    if (window.renderLayerList) renderLayerList();
    if (window.renderTimeline) renderTimeline();
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
  // ---- Follow Path (2026-08, "motion path unifié") --------------------
  // A constraint, not a track: the target layer's own path geometry (its
  // FIRST stroke with 2+ points, raw un-transformed segments as stored in
  // ld.frames) is sampled at an arc-length percentage (keyable pathPercent)
  // and the result is composed through the TARGET's own world transform
  // (its Motion + parent chain — layerWorldBoundsUnion, above, is the exact
  // same "raw local points -> world" recipe, just for bounds corners
  // instead of a path point) so a moving/parented guide still works.
  // Deliberately calls computeMotionMat on the TARGET directly rather than
  // layerMotionAt (which would recurse back into this function) — a target
  // that itself has followPath is simply read as a plain (non-path-driven)
  // layer, which sidesteps cycles by construction instead of detecting them.
  function sampleFollowPathAt(ld, frameIdx) {
    var fp = ld && ld.followPath;
    if (!fp || !fp.targetLayerUid) return null;
    var ti = findLayerIndexByUid(fp.targetLayerUid);
    if (ti < 0) return null;
    var strokes = getEffectiveStrokes(ti, frameIdx);
    var src = null;
    for (var i = 0; i < strokes.length; i++) {
      if (strokes[i] && strokes[i].segments && strokes[i].segments.length > 1) { src = strokes[i]; break; }
    }
    if (!src) return null;
    var path = new Path({
      segments: src.segments.map(function (s) {
        return new Segment(new Point(s.point[0], s.point[1]), new Point((s.handleIn || [0, 0])[0], (s.handleIn || [0, 0])[1]), new Point((s.handleOut || [0, 0])[0], (s.handleOut || [0, 0])[1]));
      }),
      closed: !!src.closed, insert: false,
    });
    var len = path.length;
    if (!len) { path.remove(); return null; }
    var per = Math.max(0, Math.min(100, valueAtFrame(ld, 'pathPercent', frameIdx)[0] || 0)) / 100;
    var offset = Math.max(0, Math.min(len, per * len));
    var pt = path.getPointAt(offset);
    var tan = path.getTangentAt(offset);
    path.remove();
    if (!pt) return null;
    // Two points (sampled position + a point 10 units along the tangent)
    // riding through the SAME world-space composition together, so the
    // tangent survives the target's own rotation/scale intact.
    var pts = [
      { point: [pt.x, pt.y], handleIn: [0, 0], handleOut: [0, 0] },
      { point: [pt.x + (tan ? tan.x * 10 : 10), pt.y + (tan ? tan.y * 10 : 0)], handleIn: [0, 0], handleOut: [0, 0] },
    ];
    var tLayer = window.userLayers && userLayers[ti];
    var tb = tLayer ? tLayer.bounds : null;
    var tMat = computeMotionMat(state.layers[ti], frameIdx);
    if (tMat && tb) pts = transformSegments(pts, { x: tb.center.x + tMat.ax, y: tb.center.y + tMat.ay }, tMat);
    var tChain = parentChainMats(ti, frameIdx);
    for (var pc = 0; pc < tChain.length; pc++) pts = transformSegments(pts, tChain[pc].pivot, tChain[pc].mat);
    var wx = pts[0].point[0], wy = pts[0].point[1];
    var angle = Math.atan2(pts[1].point[1] - wy, pts[1].point[0] - wx) * 180 / Math.PI;
    return { x: wx, y: wy, angle: angle };
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
    var mat = computeMotionMat(ld, frameIdx);
    if (ld.followPath && ld.followPath.targetLayerUid) {
      var fp = sampleFollowPathAt(ld, frameIdx);
      if (fp) {
        if (!mat) mat = { dx: 0, dy: 0, rot: 0, sx: 1, sy: 1, op: 1, ax: 0, ay: 0 };
        var layer = window.userLayers && userLayers[li];
        var b = layer ? layer.bounds : null;
        if (b) {
          // Additive, not an override (matches FollowPathEffect's own
          // `posX += posXChange`): a Position value the user ALSO keys
          // stays a relative offset FROM the path point, so pathPercent=0
          // + Position=[0,0] places the pivot exactly ON the path, and any
          // manual Position tweak just nudges it off that.
          mat.dx += fp.x - b.center.x - mat.ax;
          mat.dy += fp.y - b.center.y - mat.ay;
        }
        if (ld.followPath.align) {
          var infl = (valueAtFrame(ld, 'pathInfluence', frameIdx)[0] || 0) / 100;
          mat.rot += fp.angle * infl;
        }
      }
    }
    return mat;
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
  // World-space union bounds of several TOP-LEVEL layers, at a given frame
  // (2026-08, feedback #59 — "le null doit être placé au centre de tout les
  // calques en fonction des éléments qu'ils contiennent"). Deliberately NOT
  // userLayers[li].bounds directly: that's the LIVE Paper geometry in its
  // own untransformed space — Motion's Position/Rotation/Scale/parenting is
  // a separate JS-level system applied at RENDER time (pathTransform), so a
  // layer with existing Motion keys would report its ORIGINAL position, not
  // where it currently sits on screen. Same corner-transform technique
  // buildGuideLayerItems (engine-bridge.js) already uses for a single
  // point, just done for the 4 corners of each layer's raw bounds and
  // unioned across every layer in the list.
  function layerWorldBoundsUnion(indices, frame) {
    var left = Infinity, top = Infinity, right = -Infinity, bottom = -Infinity;
    indices.forEach(function (li) {
      var ld = state.layers[li], layer = window.userLayers && userLayers[li];
      if (!ld || !layer) return;
      // POSED bounds, not raw (2026-08-31, feedback en Motion: "la box de
      // hover n'est pas à jour si je bouge un éléments dans le groupe").
      // layer.bounds is the untransformed geometry, so a layer whose
      // elements carry their own Motion offsets reported its OLD extent —
      // the hover box, and every other consumer of this union, stayed where
      // the shapes used to be. perObjectPosedUnionLocal (tools.js) applies
      // each element's own transform and stops there, which is exactly this
      // function's input space: the layer transform and the parent chain
      // are applied just below, as before.
      var b = (window.perObjectPosedUnionLocal && perObjectPosedUnionLocal(layer)) || layer.bounds;
      if (!b || !isFinite(b.width) || !isFinite(b.height) || (b.width === 0 && b.height === 0)) return;
      var corners = [[b.left, b.top], [b.right, b.top], [b.right, b.bottom], [b.left, b.bottom]];
      var pts = corners.map(function (c) { return { point: c, handleIn: [0, 0], handleOut: [0, 0] }; });
      var ownMat = layerMotionAt(li, frame);
      if (ownMat) pts = transformSegments(pts, { x: b.center.x + ownMat.ax, y: b.center.y + ownMat.ay }, ownMat);
      var parentChain = parentChainMats(li, frame);
      for (var pc = 0; pc < parentChain.length; pc++) pts = transformSegments(pts, parentChain[pc].pivot, parentChain[pc].mat);
      pts.forEach(function (p) {
        left = Math.min(left, p.point[0]); top = Math.min(top, p.point[1]);
        right = Math.max(right, p.point[0]); bottom = Math.max(bottom, p.point[1]);
      });
    });
    if (left === Infinity) return null;
    return { x: left, y: top, w: right - left, h: bottom - top, cx: (left + right) / 2, cy: (top + bottom) / 2 };
  }
  function findLayerIndexByUid(uid) {
    if (!uid) return -1;
    for (var i = 0; i < state.layers.length; i++) if (state.layers[i].layerUid === uid) return i;
    return -1;
  }
  // Refuse to create a cycle (A parents B parents A) — BFS the CANDIDATE
  // parent's own ancestry through BOTH parent slots (a node can have up to
  // 2 outgoing edges now, parentLayerUid AND parentLayerUidB, so this is a
  // small DAG walk, not a single linear chain); if `targetUid` appears
  // anywhere in it, the assignment would loop. Shared by setLayerParent
  // and setLayerParentB so both slots get the exact same guard.
  function wouldCreateParentCycle(targetUid, candidateParentUid) {
    if (!candidateParentUid) return false;
    var queue = [candidateParentUid], visited = {}, guard = 0;
    while (queue.length && guard++ < 256) {
      var cur = queue.shift();
      if (cur === targetUid) return true;
      if (visited[cur]) continue;
      visited[cur] = true;
      var idx = findLayerIndexByUid(cur);
      if (idx < 0) continue;
      if (state.layers[idx].parentLayerUid) queue.push(state.layers[idx].parentLayerUid);
      if (state.layers[idx].parentLayerUidB) queue.push(state.layers[idx].parentLayerUidB);
    }
    return false;
  }
  // A layer's own pivot BASE in its unparented local space — same
  // convention legacyParentChainMats already uses when THIS layer is
  // read as someone else's ancestor (nullPos for a Null, bounds.center
  // otherwise), kept consistent on purpose rather than inventing a second
  // pivot convention for the compensation math below.
  function ownPivotBase(li) {
    var ld = state.layers[li];
    if (!ld) return [state.canvasW / 2, state.canvasH / 2];
    if (ld.isNullLayer) return ld.nullPos || [state.canvasW / 2, state.canvasH / 2];
    var b = userLayers[li] && userLayers[li].bounds;
    return b ? [b.center.x, b.center.y] : [state.canvasW / 2, state.canvasH / 2];
  }
  // World position of a layer's own pivot point, own Motion translation
  // (dx/dy — rotate/scale never move the point they're pivoted around)
  // THEN the live parent chain — same two-step composition
  // buildNullLayerItems (engine-bridge.js) and layerWorldBoundsUnion
  // (above) already use. Reads state.layers[li].parentLayerUid LIVE, so
  // calling this before vs. after mutating that field is exactly how
  // setLayerParent below measures "did reparenting move this layer".
  function composedPivotWorld(li, frame) {
    var ld = state.layers[li];
    var base = ownPivotBase(li);
    var m = computeMotionMat(ld, frame);
    var pt = [{ point: [base[0] + (m ? m.dx : 0), base[1] + (m ? m.dy : 0)], handleIn: [0, 0], handleOut: [0, 0] }];
    var chain = parentChainMats(li, frame);
    for (var k = 0; k < chain.length; k++) pt = transformSegments(pt, chain[k].pivot, chain[k].mat);
    return pt[0].point;
  }
  function setLayerParent(li, parentUid) {
    var ld = state.layers[li];
    if (!ld) return;
    // Mirrors setLayerParentB's own guard below — the context menu already
    // disables "Parent A : X (déjà Parent B)", but the pickwhip drag (still
    // Parent-A-only) calls straight through to this function and bypassed
    // it, reachable live: drag the pickwhip onto the existing Parent B and
    // land on a meaningless "blend a layer with itself" state.
    if (parentUid && parentUid === ld.parentLayerUidB) {
      if (window.showToast) showToast(SM.t('toastParentAMustDifferFromB'));
      return;
    }
    if (parentUid && wouldCreateParentCycle(ensureLayerUid(ld), parentUid)) {
      if (window.showToast) showToast(SM.t('toastParentingRefusedCycle'));
      return;
    }
    // Keyed re-parent (feedback #207) — a layer with ld.parentKeys already
    // turned on gets a NEW KEY at the playhead instead of the static field
    // overwrite below, matching Blend's own setExpressionCode-vs-
    // upsertBlendKeyAt split. Deliberately skips the keep-transform
    // compensation just below — see layerParentUidAt's own header comment
    // for why a correct per-frame version of that is out of scope here.
    if (ld.parentKeys && ld.parentKeys.length) {
      upsertParentKeyAt(ld, state.currentFrame, parentUid || null);
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
      return;
    }
    // Keep-transform compensation (2026-08, feedback: "quand on parent un
    // objet au null il doit conserver sa position initiale (offset)") —
    // without this, parenting to a target that isn't currently sitting at
    // its own rest position (e.g. a Null already dragged elsewhere) made
    // the child instantly jump by the parent's full current offset, same
    // as re-parenting onto ANY already-moved layer. Measures this layer's
    // own pivot point in world space before/after the reassignment and
    // folds the difference into its own Position track so nothing visibly
    // moves at the moment of parenting — translation only (matches the
    // overwhelmingly common case: a Null moved but not rotated/scaled);
    // a rotated/scaled parent still reorients the child going forward,
    // same as After Effects.
    var frame = state.currentFrame;
    var before = composedPivotWorld(li, frame);
    ld.parentLayerUid = parentUid || null;
    var after = composedPivotWorld(li, frame);
    var dx = before[0] - after[0], dy = before[1] - after[1];
    if (dx || dy) {
      var pos = valueAtFrame(ld, 'position', frame);
      setValue(ld, 'position', [pos[0] + dx, pos[1] + dy]);
    }
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  // Second parent slot (2026-07-30, "plusieurs parent... jouer comme une
  // opacité les parents entre eux") — see parentChainMats' own header
  // comment for the full crossfade design. Setting it to the SAME uid as
  // parentLayerUid is refused (not a cycle, just meaningless — blending a
  // layer with itself), everything else mirrors setLayerParent exactly.
  function setLayerParentB(li, parentUid) {
    var ld = state.layers[li];
    if (!ld) return;
    if (parentUid && parentUid === ld.parentLayerUid) {
      if (window.showToast) showToast(SM.t('toastParentBMustDifferFromA'));
      return;
    }
    if (parentUid && wouldCreateParentCycle(ensureLayerUid(ld), parentUid)) {
      if (window.showToast) showToast(SM.t('toastParentingRefusedCycle'));
      return;
    }
    ld.parentLayerUidB = parentUid || null;
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  // Follow Path's ONE UI entry point (setter, mirrors setLayerParent's own
  // contract) — no cycle check needed: sampleFollowPathAt reads the
  // target's PLAIN computeMotionMat, never layerMotionAt, so a cycle (A
  // follows B follows A) just means each one ignores the other's path
  // contribution rather than recursing infinitely.
  function setLayerFollowPath(li, targetUid) {
    var ld = state.layers[li];
    if (!ld) return;
    if (!targetUid) { ld.followPath = null; if (window.SMEngineBridge) SMEngineBridge.renderNow(); return; }
    if (targetUid === ensureLayerUid(ld)) return; // can't follow itself
    ld.followPath = { targetLayerUid: targetUid, align: (ld.followPath && ld.followPath.align) || false };
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  // ---- Multi-parent crossfade (2026-07-30, "plusieurs parent... jouer
  // comme une opacité les parents entre eux") ----
  // Confirmed with Cyril: exactly 2 parents, a single 0-100 blend (Motion-
  // keyable like opacity) crossfading between "fully follows Parent A" and
  // "fully follows Parent B" — not an N-way weighted average. A layer with
  // no parentLayerUidB behaves EXACTLY as before (parentChainMats' dispatch
  // below routes it straight to the untouched legacy path) — this is
  // purely additive, zero risk to the single-parent case every existing
  // parented layer already uses.
  //
  // Shortest-path angle lerp — naive (a + (b-a)*t) breaks at the
  // wraparound (350°->10° must cross through 0°, not swing the long way
  // through 180°). Same reasoning AE/Blender rotation blending uses.
  function lerpAngleDeg(a, b, t) { var d = ((b - a + 180) % 360 + 360) % 360 - 180; return a + d * t; }
  // Reduces an ENTIRE ancestor chain (starting at startUid, walking
  // parentLayerUid only — a chain ancestor's OWN second parent is not
  // recursively blended here, out of scope for this pass, same
  // "explicitly scoped rather than silently wrong" precedent 3D-layers-
  // ignore-parents already sets elsewhere) to ONE equivalent
  // {dx,dy,rot,sx,sy} descriptor, by tracking 3 reference points (origin,
  // +unitX, +unitY) through the chain via transformSegments — reuses the
  // exact same affine math every ordinary parented layer already renders
  // through instead of a second hand-rolled matrix-composition
  // implementation (CLAUDE.md §3's duplicated-pair trap). Exact whenever
  // the composed chain has no shear (the overwhelmingly common case — one
  // or two ancestors with ordinary rotation+uniform-or-near-uniform
  // scale); a deeply nested chain mixing non-uniform scale with rotation
  // at multiple levels can in principle introduce shear this
  // shear-free-by-construction descriptor can't represent exactly — an
  // accepted, documented approximation in that narrow case, not a silent
  // one.
  function composeChainTransform(startUid, frameIdx) {
    var startIdx = findLayerIndexByUid(startUid);
    if (startIdx < 0) return null;
    var segs = [
      { point: [0, 0], handleIn: [0, 0], handleOut: [0, 0] },
      { point: [1, 0], handleIn: [0, 0], handleOut: [0, 0] },
      { point: [0, 1], handleIn: [0, 0], handleOut: [0, 0] },
    ];
    var visited = {}, curIdx = startIdx, guard = 0, any = false;
    while (curIdx >= 0 && !visited[curIdx] && guard++ < 64) {
      visited[curIdx] = true;
      var aLd = state.layers[curIdx];
      var m = computeMotionMat(aLd, frameIdx);
      if (m && userLayers[curIdx] && userLayers[curIdx].bounds) {
        var pivot = { x: userLayers[curIdx].bounds.center.x + m.ax, y: userLayers[curIdx].bounds.center.y + m.ay };
        segs = transformSegments(segs, pivot, m);
        any = true;
      }
      var nextUid = aLd.parentLayerUid;
      curIdx = nextUid ? findLayerIndexByUid(nextUid) : -1;
    }
    if (!any) return null;
    var o = segs[0].point, ux = segs[1].point, uy = segs[2].point;
    var vx = [ux[0] - o[0], ux[1] - o[1]], vy = [uy[0] - o[0], uy[1] - o[1]];
    return { dx: o[0], dy: o[1], rot: Math.atan2(vx[1], vx[0]) * 180 / Math.PI, sx: Math.hypot(vx[0], vx[1]), sy: Math.hypot(vy[0], vy[1]) };
  }
  // The blended 2D contribution for a layer with BOTH parents set — a
  // drop-in {mat,pivot} (pivot fixed at the origin: composeChainTransform's
  // dx/dy already IS the absolute resulting position, not a pivot-relative
  // one, so no separate pivot offset is needed) that every existing
  // parentChain consumer already knows how to apply, via parentChainMats'
  // own dispatch below.
  // ---- Weighted parents, 3 and up (2026-08-30) -------------------------
  // "OK pour poids" — beyond A/B, a layer can carry ld.parentsMore, each
  // entry with its own keyframable weight (parentWeight0, 1, …). Weights are
  // ABSOLUTE percentages that get normalized by their sum, and A/B keep
  // deriving theirs from parentBlend: wA = 100 - blend, wB = blend.
  //
  // Two properties fall straight out of that choice and are the reason for
  // it. With no extras the weights are (1-t, t), they already sum to 1, so
  // normalization is the identity and the two-parent case runs the code it
  // always ran — unchanged, not "equivalent". And a freshly added parent
  // defaults to weight 0, so pressing "+" never makes the layer jump.
  //
  // The blend is a weighted average of DECOMPOSED components, never of raw
  // matrices: averaging matrices shears and collapses them. That is also
  // what the existing two-parent lerp above already does, so this is the
  // same idea with n terms rather than a different technique. Rotation uses
  // a circular mean (average the unit vectors, then atan2) because degrees
  // wrap — a plain numeric average of 350° and 10° gives 180°, pointing
  // exactly backwards.
  function parentWeightKeyFor(i) { return 'parentWeight' + i; }
  function weightedParentsOf(ld, frameIdx) {
    var extras = ld.parentsMore || [];
    if (!extras.length) return null;
    var out = [];
    var t = (valueAtFrame(ld, 'parentBlend', frameIdx)[0] || 0) / 100;
    out.push({ uid: ld.parentLayerUid, w: 1 - t });
    out.push({ uid: ld.parentLayerUidB, w: t });
    for (var i = 0; i < extras.length; i++) {
      if (!extras[i] || !extras[i].uid) continue;
      var wv = valueAtFrame(ld, parentWeightKeyFor(i), frameIdx);
      out.push({ uid: extras[i].uid, w: ((wv && wv[0]) || 0) / 100 });
    }
    return out;
  }
  function blendWeighted(parts, frameIdx) {
    var sum = 0, i;
    for (i = 0; i < parts.length; i++) sum += parts[i].w;
    // Every weight at zero has no meaningful answer — fall back to the
    // first parent rather than dividing by zero and rendering NaN, which
    // would blank the layer with no clue why.
    if (!(sum > 1e-9)) { parts = [{ uid: parts[0].uid, w: 1 }]; sum = 1; }
    var dx = 0, dy = 0, sx = 0, sy = 0, cx = 0, cy = 0, any = false;
    for (i = 0; i < parts.length; i++) {
      var w = parts[i].w / sum;
      if (!w) continue;
      var m = composeChainTransform(parts[i].uid, frameIdx);
      if (!m) { m = { dx: 0, dy: 0, rot: 0, sx: 1, sy: 1 }; } else { any = true; }
      dx += m.dx * w; dy += m.dy * w; sx += m.sx * w; sy += m.sy * w;
      var r = m.rot * Math.PI / 180;
      cx += Math.cos(r) * w; cy += Math.sin(r) * w;
    }
    if (!any) return null;
    return {
      dx: dx, dy: dy, sx: sx, sy: sy,
      rot: (cx === 0 && cy === 0) ? 0 : Math.atan2(cy, cx) * 180 / Math.PI,
      op: 1, ax: 0, ay: 0,
    };
  }
  function blendedAncestorMat(li, frameIdx) {
    var ld = state.layers[li];
    if (!ld || !ld.parentLayerUidB || !ld.parentLayerUid) return null;
    var many = weightedParentsOf(ld, frameIdx);
    if (many) return blendWeighted(many, frameIdx);
    var t = (valueAtFrame(ld, 'parentBlend', frameIdx)[0] || 0) / 100;
    var A = composeChainTransform(ld.parentLayerUid, frameIdx);
    var B = composeChainTransform(ld.parentLayerUidB, frameIdx);
    if (!A && !B) return null;
    A = A || { dx: 0, dy: 0, rot: 0, sx: 1, sy: 1 };
    B = B || { dx: 0, dy: 0, rot: 0, sx: 1, sy: 1 };
    return {
      dx: A.dx + (B.dx - A.dx) * t, dy: A.dy + (B.dy - A.dy) * t,
      rot: lerpAngleDeg(A.rot, B.rot, t),
      sx: A.sx + (B.sx - A.sx) * t, sy: A.sy + (B.sy - A.sy) * t,
      op: 1, ax: 0, ay: 0,
    };
  }
  // The blended 3D contribution (Cyril: "il faut que ça marche aussi avec
  // des calques 3D") — deliberately SINGLE-LEVEL: each parent contributes
  // its OWN positionZ/rotationX/rotationY (only if that parent itself has
  // threeD on) without recursively composing that parent's own ancestor's
  // 3D pose — full multi-level 3D chain composition is a much larger
  // undertaking (real 3D matrix composition, not 2D affines) than this
  // pass's scope. Consumed only by a 3D CHILD (engine-bridge.js's is3D
  // branch, via make3DProjector's widened extraDelta) — a 2D child never
  // reads this, its parents' 3D pose (if any) simply doesn't apply to it,
  // matching the existing "3D layers project independently" boundary.
  function blendedParent3D(li, frameIdx) {
    var ld = state.layers[li];
    if (!ld || !ld.parentLayerUidB || !ld.parentLayerUid) return null;
    // Weighted N-parent 3D (2026-08-30) — same single-level rule as the
    // two-parent path below, just averaged over n terms with the same
    // weights blendedAncestorMat uses, so the 2D and 3D halves of one
    // layer can never disagree about how much each parent counts.
    var many3 = weightedParentsOf(ld, frameIdx);
    if (many3) {
      var s3 = 0, k;
      for (k = 0; k < many3.length; k++) s3 += many3[k].w;
      if (!(s3 > 1e-9)) return null;
      var dz = 0, rcx = 0, rcy = 0, ycx = 0, ycy = 0, got = false;
      for (k = 0; k < many3.length; k++) {
        var w3 = many3[k].w / s3;
        if (!w3) continue;
        var pi = findLayerIndexByUid(many3[k].uid);
        var pl = pi >= 0 ? state.layers[pi] : null;
        if (!pl || !pl.threeD) continue;
        got = true;
        dz += valueAtFrame(pl, 'positionZ', frameIdx)[0] * w3;
        var rx = valueAtFrame(pl, 'rotationX', frameIdx)[0] * Math.PI / 180;
        var ry = valueAtFrame(pl, 'rotationY', frameIdx)[0] * Math.PI / 180;
        rcx += Math.cos(rx) * w3; rcy += Math.sin(rx) * w3;
        ycx += Math.cos(ry) * w3; ycy += Math.sin(ry) * w3;
      }
      if (!got) return null;
      return {
        dz: dz,
        drx: (rcx === 0 && rcy === 0) ? 0 : Math.atan2(rcy, rcx) * 180 / Math.PI,
        dry: (ycx === 0 && ycy === 0) ? 0 : Math.atan2(ycy, ycx) * 180 / Math.PI,
      };
    }
    var t = (valueAtFrame(ld, 'parentBlend', frameIdx)[0] || 0) / 100;
    var aIdx = findLayerIndexByUid(ld.parentLayerUid), bIdx = findLayerIndexByUid(ld.parentLayerUidB);
    var aLd = aIdx >= 0 ? state.layers[aIdx] : null, bLd = bIdx >= 0 ? state.layers[bIdx] : null;
    var aZ = (aLd && aLd.threeD) ? valueAtFrame(aLd, 'positionZ', frameIdx)[0] : 0;
    var bZ = (bLd && bLd.threeD) ? valueAtFrame(bLd, 'positionZ', frameIdx)[0] : 0;
    var aRX = (aLd && aLd.threeD) ? valueAtFrame(aLd, 'rotationX', frameIdx)[0] : 0;
    var bRX = (bLd && bLd.threeD) ? valueAtFrame(bLd, 'rotationX', frameIdx)[0] : 0;
    var aRY = (aLd && aLd.threeD) ? valueAtFrame(aLd, 'rotationY', frameIdx)[0] : 0;
    var bRY = (bLd && bLd.threeD) ? valueAtFrame(bLd, 'rotationY', frameIdx)[0] : 0;
    if (!aZ && !bZ && !aRX && !bRX && !aRY && !bRY) return null;
    return { dz: aZ + (bZ - aZ) * t, drx: lerpAngleDeg(aRX, bRX, t), dry: lerpAngleDeg(aRY, bRY, t) };
  }
  // The ONLY entry point a 3D layer needs for multi-parent support — a 3D
  // child ignores parentChain entirely (engine-bridge.js forces it to []
  // for any threeD layer), so blendedAncestorMat's 2D result never reaches
  // it through the normal path. Combines that 2D part with
  // blendedParent3D's 3D part into ONE make3DProjector-shaped extraDelta,
  // so a 3D layer with 2 parents gets both halves of the crossfade in a
  // single call. Returns null (a plain 3D layer, no parents, or only one
  // parent — the ordinary already-correct "3D layers don't parent" case)
  // exactly when make3DProjector should be called with no extraDelta at
  // all, i.e. today's unchanged behavior.
  function blendedParentContributionFor3D(li, frameIdx) {
    var ld = state.layers[li];
    if (!ld || !ld.parentLayerUidB || !ld.parentLayerUid) return null;
    var m2d = blendedAncestorMat(li, frameIdx);
    var m3d = blendedParent3D(li, frameIdx);
    if (!m2d && !m3d) return null;
    var out = {};
    if (m2d) { out.dx = m2d.dx; out.dy = m2d.dy; out.drot = m2d.rot; out.dsxPct = (m2d.sx - 1) * 100; out.dsyPct = (m2d.sy - 1) * 100; }
    if (m3d) { out.dz = m3d.dz; out.drx = m3d.drx; out.dry = m3d.dry; }
    return out;
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
  // Dispatches to the blended (2-parent) path when parentLayerUidB is set;
  // every existing single-parent layer takes the untouched legacy path
  // below, unchanged in every way (2026-07-30).
  function parentChainMats(li, frameIdx) {
    var ld = state.layers[li];
    if (ld && ld.parentLayerUidB) {
      var bm = blendedAncestorMat(li, frameIdx);
      return bm ? [{ mat: bm, pivot: { x: 0, y: 0 } }] : [];
    }
    return legacyParentChainMats(li, frameIdx);
  }
  function legacyParentChainMats(li, frameIdx) {
    var mats = [];
    var ld = state.layers[li];
    if (!ld) return mats;
    var visited = {};
    visited[li] = true;
    // layerParentUidAt (feedback #207) resolves ld.parentKeys when present,
    // falling back to the plain static field otherwise — the ONE place
    // this chain-walker needs to become frame-aware for keyed re-parenting
    // to actually change what renders.
    var curUid = layerParentUidAt(li, frameIdx);
    var guard = 0;
    while (curUid && guard++ < 64) {
      var idx = findLayerIndexByUid(curUid);
      if (idx < 0 || visited[idx]) break;
      visited[idx] = true;
      var m = computeMotionMat(state.layers[idx], frameIdx);
      // A Null has no Paper.js geometry, so no userLayers[idx].bounds to
      // pivot from — its own on-canvas marker is nullPos (world anchor,
      // see buildNullLayerItems/engine-bridge.js), not a shape's
      // bounds.center. Without this branch a Null contributed NOTHING to
      // its children's transform: `m` above only carries the Position
      // TRACK offset (dx/dy), and the `bounds` check below always failed
      // for a content-less layer — so dragging a Null (or keying its
      // Position) never moved anything parented to it (2026-08 fix).
      if (state.layers[idx].isNullLayer) {
        if (m) {
          var nBase = state.layers[idx].nullPos || [state.canvasW / 2, state.canvasH / 2];
          mats.push({ mat: m, pivot: { x: nBase[0] + m.ax, y: nBase[1] + m.ay } });
        }
      } else if (m && userLayers[idx] && userLayers[idx].bounds) {
        mats.push({ mat: m, pivot: { x: userLayers[idx].bounds.center.x + m.ax, y: userLayers[idx].bounds.center.y + m.ay } });
      }
      curUid = layerParentUidAt(idx, frameIdx);
    }
    return mats;
  }
  // World-space DELTA VECTOR -> local space, undoing only the PARENT
  // chain's rotation/scale (no translation/pivot — a vector between two
  // points transforms via the linear part alone). Shared by every canvas
  // drag that writes a Position DELTA on a parented layer (2026-08 fix,
  // feedback: "si je bouge un élément parenté... à droite il va en bas"
  // — select-bridge.js's body-drag 'move' mode added the raw mouse delta
  // straight to Position with zero parent compensation; the layer's OWN
  // rotation was already excluded there via layerMotionPointMap/invVec,
  // correctly, per computeMotionMat's "Position lives in POST-own-
  // rotation, PRE-parent-chain space" — this fills the other half). Undone
  // outermost-ancestor-first, mirroring every other parent-chain inverse
  // in this file (composedPivotWorld, motionBoxGeom.inv, outerLocalPoint).
  function invertVectorThroughParentChain(li, frameIdx, dx, dy) {
    var chain = parentChainMats(li, frameIdx);
    for (var i = chain.length - 1; i >= 0; i--) {
      var m = chain[i].mat;
      var rad = -m.rot * Math.PI / 180, c = Math.cos(rad), s = Math.sin(rad);
      var rx = dx * c - dy * s, ry = dx * s + dy * c;
      dx = rx / (m.sx || 1); dy = ry / (m.sy || 1);
    }
    return [dx, dy];
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
  // A colour CHANNEL is a u8 on the Rust side ([u8; 4] in engine.rs), and an
  // interpolated track value is a plain float: halfway between two keys the
  // red channel is 248.3356765206181, and serde refuses it outright —
  // "invalid type: floating point ..., expected u8". That throw is a WASM
  // trap, so it does not just drop one frame: it poisons the whole module
  // and disables the engine for the rest of the session (see the render
  // catch in engine-bridge.js). Hence feedback #194's symptom, "la couleur
  // fill de la shape ne s'anime pas dans le canvas": the key frames
  // themselves are integers and render fine, and the FIRST frame between
  // two keys kills the renderer, which then silently falls back to Paper.js
  // — which knows nothing about colour tracks and paints the shape's
  // original colour.
  //
  // Rounded here, at the single place both colour readers return from,
  // rather than at each engine call site: a fractional colour channel is
  // meaningless to every consumer, not just this one.
  function rgba255(v) {
    if (!v) return v;
    return v.map(function (c) {
      var n = Math.round(c);
      return n < 0 ? 0 : (n > 255 ? 255 : n);
    });
  }
  function elementFillColorAt(li, strokeId, frameIdx) {
    var ld = state.layers[li];
    if (!ld || !ld.elementMotion) return null;
    var holder = ld.elementMotion[strokeId];
    if (!holder) return null;
    if (!hasKeys(holder, 'fillColor') && !(holder.motionStatic && holder.motionStatic.fillColor)) return null;
    return rgba255(valueAtFrame(holder, 'fillColor', frameIdx));
  }
  // Extended per-shape properties: Stroke color / Stroke width (2026-08 —
  // second slice of the "propriétés étendues par forme" chantier, same
  // exact shape as Fill color above). Stroke color is [r,g,b,a] 0-255 like
  // Fill; stroke width is a single scalar, so it's stored as the usual
  // 1-element array every other scalar prop uses (rotation/opacity) and
  // unwrapped here so callers get a plain number, not an array — matching
  // what engine-bridge.js's own item.strokeWidth already expects.
  function elementStrokeColorAt(li, strokeId, frameIdx) {
    var ld = state.layers[li];
    if (!ld || !ld.elementMotion) return null;
    var holder = ld.elementMotion[strokeId];
    if (!holder) return null;
    if (!hasKeys(holder, 'strokeColor') && !(holder.motionStatic && holder.motionStatic.strokeColor)) return null;
    return rgba255(valueAtFrame(holder, 'strokeColor', frameIdx));
  }
  function elementStrokeWidthAt(li, strokeId, frameIdx) {
    var ld = state.layers[li];
    if (!ld || !ld.elementMotion) return null;
    var holder = ld.elementMotion[strokeId];
    if (!holder) return null;
    if (!hasKeys(holder, 'strokeWidth') && !(holder.motionStatic && holder.motionStatic.strokeWidth)) return null;
    return valueAtFrame(holder, 'strokeWidth', frameIdx)[0];
  }
  // Order (2026-08, "système pour animer l'id index de calque ou de shape/
  // éléments") — unlike Fill/Stroke/Brush above, this NEVER returns null:
  // every holder has a z-position (default 0, same neutral CSS-z-index
  // meaning as everyone else), so engine-bridge.js's stable sort can just
  // call this unconditionally for whichever items it already knows have
  // *some* order track (see its own _anyLayerOrder/_anyElemOrder guards —
  // the guard lives THERE, not here, so this stays a plain, cheap read).
  function layerOrderAt(li, frameIdx) {
    var ld = state.layers[li];
    if (!ld) return 0;
    // feedback #215 follow-up ("la value si non keyframé... corresponde
    // à son ordre index dans la timeline") — an untouched layer used to
    // read as a flat neutral 0 regardless of where it actually sits in
    // the stack, which collided with any small explicit rank set on
    // ANOTHER layer (see engine-bridge.js's own comment on the sort that
    // consumes this). Reading its OWN natural front-to-back position
    // instead — same "1 = topmost" counting convention explicit values
    // already use (feedback #205) — means every layer, touched or not,
    // carries a real, correctly-scaled rank: no collision, and the field
    // now shows something meaningful instead of a placeholder 0 the
    // instant you open it.
    if (!isAnimated(ld, 'order') && !(ld.motionStatic && ld.motionStatic.order)) {
      return state.layers.length - li;
    }
    return valueAtFrame(ld, 'order', frameIdx)[0];
  }
  // Whether THIS layer's Order was actually set by the user, vs. reading
  // as its natural-rank fallback above. The z-stack sort needs this
  // because a natural rank can legitimately tie with another layer's
  // EXPLICIT rank (a 3-layer document: an untouched back layer naturally
  // reads as "3", but so would a middle layer explicitly told "put me at
  // rank 3") — ties broken by document index alone would then leave the
  // explicit pick indistinguishable from a coincidence, silently doing
  // nothing. See its one call site (engine-bridge.js) for how the tie
  // actually resolves once this is known.
  function layerHasExplicitOrder(li) {
    var ld = state.layers[li];
    return !!(ld && (isAnimated(ld, 'order') || (ld.motionStatic && ld.motionStatic.order)));
  }
  function elementOrderAt(li, strokeId, frameIdx) {
    var ld = state.layers[li];
    if (!ld || !ld.elementMotion) return 0;
    var holder = ld.elementMotion[strokeId];
    if (!holder) return 0;
    return valueAtFrame(holder, 'order', frameIdx)[0];
  }
  // ---- Blend Mode keyframing (2026-08-31, feedback #207: "inspire-toi de
  // Duik") — Duik's own time-varying rig properties (e.g. re-parenting a
  // picked-up prop) never interpolate the underlying identity/enum value;
  // they SNAP at each key and hold until the next one, because there's no
  // meaningful halfway point between "Multiply" and "Screen" the way there
  // is between two numbers. ld.blendKeys is a plain [{frame,mode}] array,
  // sorted by frame — NOT routed through the generic numeric holder.motion
  // track system (which assumes lerp-able number arrays throughout;
  // shoehorning a mode NAME through it would need a fragile index<->name
  // encoding for no real benefit here, since "hold-only" is this whole
  // feature's point, not an edge case of it).
  // Opt-in, zero cost unless used: ld.blendKeys is absent for every layer
  // that has never turned this on, in which case the plain static
  // ld.blendMode (completely unchanged, still what most consumers read —
  // see this function's own callers list below) applies at every frame,
  // same contract Order/3D/Widget layers already establish elsewhere in
  // this file.
  function layerBlendModeAt(li, frameIdx) {
    var ld = state.layers[li];
    if (!ld) return undefined;
    var keys = ld.blendKeys;
    if (!keys || !keys.length) return ld.blendMode;
    var v = keys[0].mode;
    for (var i = 0; i < keys.length; i++) {
      if (keys[i].frame <= frameIdx) v = keys[i].mode; else break;
    }
    return v;
  }
  function sortBlendKeys(ld) {
    ld.blendKeys.sort(function (a, b) { return a.frame - b.frame; });
  }
  // Add/update the key AT this frame. pushUndo() is the CALLER's job,
  // matching every other write helper in this file.
  function upsertBlendKeyAt(ld, frame, mode) {
    if (!ld.blendKeys) ld.blendKeys = [];
    var k = ld.blendKeys.filter(function (kk) { return kk.frame === frame; })[0];
    if (k) k.mode = mode; else { ld.blendKeys.push({ frame: frame, mode: mode }); sortBlendKeys(ld); }
  }
  function removeBlendKeyAt(ld, frame) {
    if (!ld.blendKeys) return;
    ld.blendKeys = ld.blendKeys.filter(function (k) { return k.frame !== frame; });
  }
  // ---- Parent keyframing (2026-08-31, feedback #207: "et le parentage
  // doit être keyframable") — same shape and same reasoning as Blend Mode
  // right above: Duik's own "pick up an object" gesture re-parents at a
  // single frame, no interpolation (there's no meaningful halfway point
  // between two different parent layers). ld.parentKeys is a plain
  // [{frame,uid}] array (uid null = "no parent"), sorted by frame, NOT
  // routed through the numeric holder.motion track system, for the exact
  // same reason blendKeys isn't. Opt-in: absent ld.parentKeys means every
  // reader falls back to the plain static ld.parentLayerUid, unchanged.
  //
  // Deliberately scoped to the PRIMARY parent only — parentLayerUidB (the
  // weighted second-parent crossfade, 2026-07-30) is a separate, already-
  // intricate feature (composedPivotWorld, blendedAncestorMat, the
  // parentBlend track) that this does not touch or extend; a layer with a
  // second parent set keeps behaving exactly as it already did.
  //
  // Also deliberately WITHOUT setLayerParent's "keep transform" position
  // compensation (see that function's own note where it branches on
  // ld.parentKeys) — computing the right compensating Position KEY (not a
  // static overwrite, which would wrongly affect every earlier frame too)
  // for a re-parent that only takes effect at one specific frame is real
  // additional complexity this pass intentionally left out rather than
  // risk getting subtly wrong. A keyed re-parent can visibly "pop" at its
  // frame; keying Position there too is the manual fix, same as it would
  // be in any other tool that doesn't auto-compensate.
  function layerParentUidAt(li, frameIdx) {
    var ld = state.layers[li];
    if (!ld) return null;
    var keys = ld.parentKeys;
    if (!keys || !keys.length) return ld.parentLayerUid || null;
    var v = keys[0].uid;
    for (var i = 0; i < keys.length; i++) {
      if (keys[i].frame <= frameIdx) v = keys[i].uid; else break;
    }
    return v;
  }
  function sortParentKeys(ld) {
    ld.parentKeys.sort(function (a, b) { return a.frame - b.frame; });
  }
  function upsertParentKeyAt(ld, frame, uid) {
    if (!ld.parentKeys) ld.parentKeys = [];
    var k = ld.parentKeys.filter(function (kk) { return kk.frame === frame; })[0];
    if (k) k.uid = uid || null; else { ld.parentKeys.push({ frame: frame, uid: uid || null }); sortParentKeys(ld); }
  }
  function removeParentKeyAt(ld, frame) {
    if (!ld.parentKeys) return;
    ld.parentKeys = ld.parentKeys.filter(function (k) { return k.frame !== frame; });
  }
  // Cheap "does ANY layer/element on this layer actually use Order" scans —
  // engine-bridge.js calls these ONCE per buildSceneJson / once per layer
  // respectively, so the stable-sort + per-item lookup below them is fully
  // skipped (zero overhead) for the overwhelming default case of a
  // document that never touches this property, same "test the cheap guard
  // first" discipline as this codebase's own §5bis perf lessons.
  function anyLayerHasOrder() {
    for (var k = 0; k < state.layers.length; k++) {
      var ld = state.layers[k];
      if ((ld.motion && ld.motion.order) || (ld.motionStatic && ld.motionStatic.order)) return true;
    }
    return false;
  }
  function layerElementsHaveOrder(ld) {
    if (!ld.elementMotion) return false;
    for (var k in ld.elementMotion) {
      var h = ld.elementMotion[k];
      if (h && ((h.motion && h.motion.order) || (h.motionStatic && h.motionStatic.order))) return true;
    }
    return false;
  }
  // Whole-document check (layer OR any element on any layer) — export.js's
  // exportHasEngineOnlyMotion uses this to decide whether a render needs to
  // route through the engine (buildSceneJson, engine-bridge.js) instead of
  // its own plain-Paper.js fallback loop, which has no idea what Order even
  // is, same "engine-only feature, route the whole export through the
  // engine rather than reimplement it a second time" fix already applied to
  // 3D layers/Motion Blur.
  function anyOrderUsedAnywhere() {
    if (anyLayerHasOrder()) return true;
    for (var k = 0; k < state.layers.length; k++) if (layerElementsHaveOrder(state.layers[k])) return true;
    return false;
  }
  // Extended per-shape property: Brush size (2026-08 — third slice of the
  // "propriétés étendues par forme" chantier, the "brush" piece of
  // fill/stroke/brush/path). A vector-brush stroke's ink IS its fill (see
  // CLAUDE.md's own note on isVectorBrush), so this is genuinely new
  // ground, not something Fill/Stroke color already cover — it's a % scale
  // on the ribbon's whole width profile, same unit convention as the base
  // Scale property. Same shape as ParamShape (hasXFor/applyXFor rebuilding
  // segments), not the simple item-field-override Fill/Stroke color use,
  // because scaling width means re-deriving the outline geometry, not just
  // overriding a paint field.
  function hasBrushSizeMotionFor(li, strokeId) {
    var ld = state.layers[li]; if (!ld) return false;
    var holder = elementHolder(ld, strokeId);
    if (!holder) return false;
    return isAnimated(holder, 'brushSize') || !!(holder.motionStatic && holder.motionStatic.brushSize);
  }
  // sd.centerSegments/sd.widthProfile are the shape's OWN static pressure
  // recording (serP's output, app.js) — untouched by this scale, so
  // repeated calls across frames always start from the same source instead
  // of compounding. Scoped to the case Trim Paths' own vector-brush special
  // case (engine-bridge.js) does NOT already handle — composing an
  // animated Trim window with an animated Brush Size on the SAME stroke at
  // the same time is a real future case, not attempted here (see the call
  // site's own comment for the exact boundary).
  // A vector-brush ribbon's EDITABLE geometry is its centerline, never the
  // baked outline. Every other tool in the app already knows this —
  // nodeEditSegmentsData (tools.js) hands Select/Subselect the centerline —
  // and Motion was the one place reading the outline instead. Measured on one
  // ordinary stroke for feedback #181: 5 points under Select, 476 vertex rows
  // in Motion ("le path dans motion à plus de path que avec l'outil select.
  // Il faudrait le même nombre de point... même si celui ci est lissé").
  //
  // The counts disagreeing was the visible half. The invisible half was
  // worse: vtx0..vtx4 were being applied to outline points 0..4, so keying
  // "vertex 0" nudged an arbitrary point on the ribbon's edge instead of the
  // stroke's first node.
  function isVectorBrushSd(sd) {
    return !!(sd && sd.isVectorBrush && sd.centerSegments && sd.centerSegments.length >= 2);
  }
  // Row count for the Path accordion — panel AND grid read it through here,
  // so the §11 alignment invariant cannot drift between the two.
  function pathVertexRowCount(sd) {
    return isVectorBrushSd(sd) ? sd.centerSegments.length : ((sd && sd.segments) ? sd.segments.length : 0);
  }
  // Same list, from the LIVE Paper item, as {x,y} in the shape's local space:
  // what the on-canvas vertex dots draw and hit-test against. Mirrors
  // nodeEditSegmentsData's choice exactly — the two must agree or a dot sits
  // on a point Motion cannot key.
  function elementVertexPoints(item) {
    if (!item) return [];
    if (item.data && item.data.isVectorBrush && item.data.centerSegments && item.data.centerSegments.length >= 2)
      return item.data.centerSegments.map(function (s) { return { x: s.point[0], y: s.point[1] }; });
    return (item.segments || []).map(function (s) { return { x: s.point.x, y: s.point.y }; });
  }
  function hasVectorBrushOutlineMotionFor(li, strokeId) {
    var ld = state.layers[li]; if (!ld) return false;
    var holder = elementHolder(ld, strokeId);
    if (!holder) return false;
    return hasBrushSizeMotionFor(li, strokeId) || hasPathVertexMotion(holder);
  }
  // ONE rebuild of the ribbon from its centerline, carrying both Motion edits
  // that live on that centerline: per-vertex offsets (#181) and Brush Size
  // (#178). Kept as a single pass rather than two chained ones because both
  // consume the SAME source (sd.centerSegments + sd.widthProfile) and each
  // produces an outline — running them in sequence would mean re-deriving a
  // centerline from an outline, which is exactly the wedge/sliver family of
  // bug the Trim work already hit.
  //
  // sd.centerSegments/sd.widthProfile are the shape's own static recording
  // (serP's output, app.js) — untouched here, so repeated calls across frames
  // always start from the same source instead of compounding.
  function applyVectorBrushOutlineFor(li, strokeId, sd, frameIdx) {
    if (!isVectorBrushSd(sd)) return null;
    var ld = state.layers[li]; var holder = ld && elementHolder(ld, strokeId);
    if (!holder) return null;
    if (!window.sampleVectorBrushCenterline || !window.buildVariableWidthPath) return null;
    var center = applyPathVertexOffsets(sd.centerSegments, holder, frameIdx);
    var sampled = window.sampleVectorBrushCenterline(center, sd.widthProfile);
    var widths = sampled.widths;
    // Read brushSize only when it actually carries a value: valueAtFrame
    // falls back to a prop's PROP_DEFAULT, and brushSize has none, so an
    // unconditional read returns 0 — a scale of 0, i.e. the ribbon silently
    // disappearing on every stroke that merely has a keyed vertex.
    if (isAnimated(holder, 'brushSize') || (holder.motionStatic && holder.motionStatic.brushSize)) {
      var scale = valueAtFrame(holder, 'brushSize', frameIdx)[0] / 100;
      widths = widths.map(function (w) { return w * scale; });
    }
    var outline = window.buildVariableWidthPath(sampled.pts, widths);
    if (!outline) return null;
    var segs = outline.segments.map(function (s) { return { point: [s.point.x, s.point.y], handleIn: [s.handleIn.x, s.handleIn.y], handleOut: [s.handleOut.x, s.handleOut.y] }; });
    outline.remove();
    return segs;
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
    // Expressions count as motion (2026-08-30). This checked keys and static
    // values only, so a vertex driven PURELY by an expression — no keyframe,
    // no static override — was invisible here, and every reader gated on this
    // function (meshVertexOffsetAt, applyPathVertexOffsets) skipped it
    // entirely: the expression evaluated to the right number and nothing ever
    // asked for it. Harmless while vertices were only ever keyed by hand;
    // load-bearing now that one expression over self.vertexIndex/vertexUV is
    // the sane way to animate a 69-vertex mesh.
    if (holder.expressions) for (k in holder.expressions) if (k.indexOf('vtx') === 0 && hasExpr(holder, k)) return true;
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

  // ---- Image mesh vertices (2026-08-30, image-mesh.js) ----
  //
  // An image mesh's vertices key through the SAME vtxN machinery a Path's
  // vertices already use — same holder shape, same tracks, so mesh
  // animation inherits keyframes, ease curves, the graph editor,
  // expressions and the expression controls for free. Two deliberate
  // differences, both forced by what an image is:
  //
  // 1. The holder is keyed by **meshId**, not strokeId. A raster has no
  //    stable strokeId across frames: layerElements stamps one lazily onto
  //    the frame's own dict, and a still image's dict is a SEPARATE object
  //    literal per frame (images.js's import loop), so frame 5 would get a
  //    different id from frame 0 and the animation would vanish on scrub.
  //    meshId is written to every frame by SMImageMesh.propagate, which
  //    makes it the only id an imported still actually carries throughout.
  //
  // 2. The value is in PERCENT of the image's own size, not pixels. The
  //    mesh itself is stored normalized over the raster's display rect
  //    (image-mesh.js's header) so it survives moving/scaling the image;
  //    percent is that same unit made readable in the graph editor, where
  //    "20" beats "0.2". meshVertexOffsetAt does the /100 once, here, so
  //    no caller has to remember it.
  //
  // This offset is ADDED to the mesh's own static pose (mesh.offsets),
  // which stays the rest sculpt — the stopwatch is what decides which of
  // the two an on-canvas drag writes to, exactly like every other Motion
  // property.
  function meshHolder(li, meshId) {
    var ld = state.layers[li];
    if (!ld || !ld.elementMotion || !meshId) return null;
    return ld.elementMotion[meshId] || null;
  }
  function hasMeshVertexMotionFor(li, meshId) {
    return hasPathVertexMotion(meshHolder(li, meshId));
  }
  // [du, dv] in NORMALIZED units for one vertex at one frame, or null when
  // this mesh has no vertex tracks at all (the common case — one cheap
  // lookup and out, so an un-animated mesh costs nothing per frame).
  function meshVertexOffsetAt(li, meshId, vi, frameIdx) {
    var h = meshHolder(li, meshId);
    if (!h || !hasPathVertexMotion(h)) return null;
    var v = valueAtFrame(h, 'vtx' + vi, frameIdx);
    if (!v || (!v[0] && !v[1])) return null;
    return [v[0] / 100, v[1] / 100];
  }
  // Writes a vertex's ANIMATED offset (normalized in, percent out) through
  // the ordinary setValue path: a key at the playhead when the stopwatch is
  // on, a static override when it isn't.
  function setMeshVertexOffset(li, meshId, vi, du, dv) {
    var ld = state.layers[li];
    if (!ld || !meshId) return false;
    setValue(ensureElementHolder(ld, meshId), 'vtx' + vi, [du * 100, dv * 100]);
    return true;
  }
  function isMeshVertexAnimated(li, meshId, vi) {
    var h = meshHolder(li, meshId);
    return !!h && isAnimated(h, 'vtx' + vi);
  }
  // How many vertex rows the timeline will list for a mesh. Shared by the
  // panel and the grid so the two can NEVER disagree about the row count —
  // CLAUDE.md §11's alignment invariant is the constraint that matters most
  // in this file, and a dense mesh (32x32 = 1089 vertices) is exactly the
  // case where an ad-hoc cap on one side only would drift. Capped because
  // rendering four figures' worth of rows on both sides would lock the UI;
  // the vertices past the cap are still animatable by dragging them on
  // canvas, they just have no row.
  var MESH_ROW_CAP = 200;
  function meshVertexRowCount(meshId) {
    if (!window.SMImageMesh) return 0;
    var m = SMImageMesh.get(meshId);
    if (!m || !m.verts) return 0;
    return Math.min(m.verts.length, MESH_ROW_CAP);
  }
  // ---- Trim Paths (2026-08, "animer les stroke en in et out") ----
  // AE's own feature: Start/End (0-100%, position along the path's arc
  // length) plus Offset (shifts the whole window). Opt-in — untouched
  // (0/100/0) costs one cheap hasTrimMotion lookup and returns the segments
  // unchanged, same convention as vtxN/fillColor above.
  function hasTrimMotion(holder) {
    if (!holder) return false;
    if (hasKeys(holder, 'trimStart') || hasKeys(holder, 'trimEnd') || hasKeys(holder, 'trimOffset')) return true;
    var ms = holder.motionStatic;
    return !!(ms && (ms.trimStart || ms.trimEnd || ms.trimOffset));
  }
  function trimWindowAt(li, strokeId, frameIdx) {
    var ld = state.layers[li];
    if (!ld || !ld.elementMotion) return null;
    var holder = ld.elementMotion[strokeId];
    if (!hasTrimMotion(holder)) return null;
    return { start: valueAtFrame(holder, 'trimStart', frameIdx)[0], end: valueAtFrame(holder, 'trimEnd', frameIdx)[0], offset: valueAtFrame(holder, 'trimOffset', frameIdx)[0] };
  }
  function cubicPointAt(p0, p1, p2, p3, t) {
    var mt = 1 - t;
    var a = mt * mt * mt, b = 3 * mt * mt * t, c = 3 * mt * t * t, d = t * t * t;
    return [a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0], a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1]];
  }
  // Flattens `segments` into a dense polyline for arc-length sampling — 20
  // samples per original curve segment is enough to look smooth at normal
  // zoom while staying cheap (a shape rarely has more than a few dozen
  // segments). SAMPLES_PER_SEG intentionally NOT exposed as a setting: v1
  // scope, see applyTrimSegments' own doc comment on the polyline tradeoff.
  var TRIM_SAMPLES_PER_SEG = 20;
  function buildTrimPolyline(segments, closed) {
    var n = segments.length;
    var segCount = closed ? n : n - 1;
    var pts = [segments[0].point.slice()];
    for (var i = 0; i < segCount; i++) {
      var a = segments[i], b = segments[(i + 1) % n];
      var p0 = a.point, p1 = [a.point[0] + a.handleOut[0], a.point[1] + a.handleOut[1]];
      var p2 = [b.point[0] + b.handleIn[0], b.point[1] + b.handleIn[1]], p3 = b.point;
      for (var s = 1; s <= TRIM_SAMPLES_PER_SEG; s++) pts.push(cubicPointAt(p0, p1, p2, p3, s / TRIM_SAMPLES_PER_SEG));
    }
    return pts;
  }
  // Extracts the [startPct,endPct] (+offsetPct shift) window of `segments`'
  // own arc length, returning {segments, closed} for the trimmed portion —
  // NEW straight-line segments (handleIn/handleOut [0,0]), not a partial
  // reconstruction of the original beziers: a deliberate v1 tradeoff (exact
  // partial-bezier math is a much bigger undertaking) mitigated by sampling
  // densely enough (TRIM_SAMPLES_PER_SEG) that the polyline reads as smooth
  // curve at normal zoom, same spirit as the mask feature's own stated v1
  // simplifications.
  //
  // Offset shifts start/end together. A CLOSED path wraps the window
  // cyclically (the common "progress ring" case — no seam to speak of). An
  // OPEN path instead CLIPS the window to the path's actual [0,100] extent
  // (portion pushed past either end is simply not drawn) rather than
  // wrapping through a nonexistent loop — matches the overwhelmingly common
  // write-on/off use case; true AE-style split-into-two-arcs wraparound on
  // an open path is a known, explicitly out-of-scope gap.
  function applyTrimSegments(segments, closed, win) {
    if (!segments || segments.length < 2) return { segments: segments, closed: closed };
    var s = win.start, e = win.end, off = win.offset || 0;
    s += off; e += off;
    if (e <= s) return { segments: [], closed: false }; // fully collapsed window
    var pts = buildTrimPolyline(segments, closed);
    var cum = [0];
    for (var i = 1; i < pts.length; i++) {
      var dx = pts[i][0] - pts[i - 1][0], dy = pts[i][1] - pts[i - 1][1];
      cum.push(cum[i - 1] + Math.sqrt(dx * dx + dy * dy));
    }
    var total = cum[cum.length - 1];
    if (total <= 0) return { segments: segments, closed: closed };
    function lenAtPct(pct) {
      if (closed) { pct = ((pct % 100) + 100) % 100; return (pct / 100) * total; }
      return Math.max(0, Math.min(100, pct)) / 100 * total;
    }
    function pointAtLen(len) {
      // Wrap ONLY for a closed path — the cyclic "progress ring" case this
      // function's own header comment describes, where a window running past
      // 100% legitimately continues through the seam.
      //
      // On an OPEN path the window is already clamped to [0,total] by
      // lenAtPct, so the modulo could only ever fire on the single exact
      // value len === total — where `total % total` is 0 and it returned the
      // path's START point instead of its END. That put the last vertex of
      // the trimmed result back at the beginning of the stroke, drawing a
      // long spurious segment all the way back across the shape.
      // Bug found live 2026-08-21 (QA on brush types): trimStart=60/
      // trimEnd=100 on an open stroke returned first point x=1015 (right)
      // but last point x=129 (the start) instead of 1606; trimStart=60/
      // trimEnd=99 was correct, which is what pinned it to the exact
      // endpoint. Bites the DEFAULT trimEnd of 100 too, so every partially
      // trimmed open shape was affected, not an edge case.
      // applyTrimToVectorBrush's own sampleAtLen already clamps this way —
      // this brings the two into line (CLAUDE.md §3's duplicated-pair trap).
      if (closed) len = ((len % total) + total) % total;
      else len = Math.max(0, Math.min(total, len));
      for (var i2 = 1; i2 < cum.length; i2++) {
        if (cum[i2] >= len) {
          var segLen = cum[i2] - cum[i2 - 1];
          var t = segLen > 0 ? (len - cum[i2 - 1]) / segLen : 0;
          return [pts[i2 - 1][0] + (pts[i2][0] - pts[i2 - 1][0]) * t, pts[i2 - 1][1] + (pts[i2][1] - pts[i2 - 1][1]) * t];
        }
      }
      return pts[pts.length - 1].slice();
    }
    var lenS = lenAtPct(s), lenE = lenAtPct(closed ? s + Math.min(100, e - s) : e);
    if (!closed && lenE <= lenS) return { segments: [], closed: false };
    var windowLen = closed ? Math.min(total, (e - s) / 100 * total) : (lenE - lenS);
    if (windowLen <= 0) return { segments: [], closed: false };
    var out = [{ point: pointAtLen(lenS), handleIn: [0, 0], handleOut: [0, 0] }];
    var STEP = total / (pts.length - 1); // ~one polyline sample's worth of length
    for (var d = STEP; d < windowLen; d += STEP) out.push({ point: pointAtLen(lenS + d), handleIn: [0, 0], handleOut: [0, 0] });
    out.push({ point: pointAtLen(lenS + windowLen), handleIn: [0, 0], handleOut: [0, 0] });
    return { segments: out, closed: false };
  }
  // Resolves the element holder + applies the trim in one call, same
  // encapsulation shape as applyPathVertexOffsetsFor — engine-bridge.js/
  // export.js never need to know ld.elementMotion is where this lives.
  // Returns {segments, closed} always (a no-op trim returns the INPUT
  // segments/closed unchanged, not a copy — cheap common path).
  // KNOWN GAP (v1): wired into engine-bridge.js's live render only, NOT
  // export.js — export.js's own vertex-offset application mutates a LIVE
  // Path's EXISTING segments in place, index-for-index; Trim Paths changes
  // both the segment COUNT (a sub-arc has fewer points than the source)
  // and `closed`, which that in-place-mutation shape can't accommodate
  // without a larger rework of that function. A trimmed shape currently
  // exports un-trimmed (full path) until that's addressed.
  // "Trim Paths is on, but its window covers the whole path" — i.e. nothing
  // is actually trimmed (2026-08-31, feedback #182: "pourquoi un trim path
  // peut supprimé le fill d'une shape ?"). Merely ENABLING the property used
  // to rebuild the shape as an open polyline approximation and drop its
  // fill, before the artist had trimmed anything: measured on a plain filled
  // rectangle, 4 closed segments with a fill became 81 open ones with none.
  // Dropping the fill on a genuinely trimmed shape is deliberate and stays
  // (see engine-bridge's own comment — AE's convention, and the fix for the
  // "pac-man wedge" Cyril reported in August); doing it for a window that
  // trims nothing is just a property that breaks the drawing when switched
  // on.
  //
  // Offset is deliberately ignored: rotating a full window around a path
  // still keeps the whole path, so it changes nothing either.
  var TRIM_EPS = 0.001;
  function trimIsFullWindow(li, strokeId, frameIdx) {
    var win = trimWindowAt(li, strokeId, frameIdx);
    if (!win) return true;
    return (win.start || 0) <= TRIM_EPS && (win.end == null ? 100 : win.end) >= 100 - TRIM_EPS;
  }
  function applyTrimFor(li, strokeId, segments, closed, frameIdx) {
    var win = trimWindowAt(li, strokeId, frameIdx);
    if (!win) return { segments: segments, closed: closed };
    if (trimIsFullWindow(li, strokeId, frameIdx)) return { segments: segments, closed: closed };
    return applyTrimSegments(segments, closed, win);
  }
  // Vector-brush ribbon trim (2026-08-20) — "il faudrait mettre en place le
  // trim du coup": applyTrimSegments/applyTrimFor above trim the ribbon's
  // already-BAKED outline (the closed polygon rebuildVectorBrushOutline
  // produces), which has no notion of "one step along the stroke" — arc-
  // length-slicing that outline instead sliced across the ribbon's own
  // width, in the same wedge/degenerate-sliver family as the original
  // fill-wedge bug (see engine-bridge.js's isVectorBrush exclusion,
  // 18b2d9a). The fix mirrors that commit's own diagnosis: trim BEFORE
  // the fill-shape's geometry is built, on the shape's actual "spine" —
  // here that's the ribbon's centerline + width profile, not the outline.
  //
  // Reuses buildTrimPolyline unmodified: centerSegments already has the
  // exact same {point,handleIn,handleOut} shape buildTrimPolyline expects
  // (relative handles, Paper.js Segment convention — confirmed by
  // rebuildVectorBrushOutline's own `new Segment(point,handleIn,handleOut)`
  // call, tools.js), so passing centerSegments straight into it needs no
  // adapter. widthAtFrac (tools.js, window-exposed, pure math/no Paper
  // dependency) resolves each dense sample's width the same way
  // rebuildVectorBrushOutline already does — this function stays
  // Paper.js-free like the rest of motion.js; engine-bridge.js is the one
  // that turns the returned {pts,widths} into real Paper Points and feeds
  // buildVariableWidthPath (tools.js), since it already owns that Paper
  // object lifecycle for the untrimmed case.
  //
  // Returns null when there's no trim window (caller falls back to the
  // untrimmed centerSegments/widthProfile as-is) or the window collapses
  // to nothing; otherwise {pts:[[x,y],...], widths:[number,...]} — a
  // dense, already-windowed sample pair ready for buildVariableWidthPath.
  function applyTrimToVectorBrush(li, strokeId, centerSegments, widthProfile, frameIdx) {
    if (!centerSegments || centerSegments.length < 2) return null;
    var win = trimWindowAt(li, strokeId, frameIdx);
    if (!win) return null;
    var s = win.start, e = win.end, off = win.offset || 0;
    s += off; e += off;
    if (e <= s) return { pts: [], widths: [] };
    var pts = buildTrimPolyline(centerSegments, false);
    var cum = [0];
    for (var i = 1; i < pts.length; i++) {
      var dx = pts[i][0] - pts[i - 1][0], dy = pts[i][1] - pts[i - 1][1];
      cum.push(cum[i - 1] + Math.sqrt(dx * dx + dy * dy));
    }
    var total = cum[cum.length - 1];
    if (!(total > 0)) return null;
    var fallbackWidth = (widthProfile && widthProfile.length) ? widthProfile[widthProfile.length - 1].width : 1;
    var widthsArr = pts.map(function (_, idx) {
      var w = widthAtFrac(widthProfile, cum[idx] / total);
      return w == null ? fallbackWidth : w;
    });
    function lenAtPct(pct) { return Math.max(0, Math.min(100, pct)) / 100 * total; }
    function sampleAtLen(len) {
      len = Math.max(0, Math.min(total, len));
      for (var i2 = 1; i2 < cum.length; i2++) {
        if (cum[i2] >= len) {
          var segLen = cum[i2] - cum[i2 - 1];
          var t = segLen > 0 ? (len - cum[i2 - 1]) / segLen : 0;
          return {
            point: [pts[i2 - 1][0] + (pts[i2][0] - pts[i2 - 1][0]) * t, pts[i2 - 1][1] + (pts[i2][1] - pts[i2 - 1][1]) * t],
            width: widthsArr[i2 - 1] + (widthsArr[i2] - widthsArr[i2 - 1]) * t,
          };
        }
      }
      return { point: pts[pts.length - 1].slice(), width: widthsArr[widthsArr.length - 1] };
    }
    var lenS = lenAtPct(s), lenE = lenAtPct(e);
    if (lenE <= lenS) return { pts: [], widths: [] };
    var windowLen = lenE - lenS;
    var outPts = [], outWidths = [];
    var first = sampleAtLen(lenS); outPts.push(first.point); outWidths.push(first.width);
    var STEP = total / Math.max(1, pts.length - 1);
    for (var d = STEP; d < windowLen; d += STEP) {
      var smp = sampleAtLen(lenS + d);
      outPts.push(smp.point); outWidths.push(smp.width);
    }
    var last = sampleAtLen(lenE); outPts.push(last.point); outWidths.push(last.width);
    return { pts: outPts, widths: outWidths };
  }
  // Dynamic shapes phase 2 (2026-08-18) — same encapsulation shape as
  // applyTrimFor just above (engine-bridge.js never needs to know
  // ld.elementMotion is where this lives), for a rect's per-corner radius.
  // `sd` is the item's already-serialized dict (serP's output — see
  // getEffectiveStrokes' render-time note): sd.paramShape carries the
  // shape's OWN un-animated radii (each rect's real default, unlike every
  // other Motion prop which shares one constant — see PROP_DEFAULT's own
  // comment), read only for whichever corner ISN'T currently animated.
  // Geometry itself is NOT reimplemented here — buildRoundRectPath
  // (tools.js, window-exposed) is the one place that turns radii into
  // segments, so the static-edit path (Coins panel) and this per-frame
  // rebuild can never drift apart (CLAUDE.md §3).
  function hasParamShapeMotionFor(li, strokeId) {
    var ld = state.layers[li]; if (!ld) return false;
    var holder = elementHolder(ld, strokeId);
    if (!holder) return false;
    if (holder.paramShapeKind === 'rect') {
      return isAnimated(holder, 'cornerTL') || isAnimated(holder, 'cornerTR') || isAnimated(holder, 'cornerBR') || isAnimated(holder, 'cornerBL');
    }
    if (holder.paramShapeKind === 'ellipse') {
      return isAnimated(holder, 'arcStart') || isAnimated(holder, 'arcSweep') || isAnimated(holder, 'arcInner');
    }
    if (holder.paramShapeKind === 'star') {
      return isAnimated(holder, 'starInner') || isAnimated(holder, 'starCorner');
    }
    return false;
  }
  function applyParamShapeFor(li, strokeId, sd, frameIdx) {
    var ps = sd.paramShape; if (!ps) return sd.segments;
    var ld = state.layers[li]; var holder = ld && elementHolder(ld, strokeId);
    if (!holder) return sd.segments;
    function cv(key, fallback) { return isAnimated(holder, key) ? valueAtFrame(holder, key, frameIdx)[0] : fallback; }
    var xs = sd.segments.map(function (s) { return s.point[0]; }), ys = sd.segments.map(function (s) { return s.point[1]; });
    var x1 = Math.min.apply(null, xs), x2 = Math.max.apply(null, xs), y1 = Math.min.apply(null, ys), y2 = Math.max.apply(null, ys);
    var built;
    if (ps.kind === 'rect') {
      var tl = cv('cornerTL', ps.tl || 0), tr = cv('cornerTR', ps.tr || 0), br = cv('cornerBR', ps.br || 0), bl = cv('cornerBL', ps.bl || 0);
      built = window.buildRoundRectPath(x1, y1, x2, y2, tl, tr, br, bl);
    } else if (ps.kind === 'ellipse') {
      var startA = cv('arcStart', ps.startAngle || 0), sweepA = cv('arcSweep', ps.sweep !== undefined ? ps.sweep : 359.9), innerPct = cv('arcInner', Math.round((ps.innerRadius || 0) * 100));
      var cx = (x1 + x2) / 2, cy = (y1 + y2) / 2, rx = (x2 - x1) / 2, ry = (y2 - y1) / 2;
      built = window.buildArcEllipsePath(cx, cy, rx, ry, startA, sweepA, innerPct / 100);
    } else if (ps.kind === 'star') {
      var innerPctS = cv('starInner', Math.round((ps.innerRatio !== undefined ? ps.innerRatio : 0.5) * 100)), cornerR = cv('starCorner', ps.cornerRadius || 0);
      var cxs = (x1 + x2) / 2, cys = (y1 + y2) / 2, outerR = Math.min(x2 - x1, y2 - y1) / 2;
      built = window.buildStarPolygonPath(cxs, cys, outerR, ps.pointCount || 5, innerPctS / 100, cornerR);
    } else return sd.segments;
    var segs = built.segments.map(function (s) { return { point: [s.point.x, s.point.y], handleIn: [s.handleIn.x, s.handleIn.y], handleOut: [s.handleOut.x, s.handleOut.y] }; });
    built.remove();
    return segs;
  }
  // ---- Path-point parenting, "drive" direction (2026-08) — a vertex of
  // THIS path snaps onto another LAYER's resolved world position every
  // frame, tagged on the live item's data (data.pathVertexFollow, persisted
  // via serP/desP like data.paramShape) rather than in Motion state — it's
  // per-VERTEX config on the shape itself, not a keyable value. Same
  // render-time-rebuild shape as paramShape (hasXxxMotionFor/applyXxxFor,
  // §5ter/§1 of CLAUDE.md): both engine-bridge.js's buildSceneJson AND
  // export.js's exportBuildFrame must call these, or an exported frame
  // shows the vertex at its static authored position while the live canvas
  // shows it correctly attached.
  function hasPathVertexFollowMotionFor(li, strokeId) {
    var item = liveItemByStrokeId(li, strokeId);
    return !!(item && item.data && item.data.pathVertexFollow && item.data.pathVertexFollow.length);
  }
  function applyPathVertexFollowFor(li, strokeId, sd, frameIdx) {
    var item = liveItemByStrokeId(li, strokeId);
    var follows = item && item.data && item.data.pathVertexFollow;
    var segs = sd.segments.map(function (s) { return { point: s.point.slice(), handleIn: (s.handleIn || [0, 0]).slice(), handleOut: (s.handleOut || [0, 0]).slice() }; });
    if (!follows || !follows.length) return segs;
    follows.forEach(function (f) {
      if (f.vertexIndex == null || f.vertexIndex < 0 || f.vertexIndex >= segs.length) return;
      var targetLi = findLayerIndexByUid(f.targetLayerUid);
      if (targetLi < 0 || targetLi === li) return;
      var wp = resolveLayerWorldOrigin(targetLi, frameIdx);
      if (!wp) return;
      segs[f.vertexIndex].point = [wp[0] + (f.offset ? f.offset[0] : 0), wp[1] + (f.offset ? f.offset[1] : 0)];
    });
    return segs;
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
  // Same job as transformImageRect just above, but for a raw Paper.js
  // Matrix (camera/symMatrix's own shape — see app.js's symMatrixOf/
  // SMCamera.cameraMatrixAtFrame) instead of the decomposed {sx,sy,rot,dx,
  // dy} one every OTHER Motion transform here produces. Written for the
  // nested-video-in-Component fix (2026-07-30): a Component's own camera
  // and instance placement bake straight into stroke segments via
  // applyMatrixToStrokeData (app.js) for vector/raster content, but an
  // image RECT (nativeVideo's item shape) has no `.segments` for that
  // function's Matrix.transform(Point) calls to walk — this decomposes the
  // matrix's linear part into scale+rotation once instead (sqrt of each
  // column's squared length for scale, atan2 of the first column for
  // rotation — accurate for any similarity transform: translate+scale+
  // rotate, no skew, which is everything camera/symMatrix ever compose).
  function transformImageRectByMatrix(rb, m) {
    var cx = rb.x + rb.width / 2, cy = rb.y + rb.height / 2;
    var c = m.transform(new Point(cx, cy));
    var scaleX = Math.sqrt(m.a * m.a + m.b * m.b), scaleY = Math.sqrt(m.c * m.c + m.d * m.d);
    var rotDeg = Math.atan2(m.b, m.a) * 180 / Math.PI;
    var w = rb.width * scaleX, h = rb.height * scaleY;
    return { x: c.x - w / 2, y: c.y - h / 2, width: w, height: h, rotation: (rb.rotation || 0) + rotDeg };
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
  //
  // 2026-08 fix (feedback: "si je bouge un élément parenté à un élément
  // avec des transformation cela inverse les commande x" — also the
  // Motion PATH/dots drawing in the wrong place for a parented layer,
  // and the anchor-drag gesture on one): this used to return [] outright
  // for ANY layer-level target (`!t.strokeId`), on the assumption a plain
  // layer's own motionBoxGeom pivot was already "the" local point and
  // needed no outer wrapping. That's true for an UNPARENTED layer, but
  // the moment ld.parentLayerUid is set, the render pipeline
  // (getEffectiveStrokes/buildSceneJson, via parentChainMats) composes
  // the parent chain on top regardless — every consumer of
  // outerWorldPoint/outerLocalPoint (position path+dots draw, anchor
  // crosshair draw+hit-test+drag, position-dot drag, effector drag) needs
  // that SAME chain or it silently disagrees with where the object
  // actually renders, worse the more the parent is rotated/scaled. The
  // "add this layer's OWN motion" step stays gated on t.strokeId — that
  // part is ONLY relevant when nesting an ELEMENT inside its containing
  // layer's transform; for a plain layer target, t.holder already IS
  // that layer's own motion (baked into motionBoxGeom's fwd/pivot
  // directly), so re-adding it here would compose it twice.
  function outerMotionMaps(t){
    if(!t||t.li==null)return[];
    var out=[];
    if(t.strokeId){
      var lm=layerMotionAt(t.li,state.currentFrame);
      if(lm&&userLayers[t.li]){
        out.push({mat:lm,pivot:{x:userLayers[t.li].bounds.center.x+lm.ax,y:userLayers[t.li].bounds.center.y+lm.ay}});
      }
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
  // Hover highlight (2026-08, feedback: "quand on roll over un élément dans
  // le canvas un rec de bounding box de l'élément doit apparaitre comme
  // dans after effects") — AE shows this for whatever's under the cursor
  // regardless of selection, so it's computed and drawn OUTSIDE
  // buildOverlayItemsInner's early-return paths (multi-select union box,
  // unified multi-target box, or nothing selected at all) via the thin
  // wrapper below, not threaded through each of those branches.
  var _hoverLi = -1;
  // Cheap fingerprint of the hovered layer's CURRENT bounds (2026-08-31) —
  // see onHoverMove's own comment for why this exists alongside _hoverLi.
  function hoverBoundsSig(b) { return b ? (b.x + ',' + b.y + ',' + b.w + ',' + b.h) : null; }
  var _hoverBoundsSig = null;
  function hitTestLayerAt(pt, frame) {
    // Reverse (topmost-drawn-first) — same z-order convention the Null
    // marker hit-test already uses (select-bridge.js). Skips locked/hidden/
    // 3D layers (3D's screen bounds need the projector, out of scope here,
    // same exclusion multiLayerBox already makes) — layerWorldBoundsUnion
    // itself already returns null for a content-less layer (Null/Guide/
    // Effect all have zero-size Paper bounds), so no extra type check needed.
    for (var li = state.layers.length - 1; li >= 0; li--) {
      var ld = state.layers[li];
      if (!ld || ld.locked || ld.visible === false || ld.threeD) continue;
      var b = layerWorldBoundsUnion([li], frame);
      if (!b) continue;
      if (pt.x >= b.x && pt.x <= b.x + b.w && pt.y >= b.y && pt.y <= b.y + b.h) return li;
    }
    return -1;
  }
  // Returns true when the hover target changed (caller re-renders only
  // then, avoiding a redraw on every pixel of idle mouse movement).
  //
  // "changed" used to mean ONLY "the hovered LAYER INDEX is different from
  // last time" — but the box this drives (hoverOverlayItems) is drawn from
  // that layer's CURRENT bounds, which can change out from under an
  // unmoving mouse: drag that layer's elements apart in one gesture, exit,
  // dive into a DIFFERENT layer's group, then come back to hover the first
  // one — the index round-trips to the same value it already was (no drag
  // ever runs onHoverMove at all, per the caller's own onDrag-first gate),
  // so the box never got the redraw its now-stale geometry needed. Found
  // live chasing Cyril's report + screenshot: a group's hover box stopped
  // short of a rotated element sitting well outside the box's own stale
  // extent. A second fingerprint (the bounds themselves, not just which
  // layer they belong to) catches exactly this without paying for a
  // redraw on every idle pixel of movement — the common case (mouse
  // drifting inside one static layer) still short-circuits on the index
  // check before this ever runs a comparison.
  function onHoverMove(pt) {
    if (state.appMode !== 'motion') { var had = _hoverLi !== -1; _hoverLi = -1; _hoverBoundsSig = null; return had; }
    var li = hitTestLayerAt(pt, state.currentFrame);
    if (li !== _hoverLi) {
      _hoverLi = li;
      _hoverBoundsSig = li >= 0 ? hoverBoundsSig(layerWorldBoundsUnion([li], state.currentFrame)) : null;
      return true;
    }
    if (li < 0) return false;
    var sig = hoverBoundsSig(layerWorldBoundsUnion([li], state.currentFrame));
    if (sig === _hoverBoundsSig) return false;
    _hoverBoundsSig = sig;
    return true;
  }
  function hoverOverlayItems() {
    // Same fix as activeMotionTarget's own (2026-08, layer-list deselect
    // follow-up): state.activeLayerIdx never actually becomes "nothing"
    // (no such convention exists), so comparing _hoverLi against it raw
    // kept suppressing the hover box on the layer that WAS active even
    // after a genuine empty-canvas deselect — checked _layerSel (the
    // same source of truth the row highlight and activeMotionTarget both
    // already use) instead of the raw index.
    var curSel = (typeof _layerSel !== 'undefined') ? _layerSel : [];
    if (_hoverLi < 0 || curSel.indexOf(_hoverLi) >= 0) return [];
    var ld = state.layers[_hoverLi];
    if (!ld) return [];
    var b = layerWorldBoundsUnion([_hoverLi], state.currentFrame);
    if (!b) return [];
    var zs = 1 / Math.max(0.0001, view.zoom);
    // Solid blue, same accent as the selection box (2026-08 fix, feedback:
    // "Le roll hover ne marche pas... attention rectangle bleu pas de
    // tiret ni jaune") — it DID fire correctly (verified via
    // SMMotion.onHoverMove directly), the real problem was contrast: white
    // 170-alpha dashes on a white canvas background are nearly invisible,
    // reading as "broken" when it was actually just unseeable. Matches
    // buildOverlayItemsInner's own boxCol exactly, solid (no dashPattern),
    // so hover reads as a preview of that same selection box, not a
    // different/unrelated affordance.
    var col = [74, 158, 255, 204];
    var c1 = { x: b.x, y: b.y }, c2 = { x: b.x + b.w, y: b.y }, c3 = { x: b.x + b.w, y: b.y + b.h }, c4 = { x: b.x, y: b.y + b.h };
    return [[c1, c2], [c2, c3], [c3, c4], [c4, c1]].map(function (seg) {
      return { segments: [{ point: [seg[0].x, seg[0].y] }, { point: [seg[1].x, seg[1].y] }], closed: false, fillColor: null, strokeColor: col, strokeWidth: 1 * zs };
    });
  }
  function buildOverlayItems() {
    return buildOverlayItemsInner().concat(hoverOverlayItems()).concat(perObjectBoxItems());
  }
  // Sibling element boxes in MOTION (2026-08-30, feedback #170 "dans motion
  // le double clic dans le canvas... ne révèle toujours pas les box
  // individuelle"). Animation 2D draws these from buildTransformBoxItems
  // (engine-bridge), which returns [] outright in Motion — Motion has its
  // own complete overlay, and the two were deliberately kept from firing
  // together. So the same idea needs its own small builder here.
  // The DOUBLE-CLICKED element keeps Motion's real gizmo (drawn above via
  // activeMotionTarget/_motionExpandedElement); every other shape on the
  // layer gets a dim outline, so each one reads as individually targetable
  // — clicking its row, or double-clicking it on canvas, makes it active.
  function perObjectBoxItems() {
    // MOTION ONLY. Animation 2D draws its own per-object boxes from
    // buildTransformBoxItems (engine-bridge) — this builder exists because
    // that one returns [] in Motion. Without this gate both fired in
    // Animation 2D and every element got TWO boxes, in two slightly
    // different blues ([74,158,255,190] over [63,107,245,120]); found by
    // reading the emitted overlay layers, not by looking, because at a
    // 1px stroke the pair reads as one slightly-wrong box.
    if (state.appMode !== 'motion') return [];
    if (window._perObjBoxes !== state.activeLayerIdx) return [];
    var lyr = userLayers[state.activeLayerIdx];
    if (!lyr) return [];
    var items = [];
    var zs = 1 / Math.max(0.0001, view.zoom);
    // Same layer-level Motion map the gizmo itself uses, so a moved/rotated
    // layer carries its sibling boxes with it instead of leaving them at the
    // untransformed geometry.
    // Reached through the export rather than a bare call: layerMotionPointMap
    // is defined on the SMMotion object literal at the bottom of this file,
    // not as a local function in this closure.
    var map = (window.SMMotion && SMMotion.layerMotionPointMap) ? SMMotion.layerMotionPointMap(state.activeLayerIdx) : null;
    lyr.children.forEach(function (ch) {
      if (!ch.data || !ch.data.strokeId || ch.data.groupId) return;
      if (ch.data.isBrushTextureCopy || ch.data.isLinkedFillCompanion || ch.data.isDuplicatorCopy) return;
      // Skipped only when it IS the active one. With nothing targeted
      // (_motionExpandedElement null, the "entered the group but picked
      // nothing yet" state of #176) every shape draws its own box.
      if (window._motionExpandedElement && ch.data.strokeId === window._motionExpandedElement) return;
      // Posed bounds, not raw (#193): a sibling that has been moved through
      // Motion must carry its box with it, exactly like the active element's
      // own gizmo already does.
      var b = window.elementPosedBounds ? elementPosedBounds(lyr, ch) : ch.strokeBounds;
      if (!b || !b.width || !b.height) return;
      function P(x, y) { if (!map) return [x, y]; return map.fwd(x, y); }
      var c1 = P(b.left, b.top), c2 = P(b.right, b.top), c3 = P(b.right, b.bottom), c4 = P(b.left, b.bottom);
      items.push({ segments: [{ point: c1 }, { point: c2 }], closed: false, fillColor: null, strokeColor: [63, 107, 245, 120], strokeWidth: 1 * zs });
      items.push({ segments: [{ point: c2 }, { point: c3 }], closed: false, fillColor: null, strokeColor: [63, 107, 245, 120], strokeWidth: 1 * zs });
      items.push({ segments: [{ point: c3 }, { point: c4 }], closed: false, fillColor: null, strokeColor: [63, 107, 245, 120], strokeWidth: 1 * zs });
      items.push({ segments: [{ point: c4 }, { point: c1 }], closed: false, fillColor: null, strokeColor: [63, 107, 245, 120], strokeWidth: 1 * zs });
    });
    return items;
  }
  function buildOverlayItemsInner() {
    var ml = multiLayerBox();
    if (ml) return multiLayerOverlay(ml);
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
    // center + anchor offset, THEN this target's own position/rotation/
    // scale (motionBoxGeom's fwd, evaluated at the pivot itself — a
    // rotation/scale around a point never moves that point, so this is
    // exactly the render pivot, translated by Position same as the ring/
    // position-dot below), finally the outer/parent chain.
    // 2026-08-21 fix ("le cercle de rotation devrait être autour du point
    // d'ancrage vert"): this used to skip the target's OWN transform
    // entirely (deliberately, per a since-removed comment — "marks where
    // the pivot sits in the target's OWN unmoved bounds") while the
    // rotate ring/scale box (motionHandlePositions, below) and the
    // position keyframe dot (hitPositionDot et al.) both already included
    // it. The moment Position was ever non-[0,0], this crosshair stayed
    // stranded at the un-translated bounds+anchor point while the ring
    // and the position dot correctly moved together — confirmed live via
    // SMMotion.buildOverlayItems(): with position=[80,-40], anchor=
    // [100,50], the ring/dot centered at world (730,410) while this
    // crosshair stayed at (650,450). hitAnchorPoint (the drag hit-test)
    // and the 'anchor' onDrag handler are updated to match below, so the
    // visible marker and its grab zone never disagree again.
    var g0 = motionBoxGeom(t);
    var aw = g0 ? outerWorldPoint(t, g0.pivot) : outerWorldPoint(t, { x: bc.x + valueAtFrame(holder, 'anchor', state.currentFrame)[0], y: bc.y + valueAtFrame(holder, 'anchor', state.currentFrame)[1] });
    var ancCol = [80, 220, 140, 255];
    items.push({ segments: [{ point: [aw.x - 9 * zs, aw.y] }, { point: [aw.x + 9 * zs, aw.y] }], closed: false, fillColor: null, strokeColor: ancCol, strokeWidth: 1.5 * zs });
    items.push({ segments: [{ point: [aw.x, aw.y - 9 * zs] }, { point: [aw.x, aw.y + 9 * zs] }], closed: false, fillColor: null, strokeColor: ancCol, strokeWidth: 1.5 * zs });
    items.push({ segments: circleSegs(aw.x, aw.y, 6 * zs), closed: true, fillColor: null, strokeColor: ancCol, strokeWidth: 1.5 * zs });
    // Mograph effectors (2026-07-29) — layer-target only, only when this
    // layer has a duplicator with 1+ effectors. Each is a crosshair-in-
    // circle (same shape as the anchor point above, own color so the two
    // are never confused) plus a translucent, non-interactive falloff-
    // radius ring — same circleSegs primitive already used for the 3D
    // gizmo's own rotation rings just below.
    if (t.li != null && state.layers[t.li] && state.layers[t.li].duplicator && state.layers[t.li].duplicator.effectors && !t.strokeId) {
      var effCol = [255, 120, 220, 255];
      state.layers[t.li].duplicator.effectors.forEach(function (eff) {
        if (!eff.pos) return;
        var ew = outerWorldPoint(t, { x: eff.pos.x, y: eff.pos.y });
        items.push({ segments: [{ point: [ew.x - 8 * zs, ew.y] }, { point: [ew.x + 8 * zs, ew.y] }], closed: false, fillColor: null, strokeColor: effCol, strokeWidth: 1.5 * zs });
        items.push({ segments: [{ point: [ew.x, ew.y - 8 * zs] }, { point: [ew.x, ew.y + 8 * zs] }], closed: false, fillColor: null, strokeColor: effCol, strokeWidth: 1.5 * zs });
        items.push({ segments: circleSegs(ew.x, ew.y, 5 * zs), closed: true, fillColor: null, strokeColor: effCol, strokeWidth: 1.5 * zs });
        // Falloff ring — world-space radius (not zoom-scaled: it represents
        // a real document-space distance, unlike the handle glyphs above).
        items.push({ segments: circleSegs(ew.x, ew.y, eff.radius || 0), closed: true, fillColor: null, strokeColor: [255, 120, 220, 90], strokeWidth: 1 * zs });
      });
    }
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
    // A Null DOES get this box (2026-08, feedback: "toujours pas de
    // bounding box" — an earlier pass suppressed it on the "AE nulls have
    // no bbox" assumption; the user wants the same visual/selection
    // consistency every other layer type gets here). motionBoxGeom now
    // special-cases a Null's bounds to a fixed screen-constant square
    // around nullPos instead of userLayers[].bounds' degenerate zero-size
    // rect, so this is no longer the phantom-at-origin box it used to be.
    var is3DTargetForBox = t.li != null && state.layers[t.li] && state.layers[t.li].threeD && !t.strokeId;
    var mh = is3DTargetForBox ? null : motionHandlePositions(t);
    if (mh) {
      var boxCol = [74, 158, 255, 204];
      // Reuses mh.corners (already outer-wrapped by motionHandlePositions)
      // instead of recomputing via mh.g.fwd directly — that used to bypass
      // the outer/parent-chain wrapping entirely (2026-08 fix, same root
      // cause as the box-misaligned-after-parenting bug above).
      var bc1 = mh.corners.nw, bc2 = mh.corners.ne, bc3 = mh.corners.se, bc4 = mh.corners.sw;
      [[bc1, bc2], [bc2, bc3], [bc3, bc4], [bc4, bc1]].forEach(function (seg) {
        items.push({ segments: [{ point: [seg[0].x, seg[0].y] }, { point: [seg[1].x, seg[1].y] }], closed: false, fillColor: null, strokeColor: boxCol, strokeWidth: 1 * zs });
      });
      // Corners + edge midpoints (feedback #98) — same square style for
      // both, matching Animation 2D's own buildTransformBoxItems (no visual
      // distinction there either between a corner and an n/s/e/w handle).
      [mh.corners.nw, mh.corners.ne, mh.corners.se, mh.corners.sw, mh.corners.n, mh.corners.s, mh.corners.e, mh.corners.w].forEach(function (p) {
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
      // Through elementVertexPoints, so a vector-brush ribbon shows dots on
      // its CENTERLINE — the same nodes Select/Subselect show, and the same
      // ones the Path rows now key (#181). Reading vItem.segments here put
      // dots on outline points that no vtxN row corresponded to.
      var vPts = elementVertexPoints(vItem);
      if (vPts.length && mh) {
        var vg = mh.g, vertCol = [190, 130, 240, 255];
        vPts.forEach(function (seg, vi) {
          var voff = valueAtFrame(holder, 'vtx' + vi, state.currentFrame);
          var localX = seg.x + (voff[0] || 0), localY = seg.y + (voff[1] || 0);
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
    // NOT `aw` (the crosshair's own point, a few lines up) — aw deliberately
    // includes the CURRENT FRAME's own interpolated Position (2026-08-21 fix,
    // so the crosshair tracks where the object visually renders THIS frame).
    // Reusing it here was a mistake made fixing the ax/ay crash (feedback
    // #36/#40): position is exactly the thing being plotted per-key below,
    // so every key's dot inherited a live, current-frame-dependent shift on
    // top of its own value — dragging one key (which updates the value AT
    // the current frame while the playhead sits on/near it) visibly dragged
    // every OTHER key's dot along with it too, even though only the grabbed
    // key's data actually changed (feedback #41, "l'ensemble des clé bouge
    // quand je bouge la position d'une seule keyframe"). This pivot must
    // stay position-INDEPENDENT — exactly "bounds center + anchor offset"
    // per this block's own original comment above, matching motionBoxGeom's
    // pre-position px/py and onDown's hit-test `pv` a few hundred lines down
    // (which never included position either — only the drawing side did).
    var pvAnc = valueAtFrame(holder, 'anchor', state.currentFrame);
    var pvx = bc.x + pvAnc[0], pvy = bc.y + pvAnc[1];
    var track = holder.motion.position;
    var ks = track.keys;
    for (var i = 0; i < ks.length - 1; i++) {
      var a = ks[i], b = ks[i + 1];
      var ho = a.hOut || [0, 0], hi = b.hIn || [0, 0];
      // Dot density must encode SPEED (2026-08 fix, feedback: "les tirets
      // ne reflètent pas visuellement les keyframes et leur rapprochement
      // en fonction du lissage de keyframe") — the After Effects convention
      // this is copying draws a thin CONTINUOUS path line plus one small dot
      // per frame, packed tightly where the object moves slowly (eased-out
      // near a key) and spread out where it moves fast. A first attempt drew
      // the path itself as alternating dash/gap segments instead of dots —
      // reasonable in theory, but on a real (non-uniform, non-extreme)
      // easing curve it reads as a broken, disconnected scribble rather than
      // a path (feedback: "le motion path ça va pas du tout... il faut un
      // trait bleu fin avec des petits points représentant les keyframes").
      // The engine's native dashPattern (still used elsewhere in this file)
      // dashes at UNIFORM ARC LENGTH along whatever polyline it's given —
      // completely blind to how the sample points feeding that polyline
      // were spaced, so a fixed dashPattern here could never have shown
      // speed regardless of sampling; sampling one point per FRAME through
      // this key's own easing curve (evalCurvePoints — the same evaluator
      // rawValueAtFrame above uses for the real animation, just never reused
      // here before) and dropping a dot at each one does, while the
      // underlying line stays unbroken.
      var nFrames = Math.max(1, b.frame - a.frame);
      var framePts = [];
      for (var f = 0; f <= nFrames; f++) {
        var tt = f / nFrames;
        // Named `bt` (bezier t), NOT `t` — this function's outer `t` is the
        // Motion TARGET object (activeMotionTarget()), which outerWorldPoint
        // needs as its first argument. `var t` here used to shadow it (var
        // is function-scoped, not block-scoped), so outerWorldPoint below —
        // and every call after this loop for the REST of the function,
        // including the position-key diamonds' own outerWorldPoint calls —
        // silently ran with `t` clobbered to a plain 0..1 number instead of
        // the target object once ks.length>=2 (single-keyframe projects
        // never entered this loop, so the bug only showed once a second key
        // existed). This is what "le motion path devrait être sur le point
        // d'ancrage" was actually describing.
        var bt = a.hold ? 0 : evalCurvePoints(a.curvePoints || DEFAULT_CURVE, tt);
        var v = 1 - bt;
        var rawPathPoint={
          x:
            pvx + v * v * v * a.v[0] + 3 * v * v * bt * (a.v[0] + ho[0]) + 3 * v * bt * bt * (b.v[0] + hi[0]) + bt * bt * bt * b.v[0],
          y:pvy + v * v * v * a.v[1] + 3 * v * v * bt * (a.v[1] + ho[1]) + 3 * v * bt * bt * (b.v[1] + hi[1]) + bt * bt * bt * b.v[1]
        };
        var worldPathPoint=outerWorldPoint(t,rawPathPoint);
        framePts.push({ point: worldPathPoint });
      }
      // One unbroken thin line for the whole segment — always reads as a
      // path no matter how the frame points happen to be spaced.
      items.push({ segments: framePts, closed: false, fillColor: null, strokeColor: pathCol, strokeWidth: 1 * zs });
      // Stride caps dot count on very long segments (e.g. a key held for
      // hundreds of frames) — no visual benefit past a point, and it bloats
      // scene JSON rebuilt on every render tick while this overlay is live.
      var maxDots = 120;
      var dotStride = Math.max(1, Math.ceil(framePts.length / maxDots));
      var dotR = 1.3 * zs;
      for (var fi = 0; fi < framePts.length; fi += dotStride) {
        var dp = framePts[fi].point;
        items.push({ segments: circleSegs(dp.x, dp.y, dotR), closed: true, fillColor: pathCol, strokeColor: null, strokeWidth: 0 });
      }
    }
    ks.forEach(function (k, ki) {
      var isCur = k.frame === state.currentFrame;
      var rawKey={x:pvx+k.v[0],y:pvy+k.v[1]},worldKey=outerWorldPoint(t,rawKey);
      var kx=worldKey.x,ky=worldKey.y;
      // Diamond, not a circle (2026-07-29 fix, "plusieurs points s'affiche
      // sur un seul élément on ne sait pas trop quoi prendre") — a key at
      // its default [0,0] delta draws exactly on the anchor crosshair
      // (comment above, motionPivotOf) and the anchor itself is ALSO a
      // circle, so the two used to be indistinguishable blobs stacked on
      // top of each other with only a subtle color/fill difference to go
      // on. A diamond is the near-universal keyframe glyph (AE, Premiere,
      // every NLE's graph editor) and reads as a distinct object sitting
      // on/inside the anchor's circle instead of a second, confusing
      // circle — same diamond shape this file already uses for per-vertex
      // handles a few dozen lines below, just a different color/size.
      // Hit-testing (hitPositionDot) is untouched — it's a pure distance
      // check, never depended on the drawn shape.
      var kr = (isCur ? 6 : 4.5) * zs;
      items.push({ segments: [{ point: [kx - kr, ky] }, { point: [kx, ky - kr] }, { point: [kx + kr, ky] }, { point: [kx, ky + kr] }], closed: true, fillColor: isCur ? [255, 170, 40, 255] : [230, 230, 230, 255], strokeColor: [30, 30, 30, 255], strokeWidth: 1.2 * zs });
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
      // getEffectiveStrokesRendered, not getEffectiveStrokes (2026-07-29 fix,
      // QA-confirmed "le gizmo se décale par rapport au calque" on a
      // Component layer that also has a mograph duplicator): the plain
      // getEffectiveStrokes only returns the SEED content — duplicator
      // expansion happens in getEffectiveStrokesRendered
      // (applyLayerDuplicator, app.js) — so this union used to span just the
      // one un-duplicated shape while the render path (engine-bridge.js
      // reads userLayers[i].bounds directly) already showed every copy,
      // pulling the gizmo/transform-box pivot away from the actual content
      // centroid the instant a Motion key auto-converted the layer to a
      // Component (CLAUDE.md §8).
      getEffectiveStrokesRendered(li, f).forEach(function (sd) {
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
  // The element's bounding box ACCOUNTING for its own vertex-key offsets
  // (2026-08-31, feedback #195: "si les keyframes sont actives dans les
  // vertex de shape et que je bouge les vertex... la bounding box ne
  // s'adapte pas à la nouvelle taille de la shape"). t.bounds (below, in
  // motionBoxGeom) comes straight from the LIVE Paper item's own .bounds —
  // correct for every other kind of Motion (position/rotation/scale all
  // apply at RENDER time on top of untouched raw geometry, CLAUDE.md's
  // space-discipline invariant), but per-vertex offsets are exactly the one
  // property that changes the shape's own EXTENT, not just where it sits —
  // t.bounds never moves, so the box stops matching the shape the instant a
  // vertex is dragged past its own original corner. Confirmed live: a plain
  // rectangle with one vertex keyed +100,+100 rendered as a lopsided
  // triangle-ish shape reaching well past the box's own bottom edge.
  //
  // Reuses applyPathVertexOffsets — the SAME function buildSceneJson/
  // export.js already call to get the RENDERED geometry — so this can never
  // drift from what's actually drawn. A temp Paper Path (never inserted,
  // removed immediately) gets the bounds instead of hand-rolling bezier-
  // curve bounds math, which per-vertex handles would otherwise need.
  // Gated on hasPathVertexMotion so the overwhelmingly common case (no
  // vertex keys at all) costs nothing beyond that one cheap check — same
  // opt-in-fast-path shape every other reader of this function already uses.
  function vertexPosedElementBounds(t) {
    if (!t || !t.strokeId || !t.holder || !hasPathVertexMotion(t.holder)) return null;
    var item = findElementItem(t.li, t.strokeId);
    if (!item || !item.segments || !item.segments.length) return null;
    var rawSegs = item.segments.map(function (s) {
      return { point: [s.point.x, s.point.y], handleIn: [s.handleIn.x, s.handleIn.y], handleOut: [s.handleOut.x, s.handleOut.y] };
    });
    var offsetSegs;
    if (item.data && item.data.isVectorBrush) {
      // A vector brush's vertices are its CENTERLINE, not its baked outline
      // (#181's own distinction, isVectorBrushSd) — offsetting the outline
      // segments directly would be editing the wrong geometry. Rebuild the
      // real ribbon outline through the same path applyVectorBrushOutlineFor
      // already uses for rendering, keyed off the stored sd rather than the
      // live item (its centerSegments/widthProfile are the static recording,
      // exactly what that function expects).
      var ld2 = state.layers[t.li];
      var entry = ld2 && window.layerElements && layerElements(t.li, ld2).filter(function (e) { return e.strokeId === t.strokeId; })[0];
      offsetSegs = entry ? applyVectorBrushOutlineFor(t.li, t.strokeId, entry.sd, state.currentFrame) : null;
      if (!offsetSegs) return null;
    } else {
      offsetSegs = applyPathVertexOffsets(rawSegs, t.holder, state.currentFrame);
    }
    var tmp = new Path({ insert: false });
    offsetSegs.forEach(function (s) {
      tmp.add(new Segment(new Point(s.point[0], s.point[1]), new Point(s.handleIn[0], s.handleIn[1]), new Point(s.handleOut[0], s.handleOut[1])));
    });
    tmp.closed = !!item.closed;
    var b = tmp.bounds;
    tmp.remove();
    if (!b || !isFinite(b.width) || !isFinite(b.height)) return null;
    return { left: b.left, top: b.top, right: b.right, bottom: b.bottom, width: b.width, height: b.height, center: { x: b.center.x, y: b.center.y } };
  }
  function motionBoxGeom(t) {
    var ld = state.layers[t.li];
    var lb;
    if (ld && ld.isNullLayer) {
      // A Null has no Paper.js bounds — fixed screen-constant square
      // matching buildNullLayerItems' own marker size (engine-bridge.js,
      // HS = 12/view.zoom), centered on t.boundsCenter (nullPos, see
      // activeMotionTarget's isNullLayer branch) so re-enabling the box
      // here (2026-08, feedback: "toujours pas de bounding box") doesn't
      // resurrect the old degenerate-at-(0,0) bug the anchor gizmo had.
      var hs = 12 / Math.max(0.0001, view.zoom);
      var nb = t.boundsCenter;
      lb = { left: nb.x - hs, top: nb.y - hs, right: nb.x + hs, bottom: nb.y + hs, width: hs * 2, height: hs * 2, center: { x: nb.x, y: nb.y } };
    } else {
      // Per-ELEMENT target hugs its own shape (2026-08-30, feedback #170
      // re-opened: "ça m'ouvre pas ça comme un groupe avec les 2 bounding
      // box différencié"). activeMotionTarget has computed t.bounds for the
      // expanded element since the per-element branch was added — this
      // function simply never read it and always took the whole LAYER's
      // union, so on a layer with two shapes the gizmo drew ONE box around
      // both while its anchor correctly sat on the element. Seen on screen,
      // not deduced: the box spanned a square at (700,420) and a circle at
      // (1030,780) together.
      // Only when a strokeId is actually targeted; the whole-layer case
      // (t.strokeId null) keeps the union it always used, and a Component
      // keeps symbolUnionBounds' duration-stable box.
      // The whole-LAYER box is built from the elements' POSED bounds, not
      // the layer's raw ones (2026-08-31): moving two elements apart with
      // their own Motion left this box at its old size while they visibly
      // spread outside it. perObjectPosedUnionLocal returns the union after
      // each element's own transform but BEFORE the layer's, which is
      // exactly this function's input space — it applies the layer transform
      // itself just below. Falls back to the raw bounds when the layer has
      // no usable shapes (an empty layer, a Null, a Guide).
      var posedLb = (!t.strokeId && !(ld && ld.symbolId) && window.perObjectPosedUnionLocal)
        ? perObjectPosedUnionLocal(userLayers[t.li]) : null;
      lb = (t.strokeId && t.bounds) ? (vertexPosedElementBounds(t) || t.bounds)
        : (posedLb || ((ld && ld.symbolId) ? symbolUnionBounds(t.li) : (userLayers[t.li] && userLayers[t.li].bounds)));
    }
    if (!lb) return null;
    var anc = valueAtFrame(t.holder, 'anchor', state.currentFrame);
    var pos = valueAtFrame(t.holder, 'position', state.currentFrame);
    var rot = valueAtFrame(t.holder, 'rotation', state.currentFrame)[0];
    var scl = valueAtFrame(t.holder, 'scale', state.currentFrame);
    var px = t.boundsCenter.x + anc[0], py = t.boundsCenter.y + anc[1];
    var r = rot * Math.PI / 180, c = Math.cos(r), s = Math.sin(r);
    var sx = scl[0] / 100, sy = scl[1] / 100;
    // LOCAL only — this target's own position/rotation/scale, no outer
    // (parent chain / containing-layer) wrapping. Every caller composes
    // that separately via outerWorldPoint/outerLocalPoint (2026-08 fix,
    // see outerMotionMaps' own comment for why the wrapping lives THERE,
    // once, rather than being duplicated into this function too).
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
    // Outer (parent chain / containing-layer) wrapping applied HERE, once
    // — motionBoxGeom's fwd is local-only (2026-08 fix, see
    // outerMotionMaps' own comment). Corners used to be plain g.fwd(...)
    // with no outer wrapping at all, so a parented layer's box never
    // agreed with where it actually renders.
    // Edge midpoints (feedback #98, "scale en width ou height séparément...
    // en Animation 2D mais pas en motion") added alongside the 4 corners —
    // this dict's own keys are already what hitMotionBoxHandle iterates
    // generically (Object.keys) and what multiLayerBox unions via min/max,
    // so both stay correct with no further change; only onDown/onDrag's
    // motionScale branch needs to tell a single-letter dir (n/s/e/w) apart
    // from a two-letter corner to know which axis/axes to scale.
    var midX = (b.left + b.right) / 2, midY = (b.top + b.bottom) / 2;
    var corners = {
      nw: outerWorldPoint(t, g.fwd(b.left, b.top)), ne: outerWorldPoint(t, g.fwd(b.right, b.top)),
      se: outerWorldPoint(t, g.fwd(b.right, b.bottom)), sw: outerWorldPoint(t, g.fwd(b.left, b.bottom)),
      n: outerWorldPoint(t, g.fwd(midX, b.top)), s: outerWorldPoint(t, g.fwd(midX, b.bottom)),
      e: outerWorldPoint(t, g.fwd(b.right, midY)), w: outerWorldPoint(t, g.fwd(b.left, midY)),
    };
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
    return { g: g, corners: corners, ringCenter: outerWorldPoint(t, g.pivot), ringRadius: ringRadius };
  }
  // Whole-layer multi-selection box (feedback #54). `_layerSel` already is
  // the source of truth for Cmd/Shift-selected rows; derive one world-space
  // union from every layer's rendered Motion box so canvas and timeline
  // describe the same selection.
  function multiLayerBox() {
    if (state.appMode !== 'motion' || typeof _layerSel === 'undefined' || _layerSel.length < 2) return null;
    var targets = [], minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    _layerSel.forEach(function (li) {
      var ld = state.layers[li], ul = userLayers[li];
      if (!ld || !ul || ld.locked || !ld.visible || ld.threeD) return;
      var ub = ld.symbolId ? symbolUnionBounds(li) : null;
      var lb = ub || ul.bounds;
      if (!lb || !isFinite(lb.width) || !isFinite(lb.height)) return;
      var t = { li: li, strokeId: null, holder: ld, boundsCenter: lb.center, bounds: lb };
      var h = motionHandlePositions(t); if (!h) return;
      var cs = h.corners;
      Object.keys(cs).forEach(function (k) {
        minX = Math.min(minX, cs[k].x); minY = Math.min(minY, cs[k].y);
        maxX = Math.max(maxX, cs[k].x); maxY = Math.max(maxY, cs[k].y);
      });
      targets.push({ t: t, center: { x: (cs.nw.x + cs.ne.x + cs.se.x + cs.sw.x) / 4, y: (cs.nw.y + cs.ne.y + cs.se.y + cs.sw.y) / 4 } });
    });
    if (targets.length < 2 || minX === Infinity) return null;
    var bounds = { left: minX, top: minY, right: maxX, bottom: maxY, width: maxX - minX, height: maxY - minY };
    bounds.center = { x: (minX + maxX) / 2, y: (minY + maxY) / 2 };
    var zs = 1 / Math.max(0.0001, view.zoom);
    return { targets: targets, bounds: bounds, pivot: bounds.center, ringRadius: Math.min(36 * zs, Math.max(bounds.width, bounds.height) * 0.3) };
  }
  function multiLayerOverlay(m) {
    var b = m.bounds, zs = 1 / Math.max(0.0001, view.zoom), col = [74, 158, 255, 220], items = [];
    var corners = [{ x: b.left, y: b.top }, { x: b.right, y: b.top }, { x: b.right, y: b.bottom }, { x: b.left, y: b.bottom }];
    for (var i = 0; i < 4; i++) {
      var a = corners[i], n = corners[(i + 1) % 4];
      items.push({ segments: [{ point: [a.x, a.y] }, { point: [n.x, n.y] }], closed: false, fillColor: null, strokeColor: col, strokeWidth: 1.2 * zs });
      var hs = 3.8 * zs;
      items.push({ segments: [{ point: [a.x - hs, a.y - hs] }, { point: [a.x + hs, a.y - hs] }, { point: [a.x + hs, a.y + hs] }, { point: [a.x - hs, a.y + hs] }], closed: true, fillColor: [255, 255, 255, 255], strokeColor: col, strokeWidth: 1.2 * zs });
    }
    items.push({ segments: circleSegs(m.pivot.x, m.pivot.y, m.ringRadius), closed: true, fillColor: null, strokeColor: [74, 158, 255, 170], strokeWidth: 1.1 * zs });
    return items;
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
  // Only grabs a dot for the CURRENTLY SCRUBBED frame (2026-08-21, "je
  // bouge le rectangle toutes les keyframes bougent") — every other key's
  // dot sits wherever the render happens to place it at ITS OWN frame,
  // which with a single keyframe (or several coincident on screen) is
  // exactly where the shape is drawn RIGHT NOW regardless of playhead
  // position. Without this guard, a plain "move the shape" drag at any
  // frame always re-grabbed and mutated whichever existing key's dot
  // visually overlapped the click — so a second keyframe could never be
  // created by dragging on canvas, only the first one ever got edited.
  // Restricting to the current frame's own key means: parked ON a
  // keyframe, dragging the shape edits THAT keyframe (unchanged, correct
  // AE behavior); parked anywhere else, this returns null and onDown
  // falls through to the plain-move path below, which correctly creates
  // a NEW key at state.currentFrame (setValue/setKeyAtCurrentFrame).
  function hitPositionDot(pt, ks, pv, target) {
    var tol = 8 / view.zoom;
    for (var i = 0; i < ks.length; i++){
      if (ks[i].frame !== state.currentFrame) continue;
      var wp=outerWorldPoint(target,{x:pv.x+ks[i].v[0],y:pv.y+ks[i].v[1]});if(Math.hypot(pt.x-wp.x,pt.y-wp.y)<tol)return ks[i];
    }
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
  // Set by select-bridge.js's empty-canvas click (2026-08 fix, feedback:
  // "impossible de tout déselect dans le canvas en cliquant sur une zone
  // où y a rien") — clearSel()/syncMotionLayerSelection(null,...) already
  // ran there, correctly emptying selectedPaths/_layerSel, but
  // state.activeLayerIdx itself was never cleared (no "-1"/"nothing"
  // convention exists for it anywhere in this codebase, and introducing
  // one would touch every one of its ~27 read/write sites app-wide) — so
  // this box/gizmo and the Properties panel below kept showing whatever
  // was active before the click, unchanged. This flag is a narrow,
  // Motion-canvas-only override instead: true only right after an empty
  // click, cleared by every genuine re-selection (a layer-list row via
  // setActiveLayer, or a canvas hit on a Null/shape/component in
  // select-bridge.js) so it never gets "stuck" hiding a real selection.
  var _motionCanvasEmptyClick = false;
  function setMotionCanvasEmptyClick(v) { _motionCanvasEmptyClick = !!v; }
  // 2026-08 follow-up fix: the flag above, checked alone, could drift out
  // of sync with the layer LIST's own highlight — found live (feedback:
  // "quand on deselect tout le calque layer 1 n'est pas deselect au
  // niveau timeline") — the row's ".act.motion-selected" white-outline
  // CSS (style.css ~line 1170) is driven entirely by `_layerSel`, a
  // SEPARATE, older, far-more-thoroughly-wired selection tracker (every
  // row click, null/shape/component canvas hit already keeps it correct
  // via syncMotionLayerSelection) — this bespoke flag only got threaded
  // through a handful of call sites and could miss one. `_layerSel.length
  // === 0` is now the PRIMARY signal (matching the row highlight exactly,
  // so the two can never show different things); the flag stays as an
  // extra OR-condition, not the sole source of truth anymore.
  function activeMotionTarget() {
    if (state.appMode !== 'motion') return null;
    if (_motionCanvasEmptyClick) return null;
    if (window._motionExpandedLayer == null && typeof _layerSel !== 'undefined' && _layerSel.length === 0) return null;
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
      if (item) return { li: li, strokeId: window._motionExpandedElement, holder: ensureElementHolder(ld, window._motionExpandedElement), boundsCenter: item.bounds.center, bounds: item.bounds };
      // Element no longer present at this frame (drawing changed) — fall
      // back to the layer rather than silently drawing nothing.
    }
    // Component layers use symbolUnionBounds' fixed-for-the-whole-duration
    // center here too, so the gizmo's PIVOT doesn't jump around alongside
    // its box (motionBoxGeom, above) as the scrub crosses different
    // keyframes.
    // A Null has a real (permanently empty) Paper.js Layer in userLayers —
    // its own .bounds getter on an empty Layer returns a degenerate
    // Rectangle centered at (0,0), NOT null/undefined, so this used to
    // silently fall through to the branch below and pivot the anchor
    // crosshair/box near the canvas origin instead of the Null's actual
    // nullPos (2026-08 fix, feedback: "le point d'ancrage est décalé de
    // la forme du null" — same bounds-based-logic-breaks-for-a-content-
    // less-Null root cause as legacyParentChainMats' isNullLayer branch
    // just added above). Mirrors buildNullLayerItems' own basePos.
    if (ld.isNullLayer) {
      var nBase = ld.nullPos || [state.canvasW / 2, state.canvasH / 2];
      var nRect = { left: nBase[0], top: nBase[1], right: nBase[0], bottom: nBase[1], width: 0, height: 0, center: { x: nBase[0], y: nBase[1] } };
      return { li: li, strokeId: null, holder: ld, boundsCenter: nRect.center, bounds: nRect };
    }
    var ub = ld.symbolId ? symbolUnionBounds(li) : null;
    var lb = ub || userLayers[li].bounds;
    return { li: li, strokeId: null, holder: ld, boundsCenter: lb.center, bounds: lb };
  }
  function activePositionKeys() {
    var t = activeMotionTarget();
    if (!t || !hasKeys(t.holder, 'position')) return null;
    return t.holder.motion.position.keys;
  }
  function hitAnchorPoint(pt, t) {
    var tol = 9 / view.zoom;
    // Mirrors buildOverlayItems' anchor crosshair exactly (2026-08-21 fix)
    // — must use the SAME point the marker is actually drawn at (own
    // position/rotation/scale included via motionBoxGeom's pivot), or a
    // click precisely on the visible marker misses the moment Position is
    // non-[0,0], falling through to a plain body-drag instead.
    var g0 = motionBoxGeom(t);
    var anc = valueAtFrame(t.holder, 'anchor', state.currentFrame);
    var aw = g0 ? outerWorldPoint(t, g0.pivot) : outerWorldPoint(t, { x: t.boundsCenter.x + anc[0], y: t.boundsCenter.y + anc[1] });
    var ax=aw.x,ay=aw.y;
    return Math.hypot(pt.x - ax, pt.y - ay) < tol ? { holder: t.holder, bc: t.boundsCenter, target:t } : null;
  }
  // Mograph effector handles (2026-07-29) — layer-target only (not
  // per-element), only when this layer has a duplicator with 1+ effectors.
  // Iterated back-to-front so the LAST-added (topmost-drawn) effector wins
  // an overlapping click, matching normal z-order picking conventions.
  function hitEffectorHandle(pt, t) {
    var dup = t.li != null && state.layers[t.li] && state.layers[t.li].duplicator;
    var effs = dup && dup.effectors;
    if (!effs || !effs.length) return null;
    var tol = 8 / view.zoom;
    for (var i = effs.length - 1; i >= 0; i--) {
      var eff = effs[i]; if (!eff.pos) continue;
      var wp = outerWorldPoint(t, { x: eff.pos.x, y: eff.pos.y });
      if (Math.hypot(pt.x - wp.x, pt.y - wp.y) < tol) return eff;
    }
    return null;
  }
  function onDown(event) {
    // Inside a group with nothing picked yet, a click that lands ON a shape
    // belongs to that shape (2026-08-31). Without this the whole-LAYER gizmo
    // claimed it first — its ring, corners and body all sit over the very
    // shapes you are trying to pick, and the more the layer is rotated or
    // scaled the more of them fall under the cursor. Measured with the layer
    // at rotation 25°: clicking a shape inside the group targeted nothing
    // (_motionExpandedElement stayed empty) and the following drag moved the
    // whole layer instead, which reads exactly as "I can select an object
    // but not drag it".
    //
    // Deliberately narrow: only while per-object mode is on for THIS layer,
    // only while no element is targeted yet, and only when the point is
    // really on a shape. Every other Motion gesture — including the layer
    // gizmo outside a group, and every grab once an element IS targeted —
    // is untouched. Returning false hands the click to select-bridge, whose
    // Motion block does the targeting.
    if (window._perObjBoxes === state.activeLayerIdx && window._motionExpandedElement == null
        && window.hitTestPosed && userLayers[state.activeLayerIdx]) {
      var surForme = hitTestPosed(state.activeLayerIdx, event.point, 6 / Math.max(0.0001, view.zoom));
      if (surForme && surForme.item && surForme.item.data && surForme.item.data.strokeId) return false;
    }
    var ml = multiLayerBox();
    if (ml) {
      var mb = ml.bounds, mz = 1 / Math.max(0.0001, view.zoom);
      var dRing = Math.abs(Math.hypot(event.point.x - ml.pivot.x, event.point.y - ml.pivot.y) - ml.ringRadius);
      var hitCorner = false;
      [{x:mb.left,y:mb.top},{x:mb.right,y:mb.top},{x:mb.right,y:mb.bottom},{x:mb.left,y:mb.bottom}].forEach(function(p){
        if(Math.hypot(event.point.x-p.x,event.point.y-p.y)<9*mz)hitCorner=true;
      });
      var inside = event.point.x >= mb.left && event.point.x <= mb.right && event.point.y >= mb.top && event.point.y <= mb.bottom;
      if (dRing < 7 * mz || hitCorner || inside) {
        pushUndo();
        var records = ml.targets.map(function (rec) {
          return { t: rec.t, center: rec.center, pos: valueAtFrame(rec.t.holder, 'position', state.currentFrame).slice(), scale: valueAtFrame(rec.t.holder, 'scale', state.currentFrame).slice(), rot: valueAtFrame(rec.t.holder, 'rotation', state.currentFrame)[0] };
        });
        if (dRing < 7 * mz) {
          _motionDrag = { mode: 'multiLayerRotate', pivot: ml.pivot, startAngle: Math.atan2(event.point.y-ml.pivot.y,event.point.x-ml.pivot.x)*180/Math.PI, records: records };
        } else if (hitCorner) {
          _motionDrag = { mode: 'multiLayerScale', pivot: ml.pivot, origDist: Math.max(1e-6,Math.hypot(event.point.x-ml.pivot.x,event.point.y-ml.pivot.y)), records: records };
        } else {
          _motionDrag = { mode: 'multiLayerMove', start: {x:event.point.x,y:event.point.y}, records: records };
        }
        return true;
      }
      return false;
    }
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
      var vPts2 = elementVertexPoints(vItem2);
      if (vPts2.length && vg2) {
        var vTol = 9 / view.zoom;
        for (var vi2 = 0; vi2 < vPts2.length; vi2++) {
          var seg2 = vPts2[vi2];
          var voff2 = valueAtFrame(t.holder, 'vtx' + vi2, state.currentFrame);
          var wp2 = vg2.fwd(seg2.x + (voff2[0] || 0), seg2.y + (voff2[1] || 0));
          if (Math.hypot(event.point.x - wp2.x, event.point.y - wp2.y) < vTol) {
            pushUndo();
            _motionDrag = { mode: 'vertex', t: t, vi: vi2, basePt: { x: seg2.x, y: seg2.y } };
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
      // Also skipped for a Null layer (feedback #59, "un petit bounding box
      // que l'on peu déplacer" never actually moved on drag): motionBoxGeom
      // gives a Null a fixed tiny 24px-equivalent box (hs=12/zoom) so
      // ringRadius (30% of that) collapses to ~7.2px — right on top of
      // hitMotionBoxHandle's own ±7px ring tolerance. The tolerance band
      // then swallows the ENTIRE clickable marker, so every click matched
      // 'rotate' and the correctly-working move handler in select-bridge.js
      // (mode:'null-drag', a few lines below this file's own onDown return)
      // never got a chance to run — confirmed live, dragging always rotated,
      // position never budged. A Null has no real use for a canvas
      // rotate/scale drag anyway (both properties stay reachable from the
      // panel) — skip the box gizmo outright so a plain click always falls
      // through to the dedicated move handler instead of chasing a
      // per-layer-type ring-radius tune.
      var isNullTarget = t.li != null && state.layers[t.li] && state.layers[t.li].isNullLayer;
      var boxHit = (isNullTarget || (t.li != null && state.layers[t.li] && state.layers[t.li].threeD && !t.strokeId)) ? null : hitMotionBoxHandle(event.point, t);
      if (boxHit) {
        pushUndo();
        var g = motionBoxGeom(t);
        if (boxHit.type === 'rotate') {
          var startAngle = Math.atan2(event.point.y - g.pivot.y, event.point.x - g.pivot.x) * 180 / Math.PI;
          _motionDrag = { mode: 'motionRotate', t: t, pivot: g.pivot, startAngle: startAngle, origRot: g.rot };
        } else {
          var corner = motionHandlePositions(t).corners[boxHit.dir];
          var origDist = Math.hypot(corner.x - g.pivot.x, corner.y - g.pivot.y) || 1;
          // Single-axis edge handle (feedback #98) — the handle's own
          // world-space direction from the pivot (already rotation-correct,
          // since it's the ACTUAL rendered position, same box the corner
          // branch already trusts) becomes the axis to project the drag
          // onto, so only n/s scales Y and only e/w scales X. Two-letter
          // corners keep the untouched uniform-ratio path below.
          var axisDir = null;
          if (boxHit.dir === 'n' || boxHit.dir === 's' || boxHit.dir === 'e' || boxHit.dir === 'w') {
            axisDir = { ux: (corner.x - g.pivot.x) / origDist, uy: (corner.y - g.pivot.y) / origDist };
          }
          _motionDrag = { mode: 'motionScale', t: t, pivot: g.pivot, dir: boxHit.dir, axisDir: axisDir, origDist: origDist, origScale: g.scl.slice() };
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
      // Alt required (2026-08-21, "pour bouger le point d'ancrage c'est
      // clic + alt + drag il me semble pas le cas là") — matches Animation
      // 2D's own anchor-crosshair convention (select-bridge.js's
      // hitTestHandles: "checked FIRST/exclusively, but ONLY while Alt is
      // held... a click in that same spot now falls through to the normal
      // move/marquee logic below" when Alt isn't held). Motion mode never
      // had that gate — a plain click within the small hit radius grabbed
      // the anchor unconditionally, which is what "il bouge encore" (the
      // artwork/box moving when the user only meant to click-drag
      // normally) was really describing: an accidental anchor grab, not
      // the anchor itself misbehaving.
      var ap = event.altKey ? hitAnchorPoint(event.point, t) : null;
      if (ap) { pushUndo(); _motionDrag = { mode: 'anchor', holder: ap.holder, bc: ap.bc, t:ap.target }; return true; }
      // Effector handles (2026-07-29) — checked last, lowest priority: they
      // only exist on duplicator layers and the user places them wherever
      // they like, so overlap with the box/position/anchor controls above
      // is rare, but those established grabs should still win if it happens.
      var effHit = hitEffectorHandle(event.point, t);
      if (effHit) { pushUndo(); _motionDrag = { mode: 'effector', t: t, eff: effHit }; return true; }
      // Dragging the BODY of a per-element box moves that element
      // (2026-08-30, feedback #170 follow-up: "si je bouge la box de
      // l'ellement aprés double clic ça bouge l'ensemble et pas les
      // propriété de la shape en question").
      //
      // onDown had no body-move mode at all — only ring/corners/anchor/
      // handles/vertices/effectors. For a whole-LAYER target that is
      // correct and deliberate: returning false lets select-bridge.js take
      // the gesture and move the layer, which is what its box means. But a
      // box that now hugs ONE element still fell through to that same
      // layer move, so the visible box and the thing that moved were
      // different objects. Measured before the fix: drag the element box by
      // (200,80) and ld.motionStatic.position became [200,80] while the
      // element holder stayed null.
      //
      // Gated on t.strokeId so the whole-layer path is untouched, and
      // placed LAST so every more specific grab above still wins.
      if (t.strokeId) {
        var gBody = motionBoxGeom(t);
        if (gBody && gBody.bounds && gBody.inv) {
          // Test in the box's OWN local space, not as a world-space AABB:
          // gBody.bounds is un-transformed geometry and the drawn box can be
          // rotated/scaled, so comparing a world point against it directly
          // would hit-test a rectangle that isn't the one on screen. inv()
          // exists for exactly this (it was added for vertex dragging).
          //
          // event.point must go through outerLocalPoint FIRST (2026-08-31
          // fix) — motionBoxGeom's own inv is explicitly LOCAL-only (see its
          // comment: "no outer wrapping... every caller composes that
          // separately via outerWorldPoint/outerLocalPoint"), but this is
          // the one caller in this file that fed it the raw world point
          // directly. Every sibling grab just above (handles, position
          // dots, anchor, effector) already wraps through outerWorldPoint/
          // outerLocalPoint; this one, added later for feedback #170, never
          // got the same treatment. Invisible as long as the CONTAINING
          // layer had no Motion of its own — the two points coincide then —
          // which is why it went unnoticed until Cyril moved a whole group
          // and then tried to drag one of its elements: the box still drew
          // in the right (rotated) place, but a click dead-center on it
          // computed `lp` as if the layer had never moved, missing the
          // element's own local bounds entirely. Measured: layer rotated
          // 76.8°, click at the element's true rendered center — old code
          // path declined every time; onDown now grabs it.
          var outerPt = outerLocalPoint(t, { x: event.point.x, y: event.point.y });
          var lp = gBody.inv(outerPt.x, outerPt.y);
          var bb = gBody.bounds;
          var insideEl = lp && lp.x >= bb.left && lp.x <= bb.right && lp.y >= bb.top && lp.y <= bb.bottom;
          if (insideEl) {
            pushUndo();
            _motionDrag = {
              mode: 'elementMove', t: t,
              start: { x: event.point.x, y: event.point.y },
              basePos: valueAtFrame(t.holder, 'position', state.currentFrame).slice()
            };
            return true;
          }
        }
      }
    }
    return false;
  }
  // Local accessor for the layer point map — layerMotionPointMap is defined
  // on the exported SMMotion object at the bottom of this file, not as a
  // closure function, so callers inside the closure go through window.
  function layerMotionPointMapFor(li) {
    return (window.SMMotion && window.SMMotion.layerMotionPointMap) ? window.SMMotion.layerMotionPointMap(li) : null;
  }
  function onDrag(event) {
    if (!_motionDrag) return false;
    if (_motionDrag.mode === 'elementMove') {
      // Writes to the ELEMENT's own holder, which is what its box stands
      // for — same setValue every other Motion drag uses, so it keys at the
      // playhead when the stopwatch is on and writes motionStatic when it
      // isn't, with no second writer.
      // The pointer delta is WORLD space; an element's Position is applied
      // to its geometry BEFORE the layer's own transform, so it has to be
      // pulled back through that transform first (2026-08-31). Without it
      // the delta got rotated a second time at render: measured with the
      // layer at 25°, a drag of (200,140) moved the element (122,211) —
      // exactly (200,140) rotated by 25°.
      //
      // Note the asymmetry with the LAYER's own Position a few files over
      // (select-bridge): that one deliberately does NOT invert, because a
      // layer's dx/dy is a plain translation applied on top of its own
      // rotation, i.e. it already lives in post-rotation space. An
      // ELEMENT's does not — it sits underneath the layer transform.
      var dxEl = event.point.x - _motionDrag.start.x;
      var dyEl = event.point.y - _motionDrag.start.y;
      var mapEl = layerMotionPointMapFor(_motionDrag.t.li);
      if (mapEl && mapEl.invVec) { var vEl = mapEl.invVec(dxEl, dyEl); dxEl = vEl[0]; dyEl = vEl[1]; }
      setValue(_motionDrag.t.holder, 'position', [
        _motionDrag.basePos[0] + dxEl,
        _motionDrag.basePos[1] + dyEl
      ]);
      // The DOCUMENT changed, so say so (2026-08-30, "n'est pas en temps
      // reel, il bouge pas sur le canvas pendant le drag que au
      // relachement"). engine-bridge caches the serialized scene base
      // during a drag keyed on _sceneVersion (CLAUDE.md §5.3) — the whole
      // point being that an intercepted drag leaves the document untouched.
      // This one does NOT: it moves an element. Without the bump the cached
      // base was replayed every tick and the shape only jumped when the
      // drop invalidated it. Same hazard §5.3 already flags for the eraser
      // ("la gomme mutte SANS bump de version").
      window._sceneVersion = (window._sceneVersion || 0) + 1;
    } else if (_motionDrag.mode === 'multiLayerMove') {
      var mdx=event.point.x-_motionDrag.start.x,mdy=event.point.y-_motionDrag.start.y;
      _motionDrag.records.forEach(function(r){setValue(r.t.holder,'position',[r.pos[0]+mdx,r.pos[1]+mdy]);});
    } else if (_motionDrag.mode === 'multiLayerScale') {
      var mDist=Math.hypot(event.point.x-_motionDrag.pivot.x,event.point.y-_motionDrag.pivot.y);
      var mRatio=Math.max(0.01,mDist/_motionDrag.origDist);
      _motionDrag.records.forEach(function(r){
        setValue(r.t.holder,'scale',[r.scale[0]*mRatio,r.scale[1]*mRatio]);
        var h=motionHandlePositions(r.t);if(!h)return;
        var cs=h.corners,cur={x:(cs.nw.x+cs.ne.x+cs.se.x+cs.sw.x)/4,y:(cs.nw.y+cs.ne.y+cs.se.y+cs.sw.y)/4};
        var desired={x:_motionDrag.pivot.x+(r.center.x-_motionDrag.pivot.x)*mRatio,y:_motionDrag.pivot.y+(r.center.y-_motionDrag.pivot.y)*mRatio};
        var p=valueAtFrame(r.t.holder,'position',state.currentFrame);
        setValue(r.t.holder,'position',[p[0]+desired.x-cur.x,p[1]+desired.y-cur.y]);
      });
    } else if (_motionDrag.mode === 'multiLayerRotate') {
      var ma=Math.atan2(event.point.y-_motionDrag.pivot.y,event.point.x-_motionDrag.pivot.x)*180/Math.PI;
      var mDeg=ma-_motionDrag.startAngle,mRad=mDeg*Math.PI/180,mc=Math.cos(mRad),ms=Math.sin(mRad);
      _motionDrag.records.forEach(function(r){
        setValue(r.t.holder,'rotation',[r.rot+mDeg]);
        var h=motionHandlePositions(r.t);if(!h)return;
        var cs=h.corners,cur={x:(cs.nw.x+cs.ne.x+cs.se.x+cs.sw.x)/4,y:(cs.nw.y+cs.ne.y+cs.se.y+cs.sw.y)/4};
        var dx=r.center.x-_motionDrag.pivot.x,dy=r.center.y-_motionDrag.pivot.y;
        var desired={x:_motionDrag.pivot.x+dx*mc-dy*ms,y:_motionDrag.pivot.y+dx*ms+dy*mc};
        var p=valueAtFrame(r.t.holder,'position',state.currentFrame);
        setValue(r.t.holder,'position',[p[0]+desired.x-cur.x,p[1]+desired.y-cur.y]);
      });
    } else if (_motionDrag.mode === 'vertex') {
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
      // feedback #211 — this mode returns early, bypassing the shared tail
      // below that normally does this refresh for every other drag mode.
      liveRefreshVisiblePropertyFields();
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
      return true;
    }
    if (_motionDrag.mode === 'unified') {
      var dx = event.point.x - _motionDrag.last.x, dy = event.point.y - _motionDrag.last.y;
      _motionDrag.last = { x: event.point.x, y: event.point.y };
      var uf = _motionDrag.frame;
      _motionDrag.u.targets.forEach(function (t) {
        var track = t.holder.motion.position;
        var k = keyAt(track, uf);
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
      // Strip the outer/parent chain first (as before), THEN this target's
      // OWN position/rotation/scale (motionBoxGeom's inv — the exact
      // inverse of the fwd() the crosshair is now drawn through, see
      // buildOverlayItems/hitAnchorPoint's matching 2026-08-21 fix) —
      // without this second step the anchor silently absorbed the
      // target's own Position into itself the instant Position was
      // non-[0,0], dragging the anchor to the wrong spot the moment the
      // gesture started from the (now correctly Position-shifted)
      // crosshair.
      var outerLocal=outerLocalPoint(_motionDrag.t,{x:event.point.x,y:event.point.y});
      var ganc=motionBoxGeom(_motionDrag.t);
      var localAnchor=ganc?ganc.inv(outerLocal.x,outerLocal.y):outerLocal;
      var newAncX=localAnchor.x-_motionDrag.bc.x, newAncY=localAnchor.y-_motionDrag.bc.y;
      // Position compensation (2026-08-21 fix, "si je le rotationne avant
      // alors tout le bounding box bougent en même temps si je le
      // déplace"): with rotation/scale active, moving the pivot (anchor)
      // ALSO moves every OTHER geometry point relative to it — fwd()
      // rotates/scales around (px,py)=bc+anchor, so changing anchor alone
      // re-centers that rotation on a different point and the whole shape
      // visibly swings/shifts, even though geometry itself never changed.
      // Only true at rot=0/scale=100% (where fwd(x,y) algebraically
      // cancels anchor out entirely — confirmed live pre-fix, dragging the
      // anchor left the artwork untouched) is that a non-issue. AE's own
      // anchor-drag tool never moves the artwork regardless of rotation —
      // it does this by adjusting Position to compensate, and this is that
      // same compensation: re-deriving Position so that fwd(anyPoint)
      // stays IDENTICAL before/after the anchor change. Derivation: with
      // fwd(P) = pivot + M·(P-pivot) + pos (M = rotate∘scale, pivot =
      // bc+anchor), requiring fwd_new(P) == fwd_old(P) for every P gives
      // pos_new = pos_old + (I-M)·(pivot_old - pivot_new).
      var oldAnc=valueAtFrame(_motionDrag.holder,'anchor',state.currentFrame);
      var rotC=valueAtFrame(_motionDrag.holder,'rotation',state.currentFrame)[0];
      var sclC=valueAtFrame(_motionDrag.holder,'scale',state.currentFrame);
      var rrC=rotC*Math.PI/180, ccC=Math.cos(rrC), ssC=Math.sin(rrC);
      var sxC=sclC[0]/100, syC=sclC[1]/100;
      var dxC=oldAnc[0]-newAncX, dyC=oldAnc[1]-newAncY; // pivot_old - pivot_new (bc cancels)
      var MxC=sxC*ccC*dxC-syC*ssC*dyC, MyC=sxC*ssC*dxC+syC*ccC*dyC; // M·d
      var posC=valueAtFrame(_motionDrag.holder,'position',state.currentFrame);
      setValue(_motionDrag.holder,'position',[posC[0]+(dxC-MxC), posC[1]+(dyC-MyC)]);
      setValue(_motionDrag.holder, 'anchor', [newAncX, newAncY]);
    } else if (_motionDrag.mode === 'effector') {
      // Plain mutation, not setValue/keyframe — effectors are static
      // per-duplicator config (like its mode/rows/radius/etc.), not a
      // Motion-keyframable property (see the effectors plan's own note on
      // why: a variable-length array doesn't fit propsFor's fixed list).
      // The already-shipped renderNow() duplicator guard (engine-bridge.js)
      // re-materializes on the very next render, so this needs no explicit
      // loadFrame call here.
      var localEff=outerLocalPoint(_motionDrag.t,{x:event.point.x,y:event.point.y});
      _motionDrag.eff.pos={x:localEff.x,y:localEff.y};
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
      if (_motionDrag.axisDir) {
        // Edge handle (feedback #98) — signed projection of the drag onto
        // the handle's OWN axis (captured at grab time in onDown), so only
        // the corresponding scale component moves; dragging past the pivot
        // legally flips the sign, same as the uniform corner path below
        // never guarding against it either.
        var edx = event.point.x - _motionDrag.pivot.x, edy = event.point.y - _motionDrag.pivot.y;
        var eProj = edx * _motionDrag.axisDir.ux + edy * _motionDrag.axisDir.uy;
        var eRatio = eProj / _motionDrag.origDist;
        if (_motionDrag.dir === 'n' || _motionDrag.dir === 's') {
          setValue(_motionDrag.t.holder, 'scale', [_motionDrag.origScale[0], _motionDrag.origScale[1] * eRatio]);
        } else {
          setValue(_motionDrag.t.holder, 'scale', [_motionDrag.origScale[0] * eRatio, _motionDrag.origScale[1]]);
        }
      } else {
        // Corner handle — uniform scale, both axes move by the same ratio.
        var dist = Math.hypot(event.point.x - _motionDrag.pivot.x, event.point.y - _motionDrag.pivot.y);
        var ratio = dist / _motionDrag.origDist;
        setValue(_motionDrag.t.holder, 'scale', [_motionDrag.origScale[0] * ratio, _motionDrag.origScale[1] * ratio]);
      }
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
    liveRefreshVisiblePropertyFields();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    return true;
  }
  // Cheap live refresh of number fields during a drag (feedback #211, "les
  // valeurs de propriété dans layer properties ne changent pas en temps
  // réel pendant les modifications dans le canvas") — every onDrag branch
  // above writes the new value via setValue/setSelectedKeyDimension, which
  // updates the DATA and repaints the CANVAS (SMEngineBridge.renderNow),
  // but never touched the already-built <input> elements showing that same
  // value in the panel, so they sat stale until the drag ended and some
  // other action forced a full rebuild. A full renderLayerList()/
  // renderTimeline() on every pointermove tick would work but is exactly
  // the "rebuild the whole DOM because ONE value changed" cost CLAUDE.md
  // §5bis's own perf pass measured and fixed elsewhere in this file —
  // this only ever touches .value on whichever fields are ALREADY on
  // screen (bottom Transform group + right-panel mirror both tagged
  // _smHolder/_smProp by decorateMotionPropertyRow), never rebuilds a row.
  // Deliberately reads EVERY visible row rather than tracking exactly which
  // holder/prop the current _motionDrag touched: several branches above
  // (multiLayerMove/Scale/Rotate) write MANY holders in one tick, and this
  // stays cheap regardless — typically a handful of rows are ever expanded
  // at once. Skips a field the user has focused (mid-typing) so a live
  // refresh can never overwrite a keystroke in progress.
  function liveRefreshVisiblePropertyFields() {
    document.querySelectorAll('.lrow.motion-prop-row').forEach(function (pr) {
      var holder = pr._smHolder, prop = pr._smProp;
      if (!holder || !prop) return;
      var inputs = pr.querySelectorAll('.motion-val');
      if (!inputs.length) return;
      var vals = displayValueFor(holder, prop);
      for (var i = 0; i < inputs.length; i++) {
        if (document.activeElement === inputs[i]) continue;
        if (i >= vals.length) continue;
        inputs[i].value = fmtVal(vals[i]);
      }
    });
  }
  function onUp() {
    if (!_motionDrag) return false;
    _motionDrag = null;
    renderLayerList(); // scrub fields must reflect the dragged position/handle
    // ...and the GRID half, or a key the drag just created is invisible
    // until something unrelated happens to repaint it (2026-08-31, feedback
    // en Motion: "si j'ai mis des keyframes à une des shape dans le groupe
    // que je la bouge cela ne créer pas de keyframes tout de suite dans le
    // propertie en question dans la timeline"). setValue DOES write the key
    // — isAnimated is true, so it goes through setKeyAtCurrentFrame — the
    // diamond simply had no repaint to appear in. Once per gesture END, not
    // per tick: renderTimeline rebuilds every row (CLAUDE.md §5bis measured
    // 27.7ms at 40 layers), which is exactly why the drag itself must not
    // call it.
    renderTimeline();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    return true;
  }

  // ---- Motion mode UI: layer list (Transform property rows) ----
  function fmtVal(n) { return Math.round(n * 10) / 10; }
  function scrubField(value, onCommit, mixed) {
    var inp = document.createElement('input');
    // Typed edits are absolute (useful to align several keys). A horizontal
    // scrub is relative: ui.js raises _scrubLiveActive while it dispatches
    // the live/final change events, so each emitted input value can be
    // converted into a delta and added to every selected key without
    // collapsing their existing spacing.
    var lastScrubValue = mixed ? 0 : (Number(value) || 0);
    inp.type = 'number'; inp.className = 'pi scrub motion-val' + (mixed ? ' mixed' : '');
    inp.value = mixed ? '' : fmtVal(value);
    if (mixed) { inp.placeholder = '—'; inp.title = SM.t('titleMixedValuesHint'); }
    inp.step = 1;
    inp.addEventListener('change', function () {
      if (inp.value === '' || !isFinite(parseFloat(inp.value))) return;
      var nextValue = parseFloat(inp.value);
      var relative = !!window._scrubLiveActive;
      onCommit(nextValue, { relative: relative, delta: relative ? nextValue - lastScrubValue : 0 });
      lastScrubValue = nextValue;
    });
    inp.addEventListener('click', function (e) { e.stopPropagation(); });
    inp.addEventListener('mousedown', function (e) { e.stopPropagation(); });
    return inp;
  }
  function selectedKeysForEditableProperty(holder, prop) {
    if (!isMotionPropSelected(holder, prop)) return [];
    // Editing one selected Position row applies to every selected Position
    // track, including other layers, but never leaks into a simultaneously
    // selected Rotation/Opacity row whose dimensional meaning is different.
    return _motionKeySel.filter(function (s) { return s.prop === prop && s.key && s.key.v; });
  }
  function selectedDimensionDisplay(holder, prop, dim, fallback) {
    var keys = selectedKeysForEditableProperty(holder, prop).filter(function (s) { return dim < s.key.v.length; });
    if (!keys.length) return { value: fallback, mixed: false };
    var first = Number(keys[0].key.v[dim]) || 0;
    var mixed = keys.some(function (s) { return Math.abs((Number(s.key.v[dim]) || 0) - first) > 1e-9; });
    return { value: first, mixed: mixed };
  }
  function setSelectedKeyDimension(holder, prop, dim, value) {
    var keys = selectedKeysForEditableProperty(holder, prop).filter(function (s) { return dim < s.key.v.length; });
    if (!keys.length) return 0;
    keys.forEach(function (s) { s.key.v[dim] = value; });
    return keys.length;
  }
  function offsetSelectedKeyDimension(holder, prop, dim, delta) {
    var keys = selectedKeysForEditableProperty(holder, prop).filter(function (s) { return dim < s.key.v.length; });
    if (!keys.length) return 0;
    if (Math.abs(delta) > 1e-12) keys.forEach(function (s) { s.key.v[dim] = (Number(s.key.v[dim]) || 0) + delta; });
    return keys.length;
  }
  function setSelectedKeyVector(holder, prop, values) {
    var keys = selectedKeysForEditableProperty(holder, prop);
    if (!keys.length) return 0;
    keys.forEach(function (s) { s.key.v = values.slice(); });
    return keys.length;
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
    //
    // Only when the component holds exactly ONE layer (2026-07-30 fix,
    // Cyril: "si je met 2 calques avec un matte... que je met dans
    // componant... en ouvrant le componant les 2 calques sont séparé en 4
    // formes... je devrais me retrouver avec la même chose que les calques
    // select que j'ai mis dans le componant"). The motivating case for
    // auto-split (CLAUDE.md §8) was always a SINGLE hand-drawn layer with
    // several shapes bundled together — entering is the only way to get
    // real per-shape structure out of that, since there's nothing else to
    // preserve. A component built from convertLayersToComponent (2+
    // pre-existing, deliberately separate layers) already has the
    // structure the user chose; silently re-exploding EACH of those
    // layers by its own shape count on every entry second-guesses a
    // decision already made outside the component (and, concretely,
    // scrambles any matte/blendMode adjacency between them — matte's
    // source is "the layer directly above", which a silent re-split can
    // reorder). Manual "Release to Layers" from the layer context menu
    // (splitLayerIntoElements, not -Core) still reaches any one bundled
    // layer explicitly, with its own undo step — this only removes the
    // SILENT, automatic, on-every-entry version for the multi-layer case.
    if (window.SM && window.SM.splitLayerIntoElementsCore && state.layers.length === 1) {
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
      if (willSplit) showToast(SM.t('toastSplitToLayersUndoHint'));
    }
  }
  // Bar highlighting (layer-inout.js's own _barSel, used for group bar-drags
  // and rendered as the white outline on a timeline bar) and this module's
  // _layerSel (Cmd/Shift multi-select, the stagger/skew box) are two
  // independently-tracked selections that both render as the SAME visual
  // state on a layer's bar — every plain-click path that changes _layerSel
  // used to leave the other exactly where it was. Found live: Shift-click 2
  // bars to multi-select them, then plain-click a layer's NAME in the list
  // — the name's row highlighted correctly and the panel followed, but both
  // bars stayed lit with their OLD selection outline, and a following plain
  // click back on one of those still-lit bars read as a (zero-distance,
  // no-op) GROUP drag instead of updating the selection at all, because
  // layer-inout.js's own onDown branches on ITS OWN bar selection still
  // finding 2+ members (Cyril: "la selection des calques... pas hyper
  // bonne"). Call this after every _layerSel assignment so the two can
  // never drift apart — mirrors the fix already applied to the "click
  // empty grid space" case a few hundred lines down (endMarquee).
  function syncBarSelToLayerSel() {
    if (window.SMLayerInOut && SMLayerInOut.setBarSelection) {
      // Second arg carries the layer-list's frozen anchor into the bar side
      // (_barAnchorLi) so a bar-side Shift-click right after a row-side
      // Shift/Ctrl-click ranges from the SAME anchor the user's last click
      // established, not an unrelated stale one (2026-07-31 unification).
      SMLayerInOut.setBarSelection(_layerSel.map(function (li) { return { li: li, part: 'both' }; }), _layerSelAnchor >= 0 ? _layerSelAnchor : null);
    }
  }
  // Reverse direction of syncBarSelToLayerSel — closes the desync the bar
  // side's own Shift/Ctrl clicks introduced (2026-07-31, scoping: a bar-built
  // multi-selection never reached _layerSel, so the row highlight and every
  // _layerSel-gated menu item — 'Fusionner les calques sélectionnés',
  // 'Grouper en dossier' — ignored it; only a PLAIN bar click resynced, via
  // selectLayerFromGrid). Called from layer-inout.js's onDown modifier
  // branches with the bar selection it just built and the anchor it used.
  function syncLayerSelFromBarSel(barSel, anchorLi) {
    _layerSel = [];
    (barSel || []).forEach(function (s) { if (_layerSel.indexOf(s.li) < 0) _layerSel.push(s.li); });
    if (anchorLi != null && state.layers[anchorLi]) _layerSelAnchor = anchorLi;
    renderLayerList(); renderTimeline();
  }
  function syncMotionContextHeader() {
    var label = document.getElementById('motion-context-label');
    var list = document.getElementById('layer-list');
    if (!label || !list || state.appMode !== 'motion') return;
    var lr = list.getBoundingClientRect();
    var rows = list.querySelectorAll('.lrow[data-layer]');
    var li = state.activeLayerIdx;
    if (rows.length) li = parseInt(rows[0].dataset.layer, 10);
    for (var i = 0; i < rows.length; i++) {
      var rr = rows[i].getBoundingClientRect();
      // Keep the last layer header that has crossed the top edge. Its
      // property rows may still fill the viewport even though the header
      // itself has scrolled away; choosing the next header merely because
      // it exists below would announce the wrong context too early.
      if (rr.top <= lr.top + 2) li = parseInt(rows[i].dataset.layer, 10);
      else break;
    }
    var ld = state.layers[li];
    label.textContent = ld ? (ld.name || ('Layer ' + (li + 1))) : 'Motion';
    label.title = ld ? ('Contexte visible : ' + label.textContent) : 'Motion';
  }
  function ensureMotionHeaderTools() {
    var hdr = document.getElementById('layer-hdr');
    var panel = document.getElementById('layer-panel');
    var list = document.getElementById('layer-list');
    if (!hdr || !panel || !list) return;
    panel.dataset.motionColumns = _motionColumnPreset;
    var tools = document.getElementById('motion-header-tools');
    if (!tools) {
      tools = document.createElement('div'); tools.id = 'motion-header-tools';
      var label = document.createElement('span'); label.id = 'motion-context-label'; label.textContent = 'Motion';
      var btn = document.createElement('button'); btn.id = 'motion-filter-trigger'; btn.type = 'button';
      btn.title = 'Rechercher, filtrer et configurer les colonnes';
      btn.innerHTML = '<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2"><circle cx="10.5" cy="10.5" r="6.5"/><path d="m15.5 15.5 5 5"/></svg>';
      tools.appendChild(label); tools.appendChild(btn); hdr.appendChild(tools);
      btn.addEventListener('click', function (e) {
        e.stopPropagation();
        var old = document.getElementById('motion-filter-pop');
        if (old) { old.remove(); return; }
        var pop = document.createElement('div'); pop.id = 'motion-filter-pop'; pop.className = 'ctx-menu motion-filter-pop';
        var search = document.createElement('input'); search.type = 'search'; search.className = 'motion-filter-search';
        search.placeholder = 'Calque ou propriété…'; search.value = _motionSearch;
        var filterLabel = document.createElement('label'); filterLabel.textContent = 'Afficher';
        var filter = document.createElement('select'); filter.className = 'motion-filter-select';
        [['all','Tout'],['animated','Animé'],['modified','Modifié'],['expressions','Expressions'],['errors','Erreurs'],['effects','Effets']].forEach(function (it) {
          var o = document.createElement('option'); o.value = it[0]; o.textContent = it[1]; filter.appendChild(o);
        });
        filter.value = _motionFilterMode;
        var colLabel = document.createElement('label'); colLabel.textContent = 'Colonnes';
        var cols = document.createElement('select'); cols.className = 'motion-filter-select';
        [['compact','Compact'],['animation','Animation'],['compositing','Compositing'],['3d','3D / Mograph']].forEach(function (it) {
          var o = document.createElement('option'); o.value = it[0]; o.textContent = it[1]; cols.appendChild(o);
        });
        cols.value = _motionColumnPreset;
        var snapRow = document.createElement('label'); snapRow.className = 'motion-filter-check';
        var snap = document.createElement('input'); snap.type = 'checkbox'; snap.checked = _motionSnapEnabled;
        snapRow.appendChild(snap); snapRow.appendChild(document.createTextNode(' Magnétisme des clés'));
        var hint = document.createElement('div'); hint.className = 'motion-filter-hint'; hint.textContent = SM.t('motionSnapDragHint');
        pop.appendChild(search); pop.appendChild(filterLabel); pop.appendChild(filter); pop.appendChild(colLabel); pop.appendChild(cols); pop.appendChild(snapRow); pop.appendChild(hint);
        document.body.appendChild(pop);
        var br = btn.getBoundingClientRect();
        pop.style.left = Math.max(6, Math.min(window.innerWidth - pop.offsetWidth - 6, br.left)) + 'px';
        pop.style.top = Math.min(window.innerHeight - pop.offsetHeight - 6, br.bottom + 5) + 'px';
        function rerender() { renderLayerList(); renderTimeline(); }
        search.addEventListener('input', function () { _motionSearch = search.value.trim().toLowerCase(); rerender(); });
        filter.addEventListener('change', function () { _motionFilterMode = filter.value; try { localStorage.setItem('nemo-motion-filter', _motionFilterMode); } catch (x) {} rerender(); });
        cols.addEventListener('change', function () { _motionColumnPreset = cols.value; panel.dataset.motionColumns = _motionColumnPreset; try { localStorage.setItem('nemo-motion-columns', _motionColumnPreset); } catch (x) {} });
        snap.addEventListener('change', function () { _motionSnapEnabled = snap.checked; try { localStorage.setItem('nemo-motion-snap', _motionSnapEnabled ? '1' : '0'); } catch (x) {} });
        function close(ev) { if (!pop.contains(ev.target) && ev.target !== btn) { pop.remove(); document.removeEventListener('pointerdown', close, true); } }
        setTimeout(function () { document.addEventListener('pointerdown', close, true); search.focus(); search.select(); }, 0);
      });
    }
    if (!list._motionContextBound) {
      list._motionContextBound = true;
      list.addEventListener('scroll', syncMotionContextHeader, { passive: true });
    }
    requestAnimationFrame(syncMotionContextHeader);
  }
  function renderLayerListMotion(list) {
    ensureMotionHeaderTools();
    // Camera row (2026-08-31, feedback #186: "le layer camera n'apparait
    // pas dans motion il faudrait le mettre avec ses propre properties
    // ainsi que dans layer properties"). renderPanelRow (camera.js) is
    // already mode-agnostic — icon, eye toggle, click to switch to the
    // camera tool, right-click menu — the ONLY reason it never showed here
    // is that Animation 2D's own renderLayerList calls it, and this
    // function is Motion's own early-return replacement for that whole
    // function (see renderLayerList's own comment), so the call was simply
    // never reached. Same row, unmodified: clicking it sets state.tool =
    // 'camera', which already shows the SAME #camera-sec panel (properties:
    // position/width/rotation keys, ease curve) regardless of appMode —
    // updateCameraPanel's own show condition never checked appMode either.
    if (window.SMCamera) SMCamera.renderPanelRow(list);
    if (!list._motionEmptySelectBound) {
      list._motionEmptySelectBound = true;
      list.addEventListener('pointerdown', function (e) {
        if (e.target !== list || state.appMode !== 'motion') return;
        _layerSel = [];
        syncBarSelToLayerSel();
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
      if (!layerMatchesMotionView(ld)) return;
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
      // Folder layer (2026-08) — .in-folder is the SAME indent class
      // Animation 2D's own ld.folderId rows already use (style.css), so a
      // nested layer looks identically indented in both panels without a
      // second CSS rule. folderLayerParent comes straight from
      // computeLayerRenderOrder's splice, not re-derived here.
      row.className = 'lrow' + (_layerSel.indexOf(li) >= 0 ? ' act motion-selected' : '') + (entry.folderLayerParent != null ? ' in-folder' : '');
      row.dataset.layer = li;
      if (isComponent) row.title = SM.t('titleComponentRowHint');
      // Every row reserves this slot (real chevron OR invisible spacer),
      // never just folder-related ones — same "spacer" idiom the plain
      // twirl `arrow` right below and Animation 2D's own folder rows
      // already use (timeline.js comment: "every row reserves a
      // .larrow/.larrow-spacer slot so icons stay aligned whether or not
      // the row above was a folder"). Skipping the spacer on ordinary rows
      // would shift every OTHER icon (color dot, eye, lock, solo…) one
      // slot left relative to folder rows, breaking column alignment
      // across the whole panel — not just within a folder's own rows.
      if (ld.isFolderLayer) {
        // Disclosure triangle for the folder's OWN children — deliberately
        // a SEPARATE control from `arrow` below (that one twirls THIS
        // layer's own Transform properties open/closed, unrelated concept:
        // AE lets you have a folder's properties open while its children
        // are hidden, and vice versa).
        var farrow = document.createElement('div'); farrow.className = 'lico larrow folder-arrow';
        farrow.textContent = ld.folderCollapsed ? '▸' : '▾';
        farrow.title = ld.folderCollapsed ? 'Déplier le dossier' : 'Replier le dossier';
        farrow.addEventListener('click', function (e) {
          e.stopPropagation();
          window.SM.toggleFolderLayerCollapsed(li);
        });
        row.appendChild(farrow);
      } else {
        var fspacer = document.createElement('div'); fspacer.className = 'lico larrow-spacer folder-arrow-spacer';
        row.appendChild(fspacer);
      }
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
        if (window._motionRevealedElementLayers) {
          var rei2 = window._motionRevealedElementLayers.indexOf(li);
          if (rei2 >= 0) window._motionRevealedElementLayers.splice(rei2, 1);
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
      var cdot = document.createElement('div'); cdot.className = 'lico layer-color-dot motion-col-color'; cdot.title = 'Couleur du calque';
      cdot.style.setProperty('--dot-color', ld.color || '#8b8b9e');
      cdot.addEventListener('click', function (e) {
        e.stopPropagation();
        openLayerColorSwatches(cdot, ld.color || '#8b8b9e', function (hex) { ld.color = hex; cdot.style.setProperty('--dot-color', hex); renderTimeline(); });
      });
      row.appendChild(cdot);
      var eye = document.createElement('div'); eye.className = 'lico motion-col-visibility' + (ld.visible ? '' : ' off'); eye.title = SM.t('layerEyeTitle'); eye.innerHTML = ld.visible ? ICO_EYE : ICO_EYE_CLOSED;
      eye.addEventListener('click', function (e) { e.stopPropagation(); window.SM.toggleLayerVis(li); });
      row.appendChild(eye);
      var lock = document.createElement('div'); lock.className = 'lico motion-col-lock' + (ld.locked ? '' : ' off'); lock.title = SM.t('layerLockTitle'); lock.innerHTML = ld.locked ? ICO_LOCK : ICO_UNLOCK;
      lock.addEventListener('click', function (e) { e.stopPropagation(); window.SM.toggleLayerLock(li); });
      row.appendChild(lock);
      var solo = document.createElement('div'); solo.className = 'lico solo-btn motion-col-solo' + (ld.solo ? ' on' : ' off'); solo.title = SM.t('layerSoloTitle'); solo.textContent = 'S';
      solo.addEventListener('click', function (e) { e.stopPropagation(); window.SM.toggleLayerSolo(li); });
      row.appendChild(solo);
      // 3D layer toggle (2026-07-28) — same icon/button as Animation 2D's
      // own layer list (timeline.js renderLayerList), shown here too since
      // this is precisely where the Position Z/Rotation X/Y properties it
      // reveals actually live and get keyframed.
      var d3 = document.createElement('div'); d3.className = 'lico motion-col-3d' + (ld.threeD ? '' : ' off'); d3.title = '3D Layer'; d3.innerHTML = ICO_3D;
      d3.addEventListener('click', function (e) { e.stopPropagation(); toggleLayer3D(li); renderLayerList(); });
      row.appendChild(d3);
      // Mograph duplicator toggle — shown here too since the dupOffset*
      // properties it reveals live/get keyframed in this list (same
      // reasoning as the 3D toggle above).
      var ddup = document.createElement('div'); ddup.className = 'lico motion-col-duplicator' + (ld.duplicator ? '' : ' off'); ddup.title = 'Duplicator (grille / radial / chemin)'; ddup.innerHTML = ICO_DUP;
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
      // Parent-in-Time badge (2026-07-30, "on ne sait pas si c'est parent ou
      // pas") — Motion is exactly where a time-linked layer's OWN row lives,
      // yet unlike the spatial Parent pill right below (which always shows,
      // empty or not), a timeLink had no indicator anywhere outside the
      // panel's own expanded Temps row — collapse that and the link became
      // invisible right where it matters most. Right-click unlinks directly,
      // same fast path as the Parent pill and renderTimeLinkRow's own name
      // pill (both a few hundred lines down) — Cyril: "ça peut être un
      // raccourci ou clic droit sur les boutons de parent".
      if (ld.timeLink) {
        var tlIdx2 = -1;
        state.layers.forEach(function (o, oi) { if (o !== ld && o.layerUid === ld.timeLink.uid) tlIdx2 = oi; });
        var tlName2 = tlIdx2 >= 0 ? (state.layers[tlIdx2].name || ('Layer ' + (tlIdx2 + 1))) : 'source introuvable';
        var tlb2 = document.createElement('div'); tlb2.className = 'lico comp-badge';
        tlb2.title = SM.t('titleTimeLinkedToPrefix') + tlName2 + SM.t('titleTimeLinkedToSuffix');
        tlb2.innerHTML = '<span style="font-size:9px;line-height:1;font-weight:700">Tp</span>';
        tlb2.addEventListener('click', function (e) { e.stopPropagation(); });
        tlb2.addEventListener('contextmenu', function (e) {
          e.preventDefault(); e.stopPropagation();
          pushUndo(); unlinkTimeLinkPreserveRange(ld);
          renderLayerList(); renderTimeline();
          if (window.loadFrame) loadFrame(state.currentFrame);
          if (window.SMEngineBridge) SMEngineBridge.renderNow();
          if (window.showToast) showToast(SM.t('toastTimeLinkRemoved'));
        });
        row.appendChild(tlb2);
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
      if (typeof buildParentCell === 'function') {
        buildParentCell(row, ld, li);
        if (row.lastElementChild && row.lastElementChild.classList.contains('lparent')) row.lastElementChild.classList.add('motion-col-parent');
      }
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
          _layerSelAnchor = li;
          syncBarSelToLayerSel();
          window.SM.setActiveLayer(li, true);
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
          // Frozen anchor (2026-07-31 — see _layerSelAnchor's declaration in
          // timeline.js): _layerSel[0] drifted after any Shift-click toward a
          // lower index; the dedicated anchor never moves under Shift.
          var anchor = (_layerSelAnchor >= 0 && _layerSelAnchor < state.layers.length) ? _layerSelAnchor : (_layerSel.length ? _layerSel[0] : state.activeLayerIdx);
          _layerSel = [];
          for (var l = Math.min(anchor, li); l <= Math.max(anchor, li); l++) _layerSel.push(l);
          syncBarSelToLayerSel();
          window.SM.setActiveLayer(li, true);
          renderLayerList(); renderTimeline();
          return;
        }
        _layerSel = [li];
        _layerSelAnchor = li;
        syncBarSelToLayerSel();
        // A row can be open via the single-accordion state OR via U's
        // reveal set (or both) — always drop it from the reveal set on
        // click, but only touch the single-accordion value if THIS row is
        // the one holding it, so clicking a U-revealed row never collapses
        // some unrelated row that's separately accordion-open.
        if (window._motionRevealedLayers) {
          var ri = window._motionRevealedLayers.indexOf(li);
          if (ri >= 0) window._motionRevealedLayers.splice(ri, 1);
        }
        if (window._motionRevealedElementLayers) {
          var rei = window._motionRevealedElementLayers.indexOf(li);
          if (rei >= 0) window._motionRevealedElementLayers.splice(rei, 1);
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
      function beginMotionLayerReorder(e) {
        if (e.button !== 0 || e.target.closest('.lico')) return;
        armLayerReorder(e, li, 'panel', row);
      }
      row.addEventListener('mousedown', beginMotionLayerReorder);
      row.addEventListener('pointerdown', beginMotionLayerReorder);
      row.addEventListener('dblclick', function (e) {
        if (e.target.closest('.lico')) return;
        // Re-reversed 2026-07-17 ("montage des éléments dans le
        // component") — a Component layer (converted manually, see
        // convertLayerToComponent) now DOES have an "enter as precomp"
        // double-click again, but only once it's a Component: a plain
        // layer with several unrelated shapes still gets the old
        // Release-to-Layers split, since there's no "inside" to browse
        // for a layer that was never converted.
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
          // Renommer (2026-07-31, shortcut-parity sweep): Motion mode had NO
          // rename path at all — Animation 2D's dblclick is claimed here by
          // enterComponentLayer/splitLayerIntoElements, and this menu never
          // had the entry Animation 2D's row menu has. startLayerRename
          // (timeline.js) works unmodified against Motion rows (same
          // .lrow[data-layer]/.lnm DOM shape). F2 is the keyboard path.
          { label: SM.t('elementsRename') + '  (F2)', action: function () { startLayerRename(li); } },
          // Folder layer (2026-08) — disabled unless this row's parent
          // resolves to a Folder specifically (an ordinary Null parent
          // leaves this disabled, same guard the drag-out gesture uses —
          // see folderLayerParentIdx, timeline.js).
          { label: SM.t('ctxRemoveFromFolder'), disabled: (typeof folderLayerParentIdx !== 'function' || folderLayerParentIdx(li) < 0), action: function () { window.SM.removeLayerFromFolder(li); } },
          // Component / camera / audio (2026-08-30, Cyril: "la possibilité de
          // créer un composant dans motion... et pareil pour layer sound et
          // camera"). The three header buttons are visible in Motion again
          // (style.css), and these mirror them in the menu, the way
          // Animation 2D's own row menu already carries convert/break-apart
          // next to the split/merge pair. Camera and audio delegate to their
          // existing buttons rather than reimplementing them — one wiring,
          // one behaviour, and camera's default-keyframe/undo handling stays
          // in camera.js where it belongs.
          { label: SM.t('ctxConvertToComponent'), disabled: !!ld.symbolId || !!ld.lfsGroup, action: function () { window.SM.setActiveLayer(li); window.SM.convertActiveLayerToComponent(); } },
          { label: SM.t('ctxBreakApartComponent'), disabled: !ld.symbolId, action: function () { window.SM.setActiveLayer(li); window.SM.convertComponentToLayer(); } },
          { label: SM.t('ctxInsertCameraLayer'), action: function () { var b = document.getElementById('btn-camera'); if (b) b.click(); } },
          { label: SM.t('ctxInsertAudioTrack'), action: function () { var b = document.getElementById('btn-audio'); if (b) b.click(); } },
          { label: SM.t('ctxSplitIntoLayers'), disabled: !!ld.symbolId || !!ld.lfsGroup, action: function () { window.SM.splitLayerIntoElements(li); } },
          { label: SM.t('ctxCutAtPlayhead'), action: function () { window.SM.splitLayerAtPlayhead(li); } },
          { label: ld.shy ? SM.t('ctxRemoveShyMark') : SM.t('ctxMarkAsShy'), action: function () { window.SM.toggleLayerShy(li); } },
          { label: ld.motionBlur ? SM.t('ctxDisableMotionBlur') : SM.t('ctxEnableMotionBlur'), action: function () { window.SM.toggleLayerMotionBlur(li); } },
          { label: ld.effectsFrom ? SM.t('ctxStopInheritEffects') : SM.t('ctxInheritEffectsFromEllipsis'), action: function () {
            if (ld.effectsFrom) { pushUndo(); delete ld.effectsFrom; renderLayerList(); renderTimeline(); if (window.SMEngineBridge) SMEngineBridge.renderNow(); return; }
            var items = [];
            state.layers.forEach(function (other, oi) {
              if (oi === li || !other.effects || !other.effects.length) return;
              items.push({ label: (other.name || ('Layer ' + (oi + 1))) + '  (' + other.effects.length + ')', action: function () {
                pushUndo();
                ld.effectsFrom = ensureLayerUid(other);
                renderLayerList(); renderTimeline();
                if (window.SMEngineBridge) SMEngineBridge.renderNow();
                if (window.showToast) showToast(SM.t('toastEffectsInheritedFromSuffix') + (other.name || ('Layer ' + (oi + 1))) + SM.t('toastEffectsInheritedFollowOwnKeysSuffix'));
              } });
            });
            if (!items.length) { if (window.showToast) showToast(SM.t('toastNoOtherLayerWithEffects')); return; }
            window.showContextMenu(e.clientX + 8, e.clientY + 8, items);
          } },
          // Menu-based Parent-in-Time (2026-07-31) \u2014 creation used to be
          // pickwhip-drag ONLY; reachable now from any right-click on the
          // row regardless of what else (keyframes, bars) is selected.
          { label: SM.t('ctxParentInTimeLinkTimeEllipsis'), action: function () {
            window.showContextMenu(e.clientX + 8, e.clientY + 8, window.buildTimeLinkMenuItems(li, ld, function () { renderLayerList(); renderTimeline(); }));
          } },
          // Expression controls (2026-08-30) — the layer menu is where
          // everything else that belongs to a whole LAYER already lives
          // (parenting, time linking, key locking, markers), and a control
          // is exactly that: a parameter of this layer, not of one of its
          // properties.
          { label: SM.t('ctxExprControlsEllipsis'), action: function () { openExprControlsMenu(e.clientX + 8, e.clientY + 8, ld); } },
          // Rig widget range/size (2026-08-30) — same reasoning as the
          // controls entry right above (it belongs to the whole layer), and
          // shown only on a widget layer, the "hidden until its
          // prerequisite is set" convention Time Remap already uses. Motion
          // has its OWN layer-row menu, separate from the Animation 2D one
          // (timeline.js) that carries the same entry — a widget is edited
          // from whichever timeline you happen to be in.
          ...(ld.isWidgetLayer && window.SMRigWidget ? [{ label: SM.t('ctxWidgetSettingsEllipsis'), action: function () { SMRigWidget.openWidgetMenu(e.clientX + 8, e.clientY + 8, li); } }] : []),
          // Effector layer (2026-08-30) — offered only on a layer that has a
          // duplicator, since an effector with nothing to affect is a dead
          // control. Same "only where it means something" gate as the rows
          // propsFor adds for parentBlend and matteOn.
          ...(ld.duplicator && window.SMEffectorLayer ? [{ label: SM.t('ctxAddEffectorLayer'), action: function () { SMEffectorLayer.addEffectorLayer(li); } }] : []),
          { sep: true },
          // showContextMenu has no submenus — a disabled row is the honest
          // way to title a group rather than a button that does nothing.
          ...buildKeyLockMenuItems(li, ld),
          { label: SM.t('ctxAddMarkerOnLayer'), action: function () { if (window.SMMarkers) SMMarkers.addLayerMarker(li, state.currentFrame, ''); } },
          // UI/UX audit (2026-07-30): used to grey this out via `disabled`
          // when the layer isn't a Component — showContextMenu has no
          // hover-title mechanism for disabled rows, so that state
          // explained nothing. enableTimeRemap already has the exact
          // guard toast needed ('Le remappage temporel s'applique aux
          // calques composants') — leaving the row always clickable lets
          // that existing message do the explaining instead of a silent
          // grey row.
          { label: ld.timeRemap ? SM.t('ctxDisableTimeRemap') : SM.t('ctxEnableTimeRemap'),
            action: function () { ld.timeRemap ? disableTimeRemap(li) : enableTimeRemap(li); } },
          { label: SM.t('ctxMergeSelectedLayers'), disabled: !multi, action: function () { window.SM.mergeLayersIntoOne(_layerSel.slice()); } },
          { sep: true },
          { label: SM.t('ctxStaggerSelectedLayersEllipsis'), disabled: !multi, action: function () {
            var v = prompt(SM.t('promptStaggerOffsetFrames'), '2');
            var step = parseInt(v, 10);
            if (!isNaN(step) && step !== 0) staggerSelectedLayers(step);
          } },
        ]);
      });
      list.appendChild(row);
      if (!expanded) return;
      // Blend/Parent (feedback #207) get their own keyframe-bearing rows
      // HERE too, not just in the right-panel mirror — "mettre les
      // keyframes à droite pour cohérence dans le panel" meant the bottom
      // #layer-list/#frame-grid split specifically (§11's "panel"/"grid"),
      // where every OTHER keyable property already shows its markers.
      // renderParentRow/renderBlendRow are the exact same functions the
      // right panel calls (renderMotionPropsPanel below) — one writer,
      // just appended to a different parent element — so a key added from
      // either surface is immediately visible on both.
      renderParentRow(list, ld, li);
      renderBlendRow(list, ld, li);
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
      //
      // Gated on the MANUAL accordion specifically (_motionExpandedLayer),
      // not the broader `expanded` (isLayerExpanded also returns true for
      // a property-shortcut reveal set, _motionRevealedLayers) — P/U/S/etc
      // (handlePropShortcut/handleRevealAnimatedShortcut) are meant to
      // reveal a layer's own Transform properties, not cascade into every
      // element's breakdown too (feedback #42, "il ne faut pas afficher
      // éléments"). A real row click still shows elements exactly as
      // before — this only narrows what a SHORTCUT-driven reveal shows.
      // EXCEPT _motionRevealedElementLayers (feedback #145): U specifically
      // also opens this for a layer it already determined HAS animated
      // per-element content (text-animator glyphs, hand-keyed shapes) —
      // narrower than "every U-revealed layer", so #42's clutter complaint
      // stays fixed for the common case of a layer with no per-element keys.
      if (window._motionExpandedLayer === li || (window._motionRevealedElementLayers && window._motionRevealedElementLayers.indexOf(li) >= 0)) renderElementsList(list, li, ld);
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
    var motionDeselected = _motionCanvasEmptyClick || (window._motionExpandedLayer == null && typeof _layerSel !== 'undefined' && _layerSel.length === 0);
    var ld = motionDeselected ? null : state.layers[state.activeLayerIdx];
    var nameRow = document.createElement('div');
    nameRow.className = 'motion-props-layername';
    if (!ld) { nameRow.textContent = SM.t('noLayerSelected'); body.appendChild(nameRow); return; }
    // Component instances get the same Transform group as any layer now
    // (see renderLayerListMotion's comment) — just a label suffix so it
    // reads clearly as "the whole instance", not its internal content.
    nameRow.textContent = (ld.name || ('Layer ' + (state.activeLayerIdx + 1))) + (ld.symbolId ? ' (composant)' : '');
    body.appendChild(nameRow);
    renderParentRow(body, ld, state.activeLayerIdx);
    if (ld.isNullLayer && !ld.isFolderLayer) renderNullShapeRow(body, ld);
    renderFollowPathRow(body, ld, state.activeLayerIdx);
    renderTimeLinkRow(body, ld, state.activeLayerIdx);
    renderBlendRow(body, ld, state.activeLayerIdx);
    renderMatteRow(body, ld, state.activeLayerIdx);
    // A targeted SHAPE shows its own Transform, not the layer's (2026-08-30,
    // feedback #173: "si on select une shape dans elements alors dans layer
    // properties on voit les propriétés de la shape, pareil si on select une
    // box de shape via double clic"). Both routes already converge on the
    // same state — the Elements row click and the canvas double-click both
    // set _motionExpandedElement — so one branch here serves both, which is
    // exactly why the feedback names them together.
    // The layer's own Parent/Path/Time/Matte rows stay above: they belong to
    // the layer whatever is targeted inside it, and dropping them would make
    // the panel lose the parent pill the moment you inspect a shape.
    var elSel = (window._motionExpandedElement != null && window._motionExpandedLayer === state.activeLayerIdx)
      ? window._motionExpandedElement : null;
    if (elSel) {
      var elHolder = ensureElementHolder(ld, elSel);
      renderTransformGroup(body, elHolder, SM.t('hdrTransformElement'));
      return;
    }
    renderTransformGroup(body, ld, 'Transform');
  }
  // Track matte, IN Layer Properties (2026-08-30, "un bouton matte qui
  // ouvre dans le menu déroulant des properties de calques la config pour
  // choisir le calque à matte et si en alpha luma").
  //
  // The pieces already existed but were scattered where they could not be
  // found: the mode lived on #p-mattemode, which only surfaces in the right
  // panel's Document fallback context (nothing selected, no draw tool), and
  // the SOURCE lived in the layer row's context menu. The layer-row badge
  // added earlier only appears once a matte is ALREADY set, so there was no
  // entry point from a layer that has none — the original "je vois pas où
  // appliqué les track matte" was never really closed.
  // Both now sit on one row next to Parent / Follow Path / Time, in the same
  // pill-and-menu idiom, writing the SAME ld.matteMode / ld.matteSourceLayerUid
  // fields the existing dropdown, badge and context menu write, so no
  // surface goes stale.
  function renderMatteRow(body, ld, li) {
    var row = document.createElement('div'); row.className = 'lrow motion-prop-row';
    var label = document.createElement('span');
    label.textContent = SM.t('fieldMatte'); label.style.minWidth = '70px';
    label.title = SM.t('titleMatteHint');
    row.appendChild(label);

    var srcIdx = -1;
    if (ld.matteSourceLayerUid) srcIdx = findLayerIndexByUid(ld.matteSourceLayerUid);
    // AE convention, kept: with no explicit source, the matte comes from the
    // layer directly above. Shown as such rather than left blank, because
    // "none picked" and "none in effect" are different things here.
    if (srcIdx < 0 && ld.matteMode && li + 1 < state.layers.length) srcIdx = li + 1;
    var srcLd = srcIdx >= 0 ? state.layers[srcIdx] : null;

    var pill = document.createElement('div');
    var on = !!(ld.matteMode && ld.matteMode !== 'none');
    pill.className = 'lparent motion-parent-pill' + (on ? '' : ' none');
    var lab = document.createElement('span'); lab.className = 'mp-label';
    lab.textContent = on
      ? ((srcLd ? srcLd.name : '?') + '  ·  ' + matteModeLabelSafe(ld.matteMode))
      : '—';
    pill.appendChild(lab);
    pill.title = SM.t('titleMattePickHint');
    pill.addEventListener('click', function (e) {
      e.preventDefault(); e.stopPropagation();
      openMatteConfigMenu(e.clientX, e.clientY, li, ld);
    });
    row.appendChild(pill);
    // "+" — stack another matte on top of this one (2026-08-30). Only once
    // a first matte exists: an extra is a row UNDER the first, never a
    // matte on its own, which is the same rule resolve_all_mattes enforces
    // engine-side. Offered as a disabled-looking no-op rather than hidden
    // would just raise the question of where it went, so it simply isn't
    // built until there is something to add to.
    if (on) {
      var addBtn = document.createElement('button');
      addBtn.className = 'motion-expr-glob motion-matte-add';
      addBtn.textContent = '+';
      addBtn.title = SM.t('titleMatteAddHint');
      addBtn.addEventListener('click', function (e) {
        e.preventDefault(); e.stopPropagation();
        var pick = firstFreeMatteLayer(li, ld);
        if (pick == null) { if (window.showToast) showToast(SM.t('toastNoLayerLeftForMatte')); return; }
        pushUndoLayers(true);
        ld.mattesMore = ld.mattesMore || [];
        ld.mattesMore.push({ uid: ensureLayerUid(state.layers[pick]), mode: 'alpha' });
        matteChanged();
      });
      row.appendChild(addBtn);
    }
    body.appendChild(row);
    // The 2nd..Nth mattes, one row each, same pill idiom as the first so
    // the stack reads as one list rather than a primary plus exceptions.
    if (on && ld.mattesMore && ld.mattesMore.length) {
      ld.mattesMore.forEach(function (m, mi) {
        var r2 = document.createElement('div'); r2.className = 'lrow motion-prop-row';
        var sp = document.createElement('span');
        sp.textContent = '+ ' + SM.t('fieldMatte'); sp.style.minWidth = '70px';
        sp.style.opacity = '.7';
        r2.appendChild(sp);
        var srcI = findLayerIndexByUid(m.uid);
        var p2 = document.createElement('div');
        p2.className = 'lparent motion-parent-pill';
        var l2 = document.createElement('span'); l2.className = 'mp-label';
        l2.textContent = (srcI >= 0 ? state.layers[srcI].name : '?') + '  ·  ' + matteModeLabelSafe(m.mode);
        p2.appendChild(l2);
        p2.title = SM.t('titleMattePickHint');
        p2.addEventListener('click', function (e) {
          e.preventDefault(); e.stopPropagation();
          openExtraMatteMenu(e.clientX, e.clientY, li, ld, mi);
        });
        r2.appendChild(p2);
        body.appendChild(r2);
      });
    }
    // NO matteOn row is drawn here on purpose. propsFor already lists it
    // once a matte exists, so it arrives in the Transform group below with
    // its stopwatch, ease curves, graph editor and expressions — on BOTH
    // sides. Emitting it here as well would put an extra row on the panel
    // that the frame grid never draws, which is exactly the panel/grid
    // drift CLAUDE.md §11 makes propsFor the single decider to prevent.
  }
  function matteModeLabelSafe(m) {
    return (typeof matteModeLabel === 'function') ? matteModeLabel(m) : m;
  }
  // Blend mode, IN Layer Properties (feedback #201, "je n'ai pas de dropdown
  // menu de blend" — Motion mode genuinely had no way to reach it: the
  // right-panel #p-blendmode dropdown has been display:none in EVERY
  // context since #172 folded it into the layer row's right-click menu
  // (timeline.js), and that replacement was Animation-2D-only — Motion's
  // own row context menu (this file) never got the equivalent entry, so
  // the feature was reachable from neither surface here. Same pill-and-
  // popover idiom as renderMatteRow right below (which got this exact
  // "buried, no entry point" fix for Matte on 2026-08-30), reusing
  // window.openBlendDropdownAt/BLEND_MODE_LABELS (timeline.js) rather than
  // a second dropdown implementation — one writer of ld.blendMode either way.
  function renderBlendRow(body, ld, li) {
    var row = document.createElement('div'); row.className = 'lrow motion-prop-row';
    // Stopwatch (feedback #207, Duik-inspired): turns ld.blendKeys on/off,
    // same on/hasKeyHere/click shape renderColorRow's own stopwatch uses —
    // OFF collapses back to the plain static field (the pre-#207 behavior,
    // still what most other consumers of ld.blendMode read, see
    // layerBlendModeAt's own comment), ON seeds a single key at the
    // playhead with whatever mode was showing.
    var keyed = !!(ld.blendKeys && ld.blendKeys.length);
    var hasKeyHere = keyed && ld.blendKeys.some(function (k) { return k.frame === state.currentFrame; });
    var sw = document.createElement('div');
    sw.className = 'lico motion-stopwatch' + (keyed ? ' on' : '');
    sw.title = stopwatchTitle('motionAnimateFill', keyed, hasKeyHere);
    sw.innerHTML = '<svg viewBox="0 0 24 24" width="12" height="12"><path d="M12 3l9 9-9 9-9-9z" fill="' + (hasKeyHere ? 'currentColor' : 'none') + '" stroke="currentColor" stroke-width="2"/></svg>';
    sw.addEventListener('click', function (e) {
      e.stopPropagation(); pushUndo();
      if (!keyed) {
        ld.blendKeys = [{ frame: state.currentFrame, mode: ld.blendMode || 'normal' }];
      } else if (hasKeyHere) {
        if (ld.blendKeys.length === 1) {
          var fv = ld.blendKeys[0].mode;
          ld.blendMode = fv === 'normal' ? undefined : fv;
          delete ld.blendKeys;
        } else {
          SMMotion.removeBlendKeyAt ? SMMotion.removeBlendKeyAt(ld, state.currentFrame) : (ld.blendKeys = ld.blendKeys.filter(function (k) { return k.frame !== state.currentFrame; }));
        }
      } else {
        SMMotion.upsertBlendKeyAt(ld, state.currentFrame, layerBlendModeAt(li, state.currentFrame));
      }
      // feedback #213 — same stale-row bug the dropdown's own click handler
      // had (timeline.js's initBlendDropdown): this row is shared by the
      // right panel AND the bottom-left list (renderMotionPropsPanel /
      // renderLayerListMotion both call renderBlendRow), but only the
      // list's own rebuild was ever triggered here.
      renderLayerList(); renderTimeline(); renderMotionPropsPanel();
      if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    });
    row.appendChild(sw);
    var label = document.createElement('span');
    label.textContent = SM.t('fieldBlend'); label.style.minWidth = '70px';
    row.appendChild(label);
    var pill = document.createElement('div');
    var mode = layerBlendModeAt(li, state.currentFrame) || 'normal';
    pill.className = 'lparent motion-parent-pill' + (mode === 'normal' ? ' none' : '');
    var lab = document.createElement('span'); lab.className = 'mp-label';
    lab.textContent = (typeof BLEND_MODE_LABELS !== 'undefined' && BLEND_MODE_LABELS[mode]) || mode;
    pill.appendChild(lab);
    pill.addEventListener('click', function (e) {
      e.preventDefault(); e.stopPropagation();
      // feedback #206, "le menu blend ne s'affiche pas là où l'on clic" —
      // setActiveLayer(li) is a no-op here (li IS already state.activeLayerIdx,
      // this row only ever renders for the active layer) but it UNCONDITIONALLY
      // re-renders via updateUI(), which rebuilds #motion-props-body from
      // scratch — by the time openBlendDropdownAt ran, `pill` was a DETACHED
      // node from the PREVIOUS render, and its getBoundingClientRect() on a
      // detached element is all-zero, pinning the popup to the top-left
      // corner (confirmed live from the screenshot). Same trap
      // openBlendDropdownAt's own doc comment already calls out ("a captured
      // DOM node goes stale the moment the list re-renders") and the exact
      // reason the layer-row context-menu entry (timeline.js) passes a
      // synthetic rect built from the click instead of the row itself —
      // mirrored here rather than re-triggering it.
      var _bx = e.clientX, _by = e.clientY;
      if (window.openBlendDropdownAt) window.openBlendDropdownAt({ getBoundingClientRect: function () { return { left: _bx, right: _bx, top: _by, bottom: _by, width: 0, height: 0 }; } });
    });
    row.appendChild(pill);
    body.appendChild(row);
  }
  // One menu: pick the SOURCE layer, then the MODE. Two short lists in one
  // place beat the old split between a right-panel dropdown (mode) and a
  // row context menu (source), which is what made the feature hard to find.
  function openMatteConfigMenu(x, y, li, ld) {
    if (!window.showContextMenu) return;
    var items = [];
    items.push({ label: SM.t('matteMenuSourceHdr'), disabled: true, action: function () {} });
    state.layers.forEach(function (o, oi) {
      if (oi === li) return; // a layer cannot matte itself
      var chosen = ld.matteSourceLayerUid
        ? ld.matteSourceLayerUid === o.layerUid
        : (ld.matteMode && oi === li + 1);
      items.push({
        label: '  ' + (o.name || ('Layer ' + (oi + 1))) + (chosen ? '  ✓' : ''),
        action: function () {
          pushUndoLayers(true);
          ld.matteSourceLayerUid = ensureLayerUid(o);
          // Choosing a source on a layer with no mode yet has to DO
          // something, or the pick silently vanishes: default to Alpha,
          // the mode you almost always want first.
          if (!ld.matteMode || ld.matteMode === 'none') ld.matteMode = 'alpha';
          matteChanged();
        }
      });
    });
    items.push({ sep: true });
    items.push({ label: SM.t('matteMenuModeHdr'), disabled: true, action: function () {} });
    [['alpha', 'matteAlpha'], ['alphaInverted', 'matteAlphaInverted'],
     ['luma', 'matteLuma'], ['lumaInverted', 'matteLumaInverted']].forEach(function (m) {
      items.push({
        label: '  ' + SM.t(m[1]) + (ld.matteMode === m[0] ? '  ✓' : ''),
        action: function () { pushUndoLayers(true); ld.matteMode = m[0]; matteChanged(); }
      });
    });
    if (ld.matteMode && ld.matteMode !== 'none') {
      items.push({ sep: true });
      items.push({ label: SM.t('matteMenuRemove'), action: function () {
        pushUndoLayers(true);
        delete ld.matteMode; delete ld.matteSourceLayerUid;
        // The extras go with it: they are rows UNDER the first matte, and
        // resolve_all_mattes already returns nothing for them once the
        // first is gone. Leaving them behind would keep invisible state
        // that silently reappears the next time a matte is set.
        delete ld.mattesMore;
        // The matteOn track goes too — found by driving (2026-08-30 sweep):
        // with the track left behind, setting a NEW matte later resurrected
        // the old on/off keys, and the fresh matte was mysteriously OFF
        // from wherever the dead track said so, with nothing visible to
        // explain it (the row only renders while a matte exists). Same
        // invisible-state class as mattesMore right above; the expression
        // slot follows for the same reason. Undo restores all of it in one
        // step — pushUndoLayers ran first.
        if (ld.motion) delete ld.motion.matteOn;
        if (ld.motionStatic) delete ld.motionStatic.matteOn;
        if (ld.expressions) delete ld.expressions.matteOn;
        matteChanged();
      } });
    }
    window.showContextMenu(x, y, items);
  }
  // First layer not already used by this layer's matte stack (and not the
  // layer itself), so "+" lands on something meaningful instead of adding a
  // duplicate of the matte already there.
  function firstFreeMatteLayer(li, ld) {
    var used = {};
    if (ld.matteSourceLayerUid) used[ld.matteSourceLayerUid] = 1;
    (ld.mattesMore || []).forEach(function (m) { if (m && m.uid) used[m.uid] = 1; });
    for (var i = 0; i < state.layers.length; i++) {
      if (i === li) continue;
      var u = state.layers[i].layerUid;
      if (u && used[u]) continue;
      return i;
    }
    return null;
  }
  // Same two lists as the first matte's menu, plus Remove — kept a separate
  // function rather than parameterising openMatteConfigMenu because the two
  // write different places (the legacy pair vs an array entry) and folding
  // them would mean a branch on every line of both.
  function openExtraMatteMenu(x, y, li, ld, mi) {
    if (!window.showContextMenu) return;
    var entry = (ld.mattesMore || [])[mi];
    if (!entry) return;
    var items = [];
    items.push({ label: SM.t('matteMenuSourceHdr'), disabled: true, action: function () {} });
    state.layers.forEach(function (o, oi) {
      if (oi === li) return;
      items.push({
        label: '  ' + (o.name || ('Layer ' + (oi + 1))) + (entry.uid === o.layerUid ? '  ✓' : ''),
        action: function () { pushUndoLayers(true); entry.uid = ensureLayerUid(o); matteChanged(); }
      });
    });
    items.push({ sep: true });
    items.push({ label: SM.t('matteMenuModeHdr'), disabled: true, action: function () {} });
    [['alpha', 'matteAlpha'], ['alphaInverted', 'matteAlphaInverted'],
     ['luma', 'matteLuma'], ['lumaInverted', 'matteLumaInverted']].forEach(function (m) {
      items.push({
        label: '  ' + SM.t(m[1]) + (entry.mode === m[0] ? '  ✓' : ''),
        action: function () { pushUndoLayers(true); entry.mode = m[0]; matteChanged(); }
      });
    });
    items.push({ sep: true });
    items.push({ label: SM.t('matteMenuRemove'), action: function () {
      pushUndoLayers(true);
      ld.mattesMore.splice(mi, 1);
      if (!ld.mattesMore.length) delete ld.mattesMore;
      matteChanged();
    } });
    window.showContextMenu(x, y, items);
  }
  function matteChanged() {
    window._sceneVersion = (window._sceneVersion || 0) + 1;
    if (typeof saveActiveLayerFrame === 'function') saveActiveLayerFrame();
    renderLayerList(); renderTimeline();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  // Null shape selector, IN Layer Properties (2026-08 fix, feedback: "le
  // menu pour changé la forme du null doit apparaitre dans layer
  // properties" / "je vois le menu en dehors du menu Layer Prop") — a
  // first pass put this in a separate static psec elsewhere in the
  // properties column; moved here, right into #motion-props-body next to
  // Parent, alongside every other per-layer control. Still writes the
  // same ld.nullShape field the layer-row badge cycles (timeline.js), so
  // both stay in sync.
  function renderNullShapeRow(body, ld) {
    var row = document.createElement('div'); row.className = 'lrow motion-prop-row';
    // 2026-08 fix: label and option text were hardcoded French, showing up
    // even with English (or any other) locale selected — reuse the
    // existing fieldNullShape/nullShapeCross/-Square/-Circle/-Diamond keys
    // (already translated in all 4 locales, just never read from here).
    var label = document.createElement('span'); label.textContent = SM.t('fieldNullShape'); label.style.minWidth = '70px';
    row.appendChild(label);
    var sel = document.createElement('select'); sel.className = 'psel';
    [['cross', SM.t('nullShapeCross')], ['square', SM.t('nullShapeSquare')], ['circle', SM.t('nullShapeCircle')], ['diamond', SM.t('nullShapeDiamond')]].forEach(function (o) {
      var opt = document.createElement('option'); opt.value = o[0]; opt.textContent = o[1];
      if ((ld.nullShape || 'cross') === o[0]) opt.selected = true;
      sel.appendChild(opt);
    });
    sel.addEventListener('change', function () {
      pushUndoLayers(true);
      ld.nullShape = sel.value;
      window._sceneVersion++;
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
      if (window.renderLayerList) renderLayerList();
    });
    row.appendChild(sel);
    body.appendChild(row);
  }
  // Follow Path (2026-08, "motion path unifié" — see the FollowPathEffect
  // research note). Same pill-and-menu idiom as the Parent row right below
  // (deliberately NOT a pickwhip like Parent — a plain click-to-choose menu
  // is enough here, this constraint is set far less often than parenting).
  // "Align" is a small toggle button next to the pill: it just flips
  // ld.followPath.align, which in turn reveals/hides the pathInfluence row
  // in the ordinary Transform group below (propsFor) — no separate
  // keyframe machinery of its own.
  function renderFollowPathRow(body, ld, li) {
    var row = document.createElement('div'); row.className = 'lrow motion-prop-row';
    // 2026-08 fix: hardcoded French, shown regardless of locale.
    var label = document.createElement('span'); label.textContent = SM.t('fieldFollowPath'); label.style.minWidth = '70px';
    label.title = SM.t('titleFollowPathHint');
    row.appendChild(label);

    var fp = ld.followPath;
    var tIdx = fp ? SMMotion.findLayerIndexByUid(fp.targetLayerUid) : -1;
    var tName = (tIdx >= 0 && state.layers[tIdx]) ? (state.layers[tIdx].name || ('Layer ' + (tIdx + 1))) : null;

    var pill = document.createElement('div');
    pill.className = 'lparent motion-parent-pill' + (tName ? '' : ' none');
    pill.title = tName ? (SM.t('titleFollowPathCurrentPrefix') + tName + SM.t('titleFollowPathCurrentSuffix')) : SM.t('titleFollowPathNone');
    var lbl = document.createElement('span'); lbl.className = 'mp-label'; lbl.textContent = tName || '—';
    pill.appendChild(lbl);
    pill.addEventListener('click', function (e) {
      e.stopPropagation();
      if (!window.showContextMenu || !window.buildFollowPathMenuItems) return;
      var items = window.buildFollowPathMenuItems(li, ld, function () { renderMotionPropsPanel(); renderLayerList(); renderTimeline(); });
      var r = pill.getBoundingClientRect();
      window.showContextMenu(r.left, r.bottom + 2, items);
    });
    pill.addEventListener('contextmenu', function (e) {
      e.stopPropagation(); e.preventDefault();
      if (!tName) return;
      pushUndo(); setLayerFollowPath(li, null);
      renderMotionPropsPanel(); renderLayerList(); renderTimeline();
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
    });
    row.appendChild(pill);

    if (tName) {
      var alignBtn = document.createElement('button');
      alignBtn.type = 'button'; alignBtn.className = 'mp-align-btn' + (fp.align ? ' on' : '');
      alignBtn.textContent = SM.t('btnAlign');
      alignBtn.title = SM.t('titleAlignRotationHint');
      alignBtn.addEventListener('click', function (e) {
        e.stopPropagation(); pushUndo();
        ld.followPath.align = !ld.followPath.align;
        renderMotionPropsPanel(); renderLayerList(); renderTimeline();
        if (window.SMEngineBridge) SMEngineBridge.renderNow();
      });
      row.appendChild(alignBtn);
    }
    body.appendChild(row);
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
    // Stopwatch (feedback #207, Duik-inspired hold keys — same on/
    // hasKeyHere/click shape as renderBlendRow's own): ON seeds a key at
    // the playhead with whatever parent (Parent A only — see
    // layerParentUidAt's header comment for why Parent B stays untouched)
    // is currently resolved; OFF collapses back to the plain static field
    // (single key) or removes just the current-frame key (multiple keys).
    var parentKeyed = !!(ld.parentKeys && ld.parentKeys.length);
    var parentKeyHere = parentKeyed && ld.parentKeys.some(function (k) { return k.frame === state.currentFrame; });
    var sw = document.createElement('div');
    sw.className = 'lico motion-stopwatch' + (parentKeyed ? ' on' : '');
    sw.title = stopwatchTitle('motionAnimateFill', parentKeyed, parentKeyHere);
    sw.innerHTML = '<svg viewBox="0 0 24 24" width="12" height="12"><path d="M12 3l9 9-9 9-9-9z" fill="' + (parentKeyHere ? 'currentColor' : 'none') + '" stroke="currentColor" stroke-width="2"/></svg>';
    sw.addEventListener('click', function (e) {
      e.stopPropagation(); pushUndo();
      if (!parentKeyed) {
        ld.parentKeys = [{ frame: state.currentFrame, uid: ld.parentLayerUid || null }];
      } else if (parentKeyHere) {
        if (ld.parentKeys.length === 1) {
          var fv = ld.parentKeys[0].uid;
          ld.parentLayerUid = fv || null;
          delete ld.parentKeys;
        } else {
          removeParentKeyAt(ld, state.currentFrame);
        }
      } else {
        upsertParentKeyAt(ld, state.currentFrame, layerParentUidAt(li, state.currentFrame));
      }
      renderLayerList(); renderTimeline();
      if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    });
    row.appendChild(sw);
    var label = document.createElement('span'); label.textContent = 'Parent'; label.style.minWidth = '70px';
    row.appendChild(label);

    // Same pill as the layer-list cell ("même ui que dans le calque",
    // buildParentCell/timeline.js) — shares its .lparent look and the exact
    // same context menu (buildParentMenuItems, timeline.js) so Parent A/B
    // can't drift between the two surfaces. Unconstrained width here (no
    // .lparent max-width): this row isn't as cramped as the layer list.
    // Parent A's own name is read through layerParentUidAt (feedback #207)
    // so a keyed layer's pill shows the parent RESOLVED AT THE PLAYHEAD,
    // not the (irrelevant once keyed) static field — Parent B has no
    // keying concept, so it stays a direct static read.
    var pIdx = _layerIndexByUid(layerParentUidAt(li, state.currentFrame));
    var pName = (pIdx >= 0 && state.layers[pIdx]) ? (state.layers[pIdx].name || ('Layer ' + (pIdx + 1))) : null;
    var pbIdx = _layerIndexByUid(ld.parentLayerUidB);
    var pbName = (pbIdx >= 0 && state.layers[pbIdx]) ? (state.layers[pbIdx].name || ('Layer ' + (pbIdx + 1))) : null;

    var pill = document.createElement('div');
    pill.className = 'lparent motion-parent-pill' + (pName ? '' : ' none') + (pbName ? ' blendable' : '');
    pill.title = pbName ? ('Parent A : ' + pName + '  +  Parent B : ' + pbName + '  —  glisser pour fondre, cliquer pour changer')
      : (pName ? ('Parent : ' + pName + ' — cliquer pour changer') : 'Aucun parent — cliquer pour en choisir un');

    // "Une barre bleu dedans pour fade le parent" — only meaningful once a
    // second parent exists (blendedAncestorMat itself no-ops without one).
    // Reads/writes the SAME parentBlend value as the generic keyframable
    // row further down this panel (propsFor includes it once parentLayerUidB
    // is set) — this bar is a fast spatial way to scrub the CURRENT frame's
    // value, not a replacement for keyframing it on the track.
    var fill = null;
    if (pbName) { fill = document.createElement('span'); fill.className = 'mp-fill'; pill.appendChild(fill); }

    var pick = document.createElement('span');
    pick.className = 'lpick';
    pick.title = SM.t('titleDragLayerSetParentA');
    pick.addEventListener('mousedown', function (e) { startParentPickwhip(li, pick, e); });
    pill.appendChild(pick);

    var lbl = document.createElement('span'); lbl.className = 'mp-label';
    lbl.textContent = pbName ? (pName || '—') + ' + ' + pbName : (pName || '—');
    pill.appendChild(lbl);

    function currentBlend() {
      var v = valueAtFrame(ld, 'parentBlend', state.currentFrame);
      return (v && typeof v[0] === 'number') ? v[0] : 0;
    }
    if (fill) fill.style.width = Math.max(0, Math.min(100, currentBlend())) + '%';

    // Click vs. drag disambiguated by movement threshold — same idiom as
    // .scrub numeric fields (ui.js): a plain click still opens the parent
    // menu below; only real horizontal movement writes to parentBlend.
    // Render is rAF-coalesced (§5's own rationale — a stylet fires far
    // faster than 60Hz, only the last position per frame matters).
    var moved = false, startX = 0, startVal = 0, pid = null, liveRaf = 0;
    function scheduleLiveRender() {
      if (liveRaf) return;
      liveRaf = requestAnimationFrame(function () { liveRaf = 0; if (window.SMEngineBridge) SMEngineBridge.renderNow(); });
    }
    pill.addEventListener('pointerdown', function (e) {
      if (!pbName) return; // nothing to blend without a second parent
      if (e.target.closest && e.target.closest('.lpick')) return; // pickwhip owns this gesture
      moved = false; startX = e.clientX; startVal = currentBlend(); pid = e.pointerId;
      pill.setPointerCapture(pid);
      e.preventDefault();
    });
    pill.addEventListener('pointermove', function (e) {
      if (pid === null || e.pointerId !== pid) return;
      var dx = e.clientX - startX;
      if (!moved) {
        if (Math.abs(dx) < 3) return;
        moved = true; pushUndo(); window._scrubLiveActive = true;
      }
      var rect = pill.getBoundingClientRect();
      var raw = Math.max(0, Math.min(100, startVal + (dx / Math.max(1, rect.width)) * 100));
      fill.style.width = raw + '%';
      setValue(ld, 'parentBlend', [raw]);
      scheduleLiveRender();
    });
    function endDrag(e) {
      if (pid === null || (e && e.pointerId !== undefined && e.pointerId !== pid)) return;
      pid = null;
      if (moved) { window._scrubLiveActive = false; renderLayerList(); renderTimeline(); }
    }
    pill.addEventListener('pointerup', endDrag);
    pill.addEventListener('pointercancel', endDrag);

    pill.addEventListener('mousedown', function (e) { e.stopPropagation(); });
    pill.addEventListener('click', function (e) {
      e.stopPropagation();
      if (moved) { moved = false; return; } // a drag just ended here — don't also open the menu
      if (!window.showContextMenu || !window.buildParentMenuItems) return;
      var items = window.buildParentMenuItems(li, ld, function () { renderLayerList(); renderTimeline(); });
      var r = pill.getBoundingClientRect();
      window.showContextMenu(r.left, r.bottom + 2, items);
    });
    // Right-click = instant full un-parent (2026-07-30, Cyril: "ça peut
    // être un raccourci ou clic droit sur les boutons de parent") — same
    // fast path as the layer-list's own pill (buildParentCell, timeline.js),
    // which this row is otherwise a styled duplicate of.
    pill.addEventListener('contextmenu', function (e) {
      e.stopPropagation(); e.preventDefault();
      if (!pName && !pbName) return;
      pushUndo();
      setLayerParent(li, null);
      if (window.setLayerParentB) setLayerParentB(li, null);
      renderLayerList(); renderTimeline();
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
      if (window.showToast) showToast(SM.t('toastParentRemoved'));
    });

    row.appendChild(pill);
    // "+" — a third parent and beyond (2026-08-30, "la possibilité
    // d'ajouter d'autres parent pareil que matte et animable"). Only once
    // BOTH A and B are set: the extras are weighted against the A/B pair,
    // so there is nothing to weight them against until that pair exists.
    if (pName && pbName) {
      var addP = document.createElement('button');
      addP.className = 'motion-expr-glob motion-parent-add';
      addP.textContent = '+';
      addP.title = SM.t('titleParentAddHint');
      addP.addEventListener('click', function (e) {
        e.preventDefault(); e.stopPropagation();
        var pick = firstFreeParentLayer(li, ld);
        if (pick == null) { if (window.showToast) showToast(SM.t('toastNoLayerLeftForParent')); return; }
        pushUndoLayers(true);
        ld.parentsMore = ld.parentsMore || [];
        ld.parentsMore.push({ uid: ensureLayerUid(state.layers[pick]) });
        renderLayerList(); renderTimeline();
        if (window.SMEngineBridge) SMEngineBridge.renderNow();
      });
      row.appendChild(addP);
    }
    body.appendChild(row);
    // One pill per extra parent. The WEIGHT is not here: it is an ordinary
    // keyframable property row in the Transform group below (propsFor),
    // so it gets the stopwatch, ease curves and expressions like anything
    // else — this row only says WHICH layer, matching how the Matte stack
    // splits "which layer" from its own animatable amount.
    if (pName && pbName && ld.parentsMore && ld.parentsMore.length) {
      ld.parentsMore.forEach(function (pe, pi) {
        var r2 = document.createElement('div'); r2.className = 'lrow motion-prop-row';
        var sp = document.createElement('span');
        sp.textContent = '+ Parent'; sp.style.minWidth = '70px'; sp.style.opacity = '.7';
        r2.appendChild(sp);
        var xi = _layerIndexByUid(pe.uid);
        var p2 = document.createElement('div');
        p2.className = 'lparent motion-parent-pill';
        var l2 = document.createElement('span'); l2.className = 'mp-label';
        l2.textContent = xi >= 0 ? (state.layers[xi].name || ('Layer ' + (xi + 1))) : '?';
        p2.appendChild(l2);
        p2.title = SM.t('titleExtraParentHint');
        p2.addEventListener('click', function (e) {
          e.preventDefault(); e.stopPropagation();
          openExtraParentMenu(e.clientX, e.clientY, li, ld, pi);
        });
        r2.appendChild(p2);
        body.appendChild(r2);
      });
    }
  }
  function firstFreeParentLayer(li, ld) {
    var used = {};
    if (ld.parentLayerUid) used[ld.parentLayerUid] = 1;
    if (ld.parentLayerUidB) used[ld.parentLayerUidB] = 1;
    (ld.parentsMore || []).forEach(function (p) { if (p && p.uid) used[p.uid] = 1; });
    for (var i = 0; i < state.layers.length; i++) {
      if (i === li) continue;
      var u = state.layers[i].layerUid;
      if (u && used[u]) continue;
      // Refuse anything that would close a loop, the same guard
      // setLayerParent applies to A — an extra parent is a real parent.
      if (u && wouldCreateParentCycle(ensureLayerUid(ld), u)) continue;
      return i;
    }
    return null;
  }
  function openExtraParentMenu(x, y, li, ld, pi) {
    if (!window.showContextMenu) return;
    var entry = (ld.parentsMore || [])[pi];
    if (!entry) return;
    var items = [];
    state.layers.forEach(function (o, oi) {
      if (oi === li) return;
      var u = ensureLayerUid(o);
      var bad = wouldCreateParentCycle(ensureLayerUid(ld), u);
      items.push({
        label: (o.name || ('Layer ' + (oi + 1))) + (entry.uid === u ? '  ✓' : ''),
        disabled: bad,
        action: function () {
          if (bad) return;
          pushUndoLayers(true);
          entry.uid = u;
          renderLayerList(); renderTimeline();
          if (window.SMEngineBridge) SMEngineBridge.renderNow();
        }
      });
    });
    items.push({ sep: true });
    items.push({ label: SM.t('menuRemoveThisParent'), action: function () {
      pushUndoLayers(true);
      ld.parentsMore.splice(pi, 1);
      // The weight tracks are indexed BY POSITION, so removing an entry
      // has to shift every later one down or they'd re-point at the wrong
      // parent — the same index-drift trap per-vertex vtxN keys have.
      var keys = ['motion', 'motionStatic'];
      keys.forEach(function (bag) {
        if (!ld[bag]) return;
        for (var i = pi; i < ld.parentsMore.length + 1; i++) {
          var here = parentWeightKeyFor(i), next = parentWeightKeyFor(i + 1);
          if (ld[bag][next] !== undefined) ld[bag][here] = ld[bag][next];
          else delete ld[bag][here];
        }
      });
      if (!ld.parentsMore.length) delete ld.parentsMore;
      renderLayerList(); renderTimeline();
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
    } });
    window.showContextMenu(x, y, items);
  }
  // ---- PARENT IN TIME (Van Dijk 2.1) ---------------------------------
  // The spatial Parent row's counterpart: instead of "whose transform do I
  // follow", "whose TIME do I follow". Same pickwhip idiom as parenting and
  // as the expression whip, dropped on another layer's row; the offsets are
  // plain frame fields (scrub-enabled per CLAUDE.md §10).
  function renderTimeLinkRow(body, ld, li) {
    var row = document.createElement('div'); row.className = 'lrow motion-prop-row';
    // 2026-08 fix: hardcoded French, shown regardless of locale.
    var label = document.createElement('span'); label.textContent = SM.t('fieldTimeLink'); label.style.minWidth = '70px';
    label.title = SM.t('titleTimeLinkRowHint');
    row.appendChild(label);

    var whip = document.createElement('span');
    whip.className = 'lpick';
    whip.title = SM.t('titleTimeLinkWhipHint');
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
    name.title = srcIdx >= 0 ? SM.t('titleTimeLinkNameHint') : SM.t('titleNoTimeLink');
    // Shared by both unlink gestures below — direct right-click AND the
    // click-then-menu path, so the two can never drift (2026-07-30, Cyril:
    // "il était impossible de désactiver le parent in time" — turned out
    // the click-then-menu path DID work, but the tiny name pill + 1-item
    // menu was an easy miss; right-click is the fast, hard-to-fumble path
    // he asked for, kept alongside the menu rather than replacing it).
    function unlinkTime() {
      pushUndo(); unlinkTimeLinkPreserveRange(ld);
      renderLayerList(); renderTimeline();
      if (window.loadFrame) loadFrame(state.currentFrame);
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
      if (window.showToast) showToast(SM.t('toastTimeLinkRemoved'));
    }
    name.addEventListener('contextmenu', function (e) {
      e.stopPropagation(); e.preventDefault();
      if (!ld.timeLink) return;
      unlinkTime();
    });
    // UI/UX audit (2026-07-30): used to delete the link on a bare click,
    // no confirmation — a real trap right next to the Parent row's pill
    // one line up, which opens a menu on click instead of acting
    // instantly. Now opens a one-item menu too, same idiom.
    name.addEventListener('click', function (e) {
      e.stopPropagation();
      if (!ld.timeLink) return;
      if (!window.showContextMenu) return;
      var r = name.getBoundingClientRect();
      window.showContextMenu(r.left, r.bottom + 2, [
        { label: SM.t('ctxUnlinkTime'), action: unlinkTime },
      ]);
    });
    row.appendChild(name);

    if (ld.timeLink) {
      // Which edges follow, and by how much.
      // UI/UX audit (2026-07-30): was a bare native <select> — the exact
      // same bug class as the Parent row's own pill, fixed earlier this
      // session (renderParentRow), reappearing one row below it. .psel is
      // the app-wide styled-dropdown class (style.css), used everywhere
      // else selects appear.
      var sel = document.createElement('select'); sel.className = 'psel'; sel.style.marginLeft = '6px';
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
      // The offsets themselves (timeLinkInOffset/timeLinkOutOffset) render
      // as their own generic property rows right after this one — propsFor
      // includes them whenever ld.timeLink is set (motion.js) — so they get
      // the expression (ƒx) button for free instead of a bespoke field
      // here. migrateTimeLinkOffsets (app.js) carries old projects'
      // ld.timeLink.inOffset/outOffset plain numbers over on first read.
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
  // The ONE place a time link is actually written (2026-07-31 extraction —
  // this used to live inline in startTimeLinkPickwhip's onUp; the new
  // menu-based creation path, timeline.js's buildTimeLinkMenuItems, calls
  // this same function instead of duplicating the seed/cycle logic — the
  // exact setLayerParent/buildParentMenuItems split spatial parenting
  // already uses). mode: 'both'|'in'|'out'. Returns true on success.
  // srcAnchor (2026-08-16, spec: "un in-point peut suivre un out-point...
  // enchaîner l'apparition d'un calque sur la disparition du précédent") —
  // which of the SOURCE's own edges to read from, when it differs from the
  // child's own edge type. Only meaningful for a single driven edge (mode
  // 'in'/'out'); 'both' always reads same-type on both sides — "my whole
  // range follows your single point" has no clean meaning, so a whole-
  // layer link stays same-type-only by design. Falls back to the child's
  // own mode (today's exact behavior) when unset/'whole'/invalid, so every
  // EXISTING call site (the side-panel Temps row, the menu-based creation
  // from buildTimeLinkMenuItems, a pickwhip drop that didn't land on a
  // specific target anchor) is 100% unaffected.
  // The standing keyframe lock (ld.keyLock, van Dijk's "Lock Keyframes to In
  // and Out Points"), as menu rows. Shared verbatim by the Motion layer row
  // and the keyframe cell (2026-08-30, "sur le clic droit des keyframes dans
  // motion j'aimerais ça pour le parent in time") so the two surfaces can
  // never drift in labels, order or check state — the same reason
  // buildTimeLinkMenuItems is one function feeding three menus.
  // Order stays In / Out / Layer, matching the surface that already shipped.
  // Every row toggles: picking the active mode again clears the lock, so the
  // group behaves like radio buttons that can also be switched fully off.
  function buildKeyLockMenuItems(li, ld) {
    function row(mode, key) {
      return {
        label: SM.t(key) + (ld.keyLock === mode ? '  ✓' : ''),
        action: function () { window.SM.setLayerKeyLock(li, ld.keyLock === mode ? null : mode); }
      };
    }
    return [
      // showContextMenu has no submenus — a disabled row is the honest way
      // to title a group rather than a button that does nothing.
      { label: SM.t('ctxLockKeyframesOnColon'), disabled: true, action: function () {} },
      row('in', 'ctxKeyLockInPoint'),
      row('out', 'ctxKeyLockOutPoint'),
      row('layer', 'ctxKeyLockWholeLayer')
    ];
  }
  // Per-KEYFRAME standing lock (feedback #212, "le lock in point, out point
  // et layer affecte toute les keyframes alors que ça devrait être les
  // keyframes select au clic droit") — reusing ld.keyLock's 3 labels above
  // was a deliberate earlier call (buildKeyLockMenuItems' own header
  // comment: "the keyframe selection itself is incidental context, confirmed
  // with Cyril"), but re-asked directly this time: the keyframe cell's own
  // menu should lock ONLY whatever's selected at the moment it's opened, not
  // the whole layer. Stored right on each key (key.lockTo, same footprint as
  // the existing key.hold flag) rather than on the layer, so several keys on
  // the same layer can each follow a DIFFERENT edge. Persists for free:
  // ld.motion/ld.elementMotion are written into exportJSON wholesale
  // (timeline.js), not field-by-field, so a new key field needs no separate
  // whitelist entry.
  function buildKeySelectionLockMenuItems() {
    if (!_motionKeySel.length) return [];
    function allLockedTo(mode) { return _motionKeySel.every(function (s) { return s.key && s.key.lockTo === mode; }); }
    function row(mode, key) {
      var active = allLockedTo(mode);
      return {
        label: SM.t(key) + (active ? '  ✓' : ''),
        action: function () {
          pushUndo();
          var next = active ? null : mode;
          _motionKeySel.forEach(function (s) { if (s.key) s.key.lockTo = next; });
          renderTimeline();
        }
      };
    }
    return [
      { label: SM.t('ctxLockKeyframesOnColon'), disabled: true, action: function () {} },
      row('in', 'ctxKeyLockInPoint'),
      row('out', 'ctxKeyLockOutPoint'),
      row('layer', 'ctxKeyLockWholeLayer')
    ];
  }
  // Every key across a layer's own track set (+ each element holder's own
  // tracks — CLAUDE.md §8's per-shape Motion) currently flagged to follow
  // `mode` ('in'/'out'/'layer'). Same holder/track reach as shiftLayerMotionKeys'
  // shiftHolder (propsFor, not the base PROPS — a 3D/duplicator/multi-parent
  // -blend/timeLink holder has tracks beyond the base 5), minus timeRemap/
  // effects: those aren't reachable through the same keyframe-cell menu this
  // lock is set from, so they're out of scope for it, unlike the whole-layer
  // lock which sweeps everything on purpose.
  function keysLockedTo(li, mode) {
    var ld = state.layers[li];
    if (!ld) return [];
    var out = [];
    function scan(h) {
      if (!h || !h.motion) return;
      propsFor(h).forEach(function (prop) {
        var t = h.motion[prop];
        if (!t || !t.keys) return;
        t.keys.forEach(function (k) { if (k.lockTo === mode) out.push({ holder: h, prop: prop, key: k }); });
      });
    }
    scan(ld);
    if (ld.elementMotion) Object.keys(ld.elementMotion).forEach(function (id) { scan(ld.elementMotion[id]); });
    return out;
  }
  function setLayerTimeLink(li, targetIdx, mode, srcAnchor) {
    var ld = state.layers[li], src = state.layers[targetIdx];
    if (!ld || !src || targetIdx === li) return false;
    if (timeLinkWouldCycle(li, targetIdx)) { if (window.showToast) showToast(SM.t('toastLinkImpossibleCycle')); return false; }
    pushUndo();
    mode = mode || 'both';
    var xType = (mode !== 'both' && (srcAnchor === 'in' || srcAnchor === 'out')) ? srcAnchor : mode;
    // Seed the offsets from the CURRENT gap, so linking never makes the
    // layer jump: it stays exactly where it is and only starts following.
    var myIn = layerInPoint(ld), myOut = layerOutPoint(ld);
    var srcInVal = layerInPoint(src), srcOutVal = layerOutPoint(src);
    var xVal = xType === 'out' ? srcOutVal : srcInVal;
    var seedInOff = myIn - (mode === 'in' ? xVal : srcInVal);
    var seedOutOff = myOut - (mode === 'out' ? xVal : srcOutVal);
    ld.timeLink = { uid: ensureLayerUid(src), mode: mode };
    if (mode !== 'both' && xType !== mode) ld.timeLink.srcAnchor = xType;
    // Offsets are Motion properties now (timeLinkInOffset/Out) — write
    // through setValue like any other, not a raw field on the link.
    setValue(ld, 'timeLinkInOffset', [seedInOff]);
    setValue(ld, 'timeLinkOutOffset', [seedOutOff]);
    renderLayerList(); renderTimeline();
    if (window.loadFrame) loadFrame(state.currentFrame);
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    var srcName = src.name || ('Layer ' + (targetIdx + 1));
    var msg;
    if (mode === 'both') msg = 'Temps lié à « ' + srcName + ' »';
    else if (xType !== mode) msg = (mode === 'in' ? 'Point d’entrée' : 'Point de sortie') + ' lié à ' + (xType === 'out' ? 'la sortie' : 'l’entrée') + ' de « ' + srcName + ' »';
    else msg = 'Temps lié à « ' + srcName + ' »' + (mode === 'in' ? ' (entrée)' : ' (sortie)');
    if (window.showToast) showToast(msg);
    return true;
  }
  // mode ('both'|'in'|'out') — which edge(s) the resulting link drives.
  // Defaults to 'both' for the original single side-panel pickwhip; the
  // on-timeline-bar connection points (layer-inout.js, Van Dijk 2.1's 3
  // anchor points — In Point/Out Point/whole Layer) pass 'in'/'out'
  // explicitly. Exported via SMMotion so layer-inout.js (outside this
  // file's IIFE) can call the SAME drag-line/cycle-check/link-creation
  // logic instead of a second implementation (CLAUDE.md §3).
  function startTimeLinkPickwhip(li, fromEl, ev, mode) {
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
      // Cross-type source detection (2026-08-16) — if the drop point landed
      // on one of the TARGET's own on-bar anchor dots specifically (not
      // just its bar/row generally), that anchor's type ('in'/'out') feeds
      // setLayerTimeLink's srcAnchor; landing on 'whole' or anywhere else
      // on the row leaves it unset, same-type default (today's behavior).
      var anchorEl = el.closest('.timelink-anchor');
      var srcAnchor = null;
      if (anchorEl && (anchorEl.classList.contains('in') || anchorEl.classList.contains('out'))) {
        srcAnchor = anchorEl.classList.contains('in') ? 'in' : 'out';
      }
      return { row: row, idx: idx, srcAnchor: srcAnchor };
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
      setLayerTimeLink(li, t.idx, mode, t.srcAnchor);
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
  // Panel half of transformRowPlan's dupHeader entry — same chevron idiom
  // as renderPathVertexGroup one level down. The grid half is one blank
  // .frow spacer (renderTimelineMotion), same header↔spacer pairing as
  // every other group header in this file.
  function renderDupGroupHeader(list, holder) {
    var grp = document.createElement('div'); grp.className = 'lrow motion-group-row';
    var arrow = document.createElement('span'); arrow.className = 'lico larrow'; arrow.textContent = isDupGroupExpanded(holder) ? '▾' : '▸';
    var label = document.createElement('span'); label.textContent = 'Duplicator';
    grp.title = SM.t('titleDuplicatorGroupHint');
    grp.appendChild(arrow); grp.appendChild(label);
    grp.addEventListener('click', function (e) {
      e.stopPropagation();
      window._motionExpandedDupHolder = isDupGroupExpanded(holder) ? null : holder;
      renderLayerList(); renderTimeline();
    });
    list.appendChild(grp);
  }
  function renderTransformProps(list, holder) {
    transformRowPlan(holder).forEach(function (entry) {
      if (entry.row === 'dupHeader') { renderDupGroupHeader(list, holder); return; }
      var prop = entry.prop;
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
      decorateMotionPropertyRow(pr, holder, prop, pnm);
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
      pr.appendChild(exprBtn); pr.appendChild(pnm);
      // Anchor Point 3x3 grid selector (2026-07-29, LottieFiles-inspired) —
      // one click snaps the anchor to a corner/edge/center of the holder's
      // OWN current bounds, instead of hand-computing ax/ay offsets. Only
      // meaningful for 'anchor' (the other properties have no such notion).
      if (prop === 'anchor') {
        var gridBtn = document.createElement('div');
        gridBtn.className = 'lico motion-anchor-grid-btn' + (window._anchorGridOpenFor === holder ? ' on' : '');
        gridBtn.title = 'Point d\'ancrage — grille rapide';
        gridBtn.innerHTML = '<svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" stroke-width="1.6"><rect x="3.5" y="3.5" width="17" height="17" rx="1.5"/><path d="M3.5 9.5h17M3.5 14.5h17M9.5 3.5v17M14.5 3.5v17"/></svg>';
        gridBtn.addEventListener('click', function (e) {
          e.stopPropagation();
          window._anchorGridOpenFor = (window._anchorGridOpenFor === holder) ? null : holder;
          renderLayerList(); renderTimeline();
        });
        pr.appendChild(gridBtn);
      }
      var vals = displayValueFor(holder, prop);
      var fieldWrap = document.createElement('div'); fieldWrap.className = 'motion-fields';
      var DIM_LABEL = PROP_DIM[prop] > 1 ? (PROP_DIM_LABELS[prop] || ['X', 'Y', 'Z']) : null;
      // Expression controls (2026-08-30): two of the five types want a
      // widget that isn't a number box — a checkbox reads as on/off and a
      // colour reads as a swatch. Everything BELOW the widget (the track,
      // the keys, the stopwatch on this same row, the grid mirror) is the
      // ordinary generic machinery, unchanged: only the input differs.
      // number/angle/point fall through to the scrub-field loop that every
      // other property already uses, so they inherit CLAUDE.md §10's
      // drag-to-scrub for free.
      var ctrlType = controlTypeOf(prop);
      // Shared commit for the two custom widgets — same
      // "write into the selected keys if there are any, otherwise through
      // setValue" contract the scrub fields below follow, so editing a
      // control behaves identically however it's rendered.
      function commitControlValue(nvals) {
        pushUndo();
        if (!setSelectedKeyVector(holder, prop, nvals)) setValue(holder, prop, nvals);
        renderLayerList(); renderTimeline();
        if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
      }
      if (ctrlType === 'checkbox') {
        var cbx = document.createElement('input');
        cbx.type = 'checkbox';
        cbx.className = 'motion-ctrl-check';
        // >= 0.5, not === 1: a checkbox control is a real keyframable track
        // like any other, so two keys 0 and 1 interpolate through the middle
        // — the display has to pick a side rather than pretend it can't
        // happen. (Hold keys, already available on every track, are how you
        // get a hard on/off switch.)
        cbx.checked = (Number(vals[0]) || 0) >= 0.5;
        cbx.addEventListener('click', function (e) { e.stopPropagation(); });
        cbx.addEventListener('change', function () { commitControlValue([cbx.checked ? 1 : 0]); });
        fieldWrap.appendChild(cbx);
      } else if (ctrlType === 'color') {
        var csw = document.createElement('div');
        csw.className = 'motion-ctrl-swatch';
        var crgba = vals;
        csw.style.background = 'rgba(' + Math.round(crgba[0] || 0) + ',' + Math.round(crgba[1] || 0) + ',' + Math.round(crgba[2] || 0) + ',' + ((crgba[3] !== undefined ? crgba[3] : 255) / 255) + ')';
        csw.title = SM.t('titleControlColorSwatch');
        csw.addEventListener('click', function (e) {
          e.stopPropagation();
          if (!window.openLayerColorSwatches) return;
          // Same picker popover every other colour row in this file opens.
          openLayerColorSwatches(csw, rgba255ToHex(crgba), function (hex) { commitControlValue(hexToRgba255(hex)); });
        });
        fieldWrap.appendChild(csw);
      } else for (var d = 0; d < PROP_DIM[prop]; d++) {
        (function (dim) {
          if (DIM_LABEL) {
            var dl = document.createElement('span'); dl.className = 'motion-dim-label'; dl.textContent = DIM_LABEL[dim];
            fieldWrap.appendChild(dl);
          }
          var display = selectedDimensionDisplay(holder, prop, dim, vals[dim]);
          var f = scrubField(display.value, function (nv, edit) {
            pushUndo();
            var changed = edit && edit.relative
              ? offsetSelectedKeyDimension(holder, prop, dim, edit.delta)
              : setSelectedKeyDimension(holder, prop, dim, nv);
            if (!changed) {
              var nvals = isAnimated(holder, prop) ? valueAtFrame(holder, prop, state.currentFrame) : staticValue(holder, prop);
              nvals[dim] = nv;
              setValue(holder, prop, nvals);
            }
            // Aspect-ratio lock (feedback #120) — carries THIS dimension's
            // multiplicative change onto the other one, same convention
            // After Effects' own Scale chain-link uses (a ratio, not an
            // identical additive delta — 100%→110% on X takes a 50% Y to
            // 55%, not to 60%). Reads the OTHER dimension's PRE-edit value
            // from `vals` (this row's own render-time snapshot, same
            // fallback `display.value` already uses) rather than
            // re-deriving it, so a mixed multi-selection locks onto the
            // same reference every dimension's display used.
            if (prop === 'scale' && PROP_DIM[prop] === 2 && _scaleLockedHolders.has(holder)) {
              var otherDim = dim === 0 ? 1 : 0;
              var beforeThis = Number(vals[dim]) || 0;
              var beforeOther = Number(vals[otherDim]) || 0;
              if (beforeThis !== 0) {
                var afterOther = beforeOther * (nv / beforeThis);
                var otherChanged = setSelectedKeyDimension(holder, prop, otherDim, afterOther);
                if (!otherChanged) {
                  var onvals = isAnimated(holder, prop) ? valueAtFrame(holder, prop, state.currentFrame) : staticValue(holder, prop);
                  onvals[otherDim] = afterOther;
                  setValue(holder, prop, onvals);
                }
              }
            }
            // renderLayerList too (not just the timeline, the original
            // single-panel behavior): these same rows now render in TWO
            // places (bottom Transform group + right-panel mirror,
            // renderMotionPropsPanel) — committing a value in one must
            // refresh the other's copy of the field, or they visibly
            // disagree until the next unrelated refresh.
            renderLayerList(); renderTimeline();
            reloadIfTimeLinkOffset(prop);
            if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
          }, display.mixed);
          fieldWrap.appendChild(f);
        })(d);
      }
      // Aspect-ratio lock toggle (feedback #120) — Scale only, right after
      // its X/Y fields so it reads as "these two are chained" rather than
      // a generic row control.
      if (prop === 'scale' && PROP_DIM[prop] === 2) {
        var lockBtn = document.createElement('div');
        lockBtn.className = 'lico motion-scale-lock' + (_scaleLockedHolders.has(holder) ? ' on' : '');
        lockBtn.title = 'Lier Scale X et Y (garder le ratio)';
        lockBtn.innerHTML = _scaleLockedHolders.has(holder)
          ? '<svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/></svg>'
          : '<svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 7.5-2"/></svg>';
        lockBtn.addEventListener('click', function (e) {
          e.stopPropagation();
          if (_scaleLockedHolders.has(holder)) _scaleLockedHolders.delete(holder);
          else _scaleLockedHolders.add(holder);
          renderLayerList(); renderTimeline();
        });
        fieldWrap.appendChild(lockBtn);
      }
      var unit = document.createElement('span'); unit.className = 'motion-unit'; unit.textContent = PROP_UNIT[prop];
      fieldWrap.appendChild(unit);
      pr.appendChild(fieldWrap);
      if (!PROP_NO_STOPWATCH[prop]) pr.appendChild(sw);
      list.appendChild(pr);
      if (window._exprEditorOpen && window._exprEditorOpen.holder === holder && window._exprEditorOpen.prop === prop) {
        list.appendChild(buildExprEditorRow(holder, prop));
      }
      if (prop === 'anchor' && window._anchorGridOpenFor === holder) {
        list.appendChild(buildAnchorGridRow(holder));
      }
    });
  }
  // Inline 3x3 anchor picker, opened via the anchor row's grid icon (same
  // single-accordion convention as the expression editor below) — each cell
  // snaps ax/ay to a corner/edge/center of the holder's CURRENT bounds
  // (activeMotionTarget's bounds, the same source the canvas gizmo/pivot
  // already use — see motionPivotOf). Guarded on t.holder===holder so a
  // stale click (holder no longer the active/expanded target) is a no-op
  // rather than silently writing into the wrong shape's anchor.
  function buildAnchorGridRow(holder) {
    var row = document.createElement('div'); row.className = 'lrow motion-anchor-grid-row';
    var grid = document.createElement('div'); grid.className = 'motion-anchor-grid';
    for (var r = 0; r < 3; r++) {
      for (var c = 0; c < 3; c++) {
        (function (row9, col9) {
          var cell = document.createElement('button'); cell.type = 'button';
          cell.className = 'motion-anchor-cell' + (row9 === 1 && col9 === 1 ? ' center' : '');
          cell.addEventListener('click', function (e) {
            e.stopPropagation();
            var t = activeMotionTarget();
            if (!t || t.holder !== holder || !t.bounds) return;
            pushUndo();
            var ax = (col9 - 1) * t.bounds.width / 2;
            var ay = (row9 - 1) * t.bounds.height / 2;
            setValue(holder, 'anchor', [ax, ay]);
            renderLayerList(); renderTimeline();
            if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
          });
          grid.appendChild(cell);
        })(r, c);
      }
    }
    row.appendChild(grid);
    return row;
  }
  // Inline expression editor — appended right after its property's row when
  // toggled open via the ƒx button, same single-accordion convention as
  // _motionExpandedLayer/_motionExpandedElement/_motionExpandedPathHolder
  // elsewhere in this file (only one open at a time, tracked on window so a
  // full re-render can restore which one).
  // Breadcrumb for the split editor's header — "Layer 1 › Shape 2 › Position".
  // The panel sits beside the canvas with nothing else naming its target, so
  // it has to say what it is editing; the inline row never needed this
  // because it is physically attached to the property it belongs to.
  function exprPanelLabelFor(holder, prop) {
    var r = resolveHolderLayer(holder);
    var parts = [];
    if (r && r.ld) parts.push(r.ld.name || 'Layer');
    // An element holder names its shape by position in the layer's element
    // list rather than by strokeId — the id is an internal token ("c17881…")
    // and would be noise in a header meant to orient you.
    if (r && r.strokeId && typeof layerElements === 'function') {
      var els = layerElements(r.ld) || [];
      for (var i = 0; i < els.length; i++) {
        if (els[i] && els[i].strokeId === r.strokeId) { parts.push('Shape ' + (i + 1)); break; }
      }
    }
    parts.push(PROP_LABEL[prop] || prop);
    return parts.join('  ›  ');
  }
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
      reloadIfTimeLinkOffset(prop);
      if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    });
    head.appendChild(cb);
    head.appendChild(document.createTextNode(' ' + SM.t('exprEnableLabel')));
    row.appendChild(head);
    // The value the property WOULD have without the expression (Van Dijk
    // 7.4). With an expression on, the field above shows the RESULT, and
    // the underlying keyframed value becomes invisible — you end up
    // disabling the expression just to see what you are driving.
    function fmtVals(arr) {
      return arr.map(function (n) { return Math.round(n * 100) / 100; }).join(', ') + ' ' + (PROP_UNIT[prop] || '');
    }
    var rawWrap = document.createElement('span');
    rawWrap.className = 'motion-expr-raw';
    var raw = rawValueAtFrame(holder, prop, state.currentFrame);
    rawWrap.textContent = SM.t('exprRawValuePrefix') + fmtVals(raw);
    rawWrap.title = SM.t('titleRawValueHint');
    row.appendChild(rawWrap);
    // ...and, right beside it, what the expression ACTUALLY produces at the
    // playhead. Writing code against a value you can't see is the slowest
    // way to work; this refreshes with the playhead like every other Motion
    // row does. When the expression is failing, this is where that shows.
    var outWrap = document.createElement('span');
    outWrap.className = 'motion-expr-raw motion-expr-out';
    outWrap.title = SM.t('titleExprResultHint');
    // Repainted in place rather than by re-rendering the row: commit() runs
    // while the textarea still has focus and a caret in it, and rebuilding
    // the panel there would throw both away mid-edit.
    function paintResult() {
      outWrap.classList.remove('err');
      if (!expr.enabled || !expr.code) { outWrap.textContent = SM.t('exprResultPrefix') + '—'; return; }
      var cur = rawValueAtFrame(holder, prop, state.currentFrame);
      var evaluated = evalExpressionFor(holder, prop, state.currentFrame, cur);
      if (evaluated === null) {
        outWrap.classList.add('err');
        outWrap.textContent = SM.t('exprResultPrefix') + SM.t('exprResultError');
        return;
      }
      outWrap.textContent = SM.t('exprResultPrefix') + fmtVals(evaluated);
    }
    paintResult();
    row.appendChild(outWrap);
    // Project-wide preamble (7.2) — reachable from the same place you write
    // the expression that uses it, rather than a settings panel away.
    var glob = document.createElement('button');
    glob.className = 'motion-expr-glob';
    glob.textContent = exprGlobals() ? SM.t('btnGlobalVarsOn') : SM.t('btnGlobalVarsEllipsis');
    glob.title = SM.t('titleGlobalVarsHint');
    glob.addEventListener('click', function (e) {
      e.preventDefault(); e.stopPropagation();
      var v = prompt(SM.t('promptGlobalVarsPrefix') + (state.fps || 24) + ';', exprGlobals());
      if (v === null) return;
      pushUndo();
      window.SMMotion.setExprGlobals(v);
    });
    row.appendChild(glob);
    // Examples menu (feedback #113, "les expressions ne sont pas encore
    // très clair à utilisé il faudrait un menu... qui permettent d'avoir
    // des exemples d'expression commune et basique à utiliser"). Every
    // snippet below uses ONLY names present in EXPR_PUBLIC_NAMES — Nemo's
    // own documented vocabulary, never the compatibility aliases — so the
    // menu is copy-pasteable by construction and also doubles as the
    // shortest tour of what the engine can do. Several use let/const and
    // arrow functions on purpose: this is ordinary modern JavaScript and
    // the menu should show that. Reuses window.showContextMenu (ui.js), the
    // same generic dropdown already used elsewhere off a plain button click.
    var examplesBtn = document.createElement('button');
    examplesBtn.className = 'motion-expr-glob';
    examplesBtn.textContent = SM.t('btnExamplesEllipsis');
    examplesBtn.title = SM.t('titleExamplesHint');
    examplesBtn.addEventListener('click', function (e) {
      e.preventDefault(); e.stopPropagation();
      if (!window.showContextMenu) return;
      var followProp = (prop === 'rotation' || prop === 'opacity' || prop === 'scale' || prop === 'anchor') ? prop : 'position';
      function insert(code) {
        ta.value = code;
        paintGutter();
        commit();
        ta.focus();
      }
      // `value` is a bare [x,y] ARRAY on a 2D property (position/anchor/
      // scale) — JS's `+` on an array string-concatenates instead of adding
      // per-axis, so a 1D-only `value + wiggle(2, 10)` form silently
      // produces an invalid result (feedback #134: that exact example,
      // inserted on Position, always hit "expression must return a number
      // or [x,y] array"). Every entry is therefore PROP_DIM-branched, so
      // the SAME menu item is always valid for the property it was invoked
      // on — wiggle() itself already returns [wx, wy] for a 2D property.
      var is2D = PROP_DIM[prop] === 2;
      function pair(one, two) { return is2D ? two : one; }
      // The cross-layer examples name real OTHER layers. Found live: with a
      // hardcoded 'Layer 1' placeholder, inserting one of them while editing
      // Layer 1 produced a self-reference that the cycle guard (correctly)
      // refused — an example that fails the moment you use it on the first
      // layer of the project is not an example.
      var otherA = 'Layer 1', otherB = 'Layer 2', picked = [];
      for (var oi = 0; oi < state.layers.length && picked.length < 2; oi++) {
        if (state.layers[oi] !== holder) picked.push(state.layers[oi].name);
      }
      if (picked.length > 0) otherA = picked[0];
      if (picked.length > 1) otherB = picked[1];
      // Entries that only make sense on one dimensionality are listed as
      // null above and dropped here, rather than shown doing nothing.
      window.showContextMenu(e.clientX, e.clientY, [
        { label: SM.t('exprExValue'), action: function () { insert('value'); } },
        { label: SM.t('exprExWiggle'), action: function () {
          insert(pair('value + wiggle(2, 10)',
            '[value[0] + wiggle(2, 10)[0], value[1] + wiggle(2, 10)[1]]'));
        } },
        { label: SM.t('exprExOscillate'), action: function () {
          insert(pair('value + Math.sin(time * 3) * 10',
            'const w = Math.sin(time * 3) * 10;\nreturn [value[0] + w, value[1] + w];'));
        } },
        { label: SM.t('exprExStepped'), action: function () {
          insert(pair('stepTime(4);\nreturn value + wiggle(3, 15);',
            'stepTime(4);\nconst w = wiggle(3, 15);\nreturn [value[0] + w[0], value[1] + w[1]];'));
        } },
        { label: SM.t('exprExLoopCycle'), action: function () { insert('loopAfter(\'cycle\')'); } },
        { label: SM.t('exprExLoopPingpong'), action: function () { insert('loopAfter(\'pingpong\')'); } },
        { label: SM.t('exprExLoopOffset'), action: function () { insert('loopAfter(\'offset\')'); } },
        { label: SM.t('exprExLoopBefore'), action: function () { insert('loopBefore(\'cycle\')'); } },
        { label: SM.t('exprExBounce'), action: function () {
          var head = 'const n = self.keys.count;\n'
            + 'if (!n) return value;\n'
            + 'const t = (frame - self.keys.at(n).frame) / comp.fps;\n'
            + 'if (t <= 0) return value;\n'
            + 'const b = 40 * Math.exp(-4 * t) * Math.sin(3 * 2 * Math.PI * t);\n';
          insert(head + pair('return value + b;', 'return [value[0], value[1] + b];'));
        } },
        { label: SM.t('exprExRemapOther'), action: function () {
          insert(pair('remap(layer(\'' + otherA + '\').position[0], 0, comp.width, 0, 100)',
            'const k = remap(layer(\'' + otherA + '\').position[0], 0, comp.width, 0, 1);\n'
            + 'return [value[0], value[1] + k * 100];'));
        } },
        { label: SM.t('exprExStagger'), action: function () {
          insert('seed(self.index);\n' + pair('return value + randomFixed(-20, 20);',
            'const o = randomFixed(-20, 20);\nreturn [value[0] + o, value[1] + o];'));
        } },
        { label: SM.t('exprExContentBox'), action: function () {
          insert(pair('contentBox().width + 20',
            'const b = contentBox();\nreturn [b.width + 20, b.height + 20];'));
        } },
        // angleTo returns ONE angle, so it only has a meaning on a
        // 1-dimensional property. Offered there and nowhere else, rather
        // than padded into a two-component shape that would do nothing.
        (is2D ? null : { label: SM.t('exprExAngleTo'), action: function () {
          insert('angleTo(layer(\'' + otherA + '\').position, layer(\'' + otherB + '\').position)');
        } }),
        { label: SM.t('exprExFollowLayer') + ' · ' + followProp, action: function () { insert('layer(\'' + otherA + '\').' + followProp); } },
        { label: SM.t('exprExFirstKey'), action: function () { insert('self.keys.count ? self.keys.at(1).value : value'); } },
        // Expression controls (2026-08-30) — offered only when this layer
        // actually has one, and naming the FIRST real control rather than a
        // placeholder, for the same reason the cross-layer examples above
        // name real other layers: an example that errors the moment you use
        // it is not an example. The owning layer is resolved through
        // resolveHolderLayer so this works from a per-shape row too, where
        // `holder` is an element holder and the controls live one level up.
        (function () {
          var ownerRes = resolveHolderLayer(holder);
          var ownerLd = ownerRes ? ownerRes.ld : null;
          var first = controlsOf(ownerLd)[0];
          if (!first) return null;
          return { label: SM.t('exprExControl') + ' · ' + first.name, action: function () {
            var read = 'self.control(' + JSON.stringify(first.name) + ')';
            // A point/colour control hands back an array; adding it to a
            // scalar property (or to a bare `value`) would string-concat
            // rather than add, the exact trap feedback #134 was about — so
            // multi-dimension controls are inserted on their own line.
            var multi = CONTROL_DIM[first.type] > 1;
            if (multi) { insert(read); return; }
            insert(pair('value + ' + read, 'const c = ' + read + ';\nreturn [value[0] + c, value[1] + c];'));
          } };
        })(),
      ].filter(Boolean));
    });
    row.appendChild(examplesBtn);
    // Open the same expression in the split code editor beside the canvas
    // (2026-08-30, "un code editor window qui split la zone du canvas...
    // pouvoir le fermer où l'ouvrir depuis un bouton dans la zone
    // d'expression existante"). The inline box stays exactly as it is —
    // this is a second VIEW of one expression, never a second copy: both
    // go through SMMotion.applyExprCode. Toggles, so the same button also
    // closes a panel already showing this property.
    if (window.SMExprPanel) {
      var popBtn = document.createElement('button');
      popBtn.className = 'motion-expr-glob motion-expr-pop';
      var ref = holderRefOf(holder);
      var showing = SMExprPanel.isShowing(ref, prop);
      popBtn.textContent = showing ? SM.t('btnCloseCodeEditor') : SM.t('btnOpenCodeEditor');
      popBtn.title = SM.t('titleCodeEditorHint');
      popBtn.addEventListener('click', function (e) {
        e.preventDefault(); e.stopPropagation();
        commit(); // don't lose an uncommitted edit when the panel takes over
        if (SMExprPanel.isShowing(ref, prop)) SMExprPanel.close();
        else SMExprPanel.open(ref, prop, exprPanelLabelFor(holder, prop));
        renderLayerList();
      });
      row.appendChild(popBtn);
    }
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
    // Placeholder mirrors the "Exemples" menu's own PROP_DIM branch below —
    // `value` is a bare [x,y] array on a 2D property, so the 1D-only form
    // would be just as invalid to copy from here as it was from the menu
    // (feedback #134).
    ta.placeholder = PROP_DIM[prop] === 2 ? '[value[0] + wiggle(2, 10)[0], value[1] + wiggle(2, 10)[1]]' : 'value + wiggle(2, 10)';
    // Discoverability (2026-08-16, Cyril: "agrémenter la library
    // d'expressions... ID keyframes et layers") — the vocabulary exists with
    // zero UI surface telling anyone it's callable; a hover tooltip on the
    // one place you're already looking beats a separate docs page nobody
    // opens. Documents Nemo's own names only, never the compatibility
    // aliases.
    ta.title = SM.t('titleExprCodeHint');
    pane.appendChild(gutter); pane.appendChild(ta);
    // The line number the error points at. The engine computes it (against
    // the user's own text, after subtracting the wrapper and preamble lines
    // — see _lineFromStack) and stores it on the expression, so the gutter
    // reads a number rather than trying to parse it back out of a message
    // that is translated into four languages.
    function errorLine() {
      if (!expr.lastError) return -1;
      if (typeof expr.errorLine === 'number' && expr.errorLine > 0) return expr.errorLine;
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
      expr.errorLine = -1;
      if (holder._exprCompiled) delete holder._exprCompiled[prop];
      if (typeof saveActiveLayerFrame === 'function') saveActiveLayerFrame();
      reloadIfTimeLinkOffset(prop);
      if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
      // Show what the new code produces immediately, and repaint the gutter
      // and the message in case the error moved or went away.
      paintResult();
      paintError();
      paintGutter();
    }
    function paintError() {
      errEl.textContent = expr.lastError || '';
      errEl.style.display = expr.lastError ? '' : 'none';
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
    grip.title = SM.t('titleResizeEditorHint');
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
    // Always built, shown only when there is something to say — so commit()
    // can surface a fresh message without re-rendering the row out from
    // under the caret.
    var errEl = document.createElement('div');
    errEl.className = 'motion-expr-error';
    row.appendChild(errEl);
    paintError();
    // ---- pickwhip (Van Dijk 7.3) -------------------------------------
    // Drag onto another property row to write its reference into the code;
    // Alt-drag CLONES that property's own expression instead — his exact
    // request ("clone the Expression itself rather than the value it
    // returns"), which is what stops duplicated code from having to be
    // updated in N places.
    var whip = document.createElement('span');
    whip.className = 'motion-expr-whip';
    whip.title = SM.t('titleExprWhipHint');
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
        if (!srcEx || !srcEx.code) { if (window.showToast) showToast(SM.t('toastNoExpressionToClone')); return; }
        text = srcEx.code;
      } else {
        var li = state.layers.indexOf(t.holder);
        // A per-element holder isn't in state.layers — reference its OWNER
        // layer, which is what the sandbox's layer() can actually resolve.
        if (li < 0) state.layers.forEach(function (ld2, i2) {
          if (ld2.elementMotion) Object.keys(ld2.elementMotion).forEach(function (k) { if (ld2.elementMotion[k] === t.holder) li = i2; });
        });
        if (li < 0) { if (window.showToast) showToast(SM.t('toastPropertyNotReferenceable')); return; }
        // An expression control's PROPERTY key (xc_…) is an internal id the
        // sandbox has no name for — the vocabulary reaches a control by the
        // name shown on the row. Wiring a rig by dragging onto a control row
        // is how these are meant to be used at all, so the whip has to emit
        // the form that actually resolves, not the raw key. JSON.stringify
        // for the name so a quote or backslash in it can't break the code it
        // is being pasted into.
        var ctrlDef = null;
        if (controlTypeOf(t.prop)) {
          controlsOf(t.holder).forEach(function (c) { if (c.key === t.prop) ctrlDef = c; });
        }
        // A control row emits layerControl(uid, name), not
        // layer(uid).control(name): identical value, without building a
        // layerSnapshot per read (see exprLayerControl). Dragging onto a
        // control row is exactly the rig case, i.e. the hot one.
        text = ctrlDef
          ? 'layerControl(' + JSON.stringify(ensureLayerUid(state.layers[li])) + ', ' + JSON.stringify(ctrlDef.name) + ')'
          : 'layer("' + ensureLayerUid(state.layers[li]) + '").' + t.prop;
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
      if (window.showToast) showToast(alt ? SM.t('toastExpressionCloned') : SM.t('toastReferenceInserted'));
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
    // Linked-fill companion (2026-08, "quand je dessine avec brush il me
    // fait 2 forme pour le fill et le stroke") — a stroke drawn with Fill
    // enabled gets a SEPARATE Path for the fill backdrop, tagged
    // isLinkedFillCompanion + a shared linkedFillId (draw-bridge.js
    // commitStroke). That file's own comment on the flag is explicit: "This
    // flag is how selectedPaths building code excludes it from ever being
    // added as its own entry" — layerElements was the one consumer that
    // still listed it as one, same family-of-bug-#1 shape as the
    // isBrushTextureCopy fold below. Indexed here so the owning stroke can
    // borrow its fillColor for display and shapes-panel.js can resolve the
    // real companion item when "Fill" gets clicked/dragged separately.
    var companionByLinkId = {};
    strokes.forEach(function (sd, ci) {
      if (!sd.isLinkedFillCompanion || !sd.linkedFillId) return;
      // Same lazy-stamp as the main loop below — a companion is skipped
      // BEFORE reaching that code, so it needs its own stamp here or
      // __linkedFillStrokeId would point nowhere on a legacy stroke drawn
      // before strokeId existed.
      if (!sd.strokeId) {
        sd.strokeId = 's' + Date.now().toString(36) + '_' + ci + '_' + Math.floor(Math.random() * 1e6);
        var cLiveLayer = window.userLayers && userLayers[li];
        var cLiveItem = cLiveLayer && cLiveLayer.children[ci];
        if (cLiveItem && cLiveItem.data && !cLiveItem.data.strokeId) cLiveItem.data.strokeId = sd.strokeId;
      }
      companionByLinkId[sd.linkedFillId] = sd;
    });
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
      if (sd.isLinkedFillCompanion) return; // folded into its owning stroke below
      // Lazily stamp a strokeId onto legacy stroke data that predates this
      // feature (or fillWalls/team-review, the other lazy-assign consumers)
      // — getEffectiveStrokes returns the LIVE array reference for a real
      // keyframe (not a clone), so this mutation persists on next save,
      // same lazy-assign contract ensureStrokeId already has for live Paper
      // items (tools.js).
      var isNewStamp = !sd.strokeId;
      if (isNewStamp) sd.strokeId = 's' + Date.now().toString(36) + '_' + i + '_' + Math.floor(Math.random() * 1e6);
      // Mirror the fresh stamp onto the LIVE Paper item too (2026-07-31,
      // group/shape tree panel's click-to-select) — found live: the dict
      // and the live canvas item are two SEPARATE representations that only
      // sync via desP (dict->live, on the NEXT loadFrame) or serP (live-
      // >dict, on save). Stamping only the dict left every freshly-drawn
      // shape's live item with data.strokeId===null until some unrelated
      // later reload happened to reconstruct it from the now-stamped dict —
      // liveItemByStrokeId's lookup silently failed in the meantime. `i`
      // lines up positionally with userLayers[li].children BEFORE the
      // isBrushTextureCopy filter above (both walk the same stored array in
      // the same order for an ordinary layer — CLAUDE.md §5quater's own
      // children.length===strokes.length identity contract).
      if (isNewStamp) {
        var liveLayer = window.userLayers && userLayers[li];
        var liveItem = liveLayer && liveLayer.children[i];
        if (liveItem && liveItem.data && !liveItem.data.strokeId) liveItem.data.strokeId = sd.strokeId;
      }
      var outSd = sd;
      var companion = sd.linkedFillId && companionByLinkId[sd.linkedFillId];
      if (companion) {
        // Shallow merged VIEW only — never mutates the stored stroke dict.
        // fillColor borrowed from the real companion so the swatch/label
        // reflect it; __linkedFillStrokeId lets a caller resolve the real
        // separate live item when Fill needs its own selection/reorder
        // (see shapes-panel.js's buildPaintSubRow).
        outSd = {};
        for (var k in sd) if (Object.prototype.hasOwnProperty.call(sd, k)) outSd[k] = sd[k];
        // feedback #219 ("stroke n'a pas sa couleur affichée dans le
        // canvas (même couleur que fill)"): this overwrite is exactly why
        // — for a vector-brush anchor, sd.fillColor IS the ink (see #203),
        // so once it's replaced with the companion's fill here, the Stroke
        // row's own default-swatch read (entry.sd.fillColor, engine-bridge.js's
        // isBrushInk branch's ink source) silently started reading the
        // FILL companion's color instead of the anchor's own — both rows
        // then default-displayed and (before the user touched Stroke)
        // rendered the same color. __inkColor preserves the anchor's real
        // pre-merge value so the Stroke row can read THAT instead.
        outSd.__inkColor = sd.fillColor;
        outSd.fillColor = companion.fillColor;
        outSd.__linkedFillStrokeId = companion.strokeId;
      }
      out.push({ strokeId: sd.strokeId, sd: outSd });
    });
    return out;
  }
  function elementLabel(entry, idx, ld) {
    var sd = entry.sd;
    // Custom name (2026-07-31, group/shape tree panel) — ld.shapeNames is
    // keyed by strokeId, same identity ensureStrokeId already gives every
    // shape; falls back to the existing auto-generated text when unset.
    if (ld && ld.shapeNames && ld.shapeNames[entry.strokeId]) return ld.shapeNames[entry.strokeId];
    if (sd.isRaster) return 'Image ' + (idx + 1);
    // 2026-08 fix: hardcoded French leaking into every locale ("Forme 1"
    // shown even with English selected) — i18n keys, not a literal.
    return SM.t(sd.fillColor ? 'autoNameShape' : 'autoNameStroke') + ' ' + (idx + 1);
  }
  // Group/shape tree (2026-07-31, Cyril: "vrai panel de gestion de group et
  // shape layer") — flat, z-ordered wrapper around layerElements() that
  // nests shapes sharing a Cmd+G data.groupId (group-bridge.js) under a
  // named {type:'group'} header, emitted once at the group's FIRST
  // occurrence in z-order. Mode-agnostic (dict-based, same as
  // layerElements) — the same function backs both Motion's renderElementsList
  // AND Animation 2D's forthcoming layer-row shape list, so the two can
  // never diverge in which shapes/groups exist or in what order.
  function buildShapeTree(li, ld) {
    var flat = layerElements(li, ld);
    var out = [], emittedGroups = {};
    flat.forEach(function (entry) {
      // Feedback #143 ("toujours pas de keyframes de properties par rapport
      // au animation de texte dans la timeline motion"): vector-text glyphs
      // (vector-text-bridge.js) share their run's data.groupId — the SAME
      // field a normal Cmd+G/Combine group uses to opaquely collapse its
      // members into one summary row below. That collapse is right for an
      // ordinary group (you usually want to animate the whole thing
      // together), but wrong for text: text-animator.js's whole point is a
      // per-letter/word/line STAGGER, each unit with its OWN keyframes, and
      // collapsing them left nothing to see or edit here even though the
      // keys are real and the animation renders correctly. Raster-split
      // characters never had this problem — splitTextIntoCharacters tags
      // them with data.textGroupId, a DIFFERENT field this collapse never
      // checked, so they already listed individually. Vector text now gets
      // the same treatment: skip the group-collapse, list each glyph as its
      // own expandable element row, exactly like every other ungrouped shape.
      var gid = entry.sd.isVectorText ? null : entry.sd.groupId;
      if (gid) {
        if (!emittedGroups[gid]) {
          emittedGroups[gid] = true;
          var meta = ld.groups && ld.groups[gid];
          out.push({ type: 'group', gid: gid, name: (meta && meta.name) || SM.t('autoNameGroup') });
        }
      } else {
        out.push({ type: 'shape', strokeId: entry.strokeId, sd: entry.sd });
      }
    });
    return out;
  }
  // Resolves a strokeId back to its LIVE Paper item on layer `li` — the
  // panel operates on dicts (getEffectiveStrokes), but click-to-select and
  // delete both need the real canvas object. null for a strokeId that
  // doesn't currently resolve (e.g. mid-frame-transition edge case).
  function liveItemByStrokeId(li, strokeId) {
    var layer = window.userLayers && userLayers[li]; if (!layer) return null;
    for (var i = 0; i < layer.children.length; i++) {
      var d = layer.children[i].data;
      if (d && d.strokeId === strokeId) return layer.children[i];
    }
    // Positional fallback (2026-07-31): a strokeId stamped onto the stored
    // dict BEFORE this dual-stamp fix existed (an older session/save) never
    // reached the live item — layerElements's own iteration order matches
    // userLayers[li].children's for an ordinary layer, so resolve by the
    // dict's position among non-brush-texture-companion strokes and, if
    // found, stamp the live item so future lookups hit the fast path above.
    var ld = state.layers[li];
    var strokes = ld ? (getEffectiveStrokes(li, state.currentFrame) || []) : [];
    var pos = -1;
    for (var si = 0, seen = 0; si < strokes.length; si++) {
      if (strokes[si].isBrushTextureCopy) continue;
      if (strokes[si].strokeId === strokeId) { pos = seen; break; }
      seen++;
    }
    if (pos >= 0 && layer.children[pos]) {
      var item = layer.children[pos];
      if (item.data && !item.data.strokeId) item.data.strokeId = strokeId;
      return item;
    }
    return null;
  }
  // Click-to-select (2026-07-31 — the previous Éléments list only ever
  // toggled the row's OWN expand state, never touched selectedPaths at
  // all: "manage without going through canvas-only interaction" needs the
  // row to actually select). strokeIds is an array so a group header can
  // select every member at once.
  function selectShapesByStrokeIds(li, strokeIds) {
    var items = strokeIds.map(function (sid) { return liveItemByStrokeId(li, sid); }).filter(Boolean);
    if (!items.length) return;
    window.SM.setActiveLayer(li);
    selectedPaths = items;
    // feedback #202, "si je select une shape dans elements ça doit être les
    // properties de cette shape qui s'affiche dans layer properties" —
    // renderMotionPropsPanel's "targeted shape" branch (2026-08-30, #173)
    // only ever reads window._motionExpandedElement/_motionExpandedLayer,
    // which the LEFT-panel element row already sets itself right after this
    // call (see its own line below) — but every OTHER caller (the RIGHT-
    // panel shapes list in shapes-panel.js chief among them, plus the
    // context-menu "Select" entries and the canvas resolve-to-anchor paths
    // in select-bridge.js/tools.js) only ever called this shared selector
    // and never touched those two globals, so Layer Properties kept showing
    // the LAYER's own Transform no matter which single shape got picked.
    // Scoped to an unambiguous SINGLE target — a multi-shape pick (group
    // members) has no one shape's properties to show, so it's left alone.
    if (strokeIds.length === 1) {
      window._motionExpandedLayer = li;
      window._motionExpandedElement = strokeIds[0];
    }
    // 2026-08 fix, "je select une forme dans le groupe... si j'essaie de
    // bouger/transformer la forme dans le canvas alors ça select le groupe
    // et pas la forme": select-bridge.js's onDown widens ANY click/drag on
    // an already-selected group member back to the whole group (built for
    // recovering full-group selection after a Subselect edit leaves one
    // member selected — see its own comments) — indistinguishable from this
    // panel selection by state alone (both leave selectedPaths as exactly
    // [that one shape]). This one-shot flag marks "the narrowing was
    // deliberate", consumed by onDown's very next mousedown so only that
    // first canvas interaction after a panel pick is protected; a normal
    // click on the shape afterward re-widens to the group as usual.
    window._skipGroupWidenOnce = true;
    if (window.state) state.selectedStrokeIndices = items.map(function (it) { return typeof getSI === 'function' ? getSI(it) : -1; }).filter(function (i2) { return i2 >= 0; });
    if (window.renderArcs) renderArcs();
    if (window.updateUI) updateUI();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  // Inline rename for a shape or a group header — same input-swap idiom as
  // timeline.js's startLayerRename, targeting this row's own .lnm instead.
  function startShapeTreeRename(rowEl, currentName, commit) {
    var nm = rowEl.querySelector('.lnm'); if (!nm) return;
    var input = document.createElement('input'); input.type = 'text'; input.value = currentName;
    input.style.cssText = 'width:100%;background:var(--bg);border:1px solid var(--accent);color:var(--text);font-size:11px;border-radius:4px;padding:1px 4px;outline:none;';
    nm.innerHTML = ''; nm.appendChild(input); input.focus(); input.select();
    var done = false;
    function finish() { if (done) return; done = true; var v = input.value.trim(); if (v) commit(v); else { renderLayerList(); renderTimeline(); } }
    input.addEventListener('keydown', function (e) { e.stopPropagation(); if (e.key === 'Enter') finish(); else if (e.key === 'Escape') { done = true; renderLayerList(); renderTimeline(); } });
    input.addEventListener('blur', finish);
    input.addEventListener('mousedown', function (e) { e.stopPropagation(); });
    input.addEventListener('dblclick', function (e) { e.stopPropagation(); });
  }
  function renderElementsList(list, li, ld) {
    var tree = buildShapeTree(li, ld);
    if (!tree.length) return;
    // 2026-08 fix: hardcoded French header, shown regardless of locale.
    var hdr = document.createElement('div'); hdr.className = 'lrow motion-group-row'; hdr.textContent = SM.t('hdrElements');
    list.appendChild(hdr);
    var shapeIdx = 0;
    tree.forEach(function (node) {
      if (node.type === 'group') {
        var grow = document.createElement('div'); grow.className = 'lrow motion-elem-row motion-elem-group';
        var gswatch = document.createElement('div'); gswatch.className = 'motion-elem-swatch'; gswatch.textContent = '▤'; gswatch.style.background = 'transparent';
        var gnm = document.createElement('div'); gnm.className = 'lnm'; gnm.textContent = node.name;
        grow.appendChild(gswatch); grow.appendChild(gnm);
        // Recompute this group's own member strokeIds from the flat list
        // (layerElements), not the already-collapsed tree — click-select
        // and rename both need the full membership, not just "a group
        // exists here".
        var memberIds = layerElements(li, ld).filter(function (e) { return e.sd.groupId === node.gid; }).map(function (e) { return e.strokeId; });
        function commitGroupRename(v) {
          pushUndo();
          // SMGroup.renameGroup (group-bridge.js) — creates ld.groups[gid]
          // on demand if this is a plain (non-combine) group's FIRST rename,
          // same shared setter groupSelection itself now seeds at creation.
          if (window.SMGroup && SMGroup.renameGroup) SMGroup.renameGroup(node.gid, ld, v, memberIds);
          saveActiveLayerFrame(); renderLayerList(); renderTimeline();
        }
        grow.addEventListener('click', function () { selectShapesByStrokeIds(li, memberIds); });
        grow.addEventListener('dblclick', function (e) { e.stopPropagation(); startShapeTreeRename(grow, node.name, commitGroupRename); });
        grow.addEventListener('contextmenu', function (e) {
          e.preventDefault(); e.stopPropagation();
          if (!window.showContextMenu) return;
          // 2026-08 fix: hardcoded French context-menu labels, shown
          // regardless of locale — reuses the same elements* keys
          // shapes-panel.js's own equivalent menu already relies on.
          window.showContextMenu(e.clientX, e.clientY, [
            { label: SM.t('elementsRename'), action: function () { startShapeTreeRename(grow, node.name, commitGroupRename); } },
            { label: SM.t('elementsSelectMembers'), action: function () { selectShapesByStrokeIds(li, memberIds); } },
            { label: SM.t('elementsUngroup'), action: function () {
              pushUndo();
              memberIds.forEach(function (sid) { var it = liveItemByStrokeId(li, sid); if (it && it.data) delete it.data.groupId; });
              if (ld.groups) delete ld.groups[node.gid];
              saveActiveLayerFrame(); renderLayerList(); renderTimeline();
              if (window.SMEngineBridge) SMEngineBridge.renderNow();
            } },
          ]);
        });
        list.appendChild(grow);
        return;
      }
      var entry = { strokeId: node.strokeId, sd: node.sd };
      var idx = shapeIdx++;
      var expanded = window._motionExpandedElement === entry.strokeId;
      var row = document.createElement('div'); row.className = 'lrow motion-elem-row';
      var swatch = document.createElement('div'); swatch.className = 'motion-elem-swatch';
      swatch.style.background = entry.sd.fillColor || entry.sd.strokeColor || 'transparent';
      if (elementHasMotion(ld, entry.strokeId)) swatch.classList.add('has-motion');
      var arrow = document.createElement('div'); arrow.className = 'lico larrow'; arrow.textContent = expanded ? '▾' : '▸';
      var nm = document.createElement('div'); nm.className = 'lnm'; nm.textContent = elementLabel(entry, idx, ld);
      row.appendChild(arrow); row.appendChild(swatch); row.appendChild(nm);
      row.addEventListener('click', function () {
        selectShapesByStrokeIds(li, [entry.strokeId]);
        window._motionExpandedElement = expanded ? null : entry.strokeId;
        renderLayerList(); renderTimeline();
      });
      row.addEventListener('dblclick', function (e) {
        e.stopPropagation();
        startShapeTreeRename(row, elementLabel(entry, idx, ld), function (v) {
          pushUndo();
          if (!ld.shapeNames) ld.shapeNames = {};
          ld.shapeNames[entry.strokeId] = v;
          saveActiveLayerFrame(); renderLayerList(); renderTimeline();
        });
      });
      row.addEventListener('contextmenu', function (e) {
        e.preventDefault(); e.stopPropagation();
        if (!window.showContextMenu) return;
        // 2026-08 fix: hardcoded French context-menu labels (same as the
        // group row's own menu right above).
        window.showContextMenu(e.clientX, e.clientY, [
          { label: SM.t('elementsRename'), action: function () { startShapeTreeRename(row, elementLabel(entry, idx, ld), function (v) { pushUndo(); if (!ld.shapeNames) ld.shapeNames = {}; ld.shapeNames[entry.strokeId] = v; saveActiveLayerFrame(); renderLayerList(); renderTimeline(); }); } },
          { label: SM.t('elementsSelect'), action: function () { selectShapesByStrokeIds(li, [entry.strokeId]); } },
          { label: SM.t('elementsDelete'), action: function () {
            var item = liveItemByStrokeId(li, entry.strokeId);
            if (!item) return;
            pushUndo();
            // Stale-selection guard (2026-07-31, found live via screenshot):
            // if this shape was the current canvas selection, removing it
            // without clearing selectedPaths left a detached (.parent===
            // null) reference behind — same "reconstruction leaves a ghost
            // selection" bug shape goToFrame's own clearSel fix (2026-07-29
            // QA sweep) already exists for elsewhere in this codebase.
            if (window.selectedPaths && selectedPaths.indexOf(item) >= 0 && window.clearSel) clearSel(true);
            item.remove();
            if (window._motionExpandedElement === entry.strokeId) window._motionExpandedElement = null;
            saveActiveLayerFrame(); renderLayerList(); renderTimeline();
            if (window.renderArcs) renderArcs();
            if (window.SMEngineBridge) SMEngineBridge.renderNow();
          } },
        ]);
      });
      list.appendChild(row);
      if (!expanded) return;
      // 2026-08 fix: hardcoded French group header.
      renderTransformGroup(list, ensureElementHolder(ld, entry.strokeId), SM.t('hdrTransformElement'));
      // Path property (2026-07): opt-in extended property, hidden unless
      // the element actually has vertex geometry (a Raster/image entry
      // never does) — same "hidden by default, opt-in" convention CLAUDE.md
      // §8 documents for fill/stroke/brush extended properties.
      if (!entry.sd.isRaster && pathVertexRowCount(entry.sd)) {
        // Count through pathVertexRowCount, not entry.sd.segments.length: for
        // a vector-brush ribbon the vertices are its centerline, not the
        // baked outline (#181). The grid half below reads the SAME helper —
        // CLAUDE.md §11.
        renderPathVertexGroup(list, ensureElementHolder(ld, entry.strokeId), pathVertexRowCount(entry.sd));
      }
      // Image mesh (2026-08-30) — the raster counterpart of the Path group
      // just above: same accordion, same vertex rows, same stopwatches,
      // just fed by the mesh's vertices instead of a path's segments. Its
      // holder is keyed by meshId rather than strokeId (see meshHolder's
      // own comment). MUST stay mirrored by renderTimelineMotion's grid
      // half — CLAUDE.md §11.
      if (entry.sd.isRaster && entry.sd.meshId && meshVertexRowCount(entry.sd.meshId)) {
        renderMeshVertexGroup(list, ensureElementHolder(ld, entry.sd.meshId), meshVertexRowCount(entry.sd.meshId));
      }
      // Fill color (2026-07): opt-in extended property, hidden unless the
      // element actually has a fill — same convention as Path above.
      //
      // feedback #203 (brush half, other side of the same bug the Stroke
      // block below fixes): for a vector-brush shape, entry.sd.fillColor is
      // either borrowed from a REAL linked-fill companion (layerElements'
      // own merge sets __linkedFillStrokeId when one exists — a genuine
      // second paint, feedback #200) or, when there's no companion, IS the
      // ink color itself (nothing to merge) — that second case isn't a real
      // fill to key, it's the same ink the Stroke row now drives, and
      // showing this row for it was the Fill-side phantom-row twin of the
      // bug hasRealStroke already guards against for Stroke.
      if (entry.sd.fillColor && (!entry.sd.isVectorBrush || entry.sd.__linkedFillStrokeId)) {
        renderFillColorRow(list, ensureElementHolder(ld, entry.strokeId), entry.sd.fillColor);
      }
      // Stroke color/width (2026-08 — second slice of the "propriétés
      // étendues par forme" chantier, same convention as Fill color above):
      // hidden unless the element actually has a real stroke. Width reuses
      // renderTrimScalarRow as-is (a plain scalar row is a plain scalar
      // row) rather than writing a near-identical function — its default
      // comes from THIS shape's own current width, same "per-shape, not
      // one shared constant" principle ensureElementHolder's corner-radii
      // seeding already establishes for paramShapeKind.
      // hasRealStroke, not the mere truthiness of sd.strokeColor (feedback
      // #203, "le fill properties... agit aussi sur le stroke, il devrait y
      // avoir une propriété keyframable de stroke séparée") — serP (app.js)
      // bakes a '#ffffff' FALLBACK into strokeColor for every ordinary
      // fill-only shape (CLAUDE.md §1's own documented gotcha; the same trap
      // color-manager.js's computeUsedColors and shapes-panel.js's own
      // hasRealStroke check already guard against). Without this, EVERY
      // plain filled shape got a phantom "Stroke" row seeded with that white
      // fallback — keying it created a real (if invisible-until-then) white
      // stroke track sitting right next to Fill's own row, which is what
      // reads as "the fill row is also touching the stroke": both rows are
      // always there and both look keyable, on a shape that never actually
      // had a stroke to animate.
      var hasRealStrokeEl = entry.sd.hasRealStroke !== undefined ? entry.sd.hasRealStroke : !!entry.sd.strokeColor;
      // feedback #203 (brush half, "il devrait y avoir une propriété
      // keyframable de stroke séparé pour les shape fait avec brush"): a
      // vector-brush ribbon's ink lives in its OWN fillColor — isVectorBrush
      // forcing hasRealStroke false above is correct, there's no real
      // outline stroke to animate — but the Stroke track now DRIVES that
      // ink (engine-bridge.js's isBrushInk branch, same feedback) once this
      // row exists to key it, so it's shown here too. No width row: a
      // brush's own "Brush Size" row just below already covers scale.
      //
      // feedback #219 ("stroke n'a pas sa couleur affichée... même couleur
      // que fill"): when this shape ALSO has a real linked-fill companion
      // (feedback #200), layerElements' own merge overwrites entry.sd.fillColor
      // with the COMPANION's color for the Fill row's benefit — reading
      // fillColor here too meant the Stroke row's default swatch silently
      // showed the companion's color instead of the anchor's own ink the
      // moment a companion existed (both rows then displayed and rendered
      // identically until Stroke was manually touched). __inkColor is the
      // anchor's real pre-merge value, preserved by that same merge
      // specifically for this row to read.
      if (hasRealStrokeEl || entry.sd.isVectorBrush) {
        var strokeHolder = ensureElementHolder(ld, entry.strokeId);
        var inkDefault = entry.sd.__inkColor !== undefined ? entry.sd.__inkColor : entry.sd.fillColor;
        renderStrokeColorRow(list, strokeHolder, entry.sd.isVectorBrush ? inkDefault : entry.sd.strokeColor);
        if (!entry.sd.isVectorBrush && entry.sd.strokeWidth !== undefined) {
          renderTrimScalarRow(list, strokeHolder, 'strokeWidth', 'Stroke Width', 'px', 0, 200, entry.sd.strokeWidth);
        }
      }
      // Brush size (2026-08 — third slice, the "brush" piece of
      // fill/stroke/brush/path): opt-in, hidden unless this is actually a
      // vector-brush ribbon (isVectorBrush) with real centerline data —
      // a plain shape has no width profile to scale. % of the shape's own
      // recorded pressure widths, same convention as the base Scale prop.
      if (entry.sd.isVectorBrush && entry.sd.centerSegments && entry.sd.centerSegments.length >= 2) {
        renderTrimScalarRow(list, ensureElementHolder(ld, entry.strokeId), 'brushSize', 'Brush Size', '%', 10, 500, 100);
      }
      // Trim Paths (2026-08, "animer les stroke en in et out"): opt-in,
      // same visibility gate as Path above (needs real vertex geometry).
      if (!entry.sd.isRaster && entry.sd.segments && entry.sd.segments.length) {
        renderTrimPathsGroup(list, ensureElementHolder(ld, entry.strokeId));
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
  // popover the layer-color dot uses elsewhere (timeline.js). Generalized
  // over `prop`/`label`/`swatchTitle` (2026-08) so Fill and Stroke color
  // share one implementation instead of drifting apart as two copies
  // (CLAUDE.md §3) — renderFillColorRow/renderStrokeColorRow below are thin
  // wrappers so existing call sites don't need to change.
  function renderColorRow(list, holder, prop, label, swatchTitle, currentColorHex) {
    var row = document.createElement('div'); row.className = 'lrow motion-prop-row';
    row._smHolder = holder; row._smProp = prop;
    var sw = document.createElement('div');
    var swOn = isAnimated(holder, prop);
    var hasKeyHere = swOn && !!keyAt(holder.motion[prop], state.currentFrame);
    sw.className = 'lico motion-stopwatch' + (swOn ? ' on' : '');
    sw.title = stopwatchTitle('motionAnimateFill', swOn, hasKeyHere);
    sw.innerHTML = '<svg viewBox="0 0 24 24" width="12" height="12"><path d="M12 3l9 9-9 9-9-9z" fill="' + (hasKeyHere ? 'currentColor' : 'none') + '" stroke="currentColor" stroke-width="2"/></svg>';
    function currentRgba() {
      if (hasKeys(holder, prop)) return valueAtFrame(holder, prop, state.currentFrame);
      if (holder.motionStatic && holder.motionStatic[prop]) return holder.motionStatic[prop];
      return hexToRgba255(currentColorHex);
    }
    sw.addEventListener('click', function (e) {
      e.stopPropagation(); pushUndo();
      if (!swOn) {
        if (!holder.motionStatic) holder.motionStatic = {};
        if (!holder.motionStatic[prop]) holder.motionStatic[prop] = hexToRgba255(currentColorHex);
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
    var nm = document.createElement('div'); nm.className = 'lnm'; nm.textContent = label;
    decorateMotionPropertyRow(row, holder, prop, nm);
    var swatch = document.createElement('div');
    swatch.style.cssText = 'width:16px;height:16px;border-radius:3px;border:1px solid var(--border2);cursor:pointer;margin-left:auto;';
    var rgba = currentRgba();
    swatch.style.background = 'rgba(' + rgba[0] + ',' + rgba[1] + ',' + rgba[2] + ',' + ((rgba[3] !== undefined ? rgba[3] : 255) / 255) + ')';
    swatch.title = swatchTitle;
    swatch.addEventListener('click', function (e) {
      e.stopPropagation();
      // The REAL colour picker, not the layer-label palette (2026-08-31,
      // feedback #180: "je n'ai pas le bon panneau de couleurs (celui des
      // labels de couleurs)"). openLayerColorSwatches offers 8 fixed label
      // colours with the full picker hidden behind a "+" — right for
      // tagging a layer, wrong for a shape's fill, where you want the whole
      // spectrum and an alpha slider straight away. ColorPicker.open takes
      // the same (anchor, hex, callback) shape, so only the call changes.
      if (!window.ColorPicker || !ColorPicker.open) return;
      ColorPicker.open(swatch, rgba255ToHex(currentRgba()), function (hex) {
        pushUndo();
        var v = hexToRgba255(hex);
        if (!setSelectedKeyVector(holder, prop, v)) setValue(holder, prop, v);
        renderLayerList(); renderTimeline();
        if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
      });
    });
    row.appendChild(sw); row.appendChild(nm); row.appendChild(swatch);
    list.appendChild(row);
  }
  function renderFillColorRow(list, holder, currentFillColorHex) {
    renderColorRow(list, holder, 'fillColor', 'Fill', SM.t('shapeFillColorTitle'), currentFillColorHex);
  }
  function renderStrokeColorRow(list, holder, currentStrokeColorHex) {
    renderColorRow(list, holder, 'strokeColor', 'Stroke', SM.t('shapeStrokeColorTitle'), currentStrokeColorHex);
  }
  // Trim Paths (2026-08, "animer les stroke en in et out"): 3 scalar rows
  // (Start/End/Offset, %), same stopwatch/keying contract as
  // renderFillColorRow but with a numeric scrub field instead of a color
  // swatch — factored into one row-builder since all 3 share everything
  // except label/prop/min/max/default.
  function renderTrimScalarRow(list, holder, prop, label, unit, min, max, defaultVal) {
    var row = document.createElement('div'); row.className = 'lrow motion-prop-row';
    row._smHolder = holder; row._smProp = prop;
    var sw = document.createElement('div');
    var swOn = isAnimated(holder, prop);
    var hasKeyHere = swOn && !!keyAt(holder.motion[prop], state.currentFrame);
    sw.className = 'lico motion-stopwatch' + (swOn ? ' on' : '');
    sw.title = stopwatchTitle('motionAnimateProp', swOn, hasKeyHere);
    sw.innerHTML = '<svg viewBox="0 0 24 24" width="12" height="12"><path d="M12 3l9 9-9 9-9-9z" fill="' + (hasKeyHere ? 'currentColor' : 'none') + '" stroke="currentColor" stroke-width="2"/></svg>';
    function currentVal() {
      if (hasKeys(holder, prop)) return valueAtFrame(holder, prop, state.currentFrame)[0];
      if (holder.motionStatic && holder.motionStatic[prop]) return holder.motionStatic[prop][0];
      return defaultVal;
    }
    sw.addEventListener('click', function (e) {
      e.stopPropagation(); pushUndo();
      if (!swOn) {
        if (!holder.motionStatic) holder.motionStatic = {};
        if (!holder.motionStatic[prop]) holder.motionStatic[prop] = [defaultVal];
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
    var nm = document.createElement('div'); nm.className = 'lnm'; nm.textContent = label;
    decorateMotionPropertyRow(row, holder, prop, nm);
    var input = document.createElement('input');
    input.type = 'number'; input.className = 'pi scrub'; input.min = min; input.max = max; input.dataset.step = '1';
    input.style.cssText = 'width:52px;margin-left:auto';
    var trimDisplay = selectedDimensionDisplay(holder, prop, 0, currentVal());
    input.value = trimDisplay.mixed ? '' : trimDisplay.value;
    var lastTrimScrubValue = trimDisplay.mixed ? 0 : (Number(trimDisplay.value) || 0);
    if (trimDisplay.mixed) { input.placeholder = '—'; input.classList.add('mixed'); input.title = 'Valeurs multiples'; }
    input.addEventListener('change', function () {
      if (this.value === '' || !isFinite(parseFloat(this.value))) return;
      pushUndo();
      var v = Math.max(min, Math.min(max, parseFloat(this.value)));
      this.value = v;
      var relative = !!window._scrubLiveActive;
      var changed = relative
        ? offsetSelectedKeyDimension(holder, prop, 0, v - lastTrimScrubValue)
        : setSelectedKeyDimension(holder, prop, 0, v);
      lastTrimScrubValue = v;
      if (!changed) setValue(holder, prop, [v]);
      renderLayerList(); renderTimeline();
      if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    });
    row.appendChild(sw); row.appendChild(nm); row.appendChild(input);
    if (unit) { var u = document.createElement('span'); u.style.cssText = 'font-size:9px;color:var(--text-dim);margin-left:2px'; u.textContent = unit; row.appendChild(u); }
    list.appendChild(row);
  }
  function renderTrimPathsGroup(list, holder) {
    var grp = document.createElement('div'); grp.className = 'lrow motion-group-row';
    var expanded = window._motionExpandedTrimHolder === holder;
    var arrow = document.createElement('span'); arrow.className = 'lico larrow'; arrow.textContent = expanded ? '▾' : '▸';
    var label = document.createElement('span'); label.textContent = 'Trim Paths';
    grp.title = SM.t('titleTrimPathGroupHint');
    grp.appendChild(arrow); grp.appendChild(label);
    grp.addEventListener('click', function (e) {
      e.stopPropagation();
      window._motionExpandedTrimHolder = expanded ? null : holder;
      renderLayerList(); renderTimeline();
    });
    list.appendChild(grp);
    if (!expanded) return;
    renderTrimScalarRow(list, holder, 'trimStart', 'Start', '%', 0, 100, 0);
    renderTrimScalarRow(list, holder, 'trimEnd', 'End', '%', 0, 100, 100);
    renderTrimScalarRow(list, holder, 'trimOffset', 'Offset', '%', -100, 100, 0);
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
  // ---- Path as ONE keyframable property (2026-08-31, feedback #179) ----
  // "avant d'ouvrir la section path d'une shape on devrait a[voir] path
  // keyframable ça veut dire que si on keyframe path tous les vertex sont
  // keyframer et quand on bouge les keyframe de path alors ça bougent les
  // keyframes de path en groupe."
  //
  // The group header is a VIEW over the vtxN tracks, never a track of its
  // own: a "path key" is exactly "a key on every vertex at that frame", so
  // there is no second source of truth that could disagree with the rows
  // underneath it — no migration, no persistence change, and a shape keyed
  // vertex-by-vertex before this existed already shows its keys here.
  // Same reasoning as the widget layer owning no values of its own (§13).
  function pathGroupFrames(holder, vertexCount) {
    var frames = [];
    if (!holder || !holder.motion) return frames;
    var seen = {};
    for (var vi = 0; vi < vertexCount; vi++) {
      var tr = holder.motion['vtx' + vi];
      if (!tr || !tr.keys) continue;
      tr.keys.forEach(function (k) { if (!seen[k.frame]) { seen[k.frame] = 1; frames.push(k.frame); } });
    }
    return frames.sort(function (a, b) { return a - b; });
  }
  // Every vertex key sitting on `frame`, in the {holder, prop, key} shape the
  // existing group-drag engine already consumes (onDragMove's `d.group`
  // branch) — so dragging a path key reuses that whole path, collision
  // checks and snapping included, instead of a parallel mover.
  function pathGroupKeysAt(holder, vertexCount, frame) {
    var out = [];
    for (var vi = 0; vi < vertexCount; vi++) {
      var prop = 'vtx' + vi;
      var tr = holder.motion && holder.motion[prop];
      if (!tr) continue;
      var k = keyAt(tr, frame);
      if (k) out.push({ holder: holder, prop: prop, key: k });
    }
    return out;
  }
  // "si on keyframe path tous les vertex sont keyframer" — one click arms and
  // keys EVERY vertex at the playhead. Deliberately unconditional per vertex
  // (arm the un-armed, key the already-armed) so the result is always the
  // same well-defined state: a key on every vertex at this frame.
  function keyWholePath(holder, vertexCount) {
    for (var vi = 0; vi < vertexCount; vi++) {
      var prop = 'vtx' + vi;
      if (!isAnimated(holder, prop)) toggleAnimated(holder, prop);
      else if (!keyAt(holder.motion[prop], state.currentFrame)) setKeyAtCurrentFrame(holder, prop, valueAtFrame(holder, prop, state.currentFrame));
    }
  }
  function unkeyWholePath(holder, vertexCount) {
    for (var vi = 0; vi < vertexCount; vi++) {
      var prop = 'vtx' + vi;
      if (!isAnimated(holder, prop)) continue;
      if (!keyAt(holder.motion[prop], state.currentFrame)) continue;
      // Removing the LAST key has to fall back to a static value or the
      // vertex would silently snap back to its undeformed position — the
      // same branch renderVertexRow's own stopwatch takes.
      if (holder.motion[prop].keys.length === 1) {
        var fv = valueAtFrame(holder, prop, state.currentFrame);
        holder.motion[prop] = { keys: [] };
        if (!holder.motionStatic) holder.motionStatic = {};
        holder.motionStatic[prop] = fv;
      } else removeKeyAtCurrentFrame(holder, prop);
    }
  }
  function renderPathVertexGroup(list, holder, vertexCount) {
    var grp = document.createElement('div'); grp.className = 'lrow motion-group-row';
    var arrow = document.createElement('span'); arrow.className = 'lico larrow'; arrow.textContent = isPathGroupExpanded(holder) ? '▾' : '▸';
    // Stopwatch on the GROUP row itself (#179). Placed before the label like
    // every other keyframable row, and it stops propagation so it never
    // doubles as the expand/collapse toggle the rest of the row is.
    var frames = pathGroupFrames(holder, vertexCount);
    var anyKeys = frames.length > 0;
    var keyHere = frames.indexOf(state.currentFrame) >= 0;
    var sw = document.createElement('div');
    sw.className = 'lico motion-stopwatch' + (anyKeys ? ' on' : '');
    sw.title = stopwatchTitle('motionAnimatePath', anyKeys, keyHere);
    sw.innerHTML = '<svg viewBox="0 0 24 24" width="12" height="12"><path d="M12 3l9 9-9 9-9-9z" fill="' + (keyHere ? 'currentColor' : 'none') + '" stroke="currentColor" stroke-width="2"/></svg>';
    sw.addEventListener('click', function (e) {
      e.stopPropagation(); pushUndo();
      if (keyHere) unkeyWholePath(holder, vertexCount);
      else keyWholePath(holder, vertexCount);
      renderLayerList(); renderTimeline();
      if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    });
    var label = document.createElement('span'); label.textContent = 'Path';
    grp.appendChild(sw); grp.appendChild(arrow); grp.appendChild(label);
    grp.addEventListener('click', function (e) {
      e.stopPropagation();
      window._motionExpandedPathHolder = isPathGroupExpanded(holder) ? null : holder;
      renderLayerList(); renderTimeline();
    });
    list.appendChild(grp);
    if (!isPathGroupExpanded(holder)) return;
    for (var vi = 0; vi < vertexCount; vi++) renderVertexRow(list, holder, vi);
  }
  // Grid mirror of the header row above. Same class (and therefore the same
  // height) as the plain spacer it replaces, so the §11 panel/grid alignment
  // is untouched — it just has diamonds in it now.
  function renderPathGroupTrackRow(grid, holder, vertexCount) {
    var row = document.createElement('div'); row.className = 'frow motion-group-row';
    var frames = pathGroupFrames(holder, vertexCount);
    var mark = {};
    frames.forEach(function (f) { mark[f] = 1; });
    for (var fi = 0; fi < state.totalFrames; fi++) {
      var c = document.createElement('div');
      c.className = 'fc motion-fc' + (fi === state.currentFrame ? ' cur' : '');
      c.dataset.frame = fi;
      if (mark[fi]) {
        var dia = document.createElement('div');
        dia.className = 'motion-key' + (fi === state.currentFrame ? ' cur' : '');
        dia.title = 'Path · image ' + (fi + 1);
        if (typeof FC === 'number' && FC > 0 && FC < 7) {
          var dsz = Math.max(3, FC);
          dia.style.width = dsz + 'px'; dia.style.height = dsz + 'px';
        }
        (function (frame) {
          dia.addEventListener('mousedown', function (e) {
            e.stopPropagation(); e.preventDefault();
            var picked = pathGroupKeysAt(holder, vertexCount, frame);
            if (!picked.length) return;
            pushUndo();
            if (frame !== state.currentFrame) {
              goToFrame(frame);
              picked = pathGroupKeysAt(holder, vertexCount, frame);
            }
            // Select the whole set, then hand it to the SAME group-drag the
            // multi-selection uses — "ça bougent les keyframes de path en
            // groupe" is exactly what that engine already does, including
            // refusing a move that would collide with an unselected key.
            setKeySel(picked);
            window._motionKeyDrag = {
              group: true, startX: e.clientX, startScrollLeft: motionDragScrollLeft(),
              keys: picked.map(function (s) { return { holder: s.holder, prop: s.prop, key: s.key, origFrame: s.key.frame }; }),
            };
            renderTimeline();
          });
        })(fi);
        c.appendChild(dia);
      }
      row.appendChild(c);
    }
    grid.appendChild(row);
  }
  // Generic grid row for a DISCRETE (hold-only) key track — Blend/Parent
  // (feedback #207, "le parentage doit être keyframable... mettre les
  // keyframes à droite pour cohérence dans le panel") share this exact
  // shape: a plain [{frame,...}] array with no interpolation between
  // entries, so the marker row only needs a frame list, not a full
  // property/holder pair the way renderTracksFor's generic numeric-track
  // machinery expects. Click navigates to that frame (same as every other
  // keyframe row's left-click); right-click removes it. No drag-to-retime
  // in v1 — unlike Position/Path, neither Blend nor Parent benefit from a
  // "move several keys together" gesture, since each row here is its own
  // single, independent track.
  // feedback #214 ("impossible de select et déplacer des keyframes de
  // parent et blend et si j'essaye de faire une selection... et touche
  // supp ça fait bug l'app") — this row's diamonds used to only navigate
  // the playhead on click and offer a right-click delete; there was no
  // way to SELECT one at all, so _motionKeySel always stayed empty for
  // them, and every generic selection-driven feature (drag, Delete key,
  // the selection box) silently no-op'd rather than acting on these keys.
  //
  // Rather than duplicating trackRowHtml's full mousedown pipeline (ease
  // boxes, hold/linear context-menu items, skew-drag — none of which make
  // sense for a hold-only discrete track, CLAUDE.md §11's "reuse the
  // pattern" note doesn't mean copy every feature), this reuses only the
  // pieces that ARE generic across any {holder,prop,key} triple:
  // setKeySel/_motionKeySel/isKeySelected for selection, and the shared
  // window._motionKeyDrag group-drag object — onDragMove/onDragUp (below
  // in this file) only ever touch `.key.frame` for a group drag, and
  // deleteSelectedKeys/shiftKeySelection only ever touch trackFor(...).keys
  // — none of that cares what prop string it's given, so extending
  // trackFor (below) to resolve 'blendKeys'/'parentKeys' is the one piece
  // that makes all of it work for these rows too, unmodified.
  function renderDiscreteKeyGridRow(grid, holder, prop, label, keys, removeAt) {
    var row = document.createElement('div'); row.className = 'frow motion-group-row';
    row._smHolder = holder; row._smProp = prop;
    var byFrame = {};
    keys.forEach(function (k) { byFrame[k.frame] = k; });
    for (var fi = 0; fi < state.totalFrames; fi++) {
      var c = document.createElement('div');
      c.className = 'fc motion-fc' + (fi === state.currentFrame ? ' cur' : '');
      c.dataset.frame = fi;
      var key = byFrame[fi];
      if (key) {
        var dia = document.createElement('div');
        dia.className = 'motion-key hold' + (fi === state.currentFrame ? ' cur' : '') + (isKeySelected(holder, prop, key) ? ' sel' : '');
        dia.title = label + ' · image ' + (fi + 1);
        if (typeof FC === 'number' && FC > 0 && FC < 7) {
          var dsz = Math.max(3, FC);
          dia.style.width = dsz + 'px'; dia.style.height = dsz + 'px';
        }
        (function (frame, k) {
          dia.addEventListener('mousedown', function (e) {
            e.stopPropagation(); e.preventDefault();
            if (e.metaKey || e.ctrlKey) {
              if (isKeySelected(holder, prop, k)) {
                setKeySel(_motionKeySel.filter(function (s) { return !(s.holder === holder && s.prop === prop && s.key === k); }));
              } else {
                setKeySel(_motionKeySel.concat([{ holder: holder, prop: prop, key: k }]));
              }
              _keyAnchor = { holder: holder, prop: prop, frame: frame };
              renderTimeline();
              return;
            }
            // feedback #221 ("impossible de sélectionner... comme les
            // autres propriétés") — Shift-range, mirroring the numeric
            // row's own handler: reuse the marquee's rectangle-selection
            // between the anchor cell and this one, so it picks up
            // whatever else is in that box too (now that applyMarqueeSelection
            // itself also scans this row's own .motion-group-row class).
            if (e.shiftKey) {
              var anchorCell = null;
              if (_keyAnchor) {
                var rowsA = document.querySelectorAll('#frame-grid .motion-track-row, #frame-grid .motion-group-row');
                for (var ri = 0; ri < rowsA.length; ri++) {
                  if (rowsA[ri]._smHolder === _keyAnchor.holder && rowsA[ri]._smProp === _keyAnchor.prop) {
                    anchorCell = rowsA[ri].querySelector('.fc[data-frame="' + _keyAnchor.frame + '"]');
                    break;
                  }
                }
              }
              if (anchorCell) {
                var ar = anchorCell.getBoundingClientRect(), cr = c.getBoundingClientRect();
                var ax = ar.left + ar.width / 2, ay = ar.top + ar.height / 2;
                var bx = cr.left + cr.width / 2, by = cr.top + cr.height / 2;
                applyMarqueeSelection(Math.min(ax, bx), Math.min(ay, by), Math.max(ax, bx), Math.max(ay, by));
              } else {
                setKeySel([{ holder: holder, prop: prop, key: k }]);
                _keyAnchor = { holder: holder, prop: prop, frame: frame };
              }
              renderTimeline();
              return;
            }
            pushUndo();
            if (isKeySelected(holder, prop, k)) {
              // Already part of the current selection — drag the WHOLE
              // group together, same convention a numeric row's own
              // mousedown uses.
              _keyAnchor = { holder: holder, prop: prop, frame: frame };
              window._motionKeyDrag = {
                group: true, startX: e.clientX, startScrollLeft: motionDragScrollLeft(),
                keys: _motionKeySel.map(function (s) { return { holder: s.holder, prop: s.prop, key: s.key, origFrame: s.key.frame }; })
              };
            } else {
              setKeySel([{ holder: holder, prop: prop, key: k }]);
              _keyAnchor = { holder: holder, prop: prop, frame: frame };
              window._motionKeyDrag = {
                group: true, startX: e.clientX, startScrollLeft: motionDragScrollLeft(),
                keys: [{ holder: holder, prop: prop, key: k, origFrame: k.frame }]
              };
            }
            if (frame !== state.currentFrame) goToFrame(frame);
            renderTimeline();
          });
          dia.addEventListener('contextmenu', function (e) {
            e.preventDefault(); e.stopPropagation();
            if (!window.showContextMenu) return;
            window.showContextMenu(e.clientX, e.clientY, [
              { label: SM.t('ctxDeleteThisKey'), action: function () {
                pushUndo();
                removeAt(frame);
                setKeySel(_motionKeySel.filter(function (s) { return !(s.holder === holder && s.prop === prop && s.key === k); }));
                renderLayerList(); renderTimeline();
                if (window.SMEngineBridge) SMEngineBridge.renderNow();
              } },
            ]);
          });
        })(fi, key);
        c.appendChild(dia);
      } else {
        // feedback #221 ("impossible de sélectionner avec rec de
        // sélection") — a numeric row's own empty cells start a marquee on
        // mousedown (trackRowHtml); this row's cells never got the same
        // listener at all, so a drag STARTING on empty space within it
        // never even called startMarquee. Diamonds keep their own handler
        // above (dragging one moves it, not a marquee) — this is only for
        // the gaps between them.
        c.addEventListener('mousedown', function (e) {
          e.stopPropagation();
          startMarquee(e);
        });
      }
      row.appendChild(c);
    }
    grid.appendChild(row);
  }
  // Image mesh accordion (2026-08-30) — identical to renderPathVertexGroup
  // above apart from its label, and sharing its expand state
  // (_motionExpandedPathHolder is keyed by HOLDER, and a mesh holder is a
  // different object from any path holder, so "one group open at a time"
  // keeps working across both without a second flag). Written as its own
  // function rather than a `label` parameter on renderPathVertexGroup so
  // the grid-half mirror has an obviously-paired call to point at.
  function renderMeshVertexGroup(list, holder, vertexCount) {
    var grp = document.createElement('div'); grp.className = 'lrow motion-group-row';
    var arrow = document.createElement('span'); arrow.className = 'lico larrow'; arrow.textContent = isPathGroupExpanded(holder) ? '▾' : '▸';
    var label = document.createElement('span'); label.textContent = SM.t('hdrImageMesh');
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
    row._smHolder = holder; row._smProp = prop;
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
    decorateMotionPropertyRow(row, holder, prop, nm);
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
    rowEl.classList.toggle('prop-selected', isMotionPropSelected(ld, prop));
    var track = trackFor(ld, prop);
    var w = state.totalFrames * FC;
    var svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('width', w); svg.setAttribute('height', ROW_H);
    svg.style.cssText = 'position:absolute;left:0;top:0;pointer-events:none;';
    rowEl.appendChild(svg);
    if (track && track.keys.length) {
      // Frame-duration block behind each key (Van Dijk 3.3): the diamond is
      // centred on the frame's START, which at high zoom makes it hard to
      // see how much time one frame actually occupies — and whether a key
      // lines up with a layer's out point. Drawn only when a frame is wide
      // enough for the block to mean anything (his "closest three zoom
      // levels"), otherwise it degrades into a smear.
      //
      // Drawn BEFORE the connectors (2026-08-16): SVG paints in document
      // order, and the spec stacks these explicitly — .kf-frame z-index:0,
      // .kf-conn z-index:1, .kf z-index:3. The previous order had the
      // duration block painting OVER the connector line it's supposed to sit
      // behind, muddying the line's colour at high zoom.
      if (FC >= 18) {
        track.keys.forEach(function (k, ki) {
          var d = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
          d.setAttribute('x', k.frame * FC); d.setAttribute('y', 0);
          d.setAttribute('width', FC); d.setAttribute('height', ROW_H);
          d.setAttribute('fill', k.color || 'var(--accent)'); d.setAttribute('opacity', '0.13');
          // Same live-preview tagging as the connector bars below.
          d.setAttribute('class', 'motion-key-durblock');
          d.setAttribute('data-ki', ki);
          svg.appendChild(d);
        });
      }
      // Connection bar between consecutive keys whose value actually
      // changes — AE-wishlist idea (sandervandijk.tv "Connection"/"Keyframe
      // Duration"): makes it obvious at a glance WHERE movement happens
      // instead of a row of identical-looking diamonds with no context.
      //
      // Geometry from the spec's own .kf-conn rule (2026-08-16): height 5,
      // border-radius 2, vertically centred. Both flat pixel values, NOT
      // ratios of the row height — the spec's diamonds are 7px and Nemo's
      // Motion diamonds are 7px too (body.mode-motion .motion-key), so the
      // line has to keep the same absolute proportion against them, not
      // rescale with a row that happens to be half the mockup's height.
      // Colour/hover/selected all live in CSS (.motion-key-connect) rather
      // than presentation attributes here — the spec defines three states
      // and a CSS class ramp is the only way :hover can reach them.
      var barH = 5, barY = Math.round((ROW_H - barH) / 2);
      for (var i = 0; i < track.keys.length - 1; i++) {
        var a = track.keys[i], b = track.keys[i + 1];
        var changed = a.v.some(function (v, d) { return v !== b.v[d]; });
        if (!changed) continue;
        var rect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
        rect.setAttribute('x', a.frame * FC + FC / 2); rect.setAttribute('y', barY);
        rect.setAttribute('width', (b.frame - a.frame) * FC); rect.setAttribute('height', barH);
        rect.setAttribute('rx', 2);
        // class + key index so previewKeyframeShift can translate these live
        // during a bar/in-out drag alongside the diamonds — without the tag
        // they stayed frozen at their pre-drag position until drop ("la
        // barre bleue entre les keyframes ne bouge pas en temps réel").
        // .sel mirrors the diamonds' own convention: on when BOTH endpoint
        // keys are selected, which is exactly the state a connector click
        // (below) produces — never toggled independently of them.
        // Key colours carry onto the connector (2026-08-30, "quand je parle
        // de group c'est le trait qui joint les keyframes"): colouring a
        // walk cycle's keys should paint the whole SPAN, so the group reads
        // as one coloured block at a glance instead of a few tinted diamonds
        // lost in a row of identical ones.
        // Only when both endpoints carry the SAME colour — that is what
        // "inside the group" means. A span between two differently coloured
        // keys is a BOUNDARY between groups, and one between a coloured and
        // an uncoloured key is half outside, so both keep the default fill
        // rather than guessing which side owns them.
        // Same .tinted + --key-color mechanism the diamonds already use
        // (see the dia.classList.add('tinted') line above), so a colour is
        // set in exactly one place and CSS owns every state.
        var tint = (a.color && b.color && a.color === b.color) ? a.color : null;
        rect.setAttribute('class', 'motion-key-connect'
          + (isKeySelected(ld, prop, a) && isKeySelected(ld, prop, b) ? ' sel' : '')
          + (tint ? ' tinted' : ''));
        if (tint) rect.style.setProperty('--key-color', tint);
        rect.setAttribute('data-i', i);
        // The connector is a draggable TARGET, not decoration
        // (nemo-timeline-inout-spec.html, 2026-08-16): "le trait est une
        // cible, pas une décoration". Click selects both endpoint keys;
        // drag moves the whole segment by one shared delta (duration
        // constant); Alt+drag retimes — the first key stays planted, only
        // the second moves, stretching/compressing the movement in place.
        // pointer-events:auto (CSS) punches a hole through the parent svg's
        // pointer-events:none (set once at the top of trackRowHtml) — every
        // OTHER child (durblocks, the invisible hit area) stays pass-
        // through, only the connector itself is a real target.
        (function (keyA, keyB) {
          rect.addEventListener('mousedown', function (e) {
            e.stopPropagation();
            pushUndo();
            setKeySel([{ holder: ld, prop: prop, key: keyA }, { holder: ld, prop: prop, key: keyB }]);
            _keyAnchor = { holder: ld, prop: prop, frame: keyA.frame };
            window._motionConnectDrag = {
              ld: ld, prop: prop, a: keyA, b: keyB,
              startX: e.clientX, startAFrame: keyA.frame, startBFrame: keyB.frame,
              startScrollLeft: motionDragScrollLeft(),
              retime: e.altKey,
            };
            document.body.style.cursor = e.altKey ? 'e-resize' : 'ew-resize';
            goToFrame(keyA.frame);
            renderTimeline();
          });
        })(a, b);
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
        var interpKind = keyInterpolationKind(k);
        dia.className = 'motion-key ' + interpKind + (fi === state.currentFrame ? ' cur' : '') + (isKeySelected(ld, prop, k) ? ' sel' : '');
        dia.title = (PROP_LABEL[prop] || prop) + ' · image ' + (fi + 1) + ' · ' + keyInterpolationLabel(k);
        // Per-key colour (Van Dijk 3.4: "sometimes you have so many
        // keyframes it becomes difficult to know what does what — like
        // layers, we could color keyframes to highlight a group"). Only a
        // paint job: nothing reads k.color at evaluation time.
        if (k.color) { dia.classList.add('tinted'); dia.style.setProperty('--key-color', k.color); }
        // Cap the diamond's own size to the per-frame column width at low
        // zoom (2026-08-27, "les keyframe sont toujours écrasé visuelement
        // le rond des keyframes si je dezoom la timeline"). `.motion-key`
        // is a fixed 7px (9px .cur) in CSS (body.mode-motion rules) — FC
        // (timeline-zoom.js) can shrink well below that on long projects,
        // so two adjacent keys' diamonds started overlapping even though
        // their FRAME positions were still distinct. Inline style wins over
        // the CSS class rules by specificity, same pattern already used a
        // few lines up for `--key-color`. Floor of 3px keeps a dot visibly
        // clickable instead of vanishing.
        if (typeof FC === 'number' && FC > 0 && FC < 7) {
          var dsz = Math.max(3, FC);
          dia.style.width = dsz + 'px'; dia.style.height = dsz + 'px';
        }
        c.appendChild(dia);
        // Compact bidirectional ease controls. Only the primary key in a
        // multi-selection owns the boxes; changing either side propagates
        // to every selected key that actually has that incoming/outgoing
        // segment, avoiding a forest of overlapping inputs.
        if (isPrimarySelectedKey(ld, prop, k) && !window._motionKeyDrag && !window._motionSkewDrag) {
          // Never build the ease boxes while a keyframe itself is being
          // dragged along the timeline (2026-08 fix, feedback: "les boite de
          // lissage ne doivent pas apparraitre si on drag la keyframe") —
          // window._motionKeyDrag/_motionSkewDrag are set for exactly that
          // gesture (see the row's own mousedown handler a few hundred
          // lines up, and onUp's cleanup below). The boxes are positioned
          // relative to the key's CURRENT frame, so during a drag they'd
          // otherwise pop in and visibly slide around mid-gesture — a
          // distraction from (and visual noise on top of) the actual
          // keyframe being moved.
          // Hide a side's ease box when the neighboring key is too close on
          // screen for it to fit (2026-08 fix, feedback: "j'utilise glisser
          // dézoomer sur la timeline... les visuels de keyframe sont
          // écrasés") — each box is a fixed ~38px (27px input + 11px arrow)
          // positioned off the key's OWN center regardless of zoom, so at a
          // low pixels-per-frame (window.FC) zoom the two boxes (and the
          // neighboring key's own diamond) end up crammed on top of each
          // other into unreadable mush instead of just not being there.
          // minGapPx leaves a little breathing room past the box's own
          // width so it never touches the neighboring diamond either.
          var kIdx = track.keys.indexOf(k);
          var prevKey = kIdx > 0 ? track.keys[kIdx - 1] : null;
          var nextKey = kIdx >= 0 && kIdx < track.keys.length - 1 ? track.keys[kIdx + 1] : null;
          var minGapPx = 46;
          var roomIn = !prevKey || (k.frame - prevKey.frame) * window.FC >= minGapPx;
          var roomOut = !nextKey || (nextKey.frame - k.frame) * window.FC >= minGapPx;
          var easeInBox = roomIn ? buildKeyEaseBox(ld, prop, k, 'in') : null;
          var easeOutBox = roomOut ? buildKeyEaseBox(ld, prop, k, 'out') : null;
          if (easeInBox) c.appendChild(easeInBox);
          if (easeOutBox) c.appendChild(easeOutBox);
        }
      }
      (function (frameIdx, key) {
        c.addEventListener('mousedown', function (e) {
          e.stopPropagation();
          // Ctrl/Cmd = toggle one key, Shift = rectangular range from the
          // frozen anchor (2026-07-31 unification — keyframe diamonds had
          // ZERO modifier handling; the only multi-select was the marquee).
          // Selection-only: no drag started, no playhead move, no undo push
          // (matches the layer-list and bar handlers' own modifier clicks).
          if (key && (e.metaKey || e.ctrlKey)) {
            if (isKeySelected(ld, prop, key)) {
              setKeySel(_motionKeySel.filter(function (s) { return !(s.holder === ld && s.prop === prop && s.key === key); }));
            } else {
              var next = _motionKeySel.slice();
              next.push({ holder: ld, prop: prop, key: key });
              setKeySel(next);
            }
            _keyAnchor = { holder: ld, prop: prop, frame: key.frame };
            // Full re-render, not a class toggle: a selected diamond renders
            // extra CONTENT (the ease% velocity badge), not just a class.
            renderTimeline();
            return;
          }
          if (key && e.shiftKey) {
            // Reuse the marquee's own rectangle-selection logic between the
            // anchor cell and the clicked cell — byte-for-byte the same
            // selection a literal marquee drag between them would produce,
            // over whatever rows are currently rendered.
            var anchorCell = null;
            if (_keyAnchor) {
              var rowsA = document.querySelectorAll('#frame-grid .motion-track-row');
              for (var ri = 0; ri < rowsA.length; ri++) {
                if (rowsA[ri]._smHolder === _keyAnchor.holder && rowsA[ri]._smProp === _keyAnchor.prop) {
                  anchorCell = rowsA[ri].querySelector('.fc[data-frame="' + _keyAnchor.frame + '"]');
                  break;
                }
              }
            }
            if (anchorCell) {
              var ar = anchorCell.getBoundingClientRect(), cr2 = c.getBoundingClientRect();
              var ax = ar.left + ar.width / 2, ay = ar.top + ar.height / 2;
              var bx = cr2.left + cr2.width / 2, by = cr2.top + cr2.height / 2;
              applyMarqueeSelection(Math.min(ax, bx), Math.min(ay, by), Math.max(ax, bx), Math.max(ay, by));
            } else {
              // No usable anchor (cold start / its row collapsed): plain
              // select this key and make it the anchor for the next Shift.
              setKeySel([{ holder: ld, prop: prop, key: key }]);
              _keyAnchor = { holder: ld, prop: prop, frame: key.frame };
            }
            renderTimeline();
            return;
          }
          if (key) {
            // Navigate before creating the plain-click selection. goToFrame
            // refreshes the Motion UI and can replace the live key object;
            // selecting the pre-refresh object made a real click move the
            // playhead but immediately lose its visible .sel state. Re-read
            // the key from the refreshed track so click, drag and the value
            // fields all share the same live reference.
            if (frameIdx !== state.currentFrame) {
              goToFrame(frameIdx);
              key = keyAt(trackFor(ld, prop), frameIdx) || key;
            }
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
              _keyAnchor = { holder: ld, prop: prop, frame: key.frame };
              var skewRows = e.altKey && _motionKeySel.length >= 2 ? buildKeyRows() : null;
              if (skewRows && skewRows.length >= 2) {
                window._motionSkewDrag = { startX: e.clientX, startScrollLeft: motionDragScrollLeft(), mode: 'bottom', rows: skewRows };
              } else {
                window._motionKeyDrag = {
                  group: true, startX: e.clientX, startScrollLeft: motionDragScrollLeft(),
                  keys: _motionKeySel.map(function (s) { return { holder: s.holder, prop: s.prop, key: s.key, origFrame: s.key.frame }; })
                };
              }
              // Move the pair of influence boxes to the selected key that
              // was actually grabbed, while preserving the whole group.
              renderTimeline();
            } else {
              setKeySel([{ holder: ld, prop: prop, key: key }]);
              _keyAnchor = { holder: ld, prop: prop, frame: key.frame };
              window._motionKeyDrag = { ld: ld, prop: prop, key: key, startX: e.clientX, startScrollLeft: motionDragScrollLeft(), startFrame: key.frame };
              // setKeySel updates the selection model/box, but a plain key
              // click also needs to rebuild the diamond itself: selected
              // keys contain a velocity badge in addition to the .sel class.
              // Without this render, the click worked internally while the
              // key stayed visually indistinguishable until another UI
              // action happened to repaint the timeline.
              renderTimeline();
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
              ? { label: SM.t('ctxDeleteThisKey'), action: function () { pushUndo(); var tr = trackFor(ld, prop); tr.keys.splice(tr.keys.indexOf(key), 1); renderTimeline(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); } }
              // feedback #215 follow-up: displayValueFor, not staticValue —
              // see toggleAnimated's own comment on the same seeding gap.
              : { label: SM.t('ctxAddKeyHere'), action: function () { pushUndo(); setKeyAtCurrentFrame(ld, prop, isAnimated(ld, prop) ? valueAtFrame(ld, prop, frameIdx) : displayValueFor(ld, prop)); renderLayerList(); renderTimeline(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); } },
          ];
          if (track && track.keys.length) {
            // A single-key track auto-creates its missing second key on open
            // (see openMotionEaseEditor's header comment) — no need to
            // require a full segment already existing before offering this.
            menu.push({ label: SM.t('ctxEditEaseCurve'), action: function () { pushUndo(); openMotionEaseEditor(ld, prop); } });
          }
          if (key) {
            // Hold keyframe (2026-07): no interpolation out of this key —
            // the value snaps to the NEXT key's value the instant it's
            // reached. Renders as a square (see the .hold class above).
            menu.push({ label: key.hold ? SM.t('ctxRemoveHold') : SM.t('ctxMakeHold'), action: function () { pushUndo(); key.hold = !key.hold; renderLayerList(); renderTimeline(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); } });
          }
          // Interpolation presets, mirroring the keyboard layer added
          // alongside them (timeline.js onKeyDown) so neither is the only way
          // in — the keyboard is faster once known, the menu is how it gets
          // known. They act on the whole selection, so right-clicking a key
          // that isn't selected yet selects it first (the mousedown handler
          // above already did that by the time this fires).
          if (_motionKeySel.length) {
            menu.push({ sep: true });
            menu.push({ label: SM.t('ctxEasyEaseF9'), action: function () { applyEasyEase('ease'); } });
            menu.push({ label: SM.t('ctxEasyEaseInShiftF9'), action: function () { applyEasyEase('easeIn'); } });
            menu.push({ label: SM.t('ctxEasyEaseOutCmdShiftF9'), action: function () { applyEasyEase('easeOut'); } });
            menu.push({ label: SM.t('ctxLinearCmdAltK'), action: function () { setKeyInterp('linear'); } });
          }
          // Batch ops act on the WHOLE current multi-selection, offered
          // regardless of which key/cell was right-clicked. No Align here
          // (unlike layer bars) — a keyframe has no duration, "align"
          // doesn't map onto a single point.
          if (_motionKeySel.length >= 2) {
            menu.push({ sep: true });
            menu.push({ label: SM.t('ctxDistributeEvenly'), action: distributeKeys });
            menu.push({ label: SM.t('ctxFlipOrder'), action: flipKeys });
            menu.push({ label: SM.t('ctxSubdivideMidpoint'), action: subdivideKeys });
            menu.push({ label: SM.t('ctxColorKeysEllipsis'), action: function () {
              var palette = ['#e8b64c', '#4ea9ff', '#59d38a', '#ff6b8b', '#b98cff', '#ffffff'];
              var names = [SM.t('colorAmber'), SM.t('colorBlue'), SM.t('colorGreen'), SM.t('colorPink'), SM.t('colorPurple'), SM.t('colorWhite')];
              window.showContextMenu(e.clientX + 8, e.clientY + 8,
                names.map(function (n, i) { return { label: n, action: function () { colorSelectedKeys(palette[i]); } }; })
                  .concat([{ sep: true }, { label: SM.t('ctxRemoveColor'), action: function () { colorSelectedKeys(null); } }]));
            } });
            menu.push({ label: SM.t('ctxSelectEvery2nd'), action: function () { selectEveryNthKey(2); } });
            menu.push({ label: SM.t('ctxSelectEveryNthEllipsis'), action: function () {
              var v = prompt(SM.t('promptKeepOneKeyOutOf'), '3');
              if (v !== null) selectEveryNthKey(v);
            } });
            menu.push({ label: SM.t('ctxKeepRandomEllipsis'), action: function () {
              var v = prompt(SM.t('promptKeepPercentOfSelection'), '50');
              if (v !== null) grabRandomKeys(v);
            } });
          }
          if (_motionKeySel.length >= 1) menu.push({ label: SM.t('ctxInvertSelection'), action: invertKeySelection });
          // Menu-based Parent-in-Time (2026-07-31) — third surface after the
          // Motion layer-row and in/out-bar menus, for full symmetry: a
          // right-click on a keyframe cell can link ITS layer's time without
          // hunting for the row. Target = the active layer (keyframe rows
          // always belong to the currently-expanded active layer's tracks);
          // the keyframe selection itself is incidental context (confirmed
          // with Cyril), the offsets seed from the current gap as always.
          if (window.buildTimeLinkMenuItems && state.layers[state.activeLayerIdx]) {
            menu.push({ sep: true });
            menu.push({ label: SM.t('ctxParentInTimeLinkTimeEllipsis'), action: function () {
              window.showContextMenu(e.clientX + 8, e.clientY + 8, window.buildTimeLinkMenuItems(state.activeLayerIdx, state.layers[state.activeLayerIdx], function () { renderLayerList(); renderTimeline(); }));
            } });
          }
          // feedback #212 ("le lock in point, out point et layer affecte
          // toute les keyframes alors que ça devrait être les keyframes
          // select au clic droit") — this used to reuse buildKeyLockMenuItems
          // (the whole-LAYER ld.keyLock, same as the layer row's own menu).
          // Re-asked directly: this surface now locks ONLY whatever's
          // selected when the menu opens (key.lockTo, see its own header
          // comment) — independent of Parent-in-Time's own availability
          // gate above, so it's not folded into that block anymore.
          var lockItems = buildKeySelectionLockMenuItems();
          if (lockItems.length) { menu.push({ sep: true }); lockItems.forEach(function (it) { menu.push(it); }); }
          window.showContextMenu(e.clientX, e.clientY, menu);
        });
      })(fi, k);
      rowEl.appendChild(c);
    }
    // Re-parent the overlay svg to the END of the row now that the .fc grid
    // exists (2026-08-16, Cyril: "il est impossible de select le trait
    // entre 2 keyframes") — appendChild on a node already in the tree MOVES
    // it, no duplicate. Both the svg (position:absolute) and each .fc cell
    // (position:relative) are "positioned, z-index:auto" boxes, which paint
    // in DOM order — appended first at the top of this function, the svg
    // was BEHIND every .fc cell for hit-testing, even though it stayed
    // visually correct (the cells' own background is transparent where
    // nothing else draws). A synthetic dispatchEvent() called directly on
    // the <rect> during testing skips hit-testing entirely and "worked"
    // regardless — a real click, resolved via elementFromPoint, always hit
    // the covering .fc cell first and never reached the connector. Moving
    // the svg last makes it paint on top; its own pointer-events:none still
    // lets every empty region (and the .fc cells' own clicks/diamonds)
    // pass straight through — only the connector rect's explicit
    // pointer-events:auto actually intercepts anything.
    rowEl.appendChild(svg);
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
    // UI/UX report (2026-07-30, "encore pas mal de décalage d'ui dans la
    // timeline motion"): buildAnchorGridRow (the 3x3 anchor-point picker,
    // opened from the Anchor Point row's grid icon) is the SAME class of
    // extra variable-height row as the expression editor just above — but
    // had no matching spacer here at all, unlike the editor which was
    // already fixed. Every row below an open anchor grid was silently
    // shifted, same failure mode, just a second still-open instance of it.
    if (prop === 'anchor' && window._anchorGridOpenFor === holder) {
      var agRefRow = document.querySelector('#layer-list .motion-anchor-grid-row');
      var agSpacer = document.createElement('div'); agSpacer.className = 'frow motion-track-spacer';
      agSpacer.style.height = (agRefRow ? agRefRow.getBoundingClientRect().height : 60) + 'px';
      grid.appendChild(agSpacer);
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
    syncBarSelToLayerSel();
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
      if (!layerMatchesMotionView(ld)) return;
      var expanded = isLayerExpanded(li);
      // SAME class string renderLayerListMotion puts on this layer's row —
      // the grid half was left plain, so a selected layer lit up on the left
      // and stayed dark on the right ("cette partie n'est pas hightlight
      // quand select") even though it is one row across both panels.
      var spacer = document.createElement('div');
      spacer.className = 'frow' + (_layerSel.indexOf(li) >= 0 ? ' act motion-selected' : '');
      spacer.dataset.layer = li;
      if (window.installLayerReorderGrip) installLayerReorderGrip(spacer, li);
      if (window.SMLayerInOut) SMLayerInOut.buildBar(spacer, li);
      grid.appendChild(spacer);
      if (!expanded) return;
      // Mirrors renderLayerListMotion's own Parent/Blend rows exactly —
      // same condition (li's own layer, expanded), same order, same two
      // rows — CLAUDE.md §11's panel/grid alignment invariant.
      renderDiscreteKeyGridRow(grid, ld, 'parentKeys', SM.t('fieldParent') || 'Parent', ld.parentKeys || [], function (frame) { SMMotion.removeParentKeyAt(ld, frame); });
      renderDiscreteKeyGridRow(grid, ld, 'blendKeys', SM.t('fieldBlend'), ld.blendKeys || [], function (frame) { SMMotion.removeBlendKeyAt(ld, frame); });
      if (showsGroupHeader()) {
        // 'motion-group-row' too, not just 'frow' (2026-07-30 fix, Cyril:
        // "encore des problème de calage d'ui"): .motion-group-row carries
        // margin-top:6px (style.css) that ONLY existed on the panel side's
        // real header row — this plain spacer never picked it up, so every
        // row from here down silently sat 6px lower on the LEFT than its own
        // keyframe track on the RIGHT. Reusing the exact same class (instead
        // of hand-copying the margin value) guarantees the two can never
        // drift apart again if that CSS rule ever changes.
        var grpSpacer = document.createElement('div'); grpSpacer.className = 'frow motion-group-row';
        grid.appendChild(grpSpacer);
      }
      // transformRowPlan, not propsFor directly — the Duplicator sub-group
      // (collapsed by default) must render as ONE blank spacer here, mirroring
      // the panel's single chevron header row, not one spacer per hidden dup
      // property (see transformRowPlan's own comment, motion.js).
      transformRowPlan(ld).forEach(function (entry) {
        if (entry.row === 'dupHeader') { var dupSpacer = document.createElement('div'); dupSpacer.className = 'frow motion-group-row'; grid.appendChild(dupSpacer); return; }
        renderTracksFor(grid, ld, entry.prop);
      });
      // Mirrors renderElementsList's panel structure: one spacer per tree
      // node (shape OR group header — 2026-07-31, group/shape tree panel),
      // its own track rows only when that ONE shape entry is expanded (only
      // one element expands at a time, same single-expand contract as
      // layers; a group-header node's `.strokeId` is undefined so it can
      // never match _motionExpandedElement and never expands here — group
      // rows have no per-group Transform in this pass).
      // Same narrowing as renderLayerListMotion's own renderElementsList
      // gate just above (feedback #42) — a property-shortcut reveal
      // (_motionRevealedLayers, part of `expanded` via isLayerExpanded)
      // must not pull in the per-element tree here either, or this side
      // renders MORE rows than the panel for the exact same layer, which
      // is precisely the row-count divergence CLAUDE.md §11 warns about.
      // Same _motionRevealedElementLayers exception as the panel side
      // (feedback #145) — kept identical on both sides for the same
      // CLAUDE.md §11 reason this whole gate exists.
      var els = (window._motionExpandedLayer === li || (window._motionRevealedElementLayers && window._motionRevealedElementLayers.indexOf(li) >= 0)) ? buildShapeTree(li, ld) : [];
      if (els.length) {
        var elHdrSpacer = document.createElement('div'); elHdrSpacer.className = 'frow motion-group-row';
        grid.appendChild(elHdrSpacer);
        els.forEach(function (entry) {
          var elExpanded = entry.type === 'shape' && window._motionExpandedElement === entry.strokeId;
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
            var elGrpSpacer = document.createElement('div'); elGrpSpacer.className = 'frow motion-group-row';
            grid.appendChild(elGrpSpacer);
          }
          // transformRowPlan(elHolder), not the bare PROPS (2026-07-30 fix,
          // found while auditing the Duplicator alignment bug): this
          // hardcoded the base 5 properties instead of going through the
          // single row-list source of truth like the panel side
          // (renderTransformGroup -> renderTransformProps -> transformRowPlan)
          // already does. Dormant today — ensureElementHolder never stamps
          // .duplicator/.threeD/etc. on an element holder yet — but it's
          // exactly the shape of bug this whole pass is fixing, one level
          // down, and CLAUDE.md §1 is explicit that a dormant divergence
          // like this is still a bug: the moment a per-element duplicator/3D
          // capability ships, this line would silently disagree with the
          // panel instead of just working.
          transformRowPlan(elHolder).forEach(function (entry) {
            if (entry.row === 'dupHeader') { var elDupSpacer = document.createElement('div'); elDupSpacer.className = 'frow motion-group-row'; grid.appendChild(elDupSpacer); return; }
            renderTracksFor(grid, elHolder, entry.prop);
          });
          // Path group (mirrors renderElementsList's renderPathVertexGroup
          // exactly — same expand condition, same vertex count, same
          // spacer-then-rows shape as the Transform group just above).
          if (!entry.sd.isRaster && pathVertexRowCount(entry.sd)) {
            // Same gate as the panel half (pathVertexRowCount, not
            // sd.segments.length) — one helper decides for both sides.
            var vtxRows = pathVertexRowCount(entry.sd);
            // The header row carries the group's aggregate keys now (#179)
            // instead of being a blank spacer. Same class, same height.
            renderPathGroupTrackRow(grid, elHolder, vtxRows);
            if (isPathGroupExpanded(elHolder)) {
              for (var vi = 0; vi < vtxRows; vi++) renderTracksFor(grid, elHolder, 'vtx' + vi);
            }
          }
          // Image mesh group — the exact mirror of renderElementsList's own
          // renderMeshVertexGroup call: same condition, same row count from
          // the SAME shared helper (meshVertexRowCount), same
          // spacer-then-rows shape. Note the holder is the MESH holder
          // (keyed by meshId), not elHolder — the expand state and the
          // tracks both live on it, so reusing elHolder here would silently
          // render the wrong side of the accordion.
          if (entry.sd.isRaster && entry.sd.meshId && meshVertexRowCount(entry.sd.meshId)) {
            var meshHolderEl = ensureElementHolder(ld, entry.sd.meshId);
            var meshHdrSpacer = document.createElement('div'); meshHdrSpacer.className = 'frow motion-group-row';
            grid.appendChild(meshHdrSpacer);
            if (isPathGroupExpanded(meshHolderEl)) {
              var mCount = meshVertexRowCount(entry.sd.meshId);
              for (var mvi = 0; mvi < mCount; mvi++) renderTracksFor(grid, meshHolderEl, 'vtx' + mvi);
            }
          }
          // Fill color (mirrors renderElementsList's renderFillColorRow call
          // exactly — same condition). Bug found live (2026-07 —
          // "problème d'alignement de clé par rapport aux properties"):
          // this call was missing entirely, so #layer-list had one MORE row
          // than #frame-grid for any shape with a fill — every row further
          // down (a later Forme's own Transform/Path/Fill rows) drifted out
          // of alignment with its own keyframe track from that point on.
          // feedback #203 (brush half) — mirrors renderElementsList's own
          // isVectorBrush narrowing of this same condition exactly.
          if (entry.sd.fillColor && (!entry.sd.isVectorBrush || entry.sd.__linkedFillStrokeId)) renderTracksFor(grid, elHolder, 'fillColor');
          // Stroke color/width (mirrors renderElementsList's own Stroke
          // block exactly — same condition, same order: color row then,
          // only if the shape also reports a width, the width row).
          // hasRealStroke, not sd.strokeColor's truthiness — mirrors
          // renderElementsList's own Stroke block above exactly, same
          // condition (feedback #203, see that block's own comment).
          var hasRealStrokeElGrid = entry.sd.hasRealStroke !== undefined ? entry.sd.hasRealStroke : !!entry.sd.strokeColor;
          // feedback #203 (brush half) — mirrors renderElementsList's own
          // isVectorBrush widening of this same condition exactly, see its
          // comment there.
          if (hasRealStrokeElGrid || entry.sd.isVectorBrush) {
            renderTracksFor(grid, elHolder, 'strokeColor');
            if (!entry.sd.isVectorBrush && entry.sd.strokeWidth !== undefined) renderTracksFor(grid, elHolder, 'strokeWidth');
          }
          // Brush size (mirrors renderElementsList's own Brush size row —
          // same condition).
          if (entry.sd.isVectorBrush && entry.sd.centerSegments && entry.sd.centerSegments.length >= 2) {
            renderTracksFor(grid, elHolder, 'brushSize');
          }
          // Trim Paths group (mirrors renderElementsList's renderTrimPathsGroup
          // exactly — same condition, same expand state, same 3 scalar rows).
          // Bug found live (2026-08-21, QA pass on a bouncing-ball test scene):
          // this mirror was missing entirely — renderTrimPathsGroup ALWAYS
          // appends its header row on the panel side (any shape with vertex
          // geometry gets one, regardless of whether Trim Paths is actually
          // used), but this grid side had no matching spacer at all, so
          // #layer-list had one MORE row than #frame-grid for every such
          // shape even before expanding it, growing to four rows once
          // expanded — every row further down (a later Forme's own rows, or
          // the next layer entirely) drifted out of alignment with its own
          // keyframe track from that point on. Same class of bug as the Fill
          // row fix just above; confirmed via a live row-count diff (23 grid
          // rows vs 27 panel rows with Trim Paths expanded) before this fix.
          if (!entry.sd.isRaster && entry.sd.segments && entry.sd.segments.length) {
            var trimHdrSpacer = document.createElement('div'); trimHdrSpacer.className = 'frow motion-group-row';
            grid.appendChild(trimHdrSpacer);
            if (window._motionExpandedTrimHolder === elHolder) {
              renderTracksFor(grid, elHolder, 'trimStart');
              renderTracksFor(grid, elHolder, 'trimEnd');
              renderTracksFor(grid, elHolder, 'trimOffset');
            }
          }
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
  // Whether an ease-box scrub drag is currently in progress (2026-08 fix,
  // feedback: "la boite disparait si je drag le nombre dans la box") —
  // module-level, not a DOM class alone: a live scrub 'input' dispatch can
  // trigger a re-render mid-drag (renderTimeline rebuilds every .motion-key-
  // ease-box fresh), and a class added to the OLD node doesn't carry over to
  // its replacement. buildKeyEaseBox reads this flag at BUILD time so a
  // freshly rebuilt box picks the drag state back up instead of starting
  // hidden and staying that way for the rest of the gesture (real :hover
  // has usually already left the tiny box by then).
  var _easeBoxDragging = false;
  // Property rows are first-class selection targets, like AE's twirled-open
  // property names. Selecting a property selects all its keys; Cmd adds or
  // removes tracks and Shift extends through the visible property rows.
  // Object identity is intentional: element holders do not all have a uid,
  // while the live holder object is stable across timeline re-renders.
  var _motionPropSel = []; // [{holder, prop}]
  var _motionPropAnchor = null;
  // Frozen Shift-range anchor for keyframe clicks (2026-07-31 unification —
  // same contract as layer-inout.js's _barAnchorLi and timeline.js's
  // _layerSelAnchor): set on every plain click and Ctrl-toggle on a key,
  // never moved by a Shift-click itself. Stored as plain data ({holder,
  // prop, frame}), never a DOM node — rows are rebuilt on every render.
  var _keyAnchor = null;
  function isKeySelected(holder, prop, key) {
    return _motionKeySel.some(function (s) { return s.holder === holder && s.prop === prop && s.key === key; });
  }
  function isMotionPropSelected(holder, prop) {
    return _motionPropSel.some(function (s) { return s.holder === holder && s.prop === prop; });
  }
  function uniquePropSelection(sel) {
    var out = [];
    (sel || []).forEach(function (s) {
      if (!s || !s.holder || !s.prop || out.some(function (p) { return p.holder === s.holder && p.prop === s.prop; })) return;
      out.push({ holder: s.holder, prop: s.prop });
    });
    return out;
  }
  function paintMotionPropertySelection() {
    document.querySelectorAll('.motion-prop-row,.motion-track-row').forEach(function (row) {
      row.classList.toggle('prop-selected', !!(row._smHolder && row._smProp && isMotionPropSelected(row._smHolder, row._smProp)));
    });
  }
  function setKeySel(sel) {
    _motionKeySel = sel || [];
    _motionPropSel = uniquePropSelection(_motionKeySel);
    if (_motionPropSel.length) _motionPropAnchor = _motionPropSel[_motionPropSel.length - 1];
    paintMotionPropertySelection();
    updateKeySelectionBox();
  }
  function keysForPropertySelection() {
    var out = [];
    _motionPropSel.forEach(function (s) {
      var track = trackFor(s.holder, s.prop);
      if (!track || !track.keys) return;
      track.keys.forEach(function (key) { out.push({ holder: s.holder, prop: s.prop, key: key }); });
    });
    return out;
  }
  function visibleMotionPropertySelectionOrder() {
    var out = [];
    document.querySelectorAll('#layer-list .motion-prop-row').forEach(function (row) {
      if (!row._smHolder || !row._smProp) return;
      if (!out.some(function (s) { return s.holder === row._smHolder && s.prop === row._smProp; })) out.push({ holder: row._smHolder, prop: row._smProp });
    });
    return out;
  }
  function selectMotionProperty(holder, prop, e) {
    var current = { holder: holder, prop: prop };
    if (e && e.shiftKey && _motionPropAnchor) {
      var order = visibleMotionPropertySelectionOrder();
      var ai = order.findIndex(function (s) { return s.holder === _motionPropAnchor.holder && s.prop === _motionPropAnchor.prop; });
      var bi = order.findIndex(function (s) { return s.holder === holder && s.prop === prop; });
      if (ai >= 0 && bi >= 0) _motionPropSel = order.slice(Math.min(ai, bi), Math.max(ai, bi) + 1);
      else _motionPropSel = [current];
    } else if (e && (e.metaKey || e.ctrlKey)) {
      var at = _motionPropSel.findIndex(function (s) { return s.holder === holder && s.prop === prop; });
      if (at >= 0) _motionPropSel.splice(at, 1); else _motionPropSel.push(current);
      _motionPropAnchor = current;
    } else {
      _motionPropSel = [current];
      _motionPropAnchor = current;
    }
    _motionPropSel = uniquePropSelection(_motionPropSel);
    _motionKeySel = keysForPropertySelection();
    renderLayerList(); renderTimeline();
    updateKeySelectionBox();
  }
  function decorateMotionPropertyRow(row, holder, prop, labelEl) {
    if (!row || !holder || !prop) return;
    row._smHolder = holder; row._smProp = prop;
    row.classList.toggle('prop-selected', isMotionPropSelected(holder, prop));
    var target = labelEl || row;
    target.classList.add('motion-prop-select-target');
    target.title = (target.title ? target.title + ' · ' : '') + SM.t('propRowSelectHint');
    target.addEventListener('click', function (e) {
      e.stopPropagation();
      selectMotionProperty(holder, prop, e);
    });
    // Expression control rows get the manage menu right where they are —
    // the layer menu is the discoverable entry point, but once a control
    // exists, its own row is where you look to rename or remove it. Same
    // menu either way (openExprControlsMenu), never a second implementation.
    // ONE contextmenu listener per row, building one menu — the widget
    // wiring entries first (they apply to any property), then the control
    // management entries when this row happens to BE a control. Two
    // separate listeners on the same element would both fire and the second
    // showContextMenu would simply replace the first one's menu.
    if (controlTypeOf(prop) && controlsOf(holder).length) target.title += SM.t('titleControlRowContextHint');
    target.addEventListener('contextmenu', function (e) {
      e.preventDefault(); e.stopPropagation();
      if (!window.showContextMenu) return;
      var items = widgetWiringMenuItems(holder, prop, e.clientX, e.clientY);
      // showContextMenu has no real submenus, so the control-management
      // menu is reached as one entry that opens it (the same
      // openExprControlsMenu the layer menu opens) rather than being
      // inlined here — one implementation, one place to look.
      if (controlTypeOf(prop) && controlsOf(holder).length) {
        if (items.length) items.push({ sep: true });
        items.push({ label: SM.t('ctxExprControlsEllipsis'), action: function () { openExprControlsMenu(e.clientX, e.clientY, holder); } });
      }
      if (!items.length) return;
      window.showContextMenu(e.clientX, e.clientY, items);
    });
  }
  // ---- Wiring a property to a rig widget (2026-08-30) -------------------
  // The point of the widget feature. Typing
  // `layerControl("ly_x9","Turn")` by hand is not a workflow, so both
  // gestures are one right-click:
  //
  //   "Link to a widget axis…"      -> layerControl(uid, name)
  //       the property simply BECOMES the axis' number.
  //
  //   "Drive this pose from a widget axis…" -> self.at(layerControl(...))
  //       the headline. exprSelfAt reads this property's RAW track
  //       deliberately (no self-recursion, see its own comment), so this
  //       one line turns the property's OWN keyframes into a POSE LIBRARY
  //       that the widget scrubs through — functionally a Moho Smart Bone
  //       dial or a Rive Joystick axis, written in vocabulary this app
  //       already ships and already sandboxes.
  //
  // Both write through the same setExpressionCode below, i.e. the same
  // holder.expressions[prop] the editor's own textarea commits to — there
  // is no second expression writer.
  function setExpressionCode(holder, prop, code) {
    pushUndo();
    var ex = ensureExpr(holder, prop);
    ex.code = code;
    ex.enabled = true;
    ex.lastError = null;
    ex.errorLine = -1;
    // compiledFnFor caches against the code string, so changing it is
    // already enough to force a recompile; clearing the entry keeps a stale
    // prefixLines/exprMode from being read in the meantime.
    if (holder._exprCompiled) delete holder._exprCompiled[prop];
    reloadIfTimeLinkOffset(prop);
    renderLayerList(); renderTimeline();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  function widgetWiringMenuItems(holder, prop, x, y) {
    if (!window.SMRigWidget) return [];
    var choices = SMRigWidget.axisChoices();
    function sub(label, wrap, toastKey) {
      return { label: label, action: function () {
        if (!choices.length) { if (window.showToast) showToast(SM.t('ctxWidgetNoneYet')); return; }
        window.showContextMenu(x + 8, y + 8, choices.map(function (c) {
          return { label: c.layerName + ' · ' + c.name + '  (' + c.min + '…' + c.max + ')', action: function () {
            setExpressionCode(holder, prop, wrap(SMRigWidget.axisRef(c)));
            if (window.showToast) showToast(SM.t(toastKey));
          } };
        }));
      } };
    }
    return [
      sub(SM.t('ctxLinkToWidgetAxisEllipsis'), function (ref) { return ref; }, 'toastLinkedToWidget'),
      sub(SM.t('ctxDriveFromWidgetAxisEllipsis'), function (ref) { return 'self.at(' + ref + ')'; }, 'toastPoseDrivenByWidget'),
    ];
  }
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
    if (_motionKeySel.length < 2) { if (window.showToast) showToast(SM.t('toastSelectAtLeast2Keys')); return; }
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
    if (_motionKeySel.length < 2) { if (window.showToast) showToast(SM.t('toastSelectAtLeast2Keys')); return; }
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
    if (window.showToast) showToast(_motionKeySel.length + SM.t('toastKeysSelectedSuffix'));
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
  // Influence, After-Effects style (2026-08 rework — feedback: "inversé la
  // logique 0% les clé sont éloigné et 100% rapproché", confirmed via
  // follow-up: at 100% "la poignée d'easing s'étire presque jusqu'à toucher
  // l'autre clé"). Previously this only varied the waypoint's Y (how flat
  // the curve sits, at a FIXED x=0.25) — 0%/100% distinguished "linear" from
  // "fully held", but the waypoint's TIME position never moved, so nothing
  // ever visually "reached toward" the other key regardless of the value.
  // Now percent drives the waypoint's X too: 0% keeps it hugging its own
  // key's time (negligible reach — "éloigné" from the other key, the ease
  // barely exists) and 100% pushes it out toward the segment's own midpoint
  // — "rapproché", the ease's flat/held region stretching as far toward the
  // other key as this curve system's structure allows a waypoint to go
  // (the on-curve waypoint model used everywhere here — see DEFAULT_CURVE —
  // has a fixed midpoint at x=0.5 that the out-side waypoint can approach
  // but never cross, and the in-side mirrors it from the other end; MARGIN
  // keeps it strictly short of coinciding with either neighbor, which would
  // degenerate curveSegFor's segment lookup into a zero-width span).
  var EASE_INFLUENCE_MARGIN = 0.02;
  function easeOutPercent(k) {
    var pts = k && k.curvePoints;
    if (!pts || pts.length < 2) return null;
    var p = pts[1];
    if (!p) return 0;
    var amount = (p.x - EASE_INFLUENCE_MARGIN) / (0.5 - 2 * EASE_INFLUENCE_MARGIN);
    return Math.round(Math.max(0, Math.min(1, amount)) * 100);
  }
  function easeInPercent(k) {
    var pts = k && k.curvePoints;
    if (!pts || pts.length < 2) return null;
    var p = pts[pts.length - 2];
    if (!p) return 0;
    var amount = (1 - p.x - EASE_INFLUENCE_MARGIN) / (0.5 - 2 * EASE_INFLUENCE_MARGIN);
    return Math.round(Math.max(0, Math.min(1, amount)) * 100);
  }
  function editableInfluenceCurve(key) {
    var pts = cloneCurvePts(key.curvePoints || CURVE_LINEAR);
    // Linear only has its two endpoints. Promote it to the ordinary
    // five-point representation so either edge can be bent independently
    // while the untouched edge remains exactly linear.
    if (pts.length < 4) pts = [
      { x: 0, y: 0 }, { x: 0.25, y: 0.25 }, { x: 0.5, y: 0.5 },
      { x: 0.75, y: 0.75 }, { x: 1, y: 1 }
    ];
    return pts;
  }
  function setSegmentInfluence(key, side, percent) {
    if (!key) return false;
    var amount = Math.max(0, Math.min(100, Number(percent) || 0)) / 100;
    var reach = EASE_INFLUENCE_MARGIN + amount * (0.5 - 2 * EASE_INFLUENCE_MARGIN);
    var pts = editableInfluenceCurve(key);
    var p;
    if (side === 'out') {
      p = pts[1];
      p.x = reach;
      // Deepens toward the key's own value (y->0) as the reach grows, same
      // "hold near this key, then catch up" shape the old fixed-x version
      // had — just now paired with an X that actually moves too.
      p.y = p.x * (1 - amount);
    } else {
      p = pts[pts.length - 2];
      p.x = 1 - reach;
      p.y = 1 - (1 - p.x) * (1 - amount);
    }
    // A manually specified waypoint tangent would override the visible
    // edge change. This control owns that one edge, so release only that
    // waypoint's tangent and preserve every other custom point untouched.
    delete p.tx; delete p.ty;
    key.hold = false;
    key.curvePoints = pts;
    return true;
  }
  function keyEaseInfluence(holder, prop, key, side) {
    var track = trackFor(holder, prop);
    if (!track) return null;
    var idx = track.keys.indexOf(key);
    if (side === 'out') return idx >= 0 && idx < track.keys.length - 1 ? easeOutPercent(key) : null;
    var prev = idx > 0 ? track.keys[idx - 1] : null;
    return prev ? easeInPercent(prev) : null;
  }
  function setSelectedEaseInfluence(side, value) {
    if (!_motionKeySel.length) return 0;
    pushUndo();
    var n = 0;
    _motionKeySel.forEach(function (s) {
      var track = trackFor(s.holder, s.prop);
      if (!track) return;
      var idx = track.keys.indexOf(s.key);
      var segment = side === 'out'
        ? (idx >= 0 && idx < track.keys.length - 1 ? s.key : null)
        : (idx > 0 ? track.keys[idx - 1] : null);
      if (segment && setSegmentInfluence(segment, side, value)) n++;
    });
    if (n) {
      renderLayerList(); renderTimeline();
      if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    }
    return n;
  }
  function isPrimarySelectedKey(holder, prop, key) {
    if (!isKeySelected(holder, prop, key)) return false;
    if (_keyAnchor) {
      var anchorIsSelected = _motionKeySel.some(function (s) {
        return s.holder === _keyAnchor.holder && s.prop === _keyAnchor.prop && s.key.frame === _keyAnchor.frame;
      });
      if (anchorIsSelected) return _keyAnchor.holder === holder && _keyAnchor.prop === prop && _keyAnchor.frame === key.frame;
    }
    var last = _motionKeySel[_motionKeySel.length - 1];
    return !!last && last.holder === holder && last.prop === prop && last.key === key;
  }
  function buildKeyEaseBox(holder, prop, key, side) {
    var value = keyEaseInfluence(holder, prop, key, side);
    if (value == null) return null;
    var box = document.createElement('label');
    box.className = 'motion-key-ease-box ' + side + (_easeBoxDragging ? ' dragging' : '');
    box.title = side === 'in'
      ? 'Lissage entrant — glisser ou saisir une influence (0–100 %)'
      : 'Lissage sortant — glisser ou saisir une influence (0–100 %)';
    var marker = document.createElement('span');
    marker.className = 'motion-key-ease-marker'; marker.textContent = side === 'in' ? '◀' : '▶';
    var input = document.createElement('input');
    input.type = 'number'; input.className = 'scrub motion-key-ease-input';
    input.min = 0; input.max = 100; input.dataset.step = 1; input.value = value;
    input.addEventListener('mousedown', function (e) { e.stopPropagation(); });
    // Force-visible for the whole drag (2026-08 fix, feedback: "la boite
    // disparait si je drag le nombre dans la box") — the actual scrub drag
    // (ui.js's `.scrub` mechanism) is driven by a DOCUMENT-level `pointerdown`
    // listener, which fires and completes (including its own preventDefault())
    // BEFORE the browser's compatibility `mousedown` is even dispatched — a
    // `pointerdown`'s preventDefault() suppresses that follow-up mousedown
    // entirely, so the class-toggle used to live on 'mousedown' above never
    // actually ran during a real drag (only in synthetic MouseEvent tests,
    // which bypass that chain — the gap this fix closes). Listens on
    // 'pointerdown' here too, WITHOUT stopPropagation, so ui.js's own
    // document-level listener still sees the event and starts the real drag
    // normally — this one only piggybacks to flip the visibility class.
    // One-shot pointerup listener per press, not a persistent one, so
    // nothing accumulates.
    input.addEventListener('pointerdown', function () {
      _easeBoxDragging = true;
      box.classList.add('dragging');
      window.addEventListener('pointerup', function () {
        _easeBoxDragging = false;
        // `box` may have been replaced by a mid-drag re-render (see the
        // module-level flag's own comment) — clearing the class on this
        // possibly-stale reference is harmless either way, the flag is
        // what the NEXT build actually reads.
        box.classList.remove('dragging');
      }, { once: true });
    });
    input.addEventListener('click', function (e) { e.stopPropagation(); });
    input.addEventListener('keydown', function (e) { e.stopPropagation(); });
    input.addEventListener('change', function () {
      if (input.value === '' || !isFinite(parseFloat(input.value))) return;
      var v = Math.max(0, Math.min(100, parseFloat(input.value)));
      input.value = v;
      setSelectedEaseInfluence(side, v);
    });
    if (side === 'in') { box.appendChild(input); box.appendChild(marker); }
    else { box.appendChild(marker); box.appendChild(input); }
    return box;
  }
  function colorSelectedKeys(color) {
    if (!_motionKeySel.length) { if (window.showToast) showToast(SM.t('toastNoKeySelected')); return; }
    pushUndo();
    _motionKeySel.forEach(function (s) { if (color) s.key.color = color; else delete s.key.color; });
    renderTimeline();
    if (window.showToast) showToast(color ? (_motionKeySel.length + SM.t('toastKeysColoredSuffix')) : SM.t('toastColorRemoved'));
  }
  function subdivideKeys() {
    if (_motionKeySel.length < 2) { if (window.showToast) showToast(SM.t('toastSelectAtLeast2Keys')); return; }
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
    if (window.showToast) showToast(added + SM.t('toastKeysInsertedSuffix') + (skipped ? ' — ' + skipped + ' intervalle(s) trop court(s)' : ''));
  }
  // Keep a random subset of the current selection (Skew Pro's "Grab
  // Randomly"): the fast way to make a uniform batch of layers/keys feel
  // hand-made. Guarantees at least one key survives, so it can't silently
  // empty the selection on a small one.
  function grabRandomKeys(percent) {
    var p = Math.max(1, Math.min(100, parseInt(percent, 10) || 50)) / 100;
    if (!_motionKeySel.length) { if (window.showToast) showToast(SM.t('toastNoKeySelected')); return; }
    var pool = _motionKeySel.slice();
    var want = Math.max(1, Math.round(pool.length * p));
    var out = [];
    while (out.length < want && pool.length) out.push(pool.splice(Math.floor(Math.random() * pool.length), 1)[0]);
    setKeySel(out);
    renderTimeline();
    if (window.showToast) showToast(out.length + SM.t('toastKeysKeptRandomSuffix'));
  }
  // Inverts within whatever tracks are CURRENTLY RENDERED (the same
  // universe the marquee itself draws over).
  function invertKeySelection() {
    var all = [], prevSel = _motionKeySel;
    // feedback #221 ("impossible de sélectionner avec rec de sélection les
    // keyframes comme les autres propriétés") — .motion-group-row also
    // covers renderDiscreteKeyGridRow's Blend/Parent rows (feedback #214),
    // tagged with the same _smHolder/_smProp convention; an UNTAGGED
    // group-row (a plain section header/divider) safely no-ops on the
    // `if (!holder) return` guards already below, so widening this scan
    // costs nothing for every other kind of .motion-group-row.
    document.querySelectorAll('.motion-track-row, .motion-group-row').forEach(function (rowEl) {
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
    // feedback #221 — see invertKeySelection's identical comment just above.
    document.querySelectorAll('.motion-track-row, .motion-group-row').forEach(function (rowEl) {
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
      edge.title = SM.t('titleSkewKeysHint');
      edge.addEventListener('mousedown', function (e) {
        e.stopPropagation(); e.preventDefault();
        // .grabbing thickens the edge bar a touch further than plain hover
        // (2026-08-16, Cyril: "quand on hover et grab grossis un peu") —
        // self-contained rather than threaded through startSkewDrag/onDragUp,
        // same add-on-mousedown/remove-on-mouseup convention ui.js already
        // uses for its own resize handles (lpr/tpr/ppr .active).
        edge.classList.add('grabbing');
        document.addEventListener('mouseup', function () { edge.classList.remove('grabbing'); }, { once: true });
        onStart(e, pos);
      });
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
      edge.title = SM.t('titleSpaceKeysHint');
      edge.addEventListener('mousedown', function (e) {
        e.stopPropagation(); e.preventDefault();
        edge.classList.add('grabbing');
        document.addEventListener('mouseup', function () { edge.classList.remove('grabbing'); }, { once: true });
        onStart(e, pos);
      });
      boxEl.appendChild(edge);
    });
  }
  function addMoveFill(boxEl, onStart) {
    var fill = document.createElement('div');
    fill.className = 'motion-keysel-fill';
    fill.title = SM.t('titleMoveAllSelectedKeysHint')
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
    window._motionSkewDrag = { startX: e.clientX, startScrollLeft: motionDragScrollLeft(), mode: mode, rows: rows, fMin: fMin, fMax: fMax, grabFrame: grabFrame };
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
        if (!(f1 > f0)) { if (window.showToast) showToast(SM.t('toastSelectKeysOn2DifferentFramesToSpace')); return; }
        startSkewDrag(rows, mode, e);
      });
      addStaggerEdges(_keySelBoxEl, function (e, mode) {
        var rows = buildKeyRows();
        if (rows.length < 2) { if (window.showToast) showToast(SM.t('toastSelectKeysOn2TracksToSkew')); return; }
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
    if (window.showToast) showToast(SM.t('toastNoKeyOnLayersHint'));
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
        if (!(f1 > f0)) { if (window.showToast) showToast(SM.t('toastNeedKeysOn2DifferentFrames')); return; }
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
    if (!ld.symbolId) { if (window.showToast) showToast(SM.t('toastTimeRemapAppliesToComponents')); return false; }
    var sym = state.symbols[ld.symbolId];
    if (!sym) return false;
    if (ld.timeRemap) { if (window.showToast) showToast(SM.t('toastTimeRemapAlreadyActive')); return false; }
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
    if (window.showToast) showToast(SM.t('toastTimeRemapEnabledFromSuffix') + last);
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
    if (window.showToast) showToast(SM.t('toastTimeRemapDisabled'));
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
  // Cheap LIVE visual preview during a layer-bar/in-out drag (layer-inout.js
  // calls this from its mousemove, once per event) — 2026-07-30, Cyril:
  // "les clé ne bouge pas en temp réel avec calque ou in/outpoint". The
  // actual data commit (shiftLayerMotionKeys/shiftKeySelection below) still
  // only runs ONCE, at drop — same "cheap live visual, expensive commit at
  // drop" split as updateBar's own bar-position preview, not a live
  // shiftLayerMotionKeys call on every mousemove (which touches ld.motion,
  // ld.elementMotion, ld.effects and per-element effects across the whole
  // layer — see that function's own header comment on how much it walks —
  // and would reintroduce exactly the per-mousemove-rebuild cost CLAUDE.md
  // §5bis already paid down once for this same timeline).
  //
  // 'layer' mode only moves diamonds in tracks belonging to `li` (its own
  // holder plus each per-element holder) that are CURRENTLY RENDERED —
  // collapsed/hidden rows have no diamonds on screen to preview, and the
  // drop-time commit (which touches every track regardless of visibility)
  // is unaffected by that scoping. 'selected' mode ignores `li` and moves
  // whatever's already tagged .sel, mirroring shiftKeySelection's own
  // target. Purely a transform on existing DOM nodes — the next real
  // renderTimeline() (drop, or anything else that rebuilds the grid) throws
  // these nodes away, so nothing needs undoing on the data side.
  function previewKeyframeShift(li, dxFrames, mode) {
    if (!dxFrames) return;
    var px = Math.round(dxFrames * FC);
    var holders = null;
    if (mode !== 'selected') {
      var ld = state.layers[li];
      if (!ld) return;
      holders = [ld];
      if (ld.elementMotion) Object.keys(ld.elementMotion).forEach(function (k) { holders.push(ld.elementMotion[k]); });
    }
    document.querySelectorAll('#frame-grid .motion-track-row').forEach(function (rowEl) {
      var dias;
      if (mode === 'selected') {
        dias = rowEl.querySelectorAll('.motion-key.sel');
      } else {
        if (holders.indexOf(rowEl._smHolder) < 0) return;
        dias = rowEl.querySelectorAll('.motion-key');
      }
      // transition:none (2026-08-27, "décalage... comme un lag" — reported
      // again after the connector/box resync fix above): .motion-key has a
      // CSS transition on `transform` (for its select/interp-type scale-
      // rotate animations), so writing style.transform here on every
      // mousemove made the diamond EASE toward the cursor over 80ms instead
      // of snapping — visibly trailing behind the (transition-free) green
      // connector bar between two diamonds it's supposed to move in lockstep
      // with. clearKeyframeShiftPreview restores it at drag end/start.
      dias.forEach(function (d) { d.style.transition = 'none'; d.style.transform = 'translateX(' + px + 'px)'; });
      // SVG connector bars + duration blocks (trackRowHtml) must track the
      // diamonds live too — they were the one element family this preview
      // skipped, so they visibly froze mid-drag while everything else moved
      // ("la barre bleue entre les keyframes ne bouge pas en temps réel").
      // 'layer' mode: every key in the row shifts by the same dx, so every
      // connector/block shifts identically — no per-rect check needed.
      // 'selected' mode: a connector only moves rigidly when BOTH its
      // endpoint keys are selected (a mixed pair would need a stretch, not
      // a translate — left as-is, same as before, strictly no worse); a
      // duration block moves when its own key is selected.
      var rects = rowEl.querySelectorAll('.motion-key-connect, .motion-key-durblock');
      if (!rects.length) return;
      if (mode === 'selected') {
        var track = trackFor(rowEl._smHolder, rowEl._smProp);
        if (!track || !track.keys.length) return;
        rects.forEach(function (r) {
          var sel;
          // classList.contains, NOT getAttribute('class')===… (2026-08-29
          // fix, feedback #149: "si je select une keyframe plus un in/out
          // point... la barre verte de suit pas pendant le drag"). A
          // connector whose BOTH endpoints are already selected renders
          // with class "motion-key-connect sel" (see its own build-time
          // `.sel` suffix a few hundred lines up) — the strict `===` check
          // here only ever matched the UNselected/single-line-class case,
          // so a fully-selected connector (the exact state a click on the
          // connector itself, or a marquee over both diamonds, produces)
          // fell into the durblock `else` branch instead. There it read a
          // nonexistent `data-ki` (connectors only ever carry `data-i`),
          // `+null` coerced to 0, and by sheer coincidence checked
          // track.keys[0]'s selection instead of its own two endpoints —
          // reproduced live: a 3-key track with only keys[1]/keys[2]
          // selected (their connector IS `.sel`) dragged an in-point handle
          // with that selection active; both diamonds translated live but
          // the connector between them stayed at transform:'' the whole
          // drag, visibly detaching from the diamonds it connects, though
          // the data committed correctly at drop either way (this was a
          // live-preview-only bug, not a data bug).
          if (r.classList.contains('motion-key-durblock')) {
            var ki = +r.getAttribute('data-ki');
            sel = track.keys[ki] && isKeySelected(rowEl._smHolder, rowEl._smProp, track.keys[ki]);
          } else {
            var i = +r.getAttribute('data-i');
            sel = track.keys[i] && track.keys[i + 1] && isKeySelected(rowEl._smHolder, rowEl._smProp, track.keys[i]) && isKeySelected(rowEl._smHolder, rowEl._smProp, track.keys[i + 1]);
          }
          if (sel) r.style.transform = 'translateX(' + px + 'px)';
        });
      } else {
        rects.forEach(function (r) { r.style.transform = 'translateX(' + px + 'px)'; });
      }
    });
    // Multi-row selection box (2026-08-27, "y a un decalage not in real
    // time des keyframe par rapport au layer ou inpoint"): the diamonds and
    // connectors above already track the drag live (per the header comment
    // — this was already fixed once for those), but `_keySelBoxEl` was only
    // ever recomputed from a full renderTimeline() or setKeySel(), so it
    // stayed glued to its pre-drag rect while the diamonds it's drawn
    // around visibly moved out from under it. updateKeySelectionBox()
    // re-measures straight from the (now-translated) `.motion-key.sel`
    // rects, so calling it here is a correct, self-contained resync — it
    // no-ops instantly when the box isn't currently shown.
    updateKeySelectionBox();
  }
  // Called once at drag START (before the first preview) and at drop —
  // stray transforms must never survive past either boundary: a fresh drag
  // that never qualifies for a preview (e.g. an out-drag, never retimes)
  // must not inherit a PRIOR drag's leftover offset, and a drop always
  // rebuilds via renderTimeline() but only on the branches that actually
  // reach it (defensive here rather than trusting every return path does).
  function clearKeyframeShiftPreview() {
    document.querySelectorAll('#frame-grid .motion-key, #frame-grid .motion-key-connect, #frame-grid .motion-key-durblock').forEach(function (d) { if (d.style.transform) d.style.transform = ''; if (d.style.transition) d.style.transition = ''; });
  }
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
      // propsFor(h), not the base PROPS (2026-07-30 fix, found while auditing
      // the Duplicator alignment bug — same "PROPS instead of propsFor"
      // omission, just in retiming instead of rendering): a 3D/duplicator/
      // multi-parent-blend/timeLink holder has keyframe tracks beyond the
      // base 5, and PROPS alone silently left them at their pre-drag frames
      // — exactly the "retimed in one reader, stale in the others" bug this
      // function's own header comment warns about.
      propsFor(h).forEach(function (prop) { shiftTrack(h.motion[prop]); });
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
    // Per-ELEMENT effects (effects-panel.js's effectsTarget, single-shape-
    // selected case) live on the stroke's own dict (sd.effects), not in any
    // per-layer aggregate the way ld.elementMotion is — confirmed live this
    // gap existed: dragging a layer's in/out bar retimed ld.effects and
    // ld.elementMotion but left a keyed per-element effect's keys at their
    // original frames, exactly the "retimed in one reader, stale in the
    // others" shape this function's own header comment warns about, just
    // missing this one target. desP assigns p.data.effects=d.effects BY
    // REFERENCE (app.js), so the live selected element's view updates for
    // free once its underlying stored dict is shifted here — no separate
    // live-object pass needed. Dedupe by array identity: a held span's
    // frames all reference the SAME stored dict object (CLAUDE.md §5quater),
    // so shifting per-frame without this would shift that one shared array
    // once per held frame that references it, not once.
    if (ld.frames) {
      var seenEffArrays = new WeakSet();
      ld.frames.forEach(function (f) {
        if (!f || !f.strokes) return;
        f.strokes.forEach(function (sd) {
          if (!sd || !sd.effects || seenEffArrays.has(sd.effects)) return;
          seenEffArrays.add(sd.effects);
          sd.effects.forEach(function (eff) {
            if (eff && eff.keys) Object.keys(eff.keys).forEach(function (p) { shiftTrack(eff.keys[p]); });
          });
        });
      });
    }
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

  // Key drags can emit well above the display refresh rate. Their collision
  // and retiming logic below updates the data synchronously, but rebuilding
  // the entire Motion grid for every mousemove only paints intermediate
  // states the screen can never display. Coalesce the DOM work to one rebuild
  // per animation frame and flush the latest state on pointer release.
  var _motionDragTimelineRaf = 0;
  function requestMotionDragTimelineRender() {
    if (_motionDragTimelineRaf) return;
    _motionDragTimelineRaf = requestAnimationFrame(function () {
      _motionDragTimelineRaf = 0;
      renderTimeline();
    });
  }
  function flushMotionDragTimelineRender() {
    // 2026-08 fix (feedback: "aprés je bouge la keyframe je n'arrive pas à
    // revoir la box de lissage") — this used to early-return when there was
    // no PENDING rAF, which is the common case: a drag lasting more than one
    // frame (i.e. any real drag) already has its last mid-drag rAF fire and
    // reset _motionDragTimelineRaf to 0 well before mouseup, so by the time
    // onDragUp calls this, there's nothing to "flush" and it did nothing at
    // all — leaving the grid stuck on whatever it looked like at that last
    // mid-drag frame, rendered while window._motionKeyDrag was still truthy
    // (suppressing the ease boxes, see the isPrimarySelectedKey gate above).
    // Clearing the drag flags in onDragUp right before this call never
    // triggered the render needed to pick that back up. This function's own
    // name/comment ("flush the latest state on pointer release") always
    // meant to render unconditionally here — only the early-return was wrong.
    if (_motionDragTimelineRaf) { cancelAnimationFrame(_motionDragTimelineRaf); _motionDragTimelineRaf = 0; }
    renderTimeline();
  }

  function motionDragScrollLeft() {
    var wrap = document.getElementById('fg-wrap');
    return wrap ? wrap.scrollLeft : 0;
  }
  var _motionAutoScrollRaf = 0;
  var _motionAutoScrollEvent = null;
  function motionDragIsActive() {
    return !!(window._motionKeyDrag || window._motionConnectDrag || window._motionSkewDrag);
  }
  function stopMotionAutoScroll() {
    if (_motionAutoScrollRaf) cancelAnimationFrame(_motionAutoScrollRaf);
    _motionAutoScrollRaf = 0; _motionAutoScrollEvent = null;
  }
  function autoScrollMotionDrag(e) {
    var wrap = document.getElementById('fg-wrap');
    if (!wrap) return;
    var r = wrap.getBoundingClientRect(), edge = 34, step = 0;
    if (e.clientX < r.left + edge) step = -Math.ceil((r.left + edge - e.clientX) / 3);
    else if (e.clientX > r.right - edge) step = Math.ceil((e.clientX - (r.right - edge)) / 3);
    if (!step) { stopMotionAutoScroll(); return; }
    var before = wrap.scrollLeft;
    wrap.scrollLeft = Math.max(0, Math.min(wrap.scrollWidth - wrap.clientWidth, before + step));
    if (wrap.scrollLeft === before) { stopMotionAutoScroll(); return; }
    _motionAutoScrollEvent = {
      clientX: e.clientX, clientY: e.clientY,
      metaKey: !!e.metaKey, ctrlKey: !!e.ctrlKey, shiftKey: !!e.shiftKey, altKey: !!e.altKey,
    };
    if (!_motionAutoScrollRaf) {
      _motionAutoScrollRaf = requestAnimationFrame(function tickMotionAutoScroll() {
        _motionAutoScrollRaf = 0;
        if (!motionDragIsActive() || !_motionAutoScrollEvent) { stopMotionAutoScroll(); return; }
        // Re-enter the same absolute drag calculation after the viewport has
        // moved. If the pointer stays at the edge this schedules the next
        // frame, so scrolling continues without requiring fresh mousemove
        // events from the OS.
        onDragMove(_motionAutoScrollEvent);
      });
    }
  }
  function motionDragDeltaFrames(e, drag) {
    autoScrollMotionDrag(e);
    return Math.round((e.clientX - drag.startX + motionDragScrollLeft() - (drag.startScrollLeft || 0)) / FC);
  }
  function collectMotionSnapCandidates(excludedKeys) {
    var byFrame = Object.create(null);
    function add(frame, label) {
      frame = Math.round(Number(frame));
      if (frame < 0 || frame >= state.totalFrames || byFrame[frame]) return;
      byFrame[frame] = label;
    }
    add(state.currentFrame, 'Tête de lecture');
    add(state.waIn, 'Début zone'); add(state.waOut, 'Fin zone');
    (state.markers || []).forEach(function (m) { add(m.frame, m.name || 'Repère'); });
    function addTrack(track) {
      if (!track || !track.keys) return;
      track.keys.forEach(function (k) { if (!excludedKeys || excludedKeys.indexOf(k) < 0) add(k.frame, 'Clé'); });
    }
    function addHolder(holder) {
      if (!holder) return;
      Object.keys(holder.motion || {}).forEach(function (prop) { addTrack(holder.motion[prop]); });
    }
    state.layers.forEach(function (ld) {
      addHolder(ld); addTrack(ld.timeRemap);
      (ld.markers || []).forEach(function (m) { add(m.frame, m.name || 'Repère calque'); });
      Object.keys(ld.elementMotion || {}).forEach(function (id) { addHolder(ld.elementMotion[id]); });
      (ld.effects || []).forEach(function (eff) { Object.keys((eff && eff.keys) || {}).forEach(function (p) { addTrack(eff.keys[p]); }); });
    });
    if (state.bpmShow && state.bpm > 0 && state.fps > 0) {
      var beat = state.fps * 60 / state.bpm, off = Number(state.bpmOffset) || 0;
      for (var f = off; f < state.totalFrames; f += beat) add(f, 'Temps BPM');
    }
    return Object.keys(byFrame).map(function (f) { return { frame: Number(f), label: byFrame[f] }; });
  }
  function showMotionSnapGuide(frame, label) {
    var wrap = document.getElementById('fg-wrap');
    if (!wrap) return;
    var guide = document.getElementById('motion-snap-guide');
    if (!guide) { guide = document.createElement('div'); guide.id = 'motion-snap-guide'; guide.className = 'motion-snap-guide'; wrap.appendChild(guide); }
    guide.style.left = (frame * FC + FC / 2) + 'px';
    guide.dataset.label = (label || 'Aligné') + ' · ' + (frame + 1);
  }
  function clearMotionSnapGuide() {
    var guide = document.getElementById('motion-snap-guide');
    if (guide) guide.remove();
  }
  function snapMotionFrame(frame, excludedKeys, e) {
    clearMotionSnapGuide();
    if (!_motionSnapEnabled || (e && (e.metaKey || e.ctrlKey))) return frame;
    var threshold = Math.max(1, Math.ceil(8 / Math.max(1, FC)));
    var best = null;
    collectMotionSnapCandidates(excludedKeys).forEach(function (c) {
      var dist = Math.abs(c.frame - frame);
      if (dist <= threshold && (!best || dist < best.dist)) best = { frame: c.frame, label: c.label, dist: dist };
    });
    if (!best) return frame;
    showMotionSnapGuide(best.frame, best.label);
    return best.frame;
  }

  // Drag-to-retime a keyframe (mousemove/up delegated from ui.js's global
  // pointer handlers via SMMotion.onDragMove/onDragUp, same pattern as the
  // span-end/keyframe drag handlers already in timeline.js).
  function onDragMove(e) {
    if (onLayerSkewMove(e)) return;
    var cd = window._motionConnectDrag;
    if (cd) {
      // Collective clamp, same rule for both gestures (spec: "une clé ne
      // franchit jamais une voisine restée fixe") — the neighbor OUTSIDE
      // the pair (the key before A, the key after B) never moves, so
      // whichever endpoint is actually dragging this frame is bounded by
      // it. Re-read the track fresh every move rather than snapshotting
      // prev/next at mousedown: sortKeys() below can shuffle indices if a
      // drag ever lands exactly on a neighbor's frame (blocked below, but
      // cheap enough to just always re-derive).
      var track = trackFor(cd.ld, cd.prop);
      var ia = track.keys.indexOf(cd.a), ib = track.keys.indexOf(cd.b);
      var prevKey = ia > 0 ? track.keys[ia - 1] : null;
      var nextKey = ib >= 0 && ib < track.keys.length - 1 ? track.keys[ib + 1] : null;
      var deltaFrames = motionDragDeltaFrames(e, cd);
      if (cd.retime) {
        // First key stays planted; only the second moves — stretches or
        // compresses the segment in place. Bounded by A itself (can never
        // reach or cross it) and by the next fixed key beyond B, if any.
        var lo = cd.startAFrame + 1;
        var hi = Math.min(state.totalFrames - 1, nextKey ? nextKey.frame - 1 : state.totalFrames - 1);
        var nb = Math.max(lo, Math.min(hi, cd.startBFrame + deltaFrames));
        nb = Math.max(lo, Math.min(hi, snapMotionFrame(nb, [cd.a, cd.b], e)));
        if (nb === cd.b.frame) return;
        cd.b.frame = nb; sortKeys(track);
      } else {
        // Both endpoints move together by the SAME delta (duration
        // constant) — clamp is the intersection of what each endpoint can
        // individually tolerate against ITS OWN outside neighbor.
        var loD = prevKey ? (prevKey.frame + 1 - cd.startAFrame) : -cd.startAFrame;
        var hiD = Math.min(
          state.totalFrames - 1 - cd.startBFrame,
          nextKey ? (nextKey.frame - 1 - cd.startBFrame) : Infinity
        );
        var d = Math.max(loD, Math.min(hiD, deltaFrames));
        var snappedA = snapMotionFrame(cd.startAFrame + d, [cd.a, cd.b], e);
        d = Math.max(loD, Math.min(hiD, snappedA - cd.startAFrame));
        if (cd.a.frame === cd.startAFrame + d && cd.b.frame === cd.startBFrame + d) return;
        cd.a.frame = cd.startAFrame + d; cd.b.frame = cd.startBFrame + d;
        sortKeys(track);
      }
      requestMotionDragTimelineRender();
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
      return;
    }
    updateMarquee(e);
    var sk = window._motionSkewDrag;
    if (sk) {
      // One engine for all three box gestures (see startSkewDrag's header):
      // per-row fraction × TOTAL drag distance, applied to each key's
      // ORIGINAL frame captured at mousedown — absolute, not incremental,
      // so fractional middle rows land exactly on round(fraction × total)
      // with zero drift, and dragging back to the start restores every key
      // to its exact original frame.
      autoScrollMotionDrag(e);
      var total = (e.clientX - sk.startX + motionDragScrollLeft() - (sk.startScrollLeft || 0)) / FC;
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
      requestMotionDragTimelineRender();
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
    var deltaFrames = motionDragDeltaFrames(e, d);
    if (d.group) {
      // Whole-group move: compute the delta from ONE reference key (the
      // first), then check EVERY selected key can land there without
      // colliding with an UNselected key already at the target frame —
      // an all-or-nothing move keeps the group's relative spacing intact
      // rather than silently dropping just the colliding member.
      var movedKeys = d.keys.map(function (s) { return s.key; });
      var rawDeltaFrames = deltaFrames;
      if (d.keys.length) deltaFrames = snapMotionFrame(d.keys[0].origFrame + deltaFrames, movedKeys, e) - d.keys[0].origFrame;
      function groupCanMove(by) {
        return d.keys.every(function (s) {
          var target = s.origFrame + by;
          if (target < 0 || target >= state.totalFrames) return false;
          var existing = keyAt(trackFor(s.holder, s.prop), target);
          return !existing || existing === s.key || movedKeys.indexOf(existing) >= 0;
        });
      }
      var ok = groupCanMove(deltaFrames);
      if (!ok && deltaFrames !== rawDeltaFrames && groupCanMove(rawDeltaFrames)) {
        clearMotionSnapGuide(); deltaFrames = rawDeltaFrames; ok = true;
      }
      if (!ok) return;
      var changed = d.keys.some(function (s) { return s.key.frame !== s.origFrame + deltaFrames; });
      if (!changed) return;
      d.keys.forEach(function (s) { s.key.frame = s.origFrame + deltaFrames; sortKeys(trackFor(s.holder, s.prop)); });
      requestMotionDragTimelineRender();
      if (window.SMEngineBridge) SMEngineBridge.renderNow(); // live stage feedback — see skew-drag branch's own comment above
      return;
    }
    var nf = Math.max(0, Math.min(state.totalFrames - 1, d.startFrame + deltaFrames));
    var unsnappedNf = nf;
    nf = snapMotionFrame(nf, [d.key], e);
    if (nf === d.key.frame) return;
    var track = trackFor(d.ld, d.prop);
    if (keyAt(track, nf)) {
      // A magnetic target occupied by another key must not create a dead
      // zone around it: keep the nearest free raw frame instead.
      clearMotionSnapGuide();
      nf = unsnappedNf;
      if (nf === d.key.frame || keyAt(track, nf)) return;
    }
    d.key.frame = nf; sortKeys(track);
    requestMotionDragTimelineRender();
    if (window.SMEngineBridge) SMEngineBridge.renderNow(); // live stage feedback — see skew-drag branch's own comment above
  }
  function onDragUp() {
    if (onLayerSkewUp()) return;
    stopMotionAutoScroll();
    endMarquee();
    if (window._motionSkewDrag) { window._motionSkewDrag = null; clearMotionSnapGuide(); flushMotionDragTimelineRender(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); return; }
    if (window._motionConnectDrag) { window._motionConnectDrag = null; clearMotionSnapGuide(); document.body.style.cursor = ''; flushMotionDragTimelineRender(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); return; }
    if (!window._motionKeyDrag) return;
    window._motionKeyDrag = null;
    clearMotionSnapGuide();
    flushMotionDragTimelineRender();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }
  document.addEventListener('mousemove', onDragMove);
  document.addEventListener('mouseup', onDragUp);

  // ---- mode switching ----
  function setAppMode(mode) {
    if (state.appMode === mode) return;
    // StoryBoard freeze (2026-08, PR #209: "il faudrait mettre en freeze
    // (in Dev) StoryBoard") — single choke point so the button click,
    // any keyboard shortcut, and the Nemo scripting API's SMMotion.setAppMode
    // (nemo-script.js) all get the same refusal instead of three separate
    // gates that could drift out of sync.
    if (mode === 'storyboard' && window.SM_FROZEN_IN_DEV && window.SM_FROZEN_IN_DEV.storyboard) {
      if (window.showToast) showToast((window.SM&&SM.t)?SM.t('storyboardFrozenToast'):'StoryBoard — in development, not yet available in this build');
      return;
    }
    // Every other risky action while inside a Component (convertLayerToComponent,
    // mergeLayersIntoOne, splitLayerIntoElements, enterSymbol itself, etc. — app.js
    // lines 2276/2318/2410/2651/2674/2699/2741/2806/2896/2942) refuses with this
    // same toast rather than proceeding — the Anim2D/Motion/StoryBoard toggle had
    // no such guard, the only one of the ~10 call sites missing it (2026-07-30
    // fix, found via workflow investigation of Cyril's report: "j'ai un montage
    // dans motion, je dessine dans anim2D et reviens dans motion et là si je
    // scrub mes clé disparaissent"). Without it, double-clicking a Component to
    // enter it (the documented "precomp" gesture, CLAUDE.md §8) and then reaching
    // for the top toggle instead of the "Scene" tab silently left the user
    // editing the SYMBOL's own isolated state.layers/userLayers while every
    // panel/button relabelled itself as if they were back in the outer scene —
    // confirmed live: activeSymbolId stayed non-null after clicking Motion
    // directly, drawing there landed in the symbol's own frames untouched, and
    // exiting via "Scene" afterward left the outer scene's OTHER layer with
    // genuinely corrupted stroke data (27 strokes materializing on a frame that
    // held 2) — real, permanent data loss/corruption, not just a stale render.
    // "Scene" (exitToScene) remains the one way out of a symbol everywhere else
    // in the app.
    //
    // 2026-08-21 (feedback #47, "il faudrait pouvoir le faire"): re-permitted,
    // with the same defensive save enterSymbol/exitToScene themselves take at
    // their own transition points. state.layers/userLayers are ALIASED to the
    // symbol's own arrays while activeSymbolId is set (enterSymbol) — every
    // consumer below this point (renderLayerList/renderTimeline/renderNow)
    // reads whichever object is CURRENTLY state.layers, so it already renders
    // the symbol's own content correctly in either mode; nothing from here to
    // the end of this function reaches into the outer scene. The corruption
    // this guard was built to stop traced to aliasing gaps in enterSymbol/
    // exitToScene themselves (sym.layers/markers/motionArcs/tweenOverrides/
    // tweenEasing/cameraKeys not written back on exit if something replaced
    // rather than mutated them) — those were fixed in the following weeks
    // (2026-07-25, 2026-07-30) and are unconditionally in effect by the time
    // exitToScene runs, regardless of which mode was active while inside.
    // saveAllLayerFrames() here is the same belt-and-suspenders enterSymbol
    // takes on its own way in, so a save lands before the mode's own render
    // pass runs rather than depending on some earlier caller having already
    // flushed the live canvas into ld.frames.
    if (state.activeSymbolId) saveAllLayerFrames();
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
      window._motionRevealedElementLayers = null;
      removeKeySelectionBox();
      removeLayerStaggerBox();
      // The graph editor hides #frame-grid while it's open — leaving Motion
      // with it still on would strand the 2D timeline invisible.
      if (window.SMMotionGraph && SMMotionGraph.isOn()) SMMotionGraph.toggle(false);
      // _layerSel (Motion's own multi-layer selection) has no equivalent UI
      // in Animation 2D, so nothing visually hints it's still set — found
      // live: leaving Motion with a layer selected there left insertFrame()/
      // removeFrame() (app.js) silently targeting THAT stale selection
      // instead of the active layer, and refusing outright with no visible
      // cause whenever the stale selection happened to be a locked layer
      // (e.g. a Component instance). Same "start fresh" precedent as the
      // other Motion-only transient state cleared just above.
      _layerSel = [];
    }
    state.appMode = mode;
    // Workspace continuity (2026-08, "retrouver son workspace comme il a
    // quitté") — remembered independently of any one project's saved JSON:
    // a fresh/new project should still open in whichever of StoryBoard/
    // Animation 2D/Motion the user was last working in, not always reset to
    // Animation 2D's hardcoded default.
    try { localStorage.setItem('nemo-app-mode', mode); } catch (e) {}
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
  //   #motion-graph-resize   — the Motion Graph Editor's own height-resize
  //                          handle (motion-graph.js, 2026-07-30). Same
  //                          capture-order bug as every entry above: its
  //                          own mousedown listener lives in the bubble
  //                          phase, so it was silently swallowed here
  //                          before ever firing (confirmed live —
  //                          elementFromPoint correctly found the handle,
  //                          but dispatchEvent returned false with zero
  //                          trace of the handle's own listener ever
  //                          running). Motion-graph.js's OWN document-level
  //                          capture listener (onDown, for .mg-key/.mg-ease)
  //                          runs BEFORE this one and could stop it the
  //                          same way it already does for those two
  //                          classes, but this handle isn't a key or an
  //                          ease waypoint — excluding it here, alongside
  //                          every other "has its own gesture" element,
  //                          is the consistent fix.
  // (historical) Deliberate trade-off: dragging a bar's BODY no longer
  // moved the layer range while in Motion — same priority call as the
  // canvas, where drag-anywhere marquee wins over object-move unless you
  // grab an actual object. Superseded by the entry above.
  function initGridMarquee() {
    var wrap = document.getElementById('fg-wrap');
    if (!wrap) return;
    wrap.addEventListener('mousedown', function (e) {
      if (state.appMode !== 'motion' || e.button !== 0) return;
      // .motion-key-connect (task #101): the SVG connector rect lives in its
      // own overlay <svg>, not inside a .fc.motion-fc cell like the diamonds
      // it sits between — without its own exemption here, this capture-phase
      // marquee-starter swallowed every mousedown on it before the rect's
      // own listener (trackRowHtml) ever saw the event, silently turning a
      // connector click/drag into a marquee-select instead. Found via a live
      // dispatchEvent trace: stopPropagation/preventDefault both firing from
      // THIS function on every attempt, zero trace of the rect's own
      // listener ever running.
      //
      // .layer-reorder-grip (feedback #137, found live while investigating
      // "difficile d'attraper le in point... interfère avec le déplacement
      // de layer en index"): the grip (timeline.js, installLayerReorderGrip)
      // was added 2026-08-24 with its OWN mousedown/pointerdown listener but
      // was never added to THIS exemption list — same missing-exemption bug
      // as every entry above, just in a different file, so it was easy to
      // miss. The practical effect was worse than "hard to grab": since this
      // CAPTURE-phase listener always runs first and this class wasn't
      // exempted, EVERY mousedown on the grip was swallowed into a marquee-
      // select before the grip's own handler ever ran — verified live via
      // dispatchEvent trace (grip's own listener never fired, not even
      // once) and by dragging the grip a full 65px vertically, which is
      // normally more than enough to reorder, and nothing moved. So
      // dragging the grip to reorder a layer directly in the frame-grid
      // didn't actually work AT ALL, frame 0 or not — only the layer-list
      // panel's own row-drag (a separate, unaffected code path) did. The
      // SAME-DAY fix for the frame-0 handle-vs-grip overlap (dd7a202) was
      // therefore built and shipped on top of a handler that could never
      // run in the first place. Exempting the grip here is what actually
      // makes both that fix and normal grid-side reordering reachable.
      if (e.target.closest('.fc.motion-fc, .layer-inout-handle, .layer-inout-key, .layer-inout-bar, .motion-key-connect, .layer-reorder-grip, #frame-hdr, #playhead-flag, #bars-row, #motion-graph-resize')) return;
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
    // The PRE-expression value of a property — the keyframed/static curve
    // an expression is overriding. The graph editor plots both (see
    // motion-graph.js): without this it could only ever draw the result,
    // with no way to see what the expression is standing on top of.
    rawValueAtFrame: rawValueAtFrame,
    hasExpression: hasExpr,
    // Expression controls (2026-08-30) — see propsFor's own comment.
    exprControls: controlsOf,
    controlTypeOf: controlTypeOf,
    addExprControl: addExprControl,
    renameExprControl: renameExprControl,
    removeExprControl: removeExprControl,
    moveExprControl: moveExprControl,
    openExprControlsMenu: openExprControlsMenu,
    // Called on load/import so a project's controls have their PROP_LABEL/
    // DIM/UNIT/DEFAULT registered even before anything renders a row.
    registerControlPropMeta: registerControlPropMeta,
    // motion-preset-picker.js: refresh the Transform group's displayed
    // values right after overwriting ld.motion/motionStatic wholesale
    // (applying a preset), same as any other bulk Motion mutation.
    renderMotionPropsPanel: renderMotionPropsPanel,
    // Parent in Time on-timeline connector (2026-07-30, Van Dijk 2.1) —
    // layer-inout.js calls this directly so the 3 on-bar anchor points
    // (in/out/whole layer) share the exact same drag/cycle-check/link-
    // creation logic as the original side-panel "Temps" pickwhip.
    startTimeLinkPickwhip: startTimeLinkPickwhip,
    // AE's Numpad-Enter "open selected precomp" (timeline.js onKeyDown) —
    // was previously reachable only by double-clicking the Motion row.
    enterComponentLayer: enterComponentLayer,
    // Bar-side Shift/Ctrl selection → layer-list mirror (layer-inout.js
    // onDown calls this; see the function's own comment).
    syncLayerSelFromBarSel: syncLayerSelFromBarSel,
    // Menu-based Parent-in-Time creation (timeline.js buildTimeLinkMenuItems
    // — same core-setter split as setLayerParent/buildParentMenuItems).
    setLayerTimeLink: setLayerTimeLink,
    timeLinkWouldCycle: timeLinkWouldCycle,
    // Group/shape tree (2026-07-31) — mode-agnostic, reused by Animation
    // 2D's own layer-row shape list (timeline.js) so both modes' trees can
    // never diverge in content or z-order.
    buildShapeTree: buildShapeTree,
    selectShapesByStrokeIds: selectShapesByStrokeIds,
    elementLabel: elementLabel,
    layerElements: layerElements,
    elementLabel: elementLabel,
    liveItemByStrokeId: liveItemByStrokeId,
    selectShapesByStrokeIds: selectShapesByStrokeIds,
    startShapeTreeRename: startShapeTreeRename,
    // 3D layers (2026-07-28) — see make3DProjector/project3DSegments' own
    // doc comment (Grease-Pencil-style: vertices move in 3D, stroke width
    // never scales).
    make3DProjector: make3DProjector,
    project3DSegments: project3DSegments,
    // Duplicator/Effector "any property" generalization (2026-07-30) — the
    // single source of truth for which properties a duplicator's stagger
    // and its Effectors can target, and their PROP_DIM/PROP_LABEL/PROP_UNIT
    // shape, so app.js (the math) and timeline.js (the UI) both read the
    // SAME list instead of each hardcoding their own copy.
    DUP_TARGET_PROPS: DUP_TARGET_PROPS,
    DUP_OFFSET_PROP: DUP_OFFSET_PROP,
    propDim: function (prop) { return PROP_DIM[prop] || 1; },
    propLabel: function (prop) { return PROP_LABEL[prop] || prop; },
    propUnit: function (prop) { return PROP_UNIT[prop] || ''; },
    propDimLabels: function (prop) { return PROP_DIM_LABELS[prop] || null; },
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
      // Parent chain (2026-07-29 fix, Cyril: "fait ce qu'il faut") — this
      // used to only look at the layer's OWN transform, so a layer with NO
      // motion of its own but parented to one that DOES (e.g. "Parent" has
      // a Position offset, "Child" is parentLayerUid'd to it and holds the
      // actual shape) returned null — select-bridge.js/subselect-bridge.js/
      // rig-bridge.js then skipped mapping entirely, hit-testing/dragging at
      // the click's RAW position while the content actually rendered
      // wherever the ancestor's transform put it. Confirmed live: a shape's
      // raw point at (280,380) rendered at (400,320) once its parent had a
      // +120,-60 Position key, with layerMotionPointMap still returning
      // null for the child. fwd applies the layer's own transform first,
      // then each ancestor in order (immediate parent first) — the SAME
      // composition order buildSceneJson's own per-item loop already uses
      // (own motionMat via transformSegments, then parentChain in a loop);
      // inv undoes them in the exact reverse order.
      var chain = parentChainMats(li, state.currentFrame);
      if (!mm && !chain.length) return null;
      var lb = userLayers[li] && userLayers[li].bounds;
      if (!lb) return null;
      var ownMat = mm || { dx: 0, dy: 0, rot: 0, sx: 1, sy: 1, op: 1, ax: 0, ay: 0 };
      var px = lb.center.x + ownMat.ax, py = lb.center.y + ownMat.ay;
      var r = ownMat.rot * Math.PI / 180, c = Math.cos(r), s = Math.sin(r);
      function fwdOne(x, y, pivotX, pivotY, mat, cc, ss) {
        var lx = (x - pivotX) * mat.sx, ly = (y - pivotY) * mat.sy;
        return [pivotX + lx * cc - ly * ss + mat.dx, pivotY + lx * ss + ly * cc + mat.dy];
      }
      function invOne(x, y, pivotX, pivotY, mat, cc, ss) {
        var wx = x - mat.dx - pivotX, wy = y - mat.dy - pivotY;
        var lx = wx * cc + wy * ss, ly = -wx * ss + wy * cc;
        return [pivotX + lx / (mat.sx || 1e-6), pivotY + ly / (mat.sy || 1e-6)];
      }
      return {
        // OWN-level transform only (unchanged meaning, pre-2026-07-29) —
        // kept for callers that feed this straight into transformSegments
        // for handle-vector rotation (engine-bridge.js's buildRigPreviewItems).
        // A ROTATING ancestor's own contribution to handle-vector rotation
        // isn't composed here — a residual, narrower gap than the point-
        // mapping fix below (parented layer + Rig + a rotating ancestor
        // would still render slightly-off curve handles on the overlay;
        // point hit-testing/dragging, the part that matters for actually
        // grabbing the right thing, is fully correct).
        mat: ownMat,
        pivot: new Point(px, py),
        fwd: function (x, y) {
          var p = fwdOne(x, y, px, py, ownMat, c, s);
          for (var i = 0; i < chain.length; i++) {
            var ch = chain[i], cr = ch.mat.rot * Math.PI / 180;
            p = fwdOne(p[0], p[1], ch.pivot.x, ch.pivot.y, ch.mat, Math.cos(cr), Math.sin(cr));
          }
          return p;
        },
        inv: function (x, y) {
          var cx = x, cy = y;
          for (var i = chain.length - 1; i >= 0; i--) {
            var ch = chain[i], cr = ch.mat.rot * Math.PI / 180;
            var rr = invOne(cx, cy, ch.pivot.x, ch.pivot.y, ch.mat, Math.cos(cr), Math.sin(cr));
            cx = rr[0]; cy = rr[1];
          }
          return invOne(cx, cy, px, py, ownMat, c, s);
        },
        invVec: function (x, y) {
          var lx = x * c + y * s, ly = -x * s + y * c;
          return [lx / (ownMat.sx || 1e-6), ly / (ownMat.sy || 1e-6)];
        },
      };
    },
    // 3D counterpart of layerMotionPointMap just above — null unless
    // ld.threeD, since a 3D layer's forward map is a true perspective
    // projection, not the 2D case's affine one (see layerMotion3DPointMap's
    // own header comment). Callers check the 2D map first (the overwhelmingly
    // common case) and fall back to this one only when it's null AND the
    // active layer is 3D.
    layerMotion3DPointMap: layerMotion3DPointMap,
    setLayerValue: function (li, prop, vals) { var ld = state.layers[li]; if (ld) setValue(ld, prop, vals); },
    layerMotionAt: layerMotionAt,
    // World<->layer-local point conversion through the parent chain only
    // (t.strokeId left null) — rig-widget.js reuses this instead of
    // reimplementing chain inversion, same battle-tested math every other
    // on-canvas drag (anchor point, position keys, effector handles, gizmo)
    // already goes through.
    outerWorldPoint: outerWorldPoint,
    outerLocalPoint: outerLocalPoint,
    layerBlendModeAt: layerBlendModeAt,
    upsertBlendKeyAt: upsertBlendKeyAt,
    removeBlendKeyAt: removeBlendKeyAt,
    layerParentUidAt: layerParentUidAt,
    upsertParentKeyAt: upsertParentKeyAt,
    removeParentKeyAt: removeParentKeyAt,
    // feedback #211 — select-bridge.js's own layer-body Motion drag writes
    // Position too, straight through SMMotion.setLayerValue, same staleness
    // this fixes for every onDrag branch in this file.
    liveRefreshVisiblePropertyFields: liveRefreshVisiblePropertyFields,
    // rig-widget.js's "+" button (feedback #185) reads the SAME per-layer
    // property list every Motion row already goes through (§11's single-
    // decider invariant), instead of guessing a parallel list, and writes
    // through the SAME expression setter the property-row wiring menu uses
    // (widgetWiringMenuItems above) — one writer, two entry points.
    propsFor: propsFor,
    setExpressionCode: setExpressionCode,
    // Exposed for getEffectiveStrokes' symbolId branch (app.js, 2026-07-29
    // fix: "un calque interne d'un component multi-calques n'anime jamais
    // depuis l'extérieur") — composing a sym.layers[] entry's OWN layer-
    // level Motion when rendering the placed instance from outside. Not the
    // same as layerMotionAt just above: that one assumes a state.layers
    // index (the instance itself); a sub-layer inside state.symbols[id]
    // isn't indexed there at all. Same computeMotionMat every layer-level
    // Motion consumer already shares (layerMotionAt/parentChainMats), just
    // reachable from outside this closure with an arbitrary holder object.
    computeMotionMatFor: computeMotionMat,
    elementMotionAt: elementMotionAt,
    elementFillColorAt: elementFillColorAt,
    elementStrokeColorAt: elementStrokeColorAt,
    elementStrokeWidthAt: elementStrokeWidthAt,
    // Is layer `li` actually matted at `frameIdx`? The SINGLE reader of the
    // matteOn track's threshold, so "what counts as on" is defined once —
    // engine-bridge asks this and simply omits matteMode from the wire on
    // frames that answer false (see its own comment for why that animates a
    // field Rust holds as a plain Option<String>).
    // Layers with no track at all answer true, so every existing matte
    // keeps behaving exactly as before this property existed.
    registerEffectorPropMeta: registerEffectorPropMeta,
    matteOnAt: function (li, frameIdx) {
      var ld = state.layers[li];
      if (!ld) return true;
      if (!hasKeys(ld, 'matteOn') && !(ld.motionStatic && ld.motionStatic.matteOn)) return true;
      var v = valueAtFrame(ld, 'matteOn', frameIdx);
      return !v || v[0] == null ? true : v[0] >= 50;
    },
    // ---- Expression code, addressed from outside this closure -----------
    // For the split code editor (expr-code-panel.js, 2026-08-30: "un code
    // editor window qui split la zone du canvas"). The panel is a SECOND
    // view of an expression the inline ƒx row already edits, so it must not
    // own any state: it addresses one by the same {uid, elem} holder ref
    // copySelectedKeys uses, reads through exprSnapshotFor and writes
    // through applyExprCode — which is the inline editor's own commit(),
    // moved here so there is exactly one writer for both surfaces
    // (the two-writers-disagreeing bug this session already produced once,
    // in the color picker).
    holderRefOf: holderRefOf,
    exprSnapshotFor: function (ref, prop) {
      var holder = holderFromRef(ref);
      if (!holder) return null;
      var e = ensureExpr(holder, prop);
      return { code: e.code || '', enabled: !!e.enabled, lastError: e.lastError || null, errorLine: e.errorLine };
    },
    applyExprCode: function (ref, prop, code) {
      var holder = holderFromRef(ref);
      if (!holder) return false;
      var e = ensureExpr(holder, prop);
      if (e.code === code) return false;
      pushUndo();
      e.code = code;
      e.lastError = null;
      e.errorLine = -1;
      if (holder._exprCompiled) delete holder._exprCompiled[prop];
      if (typeof saveActiveLayerFrame === 'function') saveActiveLayerFrame();
      reloadIfTimeLinkOffset(prop);
      if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
      renderLayerList(); renderTimeline();
      return true;
    },
    setExprEnabled: function (ref, prop, on) {
      var holder = holderFromRef(ref);
      if (!holder) return false;
      pushUndo();
      ensureExpr(holder, prop).enabled = !!on;
      if (typeof saveActiveLayerFrame === 'function') saveActiveLayerFrame();
      reloadIfTimeLinkOffset(prop);
      if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
      renderLayerList(); renderTimeline();
      return true;
    },
    // Lets the ORDINARY fill/stroke color picker (SM.setFillColor /
    // setStrokeColor, timeline.js) reach an element's Motion color track.
    // Without this the two writers disagree in the one way that loses the
    // user's work: the picker wrote p.fillColor straight onto the Paper item
    // and saved the frame, while elementFillColorAt kept returning the TRACK
    // value — so with a fillColor track live, picking a color at frame 12
    // showed red for one repaint and then rendered blue again, forever, with
    // no key created and no error. Measured live before the fix: 1 key in,
    // 1 key out, live item #ff2200, engine value still [74,158,255,255].
    // Returns false when this element has no color track at all, which is
    // the overwhelmingly common case — plain Animation 2D recoloring keeps
    // its existing behavior untouched and never pays for this lookup twice.
    // setValue is the single writer (CLAUDE.md §13): it keys at the playhead
    // when the stopwatch is on and writes motionStatic when it isn't, so the
    // picker inherits both behaviors for free.
    // Colour rows the user has SELECTED in the Motion timeline (clicking the
    // row's label, the same selection the value-edit/ease machinery uses).
    // The main colour panel writes through the CANVAS selection, which is a
    // different thing entirely — feedback #180's second half: "select
    // couleurs ne prend pas en compte la selection de la propriété
    // impossible de changer la couleurs et de keyframé par là". Returns
    // [{holder, prop}] for fillColor/strokeColor rows only; every other
    // selected property is none of the colour panel's business.
    selectedColorProps: function (which) {
      if (state.appMode !== 'motion') return [];
      return _motionPropSel.filter(function (s) {
        return s.prop === which && s.holder;
      }).map(function (s) { return { holder: s.holder, prop: s.prop }; });
    },
    // Companion writer for the rows above. Same body as
    // writeElementColorFromPicker minus the (li, strokeId) lookup, since a
    // selected row already IS a holder — going back through indices would
    // just be a chance to resolve the wrong one.
    writeColorToHolder: function (holder, prop, hex) {
      if (!holder) return false;
      if (!hasKeys(holder, prop) && !(holder.motionStatic && holder.motionStatic[prop])) return false;
      setValue(holder, prop, hexToRgba255(hex));
      return true;
    },
    writeElementColorFromPicker: function (li, strokeId, prop, hex) {
      var ld = state.layers[li];
      if (!ld || !ld.elementMotion || !strokeId) return false;
      var holder = ld.elementMotion[strokeId];
      if (!holder) return false;
      if (!hasKeys(holder, prop) && !(holder.motionStatic && holder.motionStatic[prop])) return false;
      setValue(holder, prop, hexToRgba255(hex));
      return true;
    },
    layerOrderAt: layerOrderAt,
    layerHasExplicitOrder: layerHasExplicitOrder,
    elementOrderAt: elementOrderAt,
    anyLayerHasOrder: anyLayerHasOrder,
    layerElementsHaveOrder: layerElementsHaveOrder,
    anyOrderUsedAnywhere: anyOrderUsedAnywhere,
    hasBrushSizeMotionFor: hasBrushSizeMotionFor,
    hasVectorBrushOutlineMotionFor: hasVectorBrushOutlineMotionFor,
    applyVectorBrushOutlineFor: applyVectorBrushOutlineFor,
    isVectorBrushSd: isVectorBrushSd,
    pathVertexRowCount: pathVertexRowCount,
    transformSegments: transformSegments,
    transformImageRect: transformImageRect,
    transformImageRectByMatrix: transformImageRectByMatrix,
    project3DImageRect: project3DImageRect,
    ensureLayerUid: ensureLayerUid,
    layerWorldBoundsUnion: layerWorldBoundsUnion,
    findLayerIndexByUid: findLayerIndexByUid,
    setLayerParent: setLayerParent,
    setLayerParentB: setLayerParentB,
    setLayerFollowPath: setLayerFollowPath,
    blendedParentContributionFor3D: blendedParentContributionFor3D,
    shiftLayerMotionKeys: shiftLayerMotionKeys,
    previewKeyframeShift: previewKeyframeShift,
    clearKeyframeShiftPreview: clearKeyframeShiftPreview,
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
    invertVectorThroughParentChain: invertVectorThroughParentChain,
    setMotionCanvasEmptyClick: setMotionCanvasEmptyClick,
    applyParentChainToSegments: applyParentChainToSegments,
    applyParentChainToImageRect: applyParentChainToImageRect,
    applyPathVertexOffsetsFor: applyPathVertexOffsetsFor,
    // Image mesh vertex tracks (2026-08-30) — see meshHolder's own comment
    // for why these are keyed by meshId and expressed in percent.
    hasMeshVertexMotionFor: hasMeshVertexMotionFor,
    meshVertexOffsetAt: meshVertexOffsetAt,
    setMeshVertexOffset: setMeshVertexOffset,
    isMeshVertexAnimated: isMeshVertexAnimated,
    meshVertexRowCount: meshVertexRowCount,
    applyTrimFor: applyTrimFor,
    hasParamShapeMotionFor: hasParamShapeMotionFor,
    applyParamShapeFor: applyParamShapeFor,
    trimWindowAt: trimWindowAt,
    hasTrimMotion: hasTrimMotion,
    // Shared arc-length flattener (2026-08) — Trim Paths' own polyline
    // sampler, exposed for stroke-gradient-along-path (engine-bridge.js) to
    // reuse rather than duplicating the same bezier-subdivision math a
    // second time (CLAUDE.md §3's duplicated-pair hazard).
    flattenSegmentsToPolyline: buildTrimPolyline,
    buildOverlayItems: buildOverlayItems,
    onHoverMove: onHoverMove,
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
    // Same reasoning as hasPathVertexMotionFor right above — Trim Paths
    // rebuilds the geometry into a wholly different point set (not an
    // affine of the original), so a trimmed shape can never ride a stored
    // path either.
    hasTrimMotionFor: function (li, strokeId) {
      var ld = state.layers[li];
      if (!ld || !ld.elementMotion || !strokeId) return false;
      return hasTrimMotion(ld.elementMotion[strokeId]);
    },
    trimIsFullWindow: trimIsFullWindow,
    applyTrimToVectorBrush: applyTrimToVectorBrush,
    // Called by the two frame-content writers in app.js — the union is
    // derived from that content, so it must not outlive an edit.
    invalidateSymbolUnionBounds: invalidateSymbolUnionBounds,
    // layer-inout.js calls this when a bar press turned out to be a plain
    // click rather than a retime drag — the bar covers most of the grid half
    // of a layer's row, so without it that whole strip stayed unclickable.
    selectLayerFromGrid: selectLayerFromGrid,
    layerElements: layerElements,
    elementLabel: elementLabel,
    // Text Animator (text-animator.js) drives per-glyph/word/line keyframes
    // through the SAME element-holder + track primitives everything else in
    // this file uses — ensureElementHolder/setKeyAtFrame are the only two
    // pieces that weren't already exported for an external caller to build
    // a multi-key staggered animation in one pass (setValue/setKeyAtCurrentFrame
    // only ever touch state.currentFrame, one key at a time).
    ensureElementHolder: ensureElementHolder,
    setKeyAtFrame: setKeyAtFrame,
    registerExposedPropMeta: registerExposedPropMeta,
    distributeKeys: distributeKeys, flipKeys: flipKeys, selectEveryNthKey: selectEveryNthKey, invertKeySelection: invertKeySelection,
    getKeySelection: function () { return _motionKeySel.slice(); },
    // feedback #212 — per-keyframe standing lock (key.lockTo), used by
    // layer-inout.js's drag-drop alongside the whole-layer ld.keyLock pass.
    keysLockedTo: keysLockedTo,
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
    //
    // Also propagates to every OTHER selected keyframe (2026-07-30, Cyril:
    // "on devrait pouvoir appliquer les tween directement depuis le
    // panneau easing dés que l'on change la curve manuellement ou via les
    // preset... automatique sur les keyframes select") — the editor is
    // opened on exactly ONE key (segmentLeftKey, openMotionEaseEditor);
    // with 2+ keys selected when you reshape it, copy the same curve onto
    // the others too instead of only ever touching the one the editor
    // happens to be pointed at. Cross-property/cross-layer selections are
    // allowed on purpose — curvePoints is a normalized [0,1]x[0,1] shape,
    // meaningless-but-harmless on a key where it never gets evaluated (the
    // last key of a track has no "next" key to ease into), and applying
    // one curve across a mixed multi-property pick is exactly the "select
    // several keys, dial in one ease, done" workflow this was asked for.
    // Cloned per key (never shared by reference) so a later edit to ONE of
    // them doesn't silently reshape the rest all over again.
    onEaseSegChanged: function (seg) {
      if (seg && seg.curvePoints && _motionKeySel.length > 1) {
        _motionKeySel.forEach(function (s) {
          if (!s.key || s.key === seg) return;
          s.key.curvePoints = seg.curvePoints.map(function (p) {
            var o = { x: p.x, y: p.y };
            if (typeof p.tx === 'number') { o.tx = p.tx; o.ty = p.ty || 0; }
            return o;
          });
        });
      }
      renderLayerList(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    },
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
  // Workspace continuity, restore half (2026-08) — runs once at startup,
  // after timeline.js's own nemo-auto project restore (script tag order:
  // timeline.js, then this file) so layers/timeline already exist by the
  // time setAppMode's re-render calls (renderLayerList/renderTimeline)
  // fire. A brand-new/never-saved profile has no key yet and falls
  // through to whatever state.appMode's own hardcoded default already is.
  (function restoreLastAppMode(){
    var saved=null;
    try{saved=localStorage.getItem('nemo-app-mode');}catch(e){}
    if(saved&&saved!==state.appMode&&(saved==='anim2d'||saved==='motion'||saved==='storyboard'))setAppMode(saved);
  })();
})();
