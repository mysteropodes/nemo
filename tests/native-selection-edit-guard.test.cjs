'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const ROOT = path.resolve(__dirname, '..');
const GUARD = path.join(ROOT, 'src/js/application/native-edit-guard.js');
const source = (file) => fs.readFileSync(path.join(ROOT, file), 'utf8');
function freshGuard() { delete require.cache[require.resolve(GUARD)]; return require(GUARD); }
function event() { return { stopped: 0, prevented: 0, stopImmediatePropagation() { this.stopped++; }, preventDefault() { this.prevented++; } }; }
function installGuard(active, releases) {
  const guard = freshGuard();
  guard.install({
    getNativeIdentity: () => active.value ? { documentId: 'document-1', generation: 4 } : null,
    requestRelease: (request) => { releases.push(request); return { documentId: request.documentId, generation: request.generation, owner: 'legacy', status: 'released' }; },
  });
  return guard;
}
function selectionGuardContext(port) {
  const text = source('src/js/select-bridge.js');
  const context = { console, Object, window: null, SMEngineBridge: { isEnabled: () => true } };
  context.window = context;
  context.addEventListener = () => {};
  if (port !== undefined) context.SMEngineBridge.nativeEditGuard = port;
  vm.runInNewContext(text.slice(0, text.indexOf('  // Same handle-position math')) + '\nwindow.__selectionGuard={allowLegacySelectionEdit:allowLegacySelectionEdit,guardMenuItems:guardMenuItems};\n})();', context);
  return context;
}
function bootSelect(guard, motion) {
  const handlers = {}, target = { addEventListener(type, handler) { handlers[type] = handler; } };
  const context = {
    console, Object, state: { tool: 'select', playing: false, appMode: 'motion', layers: [{ locked: false }], activeLayerIdx: 0 },
    document: { readyState: 'complete', addEventListener() {}, getElementById: () => target },
    Point: function Point(x, y) { this.x = x; this.y = y; }, window: null,
  };
  context.window = context;
  context.SMEngineBridge = { isEnabled: () => true, screenToWorld: (x, y) => [x, y], nativeEditGuard: guard };
  context.SMMotion = motion;
  vm.runInNewContext(source('src/js/select-bridge.js'), context, { filename: 'select-bridge.js' });
  return { handlers, context };
}
function fakeElement() {
  return { children: [], events: {}, dataset: {}, style: {}, className: '', innerHTML: '', textContent: '', title: '',
    classList: { add() {}, remove() {} }, appendChild(child) { this.children.push(child); return child; },
    addEventListener(type, handler) { this.events[type] = handler; }, querySelectorAll() { return []; } };
}
function bootShapes(port, calls) {
  const list = fakeElement();
  const context = {
    console, Object, window: null, selectedPaths: [], userLayers: [], ICO_GROUP: '', ICO_EYE: '', ICO_EYE_CLOSED: '', ICO_COMBINE_UNITE: '', ICO_COMBINE_EXCLUDE: '', ICO_COMBINE_NONE: '',
    state: { activeLayerIdx: 0, layers: [{ groups: { group: { combineMode: 'none' } } }], selectedStrokeIndices: [] },
    document: { getElementById: (id) => id === 'shapes-list' ? list : null, createElement: fakeElement }, SM: { t: (key) => key },
    SMMotion: { buildShapeTree: () => [{ type: 'group', gid: 'group', memberIds: [], children: [] }], layerElements: () => [], liveItemByStrokeId: () => null },
    SMGroup: { setGroupCombineMode(...args) { calls.push(args); } },
  };
  context.window = context;
  context.addEventListener = () => {};
  context.SMEngineBridge = port === undefined ? {} : { nativeEditGuard: port };
  vm.runInNewContext(source('src/js/shapes-panel.js'), context, { filename: 'shapes-panel.js' });
  context.renderShapesPanel();
  return { context, list };
}
function toolFunction(name) {
  const text = source('src/js/tools.js'), start = text.indexOf(`function ${name}(`);
  assert.notEqual(start, -1, `missing ${name}`);
  const open = text.indexOf('{', start), end = functionEnd(text, open);
  return text.slice(start, end + 1);
}
function functionEnd(text, open) {
  let depth = 0, quote = '', line = false, block = false;
  for (let i = open; i < text.length; i++) {
    const ch = text[i], next = text[i + 1];
    if (line) { if (ch === '\n') line = false; continue; }
    if (block) { if (ch === '*' && next === '/') { block = false; i++; } continue; }
    if (quote) { if (ch === '\\') { i++; continue; } if (ch === quote) quote = ''; continue; }
    if (ch === '/' && next === '/') { line = true; i++; continue; }
    if (ch === '/' && next === '*') { block = true; i++; continue; }
    if (ch === '"' || ch === "'" || ch === '`') { quote = ch; continue; }
    if (ch === '{') depth++;
    if (ch === '}' && --depth === 0) return i;
  }
  throw new Error('unterminated function');
}
function runTool(name, context) { vm.runInNewContext(toolFunction(name), context, { filename: `tools-${name}.js` }); return context[name]; }
function bootTools(guard, state, motion) {
  const stage = { id: 'drawing-canvas', events: {}, addEventListener(type, handler) { this.events[type] = handler; }, classList: { add() {}, remove() {} }, style: {} };
  const context = {
    console, Object, window: null, Tool: function Tool() { return {}; }, view: {}, draggingArc: null,
    state: Object.assign({ tool: 'draw', appMode: 'animation', playing: false, isPanning: false, spaceDown: false, activeLayerIdx: 0, layers: [{}] }, state),
    document: { addEventListener() {}, getElementById: () => stage, querySelectorAll: () => [] },
    SMEngineBridge: { nativeEditGuard: guard, isEnabled: () => true, screenToWorld: (x, y) => [x, y] }, SMMotion: motion,
  };
  context.window = context;
  vm.runInNewContext(source('src/js/tools.js'), context, { filename: 'tools.js' });
  return { context, stage };
}

