// Descriptor construction and public lookup for Nemo's authored shader catalog.
// This module owns no shader execution or document/effect state.
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoShaderEffectCatalog = api;
})(typeof window === 'object' ? window : globalThis, function () {
  'use strict';
  // `spatial` (2026-07-29 fix, "un effet twirl qui bouge en fonction du
  // zoom du canvas"): engine.rs's run_one_effect deliberately does NOT
  // zoom-compensate any "custom:" effect's p1..p4 (its own comment: "p1..p4
  // there have no fixed meaning this function could assume is a pixel
  // size") — correct for a truly arbitrary user-authored shader, but this
  // SHIPPED library already knows exactly what each parameter means (e.g.
  // Twirl's Radius below is a distance from center, normalized 0-1 against
  // the FIXED-size render target/uv space — the exact same "effects change
  // size in the wrong direction as zoom changes" bug already fixed for the
  // built-in effects, just via a normalized UV radius here instead of a raw
  // pixel count). Flag any param here whose value is a spatial size/radius/
  // offset (angles, colors, ratios, counts, opacities are NOT spatial) —
  // sceneEffectsOf (engine-bridge.js) reads this flag and multiplies the
  // value by view.zoom before it ever reaches the engine, mirroring
  // run_one_effect's own `* z` compensation for the built-in effects.
  function param(key, label, min, max, step, unit, defaultValue, spatial) {
    var out = { key: key, label: label, min: min, max: max, step: step, scale: 1, unit: unit || '' };
    if (defaultValue !== undefined) out.defaultValue = defaultValue;
    if (spatial) out.spatial = true;
    return out;
  }
  function fx(id, name, category, params, lines) {
    return { id: id, name: name, category: category, params: params, source: lines.join('\n') };
  }

  function install(root, effects) {
    root.SMSHADER_EFFECT_CATEGORIES = ['Color', 'Blur', 'Stylize', 'Generate', 'Distort', 'Keying', 'Particles', 'Channel'];
    root.SMSHADER_EFFECTS = effects;
    root.SMShaderEffectDef = function (id) {
      for (var i = 0; i < effects.length; i++) if (effects[i].id === id) return effects[i];
      return null;
    };
  }
  return { param: param, fx: fx, install: install };
});
