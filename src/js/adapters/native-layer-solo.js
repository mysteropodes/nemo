/* #2371 U11: Layer Solo (the "S" button of a layer row, the folder row included) over a NATIVE object project.
 * Baseline 1065e9d8 (timeline.js toggleLayerSolo and the .solo-btn of renderLayerList, app.js layerIsEffectivelyVisible):
 * while ANY layer is soloed only the soloed layers show (the children of a soloed folder layer count as soloed), the layer eye is
 * ignored, and the flag is a session view fact that no project file ever stores. Natively that is exactly what this file is:
 * it keeps the set of soloed layer uids per project lifecycle (a different instance, document or lifecycle starts empty, so a
 * closed project never leaks into the next one), the preview request carries it as `soloLayers` (native-object-project-desktop.js
 * `soloLayers` port -> Exposure -> nemo_native_object_preview) and the PNG/SVG export request carries the same set, as the
 * baseline export reads solo too. A toggle is no command: no document revision, history entry or dirty mark moves; it only asks
 * the session for one more preview. If the renderer refuses the Solo view, the set goes back and one notice says so.
 * The cell is the baseline `.lico.solo-btn` ("S", on/off) with the baseline title. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeLayerSolo = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const MAX_LAYERS = 512;
  const ID = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;
  const states = new WeakMap();
  const tr = (host, key, fallback) => {
    let v = null;
    try { v = host.SM && typeof host.SM.t === 'function' ? host.SM.t(key) : null; } catch (_) { v = null; }
    return typeof v === 'string' && v.length && v !== key ? v : fallback;
  };
  const toast = (host, text) => { try { if (typeof host.showToast === 'function') host.showToast(text); } catch (_) { /* a notice failure must not break the view */ } };
  const identity = (info) => JSON.stringify([info.instanceId, info.documentId, info.lifecycleGeneration]);
  const usable = (info) => !!info && typeof info === 'object' && typeof info.instanceId === 'string' && typeof info.documentId === 'string';

  // The set of the project lifecycle `info` names; a different lifecycle replaces it with an empty one.
  function held(host, info) {
    const key = identity(info);
    let state = states.get(host);
    if (!state || state.key !== key) { state = { key, set: new Set() }; states.set(host, state); }
    return state;
  }
  // The soloed layer uids as the request carries them: sorted, unique, bounded, plain ids. [] for no native project.
  function read(host, info) {
    if (!usable(info)) return [];
    const state = states.get(host);
    if (!state || state.key !== identity(info)) return [];
    return Array.from(state.set).filter((uid) => ID.test(uid)).sort().slice(0, MAX_LAYERS);
  }
  const has = (host, info, uid) => read(host, info).indexOf(uid) >= 0;

  function sessionOf(host) {
    const project = host.NemoNativeObjectProject;
    try { return project && typeof project.desktop === 'function' ? project.desktop(host) : null; } catch (_) { return null; }
  }
  const activeInfo = (session) => {
    try { return session && typeof session.isActive === 'function' && session.isActive() ? session.info() : null; } catch (_) { return null; }
  };
  // The cell of every row of the layer on screen, for the on/off class.
  function paint(host, uid, on) {
    const list = host.document && host.document.getElementById ? host.document.getElementById('layer-list') : null;
    Array.from(list ? list.children : []).forEach((row) => {
      if (!row.dataset || row.dataset.layerUid !== uid) return;
      Array.from(row.children).forEach((child) => {
        if (child.dataset && child.dataset.nativeLayerSolo === 'true') {
          child.className = 'lico solo-btn ' + (on ? 'on' : 'off');
          child.dataset.soloOn = String(on);
        }
      });
    });
  }
  // One more preview at the playhead, strictly: a refusal of the Solo view is observable on the presenting call only.
  function presentAgain(host, session, onRefused) {
    try {
      if (typeof session.presentPreview !== 'function') return Promise.resolve(false);
      return Promise.resolve(session.presentPreview(host.state && host.state.currentFrame || 0, true)).then(() => true, (error) => { onRefused(error); return false; });
    } catch (error) { onRefused(error); return Promise.resolve(false); }
  }

  // Flip the solo of one layer (the baseline toggleLayerSolo). Returns a promise of whether the view now shows the new state;
  // false with no change when no native project is presented, so the caller keeps its own path.
  function toggle(host, uid) {
    const session = sessionOf(host), info = activeInfo(session);
    if (!info || typeof uid !== 'string' || !ID.test(uid)) return Promise.resolve(false);
    const state = held(host, info), was = state.set.has(uid);
    if (!was && state.set.size >= MAX_LAYERS) { toast(host, tr(host, 'toastSoloTooMany', 'Too many soloed layers.')); return Promise.resolve(false); }
    if (was) state.set.delete(uid); else state.set.add(uid);
    paint(host, uid, !was);
    const revert = () => {
      const current = states.get(host);
      if (current && current.key === state.key) { if (was) current.set.add(uid); else current.set.delete(uid); }
      paint(host, uid, was);
      toast(host, tr(host, 'toastSoloRefused', 'Solo could not be shown.'));
      presentAgain(host, session, () => {});
    };
    return presentAgain(host, session, revert);
  }

  // The row cell (the presenter's soloCell hook): the baseline S button, lit while this layer is soloed.
  function cell(host, info, layer) {
    if (!layer || typeof layer.layerUid !== 'string') return null;
    const node = host.document.createElement('div'), on = has(host, info, layer.layerUid);
    node.className = 'lico solo-btn ' + (on ? 'on' : 'off');
    node.title = tr(host, 'layerSoloTitle', 'Solo layer (hide all others)');
    node.textContent = 'S';
    node.dataset.nativeLayerSolo = 'true';
    node.dataset.soloOn = String(on);
    node.addEventListener('click', (event) => {
      if (event && typeof event.stopPropagation === 'function') event.stopPropagation();
      return toggle(host, layer.layerUid);
    });
    return node;
  }
  // Forget every solo of this host (a closed project): the next read starts empty.
  function clear(host) { states.delete(host); }
  return Object.freeze({ read, has, toggle, cell, clear });
}));
