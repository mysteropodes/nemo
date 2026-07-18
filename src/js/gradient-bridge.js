// ---- GRADIENT FILL (2026-07) ----
// Applies a 2-stop linear or radial gradient to the CURRENT selection's fill
// — the Effects panel section's own controls (index.html #effects-sec),
// only ever active alongside the Select/Subselect tool's selectedPaths
// (same target `setFillColor` already uses, timeline.js). Stored as
// `p.data.fillGradient = {kind,from,to,stops}` (world coordinates,
// serP/desP-persisted, app.js) — a plain data field, not a live Paper.js
// Gradient object, so every consumer that doesn't yet understand gradients
// (boolean ops, the eyedropper, thumbnails) still sees a reasonable flat
// `p.fillColor` fallback (stop 1's color) instead of crashing on an
// unexpected shape.
//
// No on-canvas drag-handle gizmo in this first version — direction/size for
// a linear gradient come from an Angle field (degrees) combined with the
// selection's own bounding box (from/to computed as the box's half-diagonal
// projected at that angle through its center); a radial gradient is always
// centered on the bbox center with a radius of the half-diagonal. A real
// draggable gizmo (drag the gradient's own start/end handles on the canvas,
// Illustrator/Figma-style) is a natural follow-up, not built here.
(function () {
  function selectionTargets() {
    if ((state.tool !== 'select' && state.tool !== 'subselect') || !window.selectedPaths || !selectedPaths.length) return [];
    return selectedPaths.filter(function (p) { return p instanceof Path || p instanceof CompoundPath; });
  }
  function controlRows() { return ['p-grad-controls', 'p-grad-kind-row', 'p-grad-angle-row', 'p-grad-apply-row']; }
  function setRowsVisible(on) {
    controlRows().forEach(function (id) { var row = document.getElementById(id); if (row) row.style.display = on ? 'flex' : 'none'; });
  }
  function applyGradientToSelection() {
    var targets = selectionTargets();
    if (!targets.length) { if (window.showToast) showToast('Sélectionnez une forme d\'abord'); return; }
    var c1 = document.getElementById('p-grad-c1').value;
    var c2 = document.getElementById('p-grad-c2').value;
    var kind = document.getElementById('p-grad-kind').value;
    var angleDeg = parseFloat(document.getElementById('p-grad-angle').value) || 0;
    pushUndo();
    targets.forEach(function (p) {
      var b = p.bounds;
      var cx = b.center.x, cy = b.center.y;
      var halfDiag = Math.hypot(b.width, b.height) / 2 || 1;
      var from, to;
      if (kind === 'radial') {
        from = [cx, cy]; to = [cx + halfDiag, cy];
      } else {
        var rad = angleDeg * Math.PI / 180;
        var dx = Math.cos(rad) * halfDiag, dy = Math.sin(rad) * halfDiag;
        from = [cx - dx, cy - dy]; to = [cx + dx, cy + dy];
      }
      p.data.fillGradient = { kind: kind, from: from, to: to, stops: [{ offset: 0, color: c1 }, { offset: 1, color: c2 }] };
      // Flat-color fallback (see file header) — keeps every non-gradient-
      // aware consumer showing a sane approximation instead of nothing.
      p.fillColor = c1;
    });
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    if (window.showToast) showToast('Dégradé appliqué');
  }
  function removeGradientFromSelection() {
    var targets = selectionTargets();
    if (!targets.length) return;
    pushUndo();
    targets.forEach(function (p) { if (p.data && p.data.fillGradient) delete p.data.fillGradient; });
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }
  function init() {
    var onCb = document.getElementById('p-grad-on');
    if (!onCb) return; // effects-sec markup not present in this build
    onCb.addEventListener('change', function () {
      setRowsVisible(this.checked);
      if (this.checked) applyGradientToSelection();
      else removeGradientFromSelection();
    });
    document.getElementById('btn-grad-apply').addEventListener('click', applyGradientToSelection);
    ['p-grad-c1', 'p-grad-c2', 'p-grad-kind', 'p-grad-angle'].forEach(function (id) {
      document.getElementById(id).addEventListener('input', function () {
        if (onCb.checked) applyGradientToSelection();
      });
    });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
