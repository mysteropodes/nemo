'use strict';
/* Test support for the native vector adapters: a session double with the serialization, dispatch and reconcile semantics of
 * the REAL native-object-project.js runResourceCommand route (ctx.dispatch / ctx.reconcile, one command at a time), and an
 * owner double that follows the documented receipt rules (a receipt is keyed by request id and body: the same id with the
 * same body returns the stored response and never executes again; the same id with another body is refused). The doubles
 * stand in for the Rust owner only to observe what the adapters SEND and how they react; every request is also checked
 * against the REAL contract validator before the double sees it. */
const assert = require('node:assert/strict');
const Contract = require('../src/js/adapters/native-object-vector-contract.js');

const clone = (value) => JSON.parse(JSON.stringify(value));

function ownerDouble(state, options) {
  const receipts = new Map();
  const log = { executed: [], received: [] };
  const records = new Map();
  const layers = new Map();
  function reply(request, ok, payload) {
    const base = { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: state.revision, ok };
    return ok ? Object.assign(base, { result: payload }) : Object.assign(base, { error: payload });
  }
  const refusal = (code, reason) => ({ code, message: 'refused', details: reason ? { reason } : undefined });
  function execute(request) {
    const p = request.payload;
    if (options.refuse) { const refused = options.refuse(request); if (refused) return reply(request, false, refused); }
    if (request.expectedRevision !== state.revision) return reply(request, false, refusal('stale_revision'));
    log.executed.push(request.requestId);
    let applied = false, extra = {};
    if (request.operation === Contract.VECTOR) {
      const kind = p.command.slice('object.vector.'.length);
      if (kind === 'batch') {
        // Every step or none, as the native owner applies a batch.
        const next = new Map(records), ids = [];
        // A copy is an earlier create's record retargeted to its frame, then a create (as the native owner expands it).
        const ops = [];
        for (const op of p.ops) {
          if (op.op !== 'copy') { ops.push(op); continue; }
          const record = clone(ops[op.fromStep].record); record.target.frameScope.frame = op.frame;
          ops.push(Object.assign({}, op, { op: 'create', record }));
        }
        for (const op of ops) {
          const id = op.op === 'create' ? null : (op.op === 'replace' ? op.record.target.strokeId : op.stableTarget.strokeId);
          if (id !== null && !next.has(id)) return reply(request, false, refusal('not_found'));
          if (op.op === 'create') {
            let n = 0;
            while (next.has(`native-stroke:${state.revision + 1}:${n}`)) n += 1;
            const strokeId = `native-stroke:${state.revision + 1}:${n}`, stored = clone(op.record);
            stored.target.strokeId = strokeId; next.set(strokeId, stored); ids.push(strokeId);
          } else { if (op.op === 'replace') next.set(id, clone(op.record)); else next.delete(id); ids.push(id); }
        }
        records.clear(); next.forEach((value, key) => records.set(key, value));
        applied = true; extra = { operation: 'batch', strokeId: ids[0], strokeIds: ids };
      } else if (kind === 'createMany') {
        const ids = p.records.map((record) => {
          let n = 0;
          while (records.has(`native-stroke:${state.revision + 1}:${n}`)) n += 1;
          const strokeId = `native-stroke:${state.revision + 1}:${n}`, stored = clone(record);
          stored.target.strokeId = strokeId; records.set(strokeId, stored); return strokeId;
        });
        applied = true; extra = { operation: 'createMany', strokeId: ids[0], strokeIds: ids };
      } else if (kind === 'create') {
        let n = 0;
        while (records.has(`native-stroke:${state.revision + 1}:${n}`)) n += 1;
        const strokeId = `native-stroke:${state.revision + 1}:${n}`;
        const stored = clone(p.record); stored.target.strokeId = strokeId;
        records.set(strokeId, stored); applied = true; extra = { operation: 'create', strokeId };
      } else {
        const id = kind === 'replace' ? p.record.target.strokeId : p.stableTarget.strokeId;
        if (!records.has(id)) return reply(request, false, refusal('not_found'));
        if (kind === 'replace') { applied = JSON.stringify(records.get(id)) !== JSON.stringify(p.record); if (applied) records.set(id, clone(p.record)); } else { records.delete(id); applied = true; }
        extra = { operation: kind, strokeId: id };
      }
    } else if (request.operation === Contract.PARAMETRIC) {
      if (!records.has(p.stableTarget.strokeId)) return reply(request, false, refusal('not_found'));
      const identity = p.command === 'object.parametric.translate' ? p.delta.every((v) => v === 0)
        : p.command === 'object.parametric.scale' ? p.scale.every((v) => v === 1) : false;
      applied = !identity;
      if (applied) records.get(p.stableTarget.strokeId).lastParametric = clone(p);
    } else {
      const before = JSON.stringify(layers.get(p.layerUid) || null);
      layers.set(p.layerUid, clone(p.stack));
      applied = JSON.stringify(layers.get(p.layerUid)) !== before;
    }
    if (applied) state.revision += 1;
    return reply(request, true, Object.assign({ applied, historyEntriesAdded: applied ? 1 : 0 }, extra));
  }
  async function dispatch(request) {
    log.received.push(clone(request));
    // The double is only meaningful if the closed contract admits what the adapter sent.
    Contract.payload(request.operation, request.payload);
    const fault = options.faults && options.faults.shift();
    if (fault === 'before') throw new Error('transport lost before delivery');
    const body = JSON.stringify(request);
    let stored = receipts.get(request.requestId);
    if (stored && stored.body !== body) return reply(request, false, refusal('invalid_request', 'request_id_body_changed'));
    if (!stored) { stored = { body, response: execute(request) }; receipts.set(request.requestId, stored); }
    if (fault === 'after') throw new Error('transport lost after delivery');
    if (options.respond) return options.respond(clone(stored.response), request);
    return clone(stored.response);
  }
  return { dispatch, log, records, layers };
}

