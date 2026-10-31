'use strict';
// #2372 SEL-MASK: a layer with a mask is listed by the native hit-test read (the masked content and the mask itself, role `mask`),
// Select picks either, the Mask panel's feather on a selected mask is ONE native restyle batch entry, and the kind is declared native.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Style = require('../src/js/adapters/native-selection-style.js');
const Steps = require('../src/js/adapters/native-selection-steps.js');
const Contract = require('../src/js/adapters/native-object-vector-contract.js');
const Subselect = require('../src/js/adapters/native-subselect-contract.js');
const Paper = require('./helpers/native-import-paper.cjs');
const overlayStub = require('./helpers/native-selection-overlay-stub.cjs');

const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const target = (id) => ({ contextId: 'scene-root', layerUid: 'L', frameScope: { kind: 'authored', frame: 0 }, strokeId: id });
const quad = (id, b, extra) => ({ stableTarget: target(id), closed: true, count: 4, role: 'plain', filled: true, bounds: b, matrix: [1, 0, 0, 1, 0, 0],
  segments: [seg(b[0], b[1]), seg(b[2], b[1]), seg(b[2], b[3]), seg(b[0], b[3])], style: { fill: '#112233', width: 2, opacity: 1 }, ...extra });
// The masked content (100..300) and the mask (200..400), mask listed first as it is in record order: they overlap on 200..300.
const CONTENT = quad('content', [100, 100, 300, 300]);
const MASK = quad('mask', [200, 200, 400, 400], { role: 'mask', filled: true, style: { maskMode: 'subtract', maskFeather: 6 } });
const settle = async () => { for (let i = 0; i < 6; i++) await new Promise((done) => setTimeout(done, 0)); };

function harness(items) {
  const paper = Paper.load(), toasts = [], batches = [], timers = [];
  const pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's1', canvasW: 400, canvasH: 400, layers: [{ layerUid: 'L' }] };
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, readPresentedObjectSession: () => ({ presentation: { ...pin, frame: 0 } }),
    async readPresentedStrokes(_selector, _pin, wanted) { return { items: items.map((item) => { const { segments, ...rest } = item; return wanted(rest) ? { ...rest, segments } : rest; }) }; } };
  const root = { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool: 'select', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, layers: [{ layerUid: 'L' }], selectedStrokeIndices: [] },
    selectedPaths: [], updateUI() {}, SMEngineBridge: { renderNow() {} }, showToast: (t) => toasts.push(t),
    setTimeout: (fn) => { timers.push(fn); return timers.length; }, clearTimeout: (id) => { timers[id - 1] = null; },
    document: { getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 400, height: 400 }) }) },
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX, y: e.clientY }) } },
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: () => true },
    NemoNativeObjectLabs: { desktop: () => ({ batchStrokes: async (layerUid, frame, steps) => { batches.push({ layerUid, frame, steps: JSON.parse(JSON.stringify(steps)) }); return { status: 'applied' }; } }) },
    NemoSelectCanvasIntent: Intent, NemoNativeSelectionSteps: Steps, NemoNativeSelectOverlay: overlayStub(Intent) };
  return { root, toasts, batches, timers, fire: async () => { timers.splice(0).filter(Boolean).forEach((fn) => fn()); await settle(); } };
}

test('a click on masked content selects the content, a click on the mask alone selects the mask (with its mode and shared feather)', async () => {
  const h = harness([MASK, CONTENT]);
  const content = await Intent.pick(h.root, 150, 150);
  assert.deepEqual([content.data.strokeId, content.data.nativeRole, content.data.isMask], ['content', 'plain', undefined]);
  const overlap = await Intent.pick(h.root, 250, 250);
  assert.equal(overlap.data.strokeId, 'content', 'the content listed above the mask wins where they overlap');
  const mask = await Intent.pick(h.root, 350, 350);
  assert.deepEqual([mask.data.strokeId, mask.data.nativeRole, mask.data.isMask, mask.data.maskMode, mask.data.maskFeather, mask.data.nativeStableTarget], ['mask', 'mask', true, 'subtract', 6, target('mask')]);
  assert.equal(mask.data.nativeLayerIndex, 0);
  assert.equal(await Intent.pick(h.root, 50, 50), null, 'empty space selects nothing');
});

test('the Mask panel feather on a selected mask is ONE native restyle batch with the maskFeather member alone', async () => {
  const h = harness([MASK, CONTENT]);
  h.root.selectedPaths = [await Intent.pick(h.root, 350, 350)];
  h.root.NemoNativeSelectOverlay.show(h.root);
  assert.equal(Style.handles(h.root), true, 'a selected mask hands the edit to the native route');
  const done = Style.apply(h.root, { maskFeather: 5 }), again = Style.apply(h.root, { maskFeather: 12.5 });
  await h.fire();
  assert.deepEqual(await Promise.all([done, again]).then((r) => r.map((x) => x.status)), ['applied', 'applied']);
  assert.equal(h.batches.length, 1, 'the scrub ticks coalesce into one commit (one history entry)');
  assert.deepEqual(h.batches[0].steps, [{ restyle: { stableTarget: target('mask'), style: { maskFeather: 12.5 } } }]);
  Contract.payload(Contract.VECTOR, { command: 'object.vector.batch', ops: h.batches[0].steps.map((s) => ({ op: 'restyle', stableTarget: s.restyle.stableTarget, style: s.restyle.style, frame: 0 })) });
});

