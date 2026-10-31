'use strict';
// #2372: the path-effects section of the Effects panel under native authority. The real adapter, vector adapter, command and
// contract run against the vector session double (native-object-vector-support.cjs); the real path-fx.js and the real
// effects-panel.js run in a vm with a fake DOM whose legacy layer rows THROW on any write and whose pushUndo/loadFrame throw on
// a native project. Every control sends the layer's COMPLETE stack as one native edit; the unknown-type refusal is a toast;
// a legacy project is unchanged.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const PathUi = require('../src/js/adapters/native-path-effects-ui.js');
const Vector = require('../src/js/adapters/native-object-vector.js');
const Metadata = require('../src/js/adapters/native-object-project-metadata.js');
const { createFixture, clone } = require('./native-object-vector-support.cjs');

const source = (name) => fs.readFileSync(path.join(__dirname, '../src/js', name), 'utf8');
const OP = 'command.document.object.path.effects.edit';
const ZIG = { type: 'zigzag', enabled: true, p1: 12, p2: 4, p3: 0 };
const ROUGH = { type: 'roughen', enabled: false, p1: 3, p2: null };
const settle = async () => { for (let i = 0; i < 20; i += 1) await new Promise((resolve) => setImmediate(resolve)); };

function fakeElement(name) {
  const el = { name, children: [], events: {}, style: {}, dataset: {}, className: '', textContent: '', value: '', checked: false,
    appendChild(child) { child.parentNode = this; this.children.push(child); return child; },
    addEventListener(kind, fn) { (this.events[kind] = this.events[kind] || []).push(fn); },
    fire(kind, extra) { (this.events[kind] || []).slice().forEach((fn) => fn(Object.assign({ stopPropagation() {}, target: this }, extra))); },
    querySelector(selector) { const m = /option\[value="(.*)"\]/.exec(selector); return m ? this.children.find((c) => c.value === m[1]) || null : null; } };
  Object.defineProperty(el, 'options', { get() { return el.children; } });
  let html = '';
  Object.defineProperty(el, 'innerHTML', { set(value) { html = value; if (value === '') el.children = []; }, get() { return html; } });
  return el;
}

// layers: [{uid, kind, stack?, oversize?}]; `legacy` leaves the project non-native (the baseline path).
function panelHarness(options = {}) {
  const f = createFixture(options);
  const specs = options.layers || [{ uid: 'L1', kind: 'draw', stack: [ZIG, ROUGH] }, { uid: 'L2', kind: 'draw' }, { uid: 'LN', kind: 'null' }];
  specs.forEach((spec) => { if (spec.stack) f.owner.layers.set(spec.uid, clone(spec.stack)); });
  const rows = () => specs.map((spec) => {
    const stack = f.owner.layers.get(spec.uid) || [];
    const row = { layerUid: spec.uid, name: spec.uid, visible: true, kind: spec.kind };
    if (spec.oversize) row.pathFxOversize = true; else if (stack.length) row.pathFx = clone(stack);
    return row;
  });
  const session = Object.assign({}, f.session, { info: () => Object.assign(f.live(), { layers: rows() }), blocksLegacy: () => true });
  const vector = Vector.create(session);
  const nodes = new Map(), toasts = [], calls = [];
  const byId = (id) => { if (!['path-fx-list', 'path-fx-sec', 'path-fx-add'].includes(id)) return null; if (!nodes.has(id)) nodes.set(id, fakeElement(id)); return nodes.get(id); };
  const tripped = (name) => () => { calls.push(name); throw Error(name + ' reached on a native project'); };
  const legacyLayer = { name: 'L1', isNullLayer: false };
  const state = { activeLayerIdx: 0, currentFrame: 2, tool: 'select', layers: [legacyLayer] };
  let native = !options.legacy;
  const context = { state, console, window: null, SM: { t: (k) => k }, showToast: (m) => toasts.push(m),
    pushUndo: () => { calls.push('pushUndo'); if (native) throw Error('pushUndo reached on a native project'); },
    loadFrame: () => { calls.push('loadFrame'); if (native) throw Error('loadFrame reached on a native project'); },
    saveActiveLayerFrame: tripped('saveActiveLayerFrame'),
    document: { getElementById: byId, createElement: (tag) => fakeElement(tag), readyState: 'complete', addEventListener() {}, body: fakeElement('body') },
    NemoNativeObjectProject: { desktop: () => (native ? session : null) },
    NemoNativeObjectVector: { desktop: () => (native ? vector : null) },
    NemoNativePathEffectsUi: PathUi,
    selectedPaths: [], Path: function Path() {}, CompoundPath: function CompoundPath() {} };
  context.window = context;
  context.window.addEventListener = () => {};
  vm.runInNewContext(source('path-fx.js'), context, { filename: 'path-fx.js' });
  vm.runInNewContext(source('effects-panel.js'), context, { filename: 'effects-panel.js' });
  return Object.assign(f, { context, state, toasts, calls, rows, legacyLayer, list: () => byId('path-fx-list'), section: () => byId('path-fx-sec'), add: () => byId('path-fx-add'),
    setNative(on) { native = on; }, sent: () => f.owner.log.received.filter((r) => r.operation === OP) });
}
const entry = (p, at) => p.list().children[at];
const head = (p, at) => entry(p, at).children[0];

