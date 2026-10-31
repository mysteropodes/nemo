/* #2372 U12: Parent A of a layer over the native document. The baseline layer menus (buildParentMenuItems, the pickwhip and
 * the parent cell's right-click in src/js/timeline.js @1065e9d8) become ONE native edit,
 * command.document.object.layer.parent.set: it names the layer, the new parent (or null for "Parent A : Aucun") and the
 * playhead frame, and the owner keeps the child's world pose at that frame, refusing typed when it cannot do so exactly.
 * The adapter holds no document state: parents are read from the pinned project info (layers[].parentLayerUid), a pick is
 * built at execution time from the current revision, and there is no fallback writer - an absent project, route or
 * declaration is a typed unavailability. Parent B, Parent in Time and Follow Path have no native owner yet: their menus report
 * unavailable and never reach the legacy layer model. */
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
    return Object.freeze({ layerUid: uidOf, parentUidOf, descendants, setParent, rowKind: (index) => { const row = rowAt(index); return row && row.kind || null; } });
  }

  // What a refusal says to the person: the owner's code, in the words of the menu's own cases.
  function refusalText(error) {
    const code = error && error.code;
    if (code === 'unavailable') return 'Parenting would move the layer: a rotated or scaled parent, an expression or an animated Position cannot be kept in place natively.';
    if (code === 'not_found') return 'The layer or its parent is no longer in the project.';
    if (code === 'invalid_request') return 'Parenting refused: a layer cannot be its own parent, a folder cannot be a parent and a cycle is not allowed.';
    return 'Native layer parent command was refused.';
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
    const layers = host.state.layers, bad = adapter.descendants(li), current = adapter.parentUidOf(li);
    const items = [{ label: 'Parent A : Aucun (parentage libre)', disabled: !current, action: () => { pick(host, adapter, li, null, onChanged); } }];
    layers.forEach((other, oi) => {
      if (oi === li) return; // a layer can't parent itself
      const uid = adapter.layerUid(oi), folder = adapter.rowKind(oi) === 'folder';
      items.push({
        label: 'Parent A : ' + (other.name || ('Layer ' + (oi + 1))) + (bad.has(oi) ? '  (descendant)' : '') + (folder ? '  (dossier)' : ''),
        disabled: !uid || bad.has(oi) || folder || current === uid,
        action: () => { pick(host, adapter, li, oi, onChanged); },
      });
    });
    items.push({ sep: true });
    items.push({ label: 'Parent B (indisponible sur un projet natif)', disabled: true });
    return items;
  }
  // Parent in Time and Follow Path: no native owner; the menu says so and nothing reaches the legacy layer model.
  function unavailableItems(host, label, notice) {
    toast(host, notice);
    return [{ label, disabled: true }];
  }
  function parentCleared(host, adapter, li, onChanged) { return pick(host, adapter, li, null, onChanged); }
  function parentChosen(host, adapter, li, parentIndex, onChanged) { return pick(host, adapter, li, parentIndex, onChanged); }

  return Object.freeze({ create, desktop, refusalText, parentMenuItems, unavailableItems, parentCleared, parentChosen });
}));
