'use strict';
// #2372 RV6-STYLE: the review of the native Select restyle (#2630 / #2632). Each finding is pinned by a test that failed before the fix:
// stale selection, held-frame race, flush on a context change or a new pick, scrub through the real ui.js scrub, linked-fill style, multi-layer, ribbon ink.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Style = require('../src/js/adapters/native-selection-style.js');
const Steps = require('../src/js/adapters/native-selection-steps.js');
const Paper = require('./helpers/native-import-paper.cjs');
const overlayStub = require('./helpers/native-selection-overlay-stub.cjs');

const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const quad = (id, x0, y0, x1, y1, extra) => ({ stableTarget: { contextId: 'scene-root', layerUid: 'L', frameScope: { kind: 'authored', frame: 0 }, strokeId: id },
  closed: true, count: 4, role: 'plain', filled: true, bounds: [x0, y0, x1, y1], matrix: [1, 0, 0, 1, 0, 0], segments: [seg(x0, y0), seg(x1, y0), seg(x1, y1), seg(x0, y1)],
  style: { fill: '#112233', stroke: '#445566', width: 4, opacity: 1 }, ...extra });
const tick = () => new Promise((done) => setTimeout(done, 0));
const settle = async () => { for (let i = 0; i < 8; i++) await tick(); };

function harness(items, options = {}) {
  const paper = Paper.load(), toasts = [], batches = [], timers = [], gates = { batch: null, read: null };
  let pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's1', canvasW: 200, canvasH: 200, layers: [{ layerUid: 'L' }, { layerUid: 'M' }] };
  const store = items.map((i) => JSON.parse(JSON.stringify(i)));
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, readPresentedObjectSession: () => ({ presentation: { ...pin, frame: root.state.currentFrame } }),
    async readPresentedStrokes(_selector, _pin, wanted) { if (gates.read) await gates.read; return { items: store.map((item) => { const { segments, ...rest } = item; return wanted(rest) ? { ...rest, segments } : rest; }) }; } };
  const root = { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool: 'select', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, selectedStrokeIndices: [] },
    selectedPaths: [], updateUI() {}, SMEngineBridge: { renderNow() {} }, showToast: (t) => toasts.push(t),
    setTimeout: (fn) => { timers.push(fn); return timers.length; }, clearTimeout: (id) => { timers[id - 1] = null; },
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: (_r, at) => { root.state.activeLayerIdx = at; return true; } },
    NemoNativeObjectLabs: { desktop: () => ({ batchStrokes: async (layerUid, frame, steps) => {
      batches.push({ layerUid, frame, steps: JSON.parse(JSON.stringify(steps)) });
      if (gates.batch) await gates.batch;
      steps.forEach((s) => { const it = store.find((i) => i.stableTarget.strokeId === s.restyle.stableTarget.strokeId); it.style = { ...it.style, ...s.restyle.style }; });
      pin = { ...pin, contentRevision: pin.contentRevision + 1, documentSnapshotId: 's' + (pin.contentRevision + 1) };
      return { status: 'applied' };
    } }) },
    NemoSelectCanvasIntent: Intent, NemoNativeSelectionSteps: Steps };
  root.NemoNativeSelectionStyle = Style;
  root.NemoNativeSelectOverlay = overlayStub(Intent, (r) => Style.flush(r));
  return { root, toasts, batches, store, gates,
    async select(ids) { const layers = await Intent.layerPaths(root, (item) => ids.includes(item.stableTarget.strokeId), false, 0); root.selectedPaths = layers.paths; root.NemoNativeSelectOverlay.show(root); },
    fire: async () => { const fns = timers.splice(0).filter(Boolean); fns.forEach((fn) => fn()); await settle(); } };
}

test('a selection that outlived a frame, layer or tool change is dropped, never restyled into another cell', async () => {
  for (const [name, move] of [['frame', (s) => { s.currentFrame = 10; }], ['layer', (s) => { s.activeLayerIdx = 1; }], ['tool', (s) => { s.tool = 'subselect'; }]]) {
    const h = harness([quad('A', 10, 10, 30, 30)]);
    await h.select(['A']);
    assert.deepEqual(await Promise.race([Style.apply(h.root, { fill: '#ff0000' }), tick().then(() => 'pending')]), 'pending', 'control: the drawn selection is taken');
    await h.fire();
    const batches = h.batches.length;
    await h.select(['A']);
    move(h.root.state);
    const result = await Style.apply(h.root, { fill: '#00ff00' });
    await h.fire();
    assert.deepEqual(result, { status: 'unavailable' }, name);
    assert.equal(h.batches.length, batches, name + ': nothing was written');
    assert.deepEqual(h.root.selectedPaths, [], name + ': the stale selection is cleared');
    assert.match(h.toasts.join('|'), /select the items again/, name);
  }
});

