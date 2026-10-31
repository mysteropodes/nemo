'use strict';
// #2372 native-fill-gradient: the Fill row's gradient button / checkbox, the stop editor and the on-canvas gizmo edit ONE selected native shape through
// the engine. The real listeners of gradient-bridge.js run in a fake DOM over a real native Select selection; every write is one `fillGradient` restyle
// step of one object.vector.batch (one revision, one history entry), nothing reaches a Paper writer, and a refusal is a notice, never a throw.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Style = require('../src/js/adapters/native-selection-style.js');
const Steps = require('../src/js/adapters/native-selection-steps.js');
const Gradient = require('../src/js/adapters/native-fill-gradient.js');
const Contract = require('../src/js/adapters/native-object-vector-contract.js');
const Subselect = require('../src/js/adapters/native-subselect-contract.js');
const Paper = require('./helpers/native-import-paper.cjs');
const overlayStub = require('./helpers/native-selection-overlay-stub.cjs');

const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const quad = (id, x0, y0, x1, y1, extra) => ({ stableTarget: { contextId: 'scene-root', layerUid: 'L', frameScope: { kind: 'authored', frame: 0 }, strokeId: id },
  closed: true, count: 4, role: 'plain', filled: true, bounds: [x0, y0, x1, y1], matrix: [1, 0, 0, 1, 0, 0], segments: [seg(x0, y0), seg(x1, y0), seg(x1, y1), seg(x0, y1)],
  style: { fill: '#112233', stroke: '#445566', width: 4, opacity: 1 }, ...extra });
const RAMP = { kind: 'linear', from: [10, 20], to: [30, 20], stops: [{ offset: 0, color: '#ff0000cc' }, { offset: 1, color: '#0000ffff' }] };
const withRamp = (id, x0, y0, x1, y1, extra) => quad(id, x0, y0, x1, y1, { style: { fill: '#ff0000cc', width: 4, opacity: 0.8, fillGradient: JSON.parse(JSON.stringify(RAMP)) }, ...extra });
const image = (id) => quad(id, 200, 200, 220, 220, { role: 'raster', style: { opacity: 1 } });
const ribbon = (id) => quad(id, 100, 100, 180, 120, { role: 'ribbon', style: { fill: '#101010', width: 8, opacity: 1 } });
const tick = () => new Promise((done) => setTimeout(done, 0));
const settle = async () => { for (let i = 0; i < 8; i++) await tick(); };

// A DOM element as gradient-bridge uses it: children, listeners, dataset, style, a value and a checked flag.
class El {
  constructor(id) { this.id = id; this.children = []; this.listeners = {}; this.dataset = {}; this.style = {}; this.checked = false; this.disabled = false; this.value = ''; this.className = ''; }
  addEventListener(type, fn) { (this.listeners[type] = this.listeners[type] || []).push(fn); }
  appendChild(child) { this.children.push(child); return child; }
  set innerHTML(v) { this.children = []; }
  get innerHTML() { return ''; }
  fire(type, extra) {
    const event = Object.assign({ type, button: 0, target: this, stopped: false, prevented: false, stopImmediatePropagation() { this.stopped = true; }, preventDefault() { this.prevented = true; } }, extra);
    for (const fn of this.listeners[type] || []) { if (event.stopped) break; fn.call(this, event); }
    return event;
  }
}

