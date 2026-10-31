'use strict';
// #2372: layer blend / track matte / effect stack under native authority. The real adapter, rig command and contract run
// against a fake session; the real timeline.js picker code and the real effects-panel.js run in a vm with a fake DOM whose
// legacy layer rows THROW on any write and whose pushUndo throws. A hover must write nothing; a commit is exactly one
// native command; the effect stack sends the complete resulting stack.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const Compositing = require('../src/js/adapters/native-object-layer-compositing.js');
const Contract = require('../src/js/adapters/native-object-rig-contract.js');

const source = (name) => fs.readFileSync(path.join(__dirname, '../src/js', name), 'utf8');
const plain = (value) => JSON.parse(JSON.stringify(value));
const OP = 'command.document.object.layer.compositing.edit';
const BLUR = { type: 'blur', enabled: true, p1: 4, p2: 0, p3: 0, p4: 0, p5: 0, p6: 0, p7: 0, p8: 0 };
const GLOW = { type: 'glow', enabled: false, p1: 0.5, p2: null, keys: { p1: { keys: [{ frame: 0, v: [1] }] } } };

function rows() {
  return [
    { layerUid: 'L-top', name: 'Top', visible: true, blendMode: 'overlay', effects: [BLUR, GLOW] },
    { layerUid: 'L-mid', name: 'Mid', visible: true },
    { layerUid: 'L-low', name: 'Low', visible: true, matteMode: 'alpha', matteSourceLayerUid: 'L-top' },
    { layerUid: 'L-big', name: 'Big', visible: true, effectsOversize: true },
  ];
}
function harness(options = {}) {
  const sent = [], pin = { instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 1, contentRevision: 4, documentSnapshotId: 'snap-4', layers: options.layers || rows() };
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, requestId: (label) => label + '-' + (sent.length + 1),
    runResourceCommand: (action) => action({
      async dispatch(request) {
        sent.push(request);
        if (options.refuse) return { apiVersion: 2, requestId: request.requestId, instanceId: pin.instanceId, documentId: pin.documentId, contentRevision: pin.contentRevision, ok: false, error: { code: options.refuse, message: 'refused' } };
        return { apiVersion: 2, requestId: request.requestId, instanceId: pin.instanceId, documentId: pin.documentId, contentRevision: pin.contentRevision + 1, ok: true, result: { applied: true, historyEntriesAdded: 1 } };
      },
      async reconcile() {},
    }) };
  return { session, sent, pin, adapter: Compositing.create(session) };
}
const settle = async () => { for (let i = 0; i < 14; i += 1) await new Promise((resolve) => setImmediate(resolve)); };

test('the contract admits the compositing edit as a closed union by edit and refuses unknown shapes locally', () => {
  assert.equal(Contract.admitted(OP), true);
  assert.equal(Contract.requiresRevision(OP), true);
  const ok = (extra) => { try { Contract.payload(OP, Object.assign({ command: 'object.layer.compositing.edit', layerUid: 'L' }, extra)); return true; } catch (_) { return false; } };
  assert.equal(ok({ edit: 'blend', mode: 'multiply' }), true);
  assert.equal(ok({ edit: 'matteMode', mode: 'luma', source: 'S' }), true);
  assert.equal(ok({ edit: 'matteClear' }), true);
  assert.equal(ok({ edit: 'effects', effects: [BLUR, GLOW] }), true, 'null parameters and keyed tracks are admitted');
  assert.equal(ok({ edit: 'blend', mode: 'Multiply' }), false);
  assert.equal(ok({ edit: 'matteMode', mode: 'none' }), false, 'None is matteClear');
  assert.equal(ok({ edit: 'effects' }), false);
  assert.equal(ok({ edit: 'effects', effects: [{ type: 'blur' }] }), false);
  assert.equal(ok({ edit: 'blend', mode: 'screen', source: 'S' }), false);
  assert.equal(ok({ edit: 'effects', effects: [{ type: 'blur', enabled: true, keys: { p1: { keys: 'x'.repeat(2100) } } }] }), false, 'key tracks are bounded');
});

