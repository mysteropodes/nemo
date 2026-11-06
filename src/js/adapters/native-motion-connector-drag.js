/* #2377: the drag of a Motion key connector of a native property row. Baseline: the connector mousedown and onDragMove of motion.js@1065e9d8 ("le trait est une
 * cible"): a plain drag moves both end keys by one shared delta (duration constant), Alt+drag retimes: the first key stays planted and only the second moves.
 * Neither can cross a key outside the pair, or leave 0..total-1. Nothing here holds the document: the move is ONE native `shift` edit of the selection on release
 * (a retime selects the second key alone for that edit and gives the pair its selection back), through the supplied ports. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeMotionConnectorDrag = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  /* The inclusive range of delta frames the pair may take, from the keys of the track outside it. */
  function limits(items, a, b, total, retime) {
    const sorted = items.slice().sort((x, y) => x.frame - y.frame);
    const ia = sorted.findIndex((k) => k.frame === a.frame), ib = sorted.findIndex((k) => k.frame === b.frame);
    if (ia < 0 || ib < 0) return null;
    const prev = ia > 0 ? sorted[ia - 1] : null, next = ib < sorted.length - 1 ? sorted[ib + 1] : null;
    const last = Math.min(total - 1, next ? next.frame - 1 : total - 1);
    return retime ? { lo: a.frame + 1 - b.frame, hi: last - b.frame } : { lo: (prev ? prev.frame + 1 : 0) - a.frame, hi: last - b.frame };
  }
  /* opts: {gesture (NemoNativeMotionKeyGesture), doc, event, fc, total, track, a, b, address(track, frame), view() (a fresh key read with selectKeys),
   * edit(action) (the selected keys' route), nodes(frames) (the key buttons of this track at those frames), snap (optional, as beginDrag), refresh(), onEnd()}.
   * Returns the drag handle, or null when the pair cannot be dragged. */
  function start(opts) {
    const retime = !!(opts.event && opts.event.altKey);
    const range = limits(opts.track.items, opts.a, opts.b, opts.total, retime);
    if (!range || !opts.gesture || typeof opts.gesture.beginDrag !== 'function' || range.lo > 0 || range.hi < 0) return null;
    const moving = retime ? [opts.b.frame] : [opts.a.frame, opts.b.frame];
    const mark = (view) => view.tracks.map((t) => (JSON.stringify(t.target) === JSON.stringify(opts.track.target) && t.property === opts.track.property
      ? { ...t, items: t.items.map((k) => ({ ...k, selected: moving.includes(k.frame) })) } : { ...t, items: t.items.map((k) => ({ ...k, selected: false })) }));
    async function commit(action) {
      if (!retime) return opts.edit(action);
      const view = opts.view();
      if (!view || view.selectKeys([opts.address(opts.track, opts.b.frame)]) !== true) return false;
      const done = await opts.edit(action);
      const after = opts.view();
      if (after && done === true) after.selectKeys([opts.address(opts.track, opts.a.frame), opts.address(opts.track, opts.b.frame + action.delta)]);
      if (opts.refresh) opts.refresh();
      return done;
    }
    return opts.gesture.beginDrag({
      doc: opts.doc, startX: opts.event.clientX, fc: opts.fc, total: opts.total, edit: commit, snap: opts.snap || null,
      nodes: () => opts.nodes(moving),
      tracks: () => { const view = opts.view(); return view ? mark(view) : null; },
      clamp: (delta) => Math.max(range.lo, Math.min(range.hi, delta)),
      onEnd: opts.onEnd
    });
  }
  return Object.freeze({ limits, start });
}));