function harness(items, options = {}) {
  const paper = Paper.load(), toasts = [], batches = [], timers = [], legacy = [], ids = {};
  const pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's1', canvasW: 400, canvasH: 400, layers: [{ layerUid: 'L' }] };
  const store = items.map((i) => JSON.parse(JSON.stringify(i)));
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, readPresentedObjectSession: () => ({ presentation: { ...pin, frame: 0 } }),
    async readPresentedStrokes(_selector, _pin, wanted) { return { items: store.map((item) => { const { segments, ...rest } = item; return wanted(rest) ? { ...rest, segments } : rest; }) }; } };
  const mapped = { ...Intent, worldPoint: (_root, x, y) => ({ x, y, perPixel: 1 }) };
  const document = { readyState: 'complete', getElementById: (id) => (ids[id] = ids[id] || new El(id)), createElement: () => new El('') };
  const root = { paper, Path: paper.Path, CompoundPath: paper.CompoundPath || function CompoundPath() {}, Point: paper.Point, Matrix: paper.Matrix, document,
    state: { tool: 'select', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, layers: [{ layerUid: 'L' }], selectedStrokeIndices: [] },
    selectedPaths: [], showToast: (t) => toasts.push(t), setTimeout: (fn) => { timers.push(fn); return timers.length; }, clearTimeout: (id) => { timers[id - 1] = null; },
    pushUndo: () => legacy.push('pushUndo'), saveActiveLayerFrameOrPromote: () => legacy.push('save'), colorHex8: () => '#000000ff',
    SMEngineBridge: { renderNow() {}, isEnabled: () => false }, SM: { t: (key) => `t:${key}` },
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: () => true },
    NemoNativeObjectLabs: { desktop: () => ({ batchStrokes: async (layerUid, frame, steps) => {
      batches.push({ layerUid, frame, steps: JSON.parse(JSON.stringify(steps)) });
      if (options.refuse) return { status: 'unavailable' };
      // The engine's own rule: the named record takes the stored gradient (null drops it); the listing then carries it.
      for (const step of steps) { const item = store.find((i) => i.stableTarget.strokeId === step.restyle.stableTarget.strokeId), g = step.restyle.style.fillGradient;
        if (g) item.style.fillGradient = g; else delete item.style.fillGradient; }
      return { status: 'applied' };
    } }) },
    NemoSelectCanvasIntent: mapped, NemoNativeSelectionSteps: Steps, NemoNativeSelectionStyle: Style, NemoNativeFillGradient: Gradient, NemoNativeSelectOverlay: overlayStub(mapped) };
  root.window = root;
  // updateUI re-reads the properties panel, as timeline.js's updateProps does.
  root.updateUI = () => { if (root.renderGradientPanel) root.renderGradientPanel(); };
  const ctx = vm.createContext(root);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '../src/js/gradient-bridge.js'), 'utf8'), ctx, { filename: 'gradient-bridge.js' });
  const el = (id) => document.getElementById(id);
  return { root, toasts, batches, timers, legacy, store, el, canvas: el('canvas-area'),
    async select(list) { const layers = await Intent.layerPaths(root, (item) => list.includes(item.stableTarget.strokeId), false, 0); root.selectedPaths = layers.paths; root.NemoNativeSelectOverlay.show(root); root.renderGradientPanel(); },
    fire: async () => { const fns = timers.splice(0).filter(Boolean); fns.forEach((fn) => fn()); await settle(); },
    // The Fill row's button (timeline.js initFillGradientButton): flips the checkbox and dispatches a real change.
    async toggleButton() { const cb = el('p-grad-on'); if (cb.disabled) { root.showToast(root.SM.t('toastSelectSingleShapeForGradient')); return; } cb.checked = !cb.checked; cb.fire('change'); await this.fire(); },
    stops() { return el('p-grad-stops-list').children; } };
}
const steps = (batch) => batch.steps.map((s) => s.restyle.style);
const accepted = (batch) => Contract.payload(Contract.VECTOR, { command: 'object.vector.batch', ops: batch.steps.map((s) => ({ op: 'restyle', stableTarget: s.restyle.stableTarget, style: s.restyle.style, frame: batch.frame })) });

