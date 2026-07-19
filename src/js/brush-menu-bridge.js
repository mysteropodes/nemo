// ---- BRUSH FLOATING MENU (2026-07, Sketchbook-style) ----
// Feedback: "j'aimerais avoir un menu... dans la barre flottante quand on
// est sur brush ça ouvre un menu avec toute les brush et des paramètre...
// 2 onglet vecto et bitmap." A big popover (opened from a new button in
// the SAME floating strip labs-float-panel.js already shows for the Draw/
// Fill Brush tools) with two tabs:
//   - Vecteur: this app's existing vector-approximated brush-texture
//     presets (state.brushPreset) — reuses brush-preset-picker.js's own
//     catalog/preview/select logic (BrushPresetPicker.groups/drawPreview/
//     labelFor/selectPreset) rather than a second copy that could drift.
//   - Bitmap: the Bitmap Brush tip library (state.bitmapTip) — same reuse
//     of bitmap-tip-picker.js/SMBitmapBrush's own exposed API.
// Both grids use the exact `.bp-grid`/`.bp-item`/`.bp-group-label` CSS
// classes the existing small swatch popovers already use (style.css), so
// this reads as the SAME visual language, just a bigger surface — not a
// new picker aesthetic.
//
// The parameter footer mirrors a handful of the most relevant existing
// inputs (Width/Smoothing for Vecteur; Spacing/Scatter/Opacity/Pressure for
// Bitmap) rather than becoming a second source of truth: each mirrored
// control writes to the SAME state field the original panel input does
// (and pushes its new value back into that original DOM element too, so
// the right panel never goes stale while this popover is open).
(function () {
  var popover = null, closeHandlers = null;
  var currentTab = 'vector';

  function closePopover() {
    if (!popover) return;
    // A live hover-preview (see below) must never survive the popover
    // closing some OTHER way than a plain mouseleave (outside click,
    // Escape, re-toggle) — none of those fire the swatch's own mouseleave,
    // so without this an uncommitted preview (no pushUndo, no
    // saveActiveLayerFrame) could linger applied to the live Paper objects.
    revertBrushPreview();
    popover.remove();
    popover = null;
    if (closeHandlers) { closeHandlers(); closeHandlers = null; }
    if (window.renderLabsFloatPanel) renderLabsFloatPanel();
  }
  function isOpen() { return !!popover; }

  // ---- Hover-preview (2026-07, "un hover en preview sur le canvas comme
  // pour le blend-mode des calques") — mirrors initBlendDropdown's own
  // applyPreview/revert pattern (timeline.js): apply live on mouseenter, no
  // undo/save; revert to the pre-hover snapshot on mouseleave. Unlike a
  // blend-mode string, a brush/tip is a STRUCTURAL edit (creates/removes
  // companion dab objects via stripAnyBrushTexture/applyBrushTexture), so
  // "revert" means re-running that same strip+apply pair against a snapshot
  // of each path's ORIGINAL texture (preset key or full bitmapSpec object),
  // not just restoring a scalar.
  var _previewSnapshot = null;
  function eligiblePathsForPreview() {
    if (!(state.tool === 'select' || state.tool === 'subselect') || !selectedPaths.length) return [];
    return selectedPaths.filter(function (p) { return p instanceof Path && !(p.data && (p.data.isVectorBrush || p.data.isFillShape)); });
  }
  function beginBrushPreview() {
    if (_previewSnapshot) return;
    _previewSnapshot = eligiblePathsForPreview().map(function (p) {
      return {
        p: p,
        preset: (p.data && p.data.brushTexturePreset) || null,
        bitmapSpec: (p.data && p.data.bitmapBrushSpec) ? JSON.parse(JSON.stringify(p.data.bitmapBrushSpec)) : null,
      };
    });
  }
  function previewVectorBrush(key) {
    beginBrushPreview();
    if (!_previewSnapshot.length) return;
    _previewSnapshot.forEach(function (rec) {
      stripAnyBrushTexture(rec.p);
      if (key && key !== 'none') applyBrushTexture(rec.p, key);
    });
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  // Explicit spec (not the current-selection default applyBitmapBrushTexture
  // would otherwise fall back to) so hovering TIP B while TIP A is actually
  // selected previews B, not a no-op re-application of A — mirrors that
  // function's own default-spec shape (bitmap-brush.js) intentionally.
  function previewBitmapBrush(tipKey) {
    beginBrushPreview();
    if (!_previewSnapshot.length) return;
    _previewSnapshot.forEach(function (rec) {
      var p = rec.p;
      var spec = {
        tip: tipKey,
        size: state.brushSize || 40,
        spacing: state.bitmapSpacing != null ? state.bitmapSpacing : 15,
        scatter: state.bitmapScatter != null ? state.bitmapScatter : 20,
        opacity: (state.bitmapOpacity != null ? state.bitmapOpacity : 100) / 100,
        color: p.strokeColor ? colorHex8(p.strokeColor) : ((p.data && p.data.preTextureStroke) || state.strokeColor || '#000000'),
        pressure: !!state.bitmapPressure,
        seed: Math.floor(Math.random() * 0xffffffff),
      };
      stripAnyBrushTexture(p);
      applyBitmapBrushTexture(p, spec);
    });
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  function revertBrushPreview() {
    if (!_previewSnapshot) return;
    _previewSnapshot.forEach(function (rec) {
      stripAnyBrushTexture(rec.p);
      if (rec.bitmapSpec) applyBitmapBrushTexture(rec.p, rec.bitmapSpec);
      else if (rec.preset) applyBrushTexture(rec.p, rec.preset);
    });
    _previewSnapshot = null;
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  // Called right before a REAL commit (a swatch click) — the preview must
  // be undone first so the commit's own pushUndo() snapshots the true
  // pre-hover state, not the already-mutated preview (else Ctrl+Z would
  // restore to the hovered-but-not-chosen brush instead of the original).
  function commitBrushPreview() { revertBrushPreview(); }

  // ---- shared param-row builder — a `.pr`/`.pl`/`.pi` row identical in
  // markup to the ones already in index.html's right panel, so it's
  // visually indistinguishable from the rest of the app's own controls.
  function numRow(container, label, value, min, max, step, onChange, title) {
    var row = document.createElement('div'); row.className = 'pr'; if (title) row.title = title;
    var lbl = document.createElement('span'); lbl.className = 'pl'; lbl.textContent = label;
    var inp = document.createElement('input');
    inp.type = 'number'; inp.className = 'pi scrub'; inp.value = value; inp.min = min; inp.max = max; inp.step = step;
    inp.addEventListener('input', function () { onChange(parseFloat(this.value) || 0); });
    row.appendChild(lbl); row.appendChild(inp);
    container.appendChild(row);
    return inp;
  }
  function checkRow(container, label, checked, onChange) {
    var row = document.createElement('div'); row.className = 'pr';
    var lbl = document.createElement('label');
    lbl.style.cssText = 'font-size:9px;color:var(--text-dim);display:flex;align-items:center;gap:5px;cursor:pointer';
    var inp = document.createElement('input');
    inp.type = 'checkbox'; inp.style.accentColor = 'var(--accent)'; inp.checked = checked;
    inp.addEventListener('change', function () { onChange(this.checked); });
    lbl.appendChild(inp); lbl.appendChild(document.createTextNode(' ' + label));
    row.appendChild(lbl);
    container.appendChild(row);
    return inp;
  }
  function selectRow(container, label, value, options, onChange, title) {
    var row = document.createElement('div'); row.className = 'pr'; if (title) row.title = title;
    var lbl = document.createElement('span'); lbl.className = 'pl'; lbl.textContent = label;
    var sel = document.createElement('select'); sel.className = 'psel';
    options.forEach(function (o) {
      var opt = document.createElement('option'); opt.value = o.value; opt.textContent = o.label;
      if (String(o.value) === String(value)) opt.selected = true;
      sel.appendChild(opt);
    });
    sel.addEventListener('change', function () { onChange(this.value); });
    row.appendChild(lbl); row.appendChild(sel);
    container.appendChild(row);
    return sel;
  }
  // Drives an EXISTING right-panel input by id and fires its real event —
  // rather than duplicating each input's own state-setting logic here
  // (risking drift if that logic ever changes), this sets the value/checked
  // state then dispatches the same event type that input already listens
  // for, so its own pre-existing handler does the actual work exactly once,
  // in exactly one place. No-op (and the mirrored control still updates its
  // own displayed value) if that id isn't present in this build.
  function driveOriginal(id, value, isCheckbox, eventType) {
    var el = document.getElementById(id);
    if (!el) return;
    if (isCheckbox) el.checked = value; else el.value = value;
    el.dispatchEvent(new Event(eventType, { bubbles: true }));
  }

  // ---- Params header (2026-07 relayout) ----
  // Feedback: "garde les options en header pas tout en bas des brush" — the
  // Taille/Lissage row used to be appended AFTER every preset grid, inside
  // the SAME scrolling column, so a long preset list scrolled it out of
  // view entirely. Split in two: buildVectorParams/buildBitmapParams build
  // a STICKY header block (never scrolls, appended once, above the grid),
  // while buildVectorGrid/buildBitmapGrid build ONLY the scrollable preset
  // grid — selecting a preset now only rebuilds the grid container (to
  // move the .active highlight), not the whole popover, so the header
  // never flickers/rebuilds on a plain preset click either.
  // Also answers "y a t'il pas plus d'option ?" — Stabilizer, Pressure
  // brush, and Taper ends are the most relevant "brush feel" settings that
  // existed elsewhere in the right panel but were never mirrored here.
  // Ordre/tooltips alignés sur index.html (Tool Options) — feedback "le
  // smooth de la brush à 0 smooth toujours" : Stabilisateur (lisse la
  // POSITION en direct, agit AVANT que le trait soit posé) et Lissage
  // (simplifie la courbe APRÈS coup) sont deux mécanismes indépendants qui
  // se ressemblent trop par leur nom seul — mesuré : un zigzag à Lissage=0
  // perd encore ~1/3 de son amplitude si Stabilisateur=Medium. D'où
  // Stabilisateur en premier (ordre du pipeline réel) + tooltips explicites
  // ici aussi, ce panneau dupliquant les mêmes contrôles.
  var STAB_TITLE = 'Lisse la POSITION en direct pendant que tu dessines (corrige le tremblement de la main) — agit AVANT que le trait soit posé, indépendamment de Lissage ci-dessous. Mets ça sur Off si tu veux un tracé qui suit ta main au pixel près.';
  var SMOOTH_TITLE = 'Simplifie la courbe APRÈS coup (moins de points) — n\'a aucun effet sur le tremblement pendant le dessin, c\'est le Stabilisateur ci-dessus qui s\'en charge. À 0, le trait peut rester visiblement adouci si le Stabilisateur est activé.';
  function buildVectorParams(params) {
    numRow(params, 'Taille', state.brushSize || 3, 1, 300, 1, function (v) { driveOriginal('p-sw', v, false, 'change'); });
    selectRow(params, 'Stabilisateur', state.stabilizer !== undefined ? state.stabilizer : 2, [
      { value: 0, label: 'Off' }, { value: 1, label: 'Low' }, { value: 2, label: 'Medium' }, { value: 3, label: 'High' },
      { value: 4, label: 'Plume — légère' }, { value: 5, label: 'Plume — moyenne' }, { value: 6, label: 'Plume — forte' },
    ], function (v) { driveOriginal('p-stab', v, false, 'change'); }, STAB_TITLE);
    numRow(params, 'Lissage', state.smoothing || 0, 0, 60, 1, function (v) { driveOriginal('p-smooth', v, false, 'input'); }, SMOOTH_TITLE);
    checkRow(params, 'Pressure brush', !!state.vectorBrush, function (v) { driveOriginal('p-vecbrush', v, true, 'change'); });
    checkRow(params, 'Taper ends', !!state.taperEnds, function (v) { driveOriginal('p-taper', v, true, 'change'); });
  }
  function buildBitmapParams(params) {
    // The Stroke panel's #p-bitmapbrush-on checkbox is gone (2026-07
    // harmonization — this floating panel is the sole owner of
    // state.bitmapBrushOn now), so this calls window.SM.setBitmapBrushOn
    // directly instead of driving a hidden original element that no longer
    // exists.
    checkRow(params, 'Bitmap Brush actif', !!state.bitmapBrushOn, function (v) { if (window.SM && window.SM.setBitmapBrushOn) window.SM.setBitmapBrushOn(v); });
    numRow(params, 'Taille', state.brushSize || 3, 1, 300, 1, function (v) { driveOriginal('p-sw', v, false, 'change'); });
    // Spacing/Scatter/Opacity/Pressure no longer have a Stroke-panel element
    // to drive at all (removed, 2026-07 harmonization) — call
    // bitmap-brush.js's own state+live-selection setters directly instead of
    // routing through driveOriginal (which would now silently no-op).
    numRow(params, 'Espacement', state.bitmapSpacing !== undefined ? state.bitmapSpacing : 15, 2, 100, 1, function (v) { if (window.SM && window.SM.setBitmapSpacing) window.SM.setBitmapSpacing(v); });
    numRow(params, 'Dispersion', state.bitmapScatter !== undefined ? state.bitmapScatter : 20, 0, 100, 1, function (v) { if (window.SM && window.SM.setBitmapScatter) window.SM.setBitmapScatter(v); });
    numRow(params, 'Opacité', state.bitmapOpacity !== undefined ? state.bitmapOpacity : 100, 5, 100, 1, function (v) { if (window.SM && window.SM.setBitmapOpacity) window.SM.setBitmapOpacity(v); });
    checkRow(params, 'Pression (tablette)', state.bitmapPressure !== false, function (v) { if (window.SM && window.SM.setBitmapPressure) window.SM.setBitmapPressure(v); });
    selectRow(params, 'Stabilisateur', state.stabilizer !== undefined ? state.stabilizer : 2, [
      { value: 0, label: 'Off' }, { value: 1, label: 'Low' }, { value: 2, label: 'Medium' }, { value: 3, label: 'High' },
      { value: 4, label: 'Plume — légère' }, { value: 5, label: 'Plume — moyenne' }, { value: 6, label: 'Plume — forte' },
    ], function (v) { driveOriginal('p-stab', v, false, 'change'); }, STAB_TITLE);
    // Import .abr — was the Stroke panel's own button + hidden file input
    // (2026-07 harmonization, removed along with the rest of that section);
    // the actual parsing logic lives in bitmap-brush.js's importAbrFile
    // (exposed via window.SM), this just needs its own button+input pair
    // since there's nowhere else left to click it from.
    var abrRow = document.createElement('div'); abrRow.className = 'pr';
    var abrLbl = document.createElement('span'); abrLbl.className = 'pl';
    var abrBtn = document.createElement('button'); abrBtn.className = 'pbtn'; abrBtn.style.cssText = 'flex:1;font-size:10px'; abrBtn.type = 'button'; abrBtn.textContent = 'Import .abr tip…';
    var abrFile = document.createElement('input'); abrFile.type = 'file'; abrFile.accept = '.abr'; abrFile.style.display = 'none';
    abrBtn.addEventListener('click', function () { abrFile.click(); });
    abrFile.addEventListener('change', function () {
      var file = this.files && this.files[0];
      this.value = ''; // allow re-importing the same file name later
      if (window.SM && window.SM.importAbrFile) window.SM.importAbrFile(file);
    });
    abrRow.appendChild(abrLbl); abrRow.appendChild(abrBtn); abrRow.appendChild(abrFile);
    params.appendChild(abrRow);
  }
  function buildVectorGrid(container) {
    container.innerHTML = ''; // rebuilt on every selection (to move the .active highlight) — must replace, not append
    if (!window.BrushPresetPicker || !window.BrushPresetPicker.groups) {
      container.innerHTML = '<div class="bp-group-label">Indisponible</div>';
      return;
    }
    var current = state.brushPreset || 'none';
    window.BrushPresetPicker.groups().forEach(function (g) {
      var label = document.createElement('div'); label.className = 'bp-group-label'; label.textContent = g.label;
      container.appendChild(label);
      var grid = document.createElement('div'); grid.className = 'bp-grid';
      g.keys.forEach(function (k) { grid.appendChild(makeBrushItem(k, k === current, function (key) {
        commitBrushPreview();
        // NOT window.SM.applyVectorBrushToSelection(key) too — selectPreset
        // (brush-preset-picker.js) already retroactively re-textures the
        // current selection itself (its own pushUndo included); calling
        // both double-applies (two undo entries for one click, redundant
        // re-stamp with a fresh random seed each time — caught via a live
        // undoStack-length check, not just visually).
        window.BrushPresetPicker.selectPreset(key);
        buildVectorGrid(container); // rebuild the GRID only (to move the .active highlight) — the params header above is untouched
      })); });
      container.appendChild(grid);
    });
    var custom = window.BrushPresetPicker.customKeys();
    if (custom.length) {
      var clabel = document.createElement('div'); clabel.className = 'bp-group-label'; clabel.textContent = 'Mes brushes';
      container.appendChild(clabel);
      var cgrid = document.createElement('div'); cgrid.className = 'bp-grid';
      custom.forEach(function (k) { cgrid.appendChild(makeBrushItem(k, k === current, function (key) {
        commitBrushPreview();
        // NOT window.SM.applyVectorBrushToSelection(key) too — selectPreset
        // (brush-preset-picker.js) already retroactively re-textures the
        // current selection itself (its own pushUndo included); calling
        // both double-applies (two undo entries for one click, redundant
        // re-stamp with a fresh random seed each time — caught via a live
        // undoStack-length check, not just visually).
        window.BrushPresetPicker.selectPreset(key);
        buildVectorGrid(container);
      })); });
      container.appendChild(cgrid);
    }
  }
  function makeBrushItem(key, active, onSelect) {
    var btn = document.createElement('button');
    btn.className = 'bp-item' + (active ? ' active' : '');
    var canvas = document.createElement('canvas'); canvas.width = 150; canvas.height = 26;
    var span = document.createElement('span'); span.textContent = window.BrushPresetPicker.labelFor(key);
    btn.appendChild(canvas); btn.appendChild(span);
    window.BrushPresetPicker.drawPreview(canvas, key);
    btn.addEventListener('click', function () { onSelect(key); });
    btn.addEventListener('mouseenter', function () { previewVectorBrush(key); });
    btn.addEventListener('mouseleave', function () { revertBrushPreview(); });
    return btn;
  }

  function buildBitmapGrid(container) {
    container.innerHTML = ''; // rebuilt on every selection (to move the .active highlight) — must replace, not append
    if (!window.SMBitmapBrush || !window.BitmapTipPicker) {
      container.innerHTML = '<div class="bp-group-label">Bitmap Brush indisponible</div>';
      return;
    }
    var current = state.bitmapTip || 'soft';
    window.SMBitmapBrush.tipGroups().forEach(function (g) {
      var label = document.createElement('div'); label.className = 'bp-group-label'; label.textContent = g.label;
      container.appendChild(label);
      var grid = document.createElement('div'); grid.className = 'bp-grid';
      g.keys.forEach(function (k) { grid.appendChild(makeTipItem(k, k === current, function (key) {
        commitBrushPreview();
        window.BitmapTipPicker.selectTip(key);
        if (window.SM && window.SM.applyBitmapBrushToSelection) window.SM.applyBitmapBrushToSelection();
        buildBitmapGrid(container);
      })); });
      container.appendChild(grid);
    });
    var custom = window.SMBitmapBrush.customTipKeys();
    if (custom.length) {
      var clabel = document.createElement('div'); clabel.className = 'bp-group-label'; clabel.textContent = 'Mes pointes';
      container.appendChild(clabel);
      var cgrid = document.createElement('div'); cgrid.className = 'bp-grid';
      custom.forEach(function (k) { cgrid.appendChild(makeTipItem(k, k === current, function (key) {
        commitBrushPreview();
        window.BitmapTipPicker.selectTip(key);
        if (window.SM && window.SM.applyBitmapBrushToSelection) window.SM.applyBitmapBrushToSelection();
        buildBitmapGrid(container);
      })); });
      container.appendChild(cgrid);
    }
  }
  function makeTipItem(key, active, onSelect) {
    var btn = document.createElement('button');
    btn.className = 'bp-item' + (active ? ' active' : '');
    var canvas = document.createElement('canvas'); canvas.width = 150; canvas.height = 26;
    var span = document.createElement('span'); span.textContent = window.BitmapTipPicker.labelFor(key);
    btn.appendChild(canvas); btn.appendChild(span);
    window.BitmapTipPicker.drawPreview(canvas, key);
    btn.addEventListener('click', function () { onSelect(key); });
    btn.addEventListener('mouseenter', function () { previewBitmapBrush(key); });
    btn.addEventListener('mouseleave', function () { revertBrushPreview(); });
    return btn;
  }

  function buildContent(el) {
    el.innerHTML = '';
    var tabs = document.createElement('div'); tabs.className = 'brush-menu-tabs';
    var vecTab = document.createElement('button'); vecTab.className = 'brush-menu-tab' + (currentTab === 'vector' ? ' active' : ''); vecTab.textContent = 'Vecteur';
    var bmpTab = document.createElement('button'); bmpTab.className = 'brush-menu-tab' + (currentTab === 'bitmap' ? ' active' : ''); bmpTab.textContent = 'Bitmap';
    vecTab.addEventListener('click', function () { currentTab = 'vector'; buildContent(el); });
    bmpTab.addEventListener('click', function () { currentTab = 'bitmap'; buildContent(el); });
    tabs.appendChild(vecTab); tabs.appendChild(bmpTab);
    el.appendChild(tabs);
    var params = document.createElement('div'); params.className = 'brush-menu-params';
    el.appendChild(params);
    var grid = document.createElement('div'); grid.className = 'brush-menu-scroll';
    el.appendChild(grid);
    if (currentTab === 'vector') { buildVectorParams(params); buildVectorGrid(grid); }
    else { buildBitmapParams(params); buildBitmapGrid(grid); }
  }

  function toggle(anchorEl) {
    if (popover) { closePopover(); return; }
    var el = document.createElement('div');
    el.className = 'ctx-menu brush-menu-pop';
    document.body.appendChild(el);
    popover = el;
    buildContent(el);

    var ar = anchorEl.getBoundingClientRect();
    el.style.visibility = 'hidden'; el.style.display = 'block';
    var ew = el.offsetWidth, eh = el.offsetHeight;
    var left = Math.min(ar.left, window.innerWidth - ew - 8);
    var top = Math.min(ar.bottom + 6, window.innerHeight - eh - 8);
    el.style.left = Math.max(4, left) + 'px'; el.style.top = Math.max(4, top) + 'px';
    el.style.visibility = '';

    function onOutside(e) { if (!el.contains(e.target) && e.target !== anchorEl) closePopover(); }
    function onKey(e) { if (e.key === 'Escape') closePopover(); }
    document.addEventListener('mousedown', onOutside, true);
    document.addEventListener('keydown', onKey);
    closeHandlers = function () {
      document.removeEventListener('mousedown', onOutside, true);
      document.removeEventListener('keydown', onKey);
    };
    if (window.renderLabsFloatPanel) renderLabsFloatPanel();
  }

  window.BrushMenu = { toggle: toggle, isOpen: isOpen, close: closePopover };
})();
