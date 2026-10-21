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
  'use strict';
  var popover = null, closeHandlers = null, contentHandlers = [], picker = null, disposed = false;
  var defaults = Object.freeze([
    { id: 'sh1', color: '#ff3355' }, { id: 'sh2', color: '#ff8800' }, { id: 'sh3', color: '#ffdd00' },
    { id: 'sh4', color: '#22cc55' }, { id: 'sh5', color: '#2288ff' }, { id: 'sh6', color: '#aa33ff' }
  ].map(function (s, i) { return Object.freeze({ ordinal: i + 1, id: s.id, color: s.color }); }));
  function native() { return !!(window.__TAURI__ && window.__TAURI__.core); }
  function controller() { return window.NemoNativeShadowController; }
  function projection() { var c = controller(); return !disposed && c && c.read(); }
  function palette() {
    if (native()) { var p = projection(); return p ? (p.shadow.items.length ? p.shadow.items : defaults) : Object.freeze([]); }
    if (!state.shadowPalette || !state.shadowPalette.length) {
      state.shadowPalette = defaults.map(function (s) { return { id: s.id, color: s.color }; });
    }
    return state.shadowPalette;
  }
  function activeSwatch() {
    if (native()) {
      var origin = projection();
      if (!origin) { disarm(); return null; }
      var list = palette(), active = origin.shadow.settings.shadowActiveId;
      return list.find(function (s) { return s.id === active; }) || list[0];
    }
    var p = palette(); return p.find(function (s) { return s.id === state.shadowActiveId; }) || p[0];
  }
  function activeColor() { var s = activeSwatch(); return s ? s.color : null; }
  function isArmed() { if (native() && !projection()) disarm(); return !!state.shadowMode; }
  function genId(list) {
    var n = 1, ids = list.map(function (s) { return s.id; });
    while (ids.indexOf('sh' + n) !== -1) n++;
    return 'sh' + n;
  }
  function addColor(hex) {
    var p = palette(), s = { id: genId(p), color: hex }; p.push(s); return s;
  }
  function listen(node, kind, action) {
    node.addEventListener(kind, action);
    contentHandlers.push(function () { node.removeEventListener(kind, action); });
  }
  function clearPicker() {
    if (!picker) return;
    var old = picker; picker = null; old.dispose();
  }
  function clearContent() {
    clearPicker(); contentHandlers.splice(0).forEach(function (remove) { remove(); });
  }
  function closePopover() {
    clearContent();
    if (popover) { popover.remove(); popover = null; }
    if (closeHandlers) { closeHandlers(); closeHandlers = null; }
    if (window.renderLabsFloatPanel) renderLabsFloatPanel();
  }
  function isOpen() { return !!popover; }
  function arm(swatchId) {
    state.shadowActiveId = swatchId; state.shadowMode = true;
    var cb = document.getElementById('p-shadowmode'); if (cb) cb.checked = true;
  }
  function disarm() {
    state.shadowMode = false;
    var cb = document.getElementById('p-shadowmode'); if (cb) cb.checked = false;
  }
  function userDisarm() {
    var c = controller();
    if (native() && c && typeof c.off === 'function') c.off();
    else disarm();
  }
  function notice() { if (typeof window.showToast === 'function') window.showToast('Shadow editing is unavailable.'); }
  async function nativeEdit(action, origin, close) {
    var c = controller();
    if (disposed || !origin || !c || c.read() !== origin || c.isPending()) { disarm(); notice(); return; }
    var result = await c.edit(action, origin);
    if (!['applied-presented', 'unchanged'].includes(result.status) || c.read() !== result.projection) return;
    if (close) closePopover();
    else if (popover) buildContent(popover);
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  function openPicker(el, origin) {
    if (picker || native() && (!origin || !controller() || controller().read() !== origin || controller().isPending())) return;
    var inp = document.createElement('input'), timer = null, handled = false;
    inp.type = 'color'; inp.style.cssText = 'position:fixed;left:-9999px;';
    document.body.appendChild(inp);
    function cleanup() {
      if (timer !== null) clearTimeout(timer);
      inp.removeEventListener('change', change); inp.removeEventListener('blur', blur);
      inp.removeEventListener('cancel', cancel); inp.remove();
    }
    var own = { dispose: cleanup }; picker = own;
    function cancel() { if (picker === own) { picker = null; cleanup(); } }
    function blur() { timer = setTimeout(cancel, 200); }
    function change() {
      if (handled || picker !== own || disposed || popover !== el) return;
      handled = true; var hex = inp.value; cancel();
      if (native()) {
        if (!origin || !controller() || controller().read() !== origin) { notice(); return; }
        nativeEdit({ kind: 'add', id: genId(palette()), color: hex }, origin, false);
      } else {
        var s = addColor(hex); arm(s.id); buildContent(el);
        if (window.SMEngineBridge) SMEngineBridge.renderNow();
      }
    }
    inp.addEventListener('change', change); inp.addEventListener('blur', blur); inp.addEventListener('cancel', cancel); inp.click();
  }
  function buildContent(el) {
    clearContent(); el.innerHTML = '';
    var title = document.createElement('div'); title.className = 'bp-group-label'; title.textContent = 'Shadow Brush'; el.appendChild(title);
    var grid = document.createElement('div');
    grid.style.cssText = 'display:grid;grid-template-columns:repeat(4,1fr);gap:6px;padding:4px 2px;width:150px;';
    var origin = native() ? projection() : null, c = controller(), ready = !native() || !!origin && c && !c.isPending();
    var active = ready ? activeSwatch() : null;
    palette().forEach(function (s, index) {
      var btn = document.createElement('button'); btn.type = 'button'; btn.disabled = !ready;
      btn.className = 'palette-swatch' + (isArmed() && active && active.ordinal === s.ordinal && active.id === s.id ? ' sel-match' : '');
      if (!native()) btn.className = 'palette-swatch' + (isArmed() && state.shadowActiveId === s.id ? ' sel-match' : '');
      btn.style.setProperty('--sw-color', s.color); btn.title = s.color;
      listen(btn, 'click', function () {
        if (native()) { nativeEdit({ kind: 'select', ordinal: index + 1 }, origin, true); return; }
        arm(s.id); closePopover(); if (window.SMEngineBridge) SMEngineBridge.renderNow();
      }); grid.appendChild(btn);
    });
    var addBtn = document.createElement('button'); addBtn.type = 'button'; addBtn.disabled = !ready;
    addBtn.className = 'palette-swatch';
    addBtn.style.cssText = 'display:flex;align-items:center;justify-content:center;font-size:16px;line-height:1;color:var(--text-dim);background:var(--panel3);';
    addBtn.textContent = '+'; addBtn.title = SM.t('hsAddColor');
    listen(addBtn, 'click', function () { openPicker(el, origin); }); grid.appendChild(addBtn); el.appendChild(grid);
    if (!ready) {
      var status = document.createElement('div'); status.setAttribute('role', 'status');
      status.textContent = c && c.isPending() ? 'Synchronizing shadow colors…' : 'Shadow editing is unavailable.'; el.appendChild(status);
    }
    var off = document.createElement('button'); off.type = 'button'; off.className = 'bp-swatch-btn';
    off.style.cssText = 'width:100%;justify-content:center;margin-top:4px;';
    off.textContent = isArmed() ? 'Désactiver' : 'Inactif';
    listen(off, 'click', function () { userDisarm(); closePopover(); }); el.appendChild(off);
  }
  function toggle(anchorEl) {
    if (disposed) return;
    if (isArmed()) { userDisarm(); if (window.renderLabsFloatPanel) renderLabsFloatPanel(); return; }
    if (popover) { closePopover(); return; }
    var el = document.createElement('div'); el.className = 'ctx-menu shadow-brush-pop'; document.body.appendChild(el); popover = el; buildContent(el);
    var ar = anchorEl.getBoundingClientRect(); el.style.visibility = 'hidden'; el.style.display = 'block';
    var left = Math.min(ar.left, window.innerWidth - el.offsetWidth - 8), top = Math.min(ar.bottom + 6, window.innerHeight - el.offsetHeight - 8);
    el.style.left = Math.max(4, left) + 'px'; el.style.top = Math.max(4, top) + 'px'; el.style.visibility = '';
    function onOutside(e) { if (!el.contains(e.target) && e.target !== anchorEl) closePopover(); }
    function onKey(e) { if (e.key === 'Escape') closePopover(); }
    document.addEventListener('mousedown', onOutside, true); document.addEventListener('keydown', onKey);
    closeHandlers = function () { document.removeEventListener('mousedown', onOutside, true); document.removeEventListener('keydown', onKey); };
    if (native() && controller()) controller().presented();
  }
  function refresh() { if (popover) buildContent(popover); if (window.renderLabsFloatPanel) renderLabsFloatPanel(); }
  function invalidate() { if (native()) { disarm(); closePopover(); } }
  function dispose() {
    if (disposed) return;
    disposed = true; disarm(); closePopover(); window.removeEventListener('pagehide', dispose);
  }
  window.addEventListener('pagehide', dispose, { once: true });
  window.SMShadowBrush = { toggle: toggle, isOpen: isOpen, close: closePopover, isArmed: isArmed,
    activeColor: activeColor, activeSwatch: activeSwatch, palette: palette, refresh: refresh, invalidate: invalidate, dispose: dispose };
})();

