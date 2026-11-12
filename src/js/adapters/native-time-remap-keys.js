/* #2377 / #2524: the Time Remap keys of a native video placement as selectable timeline keys. Baseline: the Time Remap row of motion.js is an
 * ordinary key track (ld.timeRemap): its keys are selected, dragged, nudged, deleted and tinted with the same gestures as any property key
 * (deleteSelectedKeys, onDragMove, colorSelectedKeys). Natively those keys live in the video placement, so this module adds them to the key
 * read of the range grid as one more track per remapped layer (`merge`), keeps their selection locally, and sends every edit as ONE
 * `editTimeRemapKeys` placement edit through SM.timeRemapKeysIntent (native-time-remap-ui.js): one revision, one history entry.
 * The two selections are exclusive: selecting remap keys clears the property keys and the reverse, so a gesture never spans two owners.
 * A remap selection names the keys of ONE layer (one placement per edit). Nothing here holds the document. */
(function (root, factory) {
  // Resolved when a gesture needs it, so the script load order of the two modules does not matter.
  const api = factory(() => (typeof module === 'object' && module.exports ? require('./native-motion-key-edit.js') : root.NemoNativeMotionKeyEdit));
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeTimeRemapKeys = api;
}(typeof globalThis === 'object' ? globalThis : this, function (keyEdit) {
  'use strict';
  const PROP = 'timeRemap';
  const MAX_SINGLES = 32, MAX_BYTES = 3500; // single-key addresses per edit; the payload share of the 4 KiB placement request
  const SURFACES = ['drawing-canvas', 'canvas-area', 'canvas-col', 'layer-list', 'frame-grid', 'timeline-area', 'props-panel'];
  const states = new WeakMap();
  function stateOf(host) {
    let s = states.get(host);
    if (!s) { s = { uid: null, frames: new Set(), version: 0, busy: false, listener: null, current: null }; states.set(host, s); }
    return s;
  }
  const isRemap = (a) => !!a && !!a.target && a.target.property === PROP && !!a.target.target && typeof a.target.target.layerUid === 'string';
  function clear(s) { if (s.frames.size || s.uid !== null) { s.uid = null; s.frames = new Set(); s.version++; } }
  const say = (host, key, fallback) => {
    try { const t = host.SM && host.SM.t; const v = typeof t === 'function' ? t(key) : null; if (typeof host.showToast === 'function') host.showToast(typeof v === 'string' && v && v !== key ? v : fallback); } catch (_) { /* a notice must not break the edit */ }
  };
  /* The lanes: one track per layer of `info` whose placement lists remap keys, items {frame, selected, v, color?, hold?}. */
  function lanes(host, info) {
    const facts = host.SM && host.SM.timeRemapKeyFacts;
    if (typeof facts !== 'function' || !info || !Array.isArray(info.layers)) return [];
    const s = stateOf(host), out = [];
    info.layers.forEach((layer, i) => {
      const r = facts(i), keys = r && Array.isArray(r.keys) ? r.keys : [];
      if (!keys.length) return;
      out.push({ target: { contextId: 'scene-root', layerUid: layer.layerUid }, property: PROP, remap: true, items: keys.map((k) => {
        const item = { frame: k.frame, selected: s.uid === layer.layerUid && s.frames.has(k.frame), v: [k.value] };
        if (k.color) item.color = k.color;
        if (k.hold === true) item.hold = true;
        return item;
      }) });
    });
    // A selection on a layer that no longer has a remap lane (remap turned off, layer gone) is dropped.
    if (s.uid !== null && !out.some((t) => t.target.layerUid === s.uid)) clear(s);
    return out;
  }
  /* `real` (the property key read, or null) with the remap lanes added. The returned read selects both kinds: the last key of a selection
   * decides which kind wins, and the other kind is cleared. */
  function merge(host, info, real) {
    if (!host.state || host.state.appMode !== 'motion') return real;
    const s = stateOf(host), extra = lanes(host, info);
    s.current = info;
    if (!extra.length) return real;
    // The property keys were selected after the remap keys (outside this read, e.g. from the property panel): they win.
    if (real && s.frames.size && real.tracks.some((t) => t.items.some((k) => k.selected))) { clear(s); extra.forEach((t) => t.items.forEach((k) => { k.selected = false; })); }
    const version = s.version;
    const view = {
      pin: real ? real.pin : info, frame: host.state.currentFrame, viewGeneration: real ? real.viewGeneration : undefined,
      tracks: (real ? real.tracks : []).concat(extra),
      isCurrent() { return s.version === version && (!real || real.isCurrent() === true); },
      selectKeys(keys) {
        if (!Array.isArray(keys)) return false;
        if (!keys.length) { clear(s); return real ? real.selectKeys([]) === true : true; }
        if (isRemap(keys[keys.length - 1])) {
          const uid = keys[keys.length - 1].target.target.layerUid;
          const chosen = keys.filter((a) => isRemap(a) && a.target.target.layerUid === uid).map((a) => a.frame);
          const lane = extra.find((t) => t.target.layerUid === uid);
          if (!lane || !chosen.every((f) => lane.items.some((k) => k.frame === f))) return false;
          if (real && real.selectKeys([]) !== true) return false;
          s.uid = uid; s.frames = new Set(chosen); s.version++;
          return true;
        }
        clear(s);
        return real ? real.selectKeys(keys.filter((a) => !isRemap(a))) === true : false;
      }
    };
    return view;
  }
  const runsOf = (lane) => keyEdit().rangesOf(lane).ranges;
  /* One edit of the selected remap keys. Resolves true/false, or null when no remap key is selected. `opts.expected` says where the frames land. */
  async function edit(host, action, opts) {
    const s = stateOf(host);
    if (!s.frames.size || !action || action.kind === 'copy' || action.kind === 'paste') return null;
    const intent = host.SM && host.SM.timeRemapKeysIntent, rows = host.state && host.state.layers;
    const li = Array.isArray(rows) ? rows.findIndex((r) => r && r.layerUid === s.uid) : -1;
    if (typeof intent !== 'function' || li < 0) return null;
    if (s.busy) return false;
    const frames = Array.from(s.frames).sort((a, b) => a - b);
    if ((action.kind === 'flip' || action.kind === 'distribute') && frames.length < 2) { say(host, 'toastSelectAtLeast2Keys', 'Select at least 2 keys'); return false; }
    const lane = lanes(host, s.current).find((t) => t.target.layerUid === s.uid);
    if (!lane || !frames.every((f) => lane.items.some((k) => k.frame === f))) { say(host, 'toastKeyEditRefused', 'The key edit was refused.'); return false; }
    const selections = frames.length <= MAX_SINGLES ? frames.map((frame) => ({ frame })) : [{ ranges: runsOf(Object.assign({}, lane, { items: lane.items.map((k) => Object.assign({}, k, { selected: s.frames.has(k.frame) })) })) }];
    if (new TextEncoder().encode(JSON.stringify({ selections, action })).length > MAX_BYTES) { say(host, 'toastSelectionTooLarge', 'The selection spans too many keys for one edit.'); return false; }
    s.busy = true;
    let applied = false;
    try { applied = (await intent(li, selections, action)) === true; } catch (_) { applied = false; } finally { s.busy = false; }
    if (applied) {
      const landed = opts && typeof opts.expected === 'function' ? opts.expected({ target: lane.target, property: PROP, frames }, frames) : keyEdit().expectedFrames(action, frames);
      s.frames = new Set(landed || []); s.version++;
      if (!s.frames.size) s.uid = null;
    }
    // The keys are drawn from the confirmed read; the selection follows them as soon as the lane is redrawn.
    try { const grid = host.NemoNativeLayerRangeDesktop; if (grid && typeof grid.refreshKeys === 'function') grid.refreshKeys(host); } catch (_) { /* the next paint redraws it */ }
    return applied;
  }
  /* The key route of the range grid: remap keys when they are the selection, else the property route `realEdit`. */
  function route(host, realEdit) {
    return function (action, opts) { return stateOf(host).frames.size ? edit(host, action, opts) : realEdit(action, opts); };
  }
  function accepts(event, document) {
    let node = event.target;
    if (!node || node === document.body || node === document.documentElement) return true;
    for (; node; node = node.parentNode) if (SURFACES.includes(node.id)) return true;
    return false;
  }
  /* The keyboard route (Delete, Alt+Left/Right, F9 and the interpolation shortcuts) while remap keys are selected; the property key editor
   * owns the same presses while property keys are. `live()` says whether the grid is mounted for the presented project. */
  function install(host, live) {
    const s = stateOf(host), document = host.document;
    if (!document || typeof document.addEventListener !== 'function' || s.listener) return;
    s.listener = function (event) {
      if (event.defaultPrevented || keyEdit().editable(event.target) || !s.frames.size) return;
      const action = keyEdit().actionForEvent(event);
      if (!action || !host.state || host.state.appMode !== 'motion' || !accepts(event, document)) return;
      if (document.querySelector && document.querySelector('.modal:not(.hid),dialog[open],[aria-modal="true"]')) return;
      let ok = false;
      try { ok = live() === true; } catch (_) { ok = false; }
      if (!ok || !s.current) return;
      event.preventDefault();
      if (event.stopImmediatePropagation) event.stopImmediatePropagation(); else if (event.stopPropagation) event.stopPropagation();
      Promise.resolve(edit(host, action)).catch(() => false);
    };
    document.addEventListener('keydown', s.listener, true);
    // The property key editor must not answer the interpolation shortcuts with "no selection" while remap keys are selected.
    try { s.peer = keyEdit().addPeer((event, doc) => doc === document && s.frames.size > 0); } catch (_) { s.peer = null; }
  }
  function dispose(host) {
    const s = states.get(host);
    if (s && s.listener && host.document) host.document.removeEventListener('keydown', s.listener, true);
    if (s && typeof s.peer === 'function') s.peer();
    states.delete(host);
  }
  return Object.freeze({ PROP, lanes, merge, edit, route, install, dispose, isRemap, selected: (host) => Array.from(stateOf(host).frames) });
}));
