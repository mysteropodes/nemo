'use strict';
// #2372 insp-parametric: the Rounded corners / Ellipse arc / Star panels of a selected dynamic shape on a native object project. The values come
// from the hit-test listing's paramShape, and each edit is ONE object.parametric.set (one history entry) through the REAL panel listeners of
// timeline.js (run in a vm over a fake document): typed values on change, never per keystroke, and a refusal is a toast, never a throw.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Panel = require('../src/js/adapters/native-parametric-panel.js');
const Contract = require('../src/js/adapters/native-object-vector-contract.js');
const Paper = require('./helpers/native-import-paper.cjs');
const overlayStub = require('./helpers/native-selection-overlay-stub.cjs');

const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const target = (id) => ({ contextId: 'scene-root', layerUid: 'L', frameScope: { kind: 'authored', frame: 3 }, strokeId: id });
const shapeItem = (id, paramShape) => ({ stableTarget: target(id), closed: true, count: 4, role: 'plain', filled: true, bounds: [100, 100, 300, 300], matrix: [1, 0, 0, 1, 0, 0],
  segments: [seg(100, 100), seg(300, 100), seg(300, 300), seg(100, 300)], style: { fill: '#112233', width: 2, opacity: 1 }, paramShape });
const settle = async () => { for (let i = 0; i < 8; i++) await new Promise((done) => setTimeout(done, 0)); };

function harness(paramShape, over) {
  const paper = Paper.load(), toasts = [], sets = [], pushes = [];
  let item = shapeItem('s1', paramShape), verdict = { status: 'applied' };
  const pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's1', canvasW: 400, canvasH: 400, layers: [{ layerUid: 'L' }] };
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, readPresentedObjectSession: () => ({ presentation: { ...pin, frame: 3 } }),
    async readPresentedStrokes(_selector, _pin, wanted) { const { segments, ...rest } = item; return { items: [wanted(rest) ? { ...rest, segments } : rest] }; } };
  const root = { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool: 'select', appMode: 'anim2d', currentFrame: 3, activeLayerIdx: 0, layers: [{ layerUid: 'L' }], selectedStrokeIndices: [] },
    selectedPaths: [], updateUI() {}, SMEngineBridge: { renderNow() {} }, showToast: (t) => toasts.push(t),
    document: { getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 400, height: 400 }) }) },
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX, y: e.clientY }) } },
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: () => true },
    NemoNativeObjectVector: { desktop: () => ({ setParametric: async (at, body, options) => {
      sets.push({ at: JSON.parse(JSON.stringify(at)), body: JSON.parse(JSON.stringify(body)), origin: options && options.origin });
      if (verdict.status === 'applied') item = { ...item, paramShape: JSON.parse(JSON.stringify(body)) }; // the engine stores what it rebuilt from
      return verdict; } }) },
    NemoSelectCanvasIntent: Intent, NemoNativeSelectOverlay: overlayStub(Intent), NemoNativeParametricPanel: Panel };
  Object.assign(root, over);
  return { root, toasts, sets, pushes, verdict: (v) => { verdict = v; },
    select: async () => { root.selectedPaths = [await Intent.pick(root, 200, 200)]; root.NemoNativeSelectOverlay.show(root); return root.selectedPaths[0]; } };
}

