'use strict';
// #2372 SEL-STYLE: the right panel restyles the Select selection of a native object project as ONE native object.vector.batch of `restyle`
// steps (hex8 colours, width, opacity), committed once when the edits stop, and shows the style the engine stored.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
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
const tick = () => new Promise((done) => setTimeout(done, 0));
const settle = async () => { for (let i = 0; i < 6; i++) await tick(); };

function harness(items, options = {}) {
  const paper = Paper.load(), toasts = [], batches = [], options_ = [], timers = [];
  let pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's1', canvasW: 200, canvasH: 200, layers: [{ layerUid: 'L' }] };
  const store = items.map((i) => JSON.parse(JSON.stringify(i)));
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, readPresentedObjectSession: () => ({ presentation: { ...pin, frame: 0 } }),
    async readPresentedStrokes(_selector, _pin, wanted) { return { items: store.map((item) => { const { segments, ...rest } = item; return wanted(rest) ? { ...rest, segments } : rest; }) }; } };
  const root = { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool: 'select', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, layers: [{ layerUid: 'L' }], selectedStrokeIndices: [] },
    selectedPaths: [], updateUI() {}, SMEngineBridge: { renderNow() {} }, showToast: (t) => toasts.push(t),
    setTimeout: (fn) => { timers.push(fn); return timers.length; }, clearTimeout: (id) => { timers[id - 1] = null; },
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: () => true },
    NemoNativeObjectLabs: { desktop: () => ({ batchStrokes: async (layerUid, frame, steps, sent) => {
      options_.push(sent);
      batches.push({ layerUid, frame, steps: JSON.parse(JSON.stringify(steps)) });
      if (options.reply) return options.reply;
      steps.forEach((s) => { const it = store.find((i) => i.stableTarget.strokeId === s.restyle.stableTarget.strokeId); it.style = { ...it.style, ...s.restyle.style }; });
      pin = { ...pin, contentRevision: pin.contentRevision + 1, documentSnapshotId: 's' + (pin.contentRevision + 1) };
      return { status: 'applied' };
    } }) },
    NemoSelectCanvasIntent: Intent, NemoNativeSelectionSteps: Steps, NemoNativeSelectOverlay: overlayStub(Intent) };
  const h = { root, toasts, batches, sentOptions: options_, store, timers,
    async select(ids) { const layers = await Intent.layerPaths(root, (item) => ids.includes(item.stableTarget.strokeId), false, 0); root.selectedPaths = layers.paths; root.NemoNativeSelectOverlay.show(root); },
    // The quiet timer of the pending edit fires.
    fire: async () => { const fns = timers.splice(0).filter(Boolean); fns.forEach((fn) => fn()); await settle(); },
    setPin: (n) => { pin = { ...pin, ...n }; } };
  return h;
}

test('a fill colour with alpha on three selected items is ONE native batch of hex8 restyle steps, sent when the edits stop', async () => {
  const h = harness([quad('A', 10, 10, 30, 30), quad('B', 40, 40, 60, 60), quad('C', 70, 70, 90, 90)]);
  await h.select(['A', 'B', 'C']);
  assert.equal(Style.handles(h.root), true);
  const first = Style.apply(h.root, { fill: '#ff000040' }); // a colour picker fires on every move
  Style.apply(h.root, { fill: '#ff000080' });
  const last = Style.apply(h.root, { fill: '#ff0000cc' });
  await settle();
  assert.equal(h.batches.length, 0, 'nothing is written while the colour is still being picked');
  await h.fire();
  assert.deepEqual(await Promise.all([first, last]), [{ status: 'applied' }, { status: 'applied' }]);
  assert.equal(h.batches.length, 1, 'one batch for the whole gesture and the whole selection');
  assert.equal(h.batches[0].layerUid, 'L');
  assert.equal(h.batches[0].frame, 0);
  assert.deepEqual(h.batches[0].steps.map((s) => s.restyle.style), [{ fill: '#ff0000cc' }, { fill: '#ff0000cc' }, { fill: '#ff0000cc' }], 'the alpha byte is kept');
  assert.deepEqual(h.batches[0].steps.map((s) => s.restyle.stableTarget.strokeId), ['A', 'B', 'C']);
  Contract.payload(Contract.VECTOR, { command: 'object.vector.batch', ops: h.batches[0].steps.map((s) => ({ op: 'restyle', stableTarget: s.restyle.stableTarget, style: s.restyle.style, frame: 0 })) });
});

