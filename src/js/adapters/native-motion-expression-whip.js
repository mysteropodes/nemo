/* D18-C (#2376): the expression pickwhip of the native editor. Baseline startExprPickwhip @1065e9d8: a rubber band from the
 * editor's whip to a property row; a drop on a control row emits `layerControl("uid","Name")` (CLAUDE.md section 13), Alt clones the
 * source's own expression. Only an expression control is referenceable natively; a drop on any other row, or on the editing row itself
 * (a cycle), is rejected here with no native call at all, so no history entry. Rows carry `_nemoPick` ({target, property,
 * controlName, controlType}) set by the Motion panel. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeMotionExpressionWhip = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  // source: {layerUid, property}; drop: a row's `_nemoPick` or null. -> {text} | {reject: 'cycle' | 'notReferenceable'}
  function reference(source, drop) {
    if (!drop || !drop.target || typeof drop.target.layerUid !== 'string') return { reject: 'notReferenceable' };
    if (drop.target.layerUid === source.layerUid && drop.property === source.property) return { reject: 'cycle' };
    if (drop.controlType === null || drop.controlType === undefined || typeof drop.controlName !== 'string') return { reject: 'notReferenceable' };
    return { text: 'layerControl(' + JSON.stringify(drop.target.layerUid) + ', ' + JSON.stringify(drop.controlName) + ')' };
  }
  // The DOM gesture. onDrop(pick, {alt}) is called once on release over a tagged row; Escape or a release elsewhere cancels.
  function start(host, ev, fromEl, onDrop) {
    const doc = host.document;
    if (ev && ev.stopPropagation) ev.stopPropagation();
    if (ev && ev.preventDefault) ev.preventDefault();
    const box = typeof fromEl.getBoundingClientRect === 'function' ? fromEl.getBoundingClientRect() : { left: 0, top: 0, width: 0, height: 0 };
    const ox = box.left + box.width / 2, oy = box.top + box.height / 2;
    const line = doc.createElement('div');
    line.className = 'lpick-line';
    doc.body.appendChild(line);
    let hover = null;
    const under = (x, y) => {
      const el = typeof doc.elementFromPoint === 'function' ? doc.elementFromPoint(x, y) : null;
      const row = el && el.closest ? el.closest('.motion-prop-row') : null;
      return row && row._nemoPick ? row : null;
    };
    const paint = (x, y) => {
      const dx = x - ox, dy = y - oy;
      line.style.left = ox + 'px'; line.style.top = oy + 'px';
      line.style.width = Math.sqrt(dx * dx + dy * dy) + 'px'; line.style.transform = 'rotate(' + Math.atan2(dy, dx) + 'rad)';
    };
    const onMove = (e) => {
      paint(e.clientX, e.clientY);
      const row = under(e.clientX, e.clientY);
      if (hover && hover !== row) hover.classList.remove('pick-target');
      if (row && row !== hover) row.classList.add('pick-target');
      hover = row;
    };
    const cleanup = () => {
      doc.removeEventListener('mousemove', onMove, true); doc.removeEventListener('mouseup', onUp, true); doc.removeEventListener('keydown', onKey, true);
      if (hover) hover.classList.remove('pick-target');
      line.remove();
    };
    function onUp(e) {
      const row = under(e.clientX, e.clientY), alt = !!e.altKey;
      cleanup();
      if (row) onDrop(row._nemoPick, { alt });
    }
    function onKey(e) { if (e.key === 'Escape') cleanup(); }
    doc.addEventListener('mousemove', onMove, true); doc.addEventListener('mouseup', onUp, true); doc.addEventListener('keydown', onKey, true);
    paint(ev.clientX || ox, ev.clientY || oy);
    return cleanup;
  }
  return Object.freeze({ reference, start });
}));
