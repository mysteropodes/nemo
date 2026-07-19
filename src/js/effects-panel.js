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
  // `label` holds an i18n KEY (not resolved text) — this config object is
  // built once at script-parse time, before i18n.js's initLanguage() has
  // even run (DOMContentLoaded), so baking window.SM.t() results in here
  // would freeze every param label to i18n's pre-init 'fr' fallback
  // regardless of the user's actual saved language. Resolved via
  // window.SM.t(p.label) at render time instead (renderParamsFor below).
  var EFFECT_PARAM_CONFIG = {
    blur: [{ key: 'p1', label: 'fxParamRadius', min: 0, max: 200, step: 1, scale: 1, unit: 'px' }],
    colorAdjust: [
      { key: 'p1', label: 'fxParamBrightness', min: -100, max: 100, step: 1, scale: 100, unit: '' },
      { key: 'p2', label: 'fxParamContrast', min: -100, max: 100, step: 1, scale: 100, unit: '' },
    ],
    vignette: [
      { key: 'p1', label: 'fxParamIntensity', min: 0, max: 100, step: 1, scale: 100, unit: '%' },
      { key: 'p2', label: 'fxParamSpread', min: 0, max: 95, step: 1, scale: 100, unit: '%' },
    ],
    glow: [{ key: 'p1', label: 'fxParamRadius', min: 0, max: 200, step: 1, scale: 1, unit: 'px' }],
    sepia: [], invert: [], grayscale: [],
    posterize: [{ key: 'p1', label: 'fxParamLevels', min: 2, max: 32, step: 1, scale: 1, unit: '' }],
    pixelate: [{ key: 'p1', label: 'fxParamBlockSize', min: 2, max: 64, step: 1, scale: 1, unit: 'px' }],
    chromaticAberration: [{ key: 'p1', label: 'fxParamIntensity', min: 0, max: 20, step: 1, scale: 1, unit: 'px' }],
    scanlines: [
      { key: 'p1', label: 'fxParamFrequency', min: 20, max: 480, step: 10, scale: 1, unit: '' },
      { key: 'p2', label: 'fxParamIntensity', min: 0, max: 100, step: 1, scale: 100, unit: '%' },
    ],
    grain: [{ key: 'p1', label: 'fxParamIntensity', min: 0, max: 100, step: 1, scale: 100, unit: '%' }],
    sharpen: [{ key: 'p1', label: 'fxParamIntensity', min: 0, max: 200, step: 1, scale: 100, unit: '%' }],
    edgeDetect: [{ key: 'p1', label: 'fxParamIntensity', min: 0, max: 20, step: 1, scale: 1, unit: '' }],
    threshold: [
      { key: 'p1', label: 'fxParamThreshold', min: 0, max: 100, step: 1, scale: 100, unit: '%' },
      { key: 'p2', label: 'fxParamSoftness', min: 0, max: 50, step: 1, scale: 100, unit: '%' },
    ],
    halftone: [
      { key: 'p1', label: 'fxParamCellSize', min: 2, max: 40, step: 1, scale: 1, unit: 'px' },
      { key: 'p2', label: 'fxParamIntensity', min: 0, max: 100, step: 1, scale: 100, unit: '%' },
    ],
    // layerOnly (see EFFECT_CATEGORIES) — needs the layer's own isolated
    // alpha silhouette, same constraint as groundShadow below.
    contourBrut: [
      { key: 'p1', label: 'fxParamThickness', min: 0.5, max: 20, step: 0.5, scale: 1, unit: 'px' },
      { key: 'p2', label: 'fxParamRoughness', min: 0, max: 100, step: 1, scale: 100, unit: '%' },
      { key: 'p3', label: 'fxParamBrightness', min: 0, max: 100, step: 1, scale: 100, unit: '%' },
      { key: 'p4', label: 'fxParamOpacity', min: 0, max: 100, step: 1, scale: 100, unit: '%' },
    ],
    groundShadow: [
      { key: 'p1', label: 'fxParamTilt', min: -3, max: 3, step: 0.05, scale: 1, unit: '' },
      // max capped at 90 (not 100) — feedback: "l'effet ombre au sol ne
      // marche pas j'ai l'impression". At 100% the ground line sits
      // exactly on the canvas' last row, leaving ZERO room below it for
      // the shadow to render into (the math is correct — y is never >
      // ground_y when ground_y is the frame's own bottom edge — but it
      // LOOKS broken: the slider still moves, nothing ever appears).
      // Capping at 90 guarantees at least a sliver of canvas is always
      // available for the shadow, regardless of Longueur.
      { key: 'p2', label: 'fxParamGroundY', min: 0, max: 90, step: 1, scale: 100, unit: '%' },
      { key: 'p3', label: 'fxParamLength', min: 0.1, max: 4, step: 0.05, scale: 1, unit: '×' },
      { key: 'p4', label: 'fxParamOpacity', min: 0, max: 100, step: 1, scale: 100, unit: '%' },
    ],
  };
  var EFFECT_DEFAULTS = {
    blur: [8, 0, 0, 0], colorAdjust: [0, 0, 0, 0], vignette: [0.5, 0.4, 0, 0], glow: [16, 0, 0, 0],
    sepia: [0, 0, 0, 0], invert: [0, 0, 0, 0], grayscale: [0, 0, 0, 0], posterize: [6, 0, 0, 0],
    pixelate: [16, 0, 0, 0], chromaticAberration: [4, 0, 0, 0], scanlines: [240, 0.5, 0, 0],
    grain: [0.08, 0, 0, 0], sharpen: [0.5, 0, 0, 0], edgeDetect: [4, 0, 0, 0],
    threshold: [0.5, 0.08, 0, 0], halftone: [10, 0.9, 0, 0], contourBrut: [3, 0.4, 0, 0.9],
    // groundShadow default was [0, 0.75, 1, 0.5] — at ground=75%/length=1 the
    // shader's inverse-mapped source row only reaches sy≈ground-(1-ground)
    // ≈49.7% of canvas height, right at the edge of most centered shapes'
    // own bottom edge: the shadow existed but was a near-invisible sliver
    // (root cause of "l'effet ombre au sol ne marche pas j'ai l'impression").
    // Lower ground + shorter length reach much further up the source
    // regardless of exact shape position, so the effect is visible the
    // moment it's added instead of requiring manual tuning to find it.
    groundShadow: [0, 0.62, 0.6, 0.65],
  };
  // Effect type -> i18n key (not resolved text, same reasoning as
  // EFFECT_PARAM_CONFIG's `label` above). Also read by renderLayerList's FX
  // badge (timeline.js) via window.EFFECT_LABELS — exposed as a Proxy so
  // that consumer's plain `fxLabels[type]` property reads stay a live
  // lookup (resolved in the CURRENT language at access time) rather than a
  // snapshot frozen at whatever language was active when this script parsed.
  var EFFECT_LABEL_KEYS = {
    blur: 'fxTypeBlur', colorAdjust: 'fxTypeColorAdjust', vignette: 'fxTypeVignette', glow: 'fxTypeGlow',
    sepia: 'fxTypeSepia', invert: 'fxTypeInvert', grayscale: 'fxTypeGrayscale', posterize: 'fxTypePosterize',
    pixelate: 'fxTypePixelate', chromaticAberration: 'fxTypeChromaticAberration', scanlines: 'fxTypeScanlines',
    grain: 'fxTypeGrain', sharpen: 'fxTypeSharpen', edgeDetect: 'fxTypeEdgeDetect', groundShadow: 'fxTypeGroundShadow',
    threshold: 'fxTypeThreshold', halftone: 'fxTypeHalftone', contourBrut: 'fxTypeContourBrut',
  };
  var EFFECT_LABELS = new Proxy({}, {
    get: function (target, type) {
      var key = EFFECT_LABEL_KEYS[type];
      return key ? window.SM.t(key) : undefined;
    },
  });
  window.EFFECT_LABELS = EFFECT_LABELS;
  // Grouped like After Effects' own Effects menu (Blur & Sharpen, Color
  // Correction, Stylize, Distort, Generate) — rendered as a categorized
  // flyout with a preview swatch per effect (buildAddEffectMenu below).
  // `layerOnly` categories (Ombres, Contours) are omitted from the menu
  // entirely for effect/adjustment layers — see engine.rs's LayerIn::effects
  // doc comment for why those effects only make sense per-layer. `label` is
  // an i18n key, resolved at render time in openAddMenu (same reasoning as
  // EFFECT_PARAM_CONFIG's `label` above).
  var EFFECT_CATEGORIES = [
    { label: 'fxCatBlurSharpen', types: ['blur', 'sharpen'] },
    { label: 'fxCatColor', types: ['colorAdjust', 'grayscale', 'sepia', 'posterize', 'invert', 'threshold'] },
    { label: 'fxCatStylize', types: ['vignette', 'glow', 'edgeDetect', 'grain', 'scanlines', 'halftone'] },
    { label: 'fxCatDistort', types: ['pixelate', 'chromaticAberration'] },
    { label: 'fxCatContours', types: ['contourBrut'], layerOnly: true },
    { label: 'fxCatShadows', types: ['groundShadow'], layerOnly: true },
  ];

  var expandedIdx = -1; // which row (if any) is showing its param editor

  function activeLayer() { return state.layers[state.activeLayerIdx]; }

  // Per-ELEMENT effects (2026-07, "possible de différencié les effet par
  // éléments sélectionné si j'applique un effet sur un élément select alors
  // il ne s'applique sur celui-ci") — same singleTarget() convention
  // gradient-bridge.js already established for "exactly one shape selected
  // with Select/Subselect": whenever that's true, the Effects panel targets
  // THAT element's own effects (p.data.effects, serP/desP-persisted,
  // engine.rs's ItemIn.effects) instead of the whole layer's. Falls back to
  // the layer the moment 0 or 2+ elements are selected, or a non-select
  // tool is active — no separate manual toggle, matches gradient-bridge's
  // own automatic-by-selection precedent.
  function singleSelectedElement() {
    if ((state.tool !== 'select' && state.tool !== 'subselect') || !window.selectedPaths || selectedPaths.length !== 1) return null;
    var p = selectedPaths[0];
    return (p instanceof Path || p instanceof CompoundPath) ? p : null;
  }
  // Returns {obj, isElement, item?} — `obj` is whichever plain object
  // actually carries the `effects` array (a layer, or a selected element's
  // `.data`), so every effects-list function below reads/writes through
  // `obj.effects` uniformly without needing its own element/layer branch.
  function effectsTarget() {
    var el = singleSelectedElement();
    if (el) {
      if (!el.data) el.data = {};
      return { obj: el.data, isElement: true, item: el };
    }
    var ld = activeLayer();
    return ld ? { obj: ld, isElement: false } : null;
  }

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

  // Categorized flyout menu with preview thumbnails (2026-07 rewrite —
  // feedback: "on a pas de préviz vignette et de sous menu. Pense bien
  // l'architecture ui qui marcherait le mieux"). Replaces the plain
  // <select>+<optgroup> (functional but no visual preview and no real
  // nested-menu structure) with a two-level flyout matching how AE/most
  // desktop apps present a categorized "Add Effect" menu: a category list
  // that opens a submenu of preview tiles (small CSS-approximation
  // thumbnail + label) to the side. Built FRESH every time it's opened
  // (not cached) since which categories/custom effects are available can
  // change between opens (adjustment vs ordinary layer, a shader just
  // authored, etc).
  var addMenuEl = null, subMenuEl = null;
  function closeAddMenu() {
    if (addMenuEl) { addMenuEl.remove(); addMenuEl = null; }
    if (subMenuEl) { subMenuEl.remove(); subMenuEl = null; }
  }
  function openSubmenu(anchorEl, items) {
    if (subMenuEl) { subMenuEl.remove(); subMenuEl = null; }
    var sub = document.createElement('div');
    sub.className = 'fx-submenu';
    items.forEach(function (item) {
      var row = document.createElement('div');
      row.className = 'fx-submenu-item';
      var prev = document.createElement('div');
      prev.className = 'fx-prev' + (item.preview ? ' fx-prev-' + item.preview : ' fx-prev-generic');
      var lbl = document.createElement('span'); lbl.textContent = item.label;
      row.appendChild(prev); row.appendChild(lbl);
      row.addEventListener('click', function (e) { e.stopPropagation(); closeAddMenu(); item.action(); });
      sub.appendChild(row);
    });
    document.body.appendChild(sub);
    var r = anchorEl.getBoundingClientRect();
    var sw = sub.offsetWidth, sh = sub.offsetHeight;
    var left = r.right + 2;
    if (left + sw > window.innerWidth - 4) left = r.left - sw - 2;
    var top = Math.min(r.top, window.innerHeight - sh - 4);
    sub.style.left = left + 'px'; sub.style.top = top + 'px';
    subMenuEl = sub;
  }
  function openAddMenu(anchorEl, isAdjustment) {
    closeAddMenu();
    var menu = document.createElement('div');
    menu.className = 'fx-addmenu';
    EFFECT_CATEGORIES.forEach(function (cat) {
      if (isAdjustment && cat.layerOnly) return;
      var row = document.createElement('div');
      row.className = 'fx-addmenu-cat';
      row.innerHTML = '<span>' + window.SM.t(cat.label) + '</span><span class="fx-addmenu-arrow">›</span>';
      var open = function () {
        openSubmenu(row, cat.types.map(function (type) {
          return { label: EFFECT_LABELS[type] || type, preview: type, action: function () { addEffect(type); } };
        }));
      };
      row.addEventListener('mouseenter', open);
      row.addEventListener('click', function (e) { e.stopPropagation(); open(); });
      menu.appendChild(row);
    });
    // Custom shaders (2026-07) — user-authored effects saved in
    // state.customEffects, plus a fixed entry to open the authoring modal.
    var customRow = document.createElement('div');
    customRow.className = 'fx-addmenu-cat';
    customRow.innerHTML = '<span>' + window.SM.t('fxCatCustom') + '</span><span class="fx-addmenu-arrow">›</span>';
    var openCustom = function () {
      var items = (state.customEffects || []).map(function (c) {
        return { label: c.name, preview: null, action: function () { addEffect('custom:' + c.id); } };
      });
      items.push({ label: '+ New custom shader…', preview: null, action: function () { if (window.openCustomEffectEditor) window.openCustomEffectEditor(null); } });
      openSubmenu(customRow, items);
    };
    customRow.addEventListener('mouseenter', openCustom);
    customRow.addEventListener('click', function (e) { e.stopPropagation(); openCustom(); });
    menu.appendChild(customRow);
    document.body.appendChild(menu);
    var r = anchorEl.getBoundingClientRect();
    var mw = menu.offsetWidth, mh = menu.offsetHeight;
    menu.style.left = Math.min(r.left, window.innerWidth - mw - 4) + 'px';
    menu.style.top = Math.min(r.bottom + 2, window.innerHeight - mh - 4) + 'px';
    addMenuEl = menu;
  }
  document.addEventListener('pointerdown', function (e) {
    if (addMenuEl && !addMenuEl.contains(e.target) && (!subMenuEl || !subMenuEl.contains(e.target)) && e.target.id !== 'p-add-effect-btn') closeAddMenu();
  }, true);
  window.addEventListener('keydown', function (e) { if (e.key === 'Escape') closeAddMenu(); });

  function addEffect(type) {
    var t = effectsTarget(); if (!t) return;
    pushUndo();
    if (!t.obj.effects) t.obj.effects = [];
    var d = defaultsArrFor(type);
    t.obj.effects.push({ type: type, enabled: true, p1: d[0], p2: d[1], p3: d[2], p4: d[3] });
    expandedIdx = t.obj.effects.length - 1; // open the new one immediately
    saveActiveLayerFrame(); renderEffectsSection(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }
  window.addEffectToActiveLayer = addEffect; // custom-effects.js calls this right after authoring a brand-new shader
  function toggleEnabled(idx) {
    var t = effectsTarget(); if (!t || !t.obj.effects || !t.obj.effects[idx]) return;
    pushUndo();
    t.obj.effects[idx].enabled = !t.obj.effects[idx].enabled;
    saveActiveLayerFrame(); renderEffectsSection(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }
  function deleteEffect(idx) {
    var t = effectsTarget(); if (!t || !t.obj.effects) return;
    pushUndo();
    t.obj.effects.splice(idx, 1);
    if (expandedIdx === idx) expandedIdx = -1; else if (expandedIdx > idx) expandedIdx--;
    saveActiveLayerFrame(); renderEffectsSection(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }
  function setParam(idx, key, raw) {
    var t = effectsTarget(); if (!t || !t.obj.effects || !t.obj.effects[idx]) return;
    pushUndo();
    t.obj.effects[idx][key] = raw;
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
      var label = document.createElement('span'); label.className = 'pl'; label.textContent = window.SM.t(p.label);
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
    var t = effectsTarget();
    var effects = (t && t.obj.effects) || [];
    effects.forEach(function (eff, idx) {
      var row = document.createElement('div');
      row.className = 'fx-row' + (eff.enabled ? '' : ' disabled') + (expandedIdx === idx ? ' expanded' : '');
      // Explicit disclosure chevron (feedback: "faudrait pouvoir déplier
      // ou replier les paramètre de l'effet") — the whole row already
      // toggled its own param panel on click, but with no visual cue that
      // it's expandable/collapsible at all. Own click handler (not just
      // relying on the row's) so it works even where the row's own
      // listener might be intercepted, and rotates via CSS to show state.
      var chevron = document.createElement('div'); chevron.className = 'fx-row-chevron';
      chevron.innerHTML = '<svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2.4"><path d="M9 6l6 6-6 6"/></svg>';
      chevron.title = window.SM.t(expandedIdx === idx ? 'fxCollapse' : 'fxExpand');
      chevron.addEventListener('click', function (e) { e.stopPropagation(); expandedIdx = expandedIdx === idx ? -1 : idx; renderEffectsList(); });
      var eye = document.createElement('div'); eye.className = 'fx-row-eye';
      eye.innerHTML = eff.enabled ? ICON_EYE : ICON_EYE_OFF;
      eye.title = window.SM.t(eff.enabled ? 'fxDisable' : 'fxEnable');
      eye.addEventListener('click', function (e) { e.stopPropagation(); toggleEnabled(idx); });
      var name = document.createElement('span'); name.className = 'fx-row-name';
      name.textContent = labelFor(eff.type);
      row.appendChild(chevron); row.appendChild(eye); row.appendChild(name);
      // Edit the shader source (custom effects only — built-ins have no
      // source to edit, just params, already reachable by expanding the row).
      if (isCustomEffect(eff.type)) {
        var edit = document.createElement('div'); edit.className = 'fx-row-del'; edit.textContent = '✎'; edit.title = window.SM.t('fxEditShader');
        edit.addEventListener('click', function (e) {
          e.stopPropagation();
          var def = customDefFor(eff.type);
          if (def && window.openCustomEffectEditor) window.openCustomEffectEditor(def);
        });
        row.appendChild(edit);
      }
      var del = document.createElement('div'); del.className = 'fx-row-del'; del.textContent = '×'; del.title = window.SM.t('fxDelete');
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
    var targetHint = document.getElementById('effects-target-hint');
    if (targetHint) targetHint.style.display = singleSelectedElement() ? '' : 'none';
    renderEffectsList();
  }
  window.updateEffectsPanel = renderEffectsSection;

  function init() {
    var sec = document.getElementById('effects-stack-sec');
    if (!sec) return;
    var addBtn = document.getElementById('p-add-effect-btn');
    if (addBtn) addBtn.addEventListener('click', function (e) {
      e.stopPropagation();
      var ld = activeLayer();
      openAddMenu(addBtn, !!(ld && ld.isEffectLayer));
    });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();
})();
