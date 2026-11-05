/* #2371 U11 / D36: the layer kinds of the "+" menu and the folder membership of the native layer list. Baseline 1065e9d8
 * (src/js/timeline.js addNullLayer / addEffectLayer / addGuideLayer / addFolderLayer / reparentLayersIntoFolder /
 * removeLayerFromFolder, src/js/effector-layer.js addEffectorLayer): a Null, Folder, Effect or Guide layer is named after its kind with
 * the first free "Layer N" number, a Null or Folder made over a selection adopts it (centred on its bounds) and every new layer is
 * selected. Each gesture is ONE native command from native-layer-row-writes.js, so one revision and one history entry; this file keeps
 * no document state, reads the pinned project info at the moment of the gesture and has no fallback writer: with no native project
 * every entry answers {status: 'unavailable'}. The timeline.js entry points (SM.addNullLayer ...) and SMEffectorLayer.addEffectorLayer
 * are one-line delegations to the functions below. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeLayerKindsUi = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const WORDS = {
    layerKindFolder: 'Folder', ctxMenuInsert: 'Insert…', ctxInsertLayer: 'Insert a layer', ctxInsertLayerNull: 'Insert a Null layer',
    ctxInsertLayerFolder: 'Insert a folder', ctxInsertLayerEffect: 'Insert an effect layer', ctxInsertLayerGuide: 'Insert a Guide layer',
    ctxInsertLayerJoystick: 'Insert a Joystick', ctxInsertLayerSlider: 'Insert a Slider', ctxRemoveFromFolderParent: 'Remove from the folder',
    ctxAddEffectorLayer: 'Add an effector layer', toastEffectorNeedsDuplicator: 'Enable the duplicator on this layer first',
    toastEffectorLayerAdded: 'Effector layer added', hsLayerRemovedFromFolder: 'Layer removed from the folder',
    toastLayerMovedToFolder: 'Layer moved into “{folder}”', toastLayersMovedToFolder: '{n} layers moved into “{folder}”',
  };
  const BASE_NAMES = { null: 'Null', effect: 'Effet', guide: 'Guide', effector: 'Effector' };
  const REFUSALS = {
    stale_selection: 'The layers changed - nothing was applied.',
    unavailable: 'This layer change is not available here: the layers cannot move without changing how they look.',
    invalid_request: 'The layer change was not valid.', not_found: 'The layers changed - nothing was applied.',
  };
  const word = (host, key) => { let v = null; try { v = host.SM && typeof host.SM.t === 'function' ? host.SM.t(key) : null; } catch (_) { v = null; } return typeof v === 'string' && v && v !== key ? v : WORDS[key]; };
  const toast = (host, text) => { try { if (typeof host.showToast === 'function') host.showToast(text); } catch (_) { /* a notice failure must not become a second writer */ } };
  const gone = Promise.resolve({ status: 'unavailable' });
  const failed = (host, result) => { toast(host, REFUSALS[result && result.code] || 'The layer change was not applied.'); return result; };

  // The native project binding of the layer list (writes included), or null when no native project blocks the legacy writers.
  function native(host) {
    const ui = host && host.NemoNativeLayerRowUi;
    try { return ui && typeof ui.native === 'function' ? ui.native(host) : null; } catch (_) { return null; }
  }
  const infoOf = (b) => { try { return b.session.isActive() ? b.session.info() : null; } catch (_) { return null; } };
  const inComponent = (host) => { try { return !!(host.NemoNativeComponentContext && host.NemoNativeComponentContext.presented(host)); } catch (_) { return false; } };
  // The binding and the pinned info when a layer structure write is possible right now (a scene, not an entered component).
  function usable(host) {
    const b = native(host), info = b && infoOf(b);
    return b && info && Array.isArray(info.layers) && !inComponent(host) ? { b, info } : null;
  }
  function selectUid(host, b, uid) {
    const info = infoOf(b), at = info ? info.layers.findIndex((l) => l.layerUid === uid) : -1;
    if (at >= 0 && host.NemoNativeObjectProject && typeof host.NemoNativeObjectProject.selectLayerUI === 'function') {
      try { host.NemoNativeObjectProject.selectLayerUI(host, at, false, uid, info); } catch (_) { /* best effort: the revision already landed */ }
    }
  }
  // Baseline nextLayerName().replace(/^Layer/, kind): the first free "Layer N" number, behind the word of the kind.
  function nameFor(host, info, kind) {
    const names = new Set(info.layers.map((l) => l.name)); let n = 1;
    while (names.has('Layer ' + n)) n += 1;
    return (kind === 'folder' ? word(host, 'layerKindFolder') : BASE_NAMES[kind]) + ' ' + n;
  }
  // The layers the user had selected, as uids (indices that no longer name a layer are dropped).
  function selectedUids(host, info) {
    const picked = Array.isArray(host._layerSel) ? host._layerSel : [];
    return picked.filter((i, k, all) => Number.isInteger(i) && info.layers[i] && all.indexOf(i) === k).map((i) => info.layers[i].layerUid);
  }
  const centre = (info) => [info.canvasW / 2, info.canvasH / 2];
  const finish = (host, b, promise, after) => Promise.resolve(promise).then((r) => {
    if (r && r.ok === false) return failed(host, r);
    if (r && r.layerUid) selectUid(host, b, r.layerUid);
    if (after) after(r);
    return r;
  });

  // addNullLayer / addFolderLayer / addEffectLayer / addGuideLayer: a Null or Folder over the selection adopts it.
  function add(host, kind) {
    const ctx = usable(host);
    if (!ctx) return gone;
    const grouping = kind === 'null' || kind === 'folder', children = grouping ? selectedUids(host, ctx.info) : [], options = {};
    if (children.length) options.children = children;
    else if (kind !== 'effect') options.pos = centre(ctx.info);
    return finish(host, ctx.b, ctx.b.writes.createKind(kind, nameFor(host, ctx.info, kind), options));
  }
  // The Joystick and Slider widgets are created by the widget module (it creates their controls in the same write).
  function addWidget(host, kind) {
    if (!usable(host) || !host.SMRigWidget || typeof host.SMRigWidget.addWidgetLayer !== 'function') return gone;
    host.SMRigWidget.addWidgetLayer(kind);
    return Promise.resolve({ status: 'requested' });
  }
  // addEffectorLayer(targetLi): only a layer that hosts a duplicator can be the target.
  function addEffector(host, targetUid) {
    const ctx = usable(host);
    if (!ctx) return gone;
    const target = ctx.info.layers.find((l) => l.layerUid === targetUid);
    if (!target || target.hasDuplicator !== true) { toast(host, word(host, 'toastEffectorNeedsDuplicator')); return gone; }
    return finish(host, ctx.b, ctx.b.writes.createKind('effector', nameFor(host, ctx.info, 'effector'), { target: targetUid, pos: centre(ctx.info) }),
      () => toast(host, word(host, 'toastEffectorLayerAdded')));
  }
  function addEffectorAt(host, index) {
    const ctx = usable(host), row = ctx && ctx.info.layers[index];
    return row ? addEffector(host, row.layerUid) : gone;
  }
  const kindOf = (info, uid) => { const row = info.layers.find((l) => l.layerUid === uid); return row ? row.kind : null; };
  const parentKind = (info, row) => { const parent = row && typeof row.parentLayerUid === 'string' ? info.layers.find((l) => l.layerUid === row.parentLayerUid) : null; return parent ? parent.kind : null; };

  // reparentLayersIntoFolder: the folder itself and other folders are skipped (no nested folders in v1). By uid for the drag drop.
  function moveIntoFolderUids(host, uids, folderUid) {
    const ctx = usable(host), folder = ctx && ctx.info.layers.find((l) => l.layerUid === folderUid);
    if (!folder || folder.kind !== 'folder') return gone;
    const moved = uids.filter((u) => u !== folderUid && kindOf(ctx.info, u) !== 'folder').length;
    if (!moved) return gone;
    const name = folder.name || word(host, 'layerKindFolder');
    return finish(host, ctx.b, ctx.b.writes.moveIntoFolder(uids, folderUid), () => toast(host, moved > 1
      ? word(host, 'toastLayersMovedToFolder').replace('{n}', moved).replace('{folder}', name) : word(host, 'toastLayerMovedToFolder').replace('{folder}', name)));
  }
  function moveIntoFolder(host, indices, folderIndex) {
    const ctx = usable(host), folder = ctx && ctx.info.layers[folderIndex];
    if (!folder) return gone;
    const uids = (Array.isArray(indices) ? indices : []).map((i) => ctx.info.layers[i] && ctx.info.layers[i].layerUid).filter((u) => typeof u === 'string');
    return moveIntoFolderUids(host, uids, folder.layerUid);
  }
  // removeLayerFromFolder(index): a layer parented to something other than a folder is left alone.
  function leaveFolder(host, index) {
    const ctx = usable(host), row = ctx && ctx.info.layers[index];
    if (!row || parentKind(ctx.info, row) !== 'folder') return gone;
    return finish(host, ctx.b, ctx.b.writes.leaveFolder([row.layerUid]), () => toast(host, word(host, 'hsLayerRemovedFromFolder')));
  }
  function toggleFolder(host, index) {
    const ctx = usable(host), row = ctx && ctx.info.layers[index];
    if (!row || row.kind !== 'folder') return gone;
    return finish(host, ctx.b, ctx.b.writes.setFolderCollapsed(row.layerUid, row.folderCollapsed !== true));
  }

  // The right-click entries of a native layer row that this file owns: the Insert submenu (anchored at the row), Remove from the
  // folder (greyed unless the layer sits in a folder) and, on a duplicator host, Add an effector layer.
  function rowItems(host, index, anchor) {
    const ctx = usable(host), row = ctx && ctx.info.layers[index];
    if (!row) return [];
    const sm = host.SM, call = (name) => () => { if (sm && typeof sm[name] === 'function') sm[name](); };
    const insert = [['ctxInsertLayer', 'addLayer'], ['ctxInsertLayerNull', 'addNullLayer'], ['ctxInsertLayerFolder', 'addFolderLayer'], ['ctxInsertLayerEffect', 'addEffectLayer'],
      ['ctxInsertLayerGuide', 'addGuideLayer'], ['ctxInsertLayerJoystick', 'addJoystickLayer'], ['ctxInsertLayerSlider', 'addSliderLayer']]
      .map(([key, fn]) => ({ label: word(host, key), action: call(fn) }));
    const box = anchor && typeof anchor.getBoundingClientRect === 'function' ? anchor.getBoundingClientRect() : { left: 0, top: 0, height: 0 };
    const items = [{ label: word(host, 'ctxMenuInsert'), action: () => { if (typeof host.showContextMenu === 'function') host.showContextMenu(box.left + 40, box.top + box.height / 2, insert); } },
      { label: word(host, 'ctxRemoveFromFolderParent'), disabled: parentKind(ctx.info, row) !== 'folder', action: () => leaveFolder(host, index) }];
    if (row.hasDuplicator === true) items.push({ label: word(host, 'ctxAddEffectorLayer'), action: () => addEffector(host, row.layerUid) });
    return items;
  }
  return Object.freeze({ add, addWidget, addEffector, addEffectorAt, moveIntoFolder, moveIntoFolderUids, leaveFolder, toggleFolder, rowItems, nameFor, selectedUids, native });
}));
