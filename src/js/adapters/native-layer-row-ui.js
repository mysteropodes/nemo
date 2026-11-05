/* #2371 U11: the controls of the native layer list rows, bound onto the finished rows by the layer-list presenter (rowEdit).
 * Baseline 1065e9d8 (src/js/timeline.js renderLayerList / startLayerRename / openLayerColorSwatches): double-click or F2 renames
 * in place (Enter or blur commits the trimmed name, Escape cancels), the colour dot opens the swatches (Shift+click selects every
 * layer wearing the colour), the folder chevron folds the folder, a vertical drag reorders, and the right-click menu offers
 * duplicate, delete, rename, split into layers, cut at the playhead and the colour. Every write is one native command from
 * native-layer-row-writes.js; this file renders nothing of its own state, reads the pinned project info at the moment of the
 * gesture, and has no fallback writer: with no native project every entry answers false so the caller keeps its own path. */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const Writes = commonjs ? require('./native-layer-row-writes.js') : root.NemoNativeLayerRowWrites;
  const Drag = commonjs ? require('./native-layer-row-drag.js') : root.NemoNativeLayerRowDrag;
  const api = factory(Writes, Drag);
  if (commonjs) module.exports = api;
  else root.NemoNativeLayerRowUi = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Writes, Drag) {
  'use strict';
  const bindings = new WeakMap();
  const WORDS = {
    ctxDuplicateLayer: 'Duplicate the layer', ctxDeleteLayer: 'Delete the layer', elementsRename: 'Rename', hsLayerColour: 'Layer colour',
    ctxSplitIntoLayers: 'Split into layers (one shape per layer)', ctxCutAtPlayhead: 'Cut at the playhead  (⌘⇧D)', hsCannotDeleteLastLayer: 'The last layer cannot be deleted',
    toastLayerColorApplied: 'Color applied to {n} layers', toastLayersSelectedByColor: '{n} layers selected by color', autoNameShape: 'Shape', autoNameStroke: 'Stroke',
    layerColorDotTitle: 'Layer color label — click to change it (applies to the whole selection), Shift+click to select every layer with this color',
  };
  const REFUSALS = { stale_selection: 'The layers changed - nothing was applied.', unavailable: 'This layer is locked or cannot be edited right now.', invalid_request: 'The layer change was not valid.' };
  const word = (host, key) => { let v = null; try { v = host.SM && typeof host.SM.t === 'function' ? host.SM.t(key) : null; } catch (_) { v = null; } return typeof v === 'string' && v && v !== key ? v : WORDS[key]; };
  const toast = (host, text) => { try { if (typeof host.showToast === 'function') host.showToast(text); } catch (_) { /* a notice failure must not become a second writer */ } };
  function refusal(host, result) {
    if (!result || result.ok) return;
    toast(host, result.code === 'last_layer' ? word(host, 'hsCannotDeleteLastLayer') : REFUSALS[result.code] || 'The layer change was not applied.');
  }

  // The native project binding of a host, or null when no native project blocks the legacy writers.
  function native(host) {
    const project = host && host.NemoNativeObjectProject;
    let session = null;
    try { session = project && typeof project.desktop === 'function' ? project.desktop(host) : null; } catch (_) { session = null; }
    if (!session || typeof session.blocksLegacy !== 'function' || session.blocksLegacy() !== true) return null;
    let held = bindings.get(host);
    if (!held || held.session !== session) {
      const comp = () => { const m = host.NemoNativeObjectLayerCompositing; try { return m && typeof m.desktop === 'function' ? m.desktop(host) : null; } catch (_) { return null; } };
      held = { session, writes: Writes.create(session, { compositing: comp, labels: () => ({ shape: word(host, 'autoNameShape'), stroke: word(host, 'autoNameStroke') }) }) };
      bindings.set(host, held);
    }
    return held;
  }
  const inComponent = (host) => { try { return !!(host.NemoNativeComponentContext && host.NemoNativeComponentContext.presented(host)); } catch (_) { return false; } };
  const infoOf = (b) => { try { return b.session.isActive() ? b.session.info() : null; } catch (_) { return null; } };
  const listOf = (host) => host.document.getElementById('layer-list');
  function rowFor(host, uid) {
    const list = listOf(host), rows = list ? Array.from(list.children) : [];
    return rows.find((r) => r.dataset && r.dataset.layerUid === uid) || null;
  }
  // The editor selection as layer uids, from the indices the list keeps; null when nothing is selected.
  function selectionOf(host, info) {
    const L = info.layers, ok = (i) => Number.isInteger(i) && i >= 0 && i < L.length;
    const marked = (Array.isArray(host._layerSel) ? host._layerSel : []).filter((i, k, all) => ok(i) && all.indexOf(i) === k);
    const act = host.state && ok(host.state.activeLayerIdx) ? host.state.activeLayerIdx : -1;
    const picked = marked.length ? marked : act >= 0 ? [act] : [];
    if (!picked.length) return null;
    return { active: L[act >= 0 ? act : picked[0]].layerUid, selected: picked.slice().sort((a, c) => a - c).map((i) => L[i].layerUid) };
  }
  function selectUid(host, b, uid, mode) {
    const info = infoOf(b), at = info ? info.layers.findIndex((l) => l.layerUid === uid) : -1;
    if (at >= 0 && host.NemoNativeObjectProject && typeof host.NemoNativeObjectProject.selectLayerUI === 'function') {
      try { host.NemoNativeObjectProject.selectLayerUI(host, at, mode ? { mode } : false, uid, info); } catch (_) { /* best effort: the revision already landed */ }
    }
  }
  const finish = (host, promise, after) => Promise.resolve(promise).then((r) => { if (r && r.ok === false) refusal(host, r); else if (after) after(r); return r; });

  function withinName(target, row) {
    const pick = row.children[0];
    for (let n = target; n; n = n.parentNode) { if (n === pick) return true; if (n === row) return target === row; }
    return false;
  }
  // A pointer press that starts a drag: the name, the kind icon or the bare row, never a button, the dots, the parent cell or a field.
  function dragStart(target, row) {
    const pick = row.children[0];
    for (let n = target; n && n !== row; n = n.parentNode) {
      const cls = String(n.className || '');
      if (n === pick) return true;
      if (n.tagName === 'INPUT' || n.tagName === 'BUTTON' || /\b(lico|lparent)\b/.test(cls) && !/\blkind\b/.test(cls)) return false;
    }
    return true;
  }

  // Inline rename. Resolves nothing: the revision lands through the session's own repaint.
  function startRename(host, uid) {
    const b = native(host);
    if (!b) return false;
    const info = infoOf(b), at = info ? info.layers.findIndex((l) => l.layerUid === uid) : -1, row = at >= 0 ? rowFor(host, uid) : null;
    if (!row) return true;
    const open = Array.from(row.children).find((c) => c.className === 'native-layer-rename');
    if (open) { if (typeof open.focus === 'function') open.focus(); return true; }
    const pick = row.children[0], layer = info.layers[at], current = layer.name === undefined ? 'Layer ' + (at + 1) : layer.name;
    const input = host.document.createElement('input');
    input.type = 'text'; input.value = current; input.className = 'native-layer-rename';
    input.style.cssText = 'flex:1;min-width:0;background:var(--bg);border:1px solid var(--accent);color:var(--text);font-size:11px;border-radius:4px;padding:1px 4px;outline:none;';
    input.style.order = pick.style.order || '0';
    const shown = pick.style.display;
    pick.style.display = 'none';
    if (typeof row.insertBefore === 'function') row.insertBefore(input, pick.nextSibling || null); else row.appendChild(input);
    let done = false;
    function close() { done = true; if (input.parentNode) input.parentNode.removeChild(input); pick.style.display = shown; }
    function commit() {
      if (done) return;
      const value = String(input.value).trim();
      close();
      if (!value || value === current) return;
      finish(host, b.writes.rename(uid, value));
    }
    input.addEventListener('keydown', (e) => { e.stopPropagation(); if (e.key === 'Enter') commit(); else if (e.key === 'Escape') { if (!done) close(); } });
    input.addEventListener('blur', commit);
    ['mousedown', 'pointerdown', 'click', 'dblclick'].forEach((type) => input.addEventListener(type, (e) => e.stopPropagation()));
    if (typeof input.focus === 'function') input.focus();
    if (typeof input.select === 'function') input.select();
    return true;
  }
  function recolor(host, b, uid, hex) {
    const info = infoOf(b), sel = info && selectionOf(host, info);
    const many = !!sel && sel.selected.length > 1 && sel.selected.includes(uid);
    const targets = many ? sel.selected : [uid];
    finish(host, b.writes.recolor(uid, targets, hex), () => { if (targets.length > 1) toast(host, word(host, 'toastLayerColorApplied').replace('{n}', targets.length)); });
  }
  function selectByColor(host, b, color) {
    const info = infoOf(b), want = String(color || '').toLowerCase();
    if (!info || !want) return 0;
    const hits = info.layers.map((l, i) => (String(l.color || '').toLowerCase() === want ? i : -1)).filter((i) => i >= 0);
    hits.forEach((i, k) => selectUid(host, b, info.layers[i].layerUid, k ? 'toggle' : null));
    if (hits.length) toast(host, word(host, 'toastLayersSelectedByColor').replace('{n}', hits.length));
    return hits.length;
  }
  function openSwatches(host, b, uid, anchor) {
    const info = infoOf(b), layer = info && info.layers.find((l) => l.layerUid === uid);
    if (!layer || typeof host.openLayerColorSwatches !== 'function') return false;
    host.openLayerColorSwatches(anchor, layer.color || '#8b8b9e', (hex) => recolor(host, b, uid, hex));
    return true;
  }
  function toggleFolder(host, b, uid) {
    const info = infoOf(b), layer = info && info.layers.find((l) => l.layerUid === uid);
    if (layer && layer.kind === 'folder') finish(host, b.writes.setFolderCollapsed(uid, layer.folderCollapsed !== true));
  }

  // Binds the writes of one finished row: called by the presenter once per row with the row's layer and index.
  function bind(host, layer, index, row) {
    const b = native(host);
    if (!b) return false;
    const uid = layer.layerUid;
    const arrow = Array.from(row.children).find((c) => /\blarrow\b/.test(String(c.className)) && c.dataset && c.dataset.nativeLayerDecor);
    const dot = Array.from(row.children).find((c) => /\blayer-color-dot\b/.test(String(c.className)));
    if (arrow) arrow.addEventListener('click', (e) => { e.stopPropagation(); toggleFolder(host, b, uid); });
    if (dot) {
      dot.title = word(host, 'layerColorDotTitle');
      dot.addEventListener('click', (e) => {
        e.stopPropagation();
        if (e.shiftKey) { const l = (infoOf(b) || { layers: [] }).layers.find((x) => x.layerUid === uid); selectByColor(host, b, l && l.color); return; }
        openSwatches(host, b, uid, dot);
      });
    }
    // A component opens on double-click (the presenter's own handler); any other layer renames in place.
    row.addEventListener('dblclick', (e) => { if (layer.kind !== 'component' && withinName(e.target, row)) startRename(host, uid); });
    const begin = (e) => {
      if (e.button !== 0 || !dragStart(e.target, row)) return;
      const live = native(host), info = live && infoOf(live);
      if (!info || !info.layers.some((l) => l.layerUid === uid)) return;
      Drag.arm(host, {
        writes: live.writes, session: live.session, info, uid, selection: selectionOf(host, info), list: listOf(host),
        stale: 'The layers changed - the move was cancelled.', toast: (t) => toast(host, t), refused: (r) => refusal(host, r),
      }, e);
    };
    row.addEventListener('mousedown', begin); row.addEventListener('pointerdown', begin);
    return true;
  }

  // The right-click entries of a native layer row (the caller appends its Parent / Link / Duplicator entries).
  function menuItems(host, index) {
    const b = native(host), info = b && infoOf(b), layer = info && info.layers[index];
    if (!layer) return [];
    const uid = layer.layerUid;
    // A right-click outside the selection makes that row the selection, like the baseline's setActiveLayer; the entries then
    // act on what the click named, never on a selection the click was not part of.
    if (!Array.isArray(host._layerSel) || host._layerSel.indexOf(index) < 0) selectUid(host, b, uid, null);
    const menu = () => { const now = infoOf(b), sel = now && selectionOf(host, now); return sel && sel.selected.includes(uid) ? sel : { active: uid, selected: [uid] }; };
    const plain = layer.kind === 'component';
    const items = [
      { label: word(host, 'ctxDuplicateLayer'), action: () => finish(host, b.writes.duplicate(uid), (r) => { if (r && r.layerUid) selectUid(host, b, r.layerUid, null); }) },
      { label: word(host, 'ctxDeleteLayer'), action: () => finish(host, b.writes.remove(menu())) },
      { sep: true },
      { label: word(host, 'elementsRename'), action: () => { startRename(host, uid); } },
      { label: word(host, 'ctxSplitIntoLayers'), disabled: plain, action: () => finish(host, b.writes.split(uid, host.state.currentFrame)) },
      { label: word(host, 'ctxCutAtPlayhead'), action: () => { if (host.SM && typeof host.SM.splitLayerAtPlayhead === 'function') host.SM.splitLayerAtPlayhead(index); } },
      { label: word(host, 'hsLayerColour'), action: () => { const row = rowFor(host, uid); if (row) openSwatches(host, b, uid, row); } },
    ];
    if (layer.kind === 'folder') items.push({ label: layer.folderCollapsed ? 'Déplier le dossier' : 'Replier le dossier', action: () => toggleFolder(host, b, uid) });
    return items.concat([{ sep: true }]);
  }
  // The toolbar trash and duplicate buttons: the selection (or the active layer) of the pinned project.
  function toolbar(host, kind) {
    const b = native(host);
    if (!b) return false;
    const info = infoOf(b), sel = info && !inComponent(host) && selectionOf(host, info);
    if (!sel) { refusal(host, { ok: false, code: 'unavailable' }); return true; }
    if (kind === 'delete') finish(host, b.writes.remove(sel));
    else if (kind === 'duplicate') finish(host, b.writes.duplicate(sel.active), (r) => { if (r && r.layerUid) selectUid(host, b, r.layerUid, null); });
    return true;
  }
  // F2 (cmdRenameLayer): rename the active layer in place. Returns true when it was consumed.
  function shortcut(host, action, event) {
    if (action !== 'cmdRenameLayer') return false;
    const b = native(host);
    if (!b) return false;
    if (event && typeof event.preventDefault === 'function') event.preventDefault();
    const info = infoOf(b), sel = info && !inComponent(host) && selectionOf(host, info);
    if (sel) startRename(host, sel.active);
    return true;
  }
  return Object.freeze({ bind, menuItems, toolbar, shortcut, startRename, native });
}));
