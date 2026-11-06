'use strict';
// #2256 A-comp-edit-timeline (JS): the dense editor's frame and key actions inside an entered component reach the declared native
// operations through the component command route only, stamped with the held context, against the component's own rows and length.
const test = require('node:test'), assert = require('node:assert/strict');
const CompEdit = require('../src/js/adapters/native-component-frame-edit.js');
const Desktop = require('../src/js/adapters/native-frame-edit-desktop.js');
const Intent = require('../src/js/adapters/native-frame-edit-intent.js');
const P = 'command.document.object.';
const ROWS = [{ layerUid: 'in_a', name: 'A', locked: false }, { layerUid: 'in_b', name: 'B', locked: false }, { layerUid: 'in_c', name: 'C', locked: false }];

function harness(options = {}) {
  const sent = [], routes = [], navigation = [], notices = [];
  let total = options.total || 4, revision = 5, ready = options.ready !== false;
  const pin = () => ({ instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: revision, documentSnapshotId: 's' + revision });
  const state = { currentFrame: 1, activeLayerIdx: 1, totalFrames: total, layers: ROWS };
  const context = {
    presented: () => (options.held === null ? null : Object.freeze({ contextId: 'sym_a', totalFrames: total, fps: 24 })),
    contextId: () => 'sym_a', layers: () => ROWS, projectionReady: () => ready,
  };
  const run = (route) => (action) => { routes.push(route); return Promise.resolve(action({
    dispatch: async (request) => {
      sent.push(request);
      if (options.refuse) return { apiVersion: 2, requestId: request.requestId, instanceId: 'i', documentId: 'd', contentRevision: revision, ok: false, error: { code: options.refuse } };
      const applied = options.noop !== true;
      if (applied) revision += 1;
      return { apiVersion: 2, requestId: request.requestId, instanceId: 'i', documentId: 'd', contentRevision: revision, ok: true, result: { applied, historyEntriesAdded: applied ? 1 : 0 } };
    },
    reconcile: async () => { total = request_total(sent.at(-1), total); state.totalFrames = total; return pin(); },
  })); };
  let serial = 0;
  const session = { routes, info: pin, isActive: () => true, requestId: (label) => label + '-' + (++serial), runResourceCommand: run('scene') };
  if (options.route !== false) session.runComponentCommand = run('component');
  const host = { state, _layerSel: options.layerSel || [], _sel: { frames: options.cells || [] }, NemoNativeComponentContext: context,
    NemoNativeObjectProject: { desktop: () => session }, goToFrame: async (f) => { navigation.push(f); state.currentFrame = f; },
    showToast: (m) => notices.push(m) };
  return { host, sent, routes, navigation, notices, state, bump: () => { revision += 1; } };
}
function request_total(request, total) {
  if (!request) return total;
  const p = request.payload;
  return request.operation === P + 'frames.insert' || request.operation === P + 'frames.duplicate' ? total + 1
    : request.operation === P + 'frames.extend' ? total + p.count : request.operation === P + 'frames.remove' ? total - (p.to - p.from + 1) : total;
}

test('F5 inside a component names the held context, the component rows and its own frame, on the component route only', async () => {
  const h = harness({ layerSel: [0, 2] });
  const result = await CompEdit.run(h.host, { kind: 'insertFrame' });
  assert.deepEqual(h.sent.map((r) => [r.operation, r.payload]), [[P + 'frames.insert', { command: 'object.frames.insert', cell: 1, layerUids: ['in_a', 'in_c'], contextId: 'sym_a' }]]);
  assert.deepEqual([...new Set(h.routes)], ['component']);
  assert.deepEqual([result.status, result.applied, result.playhead], ['applied-presented', true, 2], 'F5 steps to the new frame as the baseline does');
  assert.deepEqual(h.navigation, [2]);
  assert.equal(h.sent[0].expectedRevision, 5);
});

test('every routed action maps to its declared operation with the held context and the active component layer', async () => {
  const cases = [
    [{ kind: 'insertKey' }, 'layer.key.promote', { command: 'object.layer.key.promote', cells: [1], contextId: 'sym_a' }],
    [{ kind: 'insertKeyAt', layer: 2, frame: 3 }, 'layer.key.promote', { command: 'object.layer.key.promote', layerUids: ['in_c'], cells: [3], contextId: 'sym_a' }],
    [{ kind: 'convertToKeyframes' }, 'layer.key.promote', { command: 'object.layer.key.promote', layerUids: ['in_b'], cells: [1], contextId: 'sym_a' }],
    [{ kind: 'insertBlankKey' }, 'layer.key.blank', { command: 'object.layer.key.blank', layerUid: 'in_b', cell: 1, contextId: 'sym_a' }],
    [{ kind: 'duplicateKey' }, 'frames.duplicate', { command: 'object.frames.duplicate', layerUid: 'in_b', cell: 1, contextId: 'sym_a' }],
    [{ kind: 'extendExposure', count: 2 }, 'frames.extend', { command: 'object.frames.extend', cell: 1, count: 2, contextId: 'sym_a' }],
    [{ kind: 'clearKey' }, 'layer.key.clear', { command: 'object.layer.key.clear', layerUid: 'in_b', cell: 1, contextId: 'sym_a' }],
    [{ kind: 'removeTween', layer: 0, from: 0, to: 2 }, 'layer.tween.remove', { command: 'object.layer.tween.remove', layerUid: 'in_a', from: 0, to: 2, contextId: 'sym_a' }],
    [{ kind: 'removeFrame' }, 'frames.remove', { command: 'object.frames.remove', from: 1, to: 1, contextId: 'sym_a' }],
  ];
  for (const [action, operation, payload] of cases) {
    const h = harness();
    const result = await CompEdit.run(h.host, action);
    assert.equal(result.status, 'applied-presented', action.kind);
    assert.deepEqual([h.sent[0].operation, h.sent[0].payload], [P + operation, payload], action.kind);
    assert.deepEqual([...new Set(h.routes)], ['component'], action.kind);
  }
});

