'use strict';
/* Test support for the lane-3 adapters: a session double with the serialization, dispatch and reconcile semantics of the REAL
 * native-object-project.js runFontResource / runResourceCommand route, and an owner double that follows the documented receipt
 * rules (a receipt is keyed by request id and body: the same id with the same body returns the stored response and never
 * executes again; the same id with another body is refused). The doubles stand in for the Rust owner only to observe what the
 * adapters SEND and how they react; they never decide what an adapter must do. */
const assert = require('node:assert/strict');

function ownerDouble(state, options) {
  const receipts = new Map();
  const log = { executed: [], received: [] };
  function reply(request, ok, payload) {
    const base = { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: state.revision, ok };
    return ok ? Object.assign(base, { result: payload }) : Object.assign(base, { error: payload });
  }
  function execute(request) {
    if (options.refuse) {
      const refused = options.refuse(request);
      if (refused) return reply(request, false, { code: refused.code, message: 'refused', details: refused.reason ? { reason: refused.reason } : undefined });
    }
    if (request.expectedRevision !== state.revision) return reply(request, false, { code: 'stale_revision', message: 'stale' });
    log.executed.push(request.requestId);
    const created = request.operation === 'command.document.object.widget.create';
    const applied = created || (options.applied ? options.applied(request) !== false : true);
    if (applied) state.revision += 1;
    const receipt = { applied, historyEntriesAdded: applied ? 1 : 0 };
    if (created) receipt.layerUid = 'native-layer:' + state.revision + ':0';
    return reply(request, true, receipt);
  }
  async function dispatch(request) {
    log.received.push(structuredClone(request));
    const fault = options.faults && options.faults.shift();
    if (fault === 'before') throw new Error('transport lost before delivery');
    const body = JSON.stringify(request);
    let stored = receipts.get(request.requestId);
    if (stored && stored.body !== body) return reply(request, false, { code: 'invalid_request', message: 'refused', details: { reason: 'request_id_body_changed' } });
    if (!stored) { stored = { body, response: execute(request) }; receipts.set(request.requestId, stored); }
    if (fault === 'after') throw new Error('transport lost after delivery');
    if (options.respond) return options.respond(structuredClone(stored.response), request);
    return structuredClone(stored.response);
  }
  return { dispatch, log };
}

function createFixture(options) {
  options = options || {};
  const state = { revision: 7, generation: 3, active: true, document: 'document-A' };
  const owner = ownerDouble(state, options);
  const events = { reconciled: [], runs: 0, ids: [] };
  let serial = 0, queue = Promise.resolve();
  const live = () => ({ instanceId: 'instance-A', documentId: state.document, lifecycleGeneration: state.generation, contentRevision: state.revision, documentSnapshotId: 'snapshot-' + state.revision });
  const ctx = {
    dispatch: (request) => owner.dispatch(request),
    async reconcile(revision) {
      events.reconciled.push(revision);
      if (options.reconcileError) throw options.reconcileError;
      if (revision !== undefined && revision !== state.revision) throw new Error('Native font import revision is uncorrelated');
    },
  };
  function run(action) {
    events.runs += 1;
    const next = queue.then(() => action(ctx));
    queue = next.catch(() => {});
    return next;
  }
  const session = { isActive: () => state.active, info: live, blocksLegacy: () => true, requestId(kind) { const id = 'project-' + kind + '-' + (++serial); events.ids.push(id); return id; } };
  if (options.route !== 'none') session[options.route === 'font' ? 'runFontResource' : 'runResourceCommand'] = run;
  return { state, owner, events, session };
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
const LEGACY = ['SMImageMesh', 'SMMotion', 'state', 'paper', 'userLayers', 'rigResetPose', 'rigCommitFrame', 'rigSetIK', 'rigBindStroke', 'rigAutoAssignLayer', 'ensureLayerRig', 'n20AllowLegacyWrite'];

// A native refusal as the adapter reports it: the rejection carries the typed code, reason and request id.
async function refused(promise) {
  try { await promise; } catch (error) { return error; }
  throw new assert.AssertionError({ message: 'expected a rejection' });
}

module.exports = { createFixture, forbidLegacyWriters, refused, LEGACY };
