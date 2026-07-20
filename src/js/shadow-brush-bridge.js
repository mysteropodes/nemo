// ---- SHADOW BRUSH (2026-07) ----
// Feedback: a dedicated guide-line brush, reachable from the floating panel
// when the Brush tool is active, for the existing Stroke/Fill/Shadow layer-
// separation workflow (state.shadowMode / data.channelTag='shadow' — see
// tools.js's Pen/Draw/Shape commit paths and app.js's
// convertLayerToStrokeFillShadowFolder). It behaves exactly like a normal
// brush stroke, EXCEPT:
//   - it always draws in one of a small set of dedicated "shadow colors"
//     (a mini palette, 6 defaults + user-addable) instead of the normal
//     Stroke/Fill color — so shadow-boundary lines read as visually
//     distinct from real linework while drawing;
//   - each swatch carries a stable id (state.shadowPalette[].id), stamped
//     onto every stroke drawn with it (data.shadowSwatchId) — lets a future
//     step group/recolor "all the guide lines meant for this shadow bucket"
//     even if the swatch's own color is edited later;
//   - the goal is for these lines to NEVER appear in the final result: only
//     the fill area they end up delimiting (via the paint bucket, drawn
//     separately) should remain. See timeline.js's toggleShadowGuides
//     (view-only, all layers/components at once) and export.js's
//     exportIncludeShadowGuides checkbox for the two places that hide them.
//
// Popover UI/positioning mirrors brush-menu-bridge.js's toggle()/
// closePopover() almost verbatim (same .ctx-menu popover shell, same
// outside-click/Escape close), just with a palette-swatch grid instead of a
// brush-preset gallery — kept as a SEPARATE small popover rather than a
// third tab on that one, since picking a shadow color is a one-click arm/
// disarm action, not a "keep browsing a big catalog" flow.
(function () {
  var popover = null, closeHandlers = null;

  function palette() {
    if (!state.shadowPalette || !state.shadowPalette.length) {
      state.shadowPalette = [{ id: 'sh1', color: '#ff3355' }, { id: 'sh2', color: '#ff8800' }, { id: 'sh3', color: '#ffdd00' }, { id: 'sh4', color: '#22cc55' }, { id: 'sh5', color: '#2288ff' }, { id: 'sh6', color: '#aa33ff' }];
    }
    return state.shadowPalette;
  }
  function activeSwatch() {
    var p = palette();
    return p.find(function (s) { return s.id === state.shadowActiveId; }) || p[0];
  }
  function activeColor() { return activeSwatch().color; }
  function isArmed() { return !!state.shadowMode; }

  function genId() {
    var n = 1, ids = palette().map(function (s) { return s.id; });
    while (ids.indexOf('sh' + n) !== -1) n++;
    return 'sh' + n;
  }
  function addColor(hex) {
    var s = { id: genId(), color: hex };
    palette().push(s);
    return s;
  }

  function closePopover() {
    if (!popover) return;
    popover.remove();
    popover = null;
    if (closeHandlers) { closeHandlers(); closeHandlers = null; }
    if (window.renderLabsFloatPanel) renderLabsFloatPanel();
  }
  function isOpen() { return !!popover; }

  function arm(swatchId) {
    state.shadowActiveId = swatchId;
    state.shadowMode = true;
    var cb = document.getElementById('p-shadowmode'); if (cb) cb.checked = true;
  }
  function disarm() {
    state.shadowMode = false;
    var cb = document.getElementById('p-shadowmode'); if (cb) cb.checked = false;
  }

  function buildContent(el) {
    el.innerHTML = '';
    var title = document.createElement('div');
    title.className = 'bp-group-label';
    title.textContent = 'Shadow Brush';
    el.appendChild(title);
    var grid = document.createElement('div');
    grid.style.cssText = 'display:grid;grid-template-columns:repeat(4,1fr);gap:6px;padding:4px 2px;width:150px;';
    palette().forEach(function (s) {
      var btn = document.createElement('button');
      btn.className = 'palette-swatch' + (isArmed() && state.shadowActiveId === s.id ? ' sel-match' : '');
      btn.style.setProperty('--sw-color', s.color);
      btn.title = s.color;
      btn.addEventListener('click', function () { arm(s.id); closePopover(); if (window.SMEngineBridge) SMEngineBridge.renderNow(); });
      grid.appendChild(btn);
    });
    var addBtn = document.createElement('button');
    addBtn.className = 'palette-swatch';
    addBtn.style.cssText = 'display:flex;align-items:center;justify-content:center;font-size:16px;line-height:1;color:var(--text-dim);background:var(--panel3);';
    addBtn.textContent = '+';
    addBtn.title = 'Ajouter une couleur';
    addBtn.addEventListener('click', function () {
      var inp = document.createElement('input');
      inp.type = 'color'; inp.style.cssText = 'position:fixed;left:-9999px;';
      document.body.appendChild(inp);
      inp.addEventListener('change', function () {
        var s = addColor(inp.value);
        arm(s.id);
        buildContent(el);
        inp.remove();
        if (window.SMEngineBridge) SMEngineBridge.renderNow();
      });
      inp.addEventListener('blur', function () { setTimeout(function () { if (inp.parentNode) inp.remove(); }, 200); });
      inp.click();
    });
    grid.appendChild(addBtn);
    el.appendChild(grid);
    var off = document.createElement('button');
    off.className = 'bp-swatch-btn';
    off.style.cssText = 'width:100%;justify-content:center;margin-top:4px;';
    off.innerHTML = '<span>' + (isArmed() ? 'Désactiver' : 'Inactif') + '</span>';
    off.addEventListener('click', function () { disarm(); closePopover(); });
    el.appendChild(off);
  }

  function toggle(anchorEl) {
    // A single click on the floating button toggles the popover; but if
    // shadow mode is already armed, that same click is more useful as an
    // instant disarm (matches the "click once to arm+pick, click again to
    // turn off" flow described in the request) rather than reopening a
    // picker for a mode you were trying to leave.
    if (isArmed()) { disarm(); if (window.renderLabsFloatPanel) renderLabsFloatPanel(); return; }
    if (popover) { closePopover(); return; }
    var el = document.createElement('div');
    el.className = 'ctx-menu shadow-brush-pop';
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
  }

  window.SMShadowBrush = { toggle: toggle, isOpen: isOpen, close: closePopover, isArmed: isArmed, activeColor: activeColor, activeSwatch: activeSwatch, palette: palette };
})();
