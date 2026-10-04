'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const { extractFunction } = require('./fixtures/lib/sandbox.cjs');
const surface = require('../src/js/adapters/native-opacity-motion-surface.js');

const timeline = fs.readFileSync(path.join(__dirname, '../src/js/timeline.js'), 'utf8');
const app = fs.readFileSync(path.join(__dirname, '../src/js/app.js'), 'utf8');
const source = 'var playInt=null,playRaf=null,nativePlayScheduler=null,_nativeFrameNavigator=null;\n' +
  ['startNativePlay', 'advancePlayFrame', 'startPlay',
    'autoBakeThenResume', 'manualBakeCache', 'stopPlay', 'togglePlay'].map(name => extractFunction(timeline, name)).join('\n') +
  extractFunction(app, 'goToFrame');

function deferred() {
  let resolve;
  const promise = new Promise(yes => { resolve = yes; });
  return { promise, resolve };
}
function receipt(frame, identity, status = 'presented') {
  return { owner: 'native', status, frame, ...identity, lifecycleGeneration: 1,
    viewGeneration: frame + 1, workId: `frame-${frame}` };
}
function harness(options = {}) {
  let now = 0, nextRaf = 0;
  const rafs = new Map(), hostFrames = [], paints = [], audio = [], legacy = [], toasts = [];
  const identity = { instanceId: 'instance-a', documentId: 'document-a', contentRevision: 0 };
  const state = { playing: false, playDir: 1, currentFrame: 0, totalFrames: 21,
    fps: 10, waIn: 0, waOut: 20, loopPlayback: false, pingPongPlayback: false };
  const playButton = { innerHTML: '', classList: { add() {}, remove() {} } };
  const frameInput = { value: 1 };
  const controller = {
    blocksLegacy: () => options.native !== false,
    isActive: () => options.active !== false,
    identity: () => ({ ...identity }),
    async presentPreview(frame) {
      const result = options.present ? await options.present(frame, identity) : receipt(frame, identity);
      hostFrames.push(frame);
      return result;
    }
  };
  const window = { NemoNativeOpacityCutover: controller,
    SMAudio: { onPlayStart: f => audio.push(['start', f]), onPlayStop: () => audio.push('stop'),
      onLoop: f => audio.push(['loop', f]) }, _curFrame: 0 };
  const sandbox = { window, state, SMAudio: window.SMAudio,
    NemoNativeOpacityMotionSurface: surface,
    document: { getElementById: id => id === 'btn-play' ? playButton : frameInput },
    performance: { now: () => now },
    requestAnimationFrame: fn => { rafs.set(++nextRaf, fn); return nextRaf; },
    cancelAnimationFrame: id => rafs.delete(id), clearInterval() {},
    showToast: message => toasts.push(message),
    updateUI: frameOnly => paints.push([state.currentFrame, frameOnly]),
    updatePlayhead: () => legacy.push('playhead'),
    saveAllLayerFrames: () => legacy.push('save'),
    loadFrame: () => legacy.push('load'),
    renderOS: () => legacy.push('onion'), renderArcs: () => legacy.push('arcs'),
    BrushMenu: null, selectedPaths: [], _nodeSel: [] };
  vm.createContext(sandbox);
  vm.runInContext(source, sandbox);
  async function tick(at) {
    now = at;
    const pending = [...rafs.values()];
    rafs.clear();
    for (const callback of pending) callback(now);
    await new Promise(resolve => setImmediate(resolve));
  }
  return { ...sandbox, controller, identity, hostFrames, paints, audio, legacy, toasts,
    tick, rafs, replace: () => { identity.documentId = 'document-b'; } };
}

test('native Play presents wall-clock frames before publishing UI and never enters legacy cache or Paper', async () => {
  const h = harness();
  h.startPlay();
  await h.tick(1000);
  assert.equal(h.state.currentFrame, 10);
  await h.tick(2000);
  assert.equal(h.state.currentFrame, 20);
  await h.tick(2100);
  assert.equal(h.state.playing, false);
  assert.deepEqual(h.hostFrames, [10, 20, 20]); // Stop reasserts the committed native frame.
  assert.deepEqual(h.paints, [[10, true], [20, true], [20, true], [20, true]]);
  assert.deepEqual(h.legacy, []);
  assert.deepEqual(h.audio, [['start', 0], 'stop']);
  assert.equal(h.identity.contentRevision, 0);
});