// Shadow Brush (2026-07, shadow-brush-bridge.js) — every state.shadowMode
// commit site used to just stamp data.channelTag='shadow' and otherwise
// draw with whatever the normal Stroke/Fill color happened to be. Now that
// there's a dedicated small palette for this (state.shadowPalette, each
// swatch with a stable id), the same tag also needs the ACTIVE swatch's
// color (not the global strokeColor) and its id (data.shadowSwatchId, so a
// future step can group/recolor "all guide lines for this shadow bucket"
// even after the swatch's own color is edited). Pure guide lines have no
// fill — delimiting the eventual fill is the whole point (see the paint-
// bucket workflow the bridge's header comment describes), so fillColor is
// explicitly cleared here too.
function applyShadowBrushTag(p,preferFill){
  if(!p)return;
  var nativeShadow=!!(window.__TAURI__&&window.__TAURI__.core),sw=null;
  if(nativeShadow){
    sw=window.SMShadowBrush?window.SMShadowBrush.activeSwatch():null;
    if(!sw)return {status:'unavailable',code:'unavailable'};
  }
  p.data=p.data||{};
  p.data.channelTag='shadow';
  if(window.SMShadowBrush){
    if(!nativeShadow)sw=window.SMShadowBrush.activeSwatch();
    // Shadow Brush is construction ink for the fill solver, never a painted
    // ribbon. Convert pressure/Fill-Brush outlines back to their centreline
    // while that centreline still exists, then enforce a visible stroke and
    // no fill for every producer (Draw, Fill Brush, Pen and Shape).
    if(p.data.isVectorBrush&&p.data.centerSegments&&p.data.centerSegments.length){
      var shadowCenter=p.data.centerSegments.map(function(s){return{
        point:[s.point[0],s.point[1]],
        handleIn:s.handleIn?[s.handleIn[0],s.handleIn[1]]:[0,0],
        handleOut:s.handleOut?[s.handleOut[0],s.handleOut[1]]:[0,0],
      };});
      p.removeSegments();
      shadowCenter.forEach(function(s){
        p.add(new Segment(new Point(s.point[0],s.point[1]),new Point(s.handleIn[0],s.handleIn[1]),new Point(s.handleOut[0],s.handleOut[1])));
      });
      p.closed=false;
      delete p.data.isVectorBrush;delete p.data.centerSegments;delete p.data.widthProfile;
    }
    // preferFill (2026-07 fix): the "Stroke off / Fill on" vector-brush
    // commit (draw-bridge.js's shadowPreferFill) never had a stroke to
    // begin with — its ink lives entirely in fillColor. Forcing the
    // default stroke-only treatment on it silently overrode the user's
    // explicit Stroke-eye-OFF choice, turning an intended filled patch
    // into a near-invisible 1px hairline (strokeWidth falling back to
    // Paper's own Path default since this branch never sets one).
    // Confirmed live: drawing with vectorBrush+!strokeEnabled+fillEnabled
    // and Shadow armed produced fillColor:null, strokeColor:<swatch> —
    // exactly the parameter this function was documented (but never wired)
    // to prevent.
    if(preferFill){
      p.strokeColor=null;
      p.fillColor=sw.color;
    }else{
      p.fillColor=null;
      p.strokeColor=sw.color;
      p.strokeWidth=Math.max(1,p.strokeWidth||state.brushSize||3);
      p.strokeCap='round';p.strokeJoin='round';
    }
    p.data.shadowSwatchId=sw.id;
  }
}
