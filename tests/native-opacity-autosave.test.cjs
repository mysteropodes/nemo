'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const { extractFunction } = require('./fixtures/lib/sandbox.cjs');

const source = fs.readFileSync(path.join(__dirname, '../src/js/timeline.js'), 'utf8');
const start = source.indexOf('setInterval(function(){\n  if(state.playing)return;');
assert.notEqual(start, -1, 'production timed autosave must be found');
const end = source.indexOf('},30000);', start);
assert.notEqual(end, -1);
const timerSource = source.slice(start, end + '},30000);'.length);

function harness(options = {}) {
  const effects = [], warnings = [];
  const state = { playing: !!options.playing };
  let tick, delay;
  const window = {
    SM: { exportJSON() { effects.push(['legacy-json']); return 'legacy-json'; } },
    SMProject: {
      autosaveWrite(json) { effects.push(['autosave', json]); },
      pushVersionSnapshot(json) { effects.push(['snapshot', json]); },
      refreshActiveTabDirtyDot(json) { effects.push(['dirty-dot', json]); },
    },
  };
  if ('native' in options) window.NemoNativeOpacityCutover = options.native;
  if (options.localStorage) delete window.SMProject;
  const sandbox = {
    window, state,
    saveAllLayerFrames() { effects.push(['legacy-save']); },
    localStorage: { setItem(key, json) { effects.push(['localStorage', key, json]); } },
    console: { warn(...args) { warnings.push(args); } },
    setInterval(callback, ms) { tick = callback; delay = ms; },
  };
  vm.runInNewContext(timerSource, sandbox, { filename: 'timeline-autosave.js' });
  assert.equal(delay, 30000);
  return { tick, state, window, effects, warnings, sandbox };
}

function nativePin() {
  const current = { instanceId: 'native-instance', documentId: 'native-document', contentRevision: 7 };
  const pin = { ...current };
  const json = JSON.stringify({ version: 13, nativeIdentity: pin,
    layers: [{ layerUid: 'opacity-layer', motionStatic: { opacity: [42] } }] });
  let reads = 0;
  const controller = {
    blocksLegacy: () => true,
    isActive: () => true,
    identity: () => current,
    persistenceJSON() {
      reads++;
      return Object.keys(pin).every(key => current[key] === pin[key]) ? json : null;
    },
  };
  return { controller, current, json, reads: () => reads };
}

test('native 30-second tick stores the exact current pin once and never writes Paper', () => {
  const native = nativePin();
  const fixture = harness({ native: native.controller });
  fixture.tick();
  assert.equal(native.reads(), 1);
  assert.deepEqual(fixture.effects, [
    ['autosave', native.json], ['snapshot', native.json], ['dirty-dot', native.json],
  ]);
  assert.deepEqual(JSON.parse(fixture.effects[0][1]).nativeIdentity, native.current);
  assert.equal(fixture.warnings.length, 0);
});

for (const field of ['instanceId', 'documentId', 'contentRevision']) {
  test(`stale native ${field} prevents every persistence effect`, () => {
    const native = nativePin();
    native.current[field] = field === 'contentRevision' ? 8 : 'replacement';
    const fixture = harness({ native: native.controller });
    fixture.tick();
    assert.deepEqual(fixture.effects, []);
    assert.equal(fixture.warnings.length, 1);
    assert.equal(fixture.warnings[0][0], 'Timed autosave unavailable');
  });
}

for (const mode of ['unavailable', 'fenced', 'throwing', 'missing-persistence', 'missing-identity', 'missing-active']) {
  test(`native ${mode} fails closed without autosave, localStorage, snapshot or dirty-dot`, () => {
    const controller = nativePin().controller;
    if (mode === 'missing-identity') delete controller.identity;
    if (mode === 'missing-active') delete controller.isActive;
    if (mode === 'fenced') controller.isActive = () => false;
    if (mode === 'missing-persistence') delete controller.persistenceJSON;
    if (mode !== 'missing-persistence') controller.persistenceJSON = () => {
      if (mode === 'throwing') throw new Error('native serialization rejected');
      return null;
    };
    for (const localStorage of [false, true]) {
      const fixture = harness({ native: controller, localStorage });
      assert.equal(fixture.tick().status, 'unavailable');
      assert.deepEqual(fixture.effects, []);
      assert.equal(fixture.warnings.length, 1);
    }
  });
}

for (const controller of [null, undefined]) {
  test(`explicitly present ${controller === null ? 'null' : 'undefined'} controller cannot admit legacy autosave`, () => {
    const fixture = harness({ native: controller });
    assert.equal('NemoNativeOpacityCutover' in fixture.window, true);
    assert.equal(fixture.tick().status, 'unavailable');
    assert.deepEqual(fixture.effects, []);
    assert.equal(fixture.warnings.length, 1);
  });
}

for (const controller of [0, '', false, {}, { blocksLegacy: () => null }, { blocksLegacy: () => 'legacy' },
  { blocksLegacy() { throw new Error('ownership unavailable'); } }]) {
  test('malformed or ambiguous ownership never admits a legacy writer', () => {
    const fixture = harness({ native: controller });
    assert.equal(fixture.tick().status, 'unavailable');
    assert.deepEqual(fixture.effects, []);
  });
}

