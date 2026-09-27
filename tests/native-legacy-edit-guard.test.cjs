'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const ROOT = path.resolve(__dirname, '..');
const GUARD = path.join(ROOT, 'src/js/application/native-edit-guard.js');

function freshGuard() {
  delete require.cache[require.resolve(GUARD)];
  return require(GUARD);
}

function exactReceipt() {
  return { documentId: 'document-1', generation: 4, owner: 'legacy', status: 'released' };
}

test('native, malformed, and unreadable authority deny without release and stay latched', () => {
  for (const mode of ['native', 'malformed', 'throws', 'documentId-getter', 'generation-getter', 'ownKeys']) {
    const guard = freshGuard();
    let identity = mode === 'native' ? { documentId: 'document-1', generation: 4 }
      : mode === 'malformed' ? { documentId: '', generation: -1 }
        : mode === 'documentId-getter' ? Object.defineProperties({}, {
          documentId: { enumerable: true, get() { throw new Error('documentId unavailable'); } },
          generation: { enumerable: true, value: 4 },
        })
          : mode === 'generation-getter' ? Object.defineProperties({}, {
            documentId: { enumerable: true, value: 'document-1' },
            generation: { enumerable: true, get() { throw new Error('generation unavailable'); } },
          })
            : mode === 'ownKeys' ? new Proxy({}, { ownKeys() { throw new Error('identity keys unavailable'); } })
              : { documentId: 'document-1', generation: 4 };
    let throwRead = mode === 'throws';
    let releases = 0;
    guard.install({
      getNativeIdentity() {
        if (throwRead) throw new Error('identity unavailable');
        return identity;
      },
      requestRelease() { releases++; return exactReceipt(); },
    });
    assert.equal(guard.allow('draw'), false, `${mode} authority denies synchronously`);
    assert.equal(releases, 0, `${mode} authority never requests release`);
    throwRead = false;
    identity = null;
    assert.equal(guard.allow('draw'), false, `${mode} denial cannot later reopen legacy writes`);
    assert.equal(releases, 0);
  }
});

test('legacy ownership is pass-through without installing a native controller', () => {
  const guard = freshGuard();
  assert.equal(guard.allow('draw'), true);
  assert.equal(guard.isExactReleaseReceipt(exactReceipt()), true);
  assert.equal(guard.isExactReleaseReceipt({ ...exactReceipt(), extra: true }), false);
});

test('an installed guard preserves legacy writes while native identity is dormant', () => {
  const guard = freshGuard();
  let releases = 0;
  guard.install({ getNativeIdentity: () => null, requestRelease() { releases++; } });
  assert.equal(guard.allow('draw'), true);
  assert.equal(releases, 0);
});

test('native ownership synchronously denies every legacy call without requesting release', () => {
  const guard = freshGuard();
  let calls = 0;
  let native = true;
  guard.install({
    getNativeIdentity: () => native ? { documentId: 'document-1', generation: 4 } : null,
    requestRelease: () => { calls++; return exactReceipt(); },
  });
  assert.equal(guard.allow('draw'), false, 'the native-owned stack cannot mutate');
  assert.equal(calls, 0);
  assert.equal(guard.allow('draw'), false, 'the current native owner remains fail-closed after a receipt');
  assert.equal(calls, 0);
  native = false;
  assert.equal(guard.allow('draw'), false, 'native-to-legacy identity transition stays denied');
  assert.equal(calls, 0);
});

test('malformed and throwing identity reads remain fail-closed without release', () => {
  for (const getNativeIdentity of [
    () => ({ documentId: '', generation: -1 }),
    () => { throw new Error('identity unavailable'); },
  ]) {
    const guard = freshGuard();
    let calls = 0;
    guard.install({
      getNativeIdentity,
      requestRelease: () => { calls++; return exactReceipt(); },
    });
    assert.equal(guard.allow('fill'), false);
    assert.equal(calls, 0);
    assert.equal(guard.allow('fill'), false);
    assert.equal(calls, 0);
  }
});