test('different members edited before the commit travel in the same batch; a new selection flushes the old edit first', async () => {
  const h = harness([quad('A', 10, 10, 30, 30), quad('B', 40, 40, 60, 60)]);
  await h.select(['A']);
  Style.apply(h.root, { stroke: '#00ff00' });
  Style.apply(h.root, { width: 7, opacity: 0.4 });
  await h.select(['B']);
  const second = Style.apply(h.root, { fill: '#0000ff' });
  await settle();
  assert.equal(h.batches.length, 1, 'the first selection committed before the second began');
  assert.deepEqual(h.batches[0].steps.map((s) => s.restyle.style), [{ stroke: '#00ff00', width: 7, opacity: 0.4 }]);
  await h.fire();
  await second;
  assert.equal(h.batches.length, 2);
  assert.deepEqual(h.batches[1].steps.map((s) => [s.restyle.stableTarget.strokeId, s.restyle.style]), [['B', { fill: '#0000ff' }]]);
});

test('a brush size on a selected ribbon is one commit and the panel then shows the stored width', async () => {
  const ribbon = quad('R', 10, 10, 90, 30, { role: 'ribbon', filled: true, style: { fill: '#101010', width: 8, opacity: 1 } });
  const h = harness([ribbon]);
  await h.select(['R']);
  assert.deepEqual(Style.shown(h.root.selectedPaths[0]), { role: 'ribbon', fill: '#101010', width: 8, opacity: 1 }, 'the panel reads the listed style, not placeholder paint');
  for (const width of [10, 14, 20]) Style.apply(h.root, { width }); // a scrub
  await h.fire();
  assert.equal(h.batches.length, 1);
  assert.deepEqual(h.batches[0].steps, [{ restyle: { stableTarget: ribbon.stableTarget, style: { width: 20 } } }]);
  assert.equal(Style.shown(h.root.selectedPaths[0]).width, 20, 'the selection was read again after the commit');
  assert.deepEqual(h.root.selectedPaths.map((p) => p.data.strokeId), ['R']);
});

test('a scrub in flight commits when it is released, not while it runs', async () => {
  const h = harness([quad('A', 10, 10, 30, 30)]);
  await h.select(['A']);
  h.root._scrubLiveActive = true;
  const done = Style.apply(h.root, { width: 12 });
  await h.fire();
  await h.fire();
  assert.equal(h.batches.length, 0, 'the timer re-arms while the scrub is live');
  h.root._scrubLiveActive = false;
  await h.fire();
  await done;
  assert.equal(h.batches.length, 1);
  assert.deepEqual(h.batches[0].steps[0].restyle.style, { width: 12 });
});

test('a selection that changed before the commit restyles nothing and says so; a refusal keeps the selection and reports it', async () => {
  const h = harness([quad('A', 10, 10, 30, 30), quad('B', 40, 40, 60, 60)]);
  await h.select(['A']);
  Style.apply(h.root, { fill: '#ff0000' });
  h.setPin({ contentRevision: 9, documentSnapshotId: 'other' });
  await h.fire();
  assert.equal(h.batches.length, 0);
  assert.match(h.toasts.join('|'), /selection changed/);
  const refused = harness([quad('A', 10, 10, 30, 30)], { reply: { status: 'unavailable', reason: 'brush' } });
  await refused.select(['A']);
  Style.apply(refused.root, { fill: '#ff0000' });
  await refused.fire();
  assert.equal(refused.batches.length, 1);
  assert.match(refused.toasts.join('|'), /could not be applied to this kind of item/);
  assert.deepEqual(refused.root.selectedPaths.map((p) => p.data.strokeId), ['A'], 'the selection stays');
});

