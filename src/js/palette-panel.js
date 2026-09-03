// Color swatch/palette panel — Shade-for-After-Effects-style (David
// Boscolo's palette manager) color library: several NAMED palettes you
// switch between via tabs, drag-reorder colors within a palette, apply a
// swatch to Fill/Stroke, swap Fill↔Stroke, and a "find & replace" mode that
// walks the active layer's current frame swapping one color for another —
// the same "search your selection, change every instance of a color" tool
// Shade offers, scoped here to the active layer (Nemo doesn't have
// AE's multi-layer timeline-wide selection concept).
(function () {
  function palettes() { return state.palettes || (state.palettes = [{ id: 'p0', name: 'Palette 1', colors: [] }]); }
  function activePalette() {
    var list = palettes();
    if (state.activePaletteIdx == null || state.activePaletteIdx >= list.length) state.activePaletteIdx = 0;
    return list[state.activePaletteIdx];
  }
  function currentHex(kind) {
    var input = document.getElementById(kind === 'stroke' ? 'color-stroke' : 'color-fill');
    return (input && (input.dataset.hex8 || input.value)) || (kind === 'stroke' ? '#000000' : '#ff0000');
  }
  function genId() { return 'p' + Date.now().toString(36) + Math.floor(Math.random() * 1e4); }

  function addColor(hex) {
    var pal = activePalette();
    if (pal.colors.indexOf(hex) >= 0) { showToast(SM.t('toastAlreadyInPalette')); return; }
    pal.colors.push(hex);
    render();
  }
  function removeColor(idx) {
    activePalette().colors.splice(idx, 1);
    render();
  }

  // ---- find & replace across the active layer's current frame ----
  // Armed by clicking "Remplacer", then clicking a SOURCE swatch, then a
  // TARGET swatch — a 3-click flow (mirrors Shade's find/replace, which is
  // also a pick-then-pick tool) rather than drag-to-replace, which would
  // collide with the reorder drag already on these same swatches.
  // Highlights whichever swatch(es) in the active palette match the color
  // currently open in the Fill/Stroke color-picker popover (feedback #20:
  // "afficher le ou les nuanciers sélectionnés" when the picker opens) —
  // color-picker.js calls setPaletteHighlight(hex) on open and on every
  // live change while dragging inside the picker, and clears it (null) on
  // close so a stale ring doesn't linger after the popover's gone.
  var highlightHex = null;
  function setPaletteHighlight(hex) { highlightHex = hex; render(); }
  window.setPaletteHighlight = setPaletteHighlight;

  var replaceArmed = false, replaceSource = null;
  function startReplace() {
    replaceArmed = true; replaceSource = null;
    showToast(SM.t('hsPaletteReplaceHint'));
    render();
  }
  function cancelReplace() { replaceArmed = false; replaceSource = null; render(); }
  function colorsEqual(hex1, hex2) {
    if (!hex1 || !hex2) return hex1 === hex2;
    return hex1.toLowerCase() === hex2.toLowerCase();
  }
  // Portée du remplacement (2026-09, demande de Cyril : « qu'il le fasse sur
  // un calque et toutes les keyframes des calques sélectionnés en option »).
  // Trois portées, mémorisées entre deux sessions :
  //   frame     — le calque actif, à l'image courante (comportement d'origine)
  //   layer     — le calque actif, sur TOUTES ses images
  //   selection — tous les calques sélectionnés, sur toutes leurs images
  // Les deux dernières travaillent sur les dictionnaires de traits stockés,
  // pas sur les objets Paper vivants : c'est la seule vue qui contient les
  // images non chargées. loadFrame rebâtit ensuite l'image courante.
  var REPLACE_SCOPE_KEY = 'nemo-palette-replace-scope';
  function replaceScope() {
    try { return localStorage.getItem(REPLACE_SCOPE_KEY) || 'frame'; } catch (e) { return 'frame'; }
  }
  function setReplaceScope(v) {
    try { localStorage.setItem(REPLACE_SCOPE_KEY, v); } catch (e) {}
  }
  function scopeLayers() {
    if (replaceScope() !== 'selection') return [state.activeLayerIdx];
    var sel = (typeof _layerSel !== 'undefined' && _layerSel && _layerSel.length) ? _layerSel.slice() : [state.activeLayerIdx];
    return sel.filter(function (i) { return state.layers[i]; });
  }
  // Un trait stocké porte sa couleur à quatre endroits possibles : le
  // remplissage, le contour, le contour d'origine d'une texture de brosse
  // (preTextureStroke, mis à null sur l'objet vivant — voir serP) et les
  // arrêts d'un dégradé.
  function replaceInStrokeDict(sd, fromHex, toHex) {
    var n = 0;
    if (sd.fillColor && colorsEqual(sd.fillColor, fromHex)) { sd.fillColor = toHex; n++; }
    if (sd.strokeColor && colorsEqual(sd.strokeColor, fromHex)) { sd.strokeColor = toHex; n++; }
    if (sd.preTextureStroke && colorsEqual(sd.preTextureStroke, fromHex)) { sd.preTextureStroke = toHex; n++; }
    if (sd.fillGradient && sd.fillGradient.stops) {
      sd.fillGradient.stops.forEach(function (st) {
        if (st && st.color && colorsEqual(st.color, fromHex)) { st.color = toHex; n++; }
      });
    }
    return n;
  }
  function replaceAcrossFrames(fromHex, toHex) {
    var layers = scopeLayers(), n = 0;
    if (typeof saveAllLayerFrames === 'function') saveAllLayerFrames();
    if (typeof pushUndoLayers === 'function') pushUndoLayers(true);
    layers.forEach(function (li) {
      var ld = state.layers[li]; if (!ld) return;
      (ld.frames || []).forEach(function (f) {
        (f && f.strokes || []).forEach(function (sd) { n += replaceInStrokeDict(sd, fromHex, toHex); });
      });
    });
    if (n) {
      loadFrame(state.currentFrame);
      updateUI();
      if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    }
    return { count: n, layers: layers.length };
  }
  function replaceColor(fromHex, toHex) {
    if (replaceScope() === 'frame') return { count: replaceInActiveLayer(fromHex, toHex), layers: 1 };
    return replaceAcrossFrames(fromHex, toHex);
  }
  function replaceInActiveLayer(fromHex, toHex) {
    var layer = userLayers[state.activeLayerIdx];
    if (!layer) return 0;
    if (typeof pushUndo === 'function') pushUndo();
    var n = 0;
    layer.children.forEach(function (c) {
      if (!(c instanceof Path || c instanceof CompoundPath)) return;
      if (c.fillColor && colorsEqual(colorHex8(c.fillColor), fromHex)) { c.fillColor = toHex; n++; }
      if (c.strokeColor && colorsEqual(colorHex8(c.strokeColor), fromHex)) { c.strokeColor = toHex; n++; }
      // A textured anchor's real stroke lives in preTextureStroke while
      // strokeColor itself is null (see tools.js applyBrushTexture) — the
      // dab companions carry the actual visible color as their fillColor,
      // which the generic fillColor check above already catches for each
      // companion (they're separate Path children of the same layer).
      if (c.data && c.data.preTextureStroke && colorsEqual(c.data.preTextureStroke, fromHex)) { c.data.preTextureStroke = toHex; n++; }
    });
    if (n) { saveActiveLayerFrame(); updateUI(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); }
    return n;
  }

  // ---- drag-to-reorder within the active palette ----
  var dragIdx = null;
  function wireDrag(btn, idx) {
    btn.draggable = true;
    btn.addEventListener('dragstart', function (e) { dragIdx = idx; e.dataTransfer.effectAllowed = 'move'; });
    btn.addEventListener('dragover', function (e) { e.preventDefault(); e.dataTransfer.dropEffect = 'move'; });
    btn.addEventListener('drop', function (e) {
      e.preventDefault();
      if (dragIdx === null || dragIdx === idx) return;
      var colors = activePalette().colors;
      var moved = colors.splice(dragIdx, 1)[0];
      colors.splice(idx, 0, moved);
      dragIdx = null;
      render();
    });
    btn.addEventListener('dragend', function () { dragIdx = null; });
  }

  function render() {
    renderTabs();
    var grid = document.getElementById('palette-grid');
    if (!grid) return;
    grid.innerHTML = '';
    var pal = activePalette();
    pal.colors.forEach(function (hex, idx) {
      var btn = document.createElement('button');
      btn.className = 'palette-swatch' + (replaceArmed && replaceSource === hex ? ' armed' : '') + (highlightHex && colorsEqual(hex, highlightHex) ? ' sel-match' : '');
      btn.style.setProperty('--sw-color', hex);
      btn.title = hex;
      wireDrag(btn, idx);
      btn.addEventListener('click', function (e) {
        if (replaceArmed) {
          if (replaceSource === null) { replaceSource = hex; render(); return; }
          var res = replaceColor(replaceSource, hex);
          showToast(res.count
            ? (res.count + SM.t('toastColorsReplacedSuffix') + (replaceScope() === 'selection' ? ' (' + res.layers + ' ' + SM.t('paletteScopeLayersWord') + ')' : ''))
            : SM.t('toastColorNoMatchInScope'));
          cancelReplace();
          return;
        }
        if (e.shiftKey) window.SM.setStrokeColor(hex);
        else { window.SM.setFillColor(hex); if (!state.fillEnabled) window.SM.setFillEnabled(true); }
      });
      btn.addEventListener('contextmenu', function (e) {
        e.preventDefault();
        window.showContextMenu(e.clientX, e.clientY, [
          { label: 'Utiliser comme Fill', action: function () { window.SM.setFillColor(hex); if (!state.fillEnabled) window.SM.setFillEnabled(true); } },
          { label: 'Utiliser comme Stroke', action: function () { window.SM.setStrokeColor(hex); } },
          { sep: true },
          { label: 'Remplacer dans le calque…', action: function () { replaceArmed = true; replaceSource = hex; render(); showToast(SM.t('hsClickTargetColor')); } },
          { sep: true },
          { label: 'Retirer de la palette', action: function () { removeColor(idx); } },
        ]);
      });
      grid.appendChild(btn);
    });
    var scopeSel = document.getElementById('palette-replace-scope');
    if (scopeSel && scopeSel.value !== replaceScope()) scopeSel.value = replaceScope();
    var replaceBtn = document.getElementById('btn-palette-replace');
    if (replaceBtn) {
      replaceBtn.classList.toggle('active', replaceArmed);
      // Title only — writing textContent here used to DESTROY the button's
      // SVG icon on the very first render (it's an .icon-only-btn, the icon
      // is its whole face), leaving a hardcoded-French emoji label instead.
      // The armed-state prompts already reach the user via showToast.
      if (!replaceBtn.dataset.baseTitle) replaceBtn.dataset.baseTitle = replaceBtn.title;
      replaceBtn.title = replaceArmed
        ? (replaceSource ? 'Remplacer : clique la couleur CIBLE' : 'Remplacer : clique la couleur SOURCE')
        : replaceBtn.dataset.baseTitle;
    }
  }

  function renderTabs() {
    var wrap = document.getElementById('palette-tabs');
    if (!wrap) return;
    wrap.innerHTML = '';
    palettes().forEach(function (pal, idx) {
      var tab = document.createElement('div');
      tab.className = 'sym-tab' + (idx === state.activePaletteIdx ? ' act' : '');
      tab.textContent = pal.name;
      tab.title = pal.name;
      tab.addEventListener('click', function () { state.activePaletteIdx = idx; cancelReplace(); });
      tab.addEventListener('dblclick', function () { renamePalette(idx); });
      tab.addEventListener('contextmenu', function (e) {
        e.preventDefault();
        window.showContextMenu(e.clientX, e.clientY, [
          { label: 'Renommer…', action: function () { renamePalette(idx); } },
          { label: 'Dupliquer', action: function () { duplicatePalette(idx); } },
          { sep: true },
          { label: 'Supprimer', action: function () { deletePalette(idx); }, disabled: palettes().length <= 1 },
        ]);
      });
      wrap.appendChild(tab);
    });
  }

  function newPalette() {
    var name = prompt('Nom de la nouvelle palette :', 'Palette ' + (palettes().length + 1));
    if (name === null) return;
    palettes().push({ id: genId(), name: name.trim() || 'Palette', colors: [] });
    state.activePaletteIdx = palettes().length - 1;
    render();
  }
  function renamePalette(idx) {
    var pal = palettes()[idx];
    var name = prompt('Renommer la palette :', pal.name);
    if (name === null || !name.trim()) return;
    pal.name = name.trim();
    render();
  }
  function duplicatePalette(idx) {
    var src = palettes()[idx];
    palettes().splice(idx + 1, 0, { id: genId(), name: src.name + ' copie', colors: src.colors.slice() });
    state.activePaletteIdx = idx + 1;
    render();
  }
  function deletePalette(idx) {
    if (palettes().length <= 1) { showToast(SM.t('hsKeepOnePalette')); return; }
    palettes().splice(idx, 1);
    if (state.activePaletteIdx >= palettes().length) state.activePaletteIdx = palettes().length - 1;
    render();
  }

  window.renderPaletteGrid = render;

  function init() {
    var addFillBtn = document.getElementById('btn-palette-add-fill');
    var addStrokeBtn = document.getElementById('btn-palette-add-stroke');
    if (addFillBtn) addFillBtn.addEventListener('click', function () { addColor(currentHex('fill')); });
    if (addStrokeBtn) addStrokeBtn.addEventListener('click', function () { addColor(currentHex('stroke')); });
    var newBtn = document.getElementById('btn-palette-new');
    if (newBtn) newBtn.addEventListener('click', newPalette);
    var swapBtn = document.getElementById('btn-palette-swap');
    if (swapBtn) swapBtn.addEventListener('click', function () {
      var f = currentHex('fill'), s = currentHex('stroke');
      window.SM.setFillColor(s); window.SM.setStrokeColor(f);
    });
    var replaceBtn = document.getElementById('btn-palette-replace');
    if (replaceBtn) replaceBtn.addEventListener('click', function () { if (replaceArmed) cancelReplace(); else startReplace(); });
    var scopeSel = document.getElementById('palette-replace-scope');
    if (scopeSel) {
      scopeSel.value = replaceScope();
      scopeSel.addEventListener('change', function () { setReplaceScope(scopeSel.value); render(); });
    }
    document.addEventListener('keydown', function (e) { if (e.key === 'Escape' && replaceArmed) cancelReplace(); });
    render();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
