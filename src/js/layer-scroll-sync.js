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
    // Two mismatches stack here, not just one:
    // 1. grid (#fg-wrap) has an extra header INSIDE its own scrollable
    //    content — #frame-hdr (ruler) + #bars-row (onion/work-area), ~42px
    //    — that panel (#layer-list) has no equivalent of (its "Layers"
    //    title lives OUTSIDE the scroll container, in the fixed panel
    //    above it): a per-row content offset.
    // 2. panel's VIEWPORT is also shorter than grid's by ~82px, because
    //    BOTH that same title bar above AND #layer-ctrls (the +/delete/
    //    duplicate buttons) below sit OUTSIDE #layer-list, eating into its
    //    clientHeight — grid has no equivalent chrome on either edge
    //    reducing ITS clientHeight. So panel.scrollHeight-panel.clientHeight
    //    (its own true max) and grid.scrollHeight-grid.clientHeight (its
    //    own true max) don't just differ by the header — confirmed live:
    //    panelMax 452 vs gridMax 412, a 40px gap ≈ #layer-ctrls' own
    //    height, not the 42px header.
    // A raw 1:1 `to.scrollTop = from.scrollTop` mirror (the original bug)
    // therefore let panel reach ITS true max when scrolled directly
    // (452, unaffected by clamping since browsers auto-clamp an
    // OVER-assigned scrollTop down to grid's smaller max — that direction
    // "worked by accident"), but scrolling GRID to ITS OWN max only ever
    // pushed panel to 412 — 40px short of 452, hiding the last ~2 rows
    // (22px each) under #layer-ctrls exactly as reported. A single fixed
    // per-row offset can't fix the bottom edge case too (the two
    // mismatches don't cancel out), so: apply the per-row header offset
    // for ordinary mid-scroll (keeps row K's label roughly aligned with
    // row K's keyframe track), but SNAP the other side to ITS OWN true
    // max/min whenever `from` is at (or within half a row of) its own
    // bound — guarantees scrolling EITHER panel all the way down always
    // brings the other one all the way down too, regardless of the
    // viewport-height mismatch neither offset alone can paper over.
    function headerOffset() {
      return Math.max(0, grid.scrollHeight - panel.scrollHeight);
    }
    function mirror(from, to, fromIsGrid) {
      if (syncing) return;
      syncing = true;
      var fromMax = Math.max(0, from.scrollHeight - from.clientHeight);
      var toMax = Math.max(0, to.scrollHeight - to.clientHeight);
      var target;
      if (from.scrollTop >= fromMax - 1) target = toMax;
      else if (from.scrollTop <= 1) target = 0;
      else {
        var off = headerOffset();
        target = fromIsGrid ? from.scrollTop - off : from.scrollTop + off;
      }
      to.scrollTop = Math.max(0, Math.min(toMax, target));
      syncing = false;
    }
    panel.addEventListener('scroll', function () { mirror(panel, grid, false); });
    grid.addEventListener('scroll', function () { mirror(grid, panel, true); });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();
})();