test('a feather edit reaches masks only, and a mask takes no paint or preset step', async () => {
  const h = harness([MASK, CONTENT]);
  const mask = await Intent.pick(h.root, 350, 350), content = await Intent.pick(h.root, 150, 150);
  assert.deepEqual(Steps.restyleSteps([content], { maskFeather: 3 }), [], 'masked content has no feather of its own');
  assert.deepEqual(Steps.restyleSteps([mask, content], { maskFeather: 3 }).map((s) => s.restyle.stableTarget.strokeId), ['mask']);
  assert.deepEqual(Steps.restyleSteps([mask], { fill: '#ff0000', width: 4, opacity: 0.5 }), []);
  assert.deepEqual(Steps.restyleSteps([mask], { brushPreset: 'chalk-round' }), []);
  assert.equal(Steps.valid({ maskFeather: 501 }), null);
  assert.equal(Steps.valid({ maskFeather: -1 }), null);
  assert.equal(Steps.alone({ maskFeather: 4 }), 'maskFeather', 'a feather is an edit of its own');
});

test('the hit-test listing admits the mask role with its mode and feather and refuses a malformed one', () => {
  const result = (style, role = 'mask') => ({ kind: 'subselect', contextId: 'scene-root', layerUid: 'L', frame: 0, workId: 'native-work-1', viewGeneration: 1, cursor: 0, hitTest: true, preview: false, atRevision: 7,
    documentSnapshotId: 'native-object:document:7', total: 1, nextCursor: null,
    items: [{ stableTarget: target('m'), closed: true, count: 4, matrix: [1, 0, 0, 1, 0, 0], bounds: [0, 0, 1, 1], role, filled: true, style }] });
  Subselect.viewResult(result({ maskMode: 'intersect', maskFeather: 0 }));
  for (const bad of [{ maskMode: 'xor' }, { maskFeather: -1 }, { maskFeather: '3' }]) assert.throws(() => Subselect.viewResult(result(bad)), TypeError, JSON.stringify(bad));
  assert.throws(() => Subselect.viewResult(result({ maskMode: 'add' }, 'clip')), TypeError);
});

test('the Mask panel reads a native mask and routes its feather natively before any legacy writer', () => {
  const timeline = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'timeline.js'), 'utf8');
  const feather = timeline.slice(timeline.indexOf("document.getElementById('p-mask-feather').addEventListener"), timeline.indexOf("document.getElementById('btn-mask-unset')"));
  assert.ok(feather.includes("nativeSelStyle({maskFeather:"), 'the native batch is the first route');
  assert.ok(feather.indexOf('nativeSelStyle(') < feather.indexOf("n20AllowLegacyWrite('timeline-mask-feather')"), 'and the legacy gate only follows');
  assert.ok(timeline.includes("p.data.nativeStableTarget&&typeof p.data.maskFeather==='number'"), 'the panel shows the listed feather');
  const doc = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'engineering', 'application', 'legacy-writer-families.json'), 'utf8'));
  const native = doc.families.find((f) => f.family === 'selection-style-native');
  assert.ok(native.authority === 'native' && native.kinds.includes('timeline-mask-feather'));
});

// MASK-NATIVE-REST: the Mask panel's mode select and "Remove as mask" button, run through the REAL listener bodies of timeline.js.
const vm = require('node:vm');
function listener(id, type) {
  const src = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'timeline.js'), 'utf8');
  const head = `document.getElementById('${id}').addEventListener('${type}',function(){`, at = src.indexOf(head);
  assert.ok(at >= 0, id);
  return src.slice(at + head.length, src.indexOf('\n});', at));
}
function panel(id, type, scope) {
  const calls = [], body = listener(id, type);
  const ctx = vm.createContext(Object.assign({ calls, legacy: [], pushUndo() { calls.push('legacy:pushUndo'); }, saveActiveLayerFrame() { calls.push('legacy:save'); }, updateUI() {}, SMEngineBridge: { renderNow() {} },
    window: { SMEngineBridge: null }, n20AllowLegacyWrite: (kind) => { calls.push('gate:' + kind); return false; } }, scope));
  return { calls, run: (self) => vm.runInContext(`(function(){${body}})`, ctx).call(self) };
}

