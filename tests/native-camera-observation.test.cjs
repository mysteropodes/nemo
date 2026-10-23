'use strict';
// Intended destination: tests/native-camera-observation.test.cjs.
// Tests the existing Desktop API; no Rust, native paints, installed input or mode proof.
const test = require('node:test');
const assert = require('node:assert/strict');
const Desktop = require('../src/js/adapters/native-object-project-desktop.js');
const turn = () => new Promise(setImmediate);
const timerTurn = () => new Promise(resolve => setTimeout(resolve, 0));
function storage(root) {
  const writes = [], published = [];
  root.__TAURI__ = {
    core: { invoke() { assert.fail('observations must not issue native queries'); } },
    path: { appDataDir: async () => '/private/test-data/' },
    fs: { mkdir: async () => {},
      writeTextFile: async (path, bytes) => { writes.push({ path, bytes }); },
      rename: async (from, to) => { published.push({ from, to, bytes: writes.at(-1).bytes }); }
    }
  };
  return { writes, published, async records() {
    await turn(); await turn();
    return published.length ? JSON.parse(published.at(-1).bytes).events : [];
  } };
}
function camera() {
  let click, selected = false, available = true, output;
  const trace = [], observations = [];
  const view = {};
  Object.defineProperty(view, 'cameraAvailable', { get() { trace.push('available'); return available; } });
  const presentation = {};
  Object.defineProperty(presentation, 'view', { get() { trace.push('view'); return view; } });
  const shown = {};
  Object.defineProperty(shown, 'presentation', { get() { trace.push('presentation'); return presentation; } });
  const state = {};
  Object.defineProperties(state, {
    cameraView: { configurable: true, get() { trace.push('selected:get'); return selected; },
      set(value) { trace.push('selected:set:' + value); selected = value; } },
    currentFrame: { configurable: true, get() { trace.push('frame'); return 4; } }
  });
  const button = { addEventListener(type, handler) { assert.equal(type, 'click'); click = handler; },
    setAttribute(name, value) { trace.push(name + ':' + value); } };
  // This boundary supplies only Desktop's existing session ports. The composed
  // test separately exercises the real Project owner and ordinary Camera entry.
  const session = { readPresentedObjectSession() { trace.push('read'); return shown; },
    renderPreview(frame) { trace.push('preview:' + frame); return output; } };
  const root = { state, document: { getElementById: id => id === 'btn-camera' ? button : null },
    NemoNativeObjectProject: { desktop: () => session },
    SM: { t: key => { trace.push('title'); return key; } },
    __NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__: (event, code) => {
      trace.push('observe:' + code); observations.push([event, code]);
    }
  };
  const sink = storage(root);
  Desktop.bindCameraUI(root, { toggle() { assert.fail('legacy camera'); } });
  return { root, trace, observations, session, state, sink, button,
    click: () => click(), setOutput: value => { output = value; },
    setAvailable: value => { available = value; }, selected: () => selected };
}
const successfulReads = selected => [
  'read', 'presentation', 'view', 'view', 'available', 'selected:get',
  'selected:set:' + selected, 'frame', 'preview:4',
  'read', 'presentation', 'view', 'view', 'available', 'title', 'selected:get',
  'aria-pressed:' + selected,
  'observe:' + (selected ? 'selected-stored-camera' : 'selected-canvas')
];
test('Camera observations sanitize every code at storage and optional observer boundaries', async () => {
  const root = {}, sink = storage(root), observed = [];
  root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__ = (event, code) => observed.push([event, code]);
  const rows = [
    ['camera-control', 'native-refused'], ['camera-control', 'selected-canvas'],
    ['camera-control', 'selected-stored-camera'], ['camera-presented', 'canvas'],
    ['camera-presented', 'stored-camera']
  ];
  for (const event of ['camera-control', 'camera-presented']) {
    for (const code of ['PRIVATE /Users/secret message stack', null, false, 7,
      { toString() { assert.fail('diagnostic must not coerce caller data'); } }]) rows.push([event, code]);
  }
  for (const [event, code] of rows) Desktop.openDiagnostic(root, event, code);
  Desktop.openDiagnostic(root, 'PRIVATE EVENT', 'PRIVATE CODE');
  const records = await sink.records();
  const expected = rows.map(([event, code], index) => [event, index < 5 ? code : 'unknown']);
  assert.deepEqual(records.map(({ event, code }) => [event, code]), expected);
  assert.deepEqual(observed, expected);
  assert.ok(records.every(record => Object.keys(record).join(',') === 'sequence,attempt,event,code'));
  assert.doesNotMatch(JSON.stringify(records), /PRIVATE|Users|secret|message|stack/);
});
test('native Camera preserves original guard getter order/count and exact unobserved preview promise', () => {
  const f = camera();
  const preview = new Promise(() => {});
  Object.defineProperty(preview, 'then', { get() { assert.fail('observation must not inspect or wrap preview promise'); } });
  f.setOutput(preview);
  assert.strictEqual(f.click(), preview);
  assert.equal(f.selected(), true);
  assert.deepEqual(f.trace, successfulReads(true));
  f.trace.length = 0;
  assert.strictEqual(f.click(), preview);
  assert.equal(f.selected(), false);
  assert.deepEqual(f.trace, successfulReads(false));
  assert.deepEqual(f.observations, [['camera-control', 'selected-stored-camera'], ['camera-control', 'selected-canvas']]);
});
test('native Camera refusal retains original short circuit and issues no preview or selected-state read', () => {
  for (const available of [false, undefined, null, 1, 'true']) {
    const f = camera(); f.setAvailable(available);
    assert.strictEqual(f.click(), undefined);
    assert.equal(f.selected(), false);
    assert.deepEqual(f.trace, ['read', 'presentation', 'view', 'view', 'available', 'observe:native-refused']);
    assert.deepEqual(f.observations, [['camera-control', 'native-refused']]);
  }
  for (const shown of [null, { presentation: null }, { presentation: {} }, { presentation: { view: null } }]) {
    const f = camera();
    f.session.readPresentedObjectSession = () => { f.trace.push('read'); return shown; };
    assert.strictEqual(f.click(), undefined);
    assert.deepEqual(f.trace, ['read', 'observe:native-refused']);
    assert.equal(f.selected(), false);
  }
});
test('synchronous original guard, state, preview and painter errors retain identity without selected witness', () => {
  for (const stage of ['guard', 'read', 'write', 'frame', 'preview', 'painter']) {
    const f = camera(), error = { stage, private: 'PRIVATE ERROR' };
    if (stage === 'guard') f.session.readPresentedObjectSession = () => { throw error; };
    if (stage === 'read') Object.defineProperty(f.state, 'cameraView', { get() { throw error; } });
    if (stage === 'write') Object.defineProperty(f.state, 'cameraView', { get: () => false, set() { throw error; } });
    if (stage === 'frame') Object.defineProperty(f.state, 'currentFrame', { get() { throw error; } });
    if (stage === 'preview') f.session.renderPreview = () => { throw error; };
    if (stage === 'painter') f.root.SM.t = () => { throw error; };
    assert.throws(() => f.click(), caught => caught === error, stage);
    assert.deepEqual(f.observations, [], stage);
  }
});
test('async preview rejection stays the same rejection and selected observation is only assignment evidence', async () => {
  const f = camera(), error = { private: 'PRIVATE REJECTION' };
  let reject;
  const preview = new Promise((resolve, no) => { reject = no; });
  // Attach the rejection assertion before rejecting, without changing the result returned by click.
  const rejected = assert.rejects(preview, caught => caught === error);
  f.setOutput(preview);
  assert.strictEqual(f.click(), preview);
  assert.deepEqual(f.observations, [['camera-control', 'selected-stored-camera']]);
  reject(error); await rejected;
  assert.equal(f.observations.some(([event]) => event === 'camera-presented'), false);
});
test('throwing observer callback/getter and missing, failing or hung storage preserve synchronous Camera result', async () => {
  for (const mode of ['callback', 'getter', 'missing', 'path', 'mkdir', 'write', 'rename', 'hung']) {
    const f = camera(), preview = new Promise(() => {});
    f.setOutput(preview);
    if (mode === 'callback') f.root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__ = () => { throw Error('PRIVATE CALLBACK'); };
    if (mode === 'getter') Object.defineProperty(f.root, '__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__', {
      get() { throw Error('PRIVATE OBSERVER GETTER'); }
    });
    if (mode === 'missing') delete f.root.__TAURI__.path;
    if (mode === 'path') f.root.__TAURI__.path.appDataDir = async () => { throw Error('PRIVATE PATH'); };
    if (['mkdir', 'write', 'rename'].includes(mode)) {
      const method = { mkdir: 'mkdir', write: 'writeTextFile', rename: 'rename' }[mode];
      f.root.__TAURI__.fs[method] = async () => { throw Error('PRIVATE STORAGE'); };
    }
    if (mode === 'hung') f.root.__TAURI__.fs.writeTextFile = () => new Promise(() => {});
    assert.strictEqual(f.click(), preview, mode);
    assert.equal(f.selected(), true, mode);
    assert.deepEqual(f.trace.filter(value => !value.startsWith('observe:')), successfulReads(true).slice(0, -1), mode);
    await turn(); await turn(); // Surface accidental unhandled storage rejections.
  }
});
test('Camera observation storage coalesces one writer and retains only the newest 32 fixed events', async () => {
  const root = {}, sink = storage(root);
  let release, active = 0, maxActive = 0, count = 0, last;
  root.__TAURI__.fs.writeTextFile = async (path, bytes) => {
    last = bytes; count++; active++; maxActive = Math.max(maxActive, active);
    if (count === 1) await new Promise(resolve => { release = resolve; });
    active--;
  };
  root.__TAURI__.fs.rename = async (from, to) => { sink.published.push({ from, to, bytes: last }); };
  Desktop.openDiagnostic(root, 'file-open-started'); await turn();
  for (let i = 0; i < 100; i++) Desktop.openDiagnostic(root, 'camera-control', 'selected-canvas');
  Desktop.openDiagnostic(root, 'camera-presented', 'PRIVATE DOCUMENT');
  assert.equal(count, 1); release(); await turn(); await turn();
  const log = JSON.parse(last);
  assert.equal(count, 2); assert.equal(maxActive, 1);
  assert.equal(log.events.length, 32);
  assert.equal(log.events[0].sequence, 71); assert.equal(log.events.at(-1).sequence, 102);
  assert.ok(log.events.every(record => record.attempt === 1));
  assert.deepEqual(log.events.at(-1), { sequence: 102, attempt: 1, event: 'camera-presented', code: 'unknown' });
  assert.doesNotMatch(last, /PRIVATE|DOCUMENT/);
  assert.equal(sink.published.length, 2);
});
test('correlated completion reports only an own data mode without extra accessor execution', async () => {
  for (const kind of ['canvas', 'stored-camera', 'invalid', 'accessor', 'inherited', 'view-accessor', 'descriptor-error']) {
    const root = {}, sink = storage(root), observed = [], info = { canvasW: 320, canvasH: 180 };
    let modeReads = 0, viewReads = 0;
    let view = { mode: kind === 'invalid' ? 'PRIVATE MODE' : kind };
    if (kind === 'accessor') Object.defineProperty(view, 'mode', { get() { modeReads++; throw Error('PRIVATE MODE GETTER'); } });
    if (kind === 'inherited') view = Object.create({ mode: 'stored-camera' });
    if (kind === 'descriptor-error') view = new Proxy(view, { getOwnPropertyDescriptor() { throw Error('PRIVATE DESCRIPTOR'); } });
    const presentation = { view };
    if (kind === 'view-accessor') Object.defineProperty(presentation, 'view', { get() { viewReads++; return view; } });
    const shown = { presentation };
    const session = { info: () => info, readSubscriptionGeneration: () => 1, readPresentedObjectSession: () => shown };
    Object.assign(root, {
      document: { getElementById: () => null },
      NemoNativeObjectProject: { desktop: () => session },
      NemoNativeViewportUnderlay: { desktop: () => ({}) },
      __NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__: (event, code) => observed.push([event, code])
    });
    Desktop.createPorts(root).presented(info); await timerTurn();
    const expected = ['canvas', 'stored-camera'].includes(kind) ? kind : 'unknown';
    assert.deepEqual(observed, [['camera-presented', expected]], kind);
    assert.equal(modeReads, 0, kind);
    assert.equal(viewReads, kind === 'view-accessor' ? 1 : 0, 'only the preexisting zoom painter reads view');
    assert.deepEqual((await sink.records()).map(({ event, code }) => [event, code]), observed, kind);
  }
});
test('completion observation follows original painter and drops stale captured correlation', async () => {
  for (const changed of ['none', 'info', 'ticket', 'presentation', 'document', 'controller-throws']) {
    const root = {}, trace = [], info = { canvasW: 320, canvasH: 180 };
    storage(root);
    let currentInfo = info, ticket = 1, presentation = { view: { mode: 'stored-camera' } };
    const session = { info: () => currentInfo, readSubscriptionGeneration: () => ticket,
      readPresentedObjectSession: () => ({ presentation }) };
    Object.assign(root, {
      document: { getElementById: () => null },
      NemoNativeObjectProject: { desktop: () => session },
      NemoNativeViewportUnderlay: { desktop: () => ({}) },
      NemoNativeProjectSettingsController: { presented() {
        trace.push('settings'); if (changed === 'controller-throws') throw Error('PRIVATE PAINTER');
      } },
      NemoNativeObjectUI: { refreshPolicy: () => trace.push('policy') },
      __NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__: (event, code) => trace.push([event, code])
    });
    Desktop.createPorts(root).presented(info);
    if (changed === 'info') currentInfo = { ...info };
    if (changed === 'ticket') ticket++;
    if (changed === 'presentation') presentation = { view: { mode: 'stored-camera' } };
    if (changed === 'document') root.document = { getElementById: () => null };
    await timerTurn();
    assert.deepEqual(trace, changed === 'none' ? ['settings', 'policy', ['camera-presented', 'stored-camera']] :
      changed === 'controller-throws' ? ['settings'] : [], changed);
  }
});
