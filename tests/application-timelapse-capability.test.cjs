'use strict';
// P31/#1033 -- Labs timelapse capability registration. Descriptor + contract
// tests only: src/js/labs/timelapse.js is untouched, and nothing here wires
// this into the live NemoApplication.handle dispatch (bootstrap-layer work,
// outside this leaf's bounded source ownership).

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const ROOT = path.resolve(__dirname, '..');
const capability = require('../src/js/application/timelapse-capability.js');
const NemoCapabilityRegistry = require('../src/js/application/capability-registry.js');

// ---- the descriptor still agrees with the committed contract ------------

test('the runtime descriptor matches engineering/application/capabilities/timelapse.json', () => {
  const committed = JSON.parse(fs.readFileSync(
    path.join(ROOT, 'engineering', 'application', 'capabilities', 'timelapse.json'), 'utf8'));
  assert.deepEqual(capability.DESCRIPTOR, committed,
    'the synchronous runtime projection drifted from the canonical descriptor');
});

// ---- computeAvailability: every unavailable reason is reachable and honest

test('computeAvailability: no window/SMLabs is missing-dependency', () => {
  assert.deepEqual(capability.computeAvailability(null), { state: 'unavailable', reason: 'missing-dependency' });
  assert.deepEqual(capability.computeAvailability({}), { state: 'unavailable', reason: 'missing-dependency' });
});

test('computeAvailability: SMLabs loaded but no supported MediaRecorder mime is platform-unsupported', () => {
  const win = { SMLabs: { timelapseStart: () => {}, timelapseStop: () => {}, isOn: () => true },
    MediaRecorder: { isTypeSupported: () => false } };
  assert.deepEqual(capability.computeAvailability(win), { state: 'unavailable', reason: 'platform-unsupported' });
});

test('computeAvailability: loaded and supported but the labs flag is off is feature-flag-off', () => {
  const win = { SMLabs: { timelapseStart: () => {}, timelapseStop: () => {}, isOn: () => false },
    MediaRecorder: { isTypeSupported: () => true } };
  assert.deepEqual(capability.computeAvailability(win), { state: 'unavailable', reason: 'feature-flag-off' });
});

test('computeAvailability: loaded, supported and the flag is on is available', () => {
  const win = { SMLabs: { timelapseStart: () => {}, timelapseStop: () => {}, isOn: () => true },
    MediaRecorder: { isTypeSupported: () => true } };
  assert.deepEqual(capability.computeAvailability(win), { state: 'available', reason: null });
});

// ---- registration: the P05 registry contract -----------------------------

test('the static baseline (unavailable) registers without a handler', () => {
  const registry = NemoCapabilityRegistry.create();
  assert.equal(capability.register(registry), 'timelapse');
  assert.equal(registry.get('timelapse').availability.reason, 'missing-dependency');
});

test('claiming available without a bound handler is refused, same guard as opacity', () => {
  const registry = NemoCapabilityRegistry.create();
  let code = null;
  try { capability.register(registry, undefined, { state: 'available', reason: null }); }
  catch (error) { code = error.code; }
  assert.equal(code, 'availability_unbacked');
});

test('claiming available with a bound handler registers', () => {
  const registry = NemoCapabilityRegistry.create();
  const id = capability.register(registry, () => ({ ok: true }), { state: 'available', reason: null });
  assert.equal(id, 'timelapse');
  assert.equal(registry.get('timelapse').availability.state, 'available');
});

// ---- dispatch against the real module, loaded in an isolated fixture -----
//
// A minimal browser surface, not a reimplementation of timelapse.js's own
// logic: real canvas/MediaRecorder/SMLabs objects are replaced by fakes that
// satisfy the exact shape timelapse.js reads, so the module under test is
// the actual shipped source, unmodified.