test('native ownership requests one exact release and admits only a later replay', () => {
  const active = { value: true }, releases = [], guard = installGuard(active, releases), context = selectionGuardContext(guard);
  let mutations = 0;
  const retained = context.__selectionGuard.guardMenuItems([{ action() { mutations++; } }]);
  retained[0].action();
  assert.equal(mutations, 0);
  assert.deepEqual(releases, [{ kind: 'select', documentId: 'document-1', generation: 4 }]);
  retained[0].action();
  assert.equal(releases.length, 1);
  active.value = false;
  retained[0].action();
  assert.equal(mutations, 1);
});

test('absent ports pass through; malformed and throwing ports fail closed', () => {
  assert.equal(selectionGuardContext().__selectionGuard.allowLegacySelectionEdit(event(), 'select'), true);
  for (const port of [{}, { allow() { throw new Error('broken'); } }]) {
    const context = selectionGuardContext(port), e = event();
    assert.equal(context.__selectionGuard.allowLegacySelectionEdit(e, 'select'), false);
    assert.equal(e.stopped, 1);
    assert.equal(e.prevented, 1);
  }
});

test('Motion forwards no down, drag, or up callback on the denied stack', () => {
  const active = { value: true }, releases = [], guard = installGuard(active, releases), calls = { down: 0, drag: 0, up: 0 };
  const { handlers } = bootSelect(guard, { onDown() { calls.down++; return true; }, onDrag() { calls.drag++; return true; }, onUp() { calls.up++; return true; }, onHoverMove() { return false; } });
  const denied = event(); denied.clientX = 2; denied.clientY = 3; denied.button = 0;
  handlers.pointerdown(denied);
  assert.deepEqual(calls, { down: 0, drag: 0, up: 0 });
  assert.equal(releases.length, 1);
  active.value = false;
  const down = event(); down.clientX = 2; down.clientY = 3; down.button = 0; handlers.pointerdown(down);
  const drag = event(); drag.clientX = 3; drag.clientY = 4; handlers.pointermove(drag);
  const up = event(); up.clientX = 3; up.clientY = 4; handlers.pointerup(up);
  assert.deepEqual(calls, { down: 1, drag: 1, up: 1 });
});

test('a Shapes combine menu retained from legacy cannot mutate after activation', () => {
  const active = { value: false }, releases = [], guard = installGuard(active, releases), calls = [];
  const { context, list } = bootShapes(undefined, calls);
  let menu;
  context.showContextMenu = (_x, _y, items) => { menu = items; };
  list.children[0].events.contextmenu({ clientX: 1, clientY: 2, preventDefault() {}, stopPropagation() {} });
  const unite = menu.find((item) => item.label === 'combineUnion');
  assert.ok(unite);
  context.SMEngineBridge.nativeEditGuard = guard;
  active.value = true;
  unite.action();
  assert.equal(calls.length, 0);
  assert.equal(releases.length, 1);
  active.value = false;
  unite.action();
  assert.equal(calls.length, 1);
});

