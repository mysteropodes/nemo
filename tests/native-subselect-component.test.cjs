'use strict';
// #2356 A-subselect-in-component: Direct Select edits the shapes of the ENTERED component (baseline subselect-bridge.js ran while
// state.activeSymbolId was set). The real controller and contract run against the shared fixture, turned into an entered component: the
// workspace controller holds `comp-a`, the viewport presents it, the component's own layer is the active row, and the edit goes through the
// component command route (never the scene resource route) with the record's own context in its target.
const test = require('node:test'), { afterEach } = require('node:test'), assert = require('node:assert/strict');
const Contract = require('../src/js/adapters/native-subselect-contract.js');
const { fixture } = require('./helpers/native-subselect.cjs');

const CTX = 'comp-a';
function entered() {
  const f = fixture(), routes = [];
  const scene = f.s.runResourceCommand;
  f.s.runResourceCommand = (action) => { routes.push('scene'); return scene(action); };
  f.s.runComponentCommand = (action) => { routes.push('component'); return scene(action); };
  const reads = [];
  const read = f.s.readSubselect;
  f.s.readSubselect = (selector, pin) => { reads.push(selector); return read(selector, pin); };
  // The viewport presents the component; the layer table of the pin does not list the component's layer.
  f.s.readPresentedObjectSession().presentation.contextId = CTX;
  f.item.stableTarget.contextId = CTX;
  f.item.stableTarget.layerUid = 'inner';
  f.root.state.activeSymbolId = CTX;
  f.root.state.layers = [{ layerUid: 'inner' }]; f.root.state.activeLayerIdx = 0; f.root._layerSel = [0];
  // The contract reads the held component from the page global (the bridge's root is that page in production).
  f.root.NemoNativeComponentContext = globalThis.NemoNativeComponentContext = { contextId: () => CTX, layers: () => [{ layerUid: 'inner', locked: false, visible: true }] };
  return { f, routes, reads };
}

afterEach(() => { delete globalThis.NemoNativeComponentContext; });

test('Direct Select in an entered component reads the component context and commits through the component route with the record context', async () => {
  const { f, routes, reads } = entered();
  await f.select();
  assert.deepEqual(reads.map((r) => [r.contextId, r.layerUid]), [[CTX, 'inner']], 'the listing is asked for the held component and its layer');
  f.drag(); f.controller.handle('up', f.event(52, 34));
  const result = await f.controller.settled();
  assert.equal(result.status, 'applied', f.notices.join(';'));
  assert.deepEqual(routes, ['component'], 'the component route, never the scene resource route');
  assert.equal(f.sent.length, 1);
  assert.equal(f.sent[0].payload.stableTarget.contextId, CTX);
  assert.equal(f.sent[0].payload.stableTarget.layerUid, 'inner');
  assert.equal(f.sent[0].payload.command, 'object.subselect.edit');
});

test('the scene root still uses the scene resource route and the pin layer table', async () => {
  const f = fixture(), routes = [];
  const scene = f.s.runResourceCommand;
  f.s.runResourceCommand = (action) => { routes.push('scene'); return scene(action); };
  f.s.runComponentCommand = (action) => { routes.push('component'); return scene(action); };
  await f.select(); f.drag(); f.controller.handle('up', f.event(52, 34));
  assert.equal((await f.controller.settled()).status, 'applied');
  assert.deepEqual(routes, ['scene']);
  assert.equal(f.sent[0].payload.stableTarget.contextId, 'scene-root');
});

test('a component without the route, a locked or hidden component layer, and a context that is not the held one never edit', async () => {
  // No component route: the edit is refused, nothing is sent.
  const a = entered(); delete a.f.s.runComponentCommand;
  await a.f.select(); a.f.drag(); a.f.controller.handle('up', a.f.event(52, 34));
  assert.equal((await a.f.controller.settled()).status, 'refused');
  assert.equal(a.f.sent.length, 0);
  // A locked component layer is not selectable: Direct Select reports the layer as unavailable.
  const b = entered(); b.f.root.NemoNativeComponentContext.layers = () => [{ layerUid: 'inner', locked: true, visible: true }];
  b.f.controller.handle('down', b.f.event(100, 60)); await b.f.controller.settled();
  assert.equal(b.reads.length, 0, 'no read of a locked layer');
  // The presentation is another context than the held one: the mode is unavailable.
  const c = entered(); c.f.s.readPresentedObjectSession().presentation.contextId = 'scene-root';
  c.f.controller.handle('down', c.f.event(100, 60)); await c.f.controller.settled();
  assert.equal(c.reads.length, 0);
});

test('the subselect contract admits the held component for an edit, a read and a preview, and nothing else', () => {
  const edit = (contextId) => ({ command: 'object.subselect.edit', stableTarget: { contextId, layerUid: 'inner', frameScope: { kind: 'authored', frame: 0 }, strokeId: 'a' }, intent: { kind: 'move', indices: [0], delta: [1, 1] } });
  globalThis.NemoNativeComponentContext = { contextId: () => CTX };
  try {
    Contract.payload(Contract.EDIT, edit('scene-root'));
    Contract.payload(Contract.EDIT, edit(CTX));
    for (const bad of ['comp-b', '', 'a b']) assert.throws(() => Contract.payload(Contract.EDIT, edit(bad)), /Invalid native subselect intent/, bad);
    const read = (contextId) => ({ atRevision: 3, documentSnapshotId: 'native-object:d:3', stableTarget: edit(contextId).stableTarget, subselect: { cursor: 0, limit: 16, preview: { kind: 'move', indices: [0], delta: [1, 1] } } });
    Contract.readPayload(read(CTX));
    assert.throws(() => Contract.readPayload(read('comp-b')), /Invalid subselect read selector/);
    const view = { atRevision: 3, documentSnapshotId: 'native-object:d:3', view: { kind: 'subselect', contextId: CTX, layerUid: 'inner', frame: 0, workId: 'native-work-1', viewGeneration: 1, cursor: 0, limit: 8, preview: { stableTarget: edit(CTX).stableTarget, intent: { kind: 'move', indices: [0], delta: [1, 1] } } } };
    Contract.viewPayload(view);
    view.view.preview.stableTarget = edit('scene-root').stableTarget;
    assert.throws(() => Contract.viewPayload(view), /Invalid subselect presentation selector/, 'the preview target names the view context');
  } finally { delete globalThis.NemoNativeComponentContext; }
});
