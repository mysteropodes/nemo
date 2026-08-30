// Duplicator effectors as their own layer kind (2026-08-30, feedback #168:
// "les effectors de duplication ne sont pas animable il faudrait pouvoir
// animé", then "à mon avis il peut apparaitre comme un calque effector dans
// la timeline").
//
// THE DECISION EVERYTHING ELSE FOLLOWS FROM: an effector layer owns no
// values of its own. Its ORIGIN is the layer's ordinary Motion Position,
// and its radius/strength/angle/offsets are ordinary Motion properties on
// that same layer. That single choice is what makes the feature animatable
// at all — keyframes, ease curves, the graph editor, expressions, the
// pickwhip, parenting and persistence all arrive for free, exactly the way
// the rig widget's axes get them (CLAUDE.md §13). Nothing here re-implements
// any of it.
//
// It also gives the gesture Cyril actually wants: an effector you can drag
// across the grid and key, because "drag the effector" is just "move a
// layer" and the whole app already knows how to do that.
//
// The inline effectors that already live on ld.duplicator.effectors are NOT
// migrated and NOT touched. They keep working exactly as before; an effector
// LAYER is an additional contributor read alongside them. Same
// non-migration strategy as multi-matte and weighted parents earlier today,
// and for the same reason: a scene that doesn't use the feature serializes
// and renders byte-identically to before.
(function () {
  'use strict';

  // Motion property keys carried by an effector layer. Registered through
  // propsFor (motion.js) like any exposed property, so both the panel and
  // the grid list them and the §11 alignment invariant holds with no work.
  var P_RADIUS = 'effRadius', P_STRENGTH = 'effStrength', P_ANGLE = 'effAngle';
  // One offset property per duplicator-targetable channel. Deliberately a
  // FIXED set rather than the effector's old free-form channel array: a
  // Motion property key has to be stable to key against, and an array whose
  // entries can be reordered would re-point its own keyframes — the exact
  // index-drift trap the parent weights hit this morning. Every channel
  // exists always and simply sits at 0 when unused, which costs nothing and
  // can never drift.
  var CHANNELS = [
    { key: 'effOffPosition', prop: 'position', dim: 2, label: 'Offset Position', unit: 'px' },
    { key: 'effOffRotation', prop: 'rotation', dim: 1, label: 'Offset Rotation', unit: '°' },
    { key: 'effOffScale', prop: 'scale', dim: 2, label: 'Offset Scale', unit: '%' },
    { key: 'effOffOpacity', prop: 'opacity', dim: 1, label: 'Offset Opacity', unit: '%' },
    { key: 'effOffPositionZ', prop: 'positionZ', dim: 1, label: 'Offset Z', unit: 'px' },
    { key: 'effOffRotationX', prop: 'rotationX', dim: 1, label: 'Offset Rot X', unit: '°' },
    { key: 'effOffRotationY', prop: 'rotationY', dim: 1, label: 'Offset Rot Y', unit: '°' },
  ];

  function isEffectorLayer(ld) { return !!(ld && ld.isEffectorLayer && ld.effector); }

  // Every effector layer currently pointing at `targetUid`, in layer order.
  // Cheap enough to call per frame: the first test is a plain flag, so a
  // project with no effector layers pays one property read per layer.
  function effectorLayersFor(targetUid) {
    var out = [];
    if (!targetUid || !window.state || !state.layers) return out;
    for (var i = 0; i < state.layers.length; i++) {
      var ld = state.layers[i];
      if (!ld.isEffectorLayer || !ld.effector) continue;
      if (ld.effector.targetLayerUid !== targetUid) continue;
      if (ld.visible === false) continue; // hiding an effector disables it
      out.push({ li: i, ld: ld });
    }
    return out;
  }

  // Resolves ONE effector layer at `frameIdx` into the exact shape
  // _duplicatorClonePlacement's own inline effectors already have, so the
  // math there reads both kinds through one code path instead of growing a
  // second branch (CLAUDE.md §3's duplicated-pair trap, avoided by never
  // creating the pair).
  //
  // The origin comes from the layer's resolved Motion position — including
  // its parent chain — so an effector parented to something, or keyed, or
  // driven by an expression, all work with nothing added here.
  function resolveEffector(li, ld, frameIdx) {
    var M = window.SMMotion;
    if (!M) return null;
    var eff = ld.effector || {};
    var base = eff.pos || [state.canvasW / 2, state.canvasH / 2];
    var mm = M.layerMotionAt ? M.layerMotionAt(li, frameIdx) : null;
    var ox = base[0], oy = base[1];
    if (mm) { ox += (mm.dx || 0); oy += (mm.dy || 0); }
    function val(key, fallback) {
      var v = M.valueAtFrame ? M.valueAtFrame(ld, key, frameIdx) : null;
      return (v && v[0] != null) ? v[0] : fallback;
    }
    function val2(key) {
      var v = M.valueAtFrame ? M.valueAtFrame(ld, key, frameIdx) : null;
      return v ? [v[0] || 0, v[1] || 0] : [0, 0];
    }
    var channels = [];
    for (var c = 0; c < CHANNELS.length; c++) {
      var ch = CHANNELS[c];
      var v = ch.dim === 2 ? val2(ch.key) : [val(ch.key, 0)];
      if (!v[0] && !(v[1] || 0)) continue; // an untouched channel contributes nothing
      channels.push({ prop: ch.prop, value: v });
    }
    if (!channels.length) return null; // no offsets = no effect, skip the math
    return {
      pos: { x: ox, y: oy },
      falloff: eff.falloff || 'radial',
      radius: val(P_RADIUS, 200),
      strength: val(P_STRENGTH, 100),
      angle: val(P_ANGLE, 0),
      channels: channels,
      _fromLayer: true,
    };
  }

  // All effector-layer contributions for a duplicator layer, already in the
  // inline shape. Returns [] fast when there are none.
  function resolvedEffectorsFor(dupLd, frameIdx) {
    if (!dupLd || !dupLd.layerUid) return [];
    var list = effectorLayersFor(dupLd.layerUid);
    if (!list.length) return [];
    var out = [];
    for (var i = 0; i < list.length; i++) {
      var r = resolveEffector(list[i].li, list[i].ld, frameIdx);
      if (r) out.push(r);
    }
    return out;
  }

  // Property rows for the Motion panel/grid. propsFor calls this so both
  // sides get the identical list (§11).
  function propKeysFor(ld) {
    if (!isEffectorLayer(ld)) return [];
    var M = window.SMMotion;
    if (!M || !M.registerEffectorPropMeta) return [];
    var keys = [];
    M.registerEffectorPropMeta(P_RADIUS, SM.t('propEffRadius'), 200, 1, 'px');
    M.registerEffectorPropMeta(P_STRENGTH, SM.t('propEffStrength'), 100, 1, '%');
    keys.push(P_RADIUS, P_STRENGTH);
    if ((ld.effector.falloff || 'radial') === 'linear') {
      M.registerEffectorPropMeta(P_ANGLE, SM.t('propEffAngle'), 0, 1, '°');
      keys.push(P_ANGLE);
    }
    CHANNELS.forEach(function (ch) {
      M.registerEffectorPropMeta(ch.key, ch.label, 0, ch.dim, ch.unit);
      keys.push(ch.key);
    });
    return keys;
  }

  function addEffectorLayer(targetLi) {
    if (!window.SMMotion || typeof createUserLayer !== 'function') return -1;
    var target = state.layers[targetLi];
    if (!target || !target.duplicator) {
      if (window.showToast) showToast(SM.t('toastEffectorNeedsDuplicator'));
      return -1;
    }
    if (typeof saveAllLayerFrames === 'function') saveAllLayerFrames();
    if (typeof pushUndoLayers === 'function') pushUndoLayers(true);
    var nm = (typeof nextLayerName === 'function' ? nextLayerName() : 'Layer').replace(/^Layer/, 'Effector');
    var idx = createUserLayer(nm);
    var ld = state.layers[idx];
    ld.isEffectorLayer = true;
    ld.color = '#59d38a';
    ld.effector = {
      targetLayerUid: SMMotion.ensureLayerUid(target),
      falloff: 'radial',
      // World anchor, exactly like guidePos/nullPos/widget.pos — the layer's
      // own Position track offsets it, so moving or keying the effector is
      // ordinary layer animation (§13's same choice).
      pos: [state.canvasW / 2, state.canvasH / 2],
    };
    // A brand-new effector with every offset at 0 would do nothing at all
    // and look broken. Seed a visible Position offset so dropping one in
    // shows an effect immediately, the way the duplicator's own enable
    // toast promises something happened.
    if (!ld.motionStatic) ld.motionStatic = {};
    ld.motionStatic[CHANNELS[0].key] = [120, 0];
    if (typeof activateUL === 'function') activateUL(idx);
    if (typeof loadFrame === 'function') loadFrame(state.currentFrame);
    if (typeof updateUI === 'function') updateUI();
    window._sceneVersion = (window._sceneVersion || 0) + 1;
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    if (window.showToast) showToast(SM.t('toastEffectorLayerAdded'));
    return idx;
  }

  // On-canvas marker: a ring at the effector's radius plus a crosshair at
  // its origin, so the falloff is visible while you drag it. Pushed from
  // engine-bridge INSIDE its includeEditorOverlays block, which is the whole
  // never-rendered gate (§13) — nothing here can reach an export.
  function buildOverlayItems() {
    if (!window.state || !state.layers) return [];
    var items = [];
    var zs = 1 / Math.max(0.0001, view.zoom);
    var f = state.currentFrame;
    for (var i = 0; i < state.layers.length; i++) {
      var ld = state.layers[i];
      if (!isEffectorLayer(ld) || ld.visible === false) continue;
      var r = resolveEffector(i, ld, f);
      // resolveEffector returns null when every offset is zero — still draw
      // the marker in that case, or a freshly zeroed effector would vanish
      // from the canvas and become unselectable.
      var M = window.SMMotion;
      var base = ld.effector.pos || [state.canvasW / 2, state.canvasH / 2];
      var mm = M && M.layerMotionAt ? M.layerMotionAt(i, f) : null;
      var ox = base[0] + (mm ? (mm.dx || 0) : 0), oy = base[1] + (mm ? (mm.dy || 0) : 0);
      var rad = r ? r.radius : ((M && M.valueAtFrame && M.valueAtFrame(ld, P_RADIUS, f) || [200])[0] || 200);
      var col = [89, 211, 138, 200];
      var seg = [];
      for (var a = 0; a <= 48; a++) {
        var th = a / 48 * Math.PI * 2;
        seg.push({ point: [ox + Math.cos(th) * rad, oy + Math.sin(th) * rad] });
      }
      items.push({ segments: seg, closed: true, fillColor: null, strokeColor: col, strokeWidth: 1 * zs, dashPattern: [6 * zs, 5 * zs] });
      var h = 9 * zs;
      items.push({ segments: [{ point: [ox - h, oy] }, { point: [ox + h, oy] }], closed: false, fillColor: null, strokeColor: col, strokeWidth: 1.5 * zs });
      items.push({ segments: [{ point: [ox, oy - h] }, { point: [ox, oy + h] }], closed: false, fillColor: null, strokeColor: col, strokeWidth: 1.5 * zs });
    }
    return items;
  }

  window.SMEffectorLayer = {
    isEffectorLayer: isEffectorLayer,
    resolvedEffectorsFor: resolvedEffectorsFor,
    propKeysFor: propKeysFor,
    addEffectorLayer: addEffectorLayer,
    buildOverlayItems: buildOverlayItems,
    CHANNELS: CHANNELS,
  };
})();
