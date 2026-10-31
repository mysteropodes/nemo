'use strict';
// #2256: Select (pick, move / scale / rotate, restyle) and the Delete key INSIDE an entered native component. Their reads and hit-tests ask the component
// context the workspace controller holds (never the scene root), their writes are ONE object.vector.batch through the component command route
// (one history entry, the scene untouched), and what is not made context-aware still refuses. Real Select modules, real Labs / Vector / Command /
// contract / project reads; the native owner is a scripted double that records the route and the request.
const test = require('node:test'), assert = require('node:assert/strict');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Overlay = require('../src/js/adapters/native-select-overlay.js');
const Gesture = require('../src/js/adapters/native-select-gesture.js');
const Steps = require('../src/js/adapters/native-selection-steps.js');
const Style = require('../src/js/adapters/native-selection-style.js');
const Delete = require('../src/js/adapters/native-selection-delete.js');
const Labs = require('../src/js/adapters/native-object-labs.js');
const Vector = require('../src/js/adapters/native-object-vector.js');
const Reads = require('../src/js/adapters/native-object-project-reads.js');
const Paper = require('./helpers/native-import-paper.cjs');

const CTX = 'comp-a';
const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const quad = (context, layer, id, x0, y0, x1, y1, extra) => Object.assign({ stableTarget: { contextId: context, layerUid: layer, frameScope: { kind: 'authored', frame: 0 }, strokeId: id },
  closed: true, count: 4, role: 'plain', filled: true, bounds: [x0, y0, x1, y1], matrix: [1, 0, 0, 1, 0, 0], segments: [seg(x0, y0), seg(x1, y0), seg(x1, y1), seg(x0, y1)],
  style: { fill: '#112233', stroke: '#445566', width: 4, opacity: 1 } }, extra);
const tick = () => new Promise((done) => setTimeout(done, 0));
const settle = async () => { for (let i = 0; i < 8; i++) await tick(); };
function el(tag) {
  return { tag, attrs: {}, style: {}, children: [], setAttribute(k, v) { this.attrs[k] = String(v); }, appendChild(c) { this.children.push(c); c.parentNode = this; return c; },
    remove() { if (this.parentNode) this.parentNode.children = this.parentNode.children.filter((c) => c !== this); } };
}
// The controller's view of an entered component (what NemoNativeComponentContext answers while it is entered).
function controller(state, rows) {
  return { contextId: () => state.held, layers: () => (state.held === 'scene-root' ? null : rows),
    readScope: () => (state.held === 'scene-root' ? null : { contextId: state.held, totalFrames: 8, layerUids: rows.map((r) => r.layerUid) }),
    selectLayer(host, index) { if (state.held === 'scene-root' || !rows[index]) return false; host.state.activeLayerIdx = index; host._layerSel = [index]; host.selectedPaths = []; return true; } };
}

