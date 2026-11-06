'use strict';
// #2256 A-comp-edit-timeline part 2 (JS): the layer-level actions of an entered component (New / Duplicate / Delete, the eye, the padlock,
// the row menu, Cut at the playhead) reach the declared native operations through the component command route only, stamped with the held
// context, against the component's own rows.
const test = require('node:test'), assert = require('node:assert/strict');
globalThis.NemoNativeObjectVectorCommand = require('../src/js/adapters/native-object-vector-command.js');
globalThis.NemoNativeObjectEditContract = require('../src/js/adapters/native-object-edit-contract.js');
globalThis.NemoNativeLayerPlayheadCutContract = require('../src/js/adapters/native-layer-playhead-cut-contract.js');
const Edit = require('../src/js/adapters/native-component-layer-edit.js');
const P = 'command.document.object.';

function harness(options = {}) {
  const sent = [], routes = [], notices = [], selected = [], menus = [];
  let rows = [{ layerUid: 'in_a', name: 'Layer 1', locked: false, visible: true }, { layerUid: 'in_b', name: 'Layer 2', locked: true, visible: false }], revision = 5, serial = 0;
  const pin = () => ({ instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: revision, documentSnapshotId: 's' + revision });
  const context = { presented: () => (options.held === null ? null : { contextId: 'sym_a', totalFrames: 8, fps: 24 }), contextId: () => 'sym_a', layers: () => rows,
    projectionReady: () => options.ready !== false, selectLayer: (host, at) => { selected.push(at); return true; } };
  const run = (route) => (action) => { routes.push(route); return Promise.resolve(action({
    dispatch: async (request) => {
      sent.push(request);
      if (options.refuse) return { apiVersion: 2, requestId: request.requestId, instanceId: 'i', documentId: 'd', contentRevision: revision, ok: false, error: { code: options.refuse } };
      const applied = options.noop !== true;
      if (applied) revision += 1;
      const name = request.operation.slice(P.length), result = { applied, historyEntriesAdded: applied ? 1 : 0 };
      if (applied && ['layer.create', 'layer.duplicate', 'layer.cut'].includes(name)) result.layerUid = 'new_1';
      if (applied && ['layer.create', 'layer.duplicate', 'layer.cut'].includes(name)) rows = rows.concat([{ layerUid: 'new_1', name: 'x', locked: false, visible: true }]);
      return { apiVersion: 2, requestId: request.requestId, instanceId: 'i', documentId: 'd', contentRevision: revision, ok: true, result };
    },
    reconcile: async () => pin(),
  })); };
  const session = { info: pin, isActive: () => true, requestId: (label) => label + '-' + (++serial), runResourceCommand: run('scene') };
  if (options.route !== false) session.runComponentCommand = run('component');
  const el = () => { const e = { listeners: {}, children: [], addEventListener(t, f) { e.listeners[t] = f; }, appendChild(c) { e.children.push(c); return c; } }; return e; };
  const host = { state: { currentFrame: 3, activeLayerIdx: 0 }, _layerSel: options.layerSel || [0], NemoNativeComponentContext: context, NemoNativeObjectProject: { desktop: () => session },
    showToast: (m) => notices.push(m), showContextMenu: (x, y, items) => menus.push(items), document: { createElement: el }, ICO_EYE: 'EYE', ICO_EYE_CLOSED: 'NOEYE', ICO_LOCK: 'LOCK', ICO_UNLOCK: 'UNLOCK' };
  return { host, sent, routes, notices, selected, menus };
}
const flush = () => new Promise((resolve) => setImmediate(resolve));

test('New layer names the held context, the next free name, and selects the created layer', async () => {
  const h = harness();
  const r = await Edit.create(h.host);
  assert.deepEqual([r.ok, r.applied, r.layerUid], [true, true, 'new_1']);
  assert.deepEqual(h.sent.map((q) => [q.operation, q.payload]), [[P + 'layer.create', { command: 'object.layer.create', contextId: 'sym_a', name: 'Layer 3' }]]);
  assert.deepEqual([...new Set(h.routes)], ['component']);
  assert.deepEqual(h.selected, [2], 'the new layer is the active one');
});

