/* #2371 U11: drag-to-reorder of the native layer list rows (baseline armLayerReorder / moveLayerReorder / finishLayerReorder,
 * src/js/timeline.js @1065e9d8). A vertical gesture of at least 4px picks the layer (or the whole selection when the grabbed
 * row belongs to a multi-selection) up; the blue line marks the gap it will land in; the drop is ONE layer.reorder command,
 * so one gesture is one revision and one history entry. The gap is computed from the rows' own geometry, so it is right
 * whichever way the list is stacked. The gesture is bound to the project state it started on: a drop after the document
 * moved on (another edit, undo, a replaced project), or onto a layer that is gone, is cancelled with a notice and writes
 * nothing; Escape cancels too. Holds no document state beyond the gesture itself. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeLayerRowDrag = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const PIN = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  const gestures = new WeakMap();
  const samePin = (a, b) => !!a && !!b && PIN.every((k) => a[k] === b[k]);

  // Which way the list is stacked: true when the layer index grows downwards.
  function ascending(rows, info) {
    const at = rows.map((row) => info.layers.findIndex((l) => l.layerUid === row.dataset.layerUid)).filter((i) => i >= 0);
    return at.length < 2 || at[0] <= at[at.length - 1];
  }
  function rowAt(host, x, y) {
    const doc = host.document;
    let el = typeof doc.elementFromPoint === 'function' ? doc.elementFromPoint(x, y) : null;
    for (; el; el = el.parentNode) if (el.dataset && typeof el.dataset.layerUid === 'string' && el.dataset.layerUid) return el;
    return null;
  }
  // The gap (an index into the layer array, 0..n) a pointer at (x, y) selects, or -1.
  function gapAt(host, g, x, y) {
    const row = rowAt(host, x, y);
    const at = row ? g.info.layers.findIndex((l) => l.layerUid === row.dataset.layerUid) : -1;
    if (at < 0 || row.hidden) return { gap: -1 };
    const box = row.getBoundingClientRect(), after = y >= box.top + box.height / 2;
    return { gap: g.ascending ? at + (after ? 1 : 0) : at + (after ? 0 : 1), row, box, after };
  }
  function noop(g, gap) {
    const at = g.moving.map((uid) => g.info.layers.findIndex((l) => l.layerUid === uid)).sort((a, b) => a - b);
    return at.every((v, i) => i === 0 || v === at[i - 1] + 1) && (gap === at[0] || gap === at[at.length - 1] + 1);
  }

  // env: {writes, session, info, uid, selection, list, notice(text), toast(text)}; the event is the pointer/mouse down.
  function arm(host, env, event) {
    if (!event || event.button !== 0 || gestures.has(host)) return false;
    // The gesture keeps its own copy of the pin and of the layer order it started on, never the session's live object.
    const info = { layers: env.info.layers.map((l) => ({ layerUid: l.layerUid, name: l.name })) };
    PIN.forEach((k) => { info[k] = env.info[k]; });
    const list = env.list;
    const source = info.layers.findIndex((l) => l.layerUid === env.uid);
    if (source < 0 || info.layers.length < 2) return false;
    const sel = env.selection;
    const group = sel && sel.selected.length > 1 && sel.selected.includes(env.uid);
    const moving = group ? sel.selected.slice() : [env.uid];
    const rows = Array.from(list.children).filter((r) => r.dataset && r.dataset.layerUid && !r.hidden);
    const g = { info, moving, selection: group ? sel : { active: env.uid, selected: [env.uid] }, x0: event.clientX, y0: event.clientY, moved: false, gap: -1, ghost: null, line: null, dimmed: [], ascending: ascending(rows, info) };
    gestures.set(host, g);
    const doc = host.document;
    function clear() { if (g.line) { g.line.remove(); g.line = null; } g.gap = -1; }
    function move(e) {
      if (!g.moved) {
        const dx = Math.abs(e.clientX - g.x0), dy = Math.abs(e.clientY - g.y0);
        if (dy < 4 || dy < dx * 0.55) return; // a small wobble must not steal a click from the row
        g.moved = true;
        const row = rows.find((r) => r.dataset.layerUid === env.uid), box = row && row.getBoundingClientRect();
        g.ghost = doc.createElement('div'); g.ghost.className = 'layer-drag-ghost'; g.grab = box ? Math.max(0, Math.min(box.height, g.y0 - box.top)) : 11;
        g.ghost.textContent = moving.length > 1 ? moving.length + ' layers' : (info.layers[source].name || env.uid);
        if (box) { g.ghost.style.width = box.width + 'px'; g.ghost.style.height = box.height + 'px'; g.left = box.left; }
        doc.body.appendChild(g.ghost);
        g.dimmed = rows.filter((r) => moving.includes(r.dataset.layerUid)); g.dimmed.forEach((r) => { r.style.opacity = '0.42'; });
      }
      g.ghost.style.transform = 'translate3d(' + Math.round(g.left || 0) + 'px,' + Math.round(Math.max(6, e.clientY - g.grab)) + 'px,0)';
      const target = gapAt(host, g, e.clientX, e.clientY);
      if (target.gap < 0) { clear(); return; }
      g.gap = target.gap;
      if (!g.line) { g.line = doc.createElement('div'); g.line.className = 'layer-drop-indicator'; doc.body.appendChild(g.line); }
      const box = list.getBoundingClientRect();
      g.line.style.left = box.left + 'px'; g.line.style.width = box.width + 'px'; g.line.style.top = (target.after ? target.box.bottom : target.box.top) + 'px';
    }
    function end() {
      host.removeEventListener('mousemove', move, true); host.removeEventListener('pointermove', move, true);
      host.removeEventListener('mouseup', up, true); host.removeEventListener('pointerup', up, true); doc.removeEventListener('keydown', key, true);
      if (g.ghost) g.ghost.remove();
      clear(); g.dimmed.forEach((r) => { r.style.opacity = ''; });
      gestures.delete(host);
    }
    function key(e) { if (e.key === 'Escape') { e.stopPropagation(); g.cancelled = true; if (g.moved) host._layerDragJustEnded = true; end(); } }
    function up() {
      const { moved, gap } = g;
      end();
      if (!moved || g.cancelled) return;
      host._layerDragJustEnded = true; // the trailing click of a drag must not select
      if (gap < 0) return;
      // Bound to the state the gesture started on: a replaced project, another edit or an undo in between cancels it.
      let now = null;
      try { now = env.session.isActive() ? env.session.info() : null; } catch (_) { now = null; }
      if (!samePin(now, info) || g.moving.some((uid) => !now.layers.some((l) => l.layerUid === uid))) { env.toast(env.stale); return; }
      if (noop(g, gap)) return;
      const order = g.moving.map((uid) => ({ uid, at: now.layers.findIndex((l) => l.layerUid === uid) })).sort((a, b) => a.at - b.at).map((o) => o.uid);
      Promise.resolve(env.writes.reorder(order, gap, g.selection)).then((r) => { if (r && r.ok === false) env.refused(r); }, () => env.refused({ ok: false, code: 'unavailable' }));
    }
    // Both event families: the desktop webview emits the legacy mouse sequence, browser automation and pens only pointer events.
    host.addEventListener('mousemove', move, true); host.addEventListener('pointermove', move, true);
    host.addEventListener('mouseup', up, true); host.addEventListener('pointerup', up, true); doc.addEventListener('keydown', key, true);
    return true;
  }
  return Object.freeze({ arm, gapAt, samePin });
}));