test('the gradient controls show the selected native shape\'s real state, from its listing', async () => {
  const h = harness([quad('A', 10, 10, 30, 30), withRamp('G', 40, 10, 60, 30), quad('B', 70, 10, 90, 30)]);
  const cb = h.el('p-grad-on');
  await h.select(['A']);
  assert.deepEqual([cb.checked, cb.disabled], [false, false], 'a flat shape: the button is off and usable');
  await h.select(['G']);
  assert.deepEqual([cb.checked, cb.disabled], [true, false], 'a gradient shape: on');
  assert.equal(h.el('p-grad-kind').value, 'linear');
  const rows = h.stops();
  assert.equal(rows.length, 2);
  assert.equal(rows[0].children[0].value, '#ff0000', 'the colour input shows the stop');
  assert.equal(rows[0].children[0].dataset.hex8, '#ff0000cc', 'and keeps its alpha in hex8 (CLAUDE.md section 2)');
  assert.equal(rows[1].children[1].value, 100, 'the offset in percent');
  await h.select(['A', 'B']);
  assert.deepEqual([cb.checked, cb.disabled], [false, true], 'two shapes: disabled');
  await h.select(['I'].filter(() => false));
  assert.equal(cb.disabled, true, 'nothing selected: disabled');
  assert.deepEqual(h.legacy, [], 'no Paper writer was reached');
});

test('turning the gradient on is ONE restyle with the baseline default gradient, turning it off is ONE restyle back to the flat fill', async () => {
  const h = harness([quad('A', 10, 10, 30, 30)]);
  await h.select(['A']);
  await h.toggleButton();
  assert.equal(h.batches.length, 1, 'one batch');
  assert.equal(h.batches[0].steps.length, 1, 'one restyle step: one revision, one history entry');
  const half = Math.hypot(20, 20) / 2;
  assert.deepEqual(steps(h.batches[0]), [{ fillGradient: { kind: 'linear', from: [20 - half, 20], to: [20 + half, 20],
    stops: [{ offset: 0, color: '#112233ff' }, { offset: 1, color: '#0000ffff' }] } }], 'across the bounds\' half diagonal, the fill to blue');
  accepted(h.batches[0]);
  assert.equal(h.el('p-grad-on').checked, true, 'the listing (re-read after the commit) shows it on');
  assert.equal(h.stops().length, 2);
  await h.toggleButton();
  assert.equal(h.batches.length, 2);
  assert.deepEqual(steps(h.batches[1]), [{ fillGradient: null }], 'off is the flat fill');
  accepted(h.batches[1]);
  assert.equal(h.el('p-grad-on').checked, false);
  assert.deepEqual(h.legacy, [], 'no legacy undo entry, save or Paper render');
});

test('a stop colour keeps its alpha and an offset is typed in percent: each change is ONE restyle with the whole gradient, stops in ascending order', async () => {
  const h = harness([withRamp('G', 10, 10, 30, 30)]);
  await h.select(['G']);
  const [first] = h.stops();
  first.children[0].value = '#00ff00';
  first.children[0].fire('change');
  await h.fire();
  assert.equal(h.batches.length, 1);
  assert.deepEqual(steps(h.batches[0]), [{ fillGradient: { ...RAMP, stops: [{ offset: 0, color: '#00ff00cc' }, { offset: 1, color: '#0000ffff' }] } }], 'the alpha byte survives the repick');
  accepted(h.batches[0]);
  // A stop moved past its neighbour: the stored order follows the offsets (a renderer drops a stop that is out of order).
  h.stops()[1].children[1].value = '50';
  h.stops()[1].children[1].fire('change');
  await h.fire();
  accepted(h.batches[1]);
  h.stops()[0].children[1].value = '80';
  h.stops()[0].children[1].fire('change');
  await h.fire();
  assert.deepEqual(steps(h.batches[2])[0].fillGradient.stops, [{ offset: 0.5, color: '#0000ffff' }, { offset: 0.8, color: '#00ff00cc' }], 'sorted by offset');
  accepted(h.batches[2]);
  const kind = h.el('p-grad-kind');
  kind.value = 'radial';
  kind.fire('change');
  await h.fire();
  assert.equal(steps(h.batches[3])[0].fillGradient.kind, 'radial');
  accepted(h.batches[3]);
  assert.deepEqual(h.legacy, []);
});