test('an edit still waiting when the frame changes commits to the cell it was made on', async () => {
  const h = harness([quad('A', 10, 10, 30, 30)]);
  await h.select(['A']);
  const done = Style.apply(h.root, { fill: '#ff0000' });
  h.root.state.currentFrame = 10; // the user scrubs inside the quiet window
  h.root.NemoNativeSelectOverlay.refresh(h.root); // the overlay sees its context ended and flushes the style edit
  await settle();
  assert.deepEqual(await done, { status: 'applied' });
  assert.equal(h.batches.length, 1, 'the edit was not dropped');
  assert.equal(h.batches[0].frame, 0, 'it was written to the frame it was made on, not the one scrubbed to');
  assert.deepEqual(h.batches[0].steps.map((s) => s.restyle.style), [{ fill: '#ff0000' }]);
  assert.deepEqual(h.root.selectedPaths, [], 'the selection of the left frame is dropped, not re-read there');
});

test('a new pick commits the pending edit first, to the layer it was made on', async () => {
  const h = harness([quad('A', 10, 10, 30, 30), quad('B', 50, 50, 70, 70)]);
  await h.select(['A']);
  const done = Style.apply(h.root, { width: 9 });
  const other = (await Intent.layerPaths(h.root, (item) => item.stableTarget.strokeId === 'B', false, 0)).paths[0];
  other.data.nativeLayerIndex = 1; other.data.nativeLayerUid = 'M'; // the pick is on another layer: applyPick activates it
  Intent.applyPick(h.root, other, false);
  await settle();
  assert.deepEqual(await done, { status: 'applied' });
  assert.equal(h.batches.length, 1);
  assert.equal(h.batches[0].layerUid, 'L');
  assert.deepEqual(h.batches[0].steps.map((s) => [s.restyle.stableTarget.strokeId, s.restyle.style]), [['A', { width: 9 }]]);
  assert.deepEqual(h.root.selectedPaths.map((p) => p.data.strokeId), ['B'], 'the new pick stands');
});

test('an edit made while a commit runs waits for it and follows the selection it re-read', async () => {
  const h = harness([quad('A', 10, 10, 30, 30)]);
  await h.select(['A']);
  const seen = [];
  const steps = Object.assign({}, Steps, { restyleSteps: (paths, style, last) => { seen.push(paths.slice()); return Steps.restyleSteps(paths, style, last); } });
  h.root.NemoNativeSelectionSteps = steps;
  let release; h.gates.read = null;
  const first = Style.apply(h.root, { width: 6 });
  await h.fire(); // commit one runs and re-reads the selection
  await first;
  const afterFirst = h.root.selectedPaths.slice();
  h.gates.read = new Promise((go) => { release = go; });
  const again = Style.apply(h.root, { width: 8 });
  await h.fire(); // commit two starts; its re-read waits on the gate
  const during = Style.apply(h.root, { opacity: 0.5 }); // typed while the re-read is in flight
  await settle();
  release(); h.gates.read = null;
  await settle(); await h.fire(); await settle();
  assert.deepEqual([await again, await during].map((r) => r.status), ['applied', 'applied']);
  assert.equal(h.batches.length, 3, 'three edits, three commits, none lost');
  assert.deepEqual(h.batches[2].steps.map((s) => s.restyle.style), [{ opacity: 0.5 }]);
  assert.notEqual(seen[2][0], seen[1][0], 'the late edit was built on the re-read selection, not the one before the commit');
  assert.equal(seen[1][0], afterFirst[0], 'control: commit two started from the selection commit one re-read');
  assert.doesNotMatch(h.toasts.join('|'), /changed/);
});

test('a selection across layers restyles in ONE batch (MULTI-LAYER); a ribbon given two colours takes the one written last', async () => {
  const h = harness([quad('A', 10, 10, 30, 30), quad('R', 40, 40, 90, 60, { role: 'ribbon', style: { fill: '#101010', width: 8, opacity: 1 } })]);
  await h.select(['A']);
  const second = (await Intent.layerPaths(h.root, (item) => item.stableTarget.strokeId === 'R', false, 0)).paths[0];
  second.data.nativeLayerIndex = 1; second.data.nativeLayerUid = 'M';
  h.root.selectedPaths = h.root.selectedPaths.concat([second]); h.root.NemoNativeSelectOverlay.show(h.root);
  const spanning = Style.apply(h.root, { fill: '#ff0000' });
  await h.fire();
  assert.deepEqual(await spanning, { status: 'applied' });
  assert.doesNotMatch(h.toasts.join('|'), /one layer at a time/);
  assert.equal(h.batches.length, 1, 'one native batch for both layers: a restyle is one undo entry');
  assert.deepEqual(h.batches[0].steps.map((s) => s.restyle.stableTarget.strokeId), ['A', 'R']);
  // A ribbon given a fill colour and then a different stroke colour in one window is inked by the later write, as the baseline's two setters were.
  for (const [order, expected] of [[[{ fill: '#ff0000' }, { stroke: '#00ff00' }], { stroke: '#00ff00' }], [[{ stroke: '#00ff00' }, { fill: '#ff0000' }], { fill: '#ff0000' }]]) {
    const r = harness([quad('R', 40, 40, 90, 60, { role: 'ribbon', style: { fill: '#101010', width: 8, opacity: 1 } })]);
    await r.select(['R']);
    const done = order.map((style) => Style.apply(r.root, style));
    await r.fire();
    assert.deepEqual((await Promise.all(done)).map((x) => x.status), ['applied', 'applied']);
    assert.deepEqual(r.batches[0].steps.map((s) => s.restyle.style), [expected], JSON.stringify(order));
  }
});

