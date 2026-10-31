/* Onion skin and Ghost All on a native object project (#2358 GHOST-UNDER): the baseline's onion state (range, opacities, mode, tints, shift,
 * Ghost All) selected for the NATIVE viewport request. The native renderer composes the ghost frames UNDER the live frame (baseline stack order),
 * vector, text and raster ghosts alike, in the preview only and never in an export; this file draws nothing and reads no geometry.
 * `selection` is the pure view of the settings a preview request carries (it rides the same view port as the display filter, so the
 * presented frame is re-requested whenever it changes); the controller only notices a changed selection of the ACTIVE native session and
 * asks it to re-present the current frame, and says once per layer and reason when the renderer reports it could not compose the ghosts.
 * It never writes, selects or touches history, and never reaches Paper.js or a legacy writer. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else { root.NemoNativeObjectGhostUI = api; if (root.document && root.addEventListener) api.desktop(root).install(); }
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const TINTS = { prev: '#ff4d4d', next: '#4d8cff' };
  const POLL_MS = 150;
  const RETRY_MS = 1500;
  const hex6 = (v) => typeof v === 'string' && /^#[0-9a-fA-F]{6}$/.test(v);
  const num = (v, d) => (Number.isFinite(Number(v)) ? Number(v) : d);

  function store(win, key) { try { return win.localStorage.getItem(key); } catch (_) { return null; } }
  // The baseline's own preferences (tweens.js onionTint / onionShift): a local view aid, never in the project.
  function tintOf(win, which) { const v = store(win, 'nemo-onion-tint-' + which); return hex6(v) ? v : TINTS[which]; }
  function shiftOf(win) {
    try {
      const o = JSON.parse(store(win, 'nemo-onion-shift') || 'null');
      return o && (Number(o.x) || Number(o.y)) ? { x: Number(o.x) || 0, y: Number(o.y) || 0, next: !!o.next } : null;
    } catch (_) { return null; }
  }
  // What the preview request carries, or null when nothing is shown (both toggles off, playing, no active layer). It does not depend on the
  // frame: the renderer holds the range around the presented frame, so a frame change alone never changes the selection.
  function selection(win, info) {
    const st = win && win.state;
    if (!st || st.playing || !(st.onionSkin || st.ghostAllFrames) || !info || !Array.isArray(info.layers)) return null;
    const layer = info.layers[st.activeLayerIdx];
    if (!layer || typeof layer.layerUid !== 'string' || !layer.layerUid) return null;
    const pct = (v, d) => Math.max(0, Math.min(100, num(v, d)));
    const out = { layerUid: layer.layerUid, onion: !!st.onionSkin, ghostAll: !!st.ghostAllFrames,
      from: Math.max(0, Math.floor(num(st.onionIn, 0))), to: Math.max(0, Math.floor(num(st.onionOut, 0))),
      previousOpacity: pct(st.onionPrevOpacity, 30), nextOpacity: pct(st.onionNextOpacity, 30),
      mode: ['default', 'tinted', 'outline'].includes(st.onionMode) ? st.onionMode : 'tinted' };
    if (tintOf(win, 'prev') !== TINTS.prev) out.previousTint = tintOf(win, 'prev');
    if (tintOf(win, 'next') !== TINTS.next) out.nextTint = tintOf(win, 'next');
    const shift = shiftOf(win);
    if (shift) out.shift = shift;
    return out;
  }

  function create(win) {
    let timer = null, installed = false, last = 'null', retryAt = 0;
    const warned = new Set();
    function nowMs() { return win.performance && typeof win.performance.now === 'function' ? win.performance.now() : Date.now(); }
    function session() {
      try {
        const module = win.NemoNativeObjectProject, s = module && typeof module.desktop === 'function' ? module.desktop(win) : null;
        return s && s.blocksLegacy() && s.isActive() ? s : null;
      } catch (_) { return null; }
    }
    function toast(message) { try { if (win.showToast) win.showToast(message, 'warn'); } catch (_) { /* presentation only */ } }
    // The renderer says what it did with the selection; a refusal is said once per layer and reason, a stale or empty answer never.
    function notice(s) {
      let ghosts;
      try { const shown = s.readPresentedObjectSession(); ghosts = shown && shown.presentation && shown.presentation.ghosts; } catch (_) { return; }
      if (!ghosts || ghosts.status !== 'unavailable') return;
      const reason = ghosts.reason || 'unavailable', key = ghosts.selected.layerUid + ':' + reason;
      if (reason === 'stale_revision' || warned.has(key)) return;
      warned.add(key);
      toast('Onion skin is unavailable for this layer (' + reason + ').');
    }
    // A changed selection of the active session re-presents the current frame through the session's own view port.
    function check() {
      const s = session();
      if (!s) { last = 'null'; return; }
      let key;
      try { key = JSON.stringify(selection(win, s.info())); } catch (_) { return; }
      if (key !== last && nowMs() >= retryAt) {
        last = key;
        const fail = () => { last = 'retry'; retryAt = nowMs() + RETRY_MS; };
        try { const p = s.renderPreview(win.state.currentFrame); if (p && typeof p.catch === 'function') p.catch(fail); } catch (_) { fail(); }
      }
      notice(s);
    }
    function tick() { timer = null; if (!installed) return; try { check(); } finally { schedule(); } }
    function schedule() { if (installed && timer === null && win.setTimeout) timer = win.setTimeout(tick, POLL_MS); }
    function install() { if (installed) return api; installed = true; schedule(); return api; }
    function dispose() { installed = false; if (timer !== null && win.clearTimeout) win.clearTimeout(timer); timer = null; last = 'null'; }
    const api = Object.freeze({ install, check, dispose, selection: (info) => selection(win, info) });
    return api;
  }
  const controllers = new WeakMap();
  function desktop(win) {
    if (!controllers.has(win)) controllers.set(win, create(win));
    return controllers.get(win);
  }
  return Object.freeze({ create, desktop, selection });
}));
