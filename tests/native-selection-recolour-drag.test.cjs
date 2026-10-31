'use strict';
// #2372 recolour-readback-polish (review of #2674/#2676/#2677/#2680): the Selected Colors panel row moves its from-colour to the colour it just sent
// (baseline 1065e9d8 color-manager.js: the synchronous recolour re-rendered the row), so a picker drag or an opacity scrub keeps matching after the
// first debounced native commit; masks and images are skipped by the listing AND the recolour alike.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Style = require('../src/js/adapters/native-selection-style.js');
const Steps = require('../src/js/adapters/native-selection-steps.js');
const Paper = require('./helpers/native-import-paper.cjs');
const overlayStub = require('./helpers/native-selection-overlay-stub.cjs');

const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const quad = (id, x0, y0, x1, y1, style, extra) => ({ stableTarget: { contextId: 'scene-root', layerUid: 'L', frameScope: { kind: 'authored', frame: 0 }, strokeId: id },
  closed: true, count: 4, role: 'plain', filled: true, bounds: [x0, y0, x1, y1], matrix: [1, 0, 0, 1, 0, 0], segments: [seg(x0, y0), seg(x1, y0), seg(x1, y1), seg(x0, y1)],
  style: { width: 4, opacity: 1, ...style }, ...extra });
const tick = () => new Promise((done) => setTimeout(done, 0));
const settle = async () => { for (let i = 0; i < 12; i++) await tick(); };
const COLOR_MANAGER = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'color-manager.js'), 'utf8');

// A tiny DOM: enough for the panel's rows (class, dataset, listeners, children).
function element(tag) {
  const listeners = {}, kids = [], classes = new Set();
  const el = { tagName: tag, style: {}, dataset: {}, title: '', value: '', textContent: '', parentNode: null,
    classList: { add: (x) => classes.add(x), remove: (x) => classes.delete(x), contains: (x) => classes.has(x) },
    addEventListener(k, f) { (listeners[k] = listeners[k] || []).push(f); },
    appendChild(k) { kids.push(k); k.parentNode = this; return k; },
    querySelector(sel) { return el.querySelectorAll(sel)[0] || null; },
    querySelectorAll(sel) { const cls = sel.replace(/^\./, ''); return kids.filter((k) => k.className.split(' ').includes(cls)); },
    fire(type) { (listeners[type] || []).forEach((f) => f.call(el, { stopPropagation() {}, key: '' })); } };
  Object.defineProperty(el, 'className', { get: () => [...classes].join(' '), set(v) { classes.clear(); String(v).split(/\s+/).filter(Boolean).forEach((x) => classes.add(x)); } });
  Object.defineProperty(el, 'innerHTML', { set() { kids.splice(0); }, get: () => '' });
  Object.defineProperty(el, 'children', { get: () => kids.slice() });
  return el;
}

function harness(items, { hold = false } = {}) {
  const paper = Paper.load(), batches = [], timers = [], store = items.map((i) => JSON.parse(JSON.stringify(i)));
  let pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's1', canvasW: 200, canvasH: 200, layers: [{ layerUid: 'L' }] };
  let gate = null, release = () => {};
  if (hold) gate = new Promise((done) => { release = done; });
  const body = element('div'), section = element('div'), picker = { open: (_sw, _hex, cb) => { picker.cb = cb; } };
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, readPresentedObjectSession: () => ({ presentation: { ...pin, frame: 0 } }),
    async readPresentedStrokes(_s, _p, wanted) { return { items: store.map((item) => { const { segments, ...rest } = item; return wanted(rest) ? { ...rest, segments } : rest; }) }; } };
  const root = { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool: 'select', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, layers: [{ layerUid: 'L' }], selectedStrokeIndices: [], canvasBg: '#222222' },
    selectedPaths: [], SMEngineBridge: { renderNow() {} }, showToast() {}, ColorPicker: picker,
    setTimeout: (fn) => { timers.push(fn); return timers.length; }, clearTimeout: (id) => { timers[id - 1] = null; },
    colorHex8: () => { throw new Error('a Paper colour was read'); }, SM: { setCanvasBg() {}, t: (k) => k },
    parseHexInput: (v) => (/^#[0-9a-f]{6}([0-9a-f]{2})?$/i.test(v) ? v.toLowerCase() : null), hexDisplayValue: (h) => h, alphaPctFromHex: (h) => (h.length === 9 ? Math.round(parseInt(h.slice(7), 16) / 2.55) : 100),
    document: { readyState: 'complete', createElement: element, getElementById: (id) => ({ 'selected-colors-body': body, 'selected-colors-hdr': element('div'), 'selected-colors-sec': section })[id] || null },
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: () => true },
    NemoNativeObjectLabs: { desktop: () => ({ batchStrokes: async (layerUid, frame, steps) => {
      if (gate) { const g = gate; gate = null; await g; }
      batches.push({ layerUid, frame, steps: JSON.parse(JSON.stringify(steps)) });
      steps.forEach((s) => { const it = store.find((i) => i.stableTarget.strokeId === s.restyle.stableTarget.strokeId); it.style = { ...it.style, ...s.restyle.style }; });
      pin = { ...pin, contentRevision: pin.contentRevision + 1, documentSnapshotId: 's' + (pin.contentRevision + 1) };
      return { status: 'applied' };
    } }) },
    NemoSelectCanvasIntent: Intent, NemoNativeSelectionSteps: Steps, NemoNativeSelectionStyle: Style, NemoNativeSelectOverlay: overlayStub(Intent) };
  root.updateUI = () => root.updateSelectedColorsPanel();
  root.window = root;
  vm.createContext(root);
  vm.runInContext(COLOR_MANAGER, root);
  return { root, body, batches, store, picker, release,
    row: () => body.querySelectorAll('.color-row')[0],
    async select(ids) { const layers = await Intent.layerPaths(root, (item) => ids.includes(item.stableTarget.strokeId), false, 0); root.selectedPaths = layers.paths; root.NemoNativeSelectOverlay.show(root); root.updateUI(); },
    fire: async () => { timers.splice(0).filter(Boolean).forEach((fn) => fn()); await settle(); },
    fills: () => store.map((i) => i.style.fill) };
}
const SCENE = () => [quad('A', 10, 10, 30, 30, { fill: '#112233' }), quad('B', 40, 40, 60, 60, { fill: '#112233' })];

