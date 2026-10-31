'use strict';
// #2372 SEL-STYLE2: the paint switches, cap / join / miter / dash / paint order / stroke style, the swap of the two paints, a brush preset and
// Smooth on a native Select selection are each ONE native object.vector.batch with exactly the members the baseline setters would have changed.
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
const quad = (id, x0, y0, x1, y1, extra) => ({ stableTarget: { contextId: 'scene-root', layerUid: 'L', frameScope: { kind: 'authored', frame: 0 }, strokeId: id },
  closed: true, count: 4, role: 'plain', filled: true, bounds: [x0, y0, x1, y1], matrix: [1, 0, 0, 1, 0, 0], segments: [seg(x0, y0), seg(x1, y0), seg(x1, y1), seg(x0, y1)],
  style: { fill: '#112233', stroke: '#445566', width: 4, opacity: 1 }, ...extra });
const ribbon = (id, x0, y0) => quad(id, x0, y0, x0 + 80, y0 + 20, { role: 'ribbon', style: { fill: '#101010', width: 8, opacity: 1 },
  center: [{ point: [x0, y0 + 10], handleIn: [0, 0], handleOut: [0, 0], width: 4 }, { point: [x0 + 40, y0 + 10], handleIn: [0, 0], handleOut: [0, 0], width: 8 }, { point: [x0 + 80, y0 + 10], handleIn: [0, 0], handleOut: [0, 0], width: 4 }] });
const image = (id, x0, y0) => quad(id, x0, y0, x0 + 20, y0 + 20, { role: 'raster', style: { opacity: 1 } });
const tick = () => new Promise((done) => setTimeout(done, 0));
const settle = async () => { for (let i = 0; i < 6; i++) await tick(); };

function harness(items) {
  const paper = Paper.load(), toasts = [], batches = [], timers = [];
  const pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's1', canvasW: 400, canvasH: 400, layers: [{ layerUid: 'L' }] };
  const store = items.map((i) => JSON.parse(JSON.stringify(i)));
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, readPresentedObjectSession: () => ({ presentation: { ...pin, frame: 0 } }),
    async readPresentedStrokes(_selector, _pin, wanted) { return { items: store.map((item) => { const { segments, ...rest } = item; return wanted(rest) ? { ...rest, segments } : rest; }) }; } };
  const root = { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool: 'select', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, layers: [{ layerUid: 'L' }], selectedStrokeIndices: [] },
    selectedPaths: [], updateUI() {}, SMEngineBridge: { renderNow() {} }, showToast: (t) => toasts.push(t),
    setTimeout: (fn) => { timers.push(fn); return timers.length; }, clearTimeout: (id) => { timers[id - 1] = null; },
    // The baseline's centreline fit, stubbed to the two end anchors with the widest width kept.
    buildCenterSegmentsFromRawStroke: (pts, widths) => [{ point: [pts[0].x, pts[0].y], handleIn: [0, 0], handleOut: [3, 0], width: widths[0] }, { point: [pts[pts.length - 1].x, pts[pts.length - 1].y], handleIn: [-3, 0], handleOut: [0, 0], width: widths[widths.length - 1] }],
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: () => true },
    NemoNativeObjectLabs: { desktop: () => ({ batchStrokes: async (layerUid, frame, steps) => { batches.push({ layerUid, frame, steps: JSON.parse(JSON.stringify(steps)) }); return { status: 'applied' }; } }) },
    NemoSelectCanvasIntent: Intent, NemoNativeSelectionSteps: Steps, NemoNativeSelectOverlay: overlayStub(Intent) };
  return { root, toasts, batches, timers,
    async select(ids) { const layers = await Intent.layerPaths(root, (item) => ids.includes(item.stableTarget.strokeId), false, 0); root.selectedPaths = layers.paths; root.NemoNativeSelectOverlay.show(root); },
    fire: async () => { const fns = timers.splice(0).filter(Boolean); fns.forEach((fn) => fn()); await settle(); } };
}
const asOps = (steps, frame = 0) => steps.map((s) => (s.restyle ? { op: 'restyle', stableTarget: s.restyle.stableTarget, style: s.restyle.style, frame }
  : { op: 'reshape', stableTarget: s.reshape.stableTarget, ...(s.reshape.geometry ? { geometry: s.reshape.geometry } : { center: s.reshape.center }), frame }));