test('bitmap brushes and dabs are skipped with a notice while the rest of the selection is restyled; nothing selectable means unavailable', async () => {
  const h = harness([quad('A', 10, 10, 30, 30), quad('M', 40, 40, 60, 60, { role: 'brush', brush: 'bitmap', group: 'g' })]);
  await h.select(['A', 'M']);
  Style.apply(h.root, { opacity: 0.5 });
  await h.fire();
  assert.deepEqual(h.batches[0].steps.map((s) => s.restyle.stableTarget.strokeId), ['A']);
  assert.match(h.toasts.join('|'), /bitmap brushes/);
  const none = harness([quad('A', 10, 10, 30, 30)]);
  assert.equal(Style.handles(none.root), false, 'no selection: the tool defaults change instead');
  assert.deepEqual(await Style.apply(none.root, { fill: '#000000' }), { status: 'unavailable' });
  assert.deepEqual(await Style.apply(none.root, {}), { status: 'invalid' });
});

test('a selection on a non-native project is not handled, so the legacy path stays as it was', () => {
  const h = harness([quad('A', 10, 10, 30, 30)]);
  h.root.NemoNativeObjectLabs = { desktop: () => null };
  h.root.selectedPaths = [{ data: { nativeStableTarget: quad('A', 0, 0, 1, 1).stableTarget, nativeLayerIndex: 0, strokeId: 'A' } }];
  assert.equal(Style.handles(h.root), false);
});

test('batchStrokes maps a restyle step to the closed engine step, and the contract refuses malformed styles', async () => {
  // The vector adapter over a stub command channel: what it builds is what the engine receives.
  const sent = [], dir = path.join(__dirname, '..', 'src', 'js', 'adapters'), mod = { exports: {} };
  const Command = { fault: (code, message, extra) => Object.assign(new Error(message), { code }, extra),
    create: () => ({ execute: async (_op, build, map) => { sent.push(build({ instanceId: 'i', documentId: 'd', contentRevision: 1, documentSnapshotId: 's' })); return map({ applied: true, requestId: 'r', contentRevision: 2, strokeId: 'A' }); } }) };
  const require_ = (name) => (name === './native-object-vector-command.js' ? Command : require(path.join(dir, name)));
  new Function('require', 'module', fs.readFileSync(path.join(dir, 'native-object-vector.js'), 'utf8'))(require_, mod);
  const client = mod.exports.create({});
  const target = quad('A', 0, 0, 1, 1).stableTarget;
  const style = { fill: '#ff000080', width: 6 };
  const result = await client.batchStrokes('L', 3, [{ restyle: { stableTarget: target, style } }, { restyle: { stableTarget: target, style: { opacity: 0 } }, frame: 5 }]);
  assert.equal(result.status, 'applied', JSON.stringify(result));
  assert.deepEqual(sent[0].ops, [{ op: 'restyle', stableTarget: target, style, frame: 3 }, { op: 'restyle', stableTarget: target, style: { opacity: 0 }, frame: 5 }]);
  assert.equal('skipUnavailable' in sent[0], false, 'the batch is atomic unless asked');
  await client.batchStrokes('L', 3, [{ restyle: { stableTarget: target, style } }], { skipUnavailable: true });
  await client.batchStrokes('L', 3, [{ restyle: { stableTarget: target, style } }], { skipUnavailable: 'yes' });
  assert.deepEqual([sent[1].skipUnavailable, 'skipUnavailable' in sent[2]], [true, false], 'only a literal true opts in');
  const step = (style) => ({ op: 'restyle', stableTarget: target, style, frame: 2 });
  Contract.payload(Contract.VECTOR, { command: 'object.vector.batch', ops: [step({ fill: '#ff000080', stroke: '#00ff00', width: 6, opacity: 0.4 }), step({ opacity: 0 })] });
  for (const bad of [{}, { fill: 'red' }, { fill: '#12' }, { fill: null }, { width: 0 }, { width: 20000 }, { opacity: 1.5 }, { colour: '#000000' }]) {
    assert.throws(() => Contract.payload(Contract.VECTOR, { command: 'object.vector.batch', ops: [step(bad)] }), TypeError, JSON.stringify(bad));
  }
  assert.throws(() => Contract.payload(Contract.VECTOR, { command: 'object.vector.batch', ops: [{ op: 'restyle', stableTarget: target, frame: 0 }] }));
  assert.throws(() => Contract.payload(Contract.VECTOR, { command: 'object.vector.batch', ops: [{ ...step({ width: 2 }), matrix: [1, 0, 0, 1, 0, 0] }] }), 'a restyle takes no matrix');
});

