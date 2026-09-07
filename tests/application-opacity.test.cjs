'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
function app() {
  let identity = 0;
  const context = { crypto: { randomUUID: () => `test-${++identity}` }, state: { currentFrame: 0, totalFrames: 24, layers: [{ layerUid: 'a', name: 'A', motionStatic: { opacity: [100] } }] }, window: null, pushUndo() { context.pushes++; }, pushes: 0 };
  context.window = context;
  context.SMMotion = {
    valueAtFrame(layer) { return layer.motionStatic.opacity; },
    ensureLayerUid(layer) { return layer.layerUid; }
  };
  vm.createContext(context);
  for (const f of ['src/js/domain/animation/opacity.js', 'src/js/application/opacity-application.js', 'src/js/bootstrap/opacity-application.js']) vm.runInContext(fs.readFileSync(path.join(__dirname, '..', f), 'utf8'), context, { filename: f });
  return context;
}
function request(ctx, id, operation, payload) { const m = ctx.NemoOpacityApplication.meta(); return { apiVersion: 1, requestId: id, instanceId: m.instanceId, documentId: m.documentId, expectedRevision: m.revision, operation, payload: payload || {} }; }
test('opacity command uses stable layer ids, revision guards, and retained retry results', () => {
  const ctx = app(), api = ctx.NemoApplication;
  assert.equal(api.handle(request(ctx, 'snapshot', 'snapshot')).result.layers[0].id, 'a');
  assert.equal(api.handle(request(ctx, 'get', 'property.get', { layerId: 'a', property: 'opacity' })).result.value, 100);
  const set = request(ctx, 'set-a', 'property.set', { layerId: 'a', property: 'opacity', value: 42 });
  assert.equal(api.handle(set).result.value, 42);
  assert.equal(ctx.pushes, 1);
  assert.deepEqual(api.handle(set), api.handle(set));
  assert.equal(api.handle({ ...set, payload: { ...set.payload, value: 20 } }).error.code, 'invalid_request');
  assert.equal(api.handle({ ...set, requestId: 'stale', expectedRevision: 0 }).error.code, 'stale_revision');
  assert.equal(api.handle({ ...request(ctx, 'cancel', 'property.set', { layerId: 'a', property: 'opacity', value: 9 }), cancelled: true }).error.code, 'cancelled');
});
test('key commands use the opacity writer and record trace entries', () => {
  const ctx = app(), api = ctx.NemoApplication;
  assert.equal(api.handle(request(ctx, 'key', 'property.key.set', { layerId: 'a', property: 'opacity', frame: 8, value: 12 })).ok, true);
  assert.equal(ctx.state.layers[0].motion.opacity.keys[0].v[0], 12);
  assert.equal(api.handle(request(ctx, 'remove', 'property.key.remove', { layerId: 'a', property: 'opacity', frame: 8 })).ok, true);
  assert.equal(api.handle(request(ctx, 'trace', 'diagnostics.trace')).result.entries.length, 2);
});
test('native bootstrap can replace only the provisional instance identity', () => {
  const ctx = app();
  assert.equal(ctx.NemoApplication.setInstanceId('native-id').ok, true);
  assert.equal(ctx.NemoOpacityApplication.meta().instanceId, 'native-id');
  assert.equal(ctx.NemoApplication.setInstanceId('native-id').ok, true);
  assert.equal(ctx.NemoApplication.setInstanceId('other-native-id').error.code, 'INSTANCE_ID_CONFLICT');
});