const accepted = (batch) => Contract.payload(Contract.VECTOR, { command: 'object.vector.batch', ops: asOps(batch.steps) });
const byId = (batch) => Object.fromEntries(batch.steps.map((s) => [(s.restyle || s.reshape).stableTarget.strokeId, s.restyle ? s.restyle.style : s.reshape]));

test('the paint switches carry the colour they turn on with; an image and (for the fill) a ribbon take no step', async () => {
  const h = harness([quad('A', 10, 10, 30, 30), quad('B', 40, 40, 60, 60), ribbon('R', 100, 100), image('I', 200, 200)]);
  await h.select(['A', 'B', 'R', 'I']);
  Style.apply(h.root, { fillEnabled: true, fillWith: '#00ff0080' });
  await h.fire();
  assert.equal(h.batches.length, 1, 'one batch for the selection');
  assert.deepEqual(byId(h.batches[0]), { A: { fillEnabled: true, fill: '#00ff0080' }, B: { fillEnabled: true, fill: '#00ff0080' } }, 'a ribbon has no fill switch, an image no paint');
  accepted(h.batches[0]);
  Style.apply(h.root, { fillEnabled: false });
  await h.fire();
  assert.deepEqual(byId(h.batches[1]), { A: { fillEnabled: false }, B: { fillEnabled: false } });
  Style.apply(h.root, { strokeEnabled: true, strokeWith: '#ff0000' });
  await h.fire();
  assert.deepEqual(byId(h.batches[2]), { A: { strokeEnabled: true, stroke: '#ff0000' }, B: { strokeEnabled: true, stroke: '#ff0000' }, R: { strokeEnabled: true, stroke: '#ff0000' } }, 'a ribbon\'s stroke switch is its ink');
  accepted(h.batches[2]);
  Style.apply(h.root, { strokeEnabled: false });
  await h.fire();
  assert.deepEqual(byId(h.batches[3]), { A: { strokeEnabled: false }, B: { strokeEnabled: false }, R: { strokeEnabled: false } });
});

test('a colour picked with the switch in one gesture wins over the colour the switch would use, and reaches a ribbon as its ink', async () => {
  const h = harness([quad('A', 10, 10, 30, 30), ribbon('R', 100, 100)]);
  await h.select(['A', 'R']);
  Style.apply(h.root, { fill: '#123456' });
  Style.apply(h.root, { fillEnabled: true, fillWith: '#123456' });
  await h.fire();
  assert.equal(h.batches.length, 1);
  assert.deepEqual(byId(h.batches[0]), { A: { fill: '#123456', fillEnabled: true }, R: { fill: '#123456' } });
  accepted(h.batches[0]);
});

test('cap, join, miter, dash offset and paint order reach plain records and a ribbon takes only its paint order, in one batch', async () => {
  const h = harness([quad('A', 10, 10, 30, 30), ribbon('R', 100, 100)]);
  await h.select(['A', 'R']);
  for (const edit of [{ cap: 'square' }, { join: 'bevel' }, { miter: 4 }, { dashOffset: 2.5 }, { paintOrder: 'strokeFirst' }]) Style.apply(h.root, edit);
  await h.fire();
  assert.equal(h.batches.length, 1);
  assert.deepEqual(byId(h.batches[0]), { A: { cap: 'square', join: 'bevel', miter: 4, dashOffset: 2.5, paintOrder: 'strokeFirst' }, R: { paintOrder: 'strokeFirst' } });
  accepted(h.batches[0]);
  Style.apply(h.root, { cap: 'round' });
  await h.fire();
  assert.deepEqual(byId(h.batches[1]), { A: { cap: 'round' } }, 'a ribbon with nothing to take is not a step');
});

test('the stroke style rewrites the dash from the width and carries the tool\'s offset, miter and paint order as applyStrokeStyle did', async () => {
  const h = harness([quad('A', 10, 10, 30, 30)]);
  await h.select(['A']);
  Style.apply(h.root, { strokeStyle: 'dashed', dashOffset: 0, miter: 10, paintOrder: 'fillFirst' });
  await h.fire();
  assert.deepEqual(byId(h.batches[0]), { A: { strokeStyle: 'dashed', dashOffset: 0, miter: 10, paintOrder: 'fillFirst' } });
  accepted(h.batches[0]);
});

test('swapping the paints is one edit: a plain record takes both colours, a ribbon the stroke colour (the later write)', async () => {
  const h = harness([quad('A', 10, 10, 30, 30), ribbon('R', 100, 100)]);
  await h.select(['A', 'R']);
  Style.apply(h.root, { fill: '#445566', stroke: '#112233' });
  await h.fire();
  assert.equal(h.batches.length, 1);
  assert.deepEqual(byId(h.batches[0]), { A: { fill: '#445566', stroke: '#112233' }, R: { stroke: '#112233' } });
  accepted(h.batches[0]);
});

