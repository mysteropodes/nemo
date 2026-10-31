'use strict';
// P24 (#1026): the playback frame-transition kernel, its timeline wrapper and the native scheduler
// under an injected fixed clock. Headless; no app, no Paper, no real timers except where noted.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const { extractFunction } = require('./fixtures/lib/sandbox.cjs');
const surface = require('../src/js/adapters/native-opacity-motion-surface.js');
const { createObserver } = require('./desktop/play-publication-observer-bootstrap.js');
const { verifyFixedClockSteps, expectedFrames } = require('./desktop/p24-fixed-clock-play.cjs');

const timeline = fs.readFileSync(path.join(__dirname, '../src/js/timeline.js'), 'utf8');

// Independent reference: advancePlayFrame verbatim from baseline 1065e9d8 (src/js/timeline.js:38-49).
function baselineAdvance(state, audio, cur) {
  var next = cur + state.playDir;
  if (next > state.waOut) {
    if (state.loopPlayback && state.pingPongPlayback) { state.playDir = -1; next = cur - 1; if (next < state.waIn) next = state.waIn; }
    else if (state.loopPlayback) { next = state.waIn; if (audio) audio.onLoop(next); }
    else return null;
  } else if (next < state.waIn) {
    if (state.loopPlayback && state.pingPongPlayback) { state.playDir = 1; next = cur + 1; if (next > state.waOut) next = state.waOut; }
    else return null;
  }
  return next;
}
function run(fn, cur, over) {
  const state = { playDir: 1, waIn: 2, waOut: 5, loopPlayback: false, pingPongPlayback: false, ...over };
  const loops = [];
  const next = fn(state, { onLoop: f => loops.push(f) }, cur);
  return { next, playDir: state.playDir, loops };
}

test('forward edge: interior steps advance by one, the last frame is reachable, nothing else mutates', () => {
  assert.deepEqual(run(surface.advancePlaybackFrame, 3), { next: 4, playDir: 1, loops: [] });
  assert.deepEqual(run(surface.advancePlaybackFrame, 4), { next: 5, playDir: 1, loops: [] });
  assert.deepEqual(run(surface.advancePlaybackFrame, 4, { loopPlayback: true }), { next: 5, playDir: 1, loops: [] });
});

test('non-loop stop: stepping past either edge ends playback and leaves direction and audio alone', () => {
  assert.deepEqual(run(surface.advancePlaybackFrame, 5), { next: null, playDir: 1, loops: [] });
  assert.deepEqual(run(surface.advancePlaybackFrame, 2, { playDir: -1 }), { next: null, playDir: -1, loops: [] });
  assert.deepEqual(run(surface.advancePlaybackFrame, 5, { pingPongPlayback: true }), { next: null, playDir: 1, loops: [] });
});

test('normal wrap: loop returns to the work-area start and notifies audio exactly once', () => {
  assert.deepEqual(run(surface.advancePlaybackFrame, 5, { loopPlayback: true }), { next: 2, playDir: 1, loops: [2] });
  // No audio sink is a legal configuration (window.SMAudio absent).
  const state = { playDir: 1, waIn: 2, waOut: 5, loopPlayback: true, pingPongPlayback: false };
  assert.equal(surface.advancePlaybackFrame(state, undefined, 5), 2);
});

test('ping-pong reversal: reverses beyond each edge without an audio loop call', () => {
  const ping = { loopPlayback: true, pingPongPlayback: true };
  assert.deepEqual(run(surface.advancePlaybackFrame, 5, ping), { next: 4, playDir: -1, loops: [] });
  assert.deepEqual(run(surface.advancePlaybackFrame, 2, { ...ping, playDir: -1 }), { next: 3, playDir: 1, loops: [] });
  assert.deepEqual(run(surface.advancePlaybackFrame, 3, { ...ping, playDir: -1 }), { next: 2, playDir: -1, loops: [] });
});

