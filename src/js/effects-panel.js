// ---- Effects stack panel (2026-07 rewrite) ----
// Feedback: "organisé les effets dans des menu un peu comme dans tuto...
// balayer un peu tous les effets d'after effects" + "dans ce menu effet il
// faudrait voir les effet appliqué au calque... et voir les paramètre de
// ces effets un peu comme ici ci on clic sur l'effet" (reference mockup:
// a Fill/Stroke/Effects-style panel where clicking an applied effect
// expands its own parameter editor inline).
//
// Replaces two things that used to be separate: the old single-select
// "Calque d'effet" grid (effect/adjustment layers only, one effect type
// max) and the old fixed Flou/Ombre-au-sol fields in the Layer section
// (ordinary layers only, one of each max). Both now read/write the SAME
// `ld.effects` array — {type, enabled, p1, p2, p3, p4}[] — matching AE's
// own per-layer effect STACK (any number, independently toggleable,
// applied in order). See engine.rs's LayerIn::effects doc comment for why
// one array works for both contexts (ordinary layers run it on their own
// isolated alpha; effect/adjustment layers run it on the accumulator).
(function () {
  var ICON_EYE = '<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2"><path d="M1 12s4-7 11-7 11 7 11 7-4 7-11 7-11-7-11-7z"/><circle cx="12" cy="12" r="3"/></svg>';
  var ICON_EYE_OFF = '<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 3l18 18M10.6 10.6a3 3 0 004.24 4.24M9.9 4.24A10.94 10.94 0 0112 4c7 0 11 7 11 7a13.2 13.2 0 01-3.22 3.94M6.5 6.5C3.5 8.3 1 12 1 12s2.2 3.9 6 5.9"/></svg>';

  // p1..p4 param config per effect type — same meaning/defaults as the
  // Rust side's own per-type defaults (engine.rs's run_one_effect), just
  // mirrored here for the UI's min/max/step/unit/scale. `scale` divides
  // the UI-displayed value down to the stored p1..p4 value (e.g. a 0-100
  // UI percentage stored as 0-1) — same convention used throughout this
  // app's other numeric fields (brightness/contrast/opacity, etc).
  var EFFECT_PARAM_CONFIG = {
    blur: [{ key: 'p1', label: 'Rayon', min: 0, max: 200, step: 1, scale: 1, unit: 'px' }],
    colorAdjust: [
      { key: 'p1', label: 'Luminosité', min: -100, max: 100, step: 1, scale: 100, unit: '' },
      { key: 'p2', label: 'Contraste', min: -100, max: 100, step: 1, scale: 100, unit: '' },
    ],
    vignette: [
      { key: 'p1', label: 'Intensité', min: 0, max: 100, step: 1, scale: 100, unit: '%' },
      { key: 'p2', label: 'Étendue', min: 0, max: 95, step: 1, scale: 100, unit: '%' },
    ],
    glow: [{ key: 'p1', label: 'Rayon', min: 0, max: 200, step: 1, scale: 1, unit: 'px' }],
    sepia: [], invert: [], grayscale: [],
    posterize: [{ key: 'p1', label: 'Niveaux', min: 2, max: 32, step: 1, scale: 1, unit: '' }],
    pixelate: [{ key: 'p1', label: 'Taille bloc', min: 2, max: 64, step: 1, scale: 1, unit: 'px' }],
    chromaticAberration: [{ key: 'p1', label: 'Intensité', min: 0, max: 20, step: 1, scale: 1, unit: 'px' }],
    scanlines: [
      { key: 'p1', label: 'Fréquence', min: 20, max: 480, step: 10, scale: 1, unit: '' },
      { key: 'p2', label: 'Intensité', min: 0, max: 100, step: 1, scale: 100, unit: '%' },
    ],
    grain: [{ key: 'p1', label: 'Intensité', min: 0, max: 100, step: 1, scale: 100, unit: '%' }],
    sharpen: [{ key: 'p1', label: 'Intensité', min: 0, max: 200, step: 1, scale: 100, unit: '%' }],
    edgeDetect: [{ key: 'p1', label: 'Intensité', min: 0, max: 20, step: 1, scale: 1, unit: '' }],
    groundShadow: [
      { key: 'p1', label: 'Inclinaison', min: -3, max: 3, step: 0.05, scale: 1, unit: '' },
      { key: 'p2', label: 'Sol (Y)', min: 0, max: 100, step: 1, scale: 100, unit: '%' },
      { key: 'p3', label: 'Longueur', min: 0.1, max: 4, step: 0.05, scale: 1, unit: '×' },
      { key: 'p4', label: 'Opacité', min: 0, max: 100, step: 1, scale: 100, unit: '%' },
    ],
  };
  var EFFECT_DEFAULTS = {
    blur: [8, 0, 0, 0], colorAdjust: [0, 0, 0, 0], vignette: [0.5, 0.4, 0, 0], glow: [16, 0, 0, 0],
    sepia: [0, 0, 0, 0], invert: [0, 0, 0, 0], grayscale: [0, 0, 0, 0], posterize: [6, 0, 0, 0],
    pixelate: [16, 0, 0, 0], chromaticAberration: [4, 0, 0, 0], scanlines: [240, 0.5, 0, 0],
    grain: [0.08, 0, 0, 0], sharpen: [0.5, 0, 0, 0], edgeDetect: [4, 0, 0, 0], groundShadow: [0, 0.75, 1, 0.5],
  };
  // Also read by renderLayerList's FX badge (timeline.js) via window.EFFECT_LABELS.
  var EFFECT_LABELS = {
    blur: 'Flou', colorAdjust: 'Teinte/Contraste', vignette: 'Vignette', glow: 'Glow',
    sepia: 'Sépia', invert: 'Inverser', grayscale: 'Niveaux de gris', posterize: 'Postériser',
    pixelate: 'Pixelliser', chromaticAberration: 'Aberration chromatique', scanlines: 'Lignes de balayage',
    grain: 'Grain film', sharpen: 'Netteté', edgeDetect: 'Détection de contours', groundShadow: 'Ombre au sol',
  };
  window.EFFECT_LABELS = EFFECT_LABELS;
  // Grouped like After Effects' own Effects menu (Blur & Sharpen, Color
  // Correction, Stylize, Distort, Generate) — rendered as a native
  // <select>+<optgroup> rather than a custom cascading menu (feedback:
  // "organisé les effets dans des menu"): far less UI surface to build/
  // test reliably while still giving the categorized-menu structure asked
  // for. `layerOnly` categories (groundShadow) are omitted from the menu
  // entirely for effect/adjustment layers — see engine.rs's LayerIn::effects
  // doc comment for why that effect only makes sense per-layer.
  var EFFECT_CATEGORIES = [
    { label: 'Flou & Netteté', types: ['blur', 'sharpen'] },
    { label: 'Couleur', types: ['colorAdjust', 'grayscale', 'sepia', 'posterize', 'invert'] },
    { label: 'Stylisation', types: ['vignette', 'glow', 'edgeDetect', 'grain', 'scanlines'] },
    { label: 'Distorsion', types: ['pixelate', 'chromaticAberration'] },
    { label: 'Ombres', types: ['groundShadow'], layerOnly: true },
  ];

  var expandedIdx = -1; // which row (if any) is showing its param editor

  function activeLayer() { return state.layers[state.activeLayerIdx]; }

  // Custom WGSL effects (custom-effects.js) live in state.customEffects,
  // not in the static tables above — these three helpers fall back to a
  // definition lookup whenever `type` is a "custom:<id>" string, so the
  // rest of this file (param rows, row labels, defaults on add) treats a
  // custom effect exactly like a built-in one without a parallel code path.
  function isCustomEffect(type) { return typeof type === 'string' && type.indexOf('custom:') === 0; }
  function customDefFor(type) { return isCustomEffect(type) && window.customEffectDef ? window.customEffectDef(type.slice(7)) : null; }
  function labelFor(type) {
    var cd = customDefFor(type);
    return cd ? cd.name : (EFFECT_LABELS[type] || type);
  }
  function paramConfigFor(type) {
    var cd = customDefFor(type);
    return cd ? cd.params : (EFFECT_PARAM_CONFIG[type] || []);
  }
  function defaultsArrFor(type) {
    var cd = customDefFor(type);
    if (cd) return cd.params.map(function (p) { return p.min; }).concat([0, 0, 0, 0]).slice(0, 4);
    return EFFECT_DEFAULTS[type] || [0, 0, 0, 0];
  }

  function renderAddMenu(isAdjustment) {
    var sel = document.getElementById('p-add-effect');
    if (!sel) return;
    sel.innerHTML = '<option value="">+ Add Effect…</option>';
    EFFECT_CATEGORIES.forEach(function (cat) {
      if (isAdjustment && cat.layerOnly) return;
      var group = document.createElement('optgroup');
      group.label = cat.label;
      cat.types.forEach(function (type) {
        var opt = document.createElement('option');
        opt.value = type; opt.textContent = EFFECT_LABELS[type] || type;
        group.appendChild(opt);
      });
      sel.appendChild(group);
    });
    // Custom shaders (2026-07) — user-authored effects saved in
    // state.customEffects, plus a fixed entry to open the authoring modal.
    var customGroup = document.createElement('optgroup');
    customGroup.label = 'Custom';
    (state.customEffects || []).forEach(function (c) {
      var opt = document.createElement('option');
      opt.value = 'custom:' + c.id; opt.textContent = c.name;
      customGroup.appendChild(opt);
    });
    var newOpt = document.createElement('option');
    newOpt.value = '__new_custom__'; newOpt.textContent = '+ New custom shader…';
    customGroup.appendChild(newOpt);
    sel.appendChild(customGroup);
  }

  function addEffect(type) {
    var ld = activeLayer(); if (!ld) return;
    pushUndo();
    if (!ld.effects) ld.effects = [];
    var d = defaultsArrFor(type);
    ld.effects.push({ type: type, enabled: true, p1: d[0], p2: d[1], p3: d[2], p4: d[3] });
    expandedIdx = ld.effects.length - 1; // open the new one immediately
    saveActiveLayerFrame(); renderEffectsSection(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }
  window.addEffectToActiveLayer = addEffect; // custom-effects.js calls this right after authoring a brand-new shader
  function toggleEnabled(idx) {
    var ld = activeLayer(); if (!ld || !ld.effects || !ld.effects[idx]) return;
    pushUndo();
    ld.effects[idx].enabled = !ld.effects[idx].enabled;
    saveActiveLayerFrame(); renderEffectsSection(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }
  function deleteEffect(idx) {
    var ld = activeLayer(); if (!ld || !ld.effects) return;
    pushUndo();
    ld.effects.splice(idx, 1);
    if (expandedIdx === idx) expandedIdx = -1; else if (expandedIdx > idx) expandedIdx--;
    saveActiveLayerFrame(); renderEffectsSection(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }
  function setParam(idx, key, raw) {
    var ld = activeLayer(); if (!ld || !ld.effects || !ld.effects[idx]) return;
    pushUndo();
    ld.effects[idx][key] = raw;
    saveActiveLayerFrame(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }

  function renderParamsFor(row, idx, eff) {
    var cfg = paramConfigFor(eff.type);
    var wrap = document.createElement('div');
    wrap.className = 'fx-row-params';
    if (!cfg.length) {
      var none = document.createElement('div');
      none.style.cssText = 'font-size:9px;color:var(--text-dim)';
      none.textContent = 'Aucun paramètre.';
      wrap.appendChild(none);
    }
    cfg.forEach(function (p) {
      var line = document.createElement('div'); line.className = 'fx-row-param';
      var label = document.createElement('span'); label.className = 'pl'; label.textContent = p.label;
      var input = document.createElement('input');
      input.type = 'number'; input.className = 'pi scrub';
      input.min = p.min; input.max = p.max; input.dataset.step = p.step;
      var stored = eff[p.key];
      var def = defaultsArrFor(eff.type)[{ p1: 0, p2: 1, p3: 2, p4: 3 }[p.key]];
      input.value = Math.round(((stored !== undefined ? stored : def) * (p.scale || 1)) * 100) / 100;
      var unit = document.createElement('span');
      unit.style.cssText = 'font-size:9px;color:var(--text-dim)'; unit.textContent = p.unit || '';
      input.addEventListener('input', function () { setParam(idx, p.key, (parseFloat(this.value) || 0) / (p.scale || 1)); });
      input.addEventListener('click', function (e) { e.stopPropagation(); });
      line.appendChild(label); line.appendChild(input); line.appendChild(unit);
      wrap.appendChild(line);
    });
    row.parentNode.insertBefore(wrap, row.nextSibling);
  }

  function renderEffectsList() {
    var list = document.getElementById('effects-list');
    if (!list) return;
    list.innerHTML = '';
    var ld = activeLayer();
    var effects = (ld && ld.effects) || [];
    effects.forEach(function (eff, idx) {
      var row = document.createElement('div');
      row.className = 'fx-row' + (eff.enabled ? '' : ' disabled') + (expandedIdx === idx ? ' expanded' : '');
      var eye = document.createElement('div'); eye.className = 'fx-row-eye';
      eye.innerHTML = eff.enabled ? ICON_EYE : ICON_EYE_OFF;
      eye.title = eff.enabled ? 'Désactiver' : 'Activer';
      eye.addEventListener('click', function (e) { e.stopPropagation(); toggleEnabled(idx); });
      var name = document.createElement('span'); name.className = 'fx-row-name';
      name.textContent = labelFor(eff.type);
      row.appendChild(eye); row.appendChild(name);
      // Edit the shader source (custom effects only — built-ins have no
      // source to edit, just params, already reachable by expanding the row).
      if (isCustomEffect(eff.type)) {
        var edit = document.createElement('div'); edit.className = 'fx-row-del'; edit.textContent = '✎'; edit.title = 'Modifier le shader';
        edit.addEventListener('click', function (e) {
          e.stopPropagation();
          var def = customDefFor(eff.type);
          if (def && window.openCustomEffectEditor) window.openCustomEffectEditor(def);
        });
        row.appendChild(edit);
      }
      var del = document.createElement('div'); del.className = 'fx-row-del'; del.textContent = '×'; del.title = 'Supprimer';
      del.addEventListener('click', function (e) { e.stopPropagation(); deleteEffect(idx); });
      row.appendChild(del);
      row.addEventListener('click', function () {
        expandedIdx = expandedIdx === idx ? -1 : idx;
        renderEffectsList();
      });
      list.appendChild(row);
      if (expandedIdx === idx) renderParamsFor(row, idx, eff);
    });
  }

  function renderEffectsSection() {
    var sec = document.getElementById('effects-stack-sec');
    if (!sec) return;
    var ld = activeLayer();
    // Null layers never paint anything (pure organizational pivot — see
    // CLAUDE.md §1) — an effects stack on their own isolated alpha would
    // be a visible no-op, so skip showing this section for them.
    var applicable = ld && !ld.isNullLayer;
    sec.style.display = applicable ? '' : 'none';
    if (!applicable) return;
    var hint = document.getElementById('effects-adj-hint');
    if (hint) hint.style.display = ld.isEffectLayer ? '' : 'none';
    renderAddMenu(!!ld.isEffectLayer);
    renderEffectsList();
  }
  window.updateEffectsPanel = renderEffectsSection;

  function init() {
    var sec = document.getElementById('effects-stack-sec');
    if (!sec) return;
    var addSel = document.getElementById('p-add-effect');
    if (addSel) addSel.addEventListener('change', function () {
      var type = this.value; this.value = '';
      if (type === '__new_custom__') { if (window.openCustomEffectEditor) window.openCustomEffectEditor(null); return; }
      if (type) addEffect(type);
    });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();
})();