test('a brush preset is an edit of its own: the pending edit commits first, a ribbon and an image are skipped, and it is not merged', async () => {
  const h = harness([quad('A', 10, 10, 30, 30), ribbon('R', 100, 100), image('I', 200, 200)]);
  await h.select(['A', 'R', 'I']);
  Style.apply(h.root, { opacity: 0.5 });
  const picked = Style.apply(h.root, { brushPreset: 'charcoal-pencil' });
  Style.apply(h.root, { fill: '#ff0000' });
  await h.fire();
  await picked;
  await h.fire();
  assert.equal(h.batches.length, 3, 'opacity, then the preset, then the colour: three edits');
  assert.deepEqual(byId(h.batches[0]), { A: { opacity: 0.5 }, R: { opacity: 0.5 }, I: { opacity: 0.5 } });
  assert.deepEqual(byId(h.batches[1]), { A: { brushPreset: 'charcoal-pencil' } });
  accepted(h.batches[1]);
  assert.deepEqual(byId(h.batches[2]), { A: { fill: '#ff0000' }, R: { fill: '#ff0000' } });
  assert.deepEqual(await Style.apply(h.root, { brushPreset: '' }), { status: 'invalid' });
});

test('Smooth reshapes a plain path by its simplified geometry and a ribbon by its re-fitted centreline, as one batch, merging a scrub', async () => {
  const wavy = quad('W', 10, 10, 90, 30, { closed: false, count: 5, filled: false, segments: [seg(10, 10), seg(30, 14), seg(50, 8), seg(70, 13), seg(90, 10)] });
  const h = harness([wavy, ribbon('R', 100, 100), image('I', 200, 200)]);
  await h.select(['W', 'R', 'I']);
  for (const amount of [4, 8, 20]) Style.apply(h.root, { smooth: amount }); // a scrub on the field
  await h.fire();
  assert.equal(h.batches.length, 1, 'one batch however many ticks');
  const [batch] = h.batches;
  const steps = byId(batch);
  assert.deepEqual(Object.keys(steps).sort(), ['R', 'W'], 'an image has no geometry to smooth');
  assert.equal(steps.W.geometry.kind, 'cubic-path');
  assert.ok(steps.W.geometry.segments.length >= 2 && steps.W.geometry.segments.length <= 5, 'simplified');
  assert.equal(steps.R.center.length, 2, 'the centreline was re-fitted');
  assert.deepEqual(steps.R.center.map((c) => c.width), [4, 4], 'record widths follow the fit');
  accepted(batch);
  const other = await Style.apply(h.root, { smooth: -1 });
  assert.deepEqual(other, { status: 'invalid' });
});

test('an edit with a malformed member is refused before anything is sent', async () => {
  const h = harness([quad('A', 10, 10, 30, 30)]);
  await h.select(['A']);
  for (const bad of [{ cap: 'flat' }, { join: 'sharp' }, { miter: 0.2 }, { paintOrder: 'x' }, { strokeStyle: 'wavy' }, { fillEnabled: 'yes' }, { fillWith: 'red' }, { brushPreset: 3 }, { smooth: 'a' }, {}]) {
    assert.deepEqual(await Style.apply(h.root, bad), { status: 'invalid' }, JSON.stringify(bad));
  }
  assert.equal(h.batches.length, 0);
});

test('the vector contract admits the new restyle members and refuses malformed ones, a colourless switch and a preset beside other members', () => {
  const target = quad('A', 0, 0, 1, 1).stableTarget;
  const step = (style) => ({ op: 'restyle', stableTarget: target, style, frame: 2 });
  const run = (style) => Contract.payload(Contract.VECTOR, { command: 'object.vector.batch', ops: [step(style)] });
  for (const good of [{ fillEnabled: true, fill: '#00ff00' }, { fillEnabled: false }, { strokeEnabled: true, stroke: '#00ff0080' }, { cap: 'butt', join: 'miter', miter: 1, dashOffset: -3, paintOrder: 'fillFirst', strokeStyle: 'dotted' },
    { brushPreset: 'none' }, { maskFeather: 12.5 }]) run(good);
  for (const bad of [{ fillEnabled: true }, { strokeEnabled: true }, { fillEnabled: 1, fill: '#00ff00' }, { cap: 'flat' }, { join: 'sharp' }, { miter: 0.5 }, { miter: 5000 }, { dashOffset: 1e9 }, { paintOrder: 'x' },
    { strokeStyle: 'wavy' }, { brushPreset: '' }, { brushPreset: 7 }, { maskFeather: 501 }, { brushPreset: 'chalk-round', cap: 'round' }, { maskFeather: 3, fill: '#ff0000' }, { cap: null }]) {
    assert.throws(() => run(bad), TypeError, JSON.stringify(bad));
  }
});

