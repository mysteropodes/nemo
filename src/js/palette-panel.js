// Color swatch/palette panel — Shade-for-After-Effects-style (David
// Boscolo's palette manager) color library: several NAMED palettes you
// switch between via tabs, drag-reorder colors within a palette, apply a
// swatch to Fill/Stroke, swap Fill↔Stroke, and a "find & replace" mode that
// walks the active layer's current frame swapping one color for another —
// the same "search your selection, change every instance of a color" tool
// Shade offers, scoped here to the active layer (StrokeMotion doesn't have
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
    if (pal.colors.indexOf(hex) >= 0) { showToast('Déjà dans la palette'); return; }
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
  var replaceArmed = false, replaceSource = null;
  function startReplace() {
    replaceArmed = true; replaceSource = null;
    showToast('Remplacer : clique la couleur SOURCE dans le calque, puis la couleur CIBLE');
    render();
  }
  function cancelReplace() { replaceArmed = false; replaceSource = null; render(); }
  function colorsEqual(hex1, hex2) {
    if (!hex1 || !hex2) return hex1 === hex2;
    return hex1.toLowerCase() === hex2.toLowerCase();
  }
  function replaceInActiveLayer(fromHex, toHex) {
    var layer = userLayers[state.activeLayerIdx];
    if (!layer) return 0;
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
      btn.className = 'palette-swatch' + (replaceArmed && replaceSource === hex ? ' armed' : '');
      btn.style.setProperty('--sw-color', hex);
      btn.title = hex;
      wireDrag(btn, idx);
      btn.addEventListener('click', function (e) {
        if (replaceArmed) {
          if (replaceSource === null) { replaceSource = hex; render(); return; }
          var n = replaceInActiveLayer(replaceSource, hex);
          showToast(n ? (n + ' couleur(s) remplacée(s)') : 'Aucune correspondance dans le calque actif');
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
          { label: 'Remplacer dans le calque…', action: function () { replaceArmed = true; replaceSource = hex; render(); showToast('Clique la couleur CIBLE'); } },
          { sep: true },
          { label: 'Retirer de la palette', action: function () { removeColor(idx); } },
        ]);
      });
      grid.appendChild(btn);
    });
    var replaceBtn = document.getElementById('btn-palette-replace');
    if (replaceBtn) {
      replaceBtn.classList.toggle('active', replaceArmed);
      replaceBtn.textContent = replaceArmed ? (replaceSource ? '🔁 Clique la cible…' : '🔁 Clique la source…') : '🔁 Remplacer dans le calque';
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
    if (palettes().length <= 1) { showToast('Il faut garder au moins une palette'); return; }
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
    document.addEventListener('keydown', function (e) { if (e.key === 'Escape' && replaceArmed) cancelReplace(); });
    render();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
