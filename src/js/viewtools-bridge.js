// ---- C7 CUTOVER: Hand/Zoom/Eyedropper tools on the Rust engine ----
// Same capture-phase interception architecture as every other bridge.
// These three are lightweight on purpose — Hand/Zoom only ever touch the
// viewport (view.center/view.zoom), never Paper's own scene graph, and
// Eyedropper only READS colors off whatever's hit-tested and writes them
// into state + a handful of color-picker DOM elements. None of them needed
// their own suspend()/renderWithOverlayItem live-preview machinery the way
// Draw/Shapes do, since there's no in-progress geometry to keep off
// Paper's scene graph during a drag — panning IS the "drag", and it only
// ever mutates the viewport, which engine-bridge.js's own tick() already
// diffs and re-renders on its own.
//
// Loaded BEFORE draw-bridge.js/etc. in index.html (matches tools.js's own
// onMouseDown, which checks `state.tool==='hand'||state.spaceDown` as its
// very FIRST, unconditional check, ahead of every other tool branch) so
// that holding Space to pan mid-drag with another tool active is checked
// first here too, rather than being swallowed by a later bridge's own
// stopImmediatePropagation for whatever tool happens to be active.
(function () {
  var panning = false;

  function engineOn() { return window.SMEngineBridge && window.SMEngineBridge.isEnabled() && !state.playing; }
  function shouldPan() { return engineOn() && (state.tool === 'hand' || state.spaceDown); }
  function shouldZoom() { return engineOn() && state.tool === 'zoom'; }
  function shouldPick() { return engineOn() && state.tool === 'eyedropper'; }

  // Mirrors tools.js's own eyedropper DOM updates for the stroke/fill color
  // pickers (swatch value, well background, on/off checkbox for fill).
  function setColorUI(kind, css) {
    if (kind === 'stroke') {
      state.strokeColor = css;
      ['color-stroke', 'pm-stroke-c'].forEach(function (id) { var el = document.getElementById(id); if (el) el.value = css; });
      ['stroke-well', 'pm-stroke'].forEach(function (id) { var el = document.getElementById(id); if (el) el.style.background = css; });
    } else {
      state.fillColor = css; state.fillEnabled = true;
      ['color-fill', 'pm-fill-c'].forEach(function (id) { var el = document.getElementById(id); if (el) el.value = css; });
      var pmFill = document.getElementById('pm-fill'); if (pmFill) pmFill.style.background = css;
      var fillWell = document.getElementById('fill-well'); if (fillWell) { fillWell.style.background = css; fillWell.classList.remove('none'); }
      var onCb = document.getElementById('p-fill-on'); if (onCb) onCb.checked = true;
    }
  }

  function pick(e) {
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var pt = new Point(w[0], w[1]);
    var layer = userLayers[state.activeLayerIdx];
    var hit = layer.hitTest(pt, { stroke: true, fill: true, tolerance: 8 / view.zoom });
    if (!(hit && hit.item instanceof Path)) return;
    var ep = hit.item;
    var isVB = !!(ep.data && ep.data.isVectorBrush);
    if (isVB && ep.fillColor) {
      setColorUI('stroke', ep.fillColor.toCSS(true));
    } else {
      if (ep.strokeColor) setColorUI('stroke', ep.strokeColor.toCSS(true));
      if (ep.fillColor) setColorUI('fill', ep.fillColor.toCSS(true));
    }
    if (ep.strokeWidth) {
      state.brushSize = ep.strokeWidth;
      var sw = document.getElementById('p-sw'); if (sw) sw.value = Math.round(ep.strokeWidth);
    }
    showToast('Color picked');
  }

  function onDown(e) {
    if (shouldPan()) {
      e.stopImmediatePropagation();
      e.preventDefault();
      panning = true;
      state.isPanning = true;
      window.SMEngineBridge.suspend();
      return;
    }
    if (shouldZoom()) {
      e.stopImmediatePropagation();
      e.preventDefault();
      if (e.altKey) view.zoom = Math.max(0.05, view.zoom * 0.8);
      else view.zoom = Math.min(20, view.zoom * 1.25);
      updZoom(); renderArcs();
      window.SMEngineBridge.renderNow();
      return;
    }
    if (shouldPick()) {
      e.stopImmediatePropagation();
      e.preventDefault();
      pick(e);
      window.SMEngineBridge.renderNow();
      return;
    }
  }
  function onMove(e) {
    if (!panning) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var dx = e.movementX || 0, dy = e.movementY || 0;
    view.center = view.center.subtract(new Point(dx, dy).divide(view.zoom));
    window.SMEngineBridge.renderNow();
  }
  function onUp(e) {
    if (!panning) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    panning = false;
    state.isPanning = false;
    window.SMEngineBridge.renderNow();
    window.SMEngineBridge.resume();
  }

  function init() {
    var target = document.getElementById('canvas-area') || document.getElementById('drawing-canvas');
    target.addEventListener('pointerdown', onDown, { capture: true });
    target.addEventListener('pointermove', onMove, { capture: true });
    target.addEventListener('pointerup', onUp, { capture: true });
    target.addEventListener('pointercancel', onUp, { capture: true });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
