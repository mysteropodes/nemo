'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const entry = require('../src/js/adapters/native-opacity-project-entry.js');
const { extractFunction } = require('./fixtures/lib/sandbox.cjs');
const timeline = fs.readFileSync(path.join(__dirname, '../src/js/timeline.js'), 'utf8');
const timerStart = timeline.indexOf('setInterval(function(){\n  if(state.playing)return;');
assert.ok(timerStart >= 0);
const timerEnd = timeline.indexOf('},30000);', timerStart);
assert.ok(timerEnd > timerStart);
const timerSource = timeline.slice(timerStart, timerEnd + '},30000);'.length);
const project = fs.readFileSync(path.join(__dirname, '../src/js/project.js'), 'utf8');
const BYTES = '{"version":13,"native":"immutable-opacity-persistence"}';
const dev = () => ({ protocol: 'http:', hostname: 'localhost', port: '1420' });
const packaged = () => ({ protocol: 'tauri:', hostname: 'localhost', port: '', pathname: '/index.html' });
const api = () => ({ core: { invoke() { throw Error('no native command needed'); } } });
function pin() {
  return { blocksLegacy: () => true, isActive: () => true,
    identity: () => ({ instanceId: 'native-i', documentId: 'native-d', contentRevision: 7 }),
    persistenceJSON: () => BYTES };
}
function fixture(change = () => {}, browser = false) {
  const effects = [], warnings = [];
  const effect = name => (...args) => { effects.push([name, ...args]); return 'Paper'; };
  const root = { location: browser ? dev() : packaged(), SM: { exportJSON: effect('Paper export'), t: x => x },
    NemoNativeOpacityProjectEntry: entry,
    SMProject: { autosaveWrite: effect('autosave'), pushVersionSnapshot: effect('history'),
      refreshActiveTabDirtyDot: effect('dirty') } };
  if (!browser) Object.assign(root, { __TAURI__: api(), NemoNativeOpacityCutover: pin() });
  change(root);
  let tick;
  const sandbox = { window: root, state: { playing: false }, SM: root.SM,
    saveAllLayerFrames: effect('Paper frames'), localStorage: { setItem: effect('storage') },
    console: { warn(...args) { warnings.push(args); } },
    setInterval(fn, delay) { assert.equal(delay, 30000); tick = fn; } };
  vm.runInNewContext(timerSource, sandbox);
  return { root, effects, warnings, sandbox, tick };
}
const deny = [
  ['object owner without opacity admission', r => { delete r.NemoNativeOpacityCutover; }],
  ['malformed marker at dev URL', r => { r.location = dev(); r.__TAURI__ = {}; }],
  ...[undefined, null, false, '', 42, { core: {} }].map((v, i) => [`malformed marker ${i}`, r => { r.__TAURI__ = v; }]),
  ['inherited malformed marker', r => { delete r.__TAURI__; Object.setPrototypeOf(r, { __TAURI__: null }); }],
  ['packaged API absent', r => { delete r.__TAURI__; }],
  ['throwing API', r => { Object.defineProperty(r, '__TAURI__', { get() { throw Error('API inaccessible'); } }); }],
  ['foreign tauri host', r => { r.location.hostname = 'foreign'; }],
  ['tauri port', r => { r.location.port = '1420'; }],
  ['unknown protocol', r => { r.location.protocol = 'file:'; delete r.__TAURI__; }],
  ['unknown location', r => { delete r.location; delete r.__TAURI__; }],
  ['throwing location', r => { Object.defineProperty(r, 'location', { get() { throw Error('location inaccessible'); } }); }],
  ['malformed location', r => { r.location = {}; }],
  ['absent opacity method', r => { delete r.NemoNativeOpacityCutover.isActive; }],
  ['legacy opacity phase', r => { r.NemoNativeOpacityCutover.blocksLegacy = () => false; }],
  ['ambiguous opacity phase', r => { r.NemoNativeOpacityCutover.blocksLegacy = () => undefined; }],
  ['throwing opacity phase', r => { r.NemoNativeOpacityCutover.blocksLegacy = () => { throw Error('phase'); }; }],
  ...['installing', 'replacing', 'release-requested', 'releasing', 'closed', 'indeterminate'].map(phase =>
    [phase, r => { r.NemoNativeOpacityCutover.isActive = () => false; }]),
  ['throwing active state', r => { r.NemoNativeOpacityCutover.isActive = () => { throw Error('active'); }; }],
  ['null persistence', r => { r.NemoNativeOpacityCutover.persistenceJSON = () => null; }],
  ['empty persistence', r => { r.NemoNativeOpacityCutover.persistenceJSON = () => ''; }],
  ['disconnected or synchronizing persistence', r => { r.NemoNativeOpacityCutover.persistenceJSON = () => { throw Error('unreadable'); }; }],
  ...[null, {}, { instanceId: '', documentId: 'd', contentRevision: 7 },
    { instanceId: 'i', documentId: 'd', contentRevision: Number.MAX_SAFE_INTEGER + 1 },
    { instanceId: 'i', documentId: 'd', contentRevision: -1 }].map((value, i) =>
    [`invalid identity ${i}`, r => { r.NemoNativeOpacityCutover.identity = () => value; }]),
  ...['instanceId', 'documentId', 'contentRevision'].map(field => [`changed ${field}`, r => {
    const identity = { instanceId: 'native-i', documentId: 'native-d', contentRevision: 7 };
    r.NemoNativeOpacityCutover.identity = () => identity;
    r.NemoNativeOpacityCutover.persistenceJSON = () => {
      identity[field] = field === 'contentRevision' ? 8 : 'replacement'; return BYTES;
    };
  }]),
  ['controller replaced during acquisition', r => {
    r.NemoNativeOpacityCutover.persistenceJSON = () => { r.NemoNativeOpacityCutover = pin(); return BYTES; };
  }],
  ['fence lifted during acquisition', r => {
    r.NemoNativeOpacityCutover.persistenceJSON = () => { r.NemoNativeOpacityCutover.blocksLegacy = () => false; return BYTES; };
  }],
  ['API replaced during acquisition', r => {
    r.NemoNativeOpacityCutover.persistenceJSON = () => { r.__TAURI__ = api(); return BYTES; };
  }],
];
for (const [label, change] of deny) {
  test(`${label}: actual helper and timer deny before all persistence effects`, () => {
    for (const action of ['json', 'frames', 'timer']) {
      const f = fixture(change);
      if (action === 'json') assert.throws(() => entry.documentJSON(f.root));
      else if (action === 'frames') assert.throws(() => entry.saveFramesIfLegacy(f.root, f.sandbox.saveAllLayerFrames));
      else assert.equal(f.tick().status, 'unavailable');
      assert.deepEqual(f.effects, []);
    }
  });
}
for (const location of [dev(), packaged(), { ...packaged(), pathname: '/', search: '?a=1', hash: '#b' }]) {
  test(`stable native opacity ${location.protocol} returns exact native bytes`, () => {
    const f = fixture(r => { r.location = location; });
    assert.equal(entry.documentJSON(f.root), BYTES);
    entry.saveFramesIfLegacy(f.root, f.sandbox.saveAllLayerFrames);
    f.tick();
    assert.deepEqual(f.effects, [['autosave', BYTES], ['history', BYTES], ['dirty', BYTES]]);
  });
}
test('ordinary browser at the identical dev URL retains existing persistence ordering', () => {
  const f = fixture(() => {}, true);
  assert.equal(entry.documentJSON(f.root), 'Paper');
  entry.saveFramesIfLegacy(f.root, f.sandbox.saveAllLayerFrames);
  assert.deepEqual(f.effects, [['Paper export'], ['Paper frames']]);
  f.effects.length = 0; f.tick();
  assert.deepEqual(f.effects, [['Paper frames'], ['Paper export'], ['autosave', 'Paper'], ['history', 'Paper'], ['dirty', 'Paper']]);
});
test('known desktop failure does not reach actual Save As picker or writeProjectTo writer', async () => {
  const f = fixture(r => { delete r.NemoNativeOpacityCutover; });
  f.root.__TAURI__.dialog = { save() { f.effects.push(['picker']); return '/owned/project.json'; } };
  f.root.NemoProjectNativeSave = { writeProjectFile() { f.effects.push(['writer']); } };
  Object.assign(f.sandbox, { currentName: 'project', currentPath: null, tauriOk: () => true,
    showToast() { f.effects.push(['toast']); } });
  for (const name of ['projectJSON', 'saveFramesIfLegacy', 'writeProjectTo', 'saveAs']) {
    const asyncPrefix = ['writeProjectTo', 'saveAs'].includes(name) ? 'async ' : '';
    vm.runInNewContext(asyncPrefix + extractFunction(project, name), f.sandbox);
  }
  await assert.rejects(f.sandbox.saveAs());
  await assert.rejects(f.sandbox.writeProjectTo('/owned/project.json'));
  assert.deepEqual(f.effects, []);
});
test('playing preserves the existing no-effects timer fast path', () => {
  const f = fixture(r => { delete r.__TAURI__; });
  f.sandbox.state.playing = true;
  assert.equal(f.tick(), undefined);
  assert.deepEqual(f.effects, []);
});
