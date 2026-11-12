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
  // The list row under the pointer. Over the layer list the element under it names the row; over the frame grid (a gesture that began on a
  // grid grip) the grid rows keep the list's rhythm, so the row is the one whose vertical band holds the pointer.
  function rowAt(host, g, x, y) {
    const list = g.list, lb = list && typeof list.getBoundingClientRect === 'function' ? list.getBoundingClientRect() : null;
    if (lb && (x < lb.left || x > lb.right)) {
      return g.rows.find((r) => { const b = r.getBoundingClientRect(); return y >= b.top && y < b.bottom; }) || null;
    }
    const doc = host.document;
    let el = typeof doc.elementFromPoint === 'function' ? doc.elementFromPoint(x, y) : null;
    for (; el; el = el.parentNode) if (el.dataset && typeof el.dataset.layerUid === 'string' && el.dataset.layerUid) return el;
    return null;
  }
  // The gap (an index into the layer array, 0..n) a pointer at (x, y) selects, or -1.
  function gapAt(host, g, x, y) {
    const row = rowAt(host, g, x, y);
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
    const info = { layers: env.info.layers.map((l) => ({ layerUid: l.layerUid, name: l.name, kind: l.kind })) };
    PIN.forEach((k) => { info[k] = env.info[k]; });
    const list = env.list;
    const source = info.layers.findIndex((l) => l.layerUid === env.uid);
    if (source < 0 || info.layers.length < 2) return false;
    const sel = env.selection;
    const group = sel && sel.selected.length > 1 && sel.selected.includes(env.uid);
    const moving = group ? sel.selected.slice() : [env.uid];
    const rows = Array.from(list.children).filter((r) => r.dataset && r.dataset.layerUid && !r.hidden);
    const g = { info, moving, mark: null, folder: null, dropped: null, selection: group ? sel : { active: env.uid, selected: [env.uid] }, list, rows, x0: event.clientX, y0: event.clientY, moved: false, gap: -1, ghost: null, line: null, dimmed: [], ascending: ascending(rows, info) };
    gestures.set(host, g);
    const doc = host.document;
    function clear() { if (g.line) { g.line.remove(); g.line = null; } g.gap = -1; }
    function unmark() { if (g.mark) g.mark.classList.remove('folder-drop-target'); g.mark = null; g.folder = null; }
    // #2371 D36 (baseline moveLayerReorder): the MIDDLE band of another folder row drops INTO the folder instead of reordering; a dragged
    // folder never nests (no nested folders in v1), so it keeps the plain gap drop.
    function folderBand(e) {
      const row = rowAt(host, g, e.clientX, e.clientY), at = row ? info.layers.findIndex((l) => l.layerUid === row.dataset.layerUid) : -1;
      if (at < 0 || row.hidden || info.layers[at].kind !== 'folder' || info.layers[source].kind === 'folder' || moving.includes(info.layers[at].layerUid)) return null;
      const box = row.getBoundingClientRect(), rel = (e.clientY - box.top) / Math.max(1, box.height);
      return rel > 0.25 && rel < 0.75 ? row : null;
    }
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
      const band = folderBand(e);
      if (band) { clear(); if (g.mark !== band) { unmark(); g.mark = band; band.classList.add('folder-drop-target'); } g.folder = band.dataset.layerUid; return; }
      unmark();
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
      clear(); const dropped = g.folder; unmark(); g.dropped = dropped; g.dimmed.forEach((r) => { r.style.opacity = ''; });
      gestures.delete(host);
    }
    function key(e) { if (e.key === 'Escape') { e.stopPropagation(); g.cancelled = true; if (g.moved) host._layerDragJustEnded = true; end(); } }
    function up() {
      const { moved, gap } = g;
      end();
      if (!moved || g.cancelled) return;
      host._layerDragJustEnded = true; // the trailing click of a drag must not select
      if (gap < 0 && !g.dropped) return;
      // Bound to the state the gesture started on: a replaced project, another edit or an undo in between cancels it.
      let now = null;
      try { now = env.session.isActive() ? env.session.info() : null; } catch (_) { now = null; }
      if (!samePin(now, info) || g.moving.some((uid) => !now.layers.some((l) => l.layerUid === uid))) { env.toast(env.stale); return; }
      const order = g.moving.map((uid) => ({ uid, at: now.layers.findIndex((l) => l.layerUid === uid) })).sort((a, b) => a.at - b.at).map((o) => o.uid);
      const done = (r) => { if (r && r.ok === false) env.refused(r); }, lost = () => env.refused({ ok: false, code: 'unavailable' });
      if (g.dropped) { if (now.layers.some((l) => l.layerUid === g.dropped) && env.intoFolder) Promise.resolve(env.intoFolder(order, g.dropped)).then(done, lost); return; }
      // Leaving a folder by a drop frees the layers that sit in one AND lands the block at the gap, in the same command (one undo step).
      const nested = (uid) => { const l = now.layers.find((x) => x.layerUid === uid), p = l && typeof l.parentLayerUid === 'string' ? now.layers.find((x) => x.layerUid === l.parentLayerUid) : null; return !!p && p.kind === 'folder'; };
      const leaving = order.some(nested);
      if (noop(g, gap) && !leaving) return;
      Promise.resolve(leaving ? env.writes.leaveFolder(order, gap, g.selection) : env.writes.reorder(order, gap, g.selection)).then(done, lost);
    }
    // Both event families: the desktop webview emits the legacy mouse sequence, browser automation and pens only pointer events.
    host.addEventListener('mousemove', move, true); host.addEventListener('pointermove', move, true);
    host.addEventListener('mouseup', up, true); host.addEventListener('pointerup', up, true); doc.addEventListener('keydown', key, true);
    return true;
  }
  return Object.freeze({ arm, gapAt, samePin });
}));