function labsFixture(opts) {
  opts = opts || {};
  const canvas = {
    width: 100, height: 100,
    getBoundingClientRect: () => ({ width: 200, height: 200 }),
    getContext: () => ({ drawImage: () => {} }),
    captureStream: () => ({ kind: 'fake-stream' }),
  };
  function FakeMediaRecorder(stream, recorderOpts) {
    this.stream = stream; this.mimeType = recorderOpts.mimeType; this.state = 'recording';
  }
  FakeMediaRecorder.prototype.start = function () {};
  FakeMediaRecorder.prototype.stop = function () {
    const self = this;
    setTimeout(() => { if (typeof self.onstop === 'function') self.onstop(); }, 0);
  };
  FakeMediaRecorder.isTypeSupported = () => opts.mediaRecorderSupported !== false;

  const ctx = {
    console,
    document: {
      querySelectorAll: () => (opts.canvasAvailable === false ? [] : [canvas]),
      createElement: (tag) => (tag === 'canvas' ? Object.assign({}, canvas) : { click() {} }),
      readyState: 'complete',
      addEventListener: () => {},
    },
    getComputedStyle: () => ({ display: 'block' }),
    MediaRecorder: opts.mediaRecorderPresent === false ? undefined : FakeMediaRecorder,
    URL: { createObjectURL: () => 'blob:fake', revokeObjectURL: () => {} },
    Blob: function (parts, blobOpts) { this.parts = parts; this.type = blobOpts.type; this.size = 12345; },
    setInterval, clearInterval, Date,
  };
  ctx.window = ctx;
  const flagOn = opts.flagOn !== false;
  ctx.window.SMLabs = { isOn: () => flagOn, register: opts.onRegister || (() => {}) };
  vm.createContext(ctx);
  vm.runInContext(fs.readFileSync(path.resolve(ROOT, 'src/js/labs/timelapse.js'), 'utf8'), ctx,
    { filename: path.resolve(ROOT, 'src/js/labs/timelapse.js') });
  return ctx;
}

test('start delegates to the real SMLabs.timelapseStart and the handler reports started:true', async () => {
  const win = labsFixture();
  const handler = capability.handlerFor(win);
  const result = await handler({ operation: 'start', payload: {} });
  assert.deepEqual(result, { ok: true, result: { started: true } });
  await handler({ operation: 'stop', payload: {} }); // clean up the interval
});

test('a second start while one is already running matches current behavior: refused, not silently replaced', async () => {
  const win = labsFixture();
  const handler = capability.handlerFor(win);
  const first = await handler({ operation: 'start', payload: {} });
  assert.equal(first.ok, true);
  const second = await handler({ operation: 'start', payload: {} });
  assert.equal(second.ok, false);
  assert.equal(second.error.code, 'start-refused');
  await handler({ operation: 'stop', payload: {} });
});

test('no capturable canvas: start is refused, matching current behavior', async () => {
  const win = labsFixture({ canvasAvailable: false });
  const handler = capability.handlerFor(win);
  const result = await handler({ operation: 'start', payload: {} });
  assert.equal(result.ok, false);
  assert.equal(result.error.code, 'start-refused');
});

test('no supported MediaRecorder mime: start is refused, matching current behavior', async () => {
  const win = labsFixture({ mediaRecorderSupported: false });
  const handler = capability.handlerFor(win);
  const result = await handler({ operation: 'start', payload: {} });
  assert.equal(result.ok, false);
  assert.equal(result.error.code, 'start-refused');
});

test('stop with nothing recording matches current behavior: not-recording, not a thrown error', async () => {
  const win = labsFixture();
  const handler = capability.handlerFor(win);
  const result = await handler({ operation: 'stop', payload: {} });
  assert.deepEqual(result, { ok: false, error: { code: 'not-recording', message: 'timelapseStop: nothing to stop.' } });
});

test('start then stop resolves the handler with the recorded duration and size', async () => {
  const win = labsFixture();
  const handler = capability.handlerFor(win);
  await handler({ operation: 'start', payload: {} });
  const result = await handler({ operation: 'stop', payload: {} });
  assert.equal(result.ok, true);
  assert.equal(typeof result.result.seconds, 'number');
  assert.equal(result.result.bytes, 12345);
});

test('an fps input is forwarded to timelapseStart unchanged', async () => {
  const win = labsFixture();
  let seenFps = null;
  const realStart = win.SMLabs.timelapseStart;
  win.SMLabs.timelapseStart = function (fps) { seenFps = fps; return realStart(fps); };
  const handler = capability.handlerFor(win);
  await handler({ operation: 'start', payload: { fps: 15 } });
  assert.equal(seenFps, 15);
  await handler({ operation: 'stop', payload: {} });
});

test('an unknown operation fails explicitly rather than silently', async () => {
  const win = labsFixture();
  const handler = capability.handlerFor(win);
  const result = await handler({ operation: 'status', payload: {} });
  assert.equal(result.ok, false);
  assert.equal(result.error.code, 'unknown_operation');
});

// ---- onDisable still ends an owned recording, matching current behavior --

test('disabling the labs flag while recording still ends the timer/recording (unchanged onDisable contract)', async () => {
  let captured = null;
  const win = labsFixture({ onRegister: (name, def) => { captured = def; } });
  const handler = capability.handlerFor(win);
  await handler({ operation: 'start', payload: {} });
  assert.equal(typeof captured.onDisable, 'function');
  captured.onDisable();
  // A stop() right after onDisable must report not-recording: the module's
  // own cleanup already cleared its internal `rec`.
  const result = await handler({ operation: 'stop', payload: {} });
  assert.equal(result.error && result.error.code, 'not-recording');
});