test('the hit-test listing admits the stroke details and refuses malformed ones', () => {
  const target = quad('A', 0, 0, 1, 1).stableTarget;
  const result = (style) => ({ kind: 'subselect', contextId: 'scene-root', layerUid: 'L', frame: 0, workId: 'native-work-1', viewGeneration: 1, cursor: 0, hitTest: true, atRevision: 7, documentSnapshotId: 'native-object:document:7', total: 1, nextCursor: null, preview: false,
    items: [{ stableTarget: target, closed: true, count: 4, matrix: [1, 0, 0, 1, 0, 0], bounds: [0, 0, 1, 1], role: 'plain', filled: true, style }] });
  Subselect.viewResult(result({ fill: '#112233', cap: 'square', join: 'bevel', miter: 4, dashOffset: -2.5, paintOrder: 'strokeFirst' }));
  for (const bad of [{ cap: 'flat' }, { join: 'sharp' }, { miter: -1 }, { dashOffset: 'x' }, { paintOrder: 'x' }]) assert.throws(() => Subselect.viewResult(result(bad)), TypeError, JSON.stringify(bad));
});

// The panel code in timeline.js (not loadable in isolation): each setter hands a native selection to the adapter before any legacy gate.
const TIMELINE = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'timeline.js'), 'utf8');
const head = (marker) => { const at = TIMELINE.indexOf(marker); assert.ok(at >= 0, marker); const rest = TIMELINE.slice(at); return rest.slice(0, rest.indexOf('n20AllowLegacyWrite')); };
test('every selection-style setter offers a native selection to the adapter before its legacy gate', () => {
  for (const [marker, call] of [['  setFillEnabled:function(v){', 'nativeSelStyle({fillEnabled:true,fillWith:state.fillColor})'], ['  setStrokeEnabled:function(v){', 'nativeSelStyle({strokeEnabled:true,strokeWith:state.strokeColor})'],
    ['  swapStrokeFill:function(){', 'nativeSelStyle({fill:sw,stroke:fw})'], ['  setStrokeCap:function(v){', 'nativeSelStyle({cap:v})'], ['  setStrokeJoin:function(v){', 'nativeSelStyle({join:v})'],
    ['  setMiterLimit:function(v){', 'nativeSelStyle({miter:'], ['  setPaintOrder:function(v){', 'nativeSelStyle({paintOrder:v})'], ['  setDashOffset:function(v){', 'nativeSelStyle({dashOffset:'],
    ['  setStrokeStyle:function(v){', 'nativeSelStyle({strokeStyle:v,'], ['  smoothSelectedStroke:function(amount){', 'nativeSelStyle({smooth:'], ['  setBrushPreset:function(v){', 'nativeSelStyle({brushPreset:']]) {
    assert.ok(head(marker).includes(call), marker + ' hands a native selection over first');
  }
  assert.ok(TIMELINE.includes('strokeCap:st.cap,strokeJoin:st.join,miterLimit:st.miter,dashOffset:st.dashOffset'), 'the panel shows the listed stroke details');
});

test('the kinds, the mask feather included, are declared native for the vector capability', () => {
  const doc = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'engineering', 'application', 'legacy-writer-families.json'), 'utf8'));
  const family = (name) => doc.families.find((f) => f.family === name);
  const native = family('selection-style-native');
  assert.equal(native.authority, 'native');
  assert.equal(native.capability, 'native-object-vector');
  for (const kind of ['fill-enabled', 'stroke-enabled', 'stroke-cap', 'stroke-join', 'miter-limit', 'paint-order', 'dash-offset', 'stroke-style', 'swap-paint', 'brush-preset', 'smooth-stroke']) {
    assert.ok(native.kinds.includes('timeline-selection-' + kind), kind);
  }
  assert.ok(native.kinds.includes('timeline-mask-feather'), 'the mask feather restyles natively (#2372 SEL-MASK)');
  assert.equal(family('selection-style'), undefined, 'no selection style kind is left unavailable');
});
