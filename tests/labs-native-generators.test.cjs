'use strict';
// U36 (#2362): under native authority the Labs speed lines, boil, follow-path and reference fill send one public native operation each,
// built from the baseline option defaults and clamps, through the real labs adapter, rig command and contract; the legacy
// document and its writers are never touched. Refusals keep the baseline toasts; without a native session the baseline
// path still runs.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const ROOT = path.join(__dirname, '..');
const read = (file) => fs.readFileSync(path.join(ROOT, file), 'utf8');
const Labs = require('../src/js/adapters/native-object-labs.js');
const poisoned = () => { throw Error('legacy writer reached'); };
const plain = (value) => JSON.parse(JSON.stringify(value)); // arrays built inside the vm realm

function harness(options = {}) {
  const sent = [], toasts = [], cleared = [];
  const pin = { instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 1, contentRevision: 4,
    layers: [{ layerUid: 'native-layer:1:0' }, { layerUid: 'native-layer:2:0' }] };
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, requestId: (label) => label + '-' + (sent.length + 1),
    runResourceCommand: (action) => action({
      async dispatch(request) {
        sent.push(request);
        if (options.refuse) return { apiVersion: 2, requestId: request.requestId, instanceId: pin.instanceId, documentId: pin.documentId,
          contentRevision: pin.contentRevision, ok: false, error: { code: options.refuse, message: 'refused' } };
        if (options.miss) return { apiVersion: 2, requestId: request.requestId, instanceId: pin.instanceId, documentId: pin.documentId,
          contentRevision: pin.contentRevision, ok: true, result: { applied: false, historyEntriesAdded: 0 } };
        return { apiVersion: 2, requestId: request.requestId, instanceId: pin.instanceId, documentId: pin.documentId,
          contentRevision: pin.contentRevision + 1, ok: true, result: { applied: true, historyEntriesAdded: 1 } };
      },
      async reconcile() {},
    }) };
  const state = { activeLayerIdx: 1, currentFrame: 6, canvasW: 1920, canvasH: 1080, strokeEnabled: true, strokeColor: '#ff000080',
    brushSize: 3, opacity: 80, fillColor: '#336699' };
  if (!options.legacy) Object.defineProperty(state, 'layers', { get: poisoned });
  const root = { state, console, Math, JSON, document: { addEventListener() {} }, SM: { t: (k) => k }, showToast: (t) => toasts.push(t), clearSel: () => cleared.push(1),
    selectedPaths: options.selected || [], SMLabs: { register() {} },
    NemoNativeObjectProject: { desktop: () => (options.legacy ? null : session) }, NemoNativeObjectLabs: Labs,
    pushUndo: poisoned, pushUndoLayers: poisoned, saveActiveLayerFrame: poisoned, saveAllLayerFrames: poisoned, loadFrame: poisoned,
    ensureKeyframe: poisoned, getEffectiveStrokes: poisoned, Path: poisoned, renderTimeline: poisoned, userLayers: new Proxy({}, { get: poisoned }) };
  root.window = root;
  vm.createContext(root);
  for (const file of ['speed-lines.js', 'boil-effect.js', 'follow-path.js', 'reference-fill.js']) vm.runInContext(read('src/js/labs/' + file), root, { filename: file });
  return { root, sent, toasts, cleared, labs: root.SMLabs };
}

test('speed lines send the baseline defaults, the seed and the resolved colour as one native operation', async () => {
  const h = harness();
  const result = h.labs.speedLines({ seed: 9 });
  assert.equal(typeof result.then, 'function', 'the native path answers a Promise of the count');
  assert.equal(await result, 60);
  assert.equal(h.sent.length, 1);
  const r1 = 1920 * 0.75;
  assert.equal(h.sent[0].operation, 'command.document.object.labs.speedLines');
  assert.equal(h.sent[0].expectedRevision, 4);
  assert.deepEqual(h.sent[0].payload, { command: 'object.labs.speedLines', layerUid: 'native-layer:2:0', frame: 6, cx: 960, cy: 540, count: 60,
    r0: r1 * 0.45, r1, jitter: 0.35, seed: 9, color: [1, 0, 0, 128 / 255], brushSize: 3, opacity: 0.8 });
  assert.deepEqual(h.toasts, ['60labsToastSpeedLinesGeneratedSuffix']);
  assert.equal(await h.labs.speedLines({ count: 1000, jitter: 7, cx: 5, cy: 6, r0: 1, r1: 2 }), 400);
  assert.deepEqual([h.sent[1].payload.count, h.sent[1].payload.jitter, h.sent[1].payload.cx, h.sent[1].payload.r0], [400, 1, 5, 1]);
  assert.ok(Number.isInteger(h.sent[1].payload.seed) && h.sent[1].payload.seed >= 0, 'a drawn seed names an unseeded run');
});

