// ---- LAYER PANEL <-> FRAME GRID VERTICAL SCROLL SYNC (2026-07) ----
// #layer-list (the property/layer names, left) and #fg-wrap (the keyframe
// grid, right) are two INDEPENDENT scroll containers (both `overflow:auto`
// in their own right, style.css) with no sync between them — always true,
// but invisible in Animation 2D where a project rarely has enough layers
// to need vertical scrolling at all. Motion mode's rows (5 Transform
// properties × N elements × M layers) make this the common case instead of
// the rare one — feedback: scrolling one panel visibly desyncs a
// property's name from its own keyframe track row.
//
// REWRITTEN 2026-07-25 ("gros problème d'alignement des propriétés quand on
// scroll dans la timeline", with before/after screenshots). The previous
// version mirrored with a per-row `headerOffset()` correction equal to
// grid.scrollHeight - panel.scrollHeight (~42px, the ruler + bars header
// that lives INSIDE the grid's scroll content and has no counterpart in the
// panel). That correction was the bug, not the fix: the two containers
// already line up exactly at scrollTop 0 — the header is paid for by the
// panel's own "Layers" title bar, which sits OUTSIDE #layer-list and pushes
// its viewport down by the same amount. Measured live: with the old sync,
// scrolling the grid to 100 left the panel at 58, i.e. every row 42px out of
// register; a plain 1:1 gives exactly 0.
//
// What the offset was really chasing is the OTHER mismatch, at the bottom
// edge: the panel's viewport is ~82px shorter (both that title bar above and
// #layer-ctrls — the +/delete/duplicate buttons — below sit outside
// #layer-list and eat its clientHeight, while the grid has no equivalent
// chrome), so panelMax (452) > gridMax (412). Under a 1:1 mirror, scrolling
// the GRID to its own max leaves the panel 40px short, hiding its last rows
// under #layer-ctrls. A per-row offset cannot fix that — it just trades a
// bottom-edge bug for a permanent misalignment at every other position.
//
// So: equalize the two scroll RANGES with a spacer at the end of the grid's
// content, then mirror 1:1. Both ends line up, and every row in between
// stays in register by construction rather than by correction.
(function () {
  function init() {
    var panel = document.getElementById('layer-list');
    var grid = document.getElementById('fg-wrap');
    if (!panel || !grid) return;

    // Lives inside the grid's scroll content, after every row, so it extends
    // the grid's scrollable range without being mistaken for a row by
    // anything that queries .frow / .motion-track-row.
    var spacer = document.createElement('div');
    spacer.id = 'fg-bottom-spacer';
    spacer.style.cssText = 'height:0;flex:0 0 auto;pointer-events:none;';
    grid.appendChild(spacer);

    // Re-entrancy guard: setting scrollTop on the OTHER element fires its
    // own 'scroll' event, which would otherwise bounce back and forth.
    var syncing = false;

    function equalize() {
      // Zero it first so the measurement below reads the CONTENT's own
      // height rather than last call's compensation (idempotent).
      spacer.style.height = '0px';
      var panelMax = Math.max(0, panel.scrollHeight - panel.clientHeight);
      var gridMax = Math.max(0, grid.scrollHeight - grid.clientHeight);
      var pad = Math.max(0, panelMax - gridMax);
      spacer.style.height = pad + 'px';
    }

    function mirror(from, to) {
      if (syncing) return;
      syncing = true;
      // Cheap, and this is exactly the moment it matters: a render that
      // changed the row count may not have been observed yet.
      equalize();
      var toMax = Math.max(0, to.scrollHeight - to.clientHeight);
      to.scrollTop = Math.max(0, Math.min(toMax, from.scrollTop));
      syncing = false;
    }

    panel.addEventListener('scroll', function () { mirror(panel, grid); });
    grid.addEventListener('scroll', function () { mirror(grid, panel); });

    // Row counts change on almost every interaction (expand a layer, reveal
    // animated properties, add/remove a layer), and each change moves both
    // maxima — so the spacer has to be recomputed, not set once.
    // A ResizeObserver on the two SCROLL CONTAINERS is not enough: their own
    // border box never changes when their content grows, so it only catches
    // window/panel resizes (measured: the spacer stayed at 0 while the row
    // count went from 3 to 26). Both are needed — the mutation observer for
    // content changes, the resize observer for viewport changes.
    var pending = false;
    function schedule() {
      if (pending) return;
      pending = true;
      requestAnimationFrame(function () { pending = false; equalize(); });
    }
    if (window.MutationObserver) {
      var mo = new MutationObserver(schedule);
      var fg = document.getElementById('frame-grid');
      mo.observe(panel, { childList: true, subtree: true });
      if (fg) mo.observe(fg, { childList: true, subtree: true });
    }
    if (window.ResizeObserver) {
      var ro = new ResizeObserver(schedule);
      ro.observe(panel);
      ro.observe(grid);
    } else {
      window.addEventListener('resize', schedule);
    }
    equalize();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();
})();
