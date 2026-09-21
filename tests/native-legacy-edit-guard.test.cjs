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

test('legacy ownership is pass-through without installing a native controller', () => {
  const guard = freshGuard();
  assert.equal(guard.allow('draw'), true);
  assert.equal(guard.isExactReleaseReceipt(exactReceipt()), true);
  assert.equal(guard.isExactReleaseReceipt({ ...exactReceipt(), extra: true }), false);
});

test('native ownership synchronously denies the current call and releases at most once', () => {
  const guard = freshGuard();
  let calls = 0;
  let native = true;
  guard.install({
    getNativeIdentity: () => native ? { documentId: 'document-1', generation: 4 } : null,
    requestRelease: () => { calls++; return exactReceipt(); },
  });
  assert.equal(guard.allow('draw'), false, 'the release-requesting stack cannot mutate');
  assert.equal(calls, 1);
  assert.equal(guard.allow('draw'), false, 'the current native owner remains fail-closed after a receipt');
  assert.equal(calls, 1);
  native = false;
  assert.equal(guard.allow('draw'), true, 'only a later/replayed legacy-owned call may enter');
});

test('failed and indeterminate releases remain fail-closed without another request', async () => {
  for (const result of [null, { status: 'released' }, Promise.reject(new Error('cleanup failed'))]) {
    const guard = freshGuard();
    let calls = 0;
    guard.install({
      getNativeIdentity: () => ({ documentId: 'document-1', generation: 4 }),
      requestRelease: () => { calls++; return result; },
    });
    assert.equal(guard.allow('fill'), false);
    await Promise.resolve();
    assert.equal(guard.allow('fill'), false);
    assert.equal(calls, 1);
  }
});

test('wrong receipts and a second native admission cannot authorize a bypass', () => {
  const guard = freshGuard();
  let identity = { documentId: 'document-1', generation: 4 };
  let calls = 0;
  guard.install({
    getNativeIdentity: () => identity,
    requestRelease: () => {
      calls++;
      return { documentId: 'other-document', generation: 4, owner: 'legacy', status: 'released' };
    },
  });
  assert.equal(guard.allow('shape'), false);
  identity = null;
  assert.equal(guard.allow('shape'), false, 'a mismatched receipt remains fail-closed even after reported legacy ownership');
  identity = { documentId: 'document-2', generation: 5 };
  assert.equal(guard.allow('shape'), false, 'a later native authority begins a distinct denied cycle');
  assert.equal(calls, 2);
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
  assert.equal(releases, 1, `${file} must request exactly one release before its first mutation`);
  assert.ok(first.stopped >= 1, `${file} must stop the native-owned callback stack`);
  assert.ok(first.prevented >= 1, `${file} must prevent the native-owned callback default`);
  const second = event();
  callback(second);
  assert.equal(releases, 1, `${file} must not retry an indeterminate release`);
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
    state: { tool: 'draw', playing: false, layers: [{ locked: false }], activeLayerIdx: 0, brushSize: 10 },
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
  const eraserHandlers = {};
  context.document.getElementById = () => ({ addEventListener(type, handler) { eraserHandlers[type] = handler; } });
  vm.runInNewContext(fs.readFileSync(path.join(ROOT, 'src/js/eraser-bridge.js'), 'utf8'), context);
  eraserHandlers.pointermove(event());
  eraserHandlers.pointerup(event());
  assert.equal(releases, 0, 'eraser hover and idle pointerup are not document edits');
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
    assert.equal(releases, 1, `${file}:${helper} must request release`);
    assert.equal(downstream, 0, `${file}:${helper} must not reach Paper/document state before release`);
    context.__n19cHelper(...args);
    assert.equal(releases, 1, `${file}:${helper} must not retry an indeterminate release`);
    assert.equal(downstream, 0, `${file}:${helper} must stay closed after an indeterminate release`);
  }
});

test('every owned callback and commit helper contains the pre-mutation guard', () => {
  const expectations = {
    'src/js/draw-bridge.js': ['function onDown(e) {', 'if (!allowLegacyEdit(e)) return;', 'function onUp(e) {', 'function commitStroke() { if (!allowLegacyEdit()) return;'],
    'src/js/fill-bridge.js': ['function onDown(e) {\n    if (!shouldIntercept()) return;\n    if (!allowLegacyEdit(e)) return;', 'function onUp(e) {\n    if (!_fillCloseDrag) return;\n    if (!allowLegacyEdit(e)) return;', 'function onPropagateClick() {\n    if (!allowLegacyEdit()) return;'],
    'src/js/pen-bridge.js': ['function onDown(e) {\n    if (!shouldIntercept()) return;\n    if (!allowLegacyEdit(e)) return;', 'function onMove(e) {\n    if (!shouldIntercept()) return;\n    if (draggingHandle && !allowLegacyEdit(e)) return;', 'if (!draggingHandle) return;\n    if (!allowLegacyEdit(e)) return;'],
    'src/js/shape-bridge.js': ['function onDown(e) {\n    if (!shouldIntercept()) return;\n    if (!allowLegacyEdit(e)) return;', 'function onUp(e) {\n    if (!dragging) return;\n    if (!allowLegacyEdit(e)) return;', 'function commitShape(ex, ey) {\n    if (!allowLegacyEdit()) return;'],
    'src/js/eraser-bridge.js': ['function eraseAt(pt, radius) {\n    if (!allowLegacyEdit()) return;', 'function onDown(e) {', 'if (!allowLegacyEdit(e)) return;', 'if (!pointerIsDown) return;\n    if (!allowLegacyEdit(e)) return;', 'function onUp(e) {'],
  };
  for (const [file, snippets] of Object.entries(expectations)) {
    const source = fs.readFileSync(path.join(ROOT, file), 'utf8');
    for (const snippet of snippets) assert.ok(source.includes(snippet), `${file} lost ${snippet}`);
  }
});
