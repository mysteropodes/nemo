'use strict';
/* Test support for the lane-7 brush adapter: a session double that has the serialization, dispatch and reconcile
 * semantics of the REAL native-object-project.js runFontResource route, and an owner double that follows ONLY the
 * documented rules of the operations (the same request id with the same body returns the stored response and never
 * executes again; the same id with another body is refused; a publication leaves the revision unchanged; a scene read
 * pages one entry at a time in the order it was given). The doubles stand in for the Rust owner only to observe what
 * the adapter SENDS and how it reacts; nothing here decides what the adapter must do. */
const assert = require('node:assert/strict');

const PUBLISH = 'command.document.object.brush.bitmap.publish';
const QUERY = 'query.document.object.brush.bitmap';

function ownerDouble(state, options) {
  const receipts = new Map();
  const log = { executed: [], received: [] };
  // brushId -> { version, assetId, width, height }
  const store = new Map();
  let versions = 0;
  function reply(request, ok, payload, revision) {
    const base = { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: revision === undefined ? state.revision : revision, ok };
    return ok ? Object.assign(base, { result: payload }) : Object.assign(base, { error: payload });
  }
  function refusal(code, reason) { return { code, message: 'refused', details: reason ? { reason } : undefined }; }
  function row(brushId) {
    const held = store.get(brushId);
    return held ? { brushId, resident: true, version: String(held.version), width: held.width, height: held.height, bytes: held.width * held.height * 4 } : { brushId, resident: false };
  }
  function execute(request) {
    const p = request.payload;
    if (options.refuse) { const refused = options.refuse(request); if (refused) return reply(request, false, refused); }
    if (request.operation === PUBLISH) {
      if (request.expectedRevision !== state.revision) return reply(request, false, refusal('stale_revision'));
      if (p.documentSnapshotId !== 'snapshot-' + state.revision) return reply(request, false, refusal('stale_revision', 'snapshot'));
      const asset = (options.assets || {})[p.assetId];
      if (!asset) return reply(request, false, refusal('not_found', 'missing_asset'));
      if (asset.width !== p.width || asset.height !== p.height) return reply(request, false, refusal('invalid_request', 'dimension_mismatch'));
      log.executed.push(request.requestId);
      const held = store.get(p.brushId);
      const reused = !!held && held.assetId === p.assetId;
      if (!reused) store.set(p.brushId, { version: ++versions, assetId: p.assetId, width: p.width, height: p.height });
      const now = store.get(p.brushId);
      const result = { applied: !reused, historyEntriesAdded: 0, brushId: p.brushId, version: String(now.version), width: now.width, height: now.height, bytes: now.width * now.height * 4 };
      return reply(request, true, options.shapeResult ? options.shapeResult(result, request) : result);
    }
    if (request.payload.atRevision !== state.revision || request.payload.documentSnapshotId !== 'snapshot-' + state.revision) return reply(request, false, refusal('stale_revision'), state.revision);
    log.executed.push(request.requestId);
    if (p.action === 'inspect') return reply(request, true, { atRevision: p.atRevision, documentSnapshotId: p.documentSnapshotId, brush: row(p.brushId) });
    const all = (options.scene || []).map((entry) => structuredClone(entry));
    const rows = p.offset < all.length ? [all[p.offset]] : [];
    const next = p.offset + rows.length;
    const page = { atRevision: p.atRevision, documentSnapshotId: p.documentSnapshotId, contextId: p.contextId, frame: p.frame, offset: p.offset, nextOffset: next < all.length ? next : null, totalEntries: all.length, entries: rows };
    return reply(request, true, options.shapePage ? options.shapePage(page, request) : page);
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
  return { dispatch, log, store };
}

function createFixture(options) {
  options = options || {};
  const state = { revision: 7, generation: 3, active: true, document: 'document-A' };
  const owner = ownerDouble(state, options);
  const events = { reconciled: [], runs: 0, ids: [] };
  let serial = 0, queue = Promise.resolve();
  function live() {
    return { instanceId: 'instance-A', documentId: state.document, lifecycleGeneration: state.generation, contentRevision: state.revision, documentSnapshotId: 'snapshot-' + state.revision };
  }
  const ctx = {
    dispatch: (request) => owner.dispatch(request),
    async reconcile(revision) {
      events.reconciled.push(revision);
      if (options.reconcileError) throw options.reconcileError;
      if (revision !== undefined && revision !== state.revision) throw new Error('Native font import revision is uncorrelated');
    },
  };
  function runResource(action) {
    events.runs += 1;
    const next = queue.then(() => action(ctx));
    queue = next.catch(() => {});
    return next;
  }
  const session = {
    isActive: () => state.active,
    info: () => live(),
    requestId(kind) { const id = 'project-' + kind + '-' + (++serial); events.ids.push(id); return id; },
  };
  // The base session exports only the font-named route; the lane-6 patch adds the alias. Both are supported by the adapter.
  if (!options.noRoute) session[options.aliasRoute ? 'runResourceCommand' : 'runFontResource'] = runResource;
  return { state, owner, events, session };
}

/* The legacy writers and the render engine must stay unreachable: any touch of these globals fails the test that caused it. */
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

/* A minimal PNG header (signature + IHDR) of the given size: all the adapter reads of the bytes it forwards. */
function pngBytes(width, height, tail) {
  const out = new Uint8Array(33 + (tail || 0));
  out.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52]);
  const view = new DataView(out.buffer);
  view.setUint32(16, width); view.setUint32(20, height);
  out[24] = 8; out[25] = 6;
  for (let i = 33; i < out.length; i++) out[i] = i & 255;
  return out;
}

module.exports = { createFixture, forbidLegacyWriters, pngBytes, PUBLISH, QUERY };