test('Duplicate and Delete act on the component layer and the marked rows', async () => {
  let h = harness();
  await Edit.duplicate(h.host, 'in_a');
  assert.deepEqual(h.sent[0].payload, { command: 'object.layer.duplicate', layerUid: 'in_a', contextId: 'sym_a' });
  assert.deepEqual(h.selected, [2]);
  h = harness({ layerSel: [0, 1] });
  await Edit.remove(h.host);
  assert.deepEqual([h.sent[0].operation, h.sent[0].payload], [P + 'layer.delete', { command: 'object.layer.delete', selection: { active: 'in_a', selected: ['in_a', 'in_b'] }, contextId: 'sym_a' }]);
  h = harness({ layerSel: [0] });
  h.host.NemoNativeComponentContext.layers = () => [{ layerUid: 'in_a', name: 'only' }];
  assert.deepEqual(await Edit.remove(h.host), { ok: false, code: Edit.CODES.last }, 'a component keeps its last layer');
  assert.equal(h.sent.length, 0);
  h = harness();
  assert.equal((await Edit.duplicate(h.host, 'gone')).code, Edit.CODES.invalid, 'a layer the entered view does not list is refused before sending');
  assert.equal(h.sent.length, 0);
});

test('the eye and the padlock send the visibility and the locked flag of the component layer', async () => {
  const h = harness();
  await Edit.flag(h.host, 'in_a', 'visible', false);
  await Edit.flag(h.host, 'in_b', 'locked', false);
  assert.deepEqual(h.sent.map((q) => [q.operation, q.payload]), [
    [P + 'layer.visibility.set', { command: 'object.layer.visibility.set', layerUid: 'in_a', visible: false, contextId: 'sym_a' }],
    [P + 'layer.policy.edit', { command: 'object.layer.policy.edit', layerUid: 'in_b', flag: 'locked', value: false, contextId: 'sym_a' }]]);
});

test('Cut at the playhead splits the component layer at the frame; a playhead outside the layer shows the baseline notice', async () => {
  let h = harness();
  assert.equal(await Edit.cut(h.host, 1), true);
  assert.deepEqual([h.sent[0].operation, h.sent[0].payload], [P + 'layer.cut', { command: 'object.layer.cut', contextId: 'sym_a', layerUid: 'in_b', frame: 3 }]);
  assert.deepEqual(h.selected, [2]);
  h = harness({ noop: true });
  assert.equal(await Edit.cut(h.host), false);
  assert.equal(h.sent[0].payload.layerUid, 'in_a', 'the active layer when no index is given');
  assert.equal(h.notices.length, 1);
});

test('stale component, missing route, refusal are typed and never reach the scene route', async () => {
  let h = harness({ ready: false });
  assert.equal((await Edit.create(h.host)).code, Edit.CODES.stale);
  assert.equal(h.sent.length, 0);
  h = harness({ route: false });
  assert.equal((await Edit.create(h.host)).code, Edit.CODES.session);
  assert.equal(h.sent.length, 0);
  h = harness({ refuse: 'not_found' });
  assert.deepEqual([(await Edit.create(h.host)).code, h.routes.includes('scene')], ['not_found', false]);
  h = harness({ held: null });
  assert.equal((await Edit.create(h.host)).code, Edit.CODES.session, 'not inside a component: nothing is sent');
  assert.equal(Edit.toolbar(h.host, 'delete'), false, 'at the scene root the toolbar is the scene writer');
  assert.equal(h.sent.length, 0);
});

test('the toolbar is owned inside a component, and a refusal says so', async () => {
  const h = harness({ refuse: 'invalid_request' });
  assert.equal(Edit.toolbar(h.host, 'create'), true);
  await flush(); await flush();
  assert.equal(h.sent[0].operation, P + 'layer.create');
  assert.deepEqual(h.notices, ['The layer change was not applied.']);
});

test('a decorated row has an eye, a padlock and a menu bound to the component layer', async () => {
  const h = harness();
  const row = h.host.document.createElement('div');
  Edit.decorate(h.host, row, { layerUid: 'in_b', visible: false, locked: true }, 1);
  assert.deepEqual(row.children.map((c) => [c.className, c.innerHTML]), [['lico off', 'NOEYE'], ['lico', 'LOCK']]);
  row.children[0].listeners.click({ stopPropagation() {} });
  await flush(); await flush();
  assert.deepEqual(h.sent[0].payload, { command: 'object.layer.visibility.set', layerUid: 'in_b', visible: true, contextId: 'sym_a' });
  row.listeners.contextmenu({ preventDefault() {}, clientX: 4, clientY: 5 });
  assert.deepEqual(h.menus[0].filter((m) => !m.sep).length, 3, 'duplicate, delete, cut at the playhead');
  assert.deepEqual(h.selected, [1], 'a right-click outside the selection selects the row');
});