test('Delete Frames and Remove Frame Span use the selected component cells, the span falling back to the current frame', async () => {
  const cells = [{ layer: 0, frame: 1 }, { layer: 2, frame: 3 }];
  let h = harness({ cells });
  await CompEdit.run(h.host, { kind: 'deleteFrames' });
  assert.deepEqual(h.sent[0].payload, { command: 'object.layer.key.clear_cells', targets: [{ layerUid: 'in_a', frame: 1 }, { layerUid: 'in_c', frame: 3 }], contextId: 'sym_a' });
  h = harness({ cells });
  await CompEdit.run(h.host, { kind: 'removeFrameSpan' });
  assert.deepEqual(h.sent[0].payload, { command: 'object.frames.remove', from: 1, to: 3, contextId: 'sym_a' });
  h = harness();
  assert.equal((await CompEdit.run(h.host, { kind: 'deleteFrames' })).code, Intent.CODES.selection, 'no selection: typed, nothing sent');
  assert.equal(h.sent.length, 0);
  h = harness();
  await CompEdit.run(h.host, { kind: 'removeFrameSpan' });
  assert.deepEqual(h.sent[0].payload, { command: 'object.frames.remove', from: 1, to: 1, contextId: 'sym_a' });
});

test('a frame outside the component is judged against its own length, not the scene', async () => {
  const h = harness({ total: 4 });
  h.state.currentFrame = 9;
  const result = await CompEdit.run(h.host, { kind: 'insertFrame' });
  assert.deepEqual([result.status, result.code], ['unavailable', Intent.CODES.invalid]);
  assert.equal(h.sent.length, 0);
});

test('actions without a component route answer a typed unavailable code and send nothing', async () => {
  const actions = [{ kind: 'moveKey', layer: 0, from: 0, to: 1 }, { kind: 'duplicateSelection' }, { kind: 'repeatSelection', times: 2 }, { kind: 'copyFrames' },
    { kind: 'cutFrames' }, { kind: 'pasteFrames' }, { kind: 'moveFrames', selection: [], layer: 0, frame: 1 }, { kind: 'rangeDrag', gesture: {}, capture: {} }];
  for (const action of actions) {
    const h = harness({ cells: [{ layer: 0, frame: 1 }] });
    const result = await CompEdit.run(h.host, action);
    assert.deepEqual([result.status, result.code], ['unavailable', CompEdit.CODES.unavailable], action.kind);
    assert.equal(h.sent.length, 0, action.kind);
  }
});

test('stale component, missing route, refusal and no-op are typed and never reach the scene route', async () => {
  let h = harness({ ready: false });
  const unready = await CompEdit.run(h.host, { kind: 'insertFrame' });
  assert.deepEqual([unready.status, unready.code], ['unavailable', Intent.CODES.stale]);
  assert.equal(h.sent.length, 0);
  h = harness({ route: false });
  assert.equal((await CompEdit.run(h.host, { kind: 'insertFrame' })).status, 'unavailable');
  assert.equal(h.sent.length, 0);
  h = harness({ refuse: 'not_found' });
  assert.deepEqual([(await CompEdit.run(h.host, { kind: 'removeFrame' })).status, h.routes.includes('scene')], ['refused', false]);
  h = harness({ noop: true });
  const noop = await CompEdit.run(h.host, { kind: 'insertKey' });
  assert.deepEqual([noop.status, noop.applied], ['unchanged', false]);
  assert.deepEqual(h.navigation, []);
});

test('at the scene root the component route declines (null) so the scene route decides', async () => {
  const h = harness({ held: null });
  assert.equal(await CompEdit.run(h.host, { kind: 'insertFrame' }), null);
  assert.equal(h.sent.length, 0);
});

test('the desktop writer routes a timeline action into the entered component and never to the scene intent', async () => {
  const h = harness();
  const root = Object.assign(h.host, { NemoNativeFrameEditIntent: Intent, NemoNativeApplicationAdapter: { validateRequest() {} },
    NemoNativeComponentFrameEdit: CompEdit, NemoNativeObjectProjectFile: { projectionReadyForUI: () => true } });
  const controller = Desktop.install(root);
  const routed = controller.routeWriter('timeline', 'extendExposure', [3]);
  assert.equal(routed.handled, true);
  const result = await routed.result;
  assert.equal(result.status, 'applied-presented');
  assert.deepEqual(h.sent.map((r) => r.payload), [{ command: 'object.frames.extend', cell: 1, count: 3, contextId: 'sym_a' }]);
  assert.deepEqual([...new Set(h.routes)], ['component']);
  assert.equal(controller.shortcut('cmdExtendExposure', null), true);
  await new Promise(setImmediate);
  assert.equal(h.sent.at(-1).payload.count, 1);
});
