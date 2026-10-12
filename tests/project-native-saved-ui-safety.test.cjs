'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const test = require('node:test');
const vm = require('node:vm');
const fixtureSource = fs.readFileSync('tests/project-open.test.cjs', 'utf8');
// Reuse the DOM fixture, not any production classifier. Its harness runs the
// complete shipped project IIFE and existing SMProject Save/Save As methods.
const context = vm.createContext({ assert, vm, require, setTimeout, console,
  source: fs.readFileSync('src/js/project.js', 'utf8'),
  documentSource: fs.readFileSync('src/js/project-document.js', 'utf8'),
  entrySource: fs.readFileSync('src/js/project-entry.js', 'utf8') });
const fixtureStart = fixtureSource.indexOf('function harness('), fixtureEnd = fixtureSource.indexOf('\nfunction select(', fixtureStart);
assert.ok(fixtureStart >= 0 && fixtureEnd > fixtureStart);
vm.runInContext(fixtureSource.slice(fixtureStart, fixtureEnd) + '\nthis.makeHarness=harness;', context);
const A = '{"native":"A","paint":[20,80]}', B = '{"native":"B","paint":[40,60]}';
function deferred() { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }
function setup({ pickGate, writeGate, dirtyDot = false } = {}) {
  const current = { id: 'native-A', revision: 7, generation: 1, bytes: A };
  const writes = [], removes = [], picks = [], paper = [];
  const app = context.makeHarness({ dirtyDot, configureWindow(root) {
    root.SM.exportJSON = () => { paper.push('export'); throw Error('Paper is forbidden'); };
    root.NemoNativeOpacityCutover = { blocksLegacy: () => true, isActive: () => true,
      identity: () => ({ instanceId: 'native-instance', documentId: current.id, contentRevision: current.revision }),
      getNativeIdentity: () => ({ documentId: current.id, generation: current.generation }),
      persistenceJSON: () => current.bytes };
    root.__TAURI__ = { core: { invoke() {} },
      dialog: { save(options) { picks.push(options); return pickGate ? pickGate.promise : Promise.resolve('/owned/A.json'); } },
      fs: { async writeTextFile(path, bytes) { writes.push([path, bytes]); if (writeGate) await writeGate.promise; },
        async rename(from, to) { writes.push(['rename', from, to]); }, async remove(path) { removes.push(path); } } };
  } });
  app.project.enterEditor();
  const state = () => JSON.stringify({ label: app.project.getCurrentLabel(),
    tabs: app.element('project-tabs-list').children.map(row => ({ id: row.dataset.tab, name: row.children[2].textContent })),
    storage: app.writes, native: current });
  return { app, current, writes, removes, picks, paper, state };
}
async function nextTurn() { await new Promise(resolve => setImmediate(resolve)); }
for (const [field, change] of [
  ['document', h => { h.current.id = 'native-B'; h.current.generation++; h.current.bytes = B; }],
  ['revision', h => { h.current.revision++; h.current.bytes = B; }],
  ['lifecycle', h => { h.current.generation++; }],
  ['controller', h => { h.app.window.NemoNativeOpacityCutover = { ...h.app.window.NemoNativeOpacityCutover }; }],
  ['UI module', h => { h.app.window.SMProject = { ...h.app.window.SMProject }; }],
  ['filesystem receiver', h => { h.app.window.__TAURI__.fs = { ...h.app.window.__TAURI__.fs }; }],
  ['UI name', h => {
    const row = h.app.element('project-tabs-list').children[0];
    row.listeners.dblclick({ stopPropagation() {} });
    row.children[2].value = 'Renamed'; row.children[2].listeners.blur(); }],
]) {
  test(`Save As picker held across ${field} drift writes/stamps nothing`, async () => {
    const gate = deferred(), h = setup({ pickGate: gate });
    const saving = h.app.project.saveAs(); await nextTurn();
    change(h); const before = h.state(); gate.resolve('/owned/A.json');
    await saving;
    assert.deepEqual(h.writes, []);
    assert.equal(h.state(), before);
    assert.equal(h.app.toasts.some(message => /^Saved/.test(message)), false);
    assert.deepEqual(h.paper, []);
  });
}
for (const field of ['document', 'revision', 'lifecycle']) {
  test(`Save As writer held across ${field} drift retains pinned file and current UI`, async () => {
    const gate = deferred(), h = setup({ writeGate: gate });
    const saving = h.app.project.saveAs(); await nextTurn();
    if (field === 'document') h.current.id = 'native-B';
    if (field === 'revision') h.current.revision++;
    if (field === 'lifecycle') h.current.generation++;
    h.current.bytes = B;
    const before = h.state(); gate.resolve(); await saving;
    assert.equal(h.writes[0][1], A, 'writer only received frozen original bytes');
    assert.equal(h.state(), before);
    assert.equal(h.app.project.isDirty(), true);
    assert.equal(h.app.toasts.some(message => /^Saved/.test(message)), false);
    assert.deepEqual(h.removes, []); assert.deepEqual(h.paper, []);
  });
}
test('ordinary Save pins its original content before pending filesystem work', async () => {
  const h = setup(); await h.app.project.saveAs();
  h.current.bytes = B; h.current.revision++;
  const gate = deferred();
  h.app.window.__TAURI__.fs.writeTextFile = async (path, bytes) => { h.writes.push([path, bytes]); await gate.promise; };
  const saving = h.app.project.save(); await nextTurn();
  h.current.bytes = A; h.current.revision++;
  const before = h.state(), notices = h.app.toasts.length;
  gate.resolve(); await saving;
  assert.equal(h.state(), before);
  assert.equal(h.app.toasts.slice(notices).some(message => /^Saved/.test(message)), false);
});
test('successful real desktop Save As followed by Save updates exact native baseline once', async () => {
  const h = setup(); await h.app.project.saveAs();
  assert.equal(h.app.project.getCurrentLabel(), 'A — /owned/A.json');
  assert.equal(h.app.element('project-tabs-list').children[0].children[2].textContent, 'A');
  assert.equal(h.app.project.isDirty(), false);
  h.current.bytes = B; h.current.revision++;
  assert.equal(h.app.project.isDirty(), true);
  await h.app.project.save();
  assert.equal(h.app.project.isDirty(), false);
  assert.equal(h.writes.at(-2)[1], B);
  assert.equal(h.app.toasts.filter(message => /^Saved/.test(message)).length, 2);
  assert.deepEqual(h.paper, []);
});
test('cancelled real picker performs no write or acknowledgement', async () => {
  const gate = deferred(), h = setup({ pickGate: gate });
  const before = h.state(), saving = h.app.project.saveAs(); gate.resolve(null); await saving;
  assert.equal(h.state(), before); assert.deepEqual(h.writes, []);
  assert.equal(h.app.toasts.some(message => /^Saved/.test(message)), false);
});
test('overlapping real Save requests never collide in the existing P29 staging file', async () => {
  const gate = deferred(), h = setup({ writeGate: gate });
  const first = h.app.project.saveAs(); await nextTurn();
  const second = await h.app.project.save();
  assert.equal(second.status, 'save-in-progress');
  assert.equal(h.picks.length, 1); assert.equal(h.writes.length, 1);
  gate.resolve(); await first;
  assert.equal(h.writes.length, 2);
  assert.equal(h.app.toasts.filter(message => /^Saved/.test(message)).length, 1);
});