test('a texture anchor is offered to the engine with the plain items, which decides what it can take', async () => {
  const h = harness([quad('A', 10, 10, 30, 30), quad('T', 40, 40, 60, 60, { role: 'brush', brush: 'anchor', group: 'g' })]);
  await h.select(['A', 'T']);
  Style.apply(h.root, { stroke: '#00ff00' });
  await h.fire();
  assert.deepEqual(h.batches[0].steps.map((s) => s.restyle.stableTarget.strokeId), ['A', 'T'], 'one batch; the engine skips the anchor the edit would unhide and restyles the rest');
});

// ---- the numeric fields scrub through the real ui.js scrub --------------------------------------------------------------------------------------
const TIMELINE = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'timeline.js'), 'utf8');
function extract(name) {
  const from = TIMELINE.indexOf('function ' + name + '(');
  assert.ok(from >= 0, name);
  let depth = 0, at = TIMELINE.indexOf('{', from);
  for (let i = at; i < TIMELINE.length; i++) { if (TIMELINE[i] === '{') depth++; else if (TIMELINE[i] === '}' && --depth === 0) return TIMELINE.slice(from, i + 1); }
  throw new Error('unterminated ' + name);
}
function scrubbing(h, id, mark) {
  const uiSource = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'ui.js'), 'utf8');
  const start = uiSource.indexOf('var scrubState=null;'), end = uiSource.indexOf('})();', start);
  assert.ok(start >= 0 && end > start);
  const listeners = new Map(), events = [], fields = {};
  const field = (fid, value) => { let held = String(value); fields[fid] = { id: fid, get value() { return held; }, set value(v) { held = String(v); }, min: '0', max: '100', dataset: { step: '1' }, closest() { return this; }, setPointerCapture() {}, focus() {}, select() {}, classList: { add() {}, remove() {} },
    dispatchEvent(event) { events.push({ type: event.type, value: this.value }); if (event.type === 'change') h.onChange(this); } }; return fields[fid]; };
  ['p-sw', 'p-opacity', 'p-stroke-alpha', 'p-poststroke-smooth', 'p-miterlimit', 'p-dashoffset'].forEach((fid) => field(fid, '25'));
  const document = { activeElement: null, getElementById: (fid) => fields[fid] || null, addEventListener(type, fn) { listeners.set(type, fn); } };
  const window = Object.assign(h.root, { document, pushUndo() { throw new Error('native authority denied legacy history checkpoint'); }, addEventListener() {} });
  vm.runInNewContext(uiSource.slice(start, end), { document, window, Math, Event: class { constructor(type) { this.type = type; } }, requestAnimationFrame: (fn) => setTimeout(fn, 0), cancelAnimationFrame() {} });
  const sandbox = { window, document, Object, Array, parseFloat, parseInt, Math, console };
  vm.createContext(sandbox);
  const fieldsList = TIMELINE.match(/var NATIVE_SCRUB_FIELDS=\[[^\]]*\];/)[0];
  vm.runInContext(fieldsList + '\n' + ['markNativeScrubFields', 'nativeTypedInput', 'nativeScrubChange'].map(extract).join('\n') + '\nvar _nativeTyped={};this.f={markNativeScrubFields,nativeTypedInput,nativeScrubChange};', sandbox);
  if (mark) sandbox.f.markNativeScrubFields();
  const el = fields[id];
  const fire = (type, clientX) => listeners.get(type)({ type, target: el, pointerId: 3, clientX, preventDefault() {} });
  return { el, events, fire, f: sandbox.f, fields };
}

