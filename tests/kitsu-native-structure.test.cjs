'use strict';
// CS8-D08 I44 (#2381): the Kitsu Rough/Clean/Color structure is built through native layer.rename and layer.create only
// (real script writes + real lifecycle intent + real common v2 SDK; the host and the Drawing Add controller are doubles),
// confirmed against the native read model, and never touches state.layers.
const test = require('node:test'), assert = require('node:assert/strict');
const { createHarness } = require('./native-object-edit-harness.cjs');
const Structure = require('../src/js/application/kitsu-native-structure.js');
const Writes = require('../src/js/application/script-native-writes.js');
const Intent = require('../src/js/adapters/native-object-layer-lifecycle-intent.js'), Edit = require('../src/js/adapters/native-object-edit-contract.js');
const P = 'command.document.object.';

function world({ addAnswers, renameError, selectThrows } = {}) {
  const names = { 'layer-1': 'Layer 1' };
  const h = createHarness({ layers: ['layer-1'], step: (r, st) => {
    if (r.operation === P + 'layer.rename') {
      if (renameError && r.payload.name === renameError) return { error: { code: 'not_found', message: 'no' } };
      names[r.payload.layerUid] = r.payload.name;
    }
    return { applied: true };
  } });
  const legacy = [], selected = [];
  let n = 1;
  const info = () => ({ instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 1, contentRevision: h.state.revision,
    documentSnapshotId: 's' + h.state.revision, layers: h.state.layers.map((layerUid) => ({ layerUid, name: names[layerUid] })) });
  const session = { isActive: () => true, info, requestId: h.ports.requestId,
    runResourceCommand: (action) => action({
      dispatch: async (request) => { const r = await h.adapter.dispatch(request); if (!r.ok) throw new Error('Native command refused: ' + r.error.code); return r; },
      reconcile: async () => info() }) };
  const answers = addAnswers ? addAnswers.slice() : null;
  const state = new Proxy({ currentFrame: 0, layers: [{ frames: [] }] }, { set(t, k, v) { legacy.push('state.' + String(k)); return Reflect.set(t, k, v); } });
  const root = { state,
    NemoNativeObjectLayerLifecycleIntent: Intent, NemoNativeObjectEditContract: Edit, NemoScriptNativeWrites: Writes,
    createUserLayer() { legacy.push('createUserLayer'); },
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI(rt, index, extend, uid) { if (selectThrows) throw new Error('x'); selected.push([index, uid]); return true; } },
    NemoNativeDrawingAddController: { add: async () => {
      if (answers && answers.length) { const a = answers.shift(); if (a) return a; }
      n++; const uid = 'layer-' + n; h.state.layers.push(uid); names[uid] = 'Layer 1'; h.state.revision += 1;
      return { status: 'created-selected', layerUid: uid };
    } },
    setTimeout: (fn) => setTimeout(fn, 1) };
  return { root, h, names, legacy, selected, state };
}

test('Rough, Clean and Color are built natively: one rename of Layer 1, two creates, two renames, read back from the model', async () => {
  const w = world();
  const built = await Structure.build(w.root);
  assert.deepEqual(built.layers.map((l) => l.name), ['Rough', 'Clean', 'Color']);
  assert.deepEqual(w.h.state.layers, ['layer-1', 'layer-2', 'layer-3']);
  const renames = w.h.state.sent.filter((r) => r.operation === P + 'layer.rename').map((r) => r.payload);
  assert.deepEqual(renames, [
    { command: 'object.layer.rename', layerUid: 'layer-1', name: 'Rough' },
    { command: 'object.layer.rename', layerUid: 'layer-2', name: 'Clean' },
    { command: 'object.layer.rename', layerUid: 'layer-3', name: 'Color' }]);
  assert.deepEqual(w.legacy, [], 'no state.layers or createUserLayer write');
  assert.deepEqual(w.selected, [[0, 'layer-1']], 'the baseline leaves Rough active');
});

test('a busy Drawing Add is retried; a refused rename throws naming its step and later steps never run', async () => {
  let w = world({ addAnswers: [{ status: 'busy' }, { status: 'busy' }] });
  assert.deepEqual((await Structure.build(w.root)).layers.map((l) => l.name), ['Rough', 'Clean', 'Color']);
  w = world({ renameError: 'Clean' });
  await assert.rejects(Structure.build(w.root), /rename Clean/);
  assert.deepEqual(w.h.state.layers, ['layer-1', 'layer-2'], 'Color was never created');
  assert.deepEqual(w.legacy, []);
  w = world({ addAnswers: [{ status: 'unconfirmed' }] });
  await assert.rejects(Structure.build(w.root), /create Clean.*unconfirmed/);
  assert.equal(w.h.state.sent.filter((r) => r.operation === P + 'layer.rename').length, 1, 'an unconfirmed create is not retried');
});

test('an inactive session, a missing writes module or a multi-layer start is refused before any write', async () => {
  let w = world(); delete w.root.NemoScriptNativeWrites;
  await assert.rejects(Structure.build(w.root), /indisponible/);
  w = world(); w.root.NemoNativeObjectProject.desktop = () => ({ isActive: () => false });
  await assert.rejects(Structure.build(w.root), /indisponible/);
  w = world(); w.h.state.layers.push('layer-x');
  await assert.rejects(Structure.build(w.root), /layer 1/);
  assert.deepEqual(w.h.state.sent, []);
});

test('a selection failure after the build does not fail the open', async () => {
  const w = world({ selectThrows: true });
  assert.equal((await Structure.build(w.root)).layers.length, 3);
});
