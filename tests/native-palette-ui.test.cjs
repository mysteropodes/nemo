'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const Contract = require('../src/js/adapters/native-palette-contract.js');
const fs = require('node:fs'), vm = require('node:vm');
const ProjectFile = require('../src/js/adapters/native-object-project-file.js');
const { fixture, clone, node } = require('./helpers/native-palette-fixture.cjs');

test('ordinary palette uses the actual application SDK and preserves duplicate IDs and exact colors without a JS mirror', async () => {
  const h = fixture(), before = h.metadata(); await h.ready();
  assert.deepEqual(h.tabs().map(n => n.textContent), ['First', 'Second']);
  assert.deepEqual(h.swatches().map(n => n.title), ['#AbC', '#abc', '#00000000']);
  assert.deepEqual(h.metadata(), before); assert.equal(h.edits().length, 0);
  const count = h.calls.length; await h.ready(); assert.equal(h.calls.length, count);
  assert.equal(h.elements['btn-palette-new'].disabled, false);
  assert.equal(h.elements['btn-palette-replace'].disabled, false, 'Replace is native since #2372 PAL-REPLACE'); assert.equal(h.elements['btn-palette-swap'].disabled, false, 'Swap is native since #2372 PAL-APPLY');
});

test('ordinary palette presentation collects complete summaries and active colors across bounded SDK pages', async () => {
  const palettes = Array.from({ length: 19 }, (_, i) => ({ id: 'duplicate', name: 'Palette ' + i,
    colors: i === 18 ? Array.from({ length: 35 }, (_, j) => '#' + j.toString(16).padStart(6, '0')) : [] }));
  const h = fixture({ metadata: { palettes, activePaletteIdx: 18 } }); await h.ready();
  assert.equal(h.tabs().length, 19); assert.equal(h.swatches().length, 35);
  assert.deepEqual(h.calls.filter(q => q.payload.selector.kind === 'palettePage').map(q => q.payload.selector.cursor), [0, 16]);
  assert.deepEqual(h.calls.filter(q => q.payload.selector.kind === 'colorPage').map(q => q.payload.selector.cursor), [0, 16, 32]);
  assert.equal(h.edits().length, 0); assert.deepEqual(h.metadata().palettes, palettes);
});

test('ordinary create, rename, duplicate, select and delete submit field-specific native intents', async () => {
  const h = fixture(); await h.ready(); h.prompts.push('New'); h.elements['btn-palette-new'].fire('click'); await h.settled();
  assert.equal(h.edits().at(-1).payload.action.kind, 'create'); assert.equal(h.controller.read().palette.active.summary.name, 'New');
  h.prompts.push('Renamed'); h.tabs()[2].fire('click'); h.tabs()[2].fire('dblclick'); await h.settled();
  assert.deepEqual(h.edits().at(-1).payload.action, { kind: 'rename', ordinal: 3, name: 'Renamed' });
  h.prompts.push('From menu'); h.tabs()[2].fire('contextmenu'); h.menus.at(-1).find(x => x.label === 'Renommer…').action(); await h.settled();
  assert.deepEqual(h.edits().at(-1).payload.action, { kind: 'rename', ordinal: 3, name: 'From menu' }, 'the tab menu renames through the same native intent (#2378)');
  h.tabs()[2].fire('contextmenu'); h.menus.at(-1).find(x => x.label === 'Dupliquer').action(); await h.settled();
  assert.equal(h.edits().at(-1).payload.action.kind, 'duplicate'); assert.equal(h.metadata().palettes.length, 4);
  h.tabs()[0].fire('click'); await h.settled(); assert.deepEqual(h.edits().at(-1).payload.action, { kind: 'select', ordinal: 1 });
  h.tabs()[3].fire('contextmenu'); h.menus.at(-1).find(x => x.label === 'Supprimer').action(); await h.settled();
  assert.deepEqual(h.edits().at(-1).payload.action, { kind: 'remove', ordinal: 4 }); assert.equal(h.metadata().palettes.length, 3);
});