test('the hit-test listing contract admits the optional style and refuses a malformed one', () => {
  const target = quad('A', 0, 0, 1, 1).stableTarget;
  const result = (style) => ({ kind: 'subselect', contextId: 'scene-root', layerUid: 'L', frame: 0, workId: 'native-work-1', viewGeneration: 1, cursor: 0, hitTest: true, atRevision: 7, documentSnapshotId: 'native-object:document:7', total: 1, nextCursor: null, preview: false,
    items: [{ stableTarget: target, closed: true, count: 4, matrix: [1, 0, 0, 1, 0, 0], bounds: [0, 0, 1, 1], role: 'plain', filled: true, ...(style === undefined ? {} : { style }) }] });
  for (const good of [undefined, { fill: '#112233' }, { fill: '#11223380', stroke: '#445566', width: 4, opacity: 0.5 }, { opacity: 0 }, { strokeStyle: 'dashed', brushPreset: 'chalk-round' }]) Subselect.viewResult(result(good));
  for (const bad of [{}, { fill: 'red' }, { fill: null }, { width: 0 }, { opacity: 2 }, { colour: '#000000' }, '#000000', { strokeStyle: 'wavy' }, { strokeStyle: null }, { brushPreset: '' }, { brushPreset: 7 }]) {
    assert.throws(() => Subselect.viewResult(result(bad)), TypeError, JSON.stringify(bad));
  }
});