function world(options) {
  options = options || {};
  const paper = Paper.load(), body = el('body'), toasts = [], runs = [], sent = [], reads = [], held = { held: options.scene ? 'scene-root' : CTX };
  const SCENE_LAYERS = [{ layerUid: 'L1' }];
  const ROWS = [{ layerUid: 'inner', name: 'Inner', locked: false, visible: true }, { layerUid: 'inner2', name: 'Inner 2', locked: false, visible: true }];
  let pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 3, documentSnapshotId: 'native-object:d:3', canvasW: 200, canvasH: 200, layers: SCENE_LAYERS };
  const ctx = () => held.held;
  const store = (options.items || [quad(CTX, 'inner', 'a', 10, 10, 30, 30), quad(CTX, 'inner2', 'b', 50, 10, 70, 30)]).map((i) => JSON.parse(JSON.stringify(i)));
  const presentation = () => ({ ...pin, owner: 'native', status: 'presented', contextId: ctx(), frame: 0, workId: 'native-work-1', viewGeneration: 1 });
  const run = (route) => (action) => { runs.push(route); return Promise.resolve(action({ dispatch: async (request) => {
    sent.push({ route, request: JSON.parse(JSON.stringify(request)) });
    const p = request.payload, ids = (p.ops || []).map((op) => (op.stableTarget || op.record.target).strokeId);
    (p.ops || []).forEach((op) => {
      const at = store.findIndex((i) => op.stableTarget && i.stableTarget.strokeId === op.stableTarget.strokeId && i.stableTarget.layerUid === op.stableTarget.layerUid);
      if (op.op === 'delete' && at >= 0) store.splice(at, 1);
      if (op.op === 'reshape' && at >= 0) { const m = op.matrix, q = store[at]; q.segments = q.segments.map((s) => ({ point: [m[0] * s.point[0] + m[2] * s.point[1] + m[4], m[1] * s.point[0] + m[3] * s.point[1] + m[5]], handleIn: s.handleIn, handleOut: s.handleOut })); }
    });
    pin = { ...pin, contentRevision: pin.contentRevision + 1, documentSnapshotId: 'native-object:d:' + (pin.contentRevision + 1) };
    return { apiVersion: 2, requestId: request.requestId, instanceId: 'i', documentId: 'd', contentRevision: pin.contentRevision, ok: true,
      result: { applied: true, historyEntriesAdded: 1, operation: 'batch', strokeId: ids[0], strokeIds: ids } };
  }, reconcile: async () => pin })); };
  let serial = 0;
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, requestId: (label) => label + '-' + (++serial), readPresentedObjectSession: () => ({ presentation: presentation() }),
    runResourceCommand: run('scene'), runComponentCommand: run('component'),
    async readPresentedStrokes(selector, _pin, wanted) {
      reads.push(selector);
      if (selector.contextId !== ctx()) { const e = new Error('presentation differs'); e.code = 'unavailable'; throw e; }
      return { items: store.filter((i) => i.stableTarget.layerUid === selector.layerUid).map((item) => { const { segments, ...rest } = item; return wanted(rest) ? { ...rest, segments } : rest; }) };
    } };
  const root = { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool: 'select', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, layers: ROWS, selectedStrokeIndices: [], activeSymbolId: options.scene ? null : CTX },
    selectedPaths: [], _layerSel: [0], updateUI() {}, SMEngineBridge: { renderNow() {} }, showToast: (t) => toasts.push(t), setTimeout: (fn) => { fn(); return 1; }, clearTimeout() {},
    document: { body, getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 200, height: 200 }) }), createElementNS: (_n, tag) => el(tag), addEventListener() {}, removeEventListener() {} },
    requestAnimationFrame: () => {},
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI() { throw new Error('the scene layer route must never run inside a component'); } },
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX, y: e.clientY }), transform: () => [1, 0, 0, 1, 0, 0] } },
    NemoNativeObjectLabs: Labs, NemoNativeObjectVector: Vector,
    NemoSelectCanvasIntent: Intent, NemoNativeSelectOverlay: Overlay, NemoNativeSelectionSteps: Steps, NemoNativeSelectionStyle: Style };
  root.NemoNativeComponentContext = controller(held, ROWS);
  globalThis.NemoNativeComponentContext = root.NemoNativeComponentContext;
  root.NemoNativeSelectGesture = Gesture.bind(root);
  return { root, toasts, runs, sent, reads, store, held, pin: () => pin };
}
test.afterEach(() => { delete globalThis.NemoNativeComponentContext; });

test('a click inside the entered component hits the COMPONENT\'s strokes: the read names the held context and the component\'s own layers', async () => {
  const w = world();
  const hit = await Intent.pick(w.root, 20, 20);
  assert.deepEqual([hit.data.strokeId, hit.data.nativeStableTarget.contextId, hit.data.nativeLayerUid, hit.data.nativeLayerIndex], ['a', CTX, 'inner', 0]);
  assert.deepEqual([...new Set(w.reads.map((r) => r.contextId))], [CTX], 'every read asked the component context');
  assert.equal(await Intent.pick(w.root, 150, 150), null, 'an empty point walks every component layer');
  assert.deepEqual([...new Set(w.reads.map((r) => r.layerUid))].sort(), ['inner', 'inner2'], 'the layers walked are the component\'s rows, not the scene\'s');
  const other = await Intent.pick(w.root, 60, 20);
  assert.equal(other.data.nativeLayerUid, 'inner2');
  Intent.applyPick(w.root, other, false);
  assert.equal(w.root.state.activeLayerIdx, 1, 'a pick on another component layer activates it through the context controller (the scene route throws)');
  assert.deepEqual(w.root.selectedPaths.map((p) => p.data.strokeId), ['b']);
  const marquee = await Intent.marquee(w.root, [0, 0, 190, 190]);
  assert.deepEqual(marquee.map((p) => p.data.strokeId), ['a', 'b']);
});

test('at the scene root nothing changes: the scene context and the pinned layers', async () => {
  const w = world({ scene: true, items: [quad('scene-root', 'L1', 's', 10, 10, 30, 30)] });
  const hit = await Intent.pick(w.root, 20, 20);
  assert.deepEqual([hit.data.strokeId, hit.data.nativeStableTarget.contextId], ['s', 'scene-root']);
  assert.deepEqual([...new Set(w.reads.map((r) => r.contextId))], ['scene-root']);
});