test('a blend pick is one native command; Normal is sent as normal (the owner clears the member)', async () => {
  const h = harness();
  const receipt = await h.adapter.setBlend(1, 'multiply');
  assert.equal(h.sent.length, 1);
  assert.equal(h.sent[0].operation, OP);
  assert.equal(h.sent[0].expectedRevision, 4);
  assert.deepEqual(plain(h.sent[0].payload), { command: 'object.layer.compositing.edit', layerUid: 'L-mid', edit: 'blend', mode: 'multiply' });
  assert.equal(receipt.applied, true);
  await h.adapter.setBlend(0, 'normal');
  assert.equal(h.sent[1].payload.mode, 'normal');
  assert.equal(h.adapter.blendOf(0), 'overlay');
  assert.equal(h.adapter.blendOf(1), 'normal', 'absent reads as Normal');
  await assert.rejects(h.adapter.setBlend(1, 'bogus'), (error) => error.code === 'invalid_request');
  await assert.rejects(h.adapter.setBlend(9, 'screen'), (error) => error.code === 'not_found');
  assert.equal(h.sent.length, 2, 'refused-locally calls never reach the owner');
});

test('a matte pick freezes the adjacent source in the same write; None clears the matte', async () => {
  const h = harness();
  await h.adapter.setMatteMode(1, 'luma');
  assert.deepEqual(plain(h.sent[0].payload), { command: 'object.layer.compositing.edit', layerUid: 'L-mid', edit: 'matteMode', mode: 'luma', source: 'L-low' });
  await h.adapter.setMatteMode(2, 'luma');
  assert.equal('source' in h.sent[1].payload, false, 'a stored source is kept, never re-frozen');
  await h.adapter.setMatteMode(3, 'alpha');
  assert.equal('source' in h.sent[2].payload, false, 'the last layer has no layer below');
  await h.adapter.setMatteMode(2, 'none');
  assert.deepEqual(plain(h.sent[3].payload), { command: 'object.layer.compositing.edit', layerUid: 'L-low', edit: 'matteClear' });
  assert.equal(h.adapter.matteOf(2), 'alpha');
  assert.equal(h.adapter.sourceOf(2), 'L-top');
  await h.adapter.setMatteSource(1, 'L-top');
  assert.equal(h.sent[4].payload.edit, 'matteSource');
});

test('every effect stack edit sends the COMPLETE resulting stack and keeps keyed tracks and null parameters', async () => {
  const h = harness();
  await h.adapter.addEffect(0, { type: 'sepia', enabled: true, p1: 1 });
  assert.deepEqual(plain(h.sent[0].payload.effects), [BLUR, GLOW, { type: 'sepia', enabled: true, p1: 1 }]);
  await h.adapter.toggleEffect(0, 0);
  assert.deepEqual(plain(h.sent[1].payload.effects), [Object.assign({}, BLUR, { enabled: false }), GLOW]);
  await h.adapter.removeEffect(0, 0);
  assert.deepEqual(plain(h.sent[2].payload.effects), [GLOW]);
  await h.adapter.moveEffect(0, 1, 0);
  assert.deepEqual(plain(h.sent[3].payload.effects), [GLOW, BLUR]);
  await h.adapter.setEffectParam(0, 1, 'p1', 9);
  assert.equal(h.sent[4].payload.effects[1].p1, 9);
  assert.deepEqual(plain(h.sent[4].payload.effects[1].keys), GLOW.keys, 'the key tracks ride along untouched');
  assert.equal(h.sent[4].payload.effects[1].p2, null, 'an explicit null parameter stays null');
  assert.equal(h.sent.every((request) => request.operation === OP && request.payload.edit === 'effects'), true);
  await assert.rejects(h.adapter.removeEffect(0, 5), (error) => error.code === 'not_found');
  await assert.rejects(h.adapter.setEffectParam(0, 0, 'p9', 1), (error) => error.code === 'invalid_request');
  assert.equal(h.sent.length, 5);
});