// The panel code in timeline.js (not loadable in isolation): the four setters hand a native selection to the adapter before any legacy gate,
// and the properties panel reads the listed style through nativeStyleView.
const TIMELINE = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'timeline.js'), 'utf8');
function extract(name) {
  const from = TIMELINE.indexOf('function ' + name + '(');
  assert.ok(from >= 0, name);
  let depth = 0, at = TIMELINE.indexOf('{', from);
  for (let i = at; i < TIMELINE.length; i++) { if (TIMELINE[i] === '{') depth++; else if (TIMELINE[i] === '}' && --depth === 0) return TIMELINE.slice(from, i + 1); }
  throw new Error('unterminated ' + name);
}
test('the panel setters route a native selection to the adapter first and the panel shows the listed style', async () => {
  const calls = [];
  class Color { constructor(v) { this.value = v; } }
  const sandbox = { window: { NemoNativeSelectionStyle: { handles: () => true, apply: (_r, style) => calls.push(style), shown: Style.shown } }, Color, Object, Number, isFinite, RegExp };
  vm.createContext(sandbox);
  vm.runInContext(['nativeSelStyle', 'nativeHex', 'nativeStyleView'].map(extract).join('\n') + '\nthis.f={nativeSelStyle,nativeHex,nativeStyleView};', sandbox);
  assert.equal(sandbox.f.nativeSelStyle({ width: 9 }), true);
  assert.deepEqual(calls, [{ width: 9 }]);
  assert.equal(sandbox.f.nativeHex('#ff000080'), '#ff000080');
  assert.equal(sandbox.f.nativeHex('rgb(1,2,3)'), null);
  const path = { id: 4, data: { strokeId: 'R', nativeRole: 'ribbon', nativeStyle: { fill: '#10101080', width: 8, opacity: 0.5 } } };
  const view = sandbox.f.nativeStyleView(path);
  assert.equal(JSON.stringify([view.fillColor.value, view.strokeColor, view.strokeWidth, view.data.isVectorBrush, view.data.widthProfile]), JSON.stringify(['#10101080', null, 8, true, [{ width: 8 }]]));
  const plain = sandbox.f.nativeStyleView({ id: 5, data: { strokeId: 'P', nativeRole: 'plain', nativeStyle: { fill: '#112233', stroke: '#445566', width: 4, opacity: 1 } } });
  assert.deepEqual([plain.fillColor.value, plain.strokeColor.value, plain.strokeWidth, plain.nativeOpacity], ['#112233', '#445566', 4, 1]);
  const bare = { id: 6, data: {} };
  assert.equal(sandbox.f.nativeStyleView(bare), bare, 'a path with no listed style is read as it was');
  sandbox.window.NemoNativeSelectionStyle.handles = () => false;
  assert.equal(sandbox.f.nativeSelStyle({ width: 9 }), false, 'a legacy selection is not handed over');
  for (const [setter, first] of [['setBrushSize', 'nativeSelStyle({width:v})'], ['setStrokeColor', 'nativeSelStyle({stroke:v})'], ['setFillColor', 'nativeSelStyle({fill:v})'], ['setOpacity', 'nativeSelStyle({opacity:']]) {
    const body = TIMELINE.slice(TIMELINE.indexOf('  ' + setter + ':function(v){'));
    const head = body.slice(0, body.indexOf('n20AllowLegacyWrite'));
    assert.ok(head.includes(first), setter + ' hands a native selection over before the legacy gate');
  }
  assert.ok(TIMELINE.includes('var ref=nativeStyleView(selectedPaths[0]);'), 'the panel reads the selection through the listed style');
});

