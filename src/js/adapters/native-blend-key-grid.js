/* #2257: the Blend row of the Motion key grid on a native object project. Baseline: motion.js renderDiscreteKeyGridRow,
 * onDragMove / onDragUp (group drag), nudgeSelectedKeys and deleteSelectedKeys @1065e9d8: click / Cmd-click / Shift-range /
 * marquee select the diamonds, a drag or Alt+Left/Right moves the selected ones together, Delete / Backspace removes them.
 * The keys live in the engine, so this module owns two things the legacy machinery cannot: (1) STABLE key objects for the
 * generic selection (`trackFor` hands them out; the project info is re-read on every render, a fresh copy each time would
 * lose the selection), and (2) the commit of a gesture as ONE native compositing edit per layer (`blendKeysMove` /
 * `blendKeysRemove`: one revision, one history entry, modes travel with the keys). It writes nothing else, keeps no
 * document state, and has no fallback writer: a refused edit leaves the engine keys as they were. A selection that spans
 * several layers is applied layer by layer (one edit, hence one history entry, per layer: the engine edit is per layer). */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeBlendKeyGrid = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const PROP = 'blendKeys';
  // The surfaces the Motion timeline keys act on (the same list as motion.js's acceptsTarget for native Motion keys).
  const SURFACES = ['drawing-canvas', 'canvas-area', 'canvas-col', 'layer-list', 'frame-grid', 'timeline-area', 'props-panel'];
  const isMod = (e) => !!(e.metaKey || e.ctrlKey);
  const byFrame = (a, b) => a.frame - b.frame;
  function editable(node) {
    for (; node; node = node.parentNode) if (['INPUT', 'SELECT', 'TEXTAREA'].includes(node.tagName) || node.isContentEditable) return true;
    return false;
  }
  function accepts(event, document) {
    let node = event.target;
    if (!node || node === document.body || node === document.documentElement) return true;
    for (; node; node = node.parentNode) if (SURFACES.includes(node.id)) return true;
    return false;
  }
  /* The one gesture a key press means over selected Blend keys, or null. */
  function actionForEvent(e) {
    const k = String(e.key || '');
    if (k === 'Delete' || k === 'Backspace') return { kind: 'remove' };
    if (e.altKey && !isMod(e) && (k === 'ArrowLeft' || k === 'ArrowRight')) return { kind: 'move', by: k === 'ArrowRight' ? 1 : -1 };
    return null;
  }

  /* host: the window (state, SMMotion, document, nativeCompositingRefresh / nativeCompositingRefused); comp: the native
   * compositing adapter (blendKeysOf, layerUid, moveBlendKeys, removeBlendKeys). */
  function create(host, comp) {
    const cache = new Map(); // layerUid -> Map('frame|mode' -> the key object handed out for it)
    let busy = false, retired = false, listener = null;
    const layers = () => (host.state && Array.isArray(host.state.layers) ? host.state.layers : []);
    const motion = () => host.SMMotion || null;
    const holderAt = (li) => layers()[li] || null;
    const selection = () => { const m = motion(); const sel = m && typeof m.keySelection === 'function' ? m.keySelection() : null; return Array.isArray(sel) ? sel : []; };

    /* The layer's keys as stable objects: the same object for the same (frame, mode) while the engine holds it, so the
     * generic selection (which compares objects) survives the re-reads of every render. A drag mutates `frame` of the
     * object live; the lookup is by the ENGINE frame, so the moving object keeps being handed out until it is committed. */
    function keysOf(li) {
      const uid = comp.layerUid(li);
      if (!uid) return [];
      const prev = cache.get(uid) || new Map(), next = new Map();
      const out = comp.blendKeysOf(li).map((k) => {
        const id = k.frame + '|' + k.mode;
        const key = prev.get(id) || { frame: k.frame, mode: k.mode };
        // Only a live drag may hold a key off the engine's frame; any other mutation (a box skew) is not a write.
        if (key.frame !== k.frame && !host._motionKeyDrag && !host._motionSkewDrag) key.frame = k.frame;
        next.set(id, key);
        return key;
      });
      cache.set(uid, next);
      return out;
    }
    /* `trackFor(holder, 'blendKeys')`: the keys sorted by their CURRENT frame (a drag moves them), null when none. */
    function track(holder) {
      const li = layers().indexOf(holder);
      if (li < 0) return null;
      const keys = keysOf(li).slice().sort(byFrame);
      return keys.length ? { keys } : null;
    }
    /* The selected Blend keys by layer index: Map(li -> [{key, entry}]). */
    function selectedByLayer() {
      const groups = new Map();
      selection().forEach((entry) => {
        if (!entry || entry.prop !== PROP || !entry.key) return;
        const li = layers().indexOf(entry.holder);
        if (li < 0) return;
        if (!groups.has(li)) groups.set(li, []);
        groups.get(li).push({ key: entry.key, entry });
      });
      return groups;
    }
    function refresh() { if (typeof host.nativeCompositingRefresh === 'function') host.nativeCompositingRefresh(); }
    function refused() { if (typeof host.nativeCompositingRefused === 'function') host.nativeCompositingRefused(); }
    function select(list) {
      const m = motion();
      if (m && typeof m.setKeySelection === 'function') m.setKeySelection(list);
    }
    /* One edit per layer, in order. `jobs`: [{li, frames (strictly increasing), by (null: remove)}]. After the writes the
     * selection follows the keys (moved: onto their new frames; removed: gone; refused: back where it was), the confirmed
     * project info is drawn, and the result says whether every edit landed. */
    async function dispatch(jobs) {
      const others = selection().filter((s) => s && s.prop !== PROP);
      const kept = [];
      let ok = true;
      for (const job of jobs) {
        let landed = false;
        try {
          await (job.by === null ? comp.removeBlendKeys(job.li, job.frames) : comp.moveBlendKeys(job.li, job.frames, job.by));
          landed = true;
        } catch (_) { ok = false; refused(); }
        if (landed && job.by === null) continue;
        const frames = landed ? job.frames.map((f) => f + job.by) : job.frames;
        const here = keysOf(job.li);
        frames.forEach((f) => {
          const key = here.find((k) => k.frame === f);
          if (key) kept.push({ holder: holderAt(job.li), prop: PROP, key });
        });
      }
      select(others.concat(kept));
      refresh();
      return ok;
    }
    function jobsFor(groups, by) {
      return Array.from(groups.entries()).map(([li, items]) => ({ li, frames: items.map((i) => i.key.frame).sort((a, b) => a - b), by }));
    }
    /* Delete / Backspace over the selection: every selected Blend key removed, one edit per layer. */
    function removeSelected() {
      const groups = selectedByLayer();
      return groups.size ? dispatch(jobsFor(groups, null)) : Promise.resolve(null);
    }
    /* Alt+Left / Alt+Right: the selected Blend keys shifted by `by` frames, whole or not at all. */
    function nudgeSelected(by) {
      const groups = selectedByLayer();
      return groups.size && Number.isInteger(by) && by !== 0 ? dispatch(jobsFor(groups, by)) : Promise.resolve(null);
    }
    /* The end of a drag: `dragged` is the legacy group-drag list [{holder, prop, key, origFrame}]. The diamonds were moved
     * live (their `frame` mutated); put them back, then write the move natively. Returns null when no Blend key was dragged. */
    function commitDrag(dragged) {
      const groups = new Map();
      (dragged || []).forEach((d) => {
        if (!d || d.prop !== PROP || !d.key) return;
        const li = layers().indexOf(d.holder);
        if (li < 0) return;
        if (!groups.has(li)) groups.set(li, []);
        groups.get(li).push(d);
      });
      if (!groups.size) return null;
      const jobs = [];
      groups.forEach((items, li) => {
        const by = items[0].key.frame - items[0].origFrame;
        const frames = items.map((d) => d.origFrame).sort((a, b) => a - b);
        items.forEach((d) => { d.key.frame = d.origFrame; });
        if (by !== 0) jobs.push({ li, frames, by });
      });
      if (!jobs.length) { refresh(); return Promise.resolve(true); }
      return dispatch(jobs);
    }
    function onKey(event) {
      if (retired || event.defaultPrevented || editable(event.target)) return;
      const action = actionForEvent(event);
      if (!action) return;
      const document = host.document;
      if (!host.state || host.state.appMode !== 'motion' || !accepts(event, document)) return;
      if (document.querySelector && document.querySelector('.modal:not(.hid),dialog[open],[aria-modal="true"]')) return;
      const groups = selectedByLayer();
      if (!groups.size) return;
      // A selection that also holds other keys leaves the press to their owners (the native Motion key editor).
      const pure = selection().every((s) => s && s.prop === PROP);
      if (pure) {
        event.preventDefault();
        if (event.stopImmediatePropagation) event.stopImmediatePropagation(); else if (event.stopPropagation) event.stopPropagation();
      }
      if (busy) return;
      busy = true;
      const done = () => { busy = false; };
      (action.kind === 'remove' ? removeSelected() : nudgeSelected(action.by)).then(done, done);
    }
    function install() {
      const document = host.document;
      if (listener || !document || typeof document.addEventListener !== 'function') return;
      listener = onKey;
      document.addEventListener('keydown', listener, true);
    }
    function dispose() {
      retired = true;
      if (listener && host.document) host.document.removeEventListener('keydown', listener, true);
      listener = null;
    }
    return Object.freeze({ keysOf, track, selectedByLayer, removeSelected, nudgeSelected, commitDrag, install, dispose, onKey });
  }

  const bindings = new WeakMap();
  /* The binding of `host`'s native project, or null when none is active (the caller keeps its legacy path). `comp` is the
   * native compositing adapter of the same project; a changed adapter (another project) gets a fresh binding. */
  function desktop(host, comp) {
    if (!host || !comp) return null;
    const held = bindings.get(host);
    if (held && held.comp === comp) return held.grid;
    if (held) held.grid.dispose();
    const grid = create(host, comp);
    grid.install();
    bindings.set(host, { comp, grid });
    return grid;
  }
  return Object.freeze({ create, desktop, actionForEvent, accepts, editable, PROP });
}));
