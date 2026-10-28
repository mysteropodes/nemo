'use strict';
// U33 (#2362): under native authority the Labs knife plans its cut on the presented strokes with Paper geometry and commits
// ONE native vector batch with the copy key: a stroke with no other crossing is deleted, a cut stroke's surviving head
// replaces it under its own id and its other surviving pieces are created with fresh ids. The presented items and the
// legacy writers are never touched; a refusal keeps the baseline toast.
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
    layers: [{ layerUid: 'native-layer:1:0', name: 'Ink' }] };
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, requestId: (label) => label + '-' + (sent.length + 1),
    runResourceCommand: (action) => action({
      async dispatch(request) {
        sent.push(request);
        const base = { apiVersion: 2, requestId: request.requestId, instanceId: pin.instanceId, documentId: pin.documentId };
        if (options.refuse) return { ...base, contentRevision: 4, ok: false, error: { code: options.refuse, message: 'refused' } };
        const ids = request.payload.ops.map((op, i) => (op.op === 'create' || op.op === 'clone' ? 'native-stroke:5:' + i
          : op.op === 'replace' ? op.record.target.strokeId : op.stableTarget.strokeId));
        return { ...base, contentRevision: 5, ok: true, result: { applied: true, historyEntriesAdded: 1, operation: 'batch', strokeId: ids[0], strokeIds: ids } };
      },
      async reconcile() {},
    }) };
  const layer = new paper.Layer();
  const line = (id, from, to) => layer.addChild(new paper.Path({ segments: [from, to], strokeColor: new paper.Color(0, 0, 0), strokeWidth: 2, data: { strokeId: id, brushGroupId: 'g' } }));
  line('a', [0, 0], [100, 0]); line('b', [30, -50], [30, 50]); line('c', [70, -50], [70, 50]); line('d', [0, 100], [100, 100]);
  const segs = (p) => p.segments.map((s) => [s.point.x, s.point.y, s.handleIn.x, s.handleIn.y, s.handleOut.x, s.handleOut.y]);
  const area = { addEventListener(kind, fn) { (listeners[kind] = listeners[kind] || []).push(fn); }, style: {} };
  const state = { activeLayerIdx: 0, currentFrame: 3, playing: false };
  Object.defineProperty(state, 'layers', { get: poisoned });
  const root = { state, console, Math, JSON, Path: paper.Path, Point: paper.Point, view: { zoom: 1 }, SM: { t: (k) => k }, showToast: (t) => toasts.push(t),
    document: { readyState: 'complete', getElementById: (id) => (id === 'canvas-area' ? area : null), addEventListener() {} },
    SMEngineBridge: { isEnabled: () => true, screenToWorld: (x, y) => [x, y], suspend() {}, resume() {}, renderNow() {}, renderWithOverlayItem() {} },
    SMLabs: { register() {} }, NemoNativeObjectProject: { desktop: () => session }, NemoNativeObjectLabs: Labs, NemoNativeObjectVector: Vector,
    userLayers: [layer],
    serP: (p) => ({ strokeId: p.data.strokeId, brushGroupId: p.data.brushGroupId, segments: p.segments.map((s) => ({ point: [s.point.x, s.point.y],
      handleIn: [s.handleIn.x, s.handleIn.y], handleOut: [s.handleOut.x, s.handleOut.y] })), strokeColor: '#000000', strokeWidth: p.strokeWidth, closed: p.closed }),
    pushUndo: poisoned, saveActiveLayerFrame: poisoned, updateUI: poisoned };
  if (options.object) {
    // #2362: a native object project has no Paper items; the presented lines come from the native subselect read.
    const items = layer.children.map((p) => ({ stableTarget: { contextId: 'scene-root', layerUid: 'native-layer:1:0', frameScope: { kind: 'authored', frame: 3 }, strokeId: p.data.strokeId },
      closed: false, count: 2, matrix: [1, 0, 0, 1, 0, 0], segments: p.segments.map((g) => ({ point: [g.point.x, g.point.y], handleIn: [0, 0], handleOut: [0, 0] })) }));
    Object.assign(pin, { canvasW: 1920, canvasH: 1080 });
    Object.assign(session, { readPresentedObjectSession: () => ({ presentation: { ...pin, frame: 3 } }), readSubselect: async () => ({ items }) });
    Object.assign(root, { userLayers: [new paper.Layer()], Matrix: paper.Matrix, NemoSelectCanvasIntent: require('../src/js/adapters/select-canvas-intent.js'),
      SMProject: { nativeObjectProject: () => ({ blocksLegacy: () => true }) },
      NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX, y: e.clientY }) } } });
    root.document.getElementById = (id) => (id === 'canvas-area' ? area : id === 'drawing-canvas' ? { getBoundingClientRect: () => ({ left: 0, top: 0, width: 1920, height: 1080 }) } : null);
    layer.remove();
  }
  root.window = root;
  vm.createContext(root);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '..', 'src/js/labs/vector-trim.js'), 'utf8'), root, { filename: 'vector-trim.js' });
  const fire = (kind, x, y) => { for (const fn of listeners[kind] || []) fn({ clientX: x, clientY: y, stopImmediatePropagation() {}, preventDefault() {} }); };
  const cut = async (from, to) => {
    fire('pointerdown', ...from); fire('pointermove', (from[0] + to[0]) / 2, (from[1] + to[1]) / 2); fire('pointerup', ...to);
    for (let i = 0; i < 4; i++) await new Promise(setImmediate);
  };
  return { root, sent, toasts, layer, segs, cut, labs: root.SMLabs };
}

