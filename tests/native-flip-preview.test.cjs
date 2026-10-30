'use strict';

// D24 (#2358): native playback transition and flip preview, pinned on a fake clock. Playback runs the real shared
// kernel (advancePlaybackFrame) through the real scheduler; Flip preview runs through the real frame navigator.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const { extractFunction } = require('./fixtures/lib/sandbox.cjs');
const surface = require('../src/js/adapters/native-opacity-motion-surface.js');
const flip = require('../src/js/adapters/native-flip-preview.js');
const Targets = require('../src/js/application/shortcut-targets.js');

const timeline = fs.readFileSync(path.join(__dirname, '../src/js/timeline.js'), 'utf8');
const IDENTITY = { instanceId: 'instance-a', documentId: 'document-a', contentRevision: 0 };
const tick = () => new Promise(resolve => setImmediate(resolve));

function owner(overrides = {}) {
  const identity = { ...IDENTITY };
  return Object.assign({ isActive: () => true, identity: () => ({ ...identity }), id: identity }, overrides);
}

// ---- Play: normal tick, late tick, loop boundary, stop/restart on a fake clock --------------------------------------
function playback(options = {}) {
  const state = { playing: false, playDir: 1, currentFrame: options.from ?? 0, fps: 10,
    waIn: options.waIn ?? 0, waOut: options.waOut ?? 20, loopPlayback: !!options.loop, pingPongPlayback: false };
  const audio = [], navigated = [], stops = [], rafs = new Map();
  let now = 0, next = 0, current = owner();
  const scheduler = surface.createPlaybackScheduler({
    frame: () => state.currentFrame, fps: () => state.fps, playing: () => state.playing,
    controller: () => current, advance: cur => surface.advancePlaybackFrame(state, { onLoop: f => audio.push(f) }, cur),
    navigate: frame => { navigated.push(frame); state.currentFrame = frame; return Promise.resolve(true); },
    stop: () => { stops.push(state.currentFrame); state.playing = false; scheduler.stop(); },
    now: () => now, request: fn => { rafs.set(++next, fn); return next; }, cancel: id => rafs.delete(id)
  });
  const api = {
    state, audio, navigated, stops, rafs, scheduler,
    play() { assert.equal(scheduler.start(current), true); state.playing = true; },
    async at(ms) { now = ms; const pending = [...rafs.values()]; rafs.clear(); pending.forEach(fn => fn(ms)); await tick(); },
    clock(ms) { now = ms; },
    replace() { current = owner(); }
  };
  return api;
}

test('Play: a normal tick advances exactly one frame and a tick before the interval advances none', async () => {
  const p = playback();
  p.play();
  await p.at(60);
  assert.deepEqual(p.navigated, []);
  await p.at(100);
  assert.deepEqual(p.navigated, [1]);
  await p.at(200);
  assert.deepEqual(p.navigated, [1, 2]);
  assert.equal(p.state.currentFrame, 2);
});

test('Play: a late tick skips to the frame the wall clock reached, keeps the remainder, and caps a stall at one step', async () => {
  const p = playback();
  p.play();
  await p.at(350); // three frames elapsed
  assert.deepEqual(p.navigated, [3]);
  await p.at(400); // the 50 ms remainder completes the next interval
  assert.deepEqual(p.navigated, [3, 4]);
  await p.at(10000); // a stall of far more than two seconds: one step, clock re-anchored
  assert.deepEqual(p.navigated, [3, 4, 5]);
  await p.at(10050);
  assert.deepEqual(p.navigated, [3, 4, 5], 'the re-anchored clock does not owe the stalled time');
  await p.at(10100);
  assert.deepEqual(p.navigated, [3, 4, 5, 6]);
});

