/* The Select tool's right-click canvas menu on a native object project (#2372/#2256, baseline 1065e9d8 select-bridge.js onContext). The legacy
 * handler is retired there (it builds Paper clones and Paper z-order moves), so this module offers the baseline rows that have a native route and
 * sends each one through that route; it holds no document state and writes nothing itself:
 *   Copier / Couper / Coller / Dupliquer   -> NemoNativeSelectionClipboard (native-selection-clipboard.js)
 *   Supprimer                              -> NemoNativeSelectionDelete.removeSelection
 *   Grouper / Dissocier                    -> NemoNativeSelectGroup (the group intent)
 *   Combiner (Union), Sortir du groupe, Aplatir -> NemoNativeSelectionBoolean / NemoNativeSelectGroup
 * Right-click rule (baseline): an item that is already part of the selection keeps the whole selection; any other item replaces it, exactly as a
 * plain click does (the native pick, then applyPick); an empty spot keeps the selection and offers Coller alone when a clip is held (otherwise the
 * menu does not open). open() resolves to the rows it showed. The rows the baseline has that still lack a native route (front/back order, pivot reset, image mesh, tween opt-in,
 * tracked roles, revision actions, stroke profiles) are added with their own routes; none is shown as a dead entry. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeSelectMenu = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  function selected(host) {
    return (host.selectedPaths || []).filter(function (p) { return p && p.data && p.data.nativeStableTarget && Number.isInteger(p.data.nativeLayerIndex) && p.data.strokeId; });
  }
  function ready(host) {
    var C = host.NemoNativeSelectionClipboard, I = host.NemoSelectCanvasIntent, st = host.state;
    return !!C && !!I && typeof host.showContextMenu === 'function' && !!st && st.tool === 'select' && !st.playing && C.native(host);
  }
  // The group state of the selection from the layer's native listing (null when it cannot be read: the group rows are then left out).
  async function groupState(host, paths) {
    var G = host.NemoNativeSelectGroup, I = host.NemoSelectCanvasIntent;
    if (!G || paths.some(function (p) { return p.data.nativeLayerIndex !== paths[0].data.nativeLayerIndex; })) return null;
    try {
      var layers = await I.layerPaths(host, function () { return false; }, false, paths[0].data.nativeLayerIndex);
      if (!layers) return null;
      var info = G.groupsInfo(layers.items), ids = paths.map(function (p) { return p.data.strokeId; });
      return { grouped: ids.some(function (id) { return (info.pathOf[id] || []).length > 0; }), active: G.activeGroup(info, ids) };
    } catch (_) { return null; }
  }
  // The menu rows for the selection the right-click left (empty: the selection is empty).
  async function rows(host, paths) {
    var C = host.NemoNativeSelectionClipboard, G = host.NemoNativeSelectGroup, B = host.NemoNativeSelectionBoolean, D = host.NemoNativeSelectionDelete;
    if (!paths.length) return C.hasClip(host) ? [{ label: 'Coller', shortcut: '⌘V', action: function () { C.paste(host); } }] : [];
    var multi = paths.length > 1;
    var items = [
      { label: 'Copier', shortcut: '⌘C', action: function () { C.copy(host); } },
      { label: 'Couper', shortcut: '⌘X', action: function () { C.cut(host); } },
      { label: 'Coller', shortcut: '⌘V', disabled: !C.hasClip(host), action: function () { C.paste(host); } },
      { label: 'Dupliquer', shortcut: '⌘D', action: function () { C.duplicate(host); } }
    ];
    if (D && typeof D.removeSelection === 'function') items.push({ label: multi ? 'Supprimer la sélection' : 'Supprimer', shortcut: 'Suppr', action: function () { D.removeSelection(host); } });
    var state = G ? await groupState(host, paths) : null;
    if (state) {
      var extra = [];
      if (state.grouped) extra.push({ label: 'Dissocier', shortcut: '⇧⌘G', action: function () { G.ungroup(host); } });
      else if (multi) extra.push({ label: 'Grouper', shortcut: '⌘G', action: function () { G.group(host); } });
      if (multi && !state.active && B) extra.push({ label: 'Combiner (Union) — non destructif', action: function () { B.combine(host, 'unite'); } });
      if (state.active) {
        if (!multi) extra.push({ label: 'Sortir du groupe', action: function () { G.removeMember(host); } });
        extra.push({ label: 'Aplatir', action: function () { G.flatten(host); } });
      }
      if (extra.length) items = items.concat([{ sep: true }], extra);
    }
    return items;
  }
  async function build(host, x, y) {
    var I = host.NemoSelectCanvasIntent, hit;
    try { hit = await I.pick(host, x, y); } catch (_) { hit = undefined; }
    var held = host.selectedPaths || [];
    // An item already in the selection keeps the whole selection; any other item replaces it (a plain click). A spot the read could not prove
    // (undefined) leaves the selection as it is.
    if (hit && !held.some(function (p) { return p && p.data && p.data.strokeId === hit.data.strokeId; })) I.applyPick(host, hit, false);
    var items = await rows(host, selected(host));
    if (items.length) host.showContextMenu(x, y, items);
    return items;
  }
  // Called from the canvas's contextmenu listener once the pointer intent has consumed the event. True when the menu took the gesture.
  function open(host, e) {
    if (!ready(host)) return false;
    var x = e.clientX, y = e.clientY;
    return build(host, x, y).catch(function () { return []; });
  }
  return Object.freeze({ open: open, rows: rows, ready: ready });
}));
