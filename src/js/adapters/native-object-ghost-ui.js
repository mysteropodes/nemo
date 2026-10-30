/* Onion skin and Ghost All on a native object project (#2358 D23): the neighbour frames of the active layer drawn behind the user's eye as
 * tinted, faded outlines of the baseline (tweens.js renderOS / renderGhostAll), without `state.layers`, Paper.js or any writer.
 * The facts are ONE bounded native read (`readGhosts`, the ghosts view of query.document.object): the stored vector geometry of each ghost
 * frame, its opacity falloff, tint and shift policy decided by the engine from the retained ghost query. The read is pinned to the actual
 * presentation (revision, snapshot, work, view generation, frame); a result that lands after any of those moved, or after the onion
 * settings changed, is discarded, never shown. This file only reads and paints: it never writes, selects or touches history.
 * The view preferences (onion toggle, range, opacities, mode, Ghost All) stay in `state`, the tints and shift in the baseline's own
 * localStorage keys. Viewport compositing of ghosts inside the native renderer is not available: the ghosts are an SVG overlay of the
 * viewport, below the select / rig / widget overlays and never part of an export. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else { root.NemoNativeObjectGhostUI = api; if (root.document && root.addEventListener) api.desktop(root).install(); }
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const SVG = 'http://www.w3.org/2000/svg';
  const PIN = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  const TINTS = { prev: '#ff4d4d', next: '#4d8cff' };
  const samePin = (a, b) => !!a && !!b && PIN.every((k) => a[k] === b[k]);
  const hex6 = (v) => typeof v === 'string' && /^#[0-9a-fA-F]{6}$/.test(v);

  function session(win) {
    try {
      const module = win.NemoNativeObjectProject, s = module && typeof module.desktop === 'function' ? module.desktop(win) : null;
      return s && s.blocksLegacy() ? s : null;
    } catch (_) { return null; }
  }
  function store(win, key) { try { return win.localStorage.getItem(key); } catch (_) { return null; } }
  // The baseline's own preferences (tweens.js onionTint / onionShift): a local view aid, never in the project.
  function tintOf(win, which) { const v = store(win, 'nemo-onion-tint-' + which); return hex6(v) ? v : TINTS[which]; }
  function shiftOf(win) {
    try {
      const o = JSON.parse(store(win, 'nemo-onion-shift') || 'null');
      return o && (Number(o.x) || Number(o.y)) ? { x: Number(o.x) || 0, y: Number(o.y) || 0, next: !!o.next } : null;
    } catch (_) { return null; }
  }
  // What to ask for, or null when nothing is shown (both toggles off, playing, no native presentation at this frame).
  function selectorNow(win) {
    const st = win.state;
    if (!st || st.playing || !(st.onionSkin || st.ghostAllFrames)) return null;
    const s = session(win);
    if (!s || !s.isActive() || typeof s.readGhosts !== 'function') return null;
    try {
      const info = s.info(), shown = s.readPresentedObjectSession(), p = shown && shown.presentation, layer = info && info.layers && info.layers[st.activeLayerIdx];
      const total = info && info.totalFrames || st.totalFrames;
      if (!info || !layer || !p || shown.available !== true || p.owner !== 'native' || p.status !== 'presented' || p.contextId !== 'scene-root' ||
        p.frame !== st.currentFrame || !samePin(p, info) || !samePin(shown, info) || !Number.isSafeInteger(total) || total < 1 || p.frame >= total) return null;
      const num = (v, d) => (Number.isFinite(Number(v)) ? Number(v) : d), frame = p.frame;
      const selector = { contextId: 'scene-root', frame, layerUid: layer.layerUid, onion: !!st.onionSkin, ghostAll: !!st.ghostAllFrames,
        from: Math.max(0, Math.min(frame, Math.floor(num(st.onionIn, 0)))), to: Math.min(total - 1, Math.max(frame, Math.floor(num(st.onionOut, frame)))),
        previousOpacity: Math.max(0, Math.min(100, num(st.onionPrevOpacity, 30))), nextOpacity: Math.max(0, Math.min(100, num(st.onionNextOpacity, 30))),
        mode: ['default', 'tinted', 'outline'].includes(st.onionMode) ? st.onionMode : 'tinted' };
      if (tintOf(win, 'prev') !== TINTS.prev) selector.previousTint = tintOf(win, 'prev');
      if (tintOf(win, 'next') !== TINTS.next) selector.nextTint = tintOf(win, 'next');
      const shift = shiftOf(win);
      if (shift) selector.shift = shift;
      const ticket = s.readSubscriptionGeneration();
      // The pin the answer must still match when it lands: the same session, subscription, presentation object and playhead.
      const current = () => {
        try {
          const live = session(win), now = live && live.readPresentedObjectSession();
          return live === s && live.isActive() && !win.state.playing && win.state.currentFrame === frame && win.state.activeLayerIdx === st.activeLayerIdx &&
            samePin(live.info(), info) && live.readSubscriptionGeneration() === ticket && !!now && now.presentation === p;
        } catch (_) { return false; }
      };
      return { session: s, info, presentation: p, selector, current, key: JSON.stringify([info.instanceId, info.documentId, info.lifecycleGeneration,
        info.contentRevision, info.documentSnapshotId, p.workId, p.viewGeneration, selector]) };
    } catch (_) { return null; }
  }
  function mappingFor(win, entry) {
    const m = win.NemoNativeViewportUnderlay && win.NemoNativeViewportUnderlay.canvasMapping;
    if (!m || !m.bounds || !m.screen || !m.matchesView || !m.matchesView(win, entry.presentation)) return null;
    try {
      const bounds = m.bounds(win, entry.info);
      return bounds ? { api: m, bounds, presentation: entry.presentation, canvasW: entry.info.canvasW, canvasH: entry.info.canvasH } : null;
    } catch (_) { return null; }
  }
  const num = (n) => Math.round(n * 100) / 100;
  // Stored segments are `[x, y]` or `[x, y, inX, inY, outX, outY]` with handles relative to the point (Paper.js), as in the document.
  function pathData(win, map, path, shift) {
    const at = (x, y) => { const p = map.api.screen({ x: x + shift[0], y: y + shift[1] }, map); return [num(p.x), num(p.y)]; };
    const segs = path.segments.map((s) => ({ p: [s[0], s[1]], i: s.length === 6 ? [s[2], s[3]] : [0, 0], o: s.length === 6 ? [s[4], s[5]] : [0, 0] }));
    const edge = (a, b) => {
      const c1 = at(a.p[0] + a.o[0], a.p[1] + a.o[1]), c2 = at(b.p[0] + b.i[0], b.p[1] + b.i[1]), e = at(b.p[0], b.p[1]);
      return ' C' + c1[0] + ' ' + c1[1] + ' ' + c2[0] + ' ' + c2[1] + ' ' + e[0] + ' ' + e[1];
    };
    const start = at(segs[0].p[0], segs[0].p[1]);
    let d = 'M' + start[0] + ' ' + start[1];
    for (let n = 1; n < segs.length; n++) d += edge(segs[n - 1], segs[n]);
    if (path.closed && segs.length > 1) d += edge(segs[segs.length - 1], segs[0]) + 'Z';
    return d;
  }

  function create(win) {
    let overlay = null, lastSig = null, shown = null, pending = null, serial = 0, paintQueued = false, queue = Promise.resolve(), timer = null, frameTick = false, installed = false;
    const warned = new Set();
    function toast(message) { try { if (win.showToast) win.showToast(message, 'warn'); } catch (_) { /* presentation only */ } }
    function node(name, attrs) {
      const n = win.document.createElementNS(SVG, name);
      Object.keys(attrs).forEach((k) => n.setAttribute(k, attrs[k]));
      return n;
    }
    function ensureOverlay() {
      if (overlay && overlay.isConnected) return overlay;
      overlay = node('svg', { 'aria-hidden': 'true', 'data-nemo-ghosts': '' });
      Object.assign(overlay.style, { position: 'fixed', left: '0', top: '0', width: '100vw', height: '100vh', overflow: 'visible', pointerEvents: 'none', zIndex: '800' });
      (win.document.body || win.document.documentElement).appendChild(overlay);
      return overlay;
    }
    function clearOverlay() { lastSig = null; if (overlay) while (overlay.firstChild) overlay.removeChild(overlay.firstChild); }
    function draw(known) {
      paintQueued = false;
      const entry = known || selectorNow(win), read = shown;
      // A stored result is shown only while its own request is still the live one (frame, revision, presentation and settings unchanged).
      if (!read || !entry || entry.key !== read.key || !read.entry.current()) { clearOverlay(); return; }
      const map = mappingFor(win, entry);
      if (!map) { clearOverlay(); return; }
      // Pan, zoom or a resized viewport repaint; an unchanged picture is not rebuilt.
      const sig = read.key + '|' + [map.bounds.left, map.bounds.top, map.bounds.width, map.bounds.height].join(',') + '|' + JSON.stringify(entry.presentation.view);
      if (sig === lastSig && overlay && overlay.firstChild) return;
      const svg = ensureOverlay(), scale = Math.abs(map.api.screen({ x: 1, y: 0 }, map).x - map.api.screen({ x: 0, y: 0 }, map).x) || 1;
      clearOverlay();
      const clip = node('clipPath', { id: 'nemo-ghost-clip' });
      clip.appendChild(node('rect', { x: map.bounds.left, y: map.bounds.top, width: map.bounds.width, height: map.bounds.height }));
      const defs = node('defs', {});
      defs.appendChild(clip);
      svg.appendChild(defs);
      const group = node('g', { 'clip-path': 'url(#nemo-ghost-clip)' });
      read.value.frames.forEach((frame) => {
        frame.paths.forEach((path) => {
          if (!path.segments.length) return;
          const attrs = { d: pathData(win, map, path, frame.translation), fill: path.fill || 'none', stroke: path.stroke || 'none', opacity: path.opacity,
            'stroke-width': num(Math.max(0.5, path.width * scale)), 'stroke-linecap': 'round', 'stroke-linejoin': 'round' };
          group.appendChild(node('path', attrs));
        });
      });
      svg.appendChild(group);
      lastSig = sig;
    }
    function paint() {
      if (paintQueued) return;
      paintQueued = true;
      (win.requestAnimationFrame || ((fn) => fn()))(() => draw());
    }
    function failure(entry, error) {
      shown = null;
      const reason = (error && (error.reason || error.code)) || 'unavailable', key = entry.selector.layerUid + ':' + reason;
      // A stale answer is not a failure: the next tick asks again. Anything else is said once per layer and reason.
      if ((error && error.code) !== 'stale_revision' && reason !== 'ghost_view_unavailable' && !warned.has(key)) {
        warned.add(key);
        toast('Onion skin is unavailable for this layer (' + reason + ').');
      }
    }
    // One read at a time, keyed by pin + presentation + settings: an unchanged key costs a comparison, a changed one supersedes the pending read.
    function refresh(known) {
      const entry = known || selectorNow(win);
      if (!entry) { if (pending || shown) { serial++; pending = null; shown = null; } paint(); return Promise.resolve(null); }
      if (shown && shown.key === entry.key && shown.entry.current()) { draw(entry); return Promise.resolve(shown.value); }
      if (pending && pending.key === entry.key && entry.current()) return pending.promise;
      serial++;
      const token = serial;
      const job = { key: entry.key, promise: null };
      pending = job;
      job.promise = queue.catch(() => {}).then(() => (token === serial && entry.current() ? entry.session.readGhosts(entry.selector, entry.info) : null))
        .then((value) => {
          const now = token === serial && value && entry.current() ? selectorNow(win) : null;
          if (!now || now.key !== entry.key || !samePin(value.info, entry.info)) return null; // stale, superseded or settings moved: discard
          shown = { key: entry.key, entry, value };
          paint();
          return value;
        }).catch((error) => { if (token === serial) failure(entry, error); paint(); return null; })
        .finally(() => { if (pending === job) pending = null; });
      queue = job.promise.catch(() => {});
      return job.promise;
    }
    // While a ghost layer is wanted an animation frame at a time follows the playhead and the revision; otherwise a slow timer notices a toggle.
    function tick() {
      timer = null; frameTick = false;
      if (!installed) return;
      refresh(selectorNow(win));
      const st = win.state;
      if (st && (st.onionSkin || st.ghostAllFrames) && win.requestAnimationFrame) { frameTick = true; win.requestAnimationFrame(tick); } else schedule();
    }
    function schedule() { if (!installed || timer !== null || frameTick) return; timer = win.setTimeout ? win.setTimeout(tick, 300) : null; }
    function install() { if (installed) return api; installed = true; schedule(); return api; }
    function dispose() { installed = false; serial++; shown = null; pending = null; if (timer !== null && win.clearTimeout) win.clearTimeout(timer); timer = null; clearOverlay(); }
    const api = Object.freeze({ install, refresh, dispose, shown: () => (shown ? shown.value : null) });
    return api;
  }
  const controllers = new WeakMap();
  function desktop(win) {
    if (!controllers.has(win)) controllers.set(win, create(win));
    return controllers.get(win);
  }
  return Object.freeze({ create, desktop, selectorNow, pathData });
}));