for (const boundary of ['name', 'label', 'recents', 'baseline render']) {
  test(`real UI ${boundary} commit failure stays written-indeterminate and dirty without retry`, async () => {
    const h = setup();
    if (boundary === 'name') h.app.window.SMProjectDocument = { ...h.app.window.SMProjectDocument,
      baseName() { throw Error('name commit failed'); } };
    if (boundary === 'label') Object.defineProperty(h.app.element('proj-current'), 'textContent', { set() { throw Error('label commit failed'); } });
    if (boundary === 'recents') {
      const push = h.app.writes.push;
      h.app.writes.push = function(entry) { if (entry[0] === 'nemo-recents') throw Error('recents commit failed'); return push.call(this, entry); };
    }
    if (boundary === 'baseline render') h.app.element('project-tabs-list').appendChild = () => { throw Error('baseline render failed'); };
    const result = await h.app.project.saveAs();
    assert.equal(result.status, 'written-indeterminate');
    assert.equal(result.written, true);
    assert.equal(h.app.project.isDirty(), true);
    assert.equal(h.app.toasts.some(message => /^Saved/.test(message)), false);
    assert.equal(h.writes.length, 2, 'one original write and publication only');
    assert.deepEqual(h.removes, []);
    assert.equal(h.app.toasts.at(-1), 'The file was written, but its saved status could not be applied to the current project.');
  });
}
function ticketFixture() {
  const module = require('../src/js/adapters/project-native-save.js');
  const fields = { tabId: 'tab-A', path: null, name: 'A', canvasW: 320, canvasH: 180, fps: 12 };
  const ui = {}, api = {}, fs = {}, effects = [];
  const native = { isActive: () => true, blocksLegacy: () => true,
    identity: () => ({ instanceId: 'i', documentId: 'd', contentRevision: 7 }),
    getNativeIdentity: () => ({ documentId: 'd', generation: 2 }) };
  let ready = false, commit = () => { ready = true; effects.push('commit'); return true; }, controller;
  controller = module.createSavedUI({ read: () => ({ ...fields, ui, api, fs, native }), serialize: () => A,
    pick: async () => '/owned/A.json', publish: async (path, json) => { assert.equal(json, A); effects.push('publish'); },
    commit: (...args) => commit(...args), invalidate: () => { ready = false; effects.push('invalidate'); },
    notice: value => effects.push(value.status), failure: () => effects.push('failure') });
  return { controller, fields, effects, ready: () => ready, setCommit: value => { commit = value; } };
}
test('actual timed dirty-dot refresh cannot clean a failed baseline UI acknowledgement', async () => {
  const h = setup({ dirtyDot: true });
  h.app.element('project-tabs-list').appendChild = () => { throw Error('baseline render failed'); };
  assert.equal((await h.app.project.saveAs()).status, 'written-indeterminate');
  assert.equal(h.app.project.isDirty(), true);
  h.app.project.refreshActiveTabDirtyDot(A);
  assert.equal(h.app.dot.classList.contains('dirty'), true,
    'matching saved bytes do not override unavailable saved-UI readiness');
});
for (const kind of ['missing owner', 'serialization', 'lifecycle admission', 'picker']) {
  test(`actual production ${kind} save refusal reports one visible failure with no IO/clean stamp`, async () => {
    const h = setup(), before = h.state();
    if (kind === 'missing owner') delete h.app.window.NemoNativeOpacityCutover;
    if (kind === 'serialization') h.app.window.NemoNativeOpacityCutover.persistenceJSON = () => { throw Error('serialization unavailable'); };
    if (kind === 'lifecycle admission') h.app.window.NemoNativeOpacityCutover.getNativeIdentity = () => { throw Error('lifecycle unavailable'); };
    if (kind === 'picker') h.app.window.__TAURI__.dialog.save = async () => { throw Error('picker unavailable'); };
    await assert.rejects(h.app.project.saveAs());
    assert.equal(h.state(), before);
    assert.deepEqual(h.writes, []); assert.deepEqual(h.paper, []);
    assert.equal(h.app.toasts.filter(message => /^Saved/.test(message)).length, 0);
    assert.equal(h.app.toasts.length, 1);
    assert.match(h.app.toasts[0], /^toastSaveFailedSuffix/);
  });
}
test('same ticket duplicate/reentrant acknowledgement publishes and notices once', async () => {
  const h = ticketFixture(), ticket = h.controller.begin();
  h.setCommit(() => {
    assert.equal(h.controller.acknowledge(ticket, '/owned/A.json', 'save-as').status, 'refused');
    h.effects.push('commit'); return true;
  });
  assert.equal((await h.controller.write('/owned/A.json', ticket, 'save-as')).status, 'saved-current');
  assert.equal((await h.controller.write('/owned/A.json', ticket, 'save-as')).status, 'refused');
  assert.equal(h.controller.acknowledge(ticket, '/owned/A.json', 'save-as').status, 'refused');
  assert.deepEqual(h.effects, ['publish', 'commit', 'saved-current']);
});
test('a newer UI attempt consumes eligibility of an older held ticket before IO', async () => {
  const h = ticketFixture(), old = h.controller.begin(); h.controller.begin();
  assert.equal((await h.controller.write('/owned/A.json', old, 'save-as')).status, 'stale-before-write');
  assert.deepEqual(h.effects, ['stale-before-write']);
});
test('no UI acknowledgement is possible before actual publisher completion', () => {
  const h = ticketFixture(), ticket = h.controller.begin();
  assert.equal(h.controller.acknowledge(ticket, '/owned/A.json', 'save-as').status, 'refused');
  assert.deepEqual(h.effects, []);
});
test('thenable commit is explicitly indeterminate, never accepted as synchronous UI success', async () => {
  const h = ticketFixture(), ticket = h.controller.begin();
  h.setCommit(() => Promise.resolve(true));
  assert.equal((await h.controller.write('/owned/A.json', ticket, 'save-as')).status, 'written-indeterminate');
  assert.equal(h.ready(), false);
  assert.deepEqual(h.effects, ['publish', 'invalidate', 'written-indeterminate']);
});
test('explicit UI token path/name change before dispatch refuses without a publisher call', async () => {
  const h = ticketFixture(), ticket = h.controller.begin();
  h.fields.path = '/owned/another.json'; h.fields.name = 'another';
  assert.equal((await h.controller.write('/owned/A.json', ticket, 'save-as')).status, 'stale-before-write');
  assert.deepEqual(h.effects, ['stale-before-write']);
});