test('single-frame work area: never leaves the frame, ping-pong flips, loop re-notifies, plain play stops', () => {
  const one = { waIn: 3, waOut: 3 };
  assert.deepEqual(run(surface.advancePlaybackFrame, 3, one), { next: null, playDir: 1, loops: [] });
  assert.deepEqual(run(surface.advancePlaybackFrame, 3, { ...one, loopPlayback: true }), { next: 3, playDir: 1, loops: [3] });
  assert.deepEqual(run(surface.advancePlaybackFrame, 3, { ...one, loopPlayback: true, pingPongPlayback: true }), { next: 3, playDir: -1, loops: [] });
  assert.deepEqual(run(surface.advancePlaybackFrame, 3, { ...one, loopPlayback: true, pingPongPlayback: true, playDir: -1 }), { next: 3, playDir: 1, loops: [] });
});

test('kernel equals the baseline step for every work area, frame, direction and mode (return, direction, onLoop)', () => {
  let cases = 0;
  for (let waIn = 0; waIn <= 3; waIn++) for (let waOut = waIn; waOut <= waIn + 3; waOut++)
    for (let cur = waIn; cur <= waOut; cur++) for (const playDir of [1, -1])
      for (const loopPlayback of [false, true]) for (const pingPongPlayback of [false, true]) {
        const over = { waIn, waOut, playDir, loopPlayback, pingPongPlayback };
        assert.deepEqual(run(surface.advancePlaybackFrame, cur, over), run(baselineAdvance, cur, over), JSON.stringify({ cur, ...over }));
        cases++;
      }
  assert.equal(cases, 320);
});

// -- the real timeline.js wrapper and Play entry points -----------------------------------------
function wrapperHarness(kernelSpy) {
  const state = { playing: false, playDir: 1, currentFrame: 0, fps: 24, waIn: 0, waOut: 3, loopPlayback: true, pingPongPlayback: false };
  const audio = { loops: [], onLoop(f) { this.loops.push(f); }, onPlayStart() {} };
  const captured = { ports: null };
  const window = { SMAudio: audio, NemoNativeOpacityCutover: { blocksLegacy: () => true },
    requestAnimationFrame: () => 1, cancelAnimationFrame() {} };
  const spy = { createPlaybackScheduler: ports => { captured.ports = ports; return { start: () => true, stop: () => ({}) }; },
    advancePlaybackFrame: (...args) => kernelSpy(...args) };
  const goToFrame = () => {};
  const sandbox = { window, state, NemoNativeOpacityMotionSurface: spy, SMAudio: audio, goToFrame, stopPlay() {},
    showToast() {}, performance: { now: () => 0 },
    document: { getElementById: () => ({ innerHTML: '', classList: { add() {}, remove() {} } }) },
    nativeFlipPreview: null };
  vm.createContext(sandbox);
  vm.runInContext('var nativePlayScheduler=null;\n' + ['startNativePlay', 'advancePlayFrame', 'startPlay']
    .map(name => extractFunction(timeline, name)).join('\n'), sandbox);
  return { sandbox, state, audio, captured, goToFrame };
}

test('wrapper passes the live state and SMAudio to the kernel and returns its answer; startPlay reaches it through the scheduler', () => {
  const calls = [];
  const h = wrapperHarness((...args) => { calls.push(args); return 7; });
  assert.equal(h.sandbox.advancePlayFrame(3), 7);
  assert.equal(calls.length, 1);
  assert.equal(calls[0][0], h.state);
  assert.equal(calls[0][1], h.audio);
  assert.equal(calls[0][2], 3);
  h.sandbox.startPlay();
  assert.equal(h.state.playing, true);
  assert.equal(h.state.playDir, 1);
  assert.equal(h.captured.ports.advance(2), 7, 'scheduler advance port is the kernel wrapper');
  assert.equal(calls.length, 2);
  assert.equal(h.captured.ports.navigate, h.goToFrame, 'navigate is goToFrame, the only frame writer');
  assert.equal(h.captured.ports.fps(), 24);
});