test('two quick edits are one batch and a third stop is refused with a notice', async () => {
  const h = harness([withRamp('G', 10, 10, 30, 30)]);
  await h.select(['G']);
  const [a, b] = h.stops();
  a.children[0].value = '#00ff00'; a.children[0].fire('change');
  b.children[1].value = '50'; b.children[1].fire('change');
  await h.fire();
  assert.equal(h.batches.length, 1, 'coalesced');
  assert.deepEqual(steps(h.batches[0])[0].fillGradient.stops, [{ offset: 0, color: '#00ff00cc' }, { offset: 0.5, color: '#0000ffff' }], 'both edits are in it');
  h.el('btn-grad-add-stop').fire('click');
  assert.equal(h.batches.length, 1, 'no write');
  assert.match(h.toasts.at(-1), /two stops/);
});

test('a gizmo drag commits ONCE at release; a click on a handle, a cancel and a press elsewhere write nothing', async () => {
  const h = harness([withRamp('G', 10, 10, 30, 30)]);
  await h.select(['G']);
  const down = (x, y) => h.canvas.fire('pointerdown', { clientX: x, clientY: y });
  const move = (x, y) => h.canvas.fire('pointermove', { clientX: x, clientY: y });
  const up = (x, y) => h.canvas.fire('pointerup', { clientX: x, clientY: y });
  assert.equal(down(100, 100).stopped, false, 'a press away from the handles is the Select gesture\'s');
  const grab = down(30, 20);
  assert.deepEqual([grab.stopped, grab.prevented], [true, true], 'the "to" handle is taken');
  for (const [x, y] of [[40, 22], [60, 30], [70, 40]]) assert.equal(move(x, y).stopped, true);
  assert.equal(h.batches.length, 0, 'nothing is written while dragging');
  assert.equal(h.timers.filter(Boolean).length, 0);
  up(70, 40);
  await h.fire();
  assert.equal(h.batches.length, 1, 'ONE commit at release');
  assert.deepEqual(steps(h.batches[0]), [{ fillGradient: { ...RAMP, to: [70, 40] } }]);
  accepted(h.batches[0]);
  // The same handle clicked without moving, and a cancelled drag: no write.
  down(10, 20); up(10, 20);
  down(10, 20); move(50, 50); h.canvas.fire('pointercancel', { clientX: 50, clientY: 50 });
  await h.fire();
  assert.equal(h.batches.length, 1);
  assert.deepEqual(h.legacy, []);
});

test('a stop diamond drags along the from-to line and clamps at the ends; the shape\'s presented matrix is honoured', async () => {
  // Record space is turned into the world by x -> 5x + 100, y -> 5y: the ends (10,20)-(30,20) sit at (150,100)-(250,100); the first stop is a quarter of the way.
  const ramp = { ...RAMP, stops: [{ offset: 0.25, color: '#ff0000cc' }, { offset: 1, color: '#0000ffff' }] };
  const h = harness([quad('G', 10, 10, 30, 30, { matrix: [5, 0, 0, 5, 100, 0], style: { fill: '#ff0000cc', width: 4, opacity: 0.8, fillGradient: ramp } })]);
  await h.select(['G']);
  const fire = (type, x, y) => h.canvas.fire(type, { clientX: x, clientY: y });
  assert.equal(fire('pointerdown', 200, 100).stopped, false, 'the line between the handles is not a handle');
  assert.equal(fire('pointerdown', 175, 100).stopped, true, 'the first stop sits a quarter along the mapped line');
  fire('pointermove', 225, 190);
  fire('pointerup', 225, 190);
  await h.fire();
  assert.deepEqual(steps(h.batches[0])[0].fillGradient, { ...ramp, stops: [{ offset: 0.75, color: '#ff0000cc' }, { offset: 1, color: '#0000ffff' }] }, 'projected on the line: (225 - 150) / 100');
  accepted(h.batches[0]);
  assert.equal(fire('pointerdown', 225, 100).stopped, true, 'the stop is now three quarters along');
  fire('pointermove', 1000, 0);
  fire('pointerup', 1000, 0);
  await h.fire();
  assert.deepEqual(steps(h.batches[1])[0].fillGradient.stops.map((t) => t.offset), [1, 1], 'dragged off the end it pins there');
  accepted(h.batches[1]);
  // The "to" end is moved in world space and stored in record space.
  assert.equal(fire('pointerdown', 250, 100).stopped, true);
  fire('pointermove', 300, 300);
  fire('pointerup', 300, 300);
  await h.fire();
  assert.deepEqual(steps(h.batches[2])[0].fillGradient.to, [40, 60], 'world (300,300) is record (40,60)');
  assert.equal(h.legacy.length, 0);
});