test('Play: the loop wraps at the out point with audio notice; without loop it ends on the edge frame', async () => {
  const looping = playback({ from: 4, waIn: 2, waOut: 4, loop: true });
  looping.play();
  await looping.at(100);
  assert.deepEqual(looping.navigated, [2]);
  assert.deepEqual(looping.audio, [2]);
  await looping.at(500); // four more steps: 3, 4, wrap 2, 3
  assert.deepEqual(looping.navigated, [2, 3]);
  assert.deepEqual(looping.audio, [2, 2]);
  assert.equal(looping.stops.length, 0);

  const once = playback({ from: 3, waIn: 2, waOut: 4 });
  once.play();
  await once.at(300); // 3 -> 4, then the third step is past the out point
  assert.deepEqual(once.navigated, [4]);
  assert.equal(once.stops.length, 1, 'playback ends after landing on the edge');
  assert.equal(once.state.currentFrame, 4);
  assert.equal(once.rafs.size, 0);
});

test('Play: stop then restart never lets the former run navigate, and the new run starts from its own clock', async () => {
  const p = playback();
  p.play();
  const former = [...p.rafs.values()][0];
  p.scheduler.stop();
  p.state.playing = false;
  assert.equal(p.rafs.size, 0, 'the pending animation frame is cancelled');
  former(5000); // a stale callback delivered anyway
  await tick();
  assert.deepEqual(p.navigated, []);
  p.clock(5000);
  p.play();
  await p.at(5100);
  assert.deepEqual(p.navigated, [1], 'the restarted clock begins at its own start time (5000), not the former one');
  await p.at(5200);
  assert.deepEqual(p.navigated, [1, 2]);
  p.replace();
  await p.at(5300);
  assert.deepEqual(p.navigated, [1, 2], 'a replaced native owner ends playback instead of presenting');
  assert.equal(p.state.playing, false);
});

// ---- Flip preview kernel -----------------------------------------------------------------------------------------------
function baselineFlip(start, wa, wo, steps) {
  // The body of SM.flipPreview at 1065e9d8, one interval tick at a time.
  let dir = 1, cur = start; const out = [];
  for (let i = 0; i < steps; i++) {
    cur += dir; if (cur > wo) { dir = -1; cur = wo - 1; } if (cur < wa) { dir = 1; cur = wa + 1; }
    out.push(cur);
  }
  return out;
}

test('flipStep reproduces the baseline back-and-forth sequence for every start inside the work area', () => {
  for (const [wa, wo] of [[0, 1], [0, 2], [2, 5], [3, 12]]) {
    for (let start = wa; start <= wo; start++) {
      const expected = baselineFlip(start, wa, wo, 40);
      let cur = start, dir = 1; const got = [];
      for (let i = 0; i < 40; i++) { const s = flip.flipStep(cur, dir, wa, wo); cur = s.frame; dir = s.dir; got.push(cur); }
      assert.deepEqual(got, expected, `work area ${wa}-${wo} from ${start}`);
    }
  }
  assert.deepEqual(flip.flipStep(4, 1, 4, 4), { frame: 4, dir: 1 }, 'a one-frame work area stays put');
});

// ---- Flip preview on the real navigator, fake timer ---------------------------------------------------------------------
function flipHarness(options = {}) {
  const state = { currentFrame: options.from ?? 3, totalFrames: 20, fps: 10, waIn: 2, waOut: 5 };
  const host = [], intervals = new Map(), toasts = [], cleared = [];
  let nextId = 0, current = options.native === false ? null : owner();
  const controller = () => current;
  const navigator = surface.createFrameNavigator({
    frame: () => state.currentFrame, total: () => state.totalFrames,
    commit: frame => { state.currentFrame = frame; }, paint() {}, syncInput() {},
    fail: () => toasts.push('Could not display this frame'), controller
  });
  const owned = current;
  if (owned) owned.presentPreview = async frame => {
    const result = options.present ? await options.present(frame) : undefined;
    host.push(frame);
    return result || { owner: 'native', status: 'presented', frame, ...owned.identity(), lifecycleGeneration: 1,
      viewGeneration: frame + 1, workId: `w${frame}` };
  };
  const preview = flip.createFlipPreview({
    frame: () => state.currentFrame, fps: () => state.fps, waIn: () => state.waIn, waOut: () => state.waOut,
    controller, navigate: frame => navigator.navigate(controller(), frame),
    setInterval: (fn, ms) => { intervals.set(++nextId, { fn, ms }); return nextId; },
    clearInterval: id => { cleared.push(id); intervals.delete(id); }, notify: message => toasts.push(message)
  });
  return { state, host, intervals, toasts, cleared, preview, owned,
    async fire() { [...intervals.values()].forEach(entry => entry.fn()); await tick(); } };
}

