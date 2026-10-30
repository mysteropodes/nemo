/* The per-vertex weight popover of the Rig tool on a native object project (#2255). The rows are the weight entries one native read
 * returned for one vertex of one bind; each edit is reported to the caller, which commits it as ONE `object.rig.edit` (setWeight on a
 * finished percent edit, removeWeight, addWeight, resetWeights) and then calls `show` again with the re-read entries. Nothing here
 * reads a rig model or keeps a weight: it draws what it was given and says what the user did. Same classes and wording as the
 * baseline popover of rig-bridge.js (feedback #112), so the stylesheet is unchanged. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectRigWeightsUI = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const popovers = new WeakMap();
  const t = (win, key, fallback) => { try { const text = win.SM && win.SM.t ? win.SM.t(key) : null; return text && text !== key ? text : fallback; } catch (_) { return fallback; } };
  function make(win, tag, className, text) {
    const el = win.document.createElement(tag);
    if (className) el.className = className;
    if (text !== undefined) el.textContent = text;
    return el;
  }
  function close(win) {
    const open = popovers.get(win);
    if (!open) return;
    popovers.delete(win);
    if (open.el.remove) open.el.remove();
    open.off();
  }
  /* spec: { x, y, vertex, entries: [{ entry, boneId, label, percent }], addable: [{ boneId, label }],
   *         on: { set(entry, percent), remove(entry), add(boneId), reset() } }  Re-opening replaces the open popover in place. */
  function show(win, spec) {
    const before = popovers.get(win);
    if (before) { before.el.innerHTML = ''; }
    const el = before ? before.el : make(win, 'div', 'rig-weight-popover');
    el.appendChild(make(win, 'div', 'rig-weight-title', 'Poids du vertex #' + (spec.vertex + 1)));
    spec.entries.forEach((row) => {
      const line = make(win, 'div', 'rig-weight-row');
      line.appendChild(make(win, 'span', 'rig-weight-lbl', row.label));
      const input = make(win, 'input', 'pi scrub rig-weight-input');
      Object.assign(input, { type: 'number', min: '0', max: '100', step: '1', value: String(row.percent) });
      // A finished edit (change), not every keystroke: one native command, so one history entry.
      input.addEventListener('change', () => {
        const percent = Math.max(0, Math.min(100, parseFloat(input.value) || 0));
        spec.on.set(row.entry, percent);
      });
      line.appendChild(input);
      line.appendChild(make(win, 'span', 'rig-weight-pct', '%'));
      const del = make(win, 'button', 'rig-weight-del', '×');
      del.title = t(win, 'hsRemoveBoneFromVertex', 'Remove this bone from the vertex');
      del.addEventListener('click', () => spec.on.remove(row.entry));
      line.appendChild(del);
      el.appendChild(line);
    });
    if (spec.addable.length) {
      const line = make(win, 'div', 'rig-weight-row rig-weight-add');
      const select = make(win, 'select', 'pi');
      spec.addable.forEach((bone) => {
        const option = make(win, 'option', '', bone.label);
        option.value = bone.boneId;
        select.appendChild(option);
      });
      line.appendChild(select);
      const add = make(win, 'button', 'pbtn', t(win, 'hsAdd', 'Add'));
      add.addEventListener('click', () => spec.on.add(select.value));
      line.appendChild(add);
      el.appendChild(line);
    }
    const resetRow = make(win, 'div', 'rig-weight-row');
    const reset = make(win, 'button', 'pbtn', 'Réinitialiser (auto)');
    reset.title = t(win, 'hsRecomputeVertexWeights', 'Recompute this vertex');
    reset.addEventListener('click', () => spec.on.reset());
    resetRow.appendChild(reset);
    el.appendChild(resetRow);
    if (before) return el;
    const inner = Number(win.innerWidth) || 1024, tall = Number(win.innerHeight) || 768;
    el.style.left = Math.max(4, Math.min(spec.x + 12, inner - 220)) + 'px';
    el.style.top = Math.max(4, Math.min(spec.y - 8, tall - 260)) + 'px';
    win.document.body.appendChild(el);
    const outside = (e) => { if (!el.contains || !el.contains(e.target)) close(win); };
    const key = (e) => { if (e.key === 'Escape') close(win); };
    win.document.addEventListener('mousedown', outside, true);
    win.document.addEventListener('keydown', key);
    popovers.set(win, { el, off: () => { win.document.removeEventListener('mousedown', outside, true); win.document.removeEventListener('keydown', key); } });
    return el;
  }
  return Object.freeze({ show, close, isOpen: (win) => popovers.has(win) });
}));