// The real panel code of timeline.js (updateCornersPanel .. the Star listeners and the native glue), run over a fake document that keeps each element's listeners.
function panels(h, calls) {
  const src = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'timeline.js'), 'utf8');
  const from = src.indexOf('function updateCornersPanel(){'), to = src.indexOf('// Team review Accept/Reject panel');
  assert.ok(from > 0 && to > from);
  const els = {};
  const el = (id) => els[id] || (els[id] = { id, value: '', checked: true, style: {}, dataset: {}, listeners: {}, addEventListener(type, fn) { (this.listeners[type] = this.listeners[type] || []).push(fn); } });
  const fire = (id, type, value) => { const e = el(id); if (value !== undefined) e.value = String(value); (e.listeners[type] || []).forEach((fn) => fn.call(e)); };
  const ctx = vm.createContext({ document: { getElementById: el }, window: h.root, state: h.root.state, applyParamShapeRect() { calls.push('legacy:applyRect'); }, applyParamShapeEllipse() { calls.push('legacy:applyEllipse'); },
    applyParamShapeStar() { calls.push('legacy:applyStar'); }, convertToDynamicEllipse() { calls.push('legacy:convert'); }, saveActiveLayerFrame() { calls.push('legacy:save'); },
    pushUndo() { calls.push('legacy:pushUndo'); throw new Error('a native project refuses the history checkpoint'); }, SMEngineBridge: null, Path: h.root.Path });
  Object.defineProperty(ctx, 'selectedPaths', { get: () => h.root.selectedPaths });
  vm.runInContext(src.slice(from, to), ctx);
  const update = () => vm.runInContext('updateCornersPanel();updateEllipseArcPanel();updateStarPanel();', ctx);
  return { el, fire, update, shown: (id) => els[id].value, visible: (id) => els[id].style.display !== 'none' };
}

const RECT = { kind: 'rect', tl: 6, tr: 6, br: 0, bl: 2.5 };

test('the listed paramShape reaches the picked path and the Corners panel shows its values (the generator box is never listed)', async () => {
  const h = harness(RECT), calls = [], ui = panels(h, calls);
  const p = await h.select();
  assert.deepEqual(JSON.parse(JSON.stringify(p.data.paramShape)), RECT);
  ui.update();
  assert.equal(ui.visible('corners-sec'), true);
  assert.deepEqual(['tl', 'tr', 'br', 'bl'].map((k) => ui.shown('p-corner-' + k)), [6, 6, 0, 2.5]);
  assert.equal(ui.visible('ellipse-arc-sec'), false, 'a dynamic rectangle offers neither the arc fields nor the convert button');
  assert.equal(ui.visible('star-sec'), false);
});

test('typing a corner radius commits once on change (linked: all four corners), never per keystroke, and never through a history checkpoint', async () => {
  const h = harness(RECT), calls = [], ui = panels(h, calls);
  await h.select(); ui.update();
  ui.fire('p-corner-tl', 'input', 1); ui.fire('p-corner-tl', 'input', 12);
  await settle();
  assert.deepEqual(h.sets, [], 'typing alone writes nothing');
  assert.deepEqual(calls, [], 'and reaches neither the legacy checkpoint nor the Paper rebuild');
  ui.fire('p-corner-tl', 'change', 12);
  await settle();
  assert.equal(h.sets.length, 1, 'one change is one native edit (one history entry)');
  assert.deepEqual(h.sets[0].at, { layerUid: 'L', frame: 3, strokeId: 's1' });
  assert.deepEqual(h.sets[0].body, { kind: 'rect', tl: 12, tr: 12, br: 12, bl: 12 });
  assert.equal(h.sets[0].origin.contentRevision, 1, 'pinned to the presented revision');
  Contract.payload(Contract.PARAMETRIC, { command: 'object.parametric.set', stableTarget: target('s1'), paramShape: h.sets[0].body });
  assert.deepEqual(calls, []);
  assert.deepEqual(JSON.parse(JSON.stringify(h.root.selectedPaths[0].data.paramShape)), h.sets[0].body, 'the selection is read again: the panel shows the stored shape');
  ui.update();
  assert.deepEqual(['tl', 'tr', 'br', 'bl'].map((k) => ui.shown('p-corner-' + k)), [12, 12, 12, 12]);
});

test('with the corners unlinked one change moves that corner alone; a negative radius clamps to 0', async () => {
  const h = harness(RECT), calls = [], ui = panels(h, calls);
  await h.select(); ui.update();
  ui.el('p-corners-link').checked = false;
  ui.fire('p-corner-br', 'change', 9);
  await settle();
  assert.deepEqual(h.sets[0].body, { kind: 'rect', tl: 6, tr: 6, br: 9, bl: 2.5 });
  ui.fire('p-corner-bl', 'change', -4);
  await settle();
  assert.deepEqual(h.sets[1].body, { kind: 'rect', tl: 6, tr: 6, br: 9, bl: 0 });
  assert.equal(h.sets.length, 2);
});

