'use strict';
// #2510: a v1 application request on a native object project must answer, never throw. The MCP transport
// (application-mcp.js) only replies to a returned response, so a throw left the host waiting out its 30 s timeout.
const test = require('node:test');
const assert = require('node:assert/strict');
const Operations = require('../src/js/application/native-opacity-operations.js');

function operations(project) {
  const handled = [];
  const ops = Operations.create({ blocksLegacy: () => false }, { objectProject: () => project }, {},
    { create: () => ({}) }, { create: () => ({ handle(request) { handled.push(request.operation); return { legacy: true }; } }) });
  return { ops, handled };
}

test('a v1 request on a native object project answers a typed refusal with the project identity', () => {
  const project = { blocksLegacy: () => true,
    info: () => ({ instanceId: 'instance-A', documentId: 'native-object-document-1', contentRevision: 4 }) };
  const { ops, handled } = operations(project);
  for (const operation of ['snapshot', 'capabilities', 'property.set']) {
    const response = ops.handleV1({ apiVersion: 1, requestId: 'r-' + operation, operation, payload: {} });
    assert.deepEqual(response, { apiVersion: 1, requestId: 'r-' + operation, instanceId: 'instance-A',
      documentId: 'native-object-document-1', revision: 4, ok: false,
      error: { code: 'unavailable', message: 'Native object projects answer only native operations (apiVersion 2).' } });
  }
  assert.deepEqual(handled, [], 'the v1 opacity handler never sees an object-project request');
});

test('without a native object project the v1 handler answers as before', () => {
  const { ops, handled } = operations({ blocksLegacy: () => false });
  assert.deepEqual(ops.handleV1({ apiVersion: 1, requestId: 'r', operation: 'snapshot', payload: {} }), { legacy: true });
  assert.deepEqual(handled, ['snapshot']);
});