test('a native layer lists its stored stack in the baseline markup and the add select carries the known types once', () => {
  const p = panelHarness();
  p.context.updatePathFxPanel();
  assert.equal(p.section().style.display, '');
  assert.equal(p.list().children.length, 2);
  const [eye, name, del] = head(p, 0).children;
  assert.deepEqual([eye.type, eye.checked, name.textContent, del.textContent], ['checkbox', true, 'pathFxZigzag', '×']);
  assert.equal(head(p, 1).children[0].checked, false, 'a stored enabled=false is shown');
  // zigzag has three parameters after its head; a null parameter shows the type default through paramValue
  const params = entry(p, 1).children.slice(1).map((row) => row.children[1]);
  assert.deepEqual(params.map((inp) => inp.value), [3, 6, 0]);
  assert.ok(params.every((inp) => inp.className === 'pi scrub' && inp.dataset.nativeOpacityScrub === 'static' && inp.type === 'number'));
  assert.deepEqual(p.add().children.map((o) => o.value), ['zigzag', 'roughen']);
  p.context.updatePathFxPanel();
  assert.deepEqual(p.add().children.map((o) => o.value), ['zigzag', 'roughen'], 'a second render adds no duplicate option');
  assert.deepEqual(p.calls, []);
});

test('add, toggle, edit and delete each send the layer\'s COMPLETE stack as ONE native edit and redraw from the confirmed row', async () => {
  const p = panelHarness();
  p.context.updatePathFxPanel();
  p.add().value = 'roughen'; p.add().fire('change');
  await settle();
  assert.equal(p.sent().length, 1);
  assert.deepEqual(p.sent()[0].payload, { command: 'object.path.effects.set', layerUid: 'L1', stack: [ZIG, ROUGH, { type: 'roughen', enabled: true, p1: 8, p2: 6, p3: 0 }] });
  assert.equal(p.add().value, '', 'the select returns to its placeholder');
  assert.equal(p.list().children.length, 3, 'the list is redrawn from the confirmed native row');

  const eye = head(p, 0).children[0];
  eye.checked = false; eye.fire('change');
  await settle();
  assert.equal(p.sent().length, 2);
  assert.deepEqual(p.sent()[1].payload.stack.map((e) => e.enabled), [false, false, true]);
  assert.equal(p.sent()[1].payload.stack.length, 3);

  const second = entry(p, 1).children[2].children[1]; // roughen p2 (detail)
  second.value = '20'; second.fire('change');
  await settle();
  assert.equal(p.sent().length, 3);
  assert.deepEqual(p.sent()[2].payload.stack[1], { type: 'roughen', enabled: false, p1: 3, p2: 20 });
  assert.equal(p.sent()[2].payload.stack.length, 3);

  const emptied = entry(p, 1).children[1].children[1];
  emptied.value = ''; emptied.fire('change');
  await settle();
  assert.equal(p.sent()[3].payload.stack[1].p1, 0, 'a cleared field writes 0 like the baseline parseFloat()||0');

  head(p, 0).children[2].fire('click');
  await settle();
  assert.equal(p.sent().length, 5);
  assert.deepEqual(p.sent()[4].payload.stack.map((e) => e.type), ['roughen', 'roughen']);
  assert.deepEqual(p.owner.layers.get('L1').map((e) => e.type), ['roughen', 'roughen']);
  assert.equal(p.list().children.length, 2);
  assert.deepEqual(p.calls, [], 'no legacy undo, load or save was touched');
  assert.deepEqual(p.toasts, []);
  assert.equal(p.legacyLayer.pathFx, undefined, 'the legacy layer model is never written');
});

test('actions run one after the other, each reading the stack the previous one confirmed', async () => {
  const p = panelHarness();
  p.context.updatePathFxPanel();
  const eye = head(p, 0).children[0], del = head(p, 1).children[2];
  eye.checked = false; eye.fire('change');
  del.fire('click'); // the second control belongs to the stack as drawn; it runs after the first confirmed
  await settle();
  assert.equal(p.sent().length, 2);
  assert.deepEqual(p.sent()[1].payload.stack, [{ type: 'zigzag', enabled: false, p1: 12, p2: 4, p3: 0 }], 'the delete kept the first edit');
  assert.equal(p.sent()[1].expectedRevision, p.sent()[0].expectedRevision + 1);
});

test('an unknown effect type is a typed refusal toast and nothing is sent', async () => {
  const p = panelHarness();
  PathUi.panel(p.context).addType('wave');
  await settle();
  assert.equal(p.sent().length, 0);
  assert.equal(p.toasts.length, 1);
  assert.match(p.toasts[0], /^Path effect not available natively: pathFx\[2\] is not a known effect$/);
  assert.deepEqual(p.owner.layers.get('L1'), [ZIG, ROUGH]);
});

