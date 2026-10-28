// ---- LABS PROTOTYPE — Pose library (Toon Boom drawing substitution) ----
// Toon Boom's drawing substitution: a layer carries a small library of
// alternate drawings (mouth shapes, hand poses...) swappable per frame.
// Labs form: save the Select tool's current selection as a named pose,
// stamp any saved pose onto the current frame later — lip-sync mouths,
// hand kits, recurring props.
//
//   1. Select strokes (Select tool)
//   2. SMLabs.savePose('bouche-A')
//   3. ...any frame later:  SMLabs.stampPose('bouche-A')
//      SMLabs.stampPose('bouche-A', {dx:120, dy:0})   — offset the stamp
//   SMLabs.listPoses() / SMLabs.deletePose('nom')
//
// Poses are stored as serP() snapshots (the exact persisted stroke form),
// stamped back through desP() with fresh strokeIds — so a stamp is
// indistinguishable from hand-drawn ink to every consumer (CLAUDE.md §1),
// and stamping the same mouth on 10 frames never creates 10 objects
// claiming one identity in tween matching. localStorage per project key,
// same precedent as timeline-markers.
(function () {
  function projectKey() {
    try { if (window.SMProject && SMProject.getProjectKey) return SMProject.getProjectKey(); } catch (e) {}
    return 'default';
  }
  function storeKey() { return 'nemo-labs-poses-' + projectKey(); }
  function load() { try { return JSON.parse(localStorage.getItem(storeKey()) || '{}'); } catch (e) { return {}; } }
  function save(m) { localStorage.setItem(storeKey(), JSON.stringify(m)); }
  function freshId() { return 'labs_' + Date.now().toString(36) + '_' + Math.floor(Math.random() * 1e6); }

  window.SMLabs.savePose = function (name) {
    if (!name) { console.warn('[labs] savePose(nom)'); return 0; }
    if (typeof selectedPaths === 'undefined' || !selectedPaths.length) {
      if (typeof showToast === 'function') showToast(SM.t('labsToastSelectStrokesFirst'));
      return 0;
    }
    var snap = [];
    selectedPaths.forEach(function (p) {
      // #2362: on a native object project the selection is presented geometry only (no Paper item, no style), so the
      // pose names its strokes and the stamp copies them natively; a linked fill goes first, below its ribbon.
      var ref = p && p.data && p.data.nativeStableTarget, fill = ref && p.data.linkedFill && p.data.linkedFill.data && p.data.linkedFill.data.strokeId;
      if (ref) { if (fill) snap.push({ nativeRef: Object.assign({}, ref, { strokeId: fill }) }); snap.push({ nativeRef: ref }); return; }
      if (!p || !p.parent) return;
      snap.push(serP(p));
      if (p.data && p.data.linkedFill && p.data.linkedFill.parent) snap.push(serP(p.data.linkedFill));
    });
    var m = load(); m[name] = snap; save(m);
    if (typeof showToast === 'function') showToast(SM.t('labsToastPosePrefix') + name + SM.t('labsToastPoseSavedMid') + snap.length + SM.t('labsToastStrokeCountSuffix'));
    return snap.length;
  };

  // Native authority (U34, #2362): every stroke of the pose is created in ONE native vector batch keyed with what a held
  // cell showed (the baseline ensureKeyframe()), each with a fresh id, without its brush group and moved by this stamp's
  // offset only. Returns a Promise of the stamped count.
  var MAX_NATIVE_STAMP = 64;
  function nativeStamp(native, name, snap, opts) {
    var toast = function (text) { if (typeof showToast === 'function') showToast(text); };
    var layerUid = native.layerUid(state.activeLayerIdx);
    if (!layerUid || snap.length > MAX_NATIVE_STAMP) { toast(SM.t('labsToastLayerInvalidLocked')); return Promise.resolve(0); }
    var dx = opts && +opts.dx || 0, dy = opts && +opts.dy || 0;
    var move = function (list) {
      (list || []).forEach(function (s) { if (s && Array.isArray(s.point)) { s.point[0] += dx; s.point[1] += dy; } });
    };
    var steps = snap.map(function (sd) {
      if (sd && sd.nativeRef) return { clone: { stableTarget: sd.nativeRef, offset: [dx, dy] } };
      var copy = JSON.parse(JSON.stringify(sd));
      delete copy.strokeId; delete copy.brushGroupId;
      move(copy.segments); move(copy.centerSegments);
      return copy.opacity === undefined ? { create: copy } : { create: copy, opacity: copy.opacity };
    });
    return native.batchStrokes(layerUid, state.currentFrame, steps).then(function (result) {
      if (!result || result.status !== 'applied') { toast(SM.t('labsToastLayerInvalidLocked')); return 0; }
      toast(SM.t('labsToastPosePrefix') + name + SM.t('labsToastPoseStampedMid') + steps.length + SM.t('labsToastStrokeCountCloseSuffix'));
      return steps.length;
    });
  }

  window.SMLabs.stampPose = function (name, opts) {
    var m = load(), snap = m[name];
    if (!snap) { console.warn('[labs] pose inconnue:', name, Object.keys(m)); return 0; }
    var native = window.NemoNativeObjectLabs && NemoNativeObjectLabs.desktop(window);
    if (native) return nativeStamp(native, name, snap, opts);
    var ld = state.layers[state.activeLayerIdx];
    if (!ld || ld.locked || ld.symbolId) { if (typeof showToast === 'function') showToast(SM.t('labsToastLayerInvalidLocked')); return 0; }
    var dx = opts && +opts.dx || 0, dy = opts && +opts.dy || 0;
    pushUndo();
    ensureKeyframe();
    var layer = userLayers[state.activeLayerIdx];
    layer.activate();
    var n = 0;
    snap.forEach(function (sd) {
      var copy = JSON.parse(JSON.stringify(sd));
      // Fresh identity per stamp — see header. brushGroupId links dab
      // scaffolding; poses snapshot simple/companion strokes, so clearing
      // it is safe and avoids cross-stamp relinking.
      if (copy.strokeId) copy.strokeId = freshId();
      delete copy.brushGroupId;
      var p = desP(copy, layer);
      if (dx || dy) p.translate(new Point(dx, dy));
      if (typeof tagOwner === 'function') tagOwner(p);
      n++;
    });
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    if (typeof showToast === 'function') showToast(SM.t('labsToastPosePrefix') + name + SM.t('labsToastPoseStampedMid') + n + SM.t('labsToastStrokeCountCloseSuffix'));
    return n;
  };

  window.SMLabs.listPoses = function () {
    var m = load();
    return Object.keys(m).map(function (n) { return { name: n, strokes: m[n].length }; });
  };
  window.SMLabs.deletePose = function (name) { var m = load(); delete m[name]; save(m); };

  window.SMLabs.register('pose-library', {
    flag: 'nemo-labs-poses',
    describe: 'labsDescribePoseLibrary',
  });
})();