test('wrapper with the real kernel mutates playDir and calls SMAudio.onLoop exactly as the baseline step', () => {
  const real = wrapperHarness((...args) => surface.advancePlaybackFrame(...args));
  const ref = { state: { ...real.state, playDir: 1 }, loops: [] };
  const seqReal = [], seqRef = [];
  real.state.pingPongPlayback = false;
  for (let cur = 0, i = 0; i < 12; i++) {
    cur = real.sandbox.advancePlayFrame(cur); seqReal.push([cur, real.state.playDir]);
  }
  for (let cur = 0, i = 0; i < 12; i++) {
    cur = baselineAdvance(ref.state, { onLoop: f => ref.loops.push(f) }, cur); seqRef.push([cur, ref.state.playDir]);
  }
  assert.deepEqual(seqReal, seqRef);
  assert.deepEqual(real.audio.loops, ref.loops);
  assert.deepEqual(real.audio.loops, [0, 0, 0]);
});

// -- the scheduler under an injected fixed clock --------------------------------------------------
function fixedClockScheduler({ fps = 24, waIn = 0, waOut = 5, loop = false, ping = false, start = 0 } = {}) {
  let now = 1000, pendingStep = null, nextId = 0;
  const state = { playDir: 1, waIn, waOut, loopPlayback: loop, pingPongPlayback: ping };
  const written = [], loops = [], stops = [], advances = [];
  let frame = start, playing = true;
  const owner = { isActive: () => true, identity: () => ({ instanceId: 'i', documentId: 'd', contentRevision: 0 }) };
  const scheduler = surface.createPlaybackScheduler({
    frame: () => frame, fps: () => fps, playing: () => playing, controller: () => owner,
    advance: cur => { const before = frame; const n = surface.advancePlaybackFrame(state, { onLoop: f => loops.push(f) }, cur);
      assert.equal(frame, before, 'advance must not write the frame'); advances.push(n); return n; },
    navigate: async n => { written.push(n); frame = n; return true; },
    stop: () => { stops.push(frame); playing = false; scheduler.stop(); },
    now: () => now, request: step => { pendingStep = step; return ++nextId; }, cancel: () => { pendingStep = null; }
  });
  async function advanceMs(ms) {
    now += ms; const step = pendingStep; pendingStep = null;
    if (step) step(now);
    await new Promise(resolve => setImmediate(resolve));
  }
  return { scheduler, owner, state, written, loops, stops, advanceMs, frame: () => frame, active: () => pendingStep !== null };
}

test('fixed clock at the project fps: one frame per period, navigate is the only writer, then a plain stop', async () => {
  for (const fps of [12, 24, 30]) {
    const h = fixedClockScheduler({ fps, waOut: 5 });
    assert.equal(h.scheduler.start(h.owner), true);
    const period = 1000 / fps;
    await h.advanceMs(period * 0.5);
    assert.deepEqual(h.written, [], 'half a period presents nothing');
    await h.advanceMs(period * 0.5 + 1e-6);
    for (let i = 0; i < 4; i++) await h.advanceMs(period + 1e-6);
    assert.deepEqual(h.written, [1, 2, 3, 4, 5], `fps ${fps}`);
    assert.deepEqual(h.stops, []);
    await h.advanceMs(period + 1e-6);
    assert.deepEqual(h.stops, [5], 'past the last frame the scheduler stops once at the final frame');
    assert.deepEqual(h.written, [1, 2, 3, 4, 5]);
  }
});

test('fixed clock: a stall coalesces elapsed periods into one navigate (frame dropping unchanged)', async () => {
  const h = fixedClockScheduler({ fps: 24, waOut: 20 });
  h.scheduler.start(h.owner);
  await h.advanceMs(3 * (1000 / 24) + 1e-6);
  assert.deepEqual(h.written, [3]);
});