test('throwing native-controller getter returns unavailable before every effect', () => {
  const fixture = harness();
  Object.defineProperty(fixture.window, 'NemoNativeOpacityCutover', {
    get() { throw new Error('controller unavailable'); },
  });
  assert.equal(fixture.tick().status, 'unavailable');
  assert.deepEqual(fixture.effects, []);
  assert.equal(fixture.warnings.length, 1);
});

for (const phase of ['installing', 'replacing', 'release-requested', 'releasing', 'closed', 'indeterminate']) {
  test(`${phase} remains native-blocking even though isActive is false`, () => {
    const native = nativePin();
    native.controller.isActive = () => false;
    const fixture = harness({ native: native.controller });
    assert.equal(fixture.tick().status, 'unavailable');
    assert.deepEqual(fixture.effects, []);
    assert.equal(native.reads(), 0);
  });
}

for (const identity of [null, {}, { instanceId: 'i', documentId: 'd', contentRevision: -1 },
  { instanceId: 'i', documentId: 'd', contentRevision: '7' }]) {
  test('malformed native identity cannot publish autosave bytes', () => {
    const native = nativePin();
    native.controller.identity = () => identity;
    const fixture = harness({ native: native.controller });
    assert.equal(fixture.tick().status, 'unavailable');
    assert.deepEqual(fixture.effects, []);
  });
}

test('ownership changes during serialization fence native bytes without legacy fallback', () => {
  const native = nativePin();
  native.controller.persistenceJSON = () => {
    native.controller.blocksLegacy = () => false;
    return native.json;
  };
  const fixture = harness({ native: native.controller });
  assert.equal(fixture.tick().status, 'unavailable');
  assert.deepEqual(fixture.effects, []);
});

for (const field of ['instanceId', 'documentId', 'contentRevision']) {
  test(`native ${field} changing during serialization rejects bytes and can recover next tick`, () => {
    const native = nativePin();
    const original = native.controller.persistenceJSON;
    native.controller.persistenceJSON = () => {
      native.current[field] = field === 'contentRevision' ? 8 : 'replacement';
      return native.json;
    };
    const fixture = harness({ native: native.controller });
    assert.equal(fixture.tick().status, 'unavailable');
    assert.deepEqual(fixture.effects, []);
    Object.assign(native.current, JSON.parse(native.json).nativeIdentity);
    native.controller.persistenceJSON = original;
    fixture.tick();
    assert.deepEqual(fixture.effects, [
      ['autosave', native.json], ['snapshot', native.json], ['dirty-dot', native.json],
    ]);
  });
}

test('native-ready localStorage fallback stores the same pinned bytes once', () => {
  const native = nativePin();
  const fixture = harness({ native: native.controller, localStorage: true });
  fixture.tick();
  assert.deepEqual(fixture.effects, [['localStorage', 'nemo-auto', native.json]]);
  assert.equal(native.reads(), 1);
});

for (const controller of [undefined, { blocksLegacy: () => false }]) {
  test(`legacy ${controller ? 'released' : 'browser'} tick preserves frame-save and adapter order`, () => {
    const fixture = harness(controller === undefined ? {} : { native: controller });
    fixture.tick();
    assert.deepEqual(fixture.effects, [
      ['legacy-save'], ['legacy-json'], ['autosave', 'legacy-json'],
      ['snapshot', 'legacy-json'], ['dirty-dot', 'legacy-json'],
    ]);
    assert.equal(fixture.warnings.length, 0);
  });
}

test('legacy browser localStorage fallback retains its existing save and serialization', () => {
  const fixture = harness({ localStorage: true });
  fixture.tick();
  assert.deepEqual(fixture.effects, [
    ['legacy-save'], ['legacy-json'], ['localStorage', 'nemo-auto', 'legacy-json'],
  ]);
});

for (const native of [undefined, nativePin()]) {
  test(`production autosave adapter mirrors exact ${native ? 'native' : 'browser'} bytes to localStorage and IndexedDB`, () => {
    const fixture = harness(native ? { native: native.controller } : {});
    fixture.window.SMIdb = {
      set(key, json) { fixture.effects.push(['IndexedDB', key, json]); return Promise.resolve(); },
    };
    const project = fs.readFileSync(path.join(__dirname, '../src/js/project.js'), 'utf8');
    vm.runInNewContext(extractFunction(project, 'autosaveWrite'), fixture.sandbox);
    fixture.window.SMProject.autosaveWrite = fixture.sandbox.autosaveWrite;
    fixture.tick();
    const bytes = native ? native.json : 'legacy-json';
    assert.deepEqual(fixture.effects, [
      ...native ? [] : [['legacy-save'], ['legacy-json']],
      ['localStorage', 'nemo-auto', bytes], ['IndexedDB', 'nemo-auto', bytes],
      ['snapshot', bytes], ['dirty-dot', bytes],
    ]);
  });
}

for (const native of [undefined, nativePin().controller]) {
  test(`playing skips the entire ${native ? 'native' : 'legacy'} tick`, () => {
    const fixture = harness(native ? { native, playing: true } : { playing: true });
    fixture.tick();
    assert.deepEqual(fixture.effects, []);
    assert.equal(fixture.warnings.length, 0);
  });
}
