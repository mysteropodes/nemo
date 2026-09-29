'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const { extractFunction } = require('./fixtures/lib/sandbox.cjs');
const NativeMotionSurface = require('../src/js/adapters/native-opacity-motion-surface.js');

const app = fs.readFileSync(path.join(__dirname, '../src/js/app.js'), 'utf8');
const navigation = 'var _nativeFrameNavigator=null;\n' +
  extractFunction(app, 'goToFrame');

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function harness(options = {}) {
  const state = { currentFrame: 0, totalFrames: 21, playing: false };
  const identity = { instanceId: 'instance-a', documentId: 'document-a', contentRevision: 0 };
  const input = { value: '1' };
  const calls = [], hostFrames = [], toasts = [];
  const cutover = {
    blocksLegacy: () => true,
    isActive: () => options.active !== false,
    identity: () => ({ ...identity }),
    async presentPreview(frame) {
      const result = options.present ? await options.present(frame) : receipt(frame, identity);
      hostFrames.push(frame);
      return result;
    },
  };
  const window = { NemoNativeOpacityCutover: cutover, _curFrame: 0 };
  const sandbox = {
    state, window, NemoNativeOpacityMotionSurface: NativeMotionSurface,
    document: { getElementById: () => input },
    selectedPaths: [], _nodeSel: [], userLayers: [],
    saveAllLayerFrames: () => calls.push('legacy-save'),
    loadFrame: () => calls.push('legacy-load'),
    clearSel: () => calls.push('legacy-clear'),
    renderOS: () => calls.push('legacy-onion'),
    renderArcs: () => calls.push('legacy-arcs'),
    updateUI: (frameOnly) => {
      calls.push(['ui', state.currentFrame, frameOnly]);
      if (options.updateUI) options.updateUI(state.currentFrame);
    },
    updatePlayhead: () => calls.push('playhead'),
    showToast: (message) => toasts.push(message),
  };
  vm.createContext(sandbox);
  vm.runInContext(navigation, sandbox);
  return { goToFrame: sandbox.goToFrame, state, window, identity, input,
    cutover, calls, hostFrames, toasts };
}

function receipt(frame, identity, status = 'presented') {
  return { owner: 'native', status, frame, ...identity, lifecycleGeneration: 1,
    viewGeneration: frame + 1, workId: `frame-${frame}` };
}

test('native frame navigation publishes only after final presented receipt and never saves Paper', async () => {
  const h = harness();
  h.input.value = '11'; // The focused frame field writes the request before goToFrame.
  const first = h.goToFrame(10);
  assert.equal(h.state.currentFrame, 0);
  assert.equal(h.input.value, 1);
  assert.equal(await first, true);
  assert.equal(h.state.currentFrame, 10);
  assert.equal(h.window._curFrame, 10);
  assert.equal(h.input.value, 11);
  assert.equal(await h.goToFrame(20), true);
  assert.equal(h.state.currentFrame, 20);
  assert.equal(h.input.value, 21);
  assert.equal(h.identity.contentRevision, 0);
  assert.deepEqual(h.hostFrames, [10, 20]);
  assert.deepEqual(h.calls, [['ui', 10, true], ['ui', 20, true]]);
});

test('a same-current request cancels a pending frame and restores the visible native frame', async () => {
  const entered = deferred(), finish = deferred();
  const h = harness({ present: async (frame) => {
    if (frame === 10) { entered.resolve(); await finish.promise; }
    return receipt(frame, h.identity);
  } });
  const older = h.goToFrame(10);
  await entered.promise;
  const latest = h.goToFrame(0);
  finish.resolve();
  assert.equal(await older, false);
  assert.equal(await latest, true);
  assert.equal(h.state.currentFrame, 0);
  assert.equal(h.input.value, 1);
  assert.deepEqual(h.hostFrames, [10, 0]);
  assert.deepEqual(h.calls, [['ui', 0, true]]);
});