test('the Ellipse arc fields show the listed values (inner radius in percent) and each change is one edit with the baseline clamps', async () => {
  const h = harness({ kind: 'ellipse', startAngle: 15, sweep: 300, innerRadius: 0.2 }), calls = [], ui = panels(h, calls);
  await h.select(); ui.update();
  assert.equal(ui.visible('ellipse-arc-sec'), true);
  assert.deepEqual([ui.shown('p-arc-start'), ui.shown('p-arc-sweep'), ui.shown('p-arc-inner')], [15, 300, 20]);
  ui.fire('p-arc-inner', 'input', 40);
  assert.equal(h.sets.length, 0);
  ui.fire('p-arc-inner', 'change', 40);
  await settle();
  assert.deepEqual(h.sets[0].body, { kind: 'ellipse', startAngle: 15, sweep: 300, innerRadius: 0.4 });
  ui.fire('p-arc-inner', 'change', 200);
  await settle();
  assert.equal(h.sets[1].body.innerRadius, 0.95, 'the inner radius stops at 95 %');
  ui.fire('p-arc-sweep', 'change', 90);
  ui.fire('p-arc-start', 'change', 30);
  await settle();
  assert.deepEqual([h.sets[2].body.sweep, h.sets[3].body.startAngle, h.sets[3].body.sweep], [90, 30, 90], 'edits are serialised: the second starts from the first one\'s stored shape');
  Contract.payload(Contract.PARAMETRIC, { command: 'object.parametric.set', stableTarget: target('s1'), paramShape: h.sets[3].body });
  assert.deepEqual(calls, []);
});

test('the Star fields show the listed values and commit pointCount / innerRatio / cornerRadius with the baseline clamps, remembering the last settings', async () => {
  const h = harness({ kind: 'star', pointCount: 7, innerRatio: 0.4, cornerRadius: 1 }), calls = [], ui = panels(h, calls);
  await h.select(); ui.update();
  assert.equal(ui.visible('star-sec'), true);
  assert.deepEqual([ui.shown('p-star-points'), ui.shown('p-star-inner'), ui.shown('p-star-corner')], [7, 40, 1]);
  ui.fire('p-star-points', 'change', 2.4);
  await settle();
  assert.deepEqual(h.sets[0].body, { kind: 'star', pointCount: 3, innerRatio: 0.4, cornerRadius: 1 }, 'at least 3 points, a whole number');
  assert.equal(h.root.state.starPointCount, 3);
  ui.fire('p-star-inner', 'change', 30);
  ui.fire('p-star-corner', 'change', 5);
  await settle();
  assert.deepEqual(h.sets[2].body, { kind: 'star', pointCount: 3, innerRatio: 0.3, cornerRadius: 5 });
  assert.equal(h.root.state.starInnerRatio, 0.3);
  ui.fire('p-star-inner', 'change', 1);
  await settle();
  assert.equal(h.sets[3].body.innerRatio, 0.05, 'the inner ratio stops at 5 %');
  assert.deepEqual(calls, []);
});

test('an absent member is sent as the baseline builder reads it (default), so the body always names every member of its kind', async () => {
  const h = harness({ kind: 'ellipse' }), calls = [], ui = panels(h, calls);
  await h.select(); ui.update();
  assert.deepEqual([ui.shown('p-arc-start'), ui.shown('p-arc-sweep'), ui.shown('p-arc-inner')], [0, 359.9, 0]);
  ui.fire('p-arc-start', 'change', 45);
  await settle();
  assert.deepEqual(h.sets[0].body, { kind: 'ellipse', startAngle: 45, sweep: 359.9, innerRadius: 0 });
});