test('fixed clock: loop wraps through the kernel with one SMAudio.onLoop per wrap; ping-pong reverses', async () => {
  const period = 1000 / 24 + 1e-6;
  const wrap = fixedClockScheduler({ waOut: 2, loop: true });
  wrap.scheduler.start(wrap.owner);
  for (let i = 0; i < 7; i++) await wrap.advanceMs(period);
  assert.deepEqual(wrap.written, [1, 2, 0, 1, 2, 0, 1]);
  assert.deepEqual(wrap.loops, [0, 0]);
  const ping = fixedClockScheduler({ waOut: 2, loop: true, ping: true });
  ping.scheduler.start(ping.owner);
  for (let i = 0; i < 6; i++) await ping.advanceMs(period);
  assert.deepEqual(ping.written, [1, 2, 1, 0, 1, 2]);
  assert.deepEqual(ping.loops, []);
});

test('fixed clock: single-frame work area never presents a frame and stops', async () => {
  const h = fixedClockScheduler({ waIn: 4, waOut: 4, start: 4 });
  h.scheduler.start(h.owner);
  await h.advanceMs(1000 / 24 + 1e-6);
  assert.deepEqual(h.written, []);
  assert.deepEqual(h.stops, [4]);
});

// -- the opt-in test clock inside the real startNativePlay ----------------------------------------
function observerHarness({ fixedClock }) {
  const events = [], sequences = [];
  const observer = createObserver({ runId: '00000000-0000-4000-8000-000000000001', fixedClock }, {
    invoke: async (action, payload) => { if (action === 'record') { events.push(payload.event); sequences.push(payload.sequence); return { sequence: payload.sequence }; } return {}; },
    listen: async () => () => {} });
  return { observer, events, sequences };
}
async function playUnderClock({ fixedClock, waOut, loop = false, ping = false, documentId = 'native-document-1', stopAfter = 0, extra = {} }) {
  const { observer, events, sequences } = observerHarness({ fixedClock });
  await observer.ready;
  const state = { playing: false, playDir: 1, currentFrame: 0, fps: 24, waIn: 0, waOut, loopPlayback: loop, pingPongPlayback: ping };
  const written = [], rafs = [];
  const owner = { isActive: () => true, blocksLegacy: () => true,
    identity: () => ({ instanceId: '00000000-0000-4000-8000-000000000002', documentId, contentRevision: 0, ...extra }) };
  const window = { NemoTestPlayObserver: observer, NemoNativeOpacityCutover: owner,
    SMAudio: { onPlayStart() {}, onLoop() {} },
    requestAnimationFrame: fn => { rafs.push(fn); return rafs.length; }, cancelAnimationFrame() {} };
  globalThis.window = window; // the adapter reads the observer from its own global
  let finish; const stopped = new Promise(resolve => { finish = resolve; });
  const sandbox = { window, state, SMAudio: window.SMAudio, NemoNativeOpacityMotionSurface: surface, performance: { now: () => 0 },
    goToFrame: async n => { written.push(n); state.currentFrame = n; if (stopAfter && written.length === stopAfter) sandbox.stopPlay(); return true; },
    stopPlay: () => { state.playing = false; sandbox.nativePlayScheduler.stop(); finish(); },
    showToast() {}, nativeFlipPreview: null,
    document: { getElementById: () => ({ innerHTML: '', classList: { add() {}, remove() {} } }) } };
  vm.createContext(sandbox);
  vm.runInContext('var nativePlayScheduler=null;\n' + ['startNativePlay', 'advancePlayFrame']
    .map(name => extractFunction(timeline, name)).join('\n'), sandbox);
  try {
    sandbox.startNativePlay(owner);
    if (fixedClock) await Promise.race([stopped, new Promise((_, reject) => setTimeout(() => reject(new Error('timeout')), 5000))]);
    await observer.ready; await new Promise(resolve => setImmediate(resolve));
    return { state, written, events, sequences, rafs, observer };
  } finally { delete globalThis.window; }
}