test('a scrub of the width field is ONE native commit when the selection owns the field; unmarked, the legacy checkpoint throws mid-drag', async () => {
  const h = harness([quad('A', 10, 10, 30, 30)]);
  await h.select(['A']);
  h.onChange = (el) => { Style.apply(h.root, { width: parseInt(el.value) }); }; // the #p-sw change listener
  const control = scrubbing(h, 'p-sw', false);
  control.fire('pointerdown', 100);
  assert.throws(() => control.fire('pointermove', 160), /native authority denied/, 'without the mark ui.js pushes an undo the native project refuses');
  const s = scrubbing(h, 'p-sw', true);
  assert.equal(s.el.dataset.nativeOpacityScrub, 'static');
  s.fire('pointerdown', 100);
  assert.doesNotThrow(() => s.fire('pointermove', 160));
  s.fire('pointermove', 200);
  await h.fire();
  assert.equal(h.batches.length, 0, 'nothing is written while the drag runs');
  s.fire('pointerup', 200);
  await h.fire();
  assert.deepEqual(s.events, [{ type: 'change', value: '50' }], 'one change at release');
  assert.equal(h.batches.length, 1, 'one native commit for the whole gesture');
  assert.deepEqual(h.batches[0].steps.map((x) => x.restyle.style), [{ width: 50 }]);
  // With the selection gone the fields are released to the legacy scrub.
  h.root.selectedPaths = [];
  s.f.markNativeScrubFields();
  assert.equal(s.fields['p-sw'].dataset.nativeOpacityScrub, undefined);
});

test('the opacity and stroke alpha fields, which listen to input, take the one change a scrub ends with and not a typed edit twice', async () => {
  const h = harness([quad('A', 10, 10, 30, 30)]);
  await h.select(['A']);
  const calls = [];
  const s = scrubbing(h, 'p-opacity', true);
  h.onChange = (el) => s.f.nativeScrubChange(el, () => calls.push(['change', el.value]));
  s.fire('pointerdown', 100); s.fire('pointermove', 160); s.fire('pointerup', 160);
  assert.deepEqual(calls, [['change', '40']], 'a scrub applies at its change');
  s.el.value = '70'; s.f.nativeTypedInput(s.el); calls.push(['input', '70']); // a typed edit: input first, then change on blur
  s.f.nativeScrubChange(s.el, () => calls.push(['change', s.el.value]));
  assert.deepEqual(calls, [['change', '40'], ['input', '70']], 'the change of a typed edit is not applied again');
  assert.equal(['p-opacity', 'p-stroke-alpha', 'p-poststroke-smooth', 'p-miterlimit', 'p-dashoffset'].every((id) => s.fields[id].dataset.nativeOpacityScrub === 'static'), true);
});

// ---- a linked fill is picked as its ribbon, with the ribbon's own listed style ------------------------------------------------------------------
test('a click on a ribbon linked fill selects the ribbon and shows the ribbon listed style, not a placeholder', async () => {
  const paper = Paper.load();
  const pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 4, documentSnapshotId: 's4', canvasW: 1920, canvasH: 1080, layers: [{ layerUid: 'L1' }] };
  const t = (id) => ({ contextId: 'scene-root', layerUid: 'L1', frameScope: { kind: 'authored', frame: 0 }, strokeId: id });
  const items = [
    { stableTarget: t('fill'), closed: true, count: 4, role: 'fill', owner: 'ribbon', bounds: [600, 0, 700, 100], matrix: [1, 0, 0, 1, 0, 0], segments: [seg(600, 0), seg(700, 0), seg(700, 100), seg(600, 100)], style: { fill: '#aa0000' } },
    { stableTarget: t('ribbon'), closed: true, count: 4, role: 'ribbon', bounds: [100, -20, 500, 20], matrix: [1, 0, 0, 1, 0, 300], segments: [seg(100, -20), seg(500, -20), seg(500, 20), seg(100, 20)], style: { fill: '#10203080', width: 8, opacity: 0.5 } },
  ];
  const session = { isActive: () => true, blocksLegacy: () => true, info: () => pin, readPresentedObjectSession: () => ({ presentation: { ...pin, frame: 0 } }),
    async readPresentedStrokes(_s, _p, wanted) { return { items: items.map(({ segments, ...item }) => (wanted(item) ? { ...item, segments } : item)) }; } };
  const r = { selectedPaths: [], state: { activeLayerIdx: 0, selectedStrokeIndices: [] }, Path: paper.Path, Matrix: paper.Matrix, Point: paper.Point, NemoNativeObjectProject: { desktop: () => session },
    document: { getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 960, height: 540 }) }) },
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX * 2, y: e.clientY * 2 }) } } };
  const hit = await Intent.pick(r, 325, 25);
  assert.deepEqual([hit.data.strokeId, hit.data.nativeRole], ['ribbon', 'ribbon']);
  assert.deepEqual(Style.shown(hit), { role: 'ribbon', fill: '#10203080', width: 8, opacity: 0.5 }, 'the panel reads the ribbon style, not black');
});
