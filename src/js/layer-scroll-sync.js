// ---- LAYER PANEL <-> FRAME GRID VERTICAL SCROLL SYNC (2026-07) ----
// #layer-list (the property/layer names, left) and #fg-wrap (the keyframe
// grid, right) are two INDEPENDENT scroll containers (both `overflow:auto`
// in their own right, style.css) with no sync between them — always true,
// but invisible in Animation 2D where a project rarely has enough layers
// to need vertical scrolling at all. Motion mode's rows (5 Transform
// properties × N elements × M layers) make this the common case instead of
// the rare one — feedback: scrolling one panel visibly desyncs a
// property's name from its own keyframe track row.
(function () {
  function init() {
    var panel = document.getElementById('layer-list');
    var grid = document.getElementById('fg-wrap');
    if (!panel || !grid) return;
    // Re-entrancy guard: setting scrollTop on the OTHER element fires its
    // own 'scroll' event, which would otherwise bounce back and forth.
    var syncing = false;
    function mirror(from, to) {
      if (syncing) return;
      syncing = true;
      to.scrollTop = from.scrollTop;
      syncing = false;
    }
    panel.addEventListener('scroll', function () { mirror(panel, grid); });
    grid.addEventListener('scroll', function () { mirror(grid, panel); });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();
})();
