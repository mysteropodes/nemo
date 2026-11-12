/* #2524 / #2377: Motion key copy and paste on a native project. Baseline: motion.js copySelectedKeys / pasteKeys and the key branch of
 * timeline.js onKeyDown (@1065e9d8). Cmd+C records the selected keys (property, offset from the earliest key, value, hold, curve points
 * and the holder they came from); Cmd+V puts them at the playhead, overwriting a key already there. The clipboard holds only that packet
 * and reads only the native key projection; the paste is ONE `object.property.keys.paste` through the supplied port (one revision, one
 * history entry). A source layer that no longer exists falls back to the active layer, as the baseline did. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeMotionKeyClipboard = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const MAX_KEYS = 512, MAX_TRACKS = 128, MAX_BYTES = 65536 - 1024; // the paste request bound, with the envelope's share kept back
  const MAX_OPEN = 12; // drawers brought back after a paste, so the new keys show without opening a hundred rows
  const SKIPPED = ['timeRemap', 'order']; // other owners (the placement remap lane, the layer order track)
  const clone = (v) => JSON.parse(JSON.stringify(v));
  let clip = null;
  const targetKey = (t) => JSON.stringify([t.contextId, t.layerUid, t.strokeId === undefined ? null : t.strokeId]);
  /* The packet of the selected keys of `view` ({tracks:[{target, property, items:[{frame, selected, v, hold, curvePoints}]}]}), or null. */
  function build(view) {
    const picked = [];
    let base = Infinity, count = 0;
    ((view && view.tracks) || []).forEach((track) => {
      if (SKIPPED.includes(track.property) || track.inert) return;
      const keys = track.items.filter((k) => k.selected && Array.isArray(k.v));
      if (!keys.length) return;
      keys.forEach((k) => { base = Math.min(base, k.frame); });
      count += keys.length;
      picked.push({ track, keys });
    });
    if (!count) return null;
    const tracks = picked.map(({ track, keys }) => ({
      target: clone(track.target), property: track.property,
      keys: keys.map((k) => {
        const out = { dt: k.frame - base, v: k.v.slice() };
        if (k.hold === true) out.hold = true;
        if (Array.isArray(k.curvePoints) && k.curvePoints.length) out.curvePoints = clone(k.curvePoints);
        return out;
      })
    }));
    return { tracks, count };
  }
  /* The paste payload at `frame`. `layers` (uids present) and `active` (the active layer target) give the fallback of a source layer that is gone. */
  function payload(packet, frame, layers, active) {
    const present = layers ? new Set(layers) : null;
    const merged = new Map();
    packet.tracks.forEach((t) => {
      let target = t.target;
      if (present && !present.has(target.layerUid) && active) target = { contextId: active.contextId, layerUid: active.layerUid };
      const id = JSON.stringify([targetKey(target), t.property]);
      const into = merged.get(id) || { target: clone(target), property: t.property, keys: [] };
      t.keys.forEach((k) => into.keys.push(clone(k)));
      merged.set(id, into);
    });
    return { command: 'object.property.keys.paste', frame, tracks: Array.from(merged.values()) };
  }
  const bytes = (text) => new TextEncoder().encode(text).length;
  /* The clipboard of one Motion key editor. Ports: readKeyTimelines() -> view|null, context() -> {frame, layers (uids), active (target)},
   * pasteKeys(payload) -> Promise<{applied}>, reopen(target, property, frames), notify(message), translate(key, fallback). `copy` and `paste` resolve true (done), false (refused,
   * with a notice) or null (nothing to do). */
  function create(ports) {
    let busy = false;
    const say = (key, fallback) => { try { ports.notify(ports.translate ? ports.translate(key, fallback) : fallback); } catch (_) { /* a notice must not break the gesture */ } };
    async function copy() {
      const view = ports.readKeyTimelines();
      const packet = view ? build(view) : null;
      if (!packet) return null;
      if (packet.count > MAX_KEYS || packet.tracks.length > MAX_TRACKS || bytes(JSON.stringify(packet)) > MAX_BYTES) { say('toastSelectionTooLarge', 'The selection spans too many properties for one edit.'); return false; }
      clip = packet;
      return true;
    }
    async function paste() {
      if (!clip) return null;
      if (busy) return false;
      const view = ports.readKeyTimelines();
      const opened = ((view && view.tracks) || []).map((t) => ({ target: t.target, property: t.property, frames: t.items.filter((k) => k.selected).map((k) => k.frame) }));
      const packet = clip;
      let applied = false, ok = true;
      busy = true;
      try {
        const where = ports.context();
        const body = payload(packet, where.frame, where.layers, where.active);
        if (bytes(JSON.stringify(body)) > MAX_BYTES) { say('toastSelectionTooLarge', 'The selection spans too many properties for one edit.'); return false; }
        const result = await ports.pasteKeys(body);
        applied = !!result && result.applied === true;
      } catch (_) {
        ok = false;
        say('toastKeyPasteRefused', 'The keys could not be pasted there.');
      } finally {
        // The refresh after a write closes the key drawers: bring the open ones back, then the pasted tracks (a few) so the new keys show.
        try {
          const seen = new Set(opened.map((t) => JSON.stringify([targetKey(t.target), t.property])));
          const extra = [];
          if (applied) packet.tracks.forEach((t) => { const id = JSON.stringify([targetKey(t.target), t.property]); if (!seen.has(id)) { seen.add(id); extra.push({ target: t.target, property: t.property, frames: [] }); } });
          for (const t of opened.concat(extra).slice(0, MAX_OPEN)) await ports.reopen(t.target, t.property, t.frames);
        } catch (_) { /* the keys can be read again from the property row */ }
        busy = false;
      }
      return ok;
    }
    return Object.freeze({ copy, paste });
  }
  return Object.freeze({ build, payload, create, has: () => !!clip, clear: () => { clip = null; }, MAX_KEYS, MAX_TRACKS });
}));