test('the Mask panel mode select on a native mask is ONE native restyle batch with the maskMode member alone, never a legacy write', async () => {
  const h = harness([MASK, CONTENT]);
  h.root.selectedPaths = [await Intent.pick(h.root, 350, 350)];
  h.root.NemoNativeSelectOverlay.show(h.root);
  const sent = [];
  const p = panel('p-mask-mode', 'change', { selectedPaths: h.root.selectedPaths, nativeSelStyle: (style) => { sent.push(style); return Style.apply(h.root, style), true; } });
  p.run({ value: 'intersect' });
  await h.fire();
  assert.deepEqual(JSON.parse(JSON.stringify(sent)), [{ maskMode: 'intersect' }]);
  assert.equal(h.batches.length, 1, 'one batch is one history entry');
  assert.deepEqual(h.batches[0].steps, [{ restyle: { stableTarget: target('mask'), style: { maskMode: 'intersect' } } }]);
  assert.deepEqual(JSON.parse(JSON.stringify(p.calls)), [], 'no legacy checkpoint or gate is reached');
  Contract.payload(Contract.VECTOR, { command: 'object.vector.batch', ops: h.batches[0].steps.map((s) => ({ op: 'restyle', stableTarget: s.restyle.stableTarget, style: s.restyle.style, frame: 0 })) });
});

test('Remove as mask on a native mask is ONE native restyle batch with the unmask member alone', async () => {
  const h = harness([MASK, CONTENT]);
  h.root.selectedPaths = [await Intent.pick(h.root, 350, 350)];
  h.root.NemoNativeSelectOverlay.show(h.root);
  const p = panel('btn-mask-unset', 'click', { selectedPaths: h.root.selectedPaths, nativeSelStyle: (style) => { Style.apply(h.root, style); return true; } });
  p.run({});
  await h.fire();
  assert.equal(h.batches.length, 1);
  assert.deepEqual(h.batches[0].steps, [{ restyle: { stableTarget: target('mask'), style: { unmask: true } } }]);
  assert.deepEqual(JSON.parse(JSON.stringify(p.calls)), []);
  Contract.payload(Contract.VECTOR, { command: 'object.vector.batch', ops: h.batches[0].steps.map((s) => ({ op: 'restyle', stableTarget: s.restyle.stableTarget, style: s.restyle.style, frame: 0 })) });
});

test('without a native route the mode and unmask handlers are gated, not written (the legacy gate follows the native route)', () => {
  for (const [id, type, kind] of [['p-mask-mode', 'change', 'timeline-mask-mode'], ['btn-mask-unset', 'click', 'timeline-mask-unset']]) {
    const mask = { data: { isMask: true, maskMode: 'add', nativeStableTarget: target('m') } };
    const p = panel(id, type, { selectedPaths: [mask], nativeSelStyle: () => false });
    p.run({ value: 'subtract' });
    assert.deepEqual(JSON.parse(JSON.stringify(p.calls)), ['gate:' + kind], id + ': refused by the legacy gate, nothing written');
    assert.equal(mask.data.maskMode, 'add');
    assert.equal(mask.data.isMask, true);
  }
  const doc = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'engineering', 'application', 'legacy-writer-families.json'), 'utf8'));
  const native = doc.families.find((f) => f.family === 'selection-style-native');
  assert.ok(['timeline-mask-mode', 'timeline-mask-unset'].every((k) => native.kinds.includes(k)), 'both kinds are declared native');
});

test('mode and unmask reach masks only, are edits of their own, and are validated', async () => {
  const h = harness([MASK, CONTENT]);
  const mask = await Intent.pick(h.root, 350, 350), content = await Intent.pick(h.root, 150, 150);
  assert.deepEqual(Steps.restyleSteps([mask, content], { maskMode: 'add' }).map((s) => [s.restyle.stableTarget.strokeId, s.restyle.style]), [['mask', { maskMode: 'add' }]]);
  assert.deepEqual(Steps.restyleSteps([mask, content], { unmask: true }).map((s) => [s.restyle.stableTarget.strokeId, s.restyle.style]), [['mask', { unmask: true }]]);
  assert.deepEqual(Steps.restyleSteps([content], { unmask: true }), []);
  assert.equal(Steps.alone({ maskMode: 'add' }), 'maskMode');
  assert.equal(Steps.alone({ unmask: true }), 'unmask');
  for (const bad of [{ maskMode: 'xor' }, { maskMode: 3 }, { unmask: false }, { unmask: 'true' }]) assert.equal(Steps.valid(bad), null, JSON.stringify(bad));
  for (const good of [{ maskMode: 'subtract' }, { unmask: true }]) assert.deepEqual(Steps.valid(good), good);
  const wrap = (style) => () => Contract.payload(Contract.VECTOR, { command: 'object.vector.batch', ops: [{ op: 'restyle', stableTarget: target('m'), style, frame: 0 }] });
  for (const ok of [{ maskMode: 'intersect' }, { unmask: true }]) wrap(ok)();
  for (const bad of [{ maskMode: 'xor' }, { unmask: false }, { maskMode: 'add', unmask: true }, { maskMode: 'add', maskFeather: 1 }, { unmask: true, fill: '#ff0000' }]) assert.throws(wrap(bad), TypeError, JSON.stringify(bad));
});