test('a move inside the component is ONE batch through the component route, naming the component context; the scene route never runs', async () => {
  const w = world();
  const hit = await Intent.pick(w.root, 20, 20);
  Intent.applyPick(w.root, hit, false);
  const outcome = await Gesture.commitMatrix(w.root, w.root.selectedPaths, [1, 0, 0, 1, 15, 5], { pin: w.pin() });
  assert.equal(outcome, 'applied', w.toasts.join('|'));
  assert.deepEqual(w.runs, ['component'], 'one request, the component route only');
  const body = w.sent[0].request.payload;
  assert.equal(body.command, 'object.vector.batch');
  assert.deepEqual(body.ops.map((o) => [o.op, o.stableTarget.contextId, o.stableTarget.layerUid, o.stableTarget.strokeId]), [['reshape', CTX, 'inner', 'a']]);
  assert.equal(body.keyframe, undefined, 'the scene-only copy key is not sent inside a component');
  assert.deepEqual(w.store[0].segments[0].point, [25, 15], 'the component record moved');
  assert.deepEqual(w.root.selectedPaths.map((p) => p.data.strokeId), ['a'], 'the selection is re-read from the component after the commit');
});

test('a selection over two component layers moves, scales and turns as ONE batch (one history entry)', async () => {
  const w = world();
  const all = await Intent.marquee(w.root, [0, 0, 190, 190]);
  w.root.selectedPaths = all;
  assert.equal(await Gesture.commitMatrix(w.root, all, [1.5, 0, 0, 1.5, 0, 0], { pin: w.pin() }), 'applied');
  assert.equal(await Gesture.commitMatrix(w.root, w.root.selectedPaths, [0, 1, -1, 0, 0, 0], { pin: w.pin() }), 'applied');
  assert.deepEqual(w.runs, ['component', 'component'], 'one request per gesture');
  assert.deepEqual(w.sent[0].request.payload.ops.map((o) => [o.stableTarget.layerUid, o.stableTarget.contextId]), [['inner', CTX], ['inner2', CTX]]);
});

test('Labs batches that are not Select\'s stay unavailable inside the component, and a Select batch is the only one that lands', async () => {
  const w = world();
  const route = Labs.desktop(w.root);
  const refused = await route.batchStrokes('inner', 0, [{ delete: 'a' }]);
  assert.deepEqual([refused.status, refused.reason], ['unavailable', 'component_labs_unavailable']);
  assert.deepEqual(w.sent, [], 'nothing was sent for a Labs batch');
  const hit = await Intent.pick(w.root, 20, 20);
  const applied = await route.batchStrokes('inner', 0, [{ restyle: { stableTarget: hit.data.nativeStableTarget, style: { fill: '#ff0000ff' } } }], { skipUnavailable: true, select: true });
  assert.equal(applied.status, 'applied');
  assert.deepEqual([w.runs, w.sent[0].request.payload.skipUnavailable, w.sent[0].request.payload.ops[0].stableTarget.contextId], [['component'], true, CTX]);
  const scene = world({ scene: true });
  await Labs.desktop(scene.root).batchStrokes('inner', 0, [{ delete: 'a' }]);
  assert.deepEqual([scene.runs, scene.sent[0].request.payload.keyframe], [['scene'], 'copy'], 'the scene root keeps the scene route and the copy key');
});

test('Delete inside the component removes the selection (and a ribbon\'s linked fill) as ONE component batch, then clears the selection', async () => {
  const w = world({ items: [quad(CTX, 'inner', 'ribbon', 10, 10, 30, 30, { role: 'ribbon' }), quad(CTX, 'inner', 'fill', 10, 10, 30, 30, { role: 'fill', owner: 'ribbon' }), quad(CTX, 'inner2', 'b', 50, 10, 70, 30)] });
  const hit = await Intent.pick(w.root, 20, 20);
  assert.equal(hit.data.strokeId, 'ribbon', 'a linked fill is selected as its ribbon');
  Intent.applyPick(w.root, hit, false);
  let prevented = 0, stopped = 0;
  const event = { key: 'Delete', target: { tagName: 'DIV' }, preventDefault() { prevented++; }, stopImmediatePropagation() { stopped++; } };
  assert.equal(await Delete.key(w.root, event), 'applied', w.toasts.join('|'));
  assert.deepEqual([prevented, stopped], [1, 1], 'the key is taken, the legacy timeline Delete never sees it');
  assert.deepEqual(w.runs, ['component']);
  assert.deepEqual(w.sent[0].request.payload.ops.map((o) => [o.op, o.stableTarget.contextId, o.stableTarget.strokeId]), [['delete', CTX, 'ribbon'], ['delete', CTX, 'fill']]);
  assert.deepEqual(w.store.map((i) => i.stableTarget.strokeId), ['b']);
  assert.deepEqual(w.root.selectedPaths, []);
});

