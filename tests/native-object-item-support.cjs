'use strict';
/* Test support for the lane-6 adapters: a session double that has the serialization, dispatch and reconcile
 * semantics of the REAL native-object-project.js runFontResource/runResourceCommand route, and an owner double
 * that follows the documented receipt rules (a receipt is keyed by request id and body: the same id with the
 * same body returns the stored response and never executes again; the same id with another body is refused).
 * The doubles stand in for the Rust owner only to observe what the adapters SEND and how they react. */
const assert = require('node:assert/strict');

function ownerDouble(state, options) {
  const receipts = new Map();
  const log = { executed: [], received: [] };
  const assets = new Map();
  const items = new Map();
  function reply(request, ok, payload) {
    const base = { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: state.revision, ok };
    return ok ? Object.assign(base, { result: payload }) : Object.assign(base, { error: payload });
  }
  function refusal(code, reason) { return { code, message: 'refused', details: reason ? { reason } : undefined }; }
  function execute(request) {
    const p = request.payload;
    if (options.refuse) { const refused = options.refuse(request); if (refused) return reply(request, false, refused); }
    if (request.expectedRevision !== state.revision) return reply(request, false, refusal('stale_revision'));
    log.executed.push(request.requestId);
    let applied = false, extra = {};
    if (request.operation === 'command.document.object.asset.attach') {
      const key = p.source.path;
      if (!assets.has(key)) { assets.set(key, 'asset-' + request.requestId); applied = true; }
    } else if (request.operation === 'command.document.object.asset.remove') {
      for (const [key, id] of assets) if (id === p.assetId) { assets.delete(key); applied = true; }
    } else {
      const c = p.command;
      const stroke = c.endsWith('.create') ? (c.startsWith('object.raster.') ? 'native-raster:' : 'native-text:') + (state.revision + 1) + ':0' : p.stableTarget.strokeId;
      const before = JSON.stringify(items.get(stroke) || null);
      if (c.endsWith('.create')) items.set(stroke, p.item || p.block);
      else if (c.endsWith('.delete')) items.delete(stroke);
      else items.set(stroke, Object.assign({}, items.get(stroke) || {}, p.patch || {}, ['x', 'y', 'width', 'height', 'rotation', 'opacity'].reduce((a, k) => (k in p ? Object.assign(a, { [k]: p[k] }) : a), {})));
      applied = JSON.stringify(items.get(stroke) || null) !== before;
      extra = { operation: c.slice('object.'.length), strokeId: stroke };
    }
    if (applied) state.revision += 1;
    return reply(request, true, Object.assign({ applied, historyEntriesAdded: applied ? 1 : 0 }, extra));
  }
  async function dispatch(request) {
    log.received.push(structuredClone(request));
    const fault = options.faults && options.faults.shift();
    if (fault === 'before') throw new Error('transport lost before delivery');
    const body = JSON.stringify(request);
    let stored = receipts.get(request.requestId);
    if (stored && stored.body !== body) return reply(request, false, refusal('invalid_request', 'request_id_body_changed'));
    if (!stored) { stored = { body, response: execute(request) }; receipts.set(request.requestId, stored); }
    if (fault === 'after') throw new Error('transport lost after delivery');
    if (options.respond) return options.respond(structuredClone(stored.response), request);
    return structuredClone(stored.response);
  }
  return { dispatch, log, assets, items };
}

function createFixture(options) {
  options = options || {};
  const state = { revision: 7, generation: 3, active: true, document: 'document-A' };
  const owner = ownerDouble(state, options);
  const events = { reconciled: [], runs: 0, ids: [] };
  let serial = 0, queue = Promise.resolve();
  let snapshot = null;
  function live() {
    return { instanceId: 'instance-A', documentId: state.document, lifecycleGeneration: state.generation, contentRevision: state.revision, documentSnapshotId: 'snapshot-' + state.revision };
  }
  // snapshotInfo: like the real session.info(), the pin only moves when the session is reconciled (inspect), never by itself.
  function info() { return options.snapshotInfo ? (snapshot = snapshot || live()) : live(); }
  const ctx = {
    dispatch: (request) => owner.dispatch(request),
    async reconcile(revision) {
      events.reconciled.push(revision);
      if (options.snapshotInfo) snapshot = live();
      if (options.reconcileError) throw options.reconcileError;
      if (revision !== undefined && revision !== state.revision) throw new Error('Native font import revision is uncorrelated');
    },
  };
  const session = {
    isActive: () => state.active,
    info,
    requestId(kind) { const id = 'project-' + kind + '-' + (++serial); events.ids.push(id); return id; },
    runResourceCommand(action) {
      events.runs += 1;
      const next = queue.then(() => action(ctx));
      queue = next.catch(() => {});
      return next;
    },
  };
  return { state, owner, events, session: options.noRoute ? { isActive: session.isActive, info, requestId: session.requestId } : session };
}

/* The legacy writers must stay unreachable: any touch of these globals fails the test that caused it. */
function forbidLegacyWriters(names) {
  const touched = [];
  const saved = new Map();
  for (const name of names) {
    saved.set(name, Object.getOwnPropertyDescriptor(globalThis, name));
    Object.defineProperty(globalThis, name, { configurable: true, get() { touched.push(name); throw new Error('legacy writer reached: ' + name); } });
  }
  return {
    touched,
    restore() {
      for (const [name, descriptor] of saved) {
        if (descriptor) Object.defineProperty(globalThis, name, descriptor); else delete globalThis[name];
      }
      assert.deepEqual(touched, []);
    },
  };
}

module.exports = { createFixture, forbidLegacyWriters };