function createFixture(options) {
  options = options || {};
  const state = { revision: 7, generation: 3, active: true, document: 'document-A' };
  const owner = ownerDouble(state, options);
  const events = { reconciled: [], runs: 0, ids: [] };
  let serial = 0, queue = Promise.resolve();
  const live = () => ({ instanceId: 'instance-A', documentId: state.document, lifecycleGeneration: state.generation, contentRevision: state.revision, documentSnapshotId: 'snapshot-' + state.revision });
  // options.sdk: the request and the response cross the REAL common v2 SDK (native-application.js with the composed contracts).
  const through = options.sdk ? options.sdk.createNativeApplicationAdapter('vector-test', { dispatch: (request) => owner.dispatch(request) }) : null;
  const ctx = {
    dispatch: (request) => (through ? through.dispatch(request) : owner.dispatch(request)),
    async reconcile(revision) {
      events.reconciled.push(revision);
      if (options.reconcileError) throw options.reconcileError;
      if (revision !== undefined && revision !== state.revision) throw new Error('Native font import revision is uncorrelated');
    },
  };
  const session = {
    isActive: () => state.active,
    info: live,
    requestId(kind) { const id = 'project-' + kind + '-' + (++serial); events.ids.push(id); return id; },
    runResourceCommand(action) {
      events.runs += 1;
      const next = queue.then(() => action(ctx));
      queue = next.catch(() => {});
      return next;
    },
  };
  return { state, owner, events, session: options.noRoute ? { isActive: session.isActive, info: live, requestId: session.requestId } : session, live };
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
      for (const [name, descriptor] of saved) { if (descriptor) Object.defineProperty(globalThis, name, descriptor); else delete globalThis[name]; }
      assert.deepEqual(touched, []);
    },
  };
}

module.exports = { createFixture, forbidLegacyWriters, clone };