test('an oversize stack is never guessed at: the edit is refused before anything is sent', async () => {
  const h = harness();
  assert.equal(h.adapter.stackOf(3), null);
  assert.equal(h.adapter.stackAvailable(3), false);
  await assert.rejects(h.adapter.addEffect(3, { type: 'blur', enabled: true }), (error) => error.code === 'unavailable' && error.reason === 'effects_oversize');
  const full = harness({ layers: [{ layerUid: 'F', effects: Array.from({ length: 32 }, () => ({ type: 'blur', enabled: true })) }] });
  await assert.rejects(full.adapter.addEffect(0, { type: 'blur', enabled: true }), (error) => error.code === 'invalid_request');
  assert.equal(h.sent.length + full.sent.length, 0);
});

test('a refused edit rejects with the owner code', async () => {
  const h = harness({ refuse: 'stale_revision' });
  await assert.rejects(h.adapter.setBlend(1, 'screen'), (error) => error.code === 'stale_revision');
});

// ---- the real timeline.js pickers ----
function node(name) {
  const el = { name, children: [], events: {}, style: {}, dataset: {}, className: '', textContent: '',
    appendChild(child) { this.children.push(child); return child; }, addEventListener(kind, fn) { this.events[kind] = fn; },
    contains(other) { return this.children.includes(other); }, getBoundingClientRect: () => ({ left: 10, right: 60, top: 10, bottom: 30 }), offsetWidth: 100, offsetHeight: 100 };
  Object.defineProperty(el, 'innerHTML', { set(value) { assert.equal(value, ''); el.children = []; }, get() { return ''; } });
  return el;
}
function timelineHarness(options = {}) {
  const h = harness(options), nodes = new Map(), toasts = [], listeners = {}, renders = [];
  const trap = (row) => new Proxy(row, { set() { throw Error('legacy layer row written'); }, deleteProperty() { throw Error('legacy layer row written'); }, defineProperty() { throw Error('legacy layer row written'); } });
  const state = { activeLayerIdx: 1, currentFrame: 3, layers: h.pin.layers.map(trap) };
  const tripped = (name) => () => { throw Error(name + ' reached'); };
  const body = node('body');
  const context = { state, console, SM: { t: (k) => k }, showToast: (m) => toasts.push(m), pushUndo: tripped('pushUndo'),
    renderLayerList: () => renders.push('list'), renderTimeline: () => renders.push('timeline'),
    document: { getElementById: (id) => { if (!['p-blendmode', 'p-mattemode'].includes(id)) return null; if (!nodes.has(id)) nodes.set(id, node(id)); return nodes.get(id); },
      createElement: () => node('div'), body, addEventListener: (kind, fn) => { listeners['doc:' + kind] = fn; } },
    NemoNativeObjectProject: { desktop: () => h.session }, NemoNativeObjectLayerCompositing: Compositing,
    SMEngineBridge: { renderNow: tripped('renderNow') }, innerWidth: 1000, innerHeight: 800 };
  context.window = context;
  context.window.addEventListener = (kind, fn) => { (listeners['win:' + kind] = listeners['win:' + kind] || []).push(fn); };
  const text = source('timeline.js');
  const from = text.indexOf('// #2372: on a native object project the blend and matte pickers');
  const to = text.indexOf('// FPS/Frames now live in the Document section');
  assert.ok(from > 0 && to > from, 'the picker block is locatable');
  vm.runInNewContext(text.slice(from, to), context, { filename: 'timeline.js' });
  const popFor = (id) => body.children.find((child) => child.id === id);
  return Object.assign(h, { nodes, toasts, listeners, renders, state, context, body, dd: (id) => nodes.get(id), popOf: (id) => popFor(id) });
}
const optionNamed = (pop, label) => pop.children.find((child) => child.textContent === label);