test('native identity changes and release-shaped results cannot authorize a bypass', () => {
  const guard = freshGuard();
  let identity = { documentId: 'document-1', generation: 4 };
  let calls = 0;
  guard.install({
    getNativeIdentity: () => identity,
    requestRelease: () => {
      calls++;
      return exactReceipt();
    },
  });
  assert.equal(guard.allow('shape'), false);
  identity = null;
  assert.equal(guard.allow('shape'), false, 'a release-shaped result cannot reopen legacy writes');
  identity = { documentId: 'document-2', generation: 5 };
  assert.equal(guard.allow('shape'), false, 'a later native authority begins a distinct denied cycle');
  assert.equal(calls, 0);
});

function event() {
  return {
    stopped: 0,
    prevented: 0,
    stopImmediatePropagation() { this.stopped++; },
    preventDefault() { this.prevented++; },
  };
}

function directCallback(file, tool) {
  const handlers = {};
  const target = { addEventListener(type, handler) { handlers[type] = handler; } };
  const guard = freshGuard();
  let releases = 0;
  guard.install({
    getNativeIdentity: () => ({ documentId: 'document-1', generation: 4 }),
    requestRelease: () => { releases++; return null; },
  });
  const context = {
    console,
    state: { tool, playing: false, layers: [{ locked: false }], activeLayerIdx: 0 },
    document: { readyState: 'complete', addEventListener() {}, getElementById: () => target },
    window: null,
  };
  context.window = context;
  context.SMEngineBridge = { isEnabled: () => true, nativeEditGuard: guard };
  vm.runInNewContext(fs.readFileSync(path.join(ROOT, file), 'utf8'), context, { filename: file });
  const callback = handlers.pointerdown;
  assert.equal(typeof callback, 'function', `${file} did not register its direct pointerdown callback`);
  const first = event();
  callback(first);
  assert.equal(releases, 0, `${file} must never request a native release`);
  assert.ok(first.stopped >= 1, `${file} must stop the native-owned callback stack`);
  assert.ok(first.prevented >= 1, `${file} must prevent the native-owned callback default`);
  const second = event();
  callback(second);
  assert.equal(releases, 0, `${file} must never request a native release`);
  assert.ok(second.stopped >= 1);
  assert.ok(second.prevented >= 1);
}

const OWNED_BRIDGES = [
  ['src/js/draw-bridge.js', 'draw'],
  ['src/js/fill-bridge.js', 'fill'],
  ['src/js/pen-bridge.js', 'pen'],
  ['src/js/shape-bridge.js', 'rect'],
  ['src/js/eraser-bridge.js', 'eraser'],
];

test('the compatibility port is optional only while absent and no bridge reads a second guard global', () => {
  for (const [file] of OWNED_BRIDGES) {
    const original = fs.readFileSync(path.join(ROOT, file), 'utf8');
    assert.doesNotMatch(original, /window\.SMNativeEditGuard/);
    const source = original.replace(
      /\n  if \(document\.readyState === 'loading'\) document\.addEventListener\('DOMContentLoaded', init\);\n  else init\(\);\n}\)\(\);\s*$/,
      '\n  window.__n19cAllow = allowLegacyEdit;\n})();\n',
    );
    assert.notEqual(source, original, `${file} guard hook was not installed`);
    const context = { window: null, SMEngineBridge: {} };
    context.window = context;
    vm.runInNewContext(source, context, { filename: file });
    const legacyEvent = event();
    assert.equal(context.__n19cAllow(legacyEvent), true, `${file} must pass through only while the port is absent`);
    assert.equal(legacyEvent.stopped, 0);
    assert.equal(legacyEvent.prevented, 0);
    for (const nativeEditGuard of [null, {}, { allow() { throw new Error('broken guard'); } }]) {
      context.SMEngineBridge.nativeEditGuard = nativeEditGuard;
      const denied = event();
      assert.equal(context.__n19cAllow(denied), false, `${file} must fail closed for a present malformed port`);
      assert.ok(denied.stopped >= 1);
      assert.ok(denied.prevented >= 1);
    }
  }
});

for (const [file, tool] of OWNED_BRIDGES) {
  test(`native-owned direct ${tool} callback stops before bridge mutation`, () => directCallback(file, tool));
}

