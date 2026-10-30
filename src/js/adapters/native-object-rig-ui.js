/* The ordinary Rig panel on a native object project (#2255): auto-assign, commit pose and reset pose act on the active layer through
 * NemoNativeObjectRig (`object.rig.edit` autoAssign, `object.rig.pose` commit / reset), one native command per click, so the
 * undo/redo and the receipt are the owner's. Nothing here reads `ld.rig` or calls rigAutoAssignLayer / rigCommitFrame / rigResetPose.
 * The Rig tool's canvas gestures (draw a bone, move an anchor or a handle, resize an influence circle, edit a weight, set an IK
 * chain) need the rig presentation read that does not exist yet, and the live pose / IK drag has no native operation by contract:
 * each is a typed unavailable (the gesture is consumed so nothing else draws), never a fall back to the JS rig model. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectRigUI = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const CANVAS_REASON = 'rig_presentation_read_unavailable';
  function session(win) {
    try {
      const module = win.NemoNativeObjectProject, s = module && typeof module.desktop === 'function' ? module.desktop(win) : null;
      return s && s.blocksLegacy() ? s : null;
    } catch (_) { return null; }
  }
  const toast = (win, message) => { try { if (win.showToast) win.showToast(message, 'warn'); } catch (_) { /* presentation only */ } };
  function failure(win, error) {
    toast(win, (error && error.message || 'Native rig is unavailable.') + (error && error.code ? ' (' + error.code + (error.reason ? ': ' + error.reason : '') + ')' : ''));
  }
  function target(win) {
    const s = session(win), info = s && s.info(), layer = info && info.layers && info.layers[win.state.activeLayerIdx];
    return layer && Number.isSafeInteger(win.state.currentFrame) ? { layerUid: layer.layerUid, frame: win.state.currentFrame } : null;
  }
  function form(win) {
    const el = (id) => win.document.getElementById(id) || undefined, value = (id) => (el(id) || {}).value, flag = (id) => !!(el(id) || {}).checked;
    return { radius: value('rig-weight-radius'), rotate: flag('rig-rotate-mode'), softness: value('rig-falloff-softness'),
      driveShapes: el('rig-drive-shapes'), driveMeshes: el('rig-drive-meshes') };
  }
  // The panel buttons. True when the native route took the click (the legacy handler must then return), false on a legacy project.
  function panel(win, action) {
    if (!session(win)) return false;
    const module = win.NemoNativeObjectRig, api = module && typeof module.desktop === 'function' ? module.desktop(win) : null, at = target(win);
    if (!api || !at) { toast(win, 'Native rig is unavailable.'); return true; }
    const run = action === 'autoAssign' ? () => api.autoAssign(at.layerUid, at.frame, module.autoAssignArgs(form(win)))
      : action === 'commit' ? () => api.commitPose(at.layerUid, at.frame) : action === 'reset' ? () => api.resetPose(at.layerUid) : null;
    if (!run) { toast(win, 'Native rig action is unavailable.'); return true; }
    Promise.resolve().then(run).then((receipt) => {
      if (action === 'reset' && receipt.applied && win.showToast && win.SM && win.SM.t) win.showToast(win.SM.t('toastRigPoseReset'));
      else if (receipt.applied === false) toast(win, 'The rig already matches: nothing to change.');
    }, (error) => failure(win, error));
    return true;
  }
  const installed = new WeakSet();
  function install(win) {
    if (installed.has(win)) return;
    installed.add(win);
    win.document.addEventListener('pointerdown', (e) => {
      const area = win.document.getElementById('canvas-area');
      if (win.state.tool !== 'rig' || win.state.playing || !session(win) || !area || !e.target || !area.contains(e.target) || (e.button !== undefined && e.button !== 0)) return;
      e.stopImmediatePropagation(); e.preventDefault();
      toast(win, 'Rig editing on the canvas is unavailable on a native project (' + CANVAS_REASON + ').');
    }, { capture: true });
  }
  const api = Object.freeze({ panel, install, CANVAS_REASON });
  // The gesture notice listens on document from the first load; it reads the window state only when a pointer arrives.
  if (typeof window === 'object' && window && typeof document === 'object' && document && typeof document.addEventListener === 'function') install(window);
  return api;
}));
