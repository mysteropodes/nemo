/* #2377: the native Motion key selection box. Baseline: updateKeySelectionBox / addMoveFill / addStaggerEdges / addSpaceEdges /
 * startSkewDrag and the skew branch of onDragMove (motion.js @1065e9d8). Shown once the selected keys sit on two or more property
 * rows; the top and bottom edges skew the rows, the left and right edges space the selection along time, the fill moves it and
 * Cmd/Ctrl + fill liquifies it (keys near the grab follow more than far ones). Alt + dragging a selected key skews like the bottom edge.
 * A drag previews the warp as a translation of the key buttons and commits ONE `warp` (or `shift` for the plain move) on release; an
 * unreachable drag keeps the last valid one and Escape commits nothing. The math mirrors the engine's `warp_factor` exactly. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeMotionKeyBox = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const trId = (t) => JSON.stringify([t.target.contextId, t.target.layerUid, Object.prototype.hasOwnProperty.call(t.target, 'strokeId'), t.target.strokeId, t.property]);
  const tr = (host, key, fallback) => { let v; try { v = host && host.SM && typeof host.SM.t === 'function' ? host.SM.t(key) : undefined; } catch (_) { v = undefined; } return typeof v === 'string' && v && v !== key ? v : fallback; };
  /* Math.round of the baseline: halves round up. */
  const round = (x) => Math.floor(x + 0.5);
  /* The share of the drag a key at `at` takes: `row` of `rows` over the selected frames `min..max` (engine `warp_factor`). */
  function share(mode, at, frame, grab) {
    const n = frame.rows, r = frame.row;
    if (mode === 'move') return 1;
    if (mode === 'liquify') { const radius = Math.max(2, Math.max(1, frame.max - frame.min) * 0.35), d = (at - grab) / radius; return Math.exp(-d * d); }
    if (mode === 'left' || mode === 'right') {
      const span = frame.max - frame.min;
      return span <= 0 ? 0 : mode === 'right' ? (at - frame.min) / span : (frame.max - at) / span;
    }
    if (n < 2) return 1;
    return mode === 'top' ? (n - 1 - r) / (n - 1) : r / (n - 1);
  }
  /* rows: [{id, selected: [frames], staying: [frames]}] in row order. Returns [{id, from: [..], to: [..]}] per row, or null when any key
   * would leave 0..last or land on a key that stays (the engine refuses the same gestures). Moved keys may pile onto one frame, as the
   * baseline `onDragMove` allowed: the engine keeps the one key the baseline evaluation answers with on that frame (#2377). */
  function plan(rows, mode, total, grab, last) {
    const all = rows.flatMap((r) => r.selected);
    if (!all.length) return null;
    const extent = { min: Math.min(...all), max: Math.max(...all), rows: rows.length };
    const out = [];
    for (let row = 0; row < rows.length; row++) {
      const r = rows[row], to = [];
      for (const at of r.selected) {
        const next = at + round(total * share(mode, at, Object.assign({ row }, extent), grab));
        if (next < 0 || next > last || r.staying.includes(next)) return null;
        to.push(next);
      }
      out.push({ id: r.id, from: r.selected.slice(), to });
    }
    return out;
  }
  /* The rows of a key read: tracks holding selected keys, top to bottom as `rank(track)` puts them. */
  function rowsOf(view, rank) {
    const rows = view.tracks.filter((t) => t.items.some((k) => k.selected))
      .map((t) => ({ track: t, id: trId(t), selected: t.items.filter((k) => k.selected).map((k) => k.frame).sort((a, b) => a - b), staying: t.items.filter((k) => !k.selected).map((k) => k.frame) }));
    if (rank) rows.sort((a, b) => rank(a.track) - rank(b.track));
    return rows;
  }
  /* One warp drag. opts: {doc, mode, startX, grab (frame, liquify), fc, last (final frame), rows (from rowsOf), nodes() ->
   * [{node, id, frame}] (every key button), edit(action, {order, expected}), onEnd()}. */
  function begin(opts) {
    const doc = opts.doc, order = new Map(opts.rows.map((r, i) => [r.id, i]));
    let total = 0, current = null, armed = false, finished = false;
    const sel = () => opts.nodes().filter((n) => order.has(n.id) && opts.rows[order.get(n.id)].selected.includes(n.frame));
    function paint(p) {
      const by = new Map();
      (p || []).forEach((r) => r.from.forEach((f, i) => by.set(r.id + '|' + f, r.to[i] - f)));
      sel().forEach((n) => { const d = by.get(n.id + '|' + n.frame) || 0; n.node.style.transform = d ? 'translateX(' + (d * opts.fc) + 'px)' : ''; });
    }
    function end(commit) {
      if (finished) return;
      finished = true;
      doc.removeEventListener('mousemove', move, true); doc.removeEventListener('mouseup', up, true); doc.removeEventListener('keydown', key, true);
      paint(null);
      const p = current, t = total;
      if (commit && p && p.some((r) => r.to.some((f, i) => f !== r.from[i]))) {
        const action = opts.mode === 'move' ? { kind: 'shift', delta: Math.round(t) } : { kind: 'warp', mode: opts.mode, total: t };
        if (opts.mode === 'liquify') action.grab = opts.grab;
        const landing = new Map(p.map((r) => [r.id, r.to]));
        Promise.resolve(opts.edit(action, {
          order: (a, b) => (order.has(trId(a)) ? order.get(trId(a)) : 1e9) - (order.has(trId(b)) ? order.get(trId(b)) : 1e9),
          expected: (track) => landing.get(trId(track)) || track.frames
        })).catch(() => {});
      }
      if (opts.onEnd) opts.onEnd();
    }
    function move(e) {
      if (!armed && Math.abs(e.clientX - opts.startX) < 3) return;
      armed = true;
      // The plain move is the rigid shift: whole frames, every row at the full distance.
      const want = opts.mode === 'move' ? Math.round((e.clientX - opts.startX) / opts.fc) : (e.clientX - opts.startX) / opts.fc;
      const next = plan(opts.rows, opts.mode, want, opts.grab, opts.last);
      if (!next) return;
      total = want; current = next; paint(next);
    }
    function up() { end(true); }
    function key(e) { if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); end(false); } }
    doc.addEventListener('mousemove', move, true); doc.addEventListener('mouseup', up, true); doc.addEventListener('keydown', key, true);
    return Object.freeze({ cancel() { end(false); }, total() { return total; } });
  }
  /* The box element over the selected keys. ctx: {doc, host, nodes() -> [{node, id, frame, selected, rect}] , view() (a fresh key read or null),
   * fc(), last(), rank(track), edit(action, opts), toast(msg), onChange()}. `update()` shows, moves or removes it. */
  function attach(ctx) {
    const doc = ctx.doc;
    let box = null, drag = null;
    const t = (key, fallback) => tr(ctx.host, key, fallback);
    function remove() { if (drag) drag.cancel(); if (box && box.parentNode) box.parentNode.removeChild(box); box = null; }
    function start(event, mode) {
      if (drag) return;
      const view = ctx.view();
      if (!view) return;
      const rows = rowsOf(view, ctx.rank);
      if (mode === 'left' || mode === 'right') {
        const all = rows.flatMap((r) => r.selected);
        if (!(Math.max(...all) > Math.min(...all))) { ctx.toast(t('toastSelectKeysOn2DifferentFramesToSpace', 'Select keys on 2 different frames to space them')); return; }
      } else if (mode !== 'move' && mode !== 'liquify' && rows.length < 2) { ctx.toast(t('toastSelectKeysOn2TracksToSkew', 'Select keys on 2 tracks to skew them')); return; }
      const grid = doc.getElementById ? doc.getElementById('frame-grid') : null;
      const grab = grid ? (event.clientX - grid.getBoundingClientRect().left) / ctx.fc() : 0;
      drag = begin({ doc, mode, startX: event.clientX, grab, fc: ctx.fc(), last: ctx.last(), rows, nodes: ctx.nodes, edit: ctx.edit, onEnd() { drag = null; } });
    }
    function edge(pos, hint) {
      const el = doc.createElement('div');
      el.className = 'motion-keysel-edge motion-keysel-edge-' + pos; el.title = hint;
      el.addEventListener('mousedown', (e) => {
        e.stopPropagation(); e.preventDefault();
        el.classList.add('grabbing');
        doc.addEventListener('mouseup', () => el.classList.remove('grabbing'), { once: true });
        start(e, pos);
      });
      return el;
    }
    function build() {
      const el = doc.createElement('div');
      el.className = 'motion-keysel-box';
      // The box's surfaces swallow right-clicks on the keys under them, and the key menu is where every batch edit lives: forward them.
      el.addEventListener('contextmenu', (e) => {
        const kids = Array.from(el.children), prev = kids.map((c) => c.style.pointerEvents), own = el.style.pointerEvents;
        el.style.pointerEvents = 'none'; kids.forEach((c) => { c.style.pointerEvents = 'none'; });
        const under = doc.elementFromPoint ? doc.elementFromPoint(e.clientX, e.clientY) : null;
        el.style.pointerEvents = own; kids.forEach((c, i) => { c.style.pointerEvents = prev[i]; });
        if (!under) return;
        e.preventDefault(); e.stopPropagation();
        under.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: e.clientX, clientY: e.clientY }));
      });
      const fill = doc.createElement('div');
      fill.className = 'motion-keysel-fill';
      fill.title = t('titleMoveAllSelectedKeysHint', 'Drag to move all selected keys together') + '\n⌘/Ctrl + drag: liquify, keys near the cursor follow more than far ones';
      fill.addEventListener('mousedown', (e) => { e.stopPropagation(); e.preventDefault(); start(e, e.metaKey || e.ctrlKey ? 'liquify' : 'move'); });
      el.appendChild(fill);
      ['top', 'bottom'].forEach((p) => el.appendChild(edge(p, t('titleSkewKeysHint', 'Drag horizontally to skew the keys (this edge moves, the other stays anchored)'))));
      ['left', 'right'].forEach((p) => el.appendChild(edge(p, t('titleSpaceKeysHint', 'Drag to space out / tighten the keys in time (the opposite edge stays anchored)'))));
      doc.body.appendChild(el);
      return el;
    }
    function update() {
      const picked = ctx.nodes().filter((n) => n.selected && n.rect);
      const rows = new Set(picked.map((n) => n.id));
      // Only keys stacked over several property rows earn the box; keys of one row are dragged as a group by grabbing any one.
      if (picked.length < 2 || rows.size < 2) { if (!drag) remove(); return false; }
      const x0 = Math.min(...picked.map((n) => n.rect.left)), x1 = Math.max(...picked.map((n) => n.rect.right));
      const y0 = Math.min(...picked.map((n) => n.rect.top)), y1 = Math.max(...picked.map((n) => n.rect.bottom));
      if (!box) box = build();
      box.style.left = x0 + 'px'; box.style.top = y0 + 'px'; box.style.width = (x1 - x0) + 'px'; box.style.height = (y1 - y0) + 'px';
      return true;
    }
    // The box is fixed over the page, so it follows the grid when it scrolls.
    let frame = 0;
    const win = doc.defaultView, schedule = win && typeof win.requestAnimationFrame === 'function' ? win.requestAnimationFrame.bind(win) : (fn) => fn();
    function onScroll() { if (!box || frame) return; frame = 1; schedule(() => { frame = 0; if (box) update(); }); }
    if (doc.addEventListener) doc.addEventListener('scroll', onScroll, true);
    return Object.freeze({ update, remove, start, cancel() { if (drag) drag.cancel(); },
      dispose() { remove(); if (doc.removeEventListener) doc.removeEventListener('scroll', onScroll, true); }, active() { return !!drag; } });
  }
  return Object.freeze({ share, plan, rowsOf, begin, attach, round });
}));