test('ordinary add, remove and drag reorder colors use one-based native positions and confirmed readback', async () => {
  const h = fixture(); await h.ready(); h.elements['btn-palette-add-fill'].fire('click'); await h.settled();
  assert.deepEqual(h.edits().at(-1).payload.action, { kind: 'appendColor', ordinal: 1, color: '#aBc' });
  h.swatches()[1].fire('contextmenu'); h.menus.at(-1).find(x => x.label === 'Retirer de la palette').action(); await h.settled();
  assert.deepEqual(h.edits().at(-1).payload.action, { kind: 'removeColor', ordinal: 1, index: 2 });
  const before = h.metadata(); h.swatches()[0].fire('dragstart');
  assert.deepEqual(h.metadata(), before); const count = h.edits().length;
  h.swatches()[2].fire('drop'); await h.settled(); assert.equal(h.edits().length, count + 1);
  assert.deepEqual(h.edits().at(-1).payload.action, { kind: 'moveColor', ordinal: 1, from: 1, to: 3 });
  assert.deepEqual(h.swatches().map(n => n.title), ['#00000000', '#aBc', '#AbC']);
});

test('retained menu, prompt and drag cannot target the same ordinal after document replacement', async () => {
  for (const kind of ['menu', 'prompt', 'drag']) {
    const h = fixture(); await h.ready(); let action;
    if (kind === 'menu') { h.tabs()[0].fire('contextmenu'); action = h.menus.at(-1).find(x => x.label === 'Supprimer').action; }
    if (kind === 'drag') { h.swatches()[0].fire('dragstart'); const target = h.swatches()[1]; action = () => target.fire('drop'); }
    if (kind === 'prompt') { h.root.prompt = () => { h.replace(); return 'Stale'; }; h.tabs()[0].fire('dblclick'); }
    else { h.replace(); await h.ready(); action(); }
    await h.settled(); assert.equal(h.edits().length, 0); assert(h.notices.length > 0);
  }
});

test('subscription change, unavailable admission and retirement refuse ordinary writes without browser fallback', async () => {
  const h = fixture(); await h.ready(); const old = h.tabs()[1]; h.resubscribe(); old.fire('click'); await h.settled();
  assert.equal(h.edits().length, 0); await h.ready(); h.refuse(); h.elements['btn-palette-add-stroke'].fire('click'); await h.settled();
  assert.equal(h.edits().length, 1); assert.equal(h.controller.read(), null); assert(h.notices.length > 0);
  h.retire(); h.elements['btn-palette-new'].fire('click'); await h.settled(); assert.equal(h.edits().length, 1);
});

test('pending edit retires gestures and never publishes an optimistic palette', async () => {
  const h = fixture(); await h.ready(); const held = h.hold(), prior = h.metadata(), retained = h.tabs()[1];
  h.elements['btn-palette-add-fill'].fire('click'); await held.started;
  assert.equal(h.controller.read(), null); assert.equal(h.elements['btn-palette-new'].disabled, true);
  retained.fire('click'); assert.equal(h.edits().length, 1); assert.deepEqual(h.metadata(), prior);
  held.release(); await h.settled(); assert.equal(h.edits().length, 1); assert.equal(h.swatches().length, 4);
});

test('raw invalid active selection and missing palettes do not materialize defaults during ordinary presentation', async () => {
  for (const metadata of [{ palettes: undefined }, { activePaletteIdx: -1 }, { activePaletteIdx: 0.5 }, { palettes: [] }]) {
    const h = fixture({ metadata });
    // JSON import represents absence rather than an explicit undefined field.
    await h.ready(); assert.equal(h.controller.read().palette.active, null); assert.equal(h.edits().length, 0);
    assert.equal(h.elements['btn-palette-new'].disabled, false); assert.equal(h.elements['btn-palette-add-fill'].disabled, true);
  }
});

test('retirement during paged palette read cannot publish stale controls', async () => {
  const h = fixture(), held = h.hold(), reading = h.controller.presented(); await held.started;
  h.retire(); held.release(); assert.equal(await reading, null); await h.settled();
  assert.equal(h.controller.read(), null); assert.equal(h.elements['btn-palette-new'].disabled, true);
});

test('a new presented document during a held read is refreshed once after the stale read settles', async () => {
  const h = fixture(), held = h.hold(), reading = h.controller.presented(); await held.started;
  h.replace(); h.controller.presented(); held.release(); await reading; await h.settled();
  assert.equal(h.controller.read().info.documentId, 'replacement');
  assert.equal(h.elements['btn-palette-new'].disabled, false);
  const settings = h.calls.filter(q => q.payload.selector && q.payload.selector.kind === 'settings');
  assert.equal(settings.length, 2); assert.deepEqual(settings.map(q => q.documentId), ['d', 'replacement']);
});