test('timeline blend picker on native: hover writes nothing (label only), commit is exactly one native edit, no pushUndo', async () => {
  const t = timelineHarness();
  t.state.activeLayerIdx = 0;
  const dd = t.dd('p-blendmode'), pop = t.body.children[0];
  dd.textContent = 'Overlay';
  dd.events.click({ stopPropagation() {} });
  assert.equal(pop.style.display, 'block');
  assert.equal(optionNamed(pop, 'Overlay').className.includes('sel'), true, 'the stored native mode is the selected one');
  optionNamed(pop, 'Multiply').events.mouseenter();
  assert.equal(dd.textContent, 'Multiply', 'hover previews in the field only');
  optionNamed(pop, 'Screen').events.mouseenter();
  assert.equal(t.sent.length, 0, 'hover sends nothing');
  assert.equal(t.context._sceneVersion, undefined, 'hover touches no scene version');
  t.listeners['win:keydown'].forEach((fn) => fn({ key: 'Escape' }));
  assert.equal(dd.textContent, 'Overlay', 'Escape reverts the label');
  assert.equal(t.sent.length, 0);
  dd.events.click({ stopPropagation() {} });
  optionNamed(pop, 'Multiply').events.mouseenter();
  optionNamed(pop, 'Multiply').events.click({ stopPropagation() {} });
  await settle();
  assert.equal(t.sent.length, 1, 'commit = exactly one command');
  assert.deepEqual(plain(t.sent[0].payload), { command: 'object.layer.compositing.edit', layerUid: 'L-top', edit: 'blend', mode: 'multiply' });
  assert.equal(dd.dataset.value, 'multiply');
  assert.equal(dd.textContent, 'Multiply');
  assert.equal(pop.style.display, 'none');
  assert.ok(t.renders.includes('list'), 'the layer list is refreshed');
  assert.deepEqual(t.toasts, []);
});

test('timeline blend picker on native: a refused commit restores the label and says so', async () => {
  const t = timelineHarness({ refuse: 'stale_revision' });
  const dd = t.dd('p-blendmode'), pop = t.body.children[0];
  dd.textContent = 'Normal';
  dd.events.click({ stopPropagation() {} });
  optionNamed(pop, 'Screen').events.mouseenter();
  optionNamed(pop, 'Screen').events.click({ stopPropagation() {} });
  await settle();
  assert.equal(dd.textContent, 'Normal');
  assert.deepEqual(t.toasts, ['Native layer compositing command was refused.']);
});

test('timeline matte picker on native: hover is label-only; a pick freezes the adjacent source; None clears', async () => {
  const t = timelineHarness();
  const dd = t.dd('p-mattemode'), pop = t.body.children[1];
  dd.events.click({ stopPropagation() {} });
  optionNamed(pop, 'matteLuma').events.mouseenter();
  assert.equal(dd.textContent, 'matteLuma');
  assert.equal(t.sent.length, 0);
  optionNamed(pop, 'matteLuma').events.click({ stopPropagation() {} });
  await settle();
  assert.equal(t.sent.length, 1);
  assert.deepEqual(plain(t.sent[0].payload), { command: 'object.layer.compositing.edit', layerUid: 'L-mid', edit: 'matteMode', mode: 'luma', source: 'L-low' });
  t.state.activeLayerIdx = 2;
  dd.events.click({ stopPropagation() {} });
  assert.equal(optionNamed(pop, 'matteAlpha').className.includes('sel'), true);
  optionNamed(pop, 'matteNone').events.click({ stopPropagation() {} });
  await settle();
  assert.equal(t.sent.length, 2);
  assert.deepEqual(plain(t.sent[1].payload), { command: 'object.layer.compositing.edit', layerUid: 'L-low', edit: 'matteClear' });
});

