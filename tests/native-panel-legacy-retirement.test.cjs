'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = name => fs.readFileSync(path.join(__dirname, '../src/js', name), 'utf8');
const copy = value => JSON.parse(JSON.stringify(value));
const preset = { label: 'Move', categoryId: 'default', motion: { pos: [{ frame: 0, v: [2, 7] }] }, motionStatic: { opacity: [75] } };

function element() {
  const classes = new Set();
  return {
    children: [], events: {}, style: {}, className: '', textContent: '',
    appendChild(child) { this.children.push(child); return child; },
    addEventListener(kind, handler) { this.events[kind] = handler; },
    setAttribute(key, value) { this[key] = value; },
    set innerHTML(value) { assert.equal(value, ''); this.children = []; },
    get innerHTML() { return ''; },
    classList: { contains: key => classes.has(key), add: key => classes.add(key), remove: key => classes.delete(key) },
    getBoundingClientRect() { return { bottom: 20, right: 100 }; },
    contains(node) { return this.children.includes(node); }, scrollIntoView() {},
  };
}
function click(node, kind = 'click') {
  assert.equal(typeof node.events[kind], 'function', 'actual DOM callback is registered');
  node.events[kind]({ stopPropagation() {}, preventDefault() {} });
}
function boot(file, guard) {
  const nodes = new Map(), stored = new Map([['nemo-motion-presets', JSON.stringify({ saved: preset })]]);
  const calls = [], notices = [], prompts = [];
  const context = {
    window: null, innerWidth: 400,
    state: { layers: [{ name: 'Layer', motion: copy(preset.motion), motionStatic: copy(preset.motionStatic) }], activeLayerIdx: 0, currentFrame: 3,
      undoStack: ['a', 'b'], redoStack: ['c'], undoLabels: [{ label: 'A' }, { label: 'B' }], redoLabels: [{ label: 'C' }] },
    document: { readyState: 'complete', addEventListener() {}, createElement: element,
      createTextNode: text => ({ textContent: text }), getElementById(id) { if (!nodes.has(id)) nodes.set(id, element()); return nodes.get(id); } },
    localStorage: { getItem: key => stored.get(key) || null, setItem(key, value) { stored.set(key, value); } },
    SM: { t: key => key }, showToast: message => notices.push(message),
    prompt() { prompts.push('prompt'); return 'Captured'; }, requestAnimationFrame: fn => fn(),
    saveAllLayerFrames() { calls.push('save'); }, pushUndoLayers() { calls.push('checkpoint'); },
    loadFrame() { calls.push('load'); }, updateUI() { calls.push('ui'); },
    undo() { calls.push('undo'); }, redo() { calls.push('redo'); },
    SMEngineBridge: guard === undefined ? {} : { nativeEditGuard: guard },
  };
  context.window = context;
  vm.runInNewContext(source(file), context, { filename: file });
  return { context, nodes, stored, calls, notices, prompts };
}
function forbidLegacyState(context) {
  Object.defineProperty(context, 'state', { get() { throw new Error('legacy state was read'); } });
}
function realGuard(native) {
  const context = {};
  vm.runInNewContext(source('application/native-edit-guard.js'), context);
  context.SMNativeEditGuard.install({ getNativeIdentity: () => native.value ? { documentId: 'native-doc', generation: 1 } : null,
    requestRelease() { throw new Error('must not release native authority'); } });
  return context.SMNativeEditGuard;
}

test('native preset callbacks refuse before reading legacy state or modifying the catalog', () => {
  const native = { value: true }, app = boot('motion-preset-picker.js', realGuard(native));
  const before = [...app.stored];
  forbidLegacyState(app.context);
  click(app.nodes.get('btn-mp-save'));
  click(app.nodes.get('motion-preset-grid').children[0]);
  assert.deepEqual([...app.stored], before);
  assert.deepEqual(app.calls, []);
  assert.deepEqual(app.prompts, []);
  assert.equal(app.notices.length, 2);
  assert.ok(app.notices.every(value => /unavailable/i.test(value)));
});

test('preset admission fails closed for malformed guards, throwing accessors and native cutover', () => {
  const setups = [
    root => { root.SMEngineBridge.nativeEditGuard = null; },
    root => { root.SMEngineBridge.nativeEditGuard = {}; },
    root => { root.SMEngineBridge.nativeEditGuard = { allow() { throw new Error('lost'); } }; },
    root => { Object.defineProperty(root.SMEngineBridge, 'nativeEditGuard', { get() { throw new Error('lost'); } }); },
    root => { root.NemoNativeOpacityCutover = { blocksLegacy: () => true }; },
    root => { root.NemoNativeOpacityCutover = null; },
  ];
  for (const setup of setups) {
    const app = boot('motion-preset-picker.js'); setup(app.context); forbidLegacyState(app.context);
    click(app.nodes.get('btn-mp-save')); click(app.nodes.get('motion-preset-grid').children[0]);
    assert.deepEqual(app.calls, []); assert.equal(app.notices.length, 2);
  }
});