test('slow native presentation coalesces elapsed frames and Stop invalidates the old receipt', async () => {
  const entered = deferred(), finish = deferred();
  const h = harness({ present: async (frame, identity) => {
    const captured = { ...identity };
    if (frame === 10) { entered.resolve(); await finish.promise; }
    return receipt(frame, captured);
  } });
  h.startPlay();
  await h.tick(1000);
  await entered.promise;
  await h.tick(1200); // No second host request while the first is outstanding.
  assert.deepEqual(h.hostFrames, []);
  h.stopPlay();
  finish.resolve();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(h.state.currentFrame, 0);
  assert.equal(h.state.playing, false);
  assert.deepEqual(h.hostFrames, [10, 0]);
  assert.deepEqual(h.legacy, []);
});

test('scrub after Stop wins over pending Play; replacement never restores the former owner', async () => {
  const entered = deferred(), finish = deferred();
  const h = harness({ present: async (frame, identity) => {
    const captured = { ...identity };
    if (frame === 10) { entered.resolve(); await finish.promise; }
    return receipt(frame, captured);
  } });
  h.startPlay();
  await h.tick(1000);
  await entered.promise;
  h.stopPlay();
  const scrub = h.goToFrame(5);
  finish.resolve();
  assert.equal(await scrub, true);
  assert.equal(h.state.currentFrame, 5);
  assert.deepEqual(h.hostFrames, [10, 5]);

  const other = harness();
  other.startPlay();
  other.replace();
  await other.tick(100);
  assert.equal(other.state.playing, false);
  assert.deepEqual(other.hostFrames, []);
  assert.deepEqual(other.legacy, []);
});

test('failed native receipt stops without publishing or falling through to legacy writers', async () => {
  const h = harness({ present: async (frame, identity) => receipt(frame, identity,
    frame === 10 ? 'deferred-occluded' : 'presented') });
  h.startPlay();
  await h.tick(1000);
  assert.equal(h.state.playing, false);
  assert.equal(h.state.currentFrame, 0);
  assert.deepEqual(h.legacy, []);
  assert.equal(h.toasts.length, 1);
});

test('rapid Stop and restart cannot publish a presentation from the former run', async () => {
  const entered = deferred(), finish = deferred();
  let first = true;
  const h = harness({ present: async (frame, identity) => {
    const captured = { ...identity };
    if (frame === 10 && first) { first = false; entered.resolve(); await finish.promise; }
    return receipt(frame, captured);
  } });
  h.startPlay();
  await h.tick(1000);
  await entered.promise;
  h.stopPlay();
  h.startPlay();
  finish.resolve();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(h.state.currentFrame, 0);
  await h.tick(2000);
  assert.equal(h.state.currentFrame, 10);
  assert.deepEqual(h.hostFrames, [10, 0, 10]);
  assert.deepEqual(h.legacy, []);
  h.stopPlay();
});

test('single-frame work area stops without changing native document or presenting an out-of-range frame', async () => {
  const h = harness();
  h.state.waOut = 0;
  h.startPlay();
  await h.tick(100);
  assert.equal(h.state.playing, false);
  assert.equal(h.state.currentFrame, 0);
  assert.equal(h.identity.contentRevision, 0);
  assert.deepEqual(h.hostFrames, [0]);
  assert.deepEqual(h.legacy, []);
});

test('native disconnect and revision change end playback without restoring an obsolete frame', async () => {
  const h = harness();
  h.startPlay();
  h.controller.isActive = () => false;
  await h.tick(100);
  assert.equal(h.state.playing, false);
  assert.deepEqual(h.hostFrames, []);
  assert.deepEqual(h.legacy, []);

  const revision = harness();
  revision.startPlay();
  revision.identity.contentRevision = 1;
  await revision.tick(100);
  assert.equal(revision.state.playing, false);
  assert.deepEqual(revision.hostFrames, []);
  assert.deepEqual(revision.legacy, []);
});

test('blocked but inactive native owner cannot start Play or enter legacy playback', async () => {
  const h = harness({ active: false });
  h.startPlay();
  await h.tick(100);
  assert.equal(h.state.playing, false);
  assert.deepEqual(h.hostFrames, []);
  assert.deepEqual(h.legacy, []);
  assert.deepEqual(h.toasts, ['Native playback is unavailable']);
});

test('native cache command is unavailable; legacy Play still takes its existing frame writer', async () => {
  const h = harness();
  h.manualBakeCache();
  assert.equal(h.toasts.length, 1);
  assert.deepEqual(h.legacy, []);
  const legacy = harness({ native: false });
  legacy.startPlay();
  await legacy.tick(100);
  assert.deepEqual(legacy.legacy, ['save', 'load', 'playhead']);
  legacy.stopPlay();
});
