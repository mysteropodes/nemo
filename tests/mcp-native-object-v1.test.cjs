'use strict';
// #2510: a v1 application request on a native object project must answer, never throw. The MCP transport
// (application-mcp.js) only replies to a returned response, so a throw left the host waiting out its 30 s timeout.
const test = require('node:test');
const assert = require('node:assert/strict');
const Operations = require('../src/js/application/native-opacity-operations.js');

function operations(project) {
  const handled = [];
  const ops = Operations.create({ blocksLegacy: () => false }, { objectProject: () => project, currentFrame: () => 3 }, {},
    { create: () => ({}) }, { create: () => ({ handle(request) { handled.push(request.operation); return { legacy: true }; } }) });
  return { ops, handled };
}
const project = { blocksLegacy: () => true, isActive: () => true, info: () => ({ instanceId: 'instance-A', documentId: 'native-object-document-1',
  contentRevision: 4, totalFrames: 21, canvasW: 320, canvasH: 180, fps: 24, layers: [
    { layerUid: 'layer-1', name: 'Ink', visible: false, locked: true, shy: false, color: '#5FA875', parentLayerUid: null }] }) };
const envelope = (operation) => ({ apiVersion: 1, requestId: 'r-' + operation, instanceId: 'instance-A',
  documentId: 'native-object-document-1', revision: 4 });

test('a v1 snapshot on a native object project answers the owner layer state', () => {
  const { ops, handled } = operations(project);
  assert.deepEqual(ops.handleV1({ apiVersion: 1, requestId: 'r-snapshot', operation: 'snapshot', payload: {} }),
    Object.assign(envelope('snapshot'), { ok: true, result: { frame: 3, totalFrames: 21, canvasW: 320, canvasH: 180, fps: 24,
      layers: [{ id: 'layer-1', name: 'Ink', visible: false, locked: true, shy: false }] } }));
  assert.deepEqual(handled, []);
});

test('other v1 requests on a native object project answer a typed refusal with the project identity', () => {
  const { ops, handled } = operations(project);
  for (const operation of ['property.get', 'property.set', 'history.undo']) {
    assert.deepEqual(ops.handleV1({ apiVersion: 1, requestId: 'r-' + operation, operation, payload: {} }),
      Object.assign(envelope(operation), { ok: false, error: { code: 'unavailable',
        message: 'Native object projects answer only capabilities, snapshot and native operations (apiVersion 2).' } }));
  }
  assert.deepEqual(handled, [], 'the v1 opacity handler never sees an object-project request');
});

test('without a native object project the v1 handler answers as before', () => {
  const { ops, handled } = operations({ blocksLegacy: () => false });
  assert.deepEqual(ops.handleV1({ apiVersion: 1, requestId: 'r', operation: 'snapshot', payload: {} }), { legacy: true });
  assert.deepEqual(handled, ['snapshot']);
});

test('before a document opens, a v1 request answers a typed refusal echoing only the request instance', () => {
  // CS4: the native session blocks legacy writes from boot, before any document exists.
  const booting = { blocksLegacy: () => true, isActive: () => false, info: () => null };
  const { ops, handled } = operations(booting);
  assert.deepEqual(ops.handleV1({ apiVersion: 1, requestId: 'r-boot', instanceId: 'instance-A', operation: 'snapshot', payload: {} }),
    { apiVersion: 1, requestId: 'r-boot', instanceId: 'instance-A', documentId: '', revision: 0, ok: false,
      error: { code: 'unavailable', message: 'No native project is open.' } });
  assert.deepEqual(handled, []);
});

test('project.* returns null so the published wrapper hands it to the bootstrap lifecycle', () => {
  for (const candidate of [project, { blocksLegacy: () => false }]) {
    const { ops, handled } = operations(candidate);
    for (const operation of ['project.open', 'project.new', 'project.saveAs', 'project.export'])
      assert.strictEqual(ops.handleV1({ apiVersion: 1, requestId: 'r', operation, payload: {} }), null, operation);
    assert.deepEqual(handled, []);
  }
});

test('a layer with default flags reports explicit booleans', () => {
  const bare = { blocksLegacy: () => true, isActive: () => true, info: () => ({ instanceId: 'instance-A', documentId: 'doc', contentRevision: 0,
    totalFrames: 1, canvasW: 1, canvasH: 1, fps: 24, layers: [{ layerUid: 'layer-1' }] }) };
  const { ops } = operations(bare);
  assert.deepEqual(ops.handleV1({ apiVersion: 1, requestId: 'r', operation: 'snapshot', payload: {} }).result.layers,
    [{ id: 'layer-1', name: undefined, visible: true, locked: false, shy: false }]);
});

test('capabilities answer what a native project serves, with or without an open document', () => {
  const result = { operations: ['capabilities', 'snapshot'], nativeApiVersion: 2, properties: [], descriptors: [],
    documentIdentity: 'open-document-incarnation' };
  const { ops, handled } = operations(project);
  assert.deepEqual(ops.handleV1({ apiVersion: 1, requestId: 'r-capabilities', operation: 'capabilities', payload: {} }),
    Object.assign(envelope('capabilities'), { ok: true, result }));
  const booting = operations({ blocksLegacy: () => true, isActive: () => false, info: () => null });
  assert.deepEqual(booting.ops.handleV1({ apiVersion: 1, requestId: 'r-boot', instanceId: 'instance-A', operation: 'capabilities', payload: {} }),
    { apiVersion: 1, requestId: 'r-boot', instanceId: 'instance-A', documentId: '', revision: 0, ok: true, result });
  assert.deepEqual(handled, []);
  assert.deepEqual(booting.handled, []);
});
