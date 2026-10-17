'use strict';
// Shared fixture for the native component and duplicator intent tests. The transport double stands only for the process
// boundary (host + owner reply literals scripted by each test); every request still crosses the REAL common v2 SDK
// (native-application.js with the composed component contract), so request and response validation is not mocked.
const SDK = require('../src/js/adapters/native-application.js');

const clone = (value) => JSON.parse(JSON.stringify(value));

// step(request, state) -> { applied, result, error, layers, drop, revision, stayStale } scripted by the test.
function createHarness(options = {}) {
  const state = {
    revision: options.revision === undefined ? 7 : options.revision,
    layers: (options.layers || ['layer-a', 'layer-b', 'layer-c']).slice(),
    sent: [], pins: 0, submits: 0, declaredChecks: 0, counter: 0,
  };
  const step = options.step || (() => ({ applied: true }));
  const transport = {
    dispatch(request) {
      state.sent.push(clone(request));
      const plan = step(request, state) || { applied: true };
      if (plan.drop) throw new Error(plan.drop);
      const base = { apiVersion: 2, requestId: request.requestId, instanceId: 'instance-A', documentId: 'document-A' };
      if (plan.error) return Object.assign(base, { contentRevision: state.revision, ok: false, error: plan.error });
      const applied = plan.applied !== false;
      const revision = request.expectedRevision + (applied ? 1 : 0);
      // The operations that mint a layer answer with its identity, as the owner does.
      const minting = /\.(?:component\.place|duplicator\.objects)$/.test(request.operation);
      const result = plan.result || (minting ? { applied: true, historyEntriesAdded: 1, layerUid: 'native-layer:' + (request.expectedRevision + 1) + ':0' } : { applied, historyEntriesAdded: applied ? 1 : 0 });
      const response = Object.assign(base, { contentRevision: plan.revision === undefined ? revision : plan.revision, ok: true, result });
      // The owner publishes on a later turn, so two unserialized intents would read the same pin.
      return new Promise((resolve) => setImmediate(() => {
        if (plan.layers) state.layers = plan.layers.slice();
        if (plan.stayStale !== true) state.revision = revision;
        resolve(response);
      }));
    },
  };
  const adapter = SDK.createNativeApplicationAdapter('component-test', transport);
  const ports = {
    pin() {
      state.pins += 1;
      if (options.noSession) return null;
      return { instanceId: 'instance-A', documentId: 'document-A', contentRevision: state.revision,
        documentSnapshotId: 'native-object:document-A:' + state.revision, layers: state.layers.map((layerUid) => ({ layerUid })) };
    },
    declared(operation) { state.declaredChecks += 1; return options.declared ? options.declared(operation) : true; },
    async submit(request) {
      state.submits += 1;
      // A port that bypasses the SDK (the session response is passed through unvalidated): the reply is scripted verbatim.
      if (options.rawResponse) return options.rawResponse(request);
      const response = await adapter.dispatch(request);
      // The desktop session throws a classified error for a refused command; mirror that port contract.
      if (options.sessionStyle && !response.ok) throw new Error('Native command refused: ' + response.error.code);
      return response;
    },
    requestId(label) { state.counter += 1; return label + '-' + state.counter; },
  };
  return { ports, state, adapter };
}

// A fake native Project session for portsFromSession: the resource route runs one action with dispatch and reconcile.
// `route` is the session member that carries it ('runResourceCommand' or the base 'runFontResource').
function createSession(options = {}) {
  const log = { dispatched: [], reconciled: [], routed: 0 };
  const info = { instanceId: 'instance-A', documentId: 'document-A', contentRevision: 7, layers: [{ layerUid: 'layer-a' }, { layerUid: 'layer-b' }] };
  const run = (action) => {
    log.routed += 1;
    if (options.busy) return Promise.reject(new Error('Native font owner busy or unavailable'));
    return Promise.resolve().then(() => action({
      dispatch(request) {
        log.dispatched.push(clone(request));
        if (options.dispatchThrows) return Promise.reject(new Error(options.dispatchThrows));
        const minting = /\.(?:component\.place|duplicator\.objects)$/.test(request.operation);
        const reply = options.reply ? options.reply(request) : { apiVersion: 2, requestId: request.requestId, instanceId: 'instance-A', documentId: 'document-A',
          contentRevision: request.expectedRevision + 1, ok: true,
          result: minting ? { applied: true, historyEntriesAdded: 1, layerUid: 'native-layer:8:0' } : { applied: true, historyEntriesAdded: 1 } };
        return Promise.resolve(reply);
      },
      reconcile(revision) {
        log.reconciled.push(revision === undefined ? null : revision);
        if (options.reconcileThrows) return Promise.reject(new Error(options.reconcileThrows));
        return Promise.resolve(info);
      },
    }));
  };
  const session = {
    isActive: () => options.active !== false,
    info: () => info,
    requestId: (label) => 'project-' + label + '-1',
    blocksLegacy: () => options.blocks !== false,
  };
  if (options.route !== null) session[options.route || 'runResourceCommand'] = run;
  return { session, log, info };
}

function request(operation, payload, requestId, revision = 7) {
  return { apiVersion: 2, requestId, instanceId: 'instance-A', documentId: 'document-A', operation, expectedRevision: revision, payload };
}

module.exports = { createHarness, createSession, request, clone, SDK };