test('a picker drag keeps matching across two debounced commits: the row moves its from-colour with every event, even while a commit is in flight', async () => {
  const h = harness(SCENE(), { hold: true });
  await h.select(['A', 'B']);
  assert.equal(h.row().dataset.hex, '#112233');
  h.row().querySelector('.cw-mini').fire('click');
  h.picker.cb('#aa0000');
  assert.equal(h.row().dataset.hex, '#aa0000', 'the row moved to the colour it sent before any commit re-read the selection');
  h.picker.cb('#bb0000');
  assert.equal(h.row().dataset.hex, '#bb0000');
  await h.fire(); // the quiet window ends: commit 1 starts and is held by the engine
  assert.equal(h.batches.length, 0);
  // The drag continues while commit 1 is in flight: its from-colour is the one the previous event sent.
  h.picker.cb('#cc0000');
  h.picker.cb('#dd0000');
  h.release(); await settle();
  assert.equal(h.batches.length, 1, 'commit 1 applied the first window: #112233 -> #bb0000');
  assert.deepEqual(h.batches[0].steps.map((s) => s.restyle.style), [{ fill: '#bb0000' }, { fill: '#bb0000' }]);
  await h.fire(); // the quiet window of the continued drag ends: commit 2
  assert.equal(h.batches.length, 2, 'the drag was not silently dropped');
  assert.deepEqual(h.batches[1].steps.map((s) => s.restyle.style), [{ fill: '#dd0000' }, { fill: '#dd0000' }], 'commit 2 recoloured what commit 1 left, to the final colour');
  assert.deepEqual(h.fills(), ['#dd0000', '#dd0000']);
  assert.equal(h.row().dataset.hex, '#dd0000', 'the panel shows the stored colour after the re-read');
});

test('one window of a drag is ONE batch: every event extends the pending edit from the colours the items still show', async () => {
  const h = harness(SCENE());
  await h.select(['A', 'B']);
  h.row().querySelector('.cw-mini').fire('click');
  ['#101010', '#202020', '#303030', '#404040'].forEach((c) => h.picker.cb(c));
  await h.fire();
  assert.equal(h.batches.length, 1);
  assert.deepEqual(h.batches[0].steps.map((s) => s.restyle.style), [{ fill: '#404040' }, { fill: '#404040' }]);
});

test('an opacity scrub follows the same chain and keeps the alpha byte', async () => {
  const h = harness(SCENE(), { hold: true });
  await h.select(['A', 'B']);
  const opacity = h.row().children[2];
  opacity.value = '80'; opacity.fire('input');
  assert.equal(h.row().dataset.hex, '#112233cc');
  await h.fire();
  opacity.value = '60'; opacity.fire('input'); // while the first commit is held
  assert.equal(h.row().dataset.hex, '#11223399');
  h.release(); await settle(); await h.fire();
  assert.deepEqual(h.fills(), ['#11223399', '#11223399']);
});

test('a recolour that does not apply puts the row back to what the selection holds', async () => {
  const h = harness(SCENE());
  await h.select(['A']);
  h.row().querySelector('.cw-mini').fire('click');
  h.picker.cb('#aa0000');
  assert.equal(h.row().dataset.hex, '#aa0000');
  h.root.setTimeout = (fn) => { fn(); return 1; };
  h.store.forEach((i) => { i.style.fill = '#556677'; }); // the document moved on: nothing matches any more, the commit is a no-op
  await h.fire();
  assert.equal(h.row().dataset.hex, '#556677', 'the stale colour is gone');
});

test('masks and images are skipped by the listing AND by the recolour (one rule)', () => {
  const path = (role, style) => ({ data: { nativeRole: role, nativeStyle: style, nativeStableTarget: { strokeId: role } } });
  const mask = path('mask', { fill: '#112233', stroke: '#112233' }), raster = path('raster', { fill: '#112233', stroke: '#112233', opacity: 1 }), plain = path('plain', { fill: '#112233', stroke: '#445566' });
  assert.deepEqual([mask, raster].map(Steps.coloursOf), [[], []]);
  assert.deepEqual(Steps.recolourSteps([mask, raster, plain], '#112233', '#ff0000').map((s) => [s.restyle.stableTarget.strokeId, s.restyle.style]), [['plain', { fill: '#ff0000' }]]);
});
