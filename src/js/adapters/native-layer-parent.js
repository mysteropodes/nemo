/* #2372 U12: Parent A of a layer over the native document. The baseline layer menus (buildParentMenuItems, the pickwhip and
 * the parent cell's right-click in src/js/timeline.js @1065e9d8) become ONE native edit,
 * command.document.object.layer.parent.set: it names the layer, the new parent (or null for "Parent A : Aucun") and the
 * playhead frame, and the owner keeps the child's world pose at that frame, refusing typed when it cannot do so exactly.
 * The adapter holds no document state: parents are read from the pinned project info (layers[].parentLayerUid), a pick is
 * built at execution time from the current revision, and there is no fallback writer - an absent project, route or
 * declaration is a typed unavailability. #2372: Link time (command.document.object.layer.timeLink.set, "Time: None" included) and
 * Follow path (command.document.object.layer.followPath.set) are the same kind of one-command edit over fields the document
 * already stores; their menus (Link time to: / Time: None, Chemin : / Chemin : Aucun) and the pickwhip setter route here. Parent B
 * has no native model yet: its item is listed disabled and nothing reaches the legacy layer model. */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const command = commonjs ? require('./native-object-rig-command.js') : root.NemoNativeObjectRigCommand;
  const contract = commonjs ? require('./native-object-rig-contract.js') : root.NemoNativeObjectRigContract;
  const api = factory(command, contract);
  if (commonjs) module.exports = api;
  else root.NemoNativeLayerParent = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Command, Contract) {
  'use strict';
  const LIMIT = 4096, ROOT_CONTEXT = 'scene-root';
  const text = (v) => typeof v === 'string' && v.length > 0;
  const fault = (code, message, extra) => Command.fault(code, message, extra);

  function create(session) {
    const commands = Command.create(session, 'parent', Contract, LIMIT);
    const rows = (info) => (info && Array.isArray(info.layers) ? info.layers : []);
    const rowAt = (index) => rows(session.info())[index] || null;
    const uidOf = (index) => { const row = rowAt(index); return row && text(row.layerUid) ? row.layerUid : null; };
    const parentUidOf = (index) => { const row = rowAt(index); return row && text(row.parentLayerUid) ? row.parentLayerUid : null; };
    // Layer indices whose chain passes through `li`: they cannot become its parent (the owner refuses the cycle as well).
    function descendants(li) {
      const all = rows(session.info()), byUid = new Map(all.map((row, i) => [row.layerUid, i])), mine = uidOf(li), out = new Set();
      all.forEach((row, i) => {
        const seen = new Set();
        for (let at = row; at && text(at.parentLayerUid) && !seen.has(at.parentLayerUid); at = all[byUid.get(at.parentLayerUid)]) {
          if (at.parentLayerUid === mine) { out.add(i); break; }
          seen.add(at.parentLayerUid);
        }
      });
      return out;
    }
    // One native edit; `parent` is a layer uid or null. The frame is the playhead the world pose is kept at.
    function setParent(index, parent, frame) {
      const uid = uidOf(index);
      if (!uid) return Promise.reject(fault('not_found', 'The layer is not in the native project', { reason: 'layer_missing' }));
      if (!Number.isInteger(frame) || frame < 0) return Promise.reject(fault('invalid_request', 'The playhead frame is invalid', { reason: 'local_validation' }));
      return commands.execute(Contract.PARENT_SET, () => ({ command: 'object.layer.parent.set', contextId: ROOT_CONTEXT, layerUid: uid, parentLayerUid: parent, frame }));
    }
    const stored = (index, key) => { const row = rowAt(index); return row && text(row[key]) ? row[key] : null; };
    // timeLinkWouldCycle: walking the source's own links reaches the layer, so linking to it would close a loop.
    function timeLinkWouldCycle(li, oi) {
      const all = rows(session.info()), mine = uidOf(li), seen = new Set();
      for (let at = all[oi], guard = 0; at && guard < 64; guard += 1) {
        if (at.layerUid === mine) return true;
        if (seen.has(at.layerUid)) return false;
        seen.add(at.layerUid);
        const next = at.timeLinkUid;
        at = all.find((row) => row.layerUid === next);
      }
      return false;
    }
    // One native edit; `source` is a layer uid or null (Time: None, which keeps the layer's range). mode/srcAnchor refine the link.
    function setTimeLink(index, source, frame, options) {
      const uid = uidOf(index);
      if (!uid) return Promise.reject(fault('not_found', 'The layer is not in the native project', { reason: 'layer_missing' }));
      if (!Number.isInteger(frame) || frame < 0) return Promise.reject(fault('invalid_request', 'The playhead frame is invalid', { reason: 'local_validation' }));
      return commands.execute(Contract.TIME_LINK_SET, () => {
        const payload = { command: 'object.layer.timeLink.set', contextId: ROOT_CONTEXT, layerUid: uid, sourceLayerUid: source, frame };
        if (source !== null && options && options.mode) payload.mode = options.mode;
        if (source !== null && options && options.srcAnchor) payload.srcAnchor = options.srcAnchor;
        return payload;
      });
    }
    // `options.align` (boolean) authors the orient toggle with the target; omitted keeps the stored one. A clear carries none.
    function setFollowPath(index, target, options) {
      const uid = uidOf(index);
      if (!uid) return Promise.reject(fault('not_found', 'The layer is not in the native project', { reason: 'layer_missing' }));
      return commands.execute(Contract.FOLLOW_PATH_SET, () => {
        const payload = { command: 'object.layer.followPath.set', contextId: ROOT_CONTEXT, layerUid: uid, targetLayerUid: target };
        if (target !== null && options && typeof options.align === 'boolean') payload.align = options.align;
        return payload;
      });
    }
    const indexOf = (uid) => rows(session.info()).findIndex((row) => row.layerUid === uid);
    const nameOf = (index) => { const row = rowAt(index); return row ? (typeof row.name === 'string' && row.name ? row.name : 'Layer ' + (index + 1)) : null; };
    return Object.freeze({ layerUid: uidOf, parentUidOf, descendants, setParent, setTimeLink, setFollowPath, timeLinkWouldCycle,
      timeLinkUidOf: (index) => stored(index, 'timeLinkUid'), followPathUidOf: (index) => stored(index, 'followPathUid'),
      followPathAlignOf: (index) => { const row = rowAt(index); return !!row && row.followPathAlign === true; }, indexOf, nameOf, count: () => rows(session.info()).length,
      rowKind: (index) => { const row = rowAt(index); return row && row.kind || null; } });
  }

  // What a refusal says to the person: the owner's code, in the words of the menu's own cases.
  function refusalText(error) {
    const code = error && error.code;
    if (code === 'unavailable') return 'Parenting would move the layer: an expression, an animated Position, Rotation or Scale, a skewing or collapsed parent chain, or a layer without a pivot (text, image, mask, 3D, rig) cannot be kept in place natively.';
    if (code === 'not_found') return 'The layer or its parent is no longer in the project.';
    if (code === 'invalid_request') return 'Parenting refused: a layer cannot be its own parent, a folder cannot be a parent and a cycle is not allowed.';
    return 'Native layer parent command was refused.';
  }

  const LINK_REFUSALS = {
    time: { unavailable: 'Link time refused: a layer edge cannot be resolved exactly (an expression-driven offset or a layer without timing).',
      invalid_request: 'Link time refused: a layer cannot follow its own time, a link cannot close a cycle and only scene layers can be linked.' },
    path: { unavailable: 'Follow Path refused: the document cannot store it.',
      invalid_request: 'Follow Path refused: a layer cannot follow itself and a folder, Null or effect layer has no path to follow.' },
  };
  function linkRefusalText(kind, error) {
    const code = error && error.code, table = LINK_REFUSALS[kind];
    if (code === 'not_found') return 'The layer or its target is no longer in the project.';
    return table[code] || 'Native layer ' + (kind === 'time' ? 'time link' : 'follow path') + ' command was refused.';
  }

  const bindings = new WeakMap();
  function desktop(host) {
    const module = host.NemoNativeObjectProject, session = module && module.desktop(host);
    if (!session || !session.blocksLegacy()) return null;
    if (!bindings.has(host)) bindings.set(host, create(session));
    return bindings.get(host);
  }
  const frameOf = (host) => Math.max(0, Math.floor(Number(host.state && host.state.currentFrame) || 0));
  const toast = (host, message) => { if (typeof host.showToast === 'function') host.showToast(message); };
  const refreshAll = (host) => { if (typeof host.nativeCompositingRefresh === 'function') host.nativeCompositingRefresh(); };

  // One pick: the native edit, then the caller's refresh and the shared panel refresh from the confirmed revision.
  function pick(host, adapter, li, parentIndex, onChanged) {
    const parent = parentIndex === null ? null : adapter.layerUid(parentIndex);
    if (parentIndex !== null && !parent) { toast(host, refusalText({ code: 'not_found' })); return Promise.resolve(false); }
    return adapter.setParent(li, parent, frameOf(host)).then(() => {
      if (typeof onChanged === 'function') onChanged();
      refreshAll(host);
      return true;
    }, (error) => { toast(host, refusalText(error)); return false; });
  }

  // The Parent A picker menu (layer-list parent cell and the Layer Properties panel share it). Parent B has no native model.
  function parentMenuItems(host, adapter, li, onChanged) {
    const bad = adapter.descendants(li), current = adapter.parentUidOf(li);
    const items = [{ label: 'Parent A : Aucun (parentage libre)', disabled: !current, action: () => { pick(host, adapter, li, null, onChanged); } }];
    for (let oi = 0, n = adapter.count(); oi < n; oi += 1) {
      if (oi === li) continue; // a layer can't parent itself
      const uid = adapter.layerUid(oi), folder = adapter.rowKind(oi) === 'folder';
      items.push({
        label: 'Parent A : ' + adapter.nameOf(oi) + (bad.has(oi) ? '  (descendant)' : '') + (folder ? '  (dossier)' : ''),
        disabled: !uid || bad.has(oi) || folder || current === uid,
        action: () => { pick(host, adapter, li, oi, onChanged); },
      });
    }
    items.push({ sep: true });
    items.push({ label: 'Parent B (indisponible sur un projet natif)', disabled: true });
    return items;
  }
  const tr = (host, key, fallback) => { const value = host.SM && typeof host.SM.t === 'function' ? host.SM.t(key) : null; return typeof value === 'string' && value && value !== key ? value : fallback; };
  const nameFor = (adapter, index) => adapter.nameOf(index) || 'Layer ' + (index + 1);
  // Link time to `sourceIndex` (null = Time: None, which keeps the layer's range): one native edit at the playhead, then the refresh.
  function timeLinkSet(host, adapter, li, sourceIndex, mode, srcAnchor, onChanged) {
    const source = sourceIndex === null ? null : adapter.layerUid(sourceIndex);
    if (sourceIndex !== null && !source) { toast(host, linkRefusalText('time', { code: 'not_found' })); return Promise.resolve(false); }
    if (sourceIndex !== null && adapter.timeLinkWouldCycle(li, sourceIndex)) { toast(host, tr(host, 'toastLinkImpossibleCycle', 'Lien impossible : créerait un cycle')); return Promise.resolve(false); }
    return adapter.setTimeLink(li, source, frameOf(host), { mode, srcAnchor }).then(() => {
      if (typeof onChanged === 'function') onChanged();
      refreshAll(host);
      toast(host, source === null ? tr(host, 'toastTimeLinkRemoved', 'Lien temporel retiré') : 'Temps lié à « ' + nameFor(adapter, sourceIndex) + ' »');
      return true;
    }, (error) => { toast(host, linkRefusalText('time', error)); return false; });
  }
  // The Link time picker (layer row menu, keyframe cell menu): "Time: None" and "Link time to: <layer>", the stored source ticked.
  function timeLinkMenuItems(host, adapter, li, onChanged) {
    const current = adapter.timeLinkUidOf(li);
    const items = [{ label: tr(host, 'ctxTimeNoneUnlink', 'Time: None (unlink, position kept)'), disabled: !current, action: () => { timeLinkSet(host, adapter, li, null, undefined, undefined, onChanged); } }];
    for (let oi = 0, n = adapter.count(); oi < n; oi += 1) {
      if (oi === li) continue; // a layer can't follow its own time
      const uid = adapter.layerUid(oi), isCurrent = !!uid && uid === current, cycle = adapter.timeLinkWouldCycle(li, oi);
      items.push({
        label: tr(host, 'ctxLinkTimeToPrefix', 'Link time to: ') + nameFor(adapter, oi) + (isCurrent ? '  ✓' : '') + (cycle ? '  (cycle)' : ''),
        disabled: !uid || isCurrent || cycle,
        action: () => { timeLinkSet(host, adapter, li, oi, 'both', undefined, onChanged); },
      });
    }
    return items;
  }
  // Follow `targetUid` (null = Chemin : Aucun): one native edit, then the refresh.
  function followPathSet(host, adapter, li, targetUid, onChanged) {
    if (targetUid !== null && !text(targetUid)) { toast(host, linkRefusalText('path', { code: 'not_found' })); return Promise.resolve(false); }
    return adapter.setFollowPath(li, targetUid).then(() => {
      if (typeof onChanged === 'function') onChanged();
      refreshAll(host);
      return true;
    }, (error) => { toast(host, linkRefusalText('path', error)); return false; });
  }
  // The Align button of the Path row (baseline mp-align-btn): author the orient toggle of the stored path. Only a layer that follows a path has one.
  function followPathAlignSet(host, adapter, li, align, onChanged) {
    const target = adapter.followPathUidOf(li);
    if (!target || typeof align !== 'boolean') { toast(host, 'Follow Path : choisissez d\'abord un calque de tracé.'); return Promise.resolve(false); }
    return adapter.setFollowPath(li, target, { align }).then(() => {
      if (typeof onChanged === 'function') onChanged();
      refreshAll(host);
      return true;
    }, (error) => { toast(host, linkRefusalText('path', error)); return false; });
  }
  // The Follow Path picker: only a layer that can carry a path is a target (a folder, Null or effect layer never does).
  function followPathMenuItems(host, adapter, li, onChanged) {
    const current = adapter.followPathUidOf(li);
    const items = [{ label: 'Chemin : Aucun', disabled: !current, action: () => { followPathSet(host, adapter, li, null, onChanged); } }];
    for (let oi = 0, n = adapter.count(); oi < n; oi += 1) {
      if (oi === li) continue; // a layer can't follow itself
      const uid = adapter.layerUid(oi), kind = adapter.rowKind(oi), bare = kind === 'folder' || kind === 'null' || kind === 'effect';
      items.push({
        label: 'Chemin : ' + nameFor(adapter, oi) + (bare ? '  (sans tracé)' : ''),
        disabled: !uid || bare || current === uid,
        action: () => { followPathSet(host, adapter, li, uid, onChanged); },
      });
    }
    return items;
  }
  // Right-click on a native layer row: the Link time and Follow path pickers (the baseline layer menu / Motion pills reach them).
  function rowMenu(host, adapter, li, row, onChanged) {
    if (!adapter || !row || typeof row.addEventListener !== 'function') return;
    row.addEventListener('contextmenu', (e) => {
      if (typeof host.showContextMenu !== 'function') return;
      e.preventDefault(); e.stopPropagation();
      const x = e.clientX + 8, y = e.clientY + 8;
      host.showContextMenu(x, y, [
        { label: tr(host, 'ctxParentInTimeLinkTimeEllipsis', 'Parent in Time — lier le temps à…'), action: () => { host.showContextMenu(x, y, timeLinkMenuItems(host, adapter, li, onChanged)); } },
        { label: 'Chemin (Follow Path)…', action: () => { host.showContextMenu(x, y, followPathMenuItems(host, adapter, li, onChanged)); } },
      ].concat(host.NemoNativeObjectDuplicatorUi ? host.NemoNativeObjectDuplicatorUi.contextItems(host, li) : [])); // #2256: Object-mode Duplicator…
    });
  }
  // The baseline layer-row Parent cell (.lparent: pickwhip .lpick + label; 1065e9d8 buildParentCell) for a NATIVE row: the label is the
  // native parent's name (or the dash), the pickwhip drag and the click menu are the Parent A edit above, a right-click clears it.
  // Parent B has no native model, so the label never shows it and the menu lists it disabled.
  function startPickwhip(host, adapter, li, fromEl, ev, onChanged) {
    ev.stopPropagation(); ev.preventDefault();
    const doc = host.document, bad = adapter.descendants(li), box = fromEl.getBoundingClientRect();
    const ox = box.left + box.width / 2, oy = box.top + box.height / 2, line = doc.createElement('div');
    line.className = 'lpick-line'; doc.body.appendChild(line);
    let hover = null;
    const paint = (x, y) => {
      const dx = x - ox, dy = y - oy;
      line.style.left = ox + 'px'; line.style.top = oy + 'px'; line.style.width = Math.sqrt(dx * dx + dy * dy) + 'px';
      line.style.transform = 'rotate(' + Math.atan2(dy, dx) + 'rad)';
    };
    const rowUnder = (x, y) => {
      const el = doc.elementFromPoint(x, y), row = el && el.closest ? el.closest('.native-object-layer') : null;
      const idx = row && row.dataset ? adapter.indexOf(row.dataset.layerUid) : -1;
      return idx < 0 || idx === li || bad.has(idx) || adapter.rowKind(idx) === 'folder' ? null : { row, idx };
    };
    const onMove = (e) => {
      paint(e.clientX, e.clientY);
      const t = rowUnder(e.clientX, e.clientY);
      if (hover && (!t || t.row !== hover.row)) hover.row.classList.remove('pick-target');
      if (t && (!hover || t.row !== hover.row)) t.row.classList.add('pick-target');
      hover = t;
    };
    const cleanup = () => {
      doc.removeEventListener('mousemove', onMove, true); doc.removeEventListener('mouseup', onUp, true); doc.removeEventListener('keydown', onKey, true);
      if (hover) hover.row.classList.remove('pick-target');
      line.remove();
    };
    const onUp = (e) => {
      const t = rowUnder(e.clientX, e.clientY);
      cleanup();
      if (t) parentChosen(host, adapter, li, t.idx, onChanged); // dropped on nothing / itself / a descendant / a folder: no command
    };
    const onKey = (e) => { if (e.key === 'Escape') cleanup(); };
    doc.addEventListener('mousemove', onMove, true); doc.addEventListener('mouseup', onUp, true); doc.addEventListener('keydown', onKey, true);
    paint(ev.clientX, ev.clientY);
  }
  function cell(host, info, layer, li, row) {
    const adapter = desktop(host), doc = host.document, uid = layer && layer.parentLayerUid;
    const pIdx = text(uid) && Array.isArray(info.layers) ? info.layers.findIndex((row) => row.layerUid === uid) : -1;
    const pName = pIdx >= 0 ? (typeof info.layers[pIdx].name === 'string' && info.layers[pIdx].name ? info.layers[pIdx].name : 'Layer ' + (pIdx + 1)) : null;
    const node = doc.createElement('div'), pickwhip = doc.createElement('span'), label = doc.createElement('span');
    const t = host.SM && typeof host.SM.t === 'function' ? host.SM.t('titleDragLayerSetParentA') : null;
    const changed = () => { if (typeof host.renderLayerList === 'function') host.renderLayerList(); if (typeof host.renderTimeline === 'function') host.renderTimeline(); };
    node.className = 'lparent' + (pName ? '' : ' none'); pickwhip.className = 'lpick';
    pickwhip.title = t && t !== 'titleDragLayerSetParentA' ? t : 'Drag onto a layer to set it as the parent (A)';
    label.textContent = pName || '—';
    node.title = pName ? 'Parent : ' + pName + ' — cliquer pour changer' : 'Aucun parent — cliquer pour en choisir un';
    node.appendChild(pickwhip); node.appendChild(label);
    node.dataset.nativeParentCell = 'true';
    pickwhip.addEventListener('mousedown', (e) => { if (adapter) startPickwhip(host, adapter, li, pickwhip, e, changed); });
    node.addEventListener('mousedown', (e) => { e.stopPropagation(); });
    node.addEventListener('click', (e) => {
      e.stopPropagation(); e.preventDefault();
      if (!adapter || typeof host.showContextMenu !== 'function') return;
      const r = node.getBoundingClientRect();
      host.showContextMenu(r.left, r.bottom + 2, parentMenuItems(host, adapter, li, changed));
    });
    node.addEventListener('contextmenu', (e) => {
      e.preventDefault(); e.stopPropagation();
      if (adapter && adapter.parentUidOf(li)) parentCleared(host, adapter, li, changed);
    });
    rowMenu(host, adapter, li, row, changed);
    return node;
  }
  function parentCleared(host, adapter, li, onChanged) { return pick(host, adapter, li, null, onChanged); }
  function parentChosen(host, adapter, li, parentIndex, onChanged) { return pick(host, adapter, li, parentIndex, onChanged); }

  return Object.freeze({ create, desktop, refusalText, parentMenuItems, timeLinkMenuItems, followPathMenuItems, timeLinkSet, followPathSet, followPathAlignSet, linkRefusalText, parentCleared, parentChosen, cell, startPickwhip });
}));