test('a refused edit is a notice and puts the controls back; a ribbon, an image and several shapes explain themselves and write nothing', async () => {
  const h = harness([quad('A', 10, 10, 30, 30), ribbon('R'), image('I')], { refuse: true });
  await h.select(['A']);
  await h.toggleButton();
  assert.equal(h.batches.length, 1, 'it was sent');
  assert.ok(h.toasts.some((t) => /could not be applied/.test(t)), 'the style module explains the refusal');
  assert.equal(h.el('p-grad-on').checked, false, 'the checkbox is back to what the listing says');
  assert.deepEqual(h.legacy, []);
  for (const pick of [['R'], ['I'], ['A', 'R']]) {
    await h.select(pick);
    const before = h.batches.length;
    assert.equal(h.el('p-grad-on').disabled, true, pick.join());
    await h.toggleButton();
    const r = await Gradient.toggle(h.root, true);
    assert.equal(r.status, 'unavailable', 'a direct call is refused too');
    assert.equal(h.batches.length, before, 'no write');
    assert.equal(h.toasts.at(-1), 't:toastSelectSingleShapeForGradient');
  }
});

test('the restyle contract, the listing contract and the steps admit exactly the two-stop gradient', () => {
  const target = quad('A', 0, 0, 1, 1).stableTarget, op = (style) => ({ command: 'object.vector.batch', ops: [{ op: 'restyle', stableTarget: target, style, frame: 0 }] });
  Contract.payload(Contract.VECTOR, op({ fillGradient: RAMP }));
  Contract.payload(Contract.VECTOR, op({ fillGradient: null }));
  const stops = (a, b) => [{ offset: 0, color: a }, { offset: 1, color: b }];
  for (const bad of [{ fillGradient: { ...RAMP, kind: 'conic' } }, { fillGradient: { ...RAMP, stops: [RAMP.stops[0]] } }, { fillGradient: { ...RAMP, stops: stops('#f00', '#00f') } },
    { fillGradient: { ...RAMP, from: [0] } }, { fillGradient: {} }, { fillGradient: 'red' }, { fillGradient: RAMP, fill: '#ff0000' }, { fillGradient: null, unmask: true }]) {
    assert.throws(() => Contract.payload(Contract.VECTOR, op(bad)), TypeError, JSON.stringify(bad));
  }
  const listing = (style) => ({ kind: 'subselect', contextId: 'scene-root', layerUid: 'L', frame: 0, workId: 'native-work-1', viewGeneration: 1, cursor: 0, hitTest: true, atRevision: 7,
    documentSnapshotId: 'native-object:document:7', total: 1, nextCursor: null, preview: false,
    items: [{ stableTarget: target, closed: true, count: 4, matrix: [1, 0, 0, 1, 0, 0], bounds: [0, 0, 1, 1], role: 'plain', filled: true, style }] });
  Subselect.viewResult(listing({ fill: '#ff0000cc', fillGradient: RAMP }));
  for (const bad of [{ fillGradient: null }, { fillGradient: {} }, { fillGradient: { ...RAMP, stops: [RAMP.stops[0]] } }, { fillGradient: { ...RAMP, to: [1] } }, { fillGradient: { ...RAMP, stops: stops('red', '#00f') } }]) {
    assert.throws(() => Subselect.viewResult(listing(bad)), TypeError, JSON.stringify(bad));
  }
  assert.deepEqual(Steps.valid({ fillGradient: null }), { fillGradient: null });
  assert.equal(Steps.valid({ fillGradient: { ...RAMP, stops: stops('#f00', '#00f') } }), null);
  assert.equal(Steps.alone({ fillGradient: null }), 'fillGradient', 'the edit stands alone');
});
