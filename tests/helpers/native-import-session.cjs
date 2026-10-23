'use strict';
// L10-J test helper: a presented native session whose `importBatch` follows ONLY the documented owner receipt rule of the accepted
// L10-H transaction (ids `native-layer:<rev+1>:<n>` / `native-stroke:<rev+1>:<n>`, one revision and one history entry for a changing
// batch, none for the no-op, unavailable rows refuse the whole batch, a locked or unknown layer reference refuses it). It stores no
// document beyond the layer table the adapters read back, and it is not the owner: the Rust owner is verified by its own suite.
const PIN_KEYS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
const clone = (v) => JSON.parse(JSON.stringify(v));

function layerRow(uid, name, extra) {
  return Object.assign({ layerUid: uid, name, visible: true, locked: false, parentLayerUid: null, color: '#a1b2c3' }, extra || {});
}

function makeSession(options) {
  const o = options || {};
  const state = {
    instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 1, contentRevision: 7, documentSnapshotId: 'snapshot-7',
    canvasW: o.canvasW || 1920, canvasH: o.canvasH || 1080, fps: 24, totalFrames: 48,
    layers: clone(o.layers || [layerRow('L0', 'Layer 1')]),
  };
  const calls = [];
  const frame = { value: o.frame === undefined ? 0 : o.frame };
  let declared = o.declared !== false;
  function importBatch(payload, expected) {
    if (!PIN_KEYS.every((k) => expected && expected[k] === state[k])) throw new Error('Native import intent is stale or invalid');
    calls.push(clone(payload));
    if (o.fault === 'transport') throw new Error('transport down');
    if (o.fault === 'stale') throw new Error('Native command refused: stale_revision');
    if (o.fault === 'busy') throw new Error('Native command refused: busy_conflict');
    if (payload.unavailable && payload.unavailable.length) throw new Error('Native command refused: unavailable');
    const batchIds = new Set(payload.layers.map((l) => l.sourceId));
    for (const item of payload.records) {
      const ref = item.record.target.layerUid;
      const existing = state.layers.find((l) => l.layerUid === ref);
      if (!batchIds.has(ref) && !existing) throw new Error('Native command refused: not_found');
      if (existing && !batchIds.has(ref) && existing.locked) throw new Error('Native command refused: unavailable');
    }
    const applied = payload.layers.length > 0 || payload.records.length > 0;
    const rev = state.contentRevision + 1;
    const layers = payload.layers.map((l, n) => ({ sourceId: l.sourceId, layerUid: `native-layer:${rev}:${n}` }));
    const objects = payload.records.map((r, n) => ({ sourceId: r.sourceId, strokeId: `native-stroke:${rev}:${n}` }));
    const result = { applied, historyEntriesAdded: applied ? 1 : 0, layers: applied ? layers : [], objects: applied ? objects : [], skipped: clone(payload.skipped || []) };
    if (applied) {
      state.contentRevision = rev; state.documentSnapshotId = `snapshot-${rev}`;
      payload.layers.forEach((l, n) => { if (!o.hideCreated) state.layers.push(layerRow(layers[n].layerUid, l.name, o.createdExtra)); });
    }
    let receipt = { apiVersion: 2, requestId: 'project-import-batch-1', instanceId: state.instanceId, documentId: state.documentId, contentRevision: state.contentRevision, ok: true, result };
    if (o.tamper) receipt = o.tamper(receipt) || receipt;
    return o.gate ? o.gate.then(() => receipt) : Promise.resolve(receipt);
  }
  const session = {
    info: () => clone(state),
    readPresentedObjectSession: () => {
      const pin = {}; PIN_KEYS.forEach((k) => { pin[k] = state[k]; });
      return Object.assign({ available: true }, pin, { presentation: Object.assign({}, pin, { owner: 'native', status: 'presented', frame: frame.value }) });
    },
    get importBatch() { return declared ? importBatch : undefined; },
  };
  return { session, state, calls, frame, setDeclared: (v) => { declared = v; } };
}

function makePorts(h, extra) {
  const selection = { value: ['L0'] };
  const ports = Object.assign({
    currentSession: () => h.session, frame: () => h.frame.value, available: () => true,
    selection: () => selection.value.slice(), select: (uid) => { selection.value = [uid]; return true; },
  }, extra || {});
  return { ports, selection };
}
module.exports = { makeSession, makePorts, layerRow, clone, PIN_KEYS };