test('Delete leaves the key to other selections and refuses what has no stored delete; nothing is written for either', async () => {
  const w = world();
  const hit = await Intent.pick(w.root, 20, 20); Intent.applyPick(w.root, hit, false);
  const take = (extra) => Delete.key(w.root, Object.assign({ key: 'Backspace', target: { tagName: 'DIV' }, preventDefault() {}, stopImmediatePropagation() {} }, extra));
  w.root._layerActiveExplicit = true; assert.equal(take(), null, 'a layer-row selection keeps its own Delete');
  w.root._layerActiveExplicit = false; w.root._sel = { frames: [{}] }; assert.equal(take(), null, 'selected frame cells keep theirs');
  w.root._sel = { frames: [] };
  assert.equal(take({ target: { tagName: 'INPUT' } }), null, 'typing in a field is not a delete');
  w.root.state.appMode = 'motion'; assert.equal(take(), null, 'Motion deletes keys'); w.root.state.appMode = 'anim2d';
  w.root.selectedPaths[0].data.nativeBrush = 'copy';
  assert.equal(await take(), 'refused');
  assert.match(w.toasts.at(-1), /not available/);
  assert.deepEqual(w.sent, []);
});

test('a document that moved on between the key and the batch deletes nothing', async () => {
  const w = world();
  const hit = await Intent.pick(w.root, 20, 20); Intent.applyPick(w.root, hit, false);
  const session = w.root.NemoNativeObjectProject.desktop(), info = session.info, read = session.readPresentedStrokes;
  // The owner moves to the next revision while the selection is being re-read for the batch.
  session.readPresentedStrokes = async function () { const out = await read.apply(this, arguments); session.info = () => ({ ...info(), contentRevision: 99, documentSnapshotId: 'native-object:d:99' }); return out; };
  const outcome = await Delete.key(w.root, { key: 'Delete', target: { tagName: 'DIV' }, preventDefault() {}, stopImmediatePropagation() {} });
  assert.equal(outcome, 'stale');
  assert.match(w.toasts.at(-1), /nothing was deleted/);
  assert.deepEqual(w.sent, []);
});

