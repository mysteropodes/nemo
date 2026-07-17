// ---- LABS PROTOTYPE — Baked per-layer effects (Umoupen effect stack, scoped) ----
// feature-scouting.md #2 explains why LIVE non-destructive effects need
// engine.rs (per-layer offscreen render + WGSL post-process pass). This
// is the honest baked version: rasterize the frame's layer content
// (reusing storyboard-mode's fixed rasterize technique — Item#rasterize
// resolution is DPI, 72=1:1, and the scratch Layer must stay ATTACHED
// while populated, both bugs found live in that prototype) with a CSS
// canvas filter string applied, then replace the layer's vector content
// with the single resulting Raster for that frame. Not live/tunable
// after baking — a real slider would need to re-bake; documented
// tradeoff, not hidden, same as clip-mask-bake.
//
//   SMLabs.bakeLayerEffect(layerIdx, 'blur(6px)')
//   SMLabs.bakeLayerEffect(layerIdx, 'blur(4px) brightness(1.3)')
//   Presets: SMLabs.bakeLayerEffect(layerIdx, 'bloom')     — soft glow
//            SMLabs.bakeLayerEffect(layerIdx, 'motion-h', {px:20}) — horizontal streak
// Only affects the CURRENT frame's stored strokes for that layer (bake
// per-frame if the whole animation needs it — a real adoption would
// batch this, kept single-frame here to keep the undo/postcondition
// easy to verify exactly).
(function () {
  var PRESETS = {
    bloom: 'blur(8px) brightness(1.4) saturate(1.2)',
  };

  function rasterizeLayer(layerIdx, frame, cssFilter, overscanPx) {
    var prevActiveLayer = project.activeLayer;
    var scratch = new Layer(); // attached while populated — see header
    var strokes = getEffectiveStrokes(layerIdx, frame);
    strokes.forEach(function (sd) { if (sd.isRaster) desR(sd, scratch); else desP(sd, scratch); });
    var bounds = scratch.children.length ? scratch.bounds : new Rectangle(0, 0, 1, 1);
    var pad = overscanPx || 0;
    var w = Math.max(1, Math.ceil(bounds.width + pad * 2));
    var h = Math.max(1, Math.ceil(bounds.height + pad * 2));
    var cv = document.createElement('canvas');
    cv.width = w; cv.height = h;
    var ctx = cv.getContext('2d');
    var raster = null;
    try {
      if (scratch.children.length) raster = scratch.rasterize({ resolution: 72, insert: false }); // 1 doc-unit = 1px
    } catch (e) { console.warn('[labs] bake rasterize a échoué', e); }
    if (raster && raster.canvas) {
      if (cssFilter) ctx.filter = cssFilter;
      // raster.canvas is already tightly cropped to the scratch content's
      // own bounds — just offset by the overscan padding so the filtered
      // result (e.g. a blur) has room to spread without being clipped.
      ctx.drawImage(raster.canvas, pad, pad);
      raster.remove();
    }
    scratch.remove();
    prevActiveLayer.activate();
    return { dataUrl: cv.toDataURL('image/png'), x: bounds.x - pad, y: bounds.y - pad, w: w, h: h, hadContent: !!raster };
  }

  window.SMLabs.bakeLayerEffect = function (layerIdx, effect, opts) {
    opts = opts || {};
    var ld = state.layers[layerIdx];
    if (!ld || ld.locked || ld.symbolId) { if (typeof showToast === 'function') showToast('Calque invalide/verrouillé'); return false; }
    var cssFilter = PRESETS[effect] || effect;
    var overscan = effect === 'bloom' ? 20 : (opts.overscan !== undefined ? opts.overscan : 12);
    var frame = state.currentFrame;
    var baked = rasterizeLayer(layerIdx, frame, cssFilter, overscan);
    if (!baked.hadContent) { if (typeof showToast === 'function') showToast('Calque vide sur cette frame'); return false; }

    pushUndo();
    ensureKeyframeAt(layerIdx, frame);
    var layer = userLayers[layerIdx];
    // Replace this frame's vector content with the single baked raster —
    // wipe live Paper children, then desR() it back in via the same
    // serialized-Raster path save/load already uses (data.src = dataUrl).
    layer.removeChildren();
    var rd = { isRaster: true, src: baked.dataUrl, x: baked.x + baked.w / 2, y: baked.y + baked.h / 2, width: baked.w, height: baked.h, opacity: 1 };
    desR(rd, layer);
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    if (typeof showToast === 'function') showToast('Effet appliqué (baké, frame ' + (frame + 1) + ') : ' + cssFilter);
    return true;
  };

  // ensureKeyframe (tools.js) only ever targets state.activeLayerIdx —
  // this bake can target ANY layer, so a tiny arbitrary-layer variant,
  // same freeze-then-promote shape as insertKeyframeAt (app.js).
  function ensureKeyframeAt(layerIdx, frame) {
    var f = state.layers[layerIdx].frames[frame];
    if (f.isKeyframe || f.isInterpolated) return;
    f.strokes = JSON.parse(JSON.stringify(getEffectiveStrokes(layerIdx, frame)));
    f.isKeyframe = true; f.isInterpolated = false;
    if (typeof syncLinkedKeyframeFolder === 'function') syncLinkedKeyframeFolder(layerIdx, frame);
  }

  window.SMLabs.register('layer-effects-bake', {
    flag: 'nemo-labs-effects',
    describe: 'Effets non-destructifs BAKÉS (pas live — voir feature-scouting #2) : SMLabs.bakeLayerEffect(layerIdx, cssFilter|\'bloom\') rastérise la frame courante avec un filtre canvas',
  });
})();