test('public selection helpers deny before undo or Paper mutation', () => {
  const helpers = [
    ['nodeSelApplyMove', [1, 1], { nodeEditTargetPath: () => ({}), _nodeSel: [0] }],
    ['nodeSelApplyScale', [2, 2, { x: 0, y: 0 }], { nodeEditTargetPath: () => ({}), _nodeSel: [0] }],
    ['nodeSelApplyRotate', [30, { x: 0, y: 0 }], { nodeEditTargetPath: () => ({}), _nodeSel: [0] }],
    ['alignSelection', ['left'], { selectedPaths: [{}, {}] }], ['distributeSelection', ['horizontal'], { selectedPaths: [{}, {}, {}] }],
    ['duplicateSelection', [], { selectedPaths: [{}] }], ['cutSelection', [], { selectedPaths: [{}] }],
    ['pasteSelection', [], { _canvasClip: { snaps: [{}] } }], ['fsApplyDelete', [], { _fsSel: [{}] }],
    ['insertVertexAt', [{ getNearestLocation() { throw new Error('Paper must not run'); } }, {}], {}],
  ];
  for (const [name, args, extra] of helpers) {
    let undo = 0;
    const context = Object.assign({ allowLegacySelectionEdit: () => false, pushUndo() { undo++; }, window: {}, state: {}, SM: { t: () => '' }, showToast() {} }, extra);
    runTool(name, context)(...args);
    assert.equal(undo, 0, `${name} reached undo while denied`);
  }
});

test('idle Paper drag and up do not request release', () => {
  let releases = 0;
  const context = {
    state: { tool: 'select', playing: false, isPanning: false, spaceDown: false, appMode: 'animation' },
    _xform: { active: false }, _marquee: { active: false }, _nodeDrag: { active: false }, _nmq: { active: false },
    _fsPromoteDrag: null, _fsBreak: null, draggingArc: null, _moveDragStarted: false, selectedPaths: [], window: {},
    allowLegacySelectionEdit() { releases++; return false; },
  };
  vm.runInNewContext(source('src/js/tools.js').match(/function selectionGestureActive\(\)\{[^\n]+/)[0], context);
  runTool('onMouseDrag', context)({ event: event(), modifiers: {}, point: {} });
  runTool('onMouseUp', context)({ event: event(), modifiers: {}, point: {} });
  assert.equal(releases, 0);
});

test('full Paper tools guards selected-item drag and up before their mutations', () => {
  const active = { value: true }, releases = [], guard = installGuard(active, releases);
  const { context } = bootTools(guard, { tool: 'select' }, {});
  context.selectedPaths = [{ position: { add() { throw new Error('Paper move must not run'); } } }];
  context.onMouseDrag({ event: event(), delta: {} });
  context.onMouseUp({ event: event() });
  assert.equal(releases.length, 1);
});

test('full Paper Motion forwarding guards an active gesture but leaves idle drag/up passive', () => {
  const active = { value: true }, releases = [], guard = installGuard(active, releases), calls = { down: 0, drag: 0, up: 0 };
  const motion = { onDown() { calls.down++; return true; }, onDrag() { calls.drag++; return true; }, onUp() { calls.up++; return true; } };
  const { context } = bootTools(guard, { tool: 'draw', appMode: 'motion' }, motion);
  context.onMouseDrag({ event: event() });
  context.onMouseUp({ event: event() });
  assert.equal(releases.length, 0, 'idle Motion forwarding must not request release');
  context.onMouseDown({ event: event(), modifiers: {} });
  assert.equal(calls.down, 0);
  assert.equal(releases.length, 1);
  active.value = false;
  context.onMouseDown({ event: event(), modifiers: {} });
  assert.equal(calls.down, 1);
  active.value = true;
  context.onMouseDrag({ event: event() });
  assert.equal(calls.drag, 1, 'the denied active drag must not reach Motion');
  active.value = false;
  context.onMouseDrag({ event: event() });
  context.onMouseUp({ event: event() });
  assert.deepEqual(calls, { down: 1, drag: 2, up: 2 });
});

test('complete Select bridge and Paper tools share a debug-visible Motion gesture', () => {
  const active = { value: false }, releases = [], guard = installGuard(active, releases), calls = { down: 0, drag: 0, up: 0 };
  let drag = null;
  const motion = {
    onDown() { calls.down++; drag = { mode: 'point' }; return true; },
    onDrag() { if (!drag) return false; calls.drag++; return true; },
    onUp() { if (!drag) return false; calls.up++; drag = null; return true; },
    debugMotionDrag() { return drag; }, onHoverMove() { return false; },
  };
  const { context, stage } = bootTools(guard, { tool: 'select', appMode: 'motion', layers: [{ locked: false }] }, motion);
  context.Point = function Point(x, y) { this.x = x; this.y = y; };
  context.document.readyState = 'complete';
  vm.runInNewContext(source('src/js/select-bridge.js'), context, { filename: 'select-bridge.js' });
  const down = event(); down.button = 0; down.clientX = 1; down.clientY = 2;
  stage.events.pointerdown(down);
  assert.equal(calls.down, 1);
  active.value = true;
  context.onMouseDrag({ event: event() });
  context.onMouseUp({ event: event() });
  assert.deepEqual(calls, { down: 1, drag: 0, up: 0 });
  assert.equal(releases.length, 1);
  active.value = false;
  context.onMouseDrag({ event: event() });
  context.onMouseUp({ event: event() });
  assert.deepEqual(calls, { down: 1, drag: 1, up: 1 });
});
