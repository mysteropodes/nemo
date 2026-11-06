/* The Motion key spans of a native property row (#2377): the duration block behind each key and the connector bar between two consecutive keys whose
 * value changes, tinted with the key colour exactly as motion.js@1065e9d8 drew them (FC >= 18 for the blocks; a connector takes a colour only when
 * BOTH its keys carry the same one). It owns no state and commits nothing: a click on a connector hands the two end keys to `onPick`. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeMotionKeySpans = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const ROW_H = 22, BAR_H = 5;
  function changed(a, b) {
    if (!Array.isArray(a.v) || !Array.isArray(b.v)) return true; // no value to compare: draw the span rather than hide a move
    return a.v.length !== b.v.length || a.v.some((n, d) => n !== b.v[d]);
  }
  function box(doc, className, left, top, width, height) {
    const node = doc.createElement('div');
    node.className = className;
    Object.assign(node.style, { position: 'absolute', left: left + 'px', top: top + 'px', width: width + 'px', height: height + 'px' });
    return node;
  }
  function setColour(node, colour) {
    node.classList.add('tinted');
    if (typeof node.style.setProperty === 'function') node.style.setProperty('--key-color', colour); else node.style['--key-color'] = colour;
  }
  /* entries: [{ track, lane, top }] of one row (top = the key buttons' top, 6px under the lane's top); returns the nodes appended to `row`. */
  function draw(doc, row, entries, fc, onPick) {
    const out = [];
    if (!doc || !row || !Array.isArray(entries) || !(fc > 0)) return out;
    entries.forEach(function (entry) {
      const items = entry.track.items, lane = entry.top - 6;
      if (fc >= 18) {
        items.forEach(function (key) {
          const block = box(doc, 'native-key-durblock motion-key-durblock', key.frame * fc, lane, fc, ROW_H);
          block.style.pointerEvents = 'none'; block.style.zIndex = '1';
          if (key.color) setColour(block, key.color);
          row.appendChild(block); out.push(block);
        });
      }
      for (let i = 0; i < items.length - 1; i++) {
        const a = items[i], b = items[i + 1];
        if (!changed(a, b)) continue;
        const bar = box(doc, 'native-key-connect motion-key-connect', a.frame * fc + fc / 2, lane + Math.round((ROW_H - BAR_H) / 2), (b.frame - a.frame) * fc, BAR_H);
        bar.style.zIndex = '2';
        if (a.selected && b.selected) bar.classList.add('sel');
        if (a.color && b.color && a.color === b.color) setColour(bar, a.color);
        bar.dataset.keyFrom = String(a.frame); bar.dataset.keyTo = String(b.frame);
        bar.addEventListener('mousedown', function (event) { if (typeof onPick === 'function') onPick(event, entry.track, a, b); });
        row.appendChild(bar); out.push(bar);
      }
    });
    return out;
  }
  return Object.freeze({ draw, changed });
}));