test('the Stroke panel shows the listed Stroke Style and brush preset of the selection without ever writing the draw defaults (#2372 INSP-READBACK)', () => {
  class Color { constructor(v) { this.value = v; } }
  const elements = { 'p-strokestyle': { value: 'solid' } }, painted = [];
  const sandbox = { window: { NemoNativeSelectionStyle: { handles: () => true, shown: Style.shown }, BrushPresetPicker: { paintButton: (k) => painted.push(k) } }, Color, Object, state: { strokeStyle: 'solid', brushPreset: 'none' },
    document: { getElementById: (id) => elements[id] || null } };
  vm.createContext(sandbox);
  vm.runInContext('var _styleReadbackShown=false;\n' + ['nativeStyleView', 'nativeStyleReadback', 'nativeStyleReadbackEnd'].map(extract).join('\n') + '\nthis.f={nativeStyleView,nativeStyleReadback,nativeStyleReadbackEnd};', sandbox);
  const view = (role, style) => sandbox.f.nativeStyleView({ id: 1, data: { strokeId: 'S', nativeRole: role, nativeStyle: style } });
  const defaults = () => [sandbox.state.strokeStyle, sandbox.state.brushPreset];
  // A dashed plain stroke: the control follows it; the draw defaults (state) are never written; no preset listed reads as None.
  sandbox.f.nativeStyleReadback(view('plain', { stroke: '#445566', width: 4, opacity: 1, strokeStyle: 'dashed' }));
  assert.deepEqual([elements['p-strokestyle'].value, defaults(), painted], ['dashed', ['solid', 'none'], ['none']]);
  // A texture anchor: its preset is shown, the style of its centreline too.
  sandbox.f.nativeStyleReadback(view('brush', { width: 4, opacity: 1, strokeStyle: 'dotted', brushPreset: 'chalk-round' }));
  assert.deepEqual([elements['p-strokestyle'].value, painted[1], defaults()], ['dotted', 'chalk-round', ['solid', 'none']]);
  // A hand-made dash lists no style: the control keeps what it showed. A ribbon (or an image) has no preset to show.
  sandbox.f.nativeStyleReadback(view('plain', { stroke: '#445566', width: 4, opacity: 1 }));
  assert.equal(elements['p-strokestyle'].value, 'dotted');
  assert.equal(view('ribbon', { fill: '#101010', width: 8, opacity: 1, brushPreset: 'chalk-round' }).brushPreset, undefined, 'a ribbon is not eligible for a preset');
  assert.equal(view('raster', { opacity: 1 }).brushPreset, undefined);
  // Select is left (tool change, Motion / Anim mode switch, selection cleared): the controls repaint from the untouched defaults, once.
  sandbox.f.nativeStyleReadbackEnd();
  assert.deepEqual([elements['p-strokestyle'].value, painted.slice(3), defaults()], ['solid', ['none'], ['solid', 'none']], 'the draw defaults are back on screen and in state');
  sandbox.f.nativeStyleReadbackEnd();
  assert.equal(painted.length, 4, 'nothing repaints when no readback was shown');
  // A legacy path (no listed style) is read as it was and starts no readback.
  elements['p-strokestyle'].value = 'dashed';
  sandbox.f.nativeStyleReadback({ id: 2, data: {}, strokeStyle: 'dotted' });
  sandbox.f.nativeStyleReadbackEnd();
  assert.deepEqual([defaults(), elements['p-strokestyle'].value], [['solid', 'none'], 'dashed']);
  // The panel reads the style back where it reads the cap and the dash offset, and EVERY way out of Select ends it.
  assert.ok(TIMELINE.includes('document.getElementById(\'p-dashoffset\').value=state.dashOffset;\n        nativeStyleReadback(ref);'), 'the panel reads the style back where it reads the cap and the dash offset');
  assert.ok(TIMELINE.includes("_selPropsSig='';nativeStyleReadbackEnd();return;"), 'no selection (or another tool) ends the readback on every updateUI, Motion / Anim mode switches included');
  assert.ok(TIMELINE.includes("if(t!=='select'&&t!=='subselect')nativeStyleReadbackEnd();"), 'a tool change ends it at once');
  const body = TIMELINE.slice(TIMELINE.indexOf('function nativeStyleReadback('), TIMELINE.indexOf('function nativeStyleReadbackEnd('));
  assert.ok(!/state\.(strokeStyle|brushPreset)\s*=/.test(body), 'the readback never assigns a drawing default');
});

test('the panel opts its multi-select restyle in to skipping the steps the engine cannot do; the adapter batch is atomic otherwise (#2372)', async () => {
  const h = harness([quad('A', 10, 10, 30, 30), quad('B', 40, 40, 60, 60)]);
  await h.select(['A', 'B']);
  const done = Style.apply(h.root, { stroke: '#00ff00' });
  await h.fire();
  assert.deepEqual(await done, { status: 'applied' });
  assert.equal(h.batches.length, 1);
  assert.deepEqual(h.sentOptions, [{ skipUnavailable: true, select: true }], 'the Select panel sends the explicit opt-in, and marks the batch as Select\'s (the only one a component accepts)');
});

test('the Labs route forwards only a literal skipUnavailable opt-in to the vector batch and always keys held cells (#2372)', async () => {
  const Labs = require('../src/js/adapters/native-object-labs.js'), calls = [];
  const session = { blocksLegacy: () => true, info: () => ({ layers: [] }) };
  const host = { NemoNativeObjectProject: { desktop: () => session }, NemoNativeObjectVector: { create: () => ({ batchStrokes: async (...args) => { calls.push(args[3]); return { status: 'applied' }; } }) } };
  const route = Labs.desktop(host);
  await route.batchStrokes('L', 0, [{}]);
  await route.batchStrokes('L', 0, [{}], { skipUnavailable: true });
  await route.batchStrokes('L', 0, [{}], { skipUnavailable: 1 });
  assert.deepEqual(calls, [{ keyframe: 'copy' }, { keyframe: 'copy', skipUnavailable: true }, { keyframe: 'copy' }]);
});