test('idle pointer events and brush-resize gestures do not request native release', () => {
  const handlers = {};
  const target = { addEventListener(type, handler) { handlers[type] = handler; } };
  const guard = freshGuard();
  let releases = 0;
  guard.install({
    getNativeIdentity: () => ({ documentId: 'document-1', generation: 4 }),
    requestRelease: () => { releases++; return null; },
  });
  const context = {
    state: { tool: 'draw', playing: false, layers: [{ locked: false }], activeLayerIdx: 0, brushSize: 10, eraserSize: 20 },
    document: { readyState: 'complete', addEventListener() {}, getElementById: () => target },
    window: null,
  };
  context.window = context;
  context.SMEngineBridge = {
    isEnabled: () => true, screenToWorld: () => [0, 0], suspend() {}, setPressureCursor() {}, renderNow() {}, nativeEditGuard: guard,
  };
  vm.runInNewContext(fs.readFileSync(path.join(ROOT, 'src/js/draw-bridge.js'), 'utf8'), context);
  const resize = event(); resize.altKey = true; resize.clientX = 1;
  handlers.pointerdown(resize);
  assert.equal(releases, 0, 'Alt brush resize is not a document edit');

  context.state.tool = 'eraser';
  const cursorCalls = [];
  let hoverRenders = 0;
  context.SMEngineBridge.screenToWorld = () => [30, 40];
  context.SMEngineBridge.setEraserCursor = (point, radius) => cursorCalls.push([point, radius]);
  context.SMEngineBridge.renderNow = () => { hoverRenders++; };
  const eraserHandlers = {};
  context.document.getElementById = () => ({ addEventListener(type, handler) { eraserHandlers[type] = handler; } });
  vm.runInNewContext(fs.readFileSync(path.join(ROOT, 'src/js/eraser-bridge.js'), 'utf8'), context);
  eraserHandlers.pointermove(event());
  eraserHandlers.pointerup(event());
  assert.equal(releases, 0, 'eraser hover and idle pointerup are not document edits');
  assert.deepEqual(cursorCalls, [[[30, 40], 10]], 'eraser hover preserves its cursor update');
  assert.equal(hoverRenders, 1, 'eraser hover preserves its render request');
});

test('a retained draw gesture cannot finish after native authority activates', () => {
  const handlers = {};
  const target = { addEventListener(type, handler) { handlers[type] = handler; } };
  const guard = freshGuard();
  let identity = null;
  let releases = 0;
  let resumes = 0;
  let renders = 0;
  const nativeIdentity = { documentId: 'native-document', generation: 9 };
  guard.install({
    getNativeIdentity: () => identity,
    requestRelease: () => { releases++; return exactReceipt(); },
  });
  const bridge = {
    isEnabled: () => true,
    nativeEditGuard: guard,
    screenToWorld: (x, y) => [x, y],
    suspend() {},
    resume() { resumes++; },
    setPressureCursor() {},
    renderNow() { renders++; },
  };
  const context = {
    Date, Math, Point: class Point { constructor(x, y) { this.x = x; this.y = y; } },
    performance: { now: () => 1 },
    state: {
      tool: 'draw', playing: false, stabilizer: 0, brushSize: 8,
      pressureInvert: false, pressureMin: 0, pressureMax: 100,
      vectorBrush: false, bitmapBrushOn: false, strokeEnabled: true,
      fillBrushSize: 8, fillColor: '#000000', strokeColor: '#000000', opacity: 100,
    },
    view: { zoom: 1 },
    document: { readyState: 'complete', addEventListener() {}, getElementById: () => target },
    window: null,
    editRefusalReason: null,
  };
  context.window = context;
  context.SMEngineBridge = bridge;
  vm.runInNewContext(fs.readFileSync(path.join(ROOT, 'src/js/draw-bridge.js'), 'utf8'), context);

  const down = event(); down.clientX = 10; down.clientY = 20; down.pressure = 0.5;
  handlers.pointerdown(down);
  identity = nativeIdentity;
  const up = event(); up.clientX = 12; up.clientY = 22; up.pressure = 0.5;
  handlers.pointerup(up);

  assert.equal(releases, 0, 'completing the retained gesture must not request native release');
  assert.equal(resumes, 0, 'the denied pointerup must return before legacy gesture completion');
  assert.equal(renders, 0, 'the denied pointerup must not run post-commit rendering');
  assert.strictEqual(identity, nativeIdentity, 'the controller keeps the same native identity');
  assert.equal(guard.allow('draw'), false, 'the retained gesture cannot reopen after pointerup');
});

