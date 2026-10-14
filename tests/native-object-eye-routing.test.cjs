'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const Surface = require('../src/js/adapters/native-opacity-legacy-surface.js');
const Operations = require('../src/js/application/native-opacity-operations.js');

function harness(project, allow = () => false) {
  const legacy = [], toasts = [];
  const root = {
    state: { layers: [{ layerUid: 'stable-alpha', visible: true }, { layerUid: 'stable-beta', visible: false }] },
    SM: { toggleLayerVis(index) { legacy.push(index); this.lastLegacyIndex = index; return 'legacy'; } },
    showToast(message) { toasts.push(message); },
  };
  const original = root.SM.toggleLayerVis;
  const dispose = Surface.wrapObjectVisibility(root, () => project, allow);
  return { root, original, dispose, legacy, toasts };
}

test('native eye without the synchronized session ABI stays unavailable and never calls the legacy writer', () => {
  const project = { isActive: () => true, info: () => ({ layers: [{ layerUid: 'stable-alpha' }] }) };
  const h = harness(project, () => true);
  assert.equal(h.root.SM.toggleLayerVis(0), false);
  assert.deepEqual(h.legacy, []);
  assert.deepEqual(h.root.state.layers.map(layer => layer.visible), [true, false]);
  h.dispose();
  assert.strictEqual(h.root.SM.toggleLayerVis, h.original);
});

test('the ordinary eye forwards one stable UID only when native UI and owner row identities agree', async () => {
  const calls = [];
  let release;
  const pending = new Promise(resolve => { release = resolve; });
  const project = {
    isActive: () => true,
    info: () => ({ layers: [{ layerUid: 'stable-alpha', visible: true }, { layerUid: 'stable-beta', visible: false }] }),
    toggleLayerVisibility(uid) { calls.push(uid); return pending; },
  };
  const h = harness(project);
  const result = h.root.SM.toggleLayerVis(1);
  assert.strictEqual(result, pending);
  assert.deepEqual(calls, ['stable-beta']);
  assert.deepEqual(h.root.state.layers.map(layer => layer.visible), [true, false], 'the wrapper must not optimistically edit rows');
  h.root.state.layers[1].layerUid = 'later-reordered-row';
  release({ ok: true });
  assert.deepEqual(await result, { ok: true });
  assert.deepEqual(calls, ['stable-beta'], 'a queued intent targets the captured UID, not a later index');
  assert.deepEqual(h.legacy, []);
  h.dispose();
});

test('stale, ambiguous, inactive and malformed eye targets refuse before native dispatch', () => {
  const calls = [];
  const project = {
    isActive: () => true,
    info: () => ({ layers: [{ layerUid: 'stable-alpha' }, { layerUid: 'stable-beta' }] }),
    toggleLayerVisibility(uid) { calls.push(uid); return Promise.resolve({ ok: true }); },
  };
  const h = harness(project);
  for (const index of [-1, 2, 0.5, '0', null]) assert.equal(h.root.SM.toggleLayerVis(index), false);
  h.root.state.layers[0].layerUid = 'stale';
  assert.equal(h.root.SM.toggleLayerVis(0), false);
  h.root.state.layers[0].layerUid = 'stable-alpha';
  project.info = () => ({ layers: [{ layerUid: 'stable-alpha' }, { layerUid: 'stable-alpha' }] });
  assert.equal(h.root.SM.toggleLayerVis(0), false);
  project.isActive = () => false;
  assert.equal(h.root.SM.toggleLayerVis(1), false);
  assert.deepEqual(calls, []);
  assert.deepEqual(h.legacy, []);
  h.dispose();
});

test('rejected native eye intent is visible as a refusal, never retried or sent to the legacy writer', async () => {
  const failure = new Error('stale revision');
  let calls = 0;
  const project = {
    isActive: () => true,
    info: () => ({ layers: [{ layerUid: 'stable-alpha' }, { layerUid: 'stable-beta' }] }),
    toggleLayerVisibility() { calls++; return Promise.reject(failure); },
  };
  const h = harness(project);
  await assert.rejects(h.root.SM.toggleLayerVis(0), /stale revision/);
  await Promise.resolve();
  assert.equal(calls, 1);
  assert.deepEqual(h.legacy, []);
  assert.deepEqual(h.root.state.layers.map(layer => layer.visible), [true, false]);
  assert.deepEqual(h.toasts, ['Native layer visibility was not confirmed.']);
  h.dispose();
});

test('a broken notice surface does not create an unhandled rejection or retry', async () => {
  let calls = 0;
  const project = {
    isActive: () => true,
    info: () => ({ layers: [{ layerUid: 'stable-alpha' }, { layerUid: 'stable-beta' }] }),
    toggleLayerVisibility() { calls++; return Promise.reject(new Error('native refusal')); },
  };
  const h = harness(project);
  h.root.showToast = () => { throw new Error('notice unavailable'); };
  await assert.rejects(h.root.SM.toggleLayerVis(0), /native refusal/);
  await Promise.resolve();
  assert.equal(calls, 1);
  assert.deepEqual(h.legacy, []);
  h.dispose();
});

test('non-object sessions preserve the existing legacy admission decision and guarded disposal', () => {
  const admitted = harness(null, kind => kind === 'timeline-toggleLayerVis');
  assert.equal(admitted.root.SM.toggleLayerVis(1), 'legacy');
  assert.deepEqual(admitted.legacy, [1]);
  admitted.dispose();
  assert.strictEqual(admitted.root.SM.toggleLayerVis, admitted.original);

  const denied = harness(null, () => false);
  assert.equal(denied.root.SM.toggleLayerVis(0), false);
  assert.deepEqual(denied.legacy, []);
  const later = function () { return 'later owner'; };
  denied.root.SM.toggleLayerVis = later;
  denied.dispose();
  assert.strictEqual(denied.root.SM.toggleLayerVis, later, 'disposal must not overwrite a later owner');
});

test('operations installation assigns the eye to the dedicated route exactly once', () => {
  const groups = [], route = [];
  const surface = {
    installGuard() {},
    wrap(target, names) { groups.push({ target, names }); return () => {}; },
    wrapObjectVisibility(project, allow) { route.push({ project, allow }); return () => {}; },
    publish() { return () => {}; },
  };
  const operations = Operations.create({ blocksLegacy: () => false }, { surface }, {},
    { create: () => ({}) }, { create: () => ({}) });
  operations.install().dispose();
  assert.equal(route.length, 1);
  assert.equal(typeof route[0].project, 'function');
  assert.equal(typeof route[0].allow, 'function');
  assert.equal(groups.filter(group => group.target === 'timeline').length, 1);
  assert.equal(groups.some(group => group.names.includes('toggleLayerVis')), false);
});