test('timeline matte menu on native: source picks and removal are native edits', async () => {
  const t = timelineHarness();
  const text = source('timeline.js');
  const from = text.indexOf('function buildMatteMenuItems(li,ld,onChanged){');
  const to = text.indexOf('window.buildMatteMenuItems=buildMatteMenuItems;');
  vm.runInContext(text.slice(from, to), t.context);
  let changed = 0;
  const items = t.context.buildMatteMenuItems(2, t.state.layers[2], () => { changed += 1; });
  assert.equal(items[0].disabled, false, 'a matte is on');
  const sources = items.filter((item) => item.label.startsWith('Source'));
  assert.equal(sources.length, 3, 'every other layer, never itself');
  assert.equal(sources.find((item) => /Top/.test(item.label)).label.endsWith('✓'), true, 'the stored source is ticked');
  sources.find((item) => /Mid/.test(item.label)).action();
  await settle();
  assert.deepEqual(plain(t.sent[0].payload), { command: 'object.layer.compositing.edit', layerUid: 'L-low', edit: 'matteSource', source: 'L-mid' });
  items[0].action();
  await settle();
  assert.equal(t.sent[1].payload.edit, 'matteClear');
  assert.equal(changed, 2);
});

// ---- the real effects-panel.js ----
function panelHarness(options = {}) {
  const h = harness(options), nodes = new Map(), toasts = [];
  const make = (name) => {
    const el = { name, children: [], events: {}, style: {}, dataset: {}, classList: { add() {}, remove() {}, toggle() {} }, className: '', textContent: '', title: '',
      appendChild(child) { child.parentNode = this; this.children.push(child); return child; }, addEventListener(kind, fn) { this.events[kind] = fn; },
      insertBefore(child) { child.parentNode = this; this.children.push(child); return child; }, querySelector: () => null, getBoundingClientRect: () => ({}) };
    let html = '';
    Object.defineProperty(el, 'innerHTML', { set(value) { html = value; if (value === '') el.children = []; }, get() { return html; } });
    return el;
  };
  const byId = (id) => { if (!['effects-stack-sec', 'effects-list'].includes(id)) return null; if (!nodes.has(id)) nodes.set(id, make(id)); return nodes.get(id); };
  const tripped = (name) => () => { throw Error(name + ' reached'); };
  const trap = (row) => new Proxy(row, { set() { throw Error('legacy layer row written'); }, deleteProperty() { throw Error('legacy layer row written'); } });
  const state = { activeLayerIdx: 0, currentFrame: 2, tool: 'select', layers: h.pin.layers.map(trap) };
  const context = { state, console, window: null, SM: { t: (k) => k }, showToast: (m) => toasts.push(m), pushUndo: tripped('pushUndo'), saveActiveLayerFrame: tripped('saveActiveLayerFrame'),
    document: { getElementById: byId, createElement: () => make('div'), readyState: 'complete', addEventListener() {}, body: make('body') },
    NemoNativeObjectProject: { desktop: () => h.session }, NemoNativeObjectLayerCompositing: Compositing, SMEngineBridge: { renderNow: tripped('renderNow') },
    selectedPaths: [], Path: function Path() {}, CompoundPath: function CompoundPath() {} };
  context.window = context;
  context.window.addEventListener = () => {};
  vm.runInNewContext(source('effects-panel.js'), context, { filename: 'effects-panel.js' });
  return Object.assign(h, { context, state, toasts, list: () => byId('effects-list'), section: () => byId('effects-stack-sec') });
}

test('effects panel on native: the stack comes from the native row, and add / toggle / delete each send the full stack', async () => {
  const p = panelHarness();
  p.context.updateEffectsPanel();
  assert.equal(p.list().children.length, 2, 'both native effects are listed');
  assert.equal(p.list().children[1].className.includes('disabled'), true, 'the stored enabled=false state is shown');
  p.context.addEffectToActiveLayer('sepia');
  await settle();
  assert.equal(p.sent.length, 1);
  assert.equal(p.sent[0].payload.edit, 'effects');
  assert.equal(p.sent[0].payload.layerUid, 'L-top');
  assert.deepEqual(plain(p.sent[0].payload.effects.slice(0, 2)), [BLUR, GLOW]);
  assert.equal(p.sent[0].payload.effects.length, 3);
  assert.equal(p.sent[0].payload.effects[2].type, 'sepia');
  const eye = p.list().children[0].children[1];
  eye.events.click({ stopPropagation() {} });
  await settle();
  assert.equal(p.sent.length, 2);
  assert.equal(p.sent[1].payload.effects[0].enabled, false);
  assert.equal(p.sent[1].payload.effects.length, 2);
  const del = p.list().children[0].children[3];
  del.events.click({ stopPropagation() {} });
  await settle();
  assert.equal(p.sent.length, 3);
  assert.deepEqual(plain(p.sent[2].payload.effects), [GLOW]);
  assert.deepEqual(p.toasts, []);
});

