'use strict';
// The selection side of NemoNativeSelectOverlay (native-select-overlay.js) as the Select style tests need it: show() records the pick context the
// selection is drawn for; selectionContext() answers it only for that very selection (the same rule as the real overlay).
module.exports = function overlayStub(Intent, onDrop) {
  let sel = null;
  return {
    show(root) { const p = root.selectedPaths; sel = p && p.length ? { paths: p, count: p.length, ctx: Intent.pickContext(root) } : null; },
    selectionContext(root) { return sel && sel.paths === root.selectedPaths && sel.count === (root.selectedPaths || []).length ? sel.ctx : null; },
    // What refresh() does when the context moved on: the drawn selection ends and the pending style edit is flushed.
    refresh(root) { if (sel && sel.ctx !== Intent.pickContext(root)) { sel = null; if (onDrop) onDrop(root); } },
  };
};