test('boil clamps like the baseline and a cell with nothing shown keeps the baseline toast', async () => {
  const h = harness();
  assert.equal(await h.labs.boil({ frames: 30, amplitude: 0.1 }), 24);
  assert.deepEqual(h.sent[0].payload, { command: 'object.labs.boil', layerUid: 'native-layer:2:0', frame: 6, frames: 24, amplitude: 0.2, seedStep: 1 });
  assert.deepEqual(h.toasts, ['labsToastBoilPrefix24labsToastBoilSuffix7']);
  const empty = harness({ refuse: 'not_found' });
  assert.equal(await empty.labs.boil({}), 0);
  assert.deepEqual(empty.toasts, ['labsToastNothingToBoil']);
  assert.deepEqual(empty.sent[0].payload, { command: 'object.labs.boil', layerUid: 'native-layer:2:0', frame: 6, frames: 3, amplitude: 2.5, seedStep: 1 });
});

test('follow-path sends the selection with its linked fills, or explicit ids, and keeps the baseline refusals', async () => {
  const fill = { data: { strokeId: 'fill-1' } };
  const h = harness({ selected: [{ data: { strokeId: 'a', linkedFill: fill } }, { data: { strokeId: 'p' } }, { data: { strokeId: 'a' } }] });
  assert.equal(await h.labs.followPath({ pathId: 'p', frames: 500, ease: 'bounce', removeTrajectory: false }), 120);
  assert.deepEqual(plain(h.sent[0].payload), { command: 'object.labs.followPath', layerUid: 'native-layer:2:0', frame: 6, pathId: 'p',
    strokeIds: ['a', 'fill-1'], frames: 120, ease: 'inout', removeTrajectory: false });
  assert.deepEqual(h.cleared, [1]);
  assert.equal(await h.labs.followPath({ pathId: 'p', strokeIds: ['z'], ease: 'in' }), 12);
  assert.deepEqual(plain([h.sent[1].payload.strokeIds, h.sent[1].payload.ease, h.sent[1].payload.removeTrajectory]), [['z'], 'in', true]);
  const none = harness();
  assert.equal(await none.labs.followPath({}), 0);
  assert.equal(await none.labs.followPath({ pathId: 'p' }), 0);
  assert.deepEqual([none.toasts, none.sent.length], [['labsToastTrajectoryNotFound', 'labsToastSelectStrokesToAnimate'], 0]);
  const missing = harness({ refuse: 'not_found' });
  assert.equal(await missing.labs.followPath({ pathId: 'p', strokeIds: ['a'] }), 0);
  assert.deepEqual(missing.toasts, ['labsToastTrajectoryNotFound']);
  const locked = harness({ refuse: 'unavailable' });
  assert.equal(await locked.labs.speedLines({}), 0);
  assert.deepEqual(locked.toasts, ['labsToastLayerInvalidLocked']);
});

test('reference fill sends the bucket colour as #rrggbbaa and keeps the baseline outcome toasts', async () => {
  const h = harness();
  assert.equal(await h.labs.referenceFill(12.5, -4), true);
  assert.deepEqual(plain(h.sent[0].payload), { command: 'object.labs.referenceFill', layerUid: 'native-layer:2:0', frame: 6,
    point: [12.5, -4], color: '#336699ff', opacity: 0.8 });
  assert.equal(h.sent[0].operation, 'command.document.object.labs.referenceFill');
  assert.deepEqual(h.toasts, ['labsToastFillPlacedAllLayers']);
  const miss = harness({ miss: true });
  assert.equal(await miss.labs.referenceFill(0, 0), false);
  assert.deepEqual(miss.toasts, ['labsToastNoClosedAreaAllLayers']);
  const locked = harness({ refuse: 'unavailable' });
  assert.equal(await locked.labs.referenceFill(0, 0), false);
  assert.deepEqual(locked.toasts, ['labsToastActiveLayerInvalidLocked']);
});

test('without a native session the baseline path runs unchanged', () => {
  const h = harness({ legacy: true });
  const lines = [];
  h.root.state.layers = [{ locked: false }];
  h.root.state.activeLayerIdx = 0;
  Object.assign(h.root, { pushUndo() {}, ensureKeyframe() {}, saveActiveLayerFrame() {}, updateUI() {}, userLayers: [{ activate() {} }],
    Point: function (x, y) { this.x = x; this.y = y; } });
  h.root.Path = function () { this.add = () => {}; lines.push(this); };
  assert.equal(h.labs.speedLines({ count: 5 }), 5);
  assert.equal(lines.length, 5);
  assert.equal(h.sent.length, 0);
  assert.equal(Labs.desktop({ NemoNativeObjectProject: { desktop: () => ({ blocksLegacy: () => false }) } }), null);
});