test('unavailable read does not retry itself and a new presentation after pending failure remains consumable', async () => {
  const unavailable = fixture({ unavailable: true }); await unavailable.ready();
  assert.equal(unavailable.calls.length, 1); assert.equal(unavailable.controller.read(), null);
  const h = fixture(); await h.ready(); const before = h.metadata(), held = h.hold(); h.elements['btn-palette-add-fill'].fire('click'); await held.started;
  h.replace(); h.controller.presented(); held.release(); await h.settled();
  assert.equal(h.controller.read().info.documentId, 'replacement');
  assert.equal(h.edits().length, 1); assert.equal(h.elements['btn-palette-new'].disabled, false);
  assert.deepEqual(h.metadata(), before);
});

test('an unchanged failed Palette presentation stays unavailable across passive callbacks and temporary public fences', async () => {
  const h = fixture({ unavailable: true });
  h.present(false); await h.ready(); assert.equal(h.calls.length, 0, 'No real public presentation means no read attempt');
  h.present(true); await h.ready(); assert.equal(h.calls.length, 1);
  h.refuse(false);
  await h.ready(); await h.ready(); assert.equal(h.calls.length, 1, 'A healthy transport alone does not authorize a retry');
  h.present(false); await h.ready(); h.present(true); await h.ready();
  assert.equal(h.calls.length, 1, 'A busy fence must preserve the prior failed-presentation stamp');
  assert.equal(h.controller.read(), null); assert.equal(h.elements['btn-palette-new'].disabled, true);
  assert.equal(h.edits().length, 0); assert.deepEqual(h.notices, []);
  h.retire(); h.renewPresentation(); await h.ready(); assert.equal(h.calls.length, 1);
});

test('Palette read refusal permits recovery for a new presentation, owner, revision, subscription or projection epoch', async () => {
  for (const change of ['presentation', 'session', 'document', 'revision', 'subscription', 'projection']) {
    const h = fixture({ unavailable: true }); await h.ready();
    const before = h.metadata(); assert.equal(h.calls.length, 1, change);
    h.refuse(false); await h.ready(); assert.equal(h.calls.length, 1, change);
    if (change === 'presentation') h.renewPresentation();
    if (change === 'session') h.rebind();
    if (change === 'document') h.replace();
    if (change === 'revision') h.replace({ contentRevision: 1, documentSnapshotId: 's1' });
    if (change === 'subscription') h.resubscribe();
    if (change === 'projection') h.controller.willProject(h.fullInfo());
    await h.ready();
    assert.equal(h.calls.filter(q => q.payload.selector && q.payload.selector.kind === 'settings').length, 2, change);
    assert.ok(h.controller.read(), change); assert.equal(h.elements['btn-palette-new'].disabled, false, change);
    assert.deepEqual(h.metadata(), before, change); assert.equal(h.edits().length, 0, change);
    const calls = h.calls.length; await h.ready(); assert.equal(h.calls.length, calls, change);
    h.retire();
  }
});

test('palette UI installation leaves browser palette DOM and button state unchanged', async () => {
  const elements = Object.fromEntries(['palette-grid', 'palette-tabs', 'btn-palette-new'].map(id => [id, node()]));
  elements['palette-grid'].textContent = 'browser palette'; elements['palette-tabs'].textContent = 'browser tabs';
  const root = { document: { getElementById: id => elements[id] || null }, addEventListener() {} };
  const UI = require('../src/js/adapters/native-palette-ui.js'), controller = UI.install(root);
  controller.render(); await controller.presented(); controller.dispose();
  assert.equal(elements['palette-grid'].textContent, 'browser palette');
  assert.equal(elements['palette-tabs'].textContent, 'browser tabs'); assert.equal(elements['btn-palette-new'].disabled, false);
  assert.strictEqual(UI.install(root), controller);
});