test('preset capture rechecks native ownership after its name prompt', () => {
  const native = { value: false }, app = boot('motion-preset-picker.js', realGuard(native)), before = [...app.stored];
  app.context.prompt = () => { native.value = true; return 'Do not capture'; };
  click(app.nodes.get('btn-mp-save'));
  assert.deepEqual([...app.stored], before);
  assert.match(app.notices[0], /unavailable/i);
});

test('legacy preset capture and apply retain deep-copy behavior and one history checkpoint', () => {
  for (const guard of [undefined, realGuard({ value: false })]) {
    const app = boot('motion-preset-picker.js', guard);
    click(app.nodes.get('btn-mp-save'));
    const library = JSON.parse(app.stored.get('nemo-motion-presets'));
    const saved = Object.values(library).find(value => value.label === 'Captured');
    assert.deepEqual(saved.motion, preset.motion);
    app.context.state.layers[0].motion.pos[0].v[0] = 900;
    assert.deepEqual(JSON.parse(app.stored.get('nemo-motion-presets')), library);
    click(app.nodes.get('motion-preset-grid').children[0]);
    assert.deepEqual(copy(app.context.state.layers[0].motion), preset.motion);
    assert.deepEqual(app.calls, ['save', 'checkpoint', 'load', 'ui']);
    app.context.state.layers[0].motion.pos[0].v[0] = 800;
    assert.deepEqual(JSON.parse(app.stored.get('nemo-motion-presets')).saved.motion, preset.motion);
  }
});

test('native ownership preserves local preset category creation, rename and deletion', () => {
  const app = boot('motion-preset-picker.js', realGuard({ value: true }));
  forbidLegacyState(app.context);
  click(app.nodes.get('motion-preset-grid').children[0].children[2]);
  assert.deepEqual(JSON.parse(app.stored.get('nemo-motion-presets')), {});
  click(app.nodes.get('btn-mp-new-tab'));
  assert.equal(JSON.parse(app.stored.get('nemo-motion-preset-categories')).length, 2);
  app.context.prompt = () => 'Renamed';
  click(app.nodes.get('motion-preset-tabs').children[1], 'dblclick');
  assert.equal(JSON.parse(app.stored.get('nemo-motion-preset-categories'))[1].name, 'Renamed');
  app.context.showContextMenu = (_x, _y, menu) => menu.find(item => item.label === "Supprimer l'onglet").action();
  click(app.nodes.get('motion-preset-tabs').children[1], 'contextmenu');
  assert.equal(JSON.parse(app.stored.get('nemo-motion-preset-categories')).length, 1);
  assert.deepEqual(app.calls, []);
});

test('native History renders explicit unavailability without reading legacy stacks', () => {
  for (const guard of [realGuard({ value: true }), null, {}, { allow() { throw new Error('lost'); } }]) {
    const app = boot('history-panel.js', guard); forbidLegacyState(app.context);
    click(app.nodes.get('history-btn'));
    const pop = app.nodes.get('history-pop');
    assert.equal(pop.children.length, 1);
    assert.equal(pop.children[0].role, 'status');
    assert.match(pop.children[0].textContent, /unavailable/i);
    assert.deepEqual(app.calls, []);
  }
});

test('retained History clear, undo and redo callbacks recheck ownership before accessing state', () => {
  for (const action of ['clear', 'undo', 'redo']) {
    const native = { value: false }, app = boot('history-panel.js', realGuard(native));
    app.context.renderHistoryPanel();
    const rows = app.nodes.get('history-pop').children;
    const target = action === 'clear' ? rows[0].children[1] : action === 'undo' ? rows[1] : rows[4];
    native.value = true; forbidLegacyState(app.context); click(target);
    assert.deepEqual(app.calls, [], action);
    assert.match(app.nodes.get('history-pop').children[0].textContent, /unavailable/i);
  }
});

test('History stops a multi-step jump if ownership changes between undo calls', () => {
  const native = { value: false }, app = boot('history-panel.js', realGuard(native));
  app.context.renderHistoryPanel();
  app.context.undo = () => { app.calls.push('undo'); native.value = true; };
  click(app.nodes.get('history-pop').children[1]);
  assert.deepEqual(app.calls, ['undo']);
});

test('legacy History retains labels, jump counts and clear behavior', () => {
  const app = boot('history-panel.js'); app.context.renderHistoryPanel();
  const rows = app.nodes.get('history-pop').children;
  assert.equal(rows[1].children[1].children[0].textContent, 'A');
  click(rows[1]); click(rows[4]);
  assert.deepEqual(app.calls, ['undo', 'undo', 'redo']);
  click(rows[0].children[1]);
  for (const key of ['undoStack', 'undoLabels', 'redoStack', 'redoLabels']) assert.equal(app.context.state[key].length, 0);
});