// #2372: the read bound (engine EFFECTS_ROW_BYTES = 4096) is at least what an edit can leave behind.
const BIG = (i) => ({ type: 'chromaticAberration', enabled: true, p1: i + 0.25, p2: 0.5, p3: 0.125, p4: 100, p5: 0.333333, p6: 0.75, p7: 12.5, p8: 0.1 });
test('a stack at the edit limit stays editable, and an edit that would leave it unlistable is refused with a clear notice', async () => {
  const stack = Array.from({ length: 29 }, (_, i) => BIG(i));
  assert.ok(JSON.stringify(stack).length > 2048 && JSON.stringify(stack).length < 3840, 'past the old 2048 read bound, inside the edit bound');
  const h = panelHarness({ layers: [{ layerUid: 'F', name: 'F', visible: true, effects: stack }] });
  h.context.updateEffectsPanel();
  assert.equal(h.list().children.length, 29, 'every effect is listed');
  h.context.addEffectToActiveLayer('blur');
  await settle();
  assert.equal(h.sent.length, 1, 'the add that still fits is one native edit');
  assert.equal(h.sent[0].payload.effects.length, 30);
  assert.ok(JSON.stringify(h.sent[0].request || h.sent[0]).length <= 4096, 'the request stays inside the dispatcher bound');
  const full = panelHarness({ layers: [{ layerUid: 'F', name: 'F', visible: true, effects: Array.from({ length: 30 }, (_, i) => BIG(i)) }] });
  full.context.addEffectToActiveLayer('chromaticAberration');
  await settle();
  assert.equal(full.sent.length, 0, 'nothing is sent');
  assert.deepEqual(full.toasts, ["No room for more effects: this layer's effect stack is full."]);
  await assert.rejects(full.adapter.addEffect(0, BIG(99)), (error) => error.code === 'invalid_request' && error.reason === 'effects_bytes');
  const tiny = harness({ layers: [{ layerUid: 'T', effects: Array.from({ length: 32 }, () => ({ type: 'blur', enabled: true })) }] });
  tiny.adapter.toggleEffect(0, 31);
  await settle();
  assert.equal(tiny.sent.length, 1, 'a 32-entry stack of small entries is still editable');
});

test('effects panel on native: an oversize row is flagged in the list, not shown as an empty stack', () => {
  const p = panelHarness();
  p.state.activeLayerIdx = 3;
  p.context.updateEffectsPanel();
  assert.equal(p.list().children.length, 1);
  assert.equal(p.list().children[0].textContent, 'This effect stack is too large to list or edit natively.');
});

test('effects panel on native: an oversize stack and a refusal both say so and never fall back to the legacy writer', async () => {
  const big = panelHarness();
  big.state.activeLayerIdx = 3;
  big.context.addEffectToActiveLayer('blur');
  await settle();
  assert.equal(big.sent.length, 0);
  assert.deepEqual(big.toasts, ['This effect stack is too large to edit natively.']);
  const refused = panelHarness({ refuse: 'stale_revision' });
  refused.context.addEffectToActiveLayer('blur');
  await settle();
  assert.deepEqual(refused.toasts, ['Native effect stack command was refused.']);
});

