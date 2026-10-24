'use strict';
// Shared fixture for the native object edit intent tests. The transport double stands only for the process boundary
// (host + owner reply literals scripted by each test); every request still crosses the REAL common v2 SDK
// (native-application.js with the composed edit contract), so request and response validation is not mocked.
const SDK = require('../src/js/adapters/native-application.js');

const clone = (value) => JSON.parse(JSON.stringify(value));

// step(request, state) -> { applied, result, error, layers, strokes, drop } scripted by the test.
function createHarness(options = {}) {
  const state = {
    revision: options.revision === undefined ? 7 : options.revision,
    layers: (options.layers || ['layer-a', 'layer-b', 'layer-c']).slice(),
    strokes: clone(options.strokes || {}),
    sent: [], pins: 0, submits: 0, declaredChecks: 0, strokeReads: 0, counter: 0,
  };
  const step = options.step || (() => ({ applied: true }));
  const transport = {
    dispatch(request) {
      state.sent.push(clone(request));
      const plan = step(request, state) || { applied: true };
      if (plan.drop) throw new Error(plan.drop);
      const base = { apiVersion: 2, requestId: request.requestId, instanceId: 'instance-A', documentId: 'document-A' };
      if (plan.error) return Object.assign(base, { contentRevision: state.revision, ok: false, error: plan.error });
      const applied = plan.applied !== false && request.operation !== 'command.document.object.clipboard.copy';
      const revision = request.expectedRevision + (applied ? 1 : 0);
      const clipboard = /\.clipboard\.(?:copy|cut)$/.test(request.operation);
      const result = plan.result || (clipboard ? { applied, historyEntriesAdded: applied ? 1 : 0, clipboard: plan.clipboard === undefined ? null : plan.clipboard } : { applied, historyEntriesAdded: applied ? 1 : 0 });
      const response = Object.assign(base, { contentRevision: plan.revision === undefined ? revision : plan.revision, ok: true, result });
      // The owner publishes on a later turn, so two unserialized intents would read the same pin.
      return new Promise((resolve) => setImmediate(() => {
        if (plan.layers) state.layers = plan.layers.slice();
        if (plan.strokes) state.strokes = Object.assign({}, state.strokes, clone(plan.strokes));
        if (plan.stayStale !== true) state.revision = revision;
        resolve(response);
      }));
    },
  };
  const adapter = SDK.createNativeApplicationAdapter('intent-test', transport);
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
  if (!options.noStrokeProjection) {
    ports.strokeIds = (target) => { state.strokeReads += 1; return Array.isArray(state.strokes[target.layerUid]) ? state.strokes[target.layerUid].slice() : null; };
  }
  return { ports, state, adapter };
}

function request(operation, payload, requestId, revision = 7) {
  return { apiVersion: 2, requestId, instanceId: 'instance-A', documentId: 'document-A', operation, expectedRevision: revision, payload };
}

module.exports = { createHarness, request, clone, SDK };