test('one knife drag commits one copy-keyed batch: the head replaces, the far piece is created, a lone stroke is deleted', async () => {
  const h = harness();
  const before = h.layer.children.map(h.segs);
  h.labs.setKnifeTrimActive(true);
  await h.cut([50, -20], [50, 120]);
  assert.equal(h.sent.length, 1);
  const { operation, payload } = h.sent[0];
  assert.deepEqual([operation, payload.command, payload.keyframe], ['command.document.object.vector.edit', 'object.vector.batch', 'copy']);
  assert.deepEqual(payload.ops.map((op) => op.op), ['replace', 'create', 'delete']);
  const [head, tail, lone] = payload.ops;
  assert.deepEqual([head.record.target.strokeId, tail.record.target.strokeId, lone.stableTarget.strokeId], ['a', '', 'd']);
  const xs = (op) => plain(op.record.geometry.segments).map((s) => [Math.round(s.point.x), Math.round(s.point.y)]);
  assert.deepEqual(xs(head), [[0, 0], [30, 0]], 'the head keeps the part before the nearest crossing');
  assert.deepEqual(xs(tail), [[70, 0], [100, 0]], 'the far piece starts at the next crossing');
  assert.deepEqual(payload.ops.filter((op) => op.record).map((op) => [op.record.target.layerUid, op.record.target.frameScope.frame]),
    [['native-layer:1:0', 3], ['native-layer:1:0', 3]]);
  assert.deepEqual(plain(lone.stableTarget.frameScope), { kind: 'authored', frame: 3 });
  assert.deepEqual(h.layer.children.map(h.segs), before, 'the presented strokes are never mutated');
  assert.deepEqual(h.toasts.slice(-1), ['2labsToastStrokesCutSuffix']);
});

test('a refused cut keeps the baseline toast and a missed drag sends nothing', async () => {
  const h = harness({ refuse: 'layer_locked' });
  h.labs.setKnifeTrimActive(true);
  await h.cut([50, 80], [50, 120]);
  assert.equal(h.sent.length, 1);
  assert.deepEqual(plain(h.sent[0].payload.ops.map((op) => op.op)), ['delete']);
  assert.deepEqual(h.toasts.slice(-1), ['labsToastLayerInvalidLocked']);
  await h.cut([200, 200], [220, 220]);
  assert.equal(h.sent.length, 1);
  assert.deepEqual(h.toasts.slice(-1), ['labsToastNoStrokeUnderKnife']);
});

test('on a native object project the knife plans on the presented geometry and keeps native style with reshape/clone (#2362)', async () => {
  const h = harness({ object: true });
  h.labs.setKnifeTrimActive(true);
  await h.cut([50, -20], [50, 120]);
  assert.equal(h.sent.length, 1, h.toasts.join());
  const { payload } = h.sent[0];
  assert.deepEqual([payload.command, payload.keyframe, payload.ops.map((op) => op.op)], ['object.vector.batch', 'copy', ['reshape', 'clone', 'delete']]);
  const [head, tail, lone] = payload.ops;
  assert.deepEqual([head.stableTarget.strokeId, tail.stableTarget.strokeId, lone.stableTarget.strokeId, head.frame, tail.frame], ['a', 'a', 'd', 3, 3]);
  const xs = (op) => plain(op.geometry.segments).map((g) => [Math.round(g.point.x), Math.round(g.point.y)]);
  assert.deepEqual([xs(head), xs(tail)], [[[0, 0], [30, 0]], [[70, 0], [100, 0]]], 'the pieces carry geometry only; style stays native');
  assert.deepEqual(h.toasts.slice(-1), ['2labsToastStrokesCutSuffix']);
  await h.cut([50, 200], [50, 300]);
  assert.equal(h.sent.length, 1);
  assert.deepEqual(h.toasts.slice(-1), ['labsToastNoStrokeUnderKnife']);
});