test('Flip preview on a native project presents each flip frame through the navigator at the fps interval and returns on stop', async () => {
  const h = flipHarness({ from: 3 });
  assert.equal(await h.preview.toggle(h.owned), true);
  assert.deepEqual([...h.intervals.values()].map(entry => entry.ms), [100]);
  for (let i = 0; i < 6; i++) await h.fire();
  assert.deepEqual(h.host, baselineFlip(3, 2, 5, 6));
  assert.equal(h.state.currentFrame, h.host[h.host.length - 1]);
  assert.equal(h.preview.active(), true);
  await h.preview.toggle(h.owned);
  assert.equal(h.preview.active(), false);
  assert.equal(h.intervals.size, 0);
  assert.equal(h.state.currentFrame, 3, 'toggled off, the flip lands on the frame it started from');
  assert.equal(h.host[h.host.length - 1], 3);
});

test('Flip preview: a tick during a slow presentation is dropped, not queued, and does not advance the cursor', async () => {
  let release;
  const gate = new Promise(resolve => { release = resolve; });
  const h = flipHarness({ from: 3, present: async frame => { if (frame === 4) await gate; } });
  await h.preview.toggle(h.owned);
  await h.fire(); // frame 4 is in flight
  await h.fire(); await h.fire();
  assert.deepEqual(h.host, [], 'no second request while the first is outstanding');
  release(); await tick(); await tick();
  assert.deepEqual(h.host, [4]);
  await h.fire();
  assert.deepEqual(h.host, [4, 5], 'the next frame follows frame 4 with nothing skipped');
});

test('Flip preview: stopping with a frame in flight leaves the start frame as the last presentation', async () => {
  let release;
  const gate = new Promise(resolve => { release = resolve; });
  const h = flipHarness({ from: 3, present: async frame => { if (frame === 4) await gate; } });
  await h.preview.toggle(h.owned);
  await h.fire();
  const stopped = h.preview.stop();
  release(); await stopped; await tick();
  assert.equal(h.preview.active(), false);
  assert.equal(h.state.currentFrame, 3, 'the abandoned frame 4 is never published');
  assert.equal(h.host[h.host.length - 1], 3);
  await h.fire();
  assert.equal(h.host[h.host.length - 1], 3, 'a stopped flip owes no further frames');
});

test('Flip preview: a replaced native owner ends the flip without presenting or restoring an obsolete frame', async () => {
  const h = flipHarness({ from: 3 });
  await h.preview.toggle(h.owned);
  await h.fire();
  const before = h.host.length;
  h.owned.identity = () => ({ ...IDENTITY, documentId: 'document-b' });
  await h.fire();
  assert.equal(h.preview.active(), false);
  assert.equal(h.intervals.size, 0);
  assert.equal(h.host.length, before);
  assert.deepEqual(await h.preview.stop(), false);
});

test('Flip preview: a refused presentation ends the flip', async () => {
  const h = flipHarness({ from: 3, present: async frame => (frame === 4 ? { owner: 'native', status: 'deferred-occluded' } : undefined) });
  await h.preview.toggle(h.owned);
  await h.fire();
  assert.equal(h.preview.active(), false);
  assert.equal(h.state.currentFrame, 3);
});

test('Flip preview: an inactive native owner refuses before any timer or frame request', async () => {
  const h = flipHarness({ from: 3 });
  h.owned.isActive = () => false;
  assert.equal(await h.preview.toggle(h.owned), false);
  assert.equal(h.intervals.size, 0);
  assert.deepEqual(h.host, []);
  assert.deepEqual(h.toasts, ['Native playback is unavailable']);
});

