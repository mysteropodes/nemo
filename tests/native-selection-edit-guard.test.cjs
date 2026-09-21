'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const ROOT = path.resolve(__dirname, '..');
const GUARD = path.join(ROOT, 'src/js/application/native-edit-guard.js');

function source(file) {
  return fs.readFileSync(path.join(ROOT, file), 'utf8');
}

function freshGuard() {
  delete require.cache[require.resolve(GUARD)];
  return require(GUARD);
}

function event() {
  return {
    stopped: 0,
    prevented: 0,
    stopImmediatePropagation() { this.stopped++; },
    preventDefault() { this.prevented++; },
  };
}

function deniedPointerDown(file, tool) {
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
    state: { tool, playing: false, appMode: 'animation', layers: [{ locked: false }], activeLayerIdx: 0 },
    document: { readyState: 'complete', addEventListener() {}, getElementById: () => target },
    window: null,
  };
  context.window = context;
  context.SMEngineBridge = { isEnabled: () => true, nativeEditGuard: guard };
  vm.runInNewContext(source(file), context, { filename: file });
  const callback = handlers.pointerdown;
  assert.equal(typeof callback, 'function', `${file} did not install a pointerdown callback`);
  const first = event();
  callback(first);
  assert.equal(releases, 1, `${file} must request release before its first edit`);
  assert.ok(first.stopped >= 1, `${file} must stop the denied stack`);
  assert.ok(first.prevented >= 1, `${file} must prevent the denied default`);
  const second = event();
  callback(second);
  assert.equal(releases, 1, `${file} must not retry an indeterminate release`);
}

test('native-owned direct Selection and Subselection callbacks deny before mutation', () => {
  deniedPointerDown('src/js/select-bridge.js', 'select');
  deniedPointerDown('src/js/subselect-bridge.js', 'subselect');
});

test('N19D uses only N19C’s bridge port and preserves absent-port pass-through', () => {
  for (const file of [
    'src/js/select-bridge.js',
    'src/js/subselect-bridge.js',
    'src/js/tools.js',
    'src/js/shapes-panel.js',
  ]) {
    const text = source(file);
    assert.match(text, /bridge\.nativeEditGuard/);
    assert.doesNotMatch(text, /window\.SMNativeEditGuard/);
    assert.match(text, /hasOwnProperty\.call\(bridge,\s*['"]nativeEditGuard['"]\)/);
  }
});

test('every owned callback and public mutation helper gates before legacy mutation', () => {
  const expected = {
    'src/js/select-bridge.js': [
      "if (shouldIntercept() && !allowLegacySelectionEdit(e, 'select')) return;",
      "if (mode && !allowLegacySelectionEdit(e, 'select')) return;",
      "if ((mode || draggingArc) && !allowLegacySelectionEdit(e, 'select')) return;",
      "if (!allowLegacySelectionEdit(e, 'select')) return;",
      "if (!allowLegacySelectionEdit(null, 'select')) return;",
    ],
    'src/js/subselect-bridge.js': [
      'if (!allowLegacySelectionEdit(e)) return;',
      'if (!(_nmq.active || _nodeDrag.active)) return;\n    if (!allowLegacySelectionEdit(e)) return;',
    ],
    'src/js/tools.js': [
      "if((state.tool==='select'||state.tool==='subselect'||state.tool==='fsselect')&&!allowLegacySelectionEdit(event.event,state.tool))return;",
      "if(!allowLegacySelectionEdit(null,'fsselect'))return;",
      "if((state.tool==='select'||state.appMode==='motion')&&!allowLegacySelectionEdit(event.event,'select'))return;",
    ],
    'src/js/shapes-panel.js': [
      'function applySelection(li, strokeIds, additive) {\n    if (!allowLegacySelectionEdit()) return;',
      'function performPaintSwap(overRow) {\n    if (!allowLegacySelectionEdit()) return;',
      'function performMemberReorder(overRow) {\n    if (!allowLegacySelectionEdit()) return;',
      'function performDropInto(destGid) {\n    if (!allowLegacySelectionEdit()) return;',
      'function performReorder(overRow) {\n    if (!allowLegacySelectionEdit()) return;',
    ],
  };
  for (const [file, snippets] of Object.entries(expected)) {
    const text = source(file);
    for (const snippet of snippets) assert.ok(text.includes(snippet), `${file} lost ${snippet}`);
  }
});
