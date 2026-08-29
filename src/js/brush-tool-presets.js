// ---- Brush tool-options presets (feedback #101, 2026-08) ----
// "dans le tool option de la brush, il serait bien de pouvoir s'en
// enregistrer quelques uns, de paramètres predefinis, avec taille,
// pression etc" — save/recall a few NAMED bundles of the Draw/Pen/Brush
// tool's numeric & boolean options (size, stabilizer, smoothing, pressure
// min/max/curve/invert/custom-curve, nib angle/force, taper, trim
// overshoots, shadow/mask mode, eraser size, draw mode).
//
// Deliberately mirrors motion-preset-picker.js's shape: a GLOBAL,
// cross-project library backed by localStorage (never touched by New
// Project / project load — these are personal tool defaults, not project
// data). Distinct from the brush TEXTURE picker (brush-preset-picker.js,
// state.brushPreset) — that's a single string key into a texture recipe;
// this is a bundle of independent numeric/boolean fields with no slot in
// that model. See that file's own comment for the full reasoning; do not
// conflate the two favorites/preset strips.
(function () {
  var LS_KEY = 'nemo-brush-tool-presets';
  var _presets = {};

  // field -> {dom, kind}. kind drives how the value is read back onto the
  // matching input on apply ('num' rounds only when round:true, e.g. the
  // brush-size field which is always displayed as an integer elsewhere).
  var FIELDS = {
    brushSize: { dom: 'p-sw', kind: 'num', round: true },
    stabilizer: { dom: 'p-stab', kind: 'select' },
    smoothing: { dom: 'p-smooth', kind: 'num' },
    vectorBrush: { dom: 'p-vecbrush', kind: 'check' },
    pressureMin: { dom: 'p-pmin', kind: 'num' },
    pressureMax: { dom: 'p-pmax', kind: 'num' },
    pressureCurve: { dom: 'p-pcurve', kind: 'select' },
    pressureInvert: { dom: 'p-pinv', kind: 'check' },
    brushAngle: { dom: 'p-nib-angle', kind: 'num' },
    brushAngleFactor: { dom: 'p-nib-factor', kind: 'num' },
    taperEnds: { dom: 'p-taper', kind: 'check' },
    trimStrokeEnds: { dom: 'p-trimends', kind: 'check' },
    trimStrokeEndsMax: { dom: 'p-trimends-max', kind: 'num' },
    shadowMode: { dom: 'p-shadowmode', kind: 'check' },
    maskMode: { dom: 'p-maskmode', kind: 'check' },
    maskModeType: { dom: 'p-maskmode-type', kind: 'select' },
    eraserSize: { dom: 'p-erasersize', kind: 'num' },
    drawMode: { dom: 'p-drawmode', kind: 'select' },
  };

  function load() {
    try { _presets = JSON.parse(localStorage.getItem(LS_KEY) || '{}') || {}; } catch (e) { _presets = {}; }
  }
  function persist() { try { localStorage.setItem(LS_KEY, JSON.stringify(_presets)); } catch (e) {} }
  function newId() { return 'btp-' + Date.now().toString(36) + '-' + Math.floor(Math.random() * 1e4); }

  function render() {
    var grid = document.getElementById('toolpreset-grid');
    if (!grid) return;
    var ids = Object.keys(_presets);
    grid.style.display = ids.length ? '' : 'none';
    grid.innerHTML = '';
    ids.forEach(function (id) {
      var p = _presets[id];
      var item = document.createElement('button');
      item.className = 'bp-item bp-item-custom';
      var icon = document.createElement('span');
      icon.className = 'bp-item-icon';
      icon.textContent = '◆'; // same filled-diamond glyph motion-preset-picker uses
      item.appendChild(icon);
      var span = document.createElement('span');
      span.textContent = p.label || id;
      item.appendChild(span);
      var del = document.createElement('span');
      del.className = 'bp-item-del';
      del.textContent = '×';
      del.title = 'Supprimer ce preset';
      del.addEventListener('click', function (e) {
        e.stopPropagation();
        delete _presets[id];
        persist();
        render();
      });
      item.appendChild(del);
      item.addEventListener('click', function () { applyPreset(id); });
      grid.appendChild(item);
    });
  }
  window.renderBrushToolPresetPanel = render;

  function savePreset() {
    var name = prompt('Nom du preset', 'Preset');
    if (!name || !name.trim()) return;
    var id = newId();
    var fields = {};
    Object.keys(FIELDS).forEach(function (k) { fields[k] = state[k]; });
    // Custom pressure-curve points (from "Éditer la courbe…") ride along
    // too — deep-cloned like motion-preset-picker's own motion/motionStatic
    // clone, so later edits to the live curve don't mutate a saved preset.
    fields.pressureCurvePoints = state.pressureCurvePoints ? JSON.parse(JSON.stringify(state.pressureCurvePoints)) : null;
    _presets[id] = { label: name.trim(), fields: fields };
    persist();
    render();
    if (window.showToast) showToast('Preset "' + name.trim() + '" enregistré');
  }

  function applyPreset(id) {
    var p = _presets[id];
    if (!p) return;
    var f = p.fields;
    Object.keys(FIELDS).forEach(function (k) {
      if (f[k] !== undefined) state[k] = f[k];
    });
    state.pressureCurvePoints = f.pressureCurvePoints ? JSON.parse(JSON.stringify(f.pressureCurvePoints)) : null;
    // Paint every mapped input back from state — same idiom as
    // timeline.js's _restoreDrawingDefaults (direct state assignment, then
    // explicit DOM sync): these fields' SM.setXxx wrappers exist to react
    // to a live user edit (e.g. setBrushSize restyling the current
    // selection when the Select tool is active) — a side effect we do NOT
    // want here, this is a tool-default bundle, not a selection edit.
    Object.keys(FIELDS).forEach(function (k) {
      var m = FIELDS[k], el = document.getElementById(m.dom);
      if (!el || f[k] === undefined) return;
      if (m.kind === 'check') el.checked = !!f[k];
      else if (m.kind === 'num') el.value = m.round ? Math.round(f[k]) : f[k];
      else el.value = f[k];
    });
    if (window.updateUI) updateUI();
    if (window.showToast) showToast('Preset "' + (p.label || id) + '" appliqué');
  }

  var saveBtn = document.getElementById('btn-toolpreset-save');
  if (saveBtn) saveBtn.addEventListener('click', savePreset);

  load();
  render();
})();