// ---- Wiring: SM.flipPreview, the native route and the legacy fallback --------------------------------------------------
function wiring(blocksLegacy) {
  const start = timeline.indexOf('  flipPreview:function(){');
  const end = timeline.indexOf('  duplicateKeyframe:function(){');
  assert.ok(start > 0 && end > start);
  const body = '({' + timeline.slice(start, end).trim().replace(/,$/, '') + '})';
  const calls = [], intervals = [];
  const window = { NemoNativeOpacityCutover: blocksLegacy === null ? undefined : { blocksLegacy: () => blocksLegacy, isActive: () => true, identity: () => ({ ...IDENTITY }) },
    setInterval: (fn, ms) => { calls.push(['window.setInterval', ms]); return 1; }, clearInterval() {} };
  const sandbox = { window, state: { currentFrame: 0, waIn: 0, waOut: 3, fps: 10 },
    goToFrame: frame => { calls.push(['goToFrame', frame]); return Promise.resolve(true); },
    setInterval: (fn, ms) => { intervals.push(ms); calls.push(['legacy.setInterval', ms]); return 1; }, clearInterval() {},
    toggleNativeFlipPreview: controller => { calls.push(['native', controller === window.NemoNativeOpacityCutover]); } };
  vm.createContext(sandbox);
  return { calls, intervals, SM: vm.runInContext(body, sandbox) };
}

test('SM.flipPreview routes to the native flip when native blocks legacy, and keeps the baseline body otherwise', () => {
  const nat = wiring(true);
  nat.SM.flipPreview();
  assert.deepEqual(nat.calls, [['native', true]], 'no legacy timer or frame write on a native project');
  const legacy = wiring(false);
  legacy.SM.flipPreview();
  assert.deepEqual(legacy.intervals, [100]);
  const none = wiring(null);
  none.SM.flipPreview();
  assert.deepEqual(none.intervals, [100]);
});

test('toggleNativeFlipPreview builds the adapter over the frame navigator and the live work area', async () => {
  const fn = extractFunction(timeline, 'toggleNativeFlipPreview');
  const calls = [];
  const controller = owner();
  const window = { NemoNativeOpacityCutover: controller, setInterval: (f, ms) => { calls.push(['interval', ms]); return 7; },
    clearInterval: id => calls.push(['clear', id]) };
  const sandbox = { window, state: { currentFrame: 1, fps: 20, waIn: 0, waOut: 6 }, NemoNativeFlipPreview: flip,
    goToFrame: frame => { calls.push(['go', frame]); return Promise.resolve(true); }, showToast: message => calls.push(['toast', message]) };
  vm.createContext(sandbox);
  vm.runInContext('var nativeFlipPreview=null;\n' + fn, sandbox);
  await sandbox.toggleNativeFlipPreview(controller);
  await sandbox.toggleNativeFlipPreview(controller);
  assert.deepEqual(calls, [['interval', 50], ['clear', 7], ['go', 1]]);
});

// ---- Flip and Play are mutually exclusive (#2358) -------------------------------------------------------------------------
function exclusive(options = {}) {
  const harness = flipHarness({ from: 3 });
  const log = [];
  const play = { playing: !!options.playing };
  // Rebuild the preview with the two exclusion ports over the same navigator the harness uses.
  const preview = flip.createFlipPreview({
    frame: () => harness.state.currentFrame, fps: () => harness.state.fps, waIn: () => harness.state.waIn, waOut: () => harness.state.waOut,
    controller: () => harness.owned, navigate: frame => { log.push(['go', frame]); harness.state.currentFrame = frame; return Promise.resolve(true); },
    playing: () => play.playing, stopPlay: () => { log.push(['stopPlay', harness.state.currentFrame]); play.playing = false; },
    setInterval: (fn, ms) => { harness.intervals.set(99, { fn, ms }); return 99; }, clearInterval: id => { harness.intervals.delete(id); },
    notify: message => harness.toasts.push(message)
  });
  return { harness, preview, play, log };
}

test('Flip start stops a running Play first and takes its origin from the frame Play left, with no stale frame', async () => {
  const e = exclusive({ playing: true });
  e.harness.state.currentFrame = 4; // Play had advanced to 4
  assert.equal(await e.preview.toggle(e.harness.owned), true);
  assert.deepEqual(e.log, [['stopPlay', 4]]);
  assert.equal(e.play.playing, false);
  await e.preview.toggle(e.harness.owned);
  assert.deepEqual(e.log[e.log.length - 1], ['go', 4], 'toggling off returns to the frame the flip started on');
});