test('actual HTML loads the palette protocol before SDK capture and the model before ordinary UI', () => {
  const wanted = ['native-palette-contract.js', 'native-application.js', 'native-palette-desktop.js', 'native-palette-ui.js'];
  const html = fs.readFileSync(require.resolve('../src/index.html'), 'utf8');
  const actual = [...html.matchAll(/<script\b[^>]*\bsrc="js\/adapters\/([^"\n]+)"[^>]*>/g)]
    .map(match => match[1]).filter(file => wanted.includes(file));
  assert.deepEqual(actual, wanted);
  function load(files) {
    const root = { document: { getElementById: () => null }, addEventListener() {} };
    root.globalThis = root; const context = vm.createContext(root);
    for (const file of files) vm.runInContext(fs.readFileSync(require.resolve('../src/js/adapters/' + file), 'utf8'), context);
    return () => vm.runInContext(`NemoNativeApplicationAdapter.validateRequest({apiVersion:2,requestId:'html-palette',
      instanceId:'i',documentId:'d',operation:'query.document.object.project.palette.read',payload:{atRevision:0,
      documentSnapshotId:'s0',contextId:'scene-root',selector:{kind:'settings'}}})`, context);
  }
  assert.doesNotThrow(load(actual));
  assert.throws(load([wanted[1], wanted[0], wanted[2], wanted[3]]), /^TypeError: operation is not admitted$/);
});

test('palette uses full project readiness and retains narrow immutable authority pins', async () => {
  const f = fixture(); await f.ready();
  assert.equal(f.projectionReady(f.fullInfo()), true);
  const origin = f.controller.read(); assert.ok(origin);
  assert.deepEqual(Object.keys(origin.info).sort(),
    ['contentRevision', 'documentId', 'documentSnapshotId', 'instanceId', 'lifecycleGeneration']);
  assert.equal(Object.isFrozen(origin.info), true);
  assert.equal(f.projectionReady(origin.info), false, 'identity pins cannot stand in for project metadata');
  const calls = f.calls.length;
  f.drift(); assert.equal(f.controller.read(), null);
  assert.equal(await f.controller.presented(), null); await f.settled();
  assert.equal(f.controller.read(), null);
  assert.equal(f.calls.length, calls, 'invalid full projection refuses before dispatch');
  assert.equal(f.edits().length, 0);
  f.repairProjection(); await f.controller.presented(); await f.settled();
  assert.ok(f.controller.read());
  assert.equal(f.edits().length, 0, 'restoring readiness does not replay a gesture');
});

// #2372: the create / rename / remove prompts read their origin while the native queue hides the presentation (queue-pending): they wait (bounded) and then prompt.
const ticks = async (n = 6) => { for (let i = 0; i < n; i++) await new Promise(done => setImmediate(done)); };
test('queue-pending: create, rename and remove prompt once the hidden presentation returns, then write once each', async () => {
  const h = fixture(); await h.ready(); const asked = [];
  h.root.prompt = (text, initial) => { asked.push(text); return h.prompts.shift(); };
  h.present(false); h.prompts.push('Waited'); h.elements['btn-palette-new'].fire('click'); await ticks(3);
  assert.deepEqual(asked, [], 'no prompt while the presentation is hidden'); assert.deepEqual(h.notices, []); assert.equal(h.edits().length, 0);
  h.present(true); await ticks(); await h.settled(); assert.equal(asked.length, 1); assert.equal(h.edits().length, 1); assert.deepEqual(h.notices, []);
  await h.ready(); const first = h.tabs()[0]; h.present(false); h.prompts.push('Renamed'); first.fire('dblclick'); await ticks(3);
  assert.equal(asked.length, 1); h.present(true); await ticks(); await h.settled(); assert.equal(asked.length, 2); assert.equal(h.edits().length, 2);
  await h.ready(); const menuTab = h.tabs()[1]; menuTab.fire('contextmenu'); const remove = h.menus.at(-1).find(x => x.label === 'Supprimer').action;
  h.present(false); remove(); await ticks(3); assert.equal(h.edits().length, 2); h.present(true); await ticks(); await h.settled();
  assert.equal(h.edits().length, 3); assert.deepEqual(h.notices, []);
});
test('queue-pending: a real change while the prompt origin waits still refuses; a queue that never frees gives one notice', async () => {
  const h = fixture(); await h.ready(); const asked = []; h.root.prompt = () => { asked.push(1); return 'x'; };
  h.present(false); const tab = h.tabs()[0]; tab.fire('dblclick'); await ticks(2); h.replace(); h.present(true); await ticks(); await h.settled();
  assert.equal(asked.length, 0); assert.equal(h.edits().length, 0); assert.equal(h.notices.length, 1);
  const g = fixture(); await g.ready(); g.root.prompt = () => { asked.push(1); return 'x'; };
  g.present(false); g.elements['btn-palette-new'].fire('click'); await ticks(40);
  assert.equal(asked.length, 0); assert.equal(g.edits().length, 0); assert.equal(g.notices.length, 1);
});
