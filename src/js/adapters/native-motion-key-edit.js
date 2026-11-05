/* #2524 / #2377: batch editing of the selected native Motion keys by keyboard. Baseline: motion.js applyEasyEase / setKeyInterp /
 * nudgeSelectedKeys / deleteSelectedKeys and the Motion key layer of timeline.js onKeyDown (@1065e9d8): F9 Easy Ease, Shift+F9 Easy
 * Ease In, Cmd/Ctrl+Shift+F9 Easy Ease Out, Cmd/Ctrl+Alt+K Linear, Cmd/Ctrl+Alt+H Hold, Delete/Backspace, Alt+Left/Right nudge.
 * This module writes nothing itself: every gesture is ONE `object.property.track.edit` through the supplied port (one revision, one
 * history entry), and it bypasses nothing: keys with no selection keep their other meanings and the event is left alone. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeMotionKeyEdit = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const MAX_KEYS = 32; // single-key addresses per request; a larger selection names runs of keys per track instead
  const MAX_BYTES = 4096; // the track-edit request bound
  const isMod = (e) => !!(e.metaKey || e.ctrlKey);
  const letter = (e, l) => String(e.key || '').toLowerCase() === l || e.code === 'Key' + l.toUpperCase();
  function editable(node) {
    for (; node; node = node.parentNode) if (['INPUT', 'SELECT', 'TEXTAREA'].includes(node.tagName) || node.isContentEditable) return true;
    return false;
  }
  /* The one action a key press means, or null. Mirrors the baseline mapping exactly (including that Cmd without Shift still means Ease). */
  function actionForEvent(e) {
    const k = String(e.key || '');
    if (k === 'F9') return { kind: 'preset', preset: e.shiftKey ? (isMod(e) ? 'easeOut' : 'easeIn') : 'ease' };
    if (isMod(e) && e.altKey && letter(e, 'k')) return { kind: 'preset', preset: 'linear' };
    if (isMod(e) && e.altKey && letter(e, 'h')) return { kind: 'preset', preset: 'hold' };
    if (k === 'Delete' || k === 'Backspace') return { kind: 'delete' };
    if (e.altKey && !isMod(e) && (k === 'ArrowLeft' || k === 'ArrowRight')) return { kind: 'shift', delta: k === 'ArrowRight' ? 1 : -1 };
    return null;
  }
  /* Where the selected frames of ONE track land after `action`, mirrored from the engine so the selection can follow the keys. */
  function expectedFrames(action, frames) {
    const sorted = frames.slice().sort((a, b) => a - b);
    if (action.kind === 'shift') return sorted.map((f) => f + action.delta);
    if (action.kind === 'delete') return [];
    if (action.kind === 'distribute' && sorted.length >= 2) {
      const first = sorted[0], step = (sorted[sorted.length - 1] - first) / (sorted.length - 1);
      return sorted.map((_, i) => Math.round(first + step * i));
    }
    return sorted;
  }
  /* Whether every selected key of every track can shift by `by` frames: inside 0..total-1 and not onto a key that stays. */
  function canShift(tracks, by, total) {
    return tracks.every((track) => {
      const moving = new Set(track.items.filter((k) => k.selected).map((k) => k.frame));
      const staying = new Set(track.items.filter((k) => !k.selected).map((k) => k.frame));
      return Array.from(moving).every((f) => { const to = f + by; return to >= 0 && to <= total - 1 && !staying.has(to); });
    });
  }
  /* One track's selected keys as inclusive runs of frames: a run never spans a key that is not selected. */
  function rangesOf(track) {
    const ranges = [];
    let open = null;
    track.items.slice().sort((a, b) => a.frame - b.frame).forEach((key) => {
      if (!key.selected) { open = null; return; }
      if (open) open[1] = key.frame; else { open = [key.frame, key.frame]; ranges.push(open); }
    });
    return { target: track.target, property: track.property, ranges };
  }
  /* The editor over the open key tracks. Ports: readKeyTimelines() -> {pin, tracks:[{target,property,items:[{frame,selected}]}]},
   * localKeys() -> an opaque list that changes whenever the local selection does, editPropertyTrack(payload, pin, isCurrent),
   * reopen(target, property, frames) (the refresh after a write closes the drawers; this restores them and their selection),
   * notify(message), translate(key, fallback). `edit` resolves null when no key is selected, else true/false. */
  function createEditor(ports) {
    let busy = false;
    const say = (key, fallback) => { try { ports.notify(ports.translate ? ports.translate(key, fallback) : fallback); } catch (_) { /* a notice must not break the edit */ } };
    async function edit(action) {
      if (busy) return false;
      const view = ports.readKeyTimelines();
      if (!view) return null;
      const selections = [], opened = [], tracks = [];
      let count = 0;
      view.tracks.forEach((track) => {
        const frames = track.items.filter((k) => k.selected).map((k) => k.frame);
        opened.push({ target: track.target, property: track.property, frames });
        frames.forEach((frame) => selections.push({ target: track.target, property: track.property, frame }));
        if (frames.length) tracks.push(track);
        count += frames.length;
      });
      if (!count) return null;
      if ((action.kind === 'flip' || action.kind === 'distribute') && count < 2) { say('toastSelectAtLeast2Keys', 'Select at least 2 keys'); return false; }
      // Up to 32 keys: one address per key. More: one entry per track naming the runs of consecutive selected keys, so the size of the
      // selection is bounded by the request bytes alone (baseline: no key limit).
      const payload = { command: 'object.property.track.edit', selections: count <= MAX_KEYS ? selections : tracks.map(rangesOf), action };
      if (new TextEncoder().encode(JSON.stringify(payload)).length > MAX_BYTES) { say('toastSelectionTooLarge', 'The selection spans too many properties for one edit.'); return false; }
      const local = JSON.stringify(ports.localKeys());
      let applied = false, ok = true;
      busy = true;
      try {
        const result = await ports.editPropertyTrack(payload, view.pin, () => JSON.stringify(ports.localKeys()) === local);
        applied = !!result && result.applied === true;
      } catch (_) {
        ok = false;
        say(action.kind === 'shift' || action.kind === 'distribute' ? 'toastKeysCannotMoveThere' : 'toastKeyEditRefused',
          action.kind === 'shift' || action.kind === 'distribute' ? 'These keys cannot move there.' : 'The key edit was refused.');
      } finally {
        // The settled refresh closed the drawers whether or not the write landed; bring them back so the keys stay on the timeline.
        try {
          for (const t of opened) await ports.reopen(t.target, t.property, applied ? expectedFrames(action, t.frames) : t.frames);
        } catch (_) { /* the keys can be read again from the property row */ }
        busy = false;
      }
      return ok;
    }
    return Object.freeze({ edit });
  }
  /* The keyboard route. Ports: ownsNativeMotion(), hasSelection(), edit(action), acceptsTarget(event), modalOpen(), unavailable(), noSelection() (optional notice).
   * A capture listener on the document: with no key selected, or off the timeline, the press is left to its other owners. */
  const owners = new WeakMap();
  function install(document, ports) {
    if (!document || !document.addEventListener || !ports || typeof ports.edit !== 'function' || typeof ports.hasSelection !== 'function') throw new TypeError('Native key edit ports unavailable');
    if (owners.has(document)) throw new Error('Native key shortcuts already owned');
    let retired = false;
    function stop(event) { event.preventDefault(); if (event.stopImmediatePropagation) event.stopImmediatePropagation(); else if (event.stopPropagation) event.stopPropagation(); }
    function onKey(event) {
      if (retired || event.defaultPrevented || editable(event.target)) return;
      const action = actionForEvent(event);
      if (!action) return;
      let selected = false;
      try {
        if (ports.ownsNativeMotion() !== true || (ports.modalOpen && ports.modalOpen()) || !ports.acceptsTarget || ports.acceptsTarget(event) !== true) return;
        selected = ports.hasSelection() === true;
      } catch (_) { return; }
      if (!selected) {
        // The interpolation shortcuts mean nothing else, so they say so; Delete and the arrows keep their other owners.
        if (action.kind === 'preset' && ports.noSelection) { stop(event); try { ports.noSelection(); } catch (_) { /* a notice must not break the key */ } }
        return;
      }
      stop(event);
      Promise.resolve(ports.edit(action)).then((r) => { if (r === null && !retired && ports.unavailable) ports.unavailable(); }, () => { if (!retired && ports.unavailable) ports.unavailable(); });
    }
    document.addEventListener('keydown', onKey, true);
    const api = Object.freeze({ dispose() { if (retired) return; retired = true; document.removeEventListener('keydown', onKey, true); if (owners.get(document) === api) owners.delete(document); } });
    owners.set(document, api);
    return api;
  }
  return Object.freeze({ actionForEvent, expectedFrames, canShift, rangesOf, createEditor, install, editable });
}));
