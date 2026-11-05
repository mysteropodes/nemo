/* #2371 U11: the writes of the native layer list rows (inline rename, delete, duplicate, drag reorder, colour label, folder
 * collapse, split into layers) over the pinned native project session. Each method is ONE typed request, so one user gesture is
 * one revision and one history entry: rename / delete / duplicate / reorder go through the layer lifecycle intent, the colour
 * label is the declared layer.color.set, the folder chevron is the compositing edit, and "split into layers" is the declared
 * layer.split. The adapter holds no document state and has no fallback writer: an absent project, route or declaration is a typed
 * refusal, and a layer uid the owner no longer lists is refused as stale before anything is sent. Resolves {ok, applied, ...}. */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const Intent = commonjs ? require('./native-object-layer-lifecycle-intent.js') : root.NemoNativeObjectLayerLifecycleIntent;
  const Edit = commonjs ? require('./native-object-edit-contract.js') : root.NemoNativeObjectEditContract;
  const Command = commonjs ? require('./native-object-rig-command.js') : root.NemoNativeObjectRigCommand;
  const Contract = commonjs ? require('./native-object-rig-contract.js') : root.NemoNativeObjectRigContract;
  const api = factory(Intent, Edit, Command, Contract);
  if (commonjs) module.exports = api;
  else root.NemoNativeLayerRowWrites = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Intent, Edit, Command, Contract) {
  'use strict';
  const CONTEXT = 'scene-root', HEX = /^#[0-9a-fA-F]{6}$/;
  const refused = (code, extra) => Object.freeze(Object.assign({ ok: false, applied: false, code }, extra || {}));
  const applied = (r) => Object.freeze({ ok: true, applied: r.applied === true, layerUid: r.layerUid });
  const fromIntent = (r) => (r && r.ok === true ? applied(r) : refused(r && r.code || 'unavailable', { reason: r && r.reason || null }));
  const guarded = (fn) => Promise.resolve().then(fn).catch((e) => refused(e && typeof e.code === 'string' ? e.code : 'unavailable', { reason: e && e.reason || null, message: String(e && e.message || e) }));

  // session: the native project session (info, isActive, requestId, runResourceCommand). `hooks.compositing` answers the
  // compositing adapter and `hooks.labels` the two locale words the split names its pieces with.
  function create(session, hooks) {
    hooks = hooks || {};
    const intent = Intent.createLayerLifecycleIntent({
      pin: () => (session.isActive() ? session.info() : null),
      declared: (operation) => Edit.admitted(operation) && session.isActive() === true,
      requestId: (label) => session.requestId(label),
      submit: (request) => session.runResourceCommand(async (ctx) => {
        const response = await ctx.dispatch(request);
        if (response && response.ok === true) await ctx.reconcile(response.contentRevision);
        return response;
      }),
    });
    const commands = Command.create(session, 'layer-row', Contract, 4096);
    const rows = () => { const info = session.isActive() ? session.info() : null; return info && Array.isArray(info.layers) ? info.layers : []; };
    const has = (uid) => typeof uid === 'string' && rows().some((row) => row.layerUid === uid);
    const stale = (uids) => uids.filter((uid) => !has(uid));
    const sel = (selection) => ({ active: selection.active, selected: selection.selected.slice() });
    const gone = (uids) => refused('stale_selection', { ids: uids });

    return Object.freeze({
      rows, has,
      rename(layerUid, name) {
        if (typeof name !== 'string') return Promise.resolve(refused('invalid_request'));
        return guarded(() => intent.renameLayer({ layerUid, name }).then(fromIntent));
      },
      remove(selection) {
        const ids = [selection.active].concat(selection.selected);
        if (stale(ids).length) return Promise.resolve(gone(stale(ids)));
        if (rows().length <= 1) return Promise.resolve(refused('last_layer'));
        return guarded(() => intent.deleteLayers({ selection: sel(selection) }).then(fromIntent));
      },
      duplicate(layerUid) {
        if (!has(layerUid)) return Promise.resolve(gone([layerUid]));
        return guarded(() => intent.duplicateLayer({ layerUid }).then(fromIntent));
      },
      reorder(layerUids, gap, selection) {
        const ids = layerUids.concat([selection.active], selection.selected);
        if (stale(ids).length) return Promise.resolve(gone(stale(ids)));
        if (!Number.isFinite(gap)) return Promise.resolve(refused('invalid_request'));
        return guarded(() => intent.reorderLayers({ layerUids: layerUids.slice(), gap, selection: sel(selection) }).then(fromIntent));
      },
      // The colour label of a layer, or of the whole selection when the clicked layer is one of several selected (baseline applyLayerColor).
      recolor(layerUid, selected, hex) {
        const ids = [layerUid].concat(selected || []);
        if (stale(ids).length) return Promise.resolve(gone(stale(ids)));
        if (typeof hex !== 'string' || !HEX.test(hex.slice(0, 7))) return Promise.resolve(refused('invalid_request'));
        const body = { command: 'object.layer.color.set', layerUid, color: hex.slice(0, 7).toLowerCase() };
        if (selected && selected.length > 1) body.selected = selected.slice();
        return guarded(() => commands.execute(Contract.COLOR_SET, () => body).then(applied));
      },
      setFolderCollapsed(layerUid, collapsed) {
        const at = rows().findIndex((row) => row.layerUid === layerUid), comp = hooks.compositing && hooks.compositing();
        if (at < 0) return Promise.resolve(gone([layerUid]));
        if (!comp) return Promise.resolve(refused('unavailable'));
        return guarded(() => comp.setFolderCollapsed(at, collapsed === true).then(applied));
      },
      // "Split into layers": one layer per shape of the playhead frame, named with the locale's shape and stroke words.
      split(layerUid, frame) {
        if (!has(layerUid)) return Promise.resolve(gone([layerUid]));
        const labels = hooks.labels && hooks.labels();
        if (!labels || !labels.shape || !labels.stroke || !Number.isInteger(frame) || frame < 0) return Promise.resolve(refused('invalid_request'));
        return guarded(() => intent.splitLayer({ contextId: CONTEXT, layerUid, frame, labels: { shape: labels.shape, stroke: labels.stroke } }).then(fromIntent));
      },
    });
  }
  return Object.freeze({ create });
}));