test('a refused edit is a toast, never a throw; the panel shows the stored values again and no selection is lost', async () => {
  const h = harness(RECT), calls = [], ui = panels(h, calls);
  const p = await h.select(); ui.update();
  h.verdict({ status: 'refused', code: 'unavailable', reason: 'locked' });
  ui.fire('p-corner-tl', 'change', 12);
  await settle();
  assert.equal(h.sets.length, 1);
  assert.deepEqual(h.toasts, ['The shape could not be changed.']);
  assert.equal(h.root.selectedPaths[0], p, 'the selection stands');
  h.verdict({ status: 'stale' });
  ui.fire('p-corner-tl', 'change', 3);
  await settle();
  assert.match(h.toasts[1], /document moved on/);
  h.verdict({ status: 'applied' });
  assert.deepEqual(calls, [], 'nothing reached a legacy writer');
});

test('a throwing route is a toast too, and a stale selection (another frame or tool) is never edited', async () => {
  const h = harness(RECT), calls = [], ui = panels(h, calls);
  await h.select(); ui.update();
  const real = h.root.NemoNativeObjectVector.desktop;
  h.root.NemoNativeObjectVector.desktop = () => ({ setParametric: async () => { throw new Error('boom'); } });
  ui.fire('p-corner-tl', 'change', 12);
  await settle();
  assert.deepEqual(h.toasts, ['The shape could not be changed.']);
  h.root.NemoNativeObjectVector.desktop = real;
  h.root.state.currentFrame = 4; // the overlay drew the selection for frame 3 (the pick context moved on)
  h.root.state.tool = 'subselect';
  ui.fire('p-corner-tl', 'change', 5);
  await settle();
  assert.equal(h.sets.length, 0, 'nothing was written');
  assert.match(h.toasts[1], /no longer applies/);
});

test('"Rendre dynamique" on a native shape is a notice, never a Paper conversion; a legacy shape still converts', async () => {
  const h = harness(RECT), calls = [], ui = panels(h, calls);
  const p = await h.select(); ui.update();
  ui.fire('btn-ellipse-arc-convert', 'click');
  await settle();
  assert.deepEqual(calls, [], 'no pushUndo (which a native project refuses), no convert');
  assert.deepEqual(h.toasts, ['Making a shape dynamic is not available on native projects yet.']);
  assert.ok(p);
  // a legacy (non-native) selection keeps the baseline path
  const legacy = harness(RECT), lcalls = [], lui = panels(legacy, lcalls);
  legacy.root.selectedPaths = [{ data: { paramShape: { kind: 'ellipse' } }, segments: [], closed: true }];
  assert.equal(Panel.owns(legacy.root, legacy.root.selectedPaths[0]), false);
  assert.throws(() => lui.fire('btn-ellipse-arc-convert', 'click'), /history checkpoint/);
  assert.deepEqual(lcalls, ['legacy:pushUndo'], 'off the native route the baseline click still starts with its checkpoint');
});

test('the ten fields are static native scrubs (one change at release) and have the scrub class in the markup', () => {
  const timeline = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'timeline.js'), 'utf8');
  const html = fs.readFileSync(path.join(__dirname, '..', 'src', 'index.html'), 'utf8');
  const ids = ['p-corner-tl', 'p-corner-tr', 'p-corner-br', 'p-corner-bl', 'p-arc-start', 'p-arc-sweep', 'p-arc-inner', 'p-star-points', 'p-star-inner', 'p-star-corner'];
  const list = /var NATIVE_SCRUB_FIELDS=\[([^\]]*)\]/.exec(timeline)[1];
  for (const id of ids) {
    assert.ok(list.includes(`'${id}'`), id + ' is a static native scrub');
    assert.match(html, new RegExp(`<input[^>]*class="pi scrub"[^>]*id="${id}"`), id + ' scrubs');
  }
});

test('next() maps every panel field and refuses a member the kind does not own', () => {
  assert.deepEqual(Panel.next({ kind: 'rect' }, 'tl', '3', true), { kind: 'rect', tl: 3, tr: 3, br: 3, bl: 3 });
  assert.deepEqual(Panel.next({ kind: 'rect', tl: 1 }, 'tr', 'abc', false), { kind: 'rect', tl: 1, tr: 0, br: 0, bl: 0 });
  assert.equal(Panel.next({ kind: 'rect' }, 'sweep', '3', false), null);
  assert.equal(Panel.next({ kind: 'star' }, 'tl', '3', false), null);
});