test('Flip start without a running Play never calls stopPlay, and a refused start does not stop Play', async () => {
  const idle = exclusive();
  await idle.preview.toggle(idle.harness.owned);
  assert.deepEqual(idle.log, []);
  const refused = exclusive({ playing: true });
  refused.harness.state.waOut = refused.harness.state.waIn; // nothing to flip
  assert.equal(await refused.preview.toggle(refused.harness.owned), false);
  assert.equal(refused.play.playing, true, 'a flip that cannot start leaves Play running');
});

test('cancel ends a running flip without presenting any frame and fences its in-flight result', async () => {
  const e = exclusive();
  await e.preview.toggle(e.harness.owned);
  [...e.harness.intervals.values()].forEach(entry => entry.fn());
  const before = e.log.length;
  assert.equal(e.preview.cancel(), true);
  await tick();
  assert.equal(e.preview.active(), false);
  assert.equal(e.harness.intervals.size, 0);
  assert.equal(e.log.length, before, 'cancel presents nothing (no origin restore racing the Play that follows)');
  assert.equal(e.preview.cancel(), false, 'cancelling an idle flip is a no-op');
});

test('F then Space: the flip ends and Play runs; Space then F: Play ends and the flip runs (real timeline wiring)', async () => {
  const toggle = extractFunction(timeline, 'toggleNativeFlipPreview');
  const startPlayFn = extractFunction(timeline, 'startNativePlay');
  const calls = [], intervals = new Map();
  const controller = owner();
  const state = { currentFrame: 3, fps: 10, waIn: 2, waOut: 5, playing: false, playDir: 1 };
  const el = { innerHTML: '', classList: { add() {}, remove() {} } };
  const sandbox = {
    window: { NemoNativeOpacityCutover: controller, setInterval: (fn, ms) => { intervals.set(1, fn); return 1; },
      clearInterval: id => { intervals.delete(id); }, requestAnimationFrame: () => 1, cancelAnimationFrame() {} },
    state, NemoNativeFlipPreview: flip, performance: { now: () => 0 },
    document: { getElementById: () => el },
    NemoNativeOpacityMotionSurface: { createPlaybackScheduler: () => ({ start: () => { calls.push('playStart'); return true; }, stop: () => {} }) },
    goToFrame: frame => { calls.push(['go', frame]); state.currentFrame = frame; return Promise.resolve(true); },
    advancePlayFrame() {}, showToast: message => calls.push(['toast', message]),
    stopPlay: () => { calls.push('stopPlay'); state.playing = false; }
  };
  vm.createContext(sandbox);
  vm.runInContext('var nativePlayScheduler=null;var nativeFlipPreview=null;\n' + toggle + '\n' + startPlayFn, sandbox);
  // F then Space.
  await sandbox.toggleNativeFlipPreview(controller);
  assert.equal(intervals.size, 1);
  intervals.get(1)();
  await tick();
  sandbox.startNativePlay(controller);
  assert.equal(intervals.size, 0, 'Play start cancelled the running flip');
  assert.equal(state.playing, true);
  assert.deepEqual(calls.filter(c => c === 'playStart' || c === 'stopPlay'), ['playStart']);
  const goes = calls.filter(c => Array.isArray(c) && c[0] === 'go').length;
  await tick();
  assert.equal(calls.filter(c => Array.isArray(c) && c[0] === 'go').length, goes, 'no stale flip frame after Play starts');
  // Space then F.
  calls.length = 0;
  await sandbox.toggleNativeFlipPreview(controller);
  assert.deepEqual(calls.slice(0, 1), ['stopPlay'], 'flip start stops Play first');
  assert.equal(state.playing, false);
  assert.equal(intervals.size, 1, 'the flip runs');
});

test('the shortcut table declares Flip preview as a native route', () => {
  assert.equal(Targets.target({}, 'cmdFlipPreview').kind, 'native');
});