test('an owner refusal and an oversize stack each say so and never write the legacy model', async () => {
  const refused = panelHarness({ refuse: () => ({ code: 'stale_revision', message: 'refused' }) });
  refused.context.updatePathFxPanel();
  refused.add().value = 'zigzag'; refused.add().fire('change');
  await settle();
  assert.deepEqual(refused.toasts, ['Native path-effect command was refused.']);
  assert.deepEqual(refused.calls, []);
  const big = panelHarness({ layers: [{ uid: 'L1', kind: 'draw', oversize: true }] });
  big.context.updatePathFxPanel();
  assert.equal(big.list().children.length, 1);
  assert.equal(big.list().children[0].textContent, 'This path-effect stack is too large to edit natively.');
  big.add().value = 'zigzag'; big.add().fire('change');
  await settle();
  assert.equal(big.sent().length, 0);
  assert.deepEqual(big.toasts, ['This path-effect stack is too large to edit natively.']);
});

test('a null layer hides the section; a missing layer row is a toast', async () => {
  const p = panelHarness();
  p.state.activeLayerIdx = 2;
  p.context.updatePathFxPanel();
  assert.equal(p.section().style.display, 'none');
  p.state.activeLayerIdx = 9;
  p.context.updatePathFxPanel();
  assert.equal(p.section().style.display, 'none');
  PathUi.panel(p.context).addType('zigzag');
  await settle();
  assert.deepEqual(p.toasts, ['Select a layer first.']);
  assert.equal(p.sent().length, 0);
});

test('a legacy project keeps the baseline writer and the native adapter stays out of it', async () => {
  const p = panelHarness({ legacy: true });
  p.state.layers[0].pathFx = [clone(ZIG)];
  p.context.updatePathFxPanel();
  assert.equal(p.list().children.length, 1);
  assert.deepEqual(p.add().children.map((o) => o.value), ['zigzag', 'roughen']);
  p.add().value = 'roughen'; p.add().fire('change');
  assert.deepEqual(p.legacyLayer.pathFx.map((e) => e.type), ['zigzag', 'roughen'], 'ld.pathFx is written as in the baseline');
  assert.ok(p.calls.includes('pushUndo'));
  const eye = head(p, 0).children[0];
  eye.checked = false; eye.fire('change');
  assert.equal(p.legacyLayer.pathFx[0].enabled, false);
  await settle();
  assert.equal(p.sent().length, 0);
  assert.equal(PathUi.panel(p.context).on(), false);
  assert.equal(PathUi.panel(p.context).render(), false);
});

test('a select wired by the legacy renderer first does not write ld.pathFx once the project is native, and gets one native edit', async () => {
  const p = panelHarness({ legacy: true });
  p.context.updatePathFxPanel(); // legacy wiring and options
  assert.equal(p.add().events.change.length, 1);
  p.setNative(true);
  p.context.updatePathFxPanel(); // native wiring
  assert.equal(p.add().events.change.length, 2);
  assert.deepEqual(p.add().children.map((o) => o.value), ['zigzag', 'roughen'], 'options are not duplicated');
  p.calls.length = 0;
  p.add().value = 'zigzag'; p.add().fire('change');
  await settle();
  assert.deepEqual(p.calls, [], 'the legacy listener did nothing');
  assert.equal(p.legacyLayer.pathFx, undefined);
  assert.equal(p.sent().length, 1);
  assert.equal(p.sent()[0].payload.stack.length, 3);
});

test('the read-model row admits pathFx / pathFxOversize and refuses a malformed stack', () => {
  const base = (layers) => ({ apiVersion: 2, requestId: 'model', instanceId: 'instance', documentId: 'document', lifecycleGeneration: 1, contentRevision: 0,
    documentSnapshotId: 'snapshot-0', documentDigest: 'a'.repeat(64), byteCount: 1000, canvasW: 320, canvasH: 180, fps: 24, totalFrames: 24, reference: null, undoDepth: 0, redoDepth: 0, layers });
  const request = { requestId: 'model', instanceId: 'instance', documentId: 'document' };
  const read = Metadata.decode(base([{ layerUid: 'a', pathFx: [ZIG, ROUGH] }, { layerUid: 'b', pathFxOversize: true }]), request);
  assert.deepEqual(read.layers[0].pathFx, [ZIG, ROUGH]);
  assert.equal(read.layers[1].pathFxOversize, true);
  assert.equal(Object.isFrozen(read.layers[0].pathFx[0]), true);
  const bad = [{ pathFx: [] }, { pathFx: {} }, { pathFx: [{ enabled: true }] }, { pathFx: [3] }, { pathFxOversize: false }, { pathFx: [ZIG], pathFxOversize: true }];
  for (const extra of bad) assert.throws(() => Metadata.decode(base([{ layerUid: 'a', ...extra }]), request), /uncorrelated/, JSON.stringify(extra));
});
