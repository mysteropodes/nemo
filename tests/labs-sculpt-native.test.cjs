'use strict';
// U33 (#2362): under native authority the Labs vector sculpt deforms only the presented strokes while dragging (a preview)
// and commits their net geometry once, at the gesture end, as ONE copy-keyed native batch of replaces under their own
// ids; the legacy writers are never reached. A refused batch restores the presented geometry and keeps a baseline toast.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const paper = require('./helpers/native-import-paper.cjs').load();
const Labs = require('../src/js/adapters/native-object-labs.js');
const Vector = require('../src/js/adapters/native-object-vector.js');
const poisoned = () => { throw Error('legacy writer reached'); };
const plain = (value) => JSON.parse(JSON.stringify(value));

function harness(options = {}) {
  const sent = [], toasts = [], listeners = {};
  const pin = { instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 1, contentRevision: 4,
    layers: [{ layerUid: 'native-layer:1:0', name: 'Ink', locked: !!options.locked }] };
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, requestId: (label) => label + '-' + (sent.length + 1),
    runResourceCommand: (action) => action({
      async dispatch(request) {
        sent.push(request);
        const base = { apiVersion: 2, requestId: request.requestId, instanceId: pin.instanceId, documentId: pin.documentId };
        if (options.refuse) return { ...base, contentRevision: 4, ok: false, error: { code: options.refuse, message: 'refused' } };
        const ids = request.payload.ops.map((op) => op.record.target.strokeId);
        return { ...base, contentRevision: 5, ok: true, result: { applied: true, historyEntriesAdded: 1, operation: 'batch', strokeId: ids[0], strokeIds: ids } };
      },
      async reconcile() {},
    }) };
  const layer = new paper.Layer();
  const line = (id, pts) => layer.addChild(new paper.Path({ segments: pts, strokeColor: new paper.Color(0, 0, 0), strokeWidth: 2, data: { strokeId: id } }));
  line('a', [[0, 0], [10, 0], [20, 0], [30, 0], [40, 0]]); line('far', [[0, 500], [40, 500]]);
  const area = { contains: (node) => node === canvas }, canvas = {};
  const el = () => ({ style: {}, setAttribute() {}, classList: { toggle() {} } });
  const state = { activeLayerIdx: 0, currentFrame: 3, playing: false, tool: 'select' };
  Object.defineProperty(state, 'layers', { get: poisoned });
  const root = { state, console, Math, JSON, Path: paper.Path, Point: paper.Point, view: { zoom: 1 }, SM: { t: (k) => k }, showToast: (t) => toasts.push(t),
    localStorage: { getItem: () => '20' }, requestAnimationFrame: () => 0,
    document: { getElementById: (id) => (id === 'canvas-area' ? area : null), createElement: el, body: { appendChild() {} },
      addEventListener(kind, fn) { (listeners[kind] = listeners[kind] || []).push(fn); } },
    addEventListener() {},
    SMEngineBridge: { screenToWorld: (x, y) => [x, y], renderNow() {} },
    SMLabs: { isOn: (n) => n === 'vector-sculpt', register() {} }, NemoNativeObjectProject: { desktop: () => session }, NemoNativeObjectLabs: Labs,
    NemoNativeObjectVector: Vector, userLayers: [layer],
    _ptGet: (vb, seg) => seg.point, _ptSet: (vb, seg, pt) => { seg.point = pt; },
    serP: (p) => ({ strokeId: p.data.strokeId, segments: p.segments.map((s) => ({ point: [s.point.x, s.point.y],
      handleIn: [s.handleIn.x, s.handleIn.y], handleOut: [s.handleOut.x, s.handleOut.y] })), strokeColor: '#000000', strokeWidth: p.strokeWidth, closed: p.closed }),
    pushUndo: poisoned, ensureKeyframe: poisoned, saveActiveLayerFrame: poisoned, updateUI: poisoned, fillRegenerateLinked: poisoned,
    regenerateBrushTexture: poisoned, harmonizeAfterEdit: poisoned };
  root.window = root;
  vm.createContext(root);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '..', 'src/js/labs/vector-sculpt.js'), 'utf8'), root, { filename: 'vector-sculpt.js' });
  const fire = (kind, x, y, extra) => { for (const fn of listeners[kind] || []) fn(Object.assign({ target: canvas, clientX: x, clientY: y, buttons: 1,
    stopImmediatePropagation() {}, preventDefault() {} }, extra)); };
  const pts = (id) => layer.children.find((p) => p.data.strokeId === id).segments.map((s) => [Math.round(s.point.x * 100) / 100, Math.round(s.point.y * 100) / 100]);
  const drag = async (moves) => {
    fire('pointerdown', ...moves[0]);
    for (const m of moves.slice(1)) fire('pointermove', ...m);
    fire('pointerup', ...moves[moves.length - 1], { buttons: 0 });
    for (let i = 0; i < 4; i++) await new Promise(setImmediate);
  };
  return { sent, toasts, pts, drag };
}

test('a push drag previews on the presented stroke and commits one copy-keyed replace under its own id', async () => {
  const h = harness();
  await h.drag([[20, 0], [20, 4], [20, 8]]);
  assert.equal(h.sent.length, 1, 'one command per gesture, none per move');
  const { operation, payload } = h.sent[0];
  assert.deepEqual([operation, payload.command, payload.keyframe], ['command.document.object.vector.edit', 'object.vector.batch', 'copy']);
  assert.deepEqual(payload.ops.map((op) => [op.op, op.record.target.strokeId, op.record.target.frameScope.frame]), [['replace', 'a', 3]]);
  const sentPts = plain(payload.ops[0].record.geometry.segments).map((s) => [Math.round(s.point.x * 100) / 100, Math.round(s.point.y * 100) / 100]);
  assert.deepEqual(sentPts, plain(h.pts('a')), 'the batch carries the previewed geometry');
  assert.ok(sentPts[2][1] > 4 && sentPts[0][1] === 0, 'the point under the brush moved with the drag, the far end stayed');
  assert.deepEqual(plain(h.pts('far')), [[0, 500], [40, 500]]);
});

test('a refused sculpt restores the presented geometry; a locked layer or an empty drag sends nothing', async () => {
  const refused = harness({ refuse: 'layer_locked' });
  const before = refused.pts('a');
  await refused.drag([[20, 0], [20, 6]]);
  assert.equal(refused.sent.length, 1);
  assert.deepEqual(refused.pts('a'), before);
  assert.deepEqual(refused.toasts, ['labsToastLayerInvalidLocked']);
  const locked = harness({ locked: true });
  await locked.drag([[20, 0], [20, 6]]);
  assert.deepEqual(plain([locked.sent.length, locked.pts('a')[2]]), [0, [20, 0]]);
  const empty = harness();
  await empty.drag([[200, 200], [200, 206]]);
  assert.equal(empty.sent.length, 0);
});
