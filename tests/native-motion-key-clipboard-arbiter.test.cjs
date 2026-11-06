'use strict';
// #2524 / #2377: on a native project the arbiter's key-clipboard branch reads the native key selection and clipboard, never the legacy
// ones, and Cmd+C / Cmd+V run the native copy and paste (the real timeline.js onKeyDown, as in keyboard-arbitration-u05.test.cjs).
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const timeline = fs.readFileSync(path.join(__dirname, '../src/js/timeline.js'), 'utf8');
const start = timeline.indexOf('function onKeyDown(event){'), end = timeline.indexOf('function onKeyUp(event)', start);
assert.ok(start >= 0 && end > start);

function harness({ owned = true, selected = false, clip = false, legacySelected = false, legacyClip = false, paths = 0 } = {}) {
  const calls = [], call = (name) => () => calls.push(name);
  const doc = { id: 'document' };
  const owner = { hasSelection: () => selected, run: (a) => { calls.push('native-' + a.kind); return Promise.resolve(true); } };
  const ctx = { state: { appMode: 'motion' }, document: doc, selectedPaths: new Array(paths).fill({}), _sel: { frames: [], clipboard: [] }, _canvasClip: { snaps: [] },
    undo: call('undo'), redo: call('redo'), copySelection: call('copySelection'), cutSelection: call('cutSelection'), pasteSelection: call('pasteSelection'),
    SMProject: { save: call('save'), saveAs: call('saveAs') }, SM: { copyFrames: call('copyFrames'), cutFrames: call('cutFrames'), pasteFrames: call('pasteFrames'), convertActiveLayerToComponent: call('toComponent') },
    SMMotion: { hasKeySelection: () => { calls.push('legacy-selection-read'); return legacySelected; }, copySelectedKeys: call('legacy-copy'), hasKeyClipboard: () => { calls.push('legacy-clip-read'); return legacyClip; }, pasteKeys: call('legacy-paste') },
    NemoNativeMotionKeyEdit: { current: (d) => { assert.equal(d, doc, 'asked for this document'); return owned ? owner : null; }, hasClip: () => clip },
    NemoKeyboardArbiter: require('../src/js/application/keyboard-arbiter.js') };
  ctx.window = ctx;
  vm.runInNewContext(timeline.slice(start, end), ctx);
  const press = (key, mods = {}) => { let prevented = 0; ctx.onKeyDown(Object.assign({ key, target: { tagName: 'DIV' }, preventDefault() { prevented++; } }, mods)); return prevented; };
  return { calls, press };
}
const cmd = { metaKey: true };

test('Cmd+C with a native key selection copies the native keys and never reads the legacy selection', async () => {
  const h = harness({ selected: true, legacySelected: true, paths: 2 });
  assert.equal(h.press('c', cmd), 1);
  assert.deepEqual(h.calls, ['native-copy']);
});

test('with no native key selected Cmd+C falls to the canvas selection even when the legacy model holds a selection', () => {
  const h = harness({ selected: false, legacySelected: true, paths: 1 });
  h.press('c', cmd);
  assert.deepEqual(h.calls, ['copySelection']);
});

test('Cmd+V pastes the native key packet in Motion, whatever was copied last, and never reads the legacy clipboard', () => {
  const h = harness({ clip: true, legacyClip: true });
  assert.equal(h.press('v', cmd), 1);
  assert.deepEqual(h.calls, ['native-paste']);
  const none = harness({ clip: false, legacyClip: true });
  none.press('v', cmd);
  assert.deepEqual(none.calls, ['pasteFrames'], 'no native packet: the other clipboards decide, the legacy key clipboard stays unread');
});

test('off the native owner the legacy key clipboard keeps its meaning', () => {
  const h = harness({ owned: false, legacySelected: true });
  h.press('c', cmd);
  assert.deepEqual(h.calls, ['legacy-selection-read', 'legacy-clip-read', 'legacy-copy']);
  const p = harness({ owned: false, legacyClip: true });
  p.press('v', cmd);
  assert.deepEqual(p.calls, ['legacy-selection-read', 'legacy-clip-read', 'legacy-paste']);
});

test('Cmd+X never cuts keys, native or legacy', () => {
  const h = harness({ selected: true, clip: true });
  assert.equal(h.press('x', cmd), 1);
  assert.deepEqual(h.calls, ['cutFrames']);
});