// The REAL project reads: the timing and presented-stroke reads ask the held component context, and a caller cannot name another.
function reader(held) {
  const pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 0, documentSnapshotId: 'native-object:d:0', layers: [{ layerUid: 'L1' }], totalFrames: 24 };
  const presentation = { ...pin, owner: 'native', status: 'presented', contextId: held, frame: 2, workId: 'work-1', viewGeneration: 1 };
  const state = { active: true, opening: false, pending: 0, synchronizing: false, projectionReady: true, ticket: 1, info: pin, presented: { ...pin, available: true, presentation } };
  const sent = [];
  const reads = Reads.create({ state: () => state, settled: async () => {}, requestId: (k) => `q-${k}-${sent.length}`, dispatch: async (q) => {
    sent.push(q); const p = q.payload;
    let result;
    if (q.operation === 'query.document.object.layer.timing') {
      const base = { instanceId: 'i', documentId: 'd', atRevision: 0, documentSnapshotId: pin.documentSnapshotId, contextId: p.contextId, layerUid: p.layerUid, frame: p.frame };
      result = p.view === 'cells' ? { kind: 'layerCells', ...base, cells: { runs: [[p.frame, 8 - p.frame, 'key', null, true, false]], nextFrame: null } }
        : { kind: 'layerRange', ...base, range: { rawInPoint: 0, rawOutPoint: 7, inPoint: 0, outPoint: 7, hasCustomRange: false, hasTimeLink: false } };
    } else {
      result = { ...p.view, atRevision: 0, documentSnapshotId: pin.documentSnapshotId, total: 1, nextCursor: null, preview: false, hitTest: true,
        items: [{ stableTarget: { contextId: p.view.contextId, layerUid: p.view.layerUid, frameScope: { kind: 'authored', frame: 0 }, strokeId: 'a' }, closed: false, count: 2, matrix: [1, 0, 0, 1, 0, 0], bounds: [0, 0, 1, 1], role: 'plain', filled: false }] };
      delete result.limit;
    }
    return { apiVersion: 2, requestId: q.requestId, instanceId: 'i', documentId: 'd', contentRevision: 0, ok: true, result };
  } });
  return { reads, sent, pin };
}
test('the project reads ask the held component context with its own frame count and layers, and refuse a context the controller does not hold', async () => {
  const w = world();
  const r = reader(CTX);
  const cells = await r.reads.readLayerCells('inner', r.pin);
  assert.deepEqual(cells.map((run) => run.slice(0, 3)), [[0, 8, 'key']], 'the component\'s eight frames');
  assert.equal(r.sent[0].payload.contextId, CTX);
  const range = await r.reads.readLayerRange('inner', 2, r.pin);
  assert.deepEqual([range.contextId, range.range.outPoint], [CTX, 7]);
  assert.equal(await r.reads.readLayerRange('L1', 2, r.pin), null, 'a scene layer is not the component\'s');
  assert.equal(await r.reads.readLayerRange('inner', 8, r.pin), null, 'a frame past the component');
  assert.equal(await r.reads.readLayerPlacement('inner', 2, r.pin), null, 'placement facts stay scene-only');
  const strokes = await r.reads.readPresentedStrokes({ contextId: CTX, layerUid: 'inner', frame: 2 }, r.pin, () => false);
  assert.equal(strokes.items[0].stableTarget.contextId, CTX);
  assert.equal(r.sent.at(-1).payload.view.contextId, CTX);
  await assert.rejects(r.reads.readPresentedStrokes({ contextId: 'comp-b', layerUid: 'inner', frame: 2 }, r.pin, () => false), /Invalid presented-stroke selection/);
  await assert.rejects(r.reads.readPresentedStrokes({ contextId: 'scene-root', layerUid: 'inner', frame: 2 }, r.pin, () => false), /Invalid presented-stroke selection/, 'the scene root is not asked while a component is entered');
  w.held.held = 'scene-root';
  const scene = reader('scene-root');
  await scene.reads.readLayerRange('L1', 2, scene.pin);
  assert.equal(scene.sent[0].payload.contextId, 'scene-root');
  assert.equal(await scene.reads.readLayerRange('inner', 2, scene.pin), null, 'at the scene root a component layer is not a scene layer');
  await scene.reads.readPresentedStrokes({ contextId: 'scene-root', layerUid: 'L1', frame: 2 }, scene.pin, () => false);
  await assert.rejects(scene.reads.readPresentedStrokes({ contextId: CTX, layerUid: 'inner', frame: 2 }, scene.pin, () => false), /Invalid presented-stroke selection/);
});

test('the subselect view contract admits the HELD component for a presented-stroke view, never free text, and the edit intent stays on the scene root', () => {
  const C = require('../src/js/adapters/native-subselect-contract.js');
  const w = world();
  const view = (contextId) => ({ atRevision: 3, documentSnapshotId: 'native-object:d:3', view: { kind: 'subselect', contextId, layerUid: 'inner', frame: 0, workId: 'native-work-1', viewGeneration: 1, cursor: 0, limit: 8, hitTest: true } });
  C.viewPayload(view('scene-root'));
  C.viewPayload(view(CTX));
  for (const bad of ['comp-b', 'a b', '', 'x'.repeat(129)]) assert.throws(() => C.viewPayload(view(bad)), /Invalid subselect presentation selector/, bad);
  w.held.held = 'scene-root';
  assert.throws(() => C.viewPayload(view(CTX)), /Invalid subselect presentation selector/, 'a component that is not entered');
  w.held.held = CTX;
  const edit = (contextId) => ({ command: 'object.subselect.edit', stableTarget: { contextId, layerUid: 'inner', frameScope: { kind: 'authored', frame: 0 }, strokeId: 'a' }, intent: { kind: 'move', indices: [0], delta: [1, 1] } });
  C.payload(C.EDIT, edit('scene-root'));
  assert.throws(() => C.payload(C.EDIT, edit(CTX)), /Invalid native subselect intent/, 'Subselect editing is not made context-aware');
});

test('a batch delete may name a listed record\'s stable target (Select\'s Delete over several layers)', async () => {
  const w = world();
  const route = Vector.create(w.root.NemoNativeObjectProject.desktop(), () => CTX);
  const target = (layerUid) => ({ contextId: CTX, layerUid, frameScope: { kind: 'authored', frame: 0 }, strokeId: 'a' });
  const result = await route.batchStrokes('inner', 0, [{ delete: target('inner') }, { delete: target('inner2') }]);
  assert.equal(result.status, 'applied');
  assert.deepEqual(w.sent[0].request.payload.ops.map((o) => o.stableTarget.layerUid), ['inner', 'inner2']);
});