test('direct programmatic commit helpers deny before downstream Paper/document access', () => {
  for (const [file, helper, args] of [
    ['src/js/draw-bridge.js', 'commitStroke', []],
    ['src/js/fill-bridge.js', 'onPropagateClick', []],
    ['src/js/shape-bridge.js', 'commitShape', [1, 1]],
    ['src/js/eraser-bridge.js', 'eraseAt', [{}, 1]],
  ]) {
    const guard = freshGuard();
    let releases = 0, downstream = 0;
    guard.install({
      getNativeIdentity: () => ({ documentId: 'document-1', generation: 4 }),
      requestRelease: () => { releases++; return null; },
    });
    const context = { window: null, SMEngineBridge: { nativeEditGuard: guard } };
    context.window = context;
    Object.defineProperty(context, 'userLayers', { get() { downstream++; return []; } });
    const source = fs.readFileSync(path.join(ROOT, file), 'utf8').replace(
      /\n  if \(document\.readyState === 'loading'\) document\.addEventListener\('DOMContentLoaded', init\);\n  else init\(\);\n}\)\(\);\s*$/,
      `\n  window.__n19cHelper = ${helper};\n})();\n`,
    );
    assert.notEqual(source, fs.readFileSync(path.join(ROOT, file), 'utf8'), `${file} helper hook was not installed`);
    vm.runInNewContext(source, context, { filename: file });
    context.__n19cHelper(...args);
    assert.equal(releases, 0, `${file}:${helper} must not request release`);
    assert.equal(downstream, 0, `${file}:${helper} must not reach Paper/document state before release`);
    context.__n19cHelper(...args);
    assert.equal(releases, 0, `${file}:${helper} must not request release`);
    assert.equal(downstream, 0, `${file}:${helper} must stay closed after an indeterminate release`);
  }
});

test('every owned callback and commit helper contains the pre-mutation guard', () => {
  const expectations = {
    'src/js/draw-bridge.js': ['function onDown(e) {', 'if (!allowLegacyEdit(e)) return;', 'function onUp(e) {', 'function commitStroke() { if (!allowLegacyEdit()) return;'],
    'src/js/fill-bridge.js': ['function onDown(e) {\n    if (!shouldIntercept()) return;\n    if (!allowLegacyEdit(e)) return;', 'function onUp(e) {\n    if (!_fillCloseDrag) return;\n    if (!allowLegacyEdit(e)) return;', 'function onPropagateClick() {\n    if (!allowLegacyEdit()) return;'],
    'src/js/pen-bridge.js': ['function onDown(e) {\n    if (!shouldIntercept()) return;\n    if (!allowLegacyEdit(e)) return;', 'function onMove(e) {\n    if (!shouldIntercept()) return;\n    if (draggingHandle && !allowLegacyEdit(e)) return;', 'if (!draggingHandle) return;\n    if (!allowLegacyEdit(e)) return;'],
    'src/js/shape-bridge.js': ['function onDown(e) {\n    if (!shouldIntercept()) return;\n    if (!allowLegacyEdit(e)) return;', 'function onUp(e) {\n    if (!dragging) return;\n    if (!allowLegacyEdit(e)) return;', 'function commitShape(ex, ey) {\n    if (!allowLegacyEdit()) return;'],
    'src/js/eraser-bridge.js': ['function eraseAt(pt, radius) {\n    if (!allowLegacyEdit()) return;', 'function onDown(e) {', 'if (!allowLegacyEdit(e)) return;', 'if (pointerIsDown && !allowLegacyEdit(e)) return;', 'function onUp(e) {'],
  };
  for (const [file, snippets] of Object.entries(expectations)) {
    const source = fs.readFileSync(path.join(ROOT, file), 'utf8');
    for (const snippet of snippets) assert.ok(source.includes(snippet), `${file} lost ${snippet}`);
  }
});