test('observer fixedClock drives the real startNativePlay one frame per tick and the verifier accepts the published steps', async () => {
  const r = await playUnderClock({ fixedClock: true, waOut: 6 });
  assert.deepEqual(r.written, [1, 2, 3, 4, 5, 6]);
  assert.equal(r.rafs.length, 0, 'requestAnimationFrame is not used while the test clock is active');
  const verdict = verifyFixedClockSteps(r.events, { start: 0, waIn: 0, waOut: 6 });
  assert.deepEqual(verdict.failures, []);
  assert.deepEqual(verdict.observed, [1, 2, 3, 4, 5, 6]);
  assert.deepEqual(expectedFrames({ start: 0, waIn: 0, waOut: 6 }), verdict.observed);
});

test('verifier rejects a skipped or repeated frame and a failed step', () => {
  const step = (frame, result) => ({ event: 'play-step', frame, ...(result === undefined ? {} : { result }) });
  const good = [step(1), step(1, true), step(2), step(2, true)];
  assert.equal(verifyFixedClockSteps(good, { start: 0, waIn: 0, waOut: 2 }).ok, true);
  assert.equal(verifyFixedClockSteps([step(1), step(1, true), step(3), step(3, true)], { start: 0, waIn: 0, waOut: 3 }).ok, false);
  assert.equal(verifyFixedClockSteps([step(1), step(1, true), step(1), step(1, true)], { start: 0, waIn: 0, waOut: 3 }).ok, false);
  assert.equal(verifyFixedClockSteps([step(1), step(1, false)], { start: 0, waIn: 0, waOut: 3 }).ok, false);
  assert.equal(verifyFixedClockSteps([], { start: 0, waIn: 0, waOut: 3 }).ok, false);
  // A non-looping run stopped early (operator Stop or stall) must not pass on its matching prefix.
  assert.equal(verifyFixedClockSteps([step(1), step(1, true)], { start: 0, waIn: 0, waOut: 3 }).ok, false);
  // A looping run must cover at least one wrap of the work area unless --steps says otherwise.
  const wrap = [1, 2, 3, 0].flatMap(f => [step(f), step(f, true)]);
  assert.equal(verifyFixedClockSteps(wrap, { start: 0, waIn: 0, waOut: 3, loop: true }).ok, true);
  assert.equal(verifyFixedClockSteps(wrap.slice(0, 4), { start: 0, waIn: 0, waOut: 3, loop: true }).ok, false);
  assert.equal(verifyFixedClockSteps(wrap.slice(0, 4), { start: 0, waIn: 0, waOut: 3, loop: true, steps: 2 }).ok, true);
});

test('without the opt-in flag the observer exposes no clock and Play keeps requestAnimationFrame', async () => {
  const r = await playUnderClock({ fixedClock: false, waOut: 6 });
  assert.equal(r.observer.fixedClock, null);
  assert.equal(r.rafs.length, 1);
  assert.deepEqual(r.written, []);
});

test('the test hook is absent from normal product wiring: only the observer bootstrap defines fixedClock', () => {
  const index = fs.readFileSync(path.join(__dirname, '../src/index.html'), 'utf8');
  assert.ok(!/fixedClock|NemoTestPlayObserver/.test(index));
  const adapters = fs.readdirSync(path.join(__dirname, '../src/js/adapters'));
  for (const file of adapters) assert.ok(!/fixedClock/.test(fs.readFileSync(path.join(__dirname, '../src/js/adapters', file), 'utf8')), file);
  assert.equal((timeline.match(/fixedClock/g) || []).length, 1, 'timeline reads the flag only inside startNativePlay');
});

// -- native object projects (#1026): ids 'native-object-document-N' reach the collector --------------
const OBJECT_DOC = 'native-object-document-7';