test('rapid frame requests commit only the latest presented frame', async () => {
  const entered = deferred(), finish = deferred();
  const h = harness({ present: async (frame) => {
    if (frame === 10) { entered.resolve(); await finish.promise; }
    return receipt(frame, h.identity);
  } });
  const older = h.goToFrame(10);
  await entered.promise;
  const middle = h.goToFrame(11);
  const latest = h.goToFrame(20);
  finish.resolve();
  assert.deepEqual(await Promise.all([older, middle, latest]), [false, false, true]);
  assert.deepEqual(h.hostFrames, [10, 20]);
  assert.equal(h.state.currentFrame, 20);
  assert.deepEqual(h.calls, [['ui', 20, true]]);
});

test('stale revision and failed receipt never publish a requested frame', async () => {
  const entered = deferred(), finish = deferred();
  const h = harness({ present: async (frame) => {
    const captured = { ...h.identity };
    if (frame === 10) { entered.resolve(); await finish.promise; }
    return receipt(frame, captured);
  } });
  const pending = h.goToFrame(10);
  await entered.promise;
  h.identity.contentRevision = 1;
  finish.resolve();
  assert.equal(await pending, false);
  assert.equal(h.state.currentFrame, 0);
  assert.equal(h.window._curFrame, 0);
  assert.equal(h.input.value, 1);
  assert.deepEqual(h.calls, []);

  const failed = harness({ present: async (frame) => receipt(frame, failed.identity,
    frame === 10 ? 'deferred-occluded' : 'presented') });
  assert.equal(await failed.goToFrame(10), false);
  assert.equal(failed.state.currentFrame, 0);
  assert.deepEqual(failed.hostFrames, [10, 0]);
  assert.equal(failed.toasts.length, 1);
});

test('replacement during a pending presentation cannot restore the old document frame', async () => {
  const entered = deferred(), finish = deferred();
  const h = harness({ present: async (frame) => {
    const captured = { ...h.identity };
    if (frame === 10) { entered.resolve(); await finish.promise; }
    return receipt(frame, captured);
  } });
  const pending = h.goToFrame(10);
  await entered.promise;
  h.identity.documentId = 'document-b';
  h.state.currentFrame = 5;
  h.window._curFrame = 5;
  finish.resolve();
  assert.equal(await pending, false);
  assert.equal(h.state.currentFrame, 5);
  assert.equal(h.window._curFrame, 5);
  assert.equal(h.input.value, 6);
  assert.deepEqual(h.hostFrames, [10]); // no stale old-owner rollback request
  assert.deepEqual(h.calls, []);
});

test('UI paint failure restores the previous native frame and invalid input cannot advance', async () => {
  let failOnce = true;
  const h = harness({ updateUI: (frame) => {
    if (frame === 10 && failOnce) { failOnce = false; throw new Error('paint failed'); }
  } });
  assert.equal(await h.goToFrame(10), false);
  assert.equal(h.state.currentFrame, 0);
  assert.equal(h.window._curFrame, 0);
  assert.deepEqual(h.hostFrames, [10, 0]);
  assert.equal(h.input.value, 1);
  assert.equal(await h.goToFrame(10.5), false);
  assert.equal(h.state.currentFrame, 0);
  assert.equal(await h.goToFrame(21), false);
  assert.equal(h.state.currentFrame, 0);
  assert.deepEqual(h.hostFrames, [10, 0]);
});

test('a blocked native document never falls through to the legacy writer', async () => {
  const h = harness({ active: false });
  h.input.value = '11';
  assert.equal(await h.goToFrame(10), false);
  assert.equal(h.state.currentFrame, 0);
  assert.equal(h.input.value, 1);
  assert.deepEqual(h.hostFrames, []);
  assert.deepEqual(h.calls, []);
});

test('legacy frame navigation keeps its existing save/load path', () => {
  const h = harness();
  delete h.window.NemoNativeOpacityCutover;
  h.goToFrame(10);
  assert.equal(h.state.currentFrame, 10);
  assert.equal(h.window._curFrame, 10);
  assert.deepEqual(h.calls, ['legacy-save', 'legacy-load', 'legacy-onion',
    'legacy-arcs', ['ui', 10, true]]);
});