// ---- the closed schema, the JS contract and the Rust validator data agree ----
test('the closed schema and the contract accept and refuse the same compositing payloads', () => {
  const { validator } = require('./native-object-rig-schema.cjs');
  const schema = JSON.parse(fs.readFileSync(path.join(__dirname, '../engineering/application/native-transport-v2.schema.json'), 'utf8'));
  const v = validator(schema), def = { $ref: '#/$defs/ObjectLayerCompositingEditPayload' };
  const base = { command: 'object.layer.compositing.edit', layerUid: 'L' };
  const cases = [
    [{ edit: 'blend', mode: 'multiply' }, true], [{ edit: 'blend', mode: 'luma' }, false], [{ edit: 'blend' }, false],
    [{ edit: 'blend', mode: 'screen', source: 'S' }, false], [{ edit: 'matteMode', mode: 'alphaInverted' }, true],
    [{ edit: 'matteMode', mode: 'luma', source: 'S' }, true], [{ edit: 'matteMode', mode: 'none' }, false], [{ edit: 'matteMode', mode: 'multiply' }, false],
    [{ edit: 'matteSource', source: 'S' }, true], [{ edit: 'matteSource' }, false], [{ edit: 'matteClear' }, true], [{ edit: 'matteClear', mode: 'alpha' }, false],
    [{ edit: 'matteMoreAdd', source: 'S' }, true], [{ edit: 'matteMoreAdd', source: 'S', mode: 'luma' }, true], [{ edit: 'matteMoreSet', index: 0, mode: 'luma' }, true],
    [{ edit: 'matteMoreSet', mode: 'luma' }, false], [{ edit: 'matteMoreRemove', index: 2 }, true], [{ edit: 'matteMoreRemove', index: -1 }, false],
    [{ edit: 'effects', effects: [BLUR, { type: 'glow', enabled: false, p2: null, keys: {} }] }, true], [{ edit: 'effects', effects: [] }, true], [{ edit: 'effects' }, false],
    [{ edit: 'effects', effects: [{ type: 'blur' }] }, false], [{ edit: 'effects', effects: [{ type: 'blur', enabled: true, p9: 1 }] }, false],
    [{ edit: 'effectsFromSet', source: 'S' }, true], [{ edit: 'effectsFromClear' }, true], [{ edit: 'folderCollapsed', collapsed: true }, true],
    [{ edit: 'folderCollapsed', collapsed: 1 }, false], [{ edit: 'unknown' }, false],
  ];
  for (const [extra, expected] of cases) {
    const payload = Object.assign({}, base, extra);
    let contract = true;
    try { Contract.payload(OP, payload); } catch (_) { contract = false; }
    assert.equal(v.ok(def, payload), expected, 'schema ' + JSON.stringify(extra));
    // The contract additionally refuses a matteMoreSet that sets nothing only in the owner, so it may be wider there.
    if (!(extra.edit === 'matteMoreSet' && extra.index === undefined)) assert.equal(contract, expected, 'contract ' + JSON.stringify(extra));
  }
});

test('panel hooks: parameter edits send one stack edit, keyed parameters and keying report unavailable, and no native project answers false', async () => {
  const p = panelHarness();
  const hooks = Compositing.panel(p.context);
  const keyed = (eff, key) => !!(eff.keys && eff.keys[key]);
  assert.equal(hooks.on(), true);
  assert.equal(hooks.param(0, 'p1', 8, keyed), true);
  await settle();
  assert.equal(p.sent.length, 1);
  assert.equal(p.sent[0].payload.effects[0].p1, 8);
  assert.equal(hooks.param(1, 'p1', 8, keyed), true, 'a keyed parameter is claimed by the native path');
  await settle();
  assert.equal(p.sent.length, 1, 'but nothing is written');
  assert.deepEqual(p.toasts, ['Keyed effect parameters are not available on native projects yet.']);
  assert.equal(hooks.keyed(), true);
  const none = Compositing.panel({ state: { activeLayerIdx: 0 }, NemoNativeObjectProject: { desktop: () => null } });
  assert.equal(none.on(), false);
  assert.equal(none.stack(), null);
  assert.equal(none.add('blur', [0, 0, 0, 0, 0, 0, 0, 0]), false);
  assert.equal(none.toggle(0), false);
  assert.equal(none.remove(0), false);
  assert.equal(none.keyed(), false);
  assert.equal(none.param(0, 'p1', 1, keyed), false);
});