test('object project Play under the fixed clock: 0..W loop run publishes gap-free play-step pairs the verifier accepts', async () => {
  const r = await playUnderClock({ fixedClock: true, waOut: 3, loop: true, documentId: OBJECT_DOC, stopAfter: 9 });
  assert.deepEqual(r.written, [1, 2, 3, 0, 1, 2, 3, 0, 1]);
  assert.equal(r.rafs.length, 0);
  assert.equal(r.observer.inspect().dropped, 0, 'no object-project event is dropped by the bootstrap');
  assert.deepEqual(r.sequences, r.sequences.map((_, i) => i + 1), 'the sink sees a gap-free sequence');
  assert.ok(r.events.some(e => e.event === 'play-start' && e.documentId === OBJECT_DOC));
  assert.ok(r.events.some(e => e.event === 'play-state' && e.playing === true));
  const verdict = verifyFixedClockSteps(r.events, { start: 0, waIn: 0, waOut: 3, loop: true });
  assert.deepEqual(verdict.failures, []);
  assert.deepEqual(verdict.observed, r.written);
});

test('object project Play under the fixed clock: a no-loop run publishes 1..W and ends with a plain stop', async () => {
  const r = await playUnderClock({ fixedClock: true, waOut: 4, documentId: OBJECT_DOC });
  assert.equal(r.observer.inspect().dropped, 0);
  const verdict = verifyFixedClockSteps(r.events, { start: 0, waIn: 0, waOut: 4 });
  assert.deepEqual(verdict.failures, []);
  assert.deepEqual(verdict.observed, [1, 2, 3, 4]);
});

test('object project Play: the identity is the whole project info, and only its pins reach the collector', async () => {
  // A native object project's identity() is its project info: layers, canvas, timing... (installed run at 854b3d88
  // logged only 'ready' because play-start carried these members, snapshot() dropped it and the sink refused the rest).
  const info = { lifecycleGeneration: 1, documentSnapshotId: 'native-object:native-object-document-7:0', canvasW: 320, canvasH: 180,
    fps: 24, totalFrames: 4, layers: [{ layerUid: 'L1', name: 'Layer 1', visible: true }], layerFolders: [], layerModelUnavailable: ['linkGroup'] };
  const r = await playUnderClock({ fixedClock: true, waOut: 3, loop: true, documentId: OBJECT_DOC, stopAfter: 5, extra: info });
  assert.equal(r.observer.inspect().dropped, 0, 'no event dropped');
  assert.deepEqual(r.sequences, r.sequences.map((_, i) => i + 1));
  const start = r.events.find(e => e.event === 'play-start');
  assert.deepEqual(Object.keys(start).sort(), ['contentRevision', 'documentId', 'documentSnapshotId', 'event', 'instanceId', 'lifecycleGeneration', 'runGeneration']);
  assert.deepEqual(verifyFixedClockSteps(r.events, { start: 0, waIn: 0, waOut: 3, loop: true, steps: 5 }).failures, []);
});

test('a looping run stopped by the operator: only the in-flight step superseded by Stop may return false', () => {
  const step = (frame, result) => ({ event: 'play-step', frame, ...(result === undefined ? {} : { result }) });
  const run = [1, 2, 3, 0].flatMap(f => [step(f), step(f, true)]);
  const stopped = [...run, step(1), { event: 'play-stop' }, step(1, false)];
  const v = verifyFixedClockSteps(stopped, { start: 0, waIn: 0, waOut: 3, loop: true });
  assert.deepEqual(v.failures, []); assert.equal(v.stoppedInFlight, true); assert.deepEqual(v.observed, [1, 2, 3, 0]);
  // a false return before Stop, or a false return that is not the last step, still fails
  assert.equal(verifyFixedClockSteps([...run, step(1), step(1, false), { event: 'play-stop' }], { start: 0, waIn: 0, waOut: 3, loop: true }).ok, false);
  assert.equal(verifyFixedClockSteps([step(1), step(1, false), ...run.slice(2), { event: 'play-stop' }], { start: 0, waIn: 0, waOut: 3, loop: true }).ok, false);
});

