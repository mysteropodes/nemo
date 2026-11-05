/* #2524 / #2377: pointer gestures on native Motion keys, as pure UI. Baseline: the key cell context menu and onDragMove/onDragUp of
 * motion.js plus selectEveryNthKey / grabRandomKeys / invertKeySelection (@1065e9d8). A drag previews the shift as a translation of the
 * key buttons and commits ONE `shift` on release (zero distance or Escape commits nothing); the menu items run one batch edit each.
 * Nothing here holds the document: `edit(action)` is the supplied native route, `view` the correlated key read. */
(function (root, factory) {
  const api = factory(typeof module === 'object' && module.exports ? require('./native-motion-key-edit.js') : root.NemoNativeMotionKeyEdit);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeMotionKeyGesture = api;
}(typeof globalThis === 'object' ? globalThis : this, function (KeyEdit) {
  'use strict';
  const MAX_SELECTED = 4096; // a guard against a runaway selection; the edit names runs of keys, so the count never sizes the request
  const tr = (host, key, fallback) => { let v; try { v = host && host.SM && typeof host.SM.t === 'function' ? host.SM.t(key) : undefined; } catch (_) { v = undefined; } return typeof v === 'string' && v && v !== key ? v : fallback; };
  const address = (track, frame) => ({ target: { kind: 'property', target: track.target, property: track.property }, frame });
  const flat = (view) => view.tracks.flatMap((track) => track.items.map((key) => ({ track, key })));
  const chosen = (view) => flat(view).filter((e) => e.key.selected);
  const byFrame = (a, b) => a.key.frame - b.key.frame;
  /* Selection-only operations over a key read; each returns whether the selection was accepted (at most 4096 keys). */
  function selectEveryNth(view, n) {
    const step = Math.max(2, parseInt(n, 10) || 2);
    return view.selectKeys(chosen(view).sort(byFrame).filter((_, i) => i % step === 0).map((e) => address(e.track, e.key.frame))) === true;
  }
  function keepRandom(view, percent, random) {
    const share = Math.max(1, Math.min(100, parseInt(percent, 10) || 50)) / 100, pool = chosen(view), out = [];
    const want = Math.max(1, Math.round(pool.length * share)), rng = random || Math.random;
    while (out.length < want && pool.length) out.push(pool.splice(Math.floor(rng() * pool.length), 1)[0]);
    return view.selectKeys(out.map((e) => address(e.track, e.key.frame))) === true;
  }
  // Within the tracks currently drawn, the same universe the marquee draws over.
  function invert(view) {
    const rest = flat(view).filter((e) => !e.key.selected).map((e) => address(e.track, e.key.frame));
    return rest.length <= MAX_SELECTED && view.selectKeys(rest) === true;
  }
  /* The key context menu. ctx: {view, edit(action), selectOnly(), hold (bool: the right-clicked key holds), toast(msg), prompt(msg, def), refresh()}. */
  function menuItems(host, ctx) {
    const t = (key, fallback) => tr(host, key, fallback);
    const count = chosen(ctx.view).length;
    const run = (fn) => () => { try { if (fn() === false) ctx.toast('The key selection was not accepted.'); } finally { ctx.refresh(); } };
    const act = (action) => () => ctx.edit(action);
    const items = [
      { label: t('ctxDeleteThisKey', 'Delete this key'), action() { return ctx.selectOnly() ? ctx.edit({ kind: 'delete' }) : false; } },
      { label: ctx.hold ? t('ctxRemoveHold', 'Remove hold') : t('ctxMakeHold', 'Hold'), action: act({ kind: 'preset', preset: 'hold' }) },
      { sep: true },
      { label: t('ctxEasyEaseF9', 'Easy Ease  (F9)'), action: act({ kind: 'preset', preset: 'ease' }) },
      { label: t('ctxEasyEaseInShiftF9', 'Easy Ease In  (Shift+F9)'), action: act({ kind: 'preset', preset: 'easeIn' }) },
      { label: t('ctxEasyEaseOutCmdShiftF9', 'Easy Ease Out  (Cmd+Shift+F9)'), action: act({ kind: 'preset', preset: 'easeOut' }) },
      { label: t('ctxLinearCmdAltK', 'Linear  (Cmd+Alt+K)'), action: act({ kind: 'preset', preset: 'linear' }) },
    ];
    if (count >= 2) {
      items.push({ sep: true },
        { label: t('ctxDistributeEvenly', 'Distribute evenly'), action: act({ kind: 'distribute' }) },
        { label: t('ctxFlipOrder', 'Flip order'), action: act({ kind: 'flip' }) },
        { label: t('ctxSubdivideMidpoint', 'Subdivide (key at midpoint)'), action: act({ kind: 'subdivide' }) },
        { label: t('ctxSelectEvery2nd', 'Select every other one'), action: run(() => selectEveryNth(ctx.view, 2)) },
        { label: t('ctxSelectEveryNthEllipsis', 'Select 1 in N…'), action: run(() => {
          const v = ctx.prompt(t('promptKeepOneKeyOutOf', 'Keep one key out of how many?'), '3');
          return v === null ? true : selectEveryNth(ctx.view, v);
        }) },
        { label: t('ctxKeepRandomEllipsis', 'Keep at random…'), action: run(() => {
          const v = ctx.prompt(t('promptKeepPercentOfSelection', 'What percentage of the selection to keep?'), '50');
          return v === null ? true : keepRandom(ctx.view, v);
        }) });
    }
    if (count >= 1) items.push({ label: t('ctxInvertSelection', 'Invert the selection'), action: run(() => invert(ctx.view)) });
    return items;
  }
  /* A group drag. opts: {startX, fc, total, nodes() (the selected key buttons), tracks() (a fresh read, for collisions), edit(action),
   * doc (document), onEnd()}. The preview never exceeds a shift the engine would accept: an unreachable frame keeps the last valid one. */
  function beginDrag(opts) {
    const doc = opts.doc;
    let delta = 0, armed = false, finished = false;
    function preview(by) { opts.nodes().forEach((n) => { n.style.transform = by ? 'translateX(' + (by * opts.fc) + 'px)' : ''; }); }
    function end(commit) {
      if (finished) return;
      finished = true;
      doc.removeEventListener('mousemove', move, true);
      doc.removeEventListener('mouseup', up, true);
      doc.removeEventListener('keydown', key, true);
      preview(0);
      const by = delta;
      if (commit && by !== 0) Promise.resolve(opts.edit({ kind: 'shift', delta: by })).catch(() => {});
      if (opts.onEnd) opts.onEnd();
    }
    function move(e) {
      if (!armed && Math.abs(e.clientX - opts.startX) < 3) return;
      armed = true;
      const want = Math.round((e.clientX - opts.startX) / opts.fc);
      if (want === delta) return;
      const tracks = opts.tracks();
      if (tracks && KeyEdit.canShift(tracks, want, opts.total)) { delta = want; preview(delta); }
    }
    function up() { end(true); }
    function key(e) { if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); end(false); } }
    doc.addEventListener('mousemove', move, true);
    doc.addEventListener('mouseup', up, true);
    doc.addEventListener('keydown', key, true);
    return Object.freeze({ cancel() { end(false); }, delta() { return delta; } });
  }
  return Object.freeze({ selectEveryNth, keepRandom, invert, menuItems, beginDrag });
}));
