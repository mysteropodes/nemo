// ---- LABS PROTOTYPE — Bake clip mask (Umoupen/CSP mask layer, scoped) ----
// feature-scouting.md #1 explains why a real NON-DESTRUCTIVE clip layer
// needs engine.rs changes (compositing lives in render()/render_to_pixels(),
// the scene-JSON serializer, and the persisted file format) — out of
// reach for a risk-free Labs prototype. This is the honest scoped-down
// version: BAKE the clip once, as a real boolean intersection, through
// the exact same WASM/Paper-fallback pipeline the existing Boolean Ops
// tool already uses (booleanOpWasm, tools.js) — so the result is ORDINARY
// geometry the Rust engine renders correctly with zero engine changes.
// Not live/reorderable like a real mask layer would be; a real repeat of
// the bake is needed after editing either layer. Documented tradeoff, not
// a hidden one.
//
//   SMLabs.bakeClipMask(maskLayerIdx, targetLayerIdx, {removeMask:true})
//
// The mask layer's shapes are unioned into one clip region (reusing
// booleanOpWasm, same fold-pairs technique the Boolean Ops tool uses for
// 3+ selected shapes); every path in the target layer is intersected
// against it via the SAME function, then exploded into flat Paths via
// insertBooleanResult (CLAUDE.md §1: a boolean result MUST go through
// this helper — CompoundPath islands would otherwise vanish from saved
// data). One pushUndo before the whole bake.
(function () {
  function collectMaskable(layer) {
    return layer.children.filter(function (c) {
      return c instanceof Path && c.segments && c.segments.length >= 2 && (c.fillColor || c.closed);
    });
  }
  function unionAll(paths) {
    if (paths.length === 1) return paths[0].clone({ insert: false });
    if (window.GeometryWasm && window.GeometryWasm.ready) {
      try { var r = booleanOpWasm('unite', paths); if (r) return r; } catch (e) { console.warn('[labs] union WASM échoué, repli Paper.js', e); }
    }
    var acc = paths[0].clone({ insert: false });
    for (var i = 1; i < paths.length; i++) { var r2 = acc.unite(paths[i], { insert: false }); acc.remove(); acc = r2; }
    return acc;
  }
  function intersectOne(path, clip) {
    if (window.GeometryWasm && window.GeometryWasm.ready) {
      try { var r = booleanOpWasm('intersect', [path, clip]); if (r) return r; } catch (e) { console.warn('[labs] intersect WASM échoué, repli Paper.js', e); }
    }
    return path.intersect(clip, { insert: false });
  }

  window.SMLabs.bakeClipMask = function (maskLayerIdx, targetLayerIdx, opts) {
    opts = opts || {};
    var mLd = state.layers[maskLayerIdx], tLd = state.layers[targetLayerIdx];
    if (!mLd || !tLd || mLd.symbolId || tLd.symbolId) { if (typeof showToast === 'function') showToast(SM.t('labsToastLayersInvalid')); return 0; }
    if (tLd.locked) { if (typeof showToast === 'function') showToast(SM.t('labsToastTargetLayerLocked')); return 0; }
    var maskLayer = userLayers[maskLayerIdx], targetLayer = userLayers[targetLayerIdx];
    var maskPaths = collectMaskable(maskLayer);
    if (!maskPaths.length) { if (typeof showToast === 'function') showToast(SM.t('labsToastMaskLayerEmpty')); return 0; }
    var targets = collectMaskable(targetLayer);
    if (!targets.length) { if (typeof showToast === 'function') showToast(SM.t('labsToastTargetLayerEmpty')); return 0; }

    pushUndo();
    var clip = unionAll(maskPaths.map(function (p) { return p.clone({ insert: false }); }));
    var kept = 0, dropped = 0;
    targets.forEach(function (p) {
      var res;
      try { res = intersectOne(p, clip); } catch (e) { console.warn('[labs] intersect a échoué sur un trait', e); res = null; }
      if (!res || !res.segments || !res.segments.length) {
        if (res) res.remove();
        p.remove();
        dropped++;
        return;
      }
      var style = p;
      var islands = insertBooleanResult(targetLayer, targetLayer.children.indexOf(p) >= 0 ? targetLayer.children.indexOf(p) : targetLayer.children.length, res, style.fillColor, style.opacity);
      islands.forEach(function (isl) {
        isl.strokeColor = style.strokeColor; isl.strokeWidth = style.strokeWidth;
        isl.strokeCap = style.strokeCap; isl.strokeJoin = style.strokeJoin;
      });
      p.remove();
      kept += islands.length;
    });
    clip.remove();
    if (opts.removeMask !== false) maskPaths.forEach(function (p) { p.remove(); });
    if (typeof fillRegenerateLinked === 'function' && targetLayer.children.length) fillRegenerateLinked(targetLayer, targetLayer.children[0]);
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    if (typeof showToast === 'function') showToast(SM.t('labsToastMaskAppliedPrefix') + kept + SM.t('labsToastMaskAppliedMid') + dropped + SM.t('labsToastMaskAppliedSuffix'));
    return kept;
  };

  window.SMLabs.register('clip-mask-bake', {
    flag: 'nemo-labs-clipmask',
    describe: 'labsDescribeClipMaskBake',
  });
})();