test('the bootstrap accepts opacity and object document ids and still refuses malformed ones', async () => {
  const ok = id => { const { observer } = observerHarness({}); return observer.ready.then(() => {
    observer.record({ event: 'host-receipt', documentId: id }); return observer.inspect().dropped === 0; }); };
  for (const id of ['native-document-1', OBJECT_DOC, 'native-object-document-18446744073709551615']) assert.equal(await ok(id), true, id);
  for (const id of ['native-object-document-0', 'native-object-document-01', 'native-object-document-', 'native-object-document-18446744073709551616',
    'native-object-document-1\n', 'native-object-documents-1', 'native-objects-document-1', 'object-document-1', 'native-work-1']) assert.equal(await ok(id), false, id);
  const { observer } = observerHarness({}); await observer.ready;
  observer.record({ event: 'host-receipt', workId: OBJECT_DOC }); observer.record({ event: 'host-receipt', workId: 'native-work-9' });
  assert.equal(observer.inspect().dropped, 1, 'workId keeps its own prefix');
});

// -- which Play path a native object project takes ------------------------------------------------
function startPlayHarness({ blocks, observer }) {
  const calls = { native: [], raf: 0, clock: 0, perf: 0 };
  const window = { NemoNativeOpacityCutover: blocks === null ? undefined : { blocksLegacy: () => blocks }, NemoTestPlayObserver: observer,
    requestAnimationFrame: () => { calls.raf++; return 1; } };
  const state = { playing: false, playDir: 1, currentFrame: 0, fps: 24 };
  const sandbox = { window, state, performance: { now: () => { calls.perf++; return 0; } }, requestAnimationFrame: window.requestAnimationFrame,
    startNativePlay: c => { calls.native.push(c); }, SMPlaybackCache: undefined, SMAudio: undefined,
    document: { getElementById: () => ({ innerHTML: '', classList: { add() {}, remove() {} } }) } };
  vm.createContext(sandbox);
  vm.runInContext(extractFunction(timeline, 'startPlay'), sandbox);
  return { sandbox, calls, window };
}

test('a native object project (cutover routes blocksLegacy to its session) plays through the native scheduler, never the legacy rAF loop', () => {
  const clock = { now() { throw new Error('legacy path read the clock'); }, request() { throw new Error('legacy path requested a tick'); }, cancel() {} };
  const { sandbox, calls, window } = startPlayHarness({ blocks: true, observer: { fixedClock: clock } });
  sandbox.startPlay();
  assert.deepEqual(calls.native, [window.NemoNativeOpacityCutover]);
  assert.equal(calls.raf, 0); assert.equal(calls.perf, 0); assert.equal(sandbox.state.playing, false);
  // The published cutover answers blocksLegacy from the active object project session before anything else.
  const operations = fs.readFileSync(path.join(__dirname, '../src/js/application/native-opacity-operations.js'), 'utf8');
  assert.match(operations, /function objectProject\(\)\{var p=ports\.objectProject&&ports\.objectProject\(\);return p&&p\.blocksLegacy\(\)\?p:null;\}/);
  assert.match(operations, /blocksLegacy: routed\('blocksLegacy',lifecycle\.blocksLegacy\)/);
});

test('without the native block (browser or legacy project) startPlay keeps the legacy loop and never reads the observer clock', () => {
  const clock = { get now() { throw new Error('observer clock read'); } };
  for (const blocks of [false, null]) {
    const { sandbox, calls } = startPlayHarness({ blocks, observer: { fixedClock: clock } });
    sandbox.startPlay();
    assert.deepEqual(calls.native, []);
    assert.equal(calls.raf, 1); assert.equal(sandbox.state.playing, true);
  }
});
