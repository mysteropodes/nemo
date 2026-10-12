'use strict';
const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const test = require('node:test');
const A = '{"native":"A ☃","opacity":[20,80]}\n', B = '{"native":"B","opacity":[40,60]}\n';
const OLD = '{"recovery":"previous-good-slot"}';
const fixture = fs.readFileSync('tests/project-open.test.cjs', 'utf8');
const start = fixture.indexOf('function harness('), end = fixture.indexOf('\nfunction select(', start);
assert.ok(start >= 0 && end > start);
const localStart = fixture.indexOf('localStorage: { getItem(key)', start);
const localEnd = fixture.indexOf(', state: {}', localStart);
assert.ok(localStart > start && localEnd > localStart && localEnd < end);
// Replace ONLY the disposable DOM fixture's storage port. The complete shipped
// project entry, public SMProject method and entry helper are evaluated unchanged.
const harnessSource = fixture.slice(start, localStart) + 'localStorage: storage' + fixture.slice(localEnd, end);
function setup({ browser = false, quota = false, idbReject = false, bootstrapFailure = false, change = () => {} } = {}) {
  const effects = [], slots = { 'nemo-auto': OLD };
  const identity = { instanceId: 'instance-A', documentId: 'document-A', contentRevision: 7 };
  const storage = { getItem: key => slots[key] ?? null,
    setItem(key, value) { effects.push(['LS', key, value]); if (quota) throw Error('quota'); slots[key] = value; },
    removeItem(key) { effects.push(['remove', key]); delete slots[key]; } };
  const context = vm.createContext({ assert, vm, require, setTimeout, console, storage,
    source: fs.readFileSync('src/js/project.js', 'utf8'),
    documentSource: fs.readFileSync('src/js/project-document.js', 'utf8'),
    entrySource: fs.readFileSync('src/js/project-entry.js', 'utf8') });
  vm.runInContext(harnessSource + '\nthis.makeHarness=harness;', context);
  const app = context.makeHarness({ allowBootstrapFailure: bootstrapFailure, configureWindow(root) {
    root.SM.exportJSON = () => { effects.push(['Paper']); throw Error('Paper forbidden'); };
    root.SMIdb = { set(key, value) { effects.push(['IDB', key, value]); return idbReject ? Promise.reject(Error('IDB failure')) : Promise.resolve(); } };
    if (!browser) {
      root.__TAURI__ = { core: { invoke() { throw Error('native command forbidden'); } },
        fs: { writeTextFile() { throw Error('filesystem forbidden'); } } };
      root.NemoNativeOpacityCutover = { blocksLegacy: () => true, isActive: () => true,
        identity: () => identity, persistenceJSON: () => A };
    }
    change(root, identity);
  } });
  app.window.SMProject.pushVersionSnapshot = () => { throw Error('history forbidden'); };
  effects.length = 0;
  assert.equal(!!app.bootstrapError, bootstrapFailure, 'pre-existing bootstrap failure remains explicit');
  return { app, effects, slots, identity };
}
for (const [name, change] of [
  ['no owner/native object', root => { delete root.NemoNativeOpacityCutover; }],
  ['unknown owner', root => { root.NemoNativeOpacityCutover = null; }],
  ['malformed API', root => { root.__TAURI__ = {}; }],
  ['packaged missing API', root => { delete root.__TAURI__; root.location = { protocol: 'tauri:', hostname: 'localhost', port: '' }; }],
  ['missing method', root => { delete root.NemoNativeOpacityCutover.identity; }],
  ['legacy phase on desktop', root => { root.NemoNativeOpacityCutover.blocksLegacy = () => false; }],
  ...['closed', 'replacing', 'indeterminate', 'synchronizing'].map(name => [name, root => { root.NemoNativeOpacityCutover.isActive = () => false; }]),
  ['unreadable native bytes', root => { root.NemoNativeOpacityCutover.persistenceJSON = () => { throw Error('unreadable'); }; }],
  ['null native bytes', root => { root.NemoNativeOpacityCutover.persistenceJSON = () => null; }],
  ['unsafe revision', (root, id) => { id.contentRevision = Number.MAX_SAFE_INTEGER + 1; }],
  ['identity changes while acquiring', (root, id) => { root.NemoNativeOpacityCutover.persistenceJSON = () => { id.contentRevision++; return A; }; }],
  ['controller replacement during acquisition', root => { root.NemoNativeOpacityCutover.persistenceJSON = () => { root.NemoNativeOpacityCutover = null; return A; }; }],
]) {
  test(`actual public direct autosave refuses ${name}, preserving old recovery slot`, async () => {
    const h = setup({ change });
    h.app.project.autosaveWrite(A);
    assert.deepEqual(h.effects, []); assert.equal(h.slots['nemo-auto'], OLD);
    await Promise.resolve(); assert.deepEqual(h.effects, []);
  });
}
for (const [name, bytes] of [['other document', B], ['empty', ''], ['undefined', undefined],
  ['null', null], ['number', 42], ['object', { toString() { throw Error('input coercion forbidden'); } }], ['Promise', Promise.resolve(A)]]) {
  test(`actual direct desktop ${name} input has zero LS/IDB/quota effects`, () => {
    const h = setup({ quota: true }); h.app.project.autosaveWrite(bytes);
    assert.deepEqual(h.effects, []); assert.equal(h.slots['nemo-auto'], OLD);
  });
}
test('exact current admitted native bytes preserve LS then IDB mirror ordering', () => {
  const h = setup(); h.app.project.autosaveWrite(A);
  assert.deepEqual(h.effects, [['LS', 'nemo-auto', A], ['IDB', 'nemo-auto', A]]);
  assert.equal(h.slots['nemo-auto'], A);
});
test('admitted quota failure retains the P34 LS attempt -> IDB -> cleanup order', () => {
  const h = setup({ quota: true }); h.app.project.autosaveWrite(A);
  assert.deepEqual(h.effects, [['LS', 'nemo-auto', A], ['IDB', 'nemo-auto', A], ['remove', 'nemo-auto']]);
  assert.equal('nemo-auto' in h.slots, false);
});
test('admitted asynchronous IDB failure keeps previous ordering and current LS bytes', async () => {
  const h = setup({ idbReject: true }); h.app.project.autosaveWrite(A); await Promise.resolve();
  assert.deepEqual(h.effects, [['LS', 'nemo-auto', A], ['IDB', 'nemo-auto', A]]);
  assert.equal(h.slots['nemo-auto'], A);
});
for (const protocol of ['http:', 'https:']) test(`ordinary ${protocol} browser preserves direct opaque passthrough`, () => {
  const h = setup({ browser: true, change: root => { root.location.protocol = protocol; } });
  h.app.project.autosaveWrite(B);
  assert.deepEqual(h.effects, [['LS', 'nemo-auto', B], ['IDB', 'nemo-auto', B]]);
});
for (const kind of ['null API', 'throwing API']) test(`${kind} direct entry still has zero storage effects despite retained bootstrap error`, () => {
  const h = setup({ bootstrapFailure: true, change(root) {
    if (kind === 'null API') root.__TAURI__ = null;
    else Object.defineProperty(root, '__TAURI__', { get() { throw Error('API inaccessible'); } });
  } });
  h.app.project.autosaveWrite(A); assert.deepEqual(h.effects, []); assert.equal(h.slots['nemo-auto'], OLD);
});
test('explicitly present undefined desktop marker does not become browser permission', () => {
  const h = setup({ change: root => { root.__TAURI__ = undefined; } });
  h.app.project.autosaveWrite(A); assert.deepEqual(h.effects, []); assert.equal(h.slots['nemo-auto'], OLD);
});
test('missing/malformed entry validator cannot bypass the public storage gate', () => {
  for (const admission of [undefined, null, {}]) {
    const h = setup(); h.app.window.NemoNativeOpacityProjectEntry = admission;
    assert.equal(h.app.project.autosaveWrite(A).status, 'unavailable');
    assert.deepEqual(h.effects, []); assert.equal(h.slots['nemo-auto'], OLD);
  }
});
test('browser phase read that exposes desktop ownership is refused', () => {
  const h = setup({ browser: true, change(root) { root.NemoNativeOpacityCutover = {
    blocksLegacy() { root.__TAURI__ = { core: { invoke() {} } }; return false; } }; } });
  delete h.app.window.__TAURI__; // Restore browser start; expose desktop during THIS admission.
  h.app.project.autosaveWrite(B); assert.deepEqual(h.effects, []); assert.equal(h.slots['nemo-auto'], OLD);
});
