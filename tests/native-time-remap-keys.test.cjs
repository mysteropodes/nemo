'use strict';
// #2377 / #2524: Time Remap keys are selectable, movable, deletable and tintable timeline keys; every gesture is ONE placement key edit.
// The range fixture is the real one of native-layer-range-desktop.test.cjs (layer-inout handlers, range grid, property key editor).
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const fixture = fs.readFileSync(path.join(__dirname, 'native-layer-range-desktop.test.cjs'), 'utf8');
const begin = fixture.indexOf('// BEGIN REUSABLE REAL RANGE FIXTURE'), end = fixture.indexOf('// END REUSABLE REAL RANGE FIXTURE', begin);
assert.ok(begin >= 0 && end > begin);
const { harness, turn } = require('node:vm').compileFunction(fixture.slice(begin, end) + '\nreturn {harness,turn};', ['require', '__dirname'])(require, __dirname);
const Range = require('../src/js/adapters/native-layer-range-desktop.js');
const Remap = require('../src/js/adapters/native-time-remap-keys.js');
const KeyEdit = require('../src/js/adapters/native-motion-key-edit.js');
const KeyGesture = require('../src/js/adapters/native-motion-key-gesture.js');

// Layer 1 ("beta") carries a remapped video: keys at 2, 6 and 9. `result` is what the placement edit answers.
async function remapHarness(options = {}) {
  const h = await harness(options), intents = [], answer = { value: true };
  const keys = [{ frame: 2, value: 0 }, { frame: 6, value: 10, color: '#4ea9ff' }, { frame: 9, value: 20, hold: true }];
  Object.assign(h.root, { NemoNativeTimeRemapKeys: Remap, NemoNativeMotionKeyEdit: KeyEdit, NemoNativeMotionKeyGesture: KeyGesture });
  h.root.SM = {
    timeRemapKeyFacts: (li) => (li === 1 && keys.length ? { keyCount: keys.length, keyAtFrame: false, value: 3, keys: keys.map((k) => ({ ...k })) } : null),
    // The confirmed read follows an applied edit, as the engine's would.
    timeRemapKeysIntent: async (li, selections, action) => {
      intents.push({ li, selections, action });
      if (!answer.value) return false;
      const named = (k) => selections.some((s) => s.frame === k.frame || (s.ranges || []).some((r) => k.frame >= r[0] && k.frame <= r[1]));
      if (action.kind === 'delete') keys.splice(0, keys.length, ...keys.filter((k) => !named(k)));
      if (action.kind === 'shift') keys.forEach((k) => { if (named(k)) k.frame += action.delta; });
      return true;
    }
  };
  h.ports.editSelectedKeys = () => Promise.resolve(true);
  Remap.install(h.root, () => true);
  Range.refreshKeys(h.root);
  return Object.assign(h, { intents, answer, remapNodes: () => h.keyNodes().filter((n) => n.dataset.keyTrack.includes('timeRemap')), propNodes: () => h.keyNodes().filter((n) => !n.dataset.keyTrack.includes('timeRemap')),
    selectedRemap: () => h.keyNodes().filter((n) => n.dataset.keyTrack.includes('timeRemap') && n.dataset.selected === 'true').map((n) => Number(n.dataset.frame)),
    press: (key, extra = {}) => { const seen = { defaultPrevented: false, stopped: false }; h.doc.fire('keydown', { key, target: h.doc.body, preventDefault() { seen.defaultPrevented = true; }, stopImmediatePropagation() { seen.stopped = true; }, ...extra }); return seen; } });
}
const flush = async () => { for (let i = 0; i < 5; i++) await turn(); };

test('a press selects a remap key, Ctrl/Cmd adds one, and the property key selection is cleared (and the reverse)', async () => {
  const h = await remapHarness({ propertyKeys: [2.25, 7] });
  assert.equal(h.remapNodes().length, 3);
  h.propNodes()[0].fire('mousedown', { button: 0 });
  assert.deepEqual(h.keyEditor.captureSelectedKeys().keys.map((k) => k.frame), [2.25]);
  h.remapNodes()[0].fire('mousedown', { button: 0 });
  assert.deepEqual(h.selectedRemap(), [2]);
  assert.deepEqual(h.keyEditor.captureSelectedKeys().keys, [], 'selecting a remap key clears the property keys');
  h.remapNodes()[1].fire('mousedown', { button: 0, ctrlKey: true });
  assert.deepEqual(h.selectedRemap(), [2, 6]);
  h.propNodes()[1].fire('mousedown', { button: 0 });
  assert.deepEqual(h.selectedRemap(), [], 'selecting a property key clears the remap keys');
  assert.deepEqual(h.keyEditor.captureSelectedKeys().keys.map((k) => k.frame), [7]);
  assert.deepEqual(h.intents, [], 'selecting writes nothing');
  assert.equal(h.commands().length, 0);
});

test('Delete over selected remap keys is one placement key edit that names those keys, and the property editor never sees it', async () => {
  const h = await remapHarness({ propertyKeys: [2.25] });
  let property = 0; h.ports.editSelectedKeys = () => { property++; return Promise.resolve(true); };
  const idle = h.press('Delete');
  assert.equal(idle.defaultPrevented, false, 'without a remap selection the press is left to its other owners');
  h.remapNodes()[0].fire('mousedown', { button: 0 }); h.remapNodes()[2].fire('mousedown', { button: 0, metaKey: true });
  const event = h.press('Delete');
  assert.ok(event.defaultPrevented && event.stopped);
  await flush();
  assert.deepEqual(h.intents, [{ li: 1, selections: [{ frame: 2 }, { frame: 9 }], action: { kind: 'delete' } }]);
  assert.equal(property, 0);
  assert.deepEqual(h.selectedRemap(), [], 'the deleted keys leave the selection');
});

test('Alt+Right nudges the selected remap keys by one frame and the selection follows them; a refusal keeps it', async () => {
  const h = await remapHarness();
  h.remapNodes()[1].fire('mousedown', { button: 0 });
  h.press('ArrowRight', { altKey: true }); await flush();
  assert.deepEqual(h.intents.pop(), { li: 1, selections: [{ frame: 6 }], action: { kind: 'shift', delta: 1 } });
  assert.deepEqual(h.selectedRemap(), [7]);
  h.answer.value = false;
  h.press('ArrowLeft', { altKey: true }); await flush();
  assert.deepEqual(h.intents.pop(), { li: 1, selections: [{ frame: 7 }], action: { kind: 'shift', delta: -1 } });
  assert.deepEqual(h.selectedRemap(), [7], 'a refused edit leaves the selection where it was');
  h.press('F9'); await flush();
  assert.deepEqual(h.intents.pop().action, { kind: 'preset', preset: 'ease' });
  assert.equal(h.commands().length, 0);
});

test('dragging a selected remap key previews and commits ONE shift on release, bounded by its neighbours', async () => {
  const h = await remapHarness();
  h.remapNodes()[0].fire('mousedown', { button: 0, clientX: 100 });
  h.doc.fire('mousemove', { clientX: 130 });
  assert.equal(h.remapNodes()[0].style.transform, 'translateX(30px)');
  assert.deepEqual(h.intents, [], 'nothing is written while dragging');
  h.doc.fire('mouseup', { clientX: 130 }); await flush();
  assert.deepEqual(h.intents, [{ li: 1, selections: [{ frame: 2 }], action: { kind: 'shift', delta: 3 } }]);
  h.intents.length = 0;
  h.remapNodes()[0].fire('mousedown', { button: 0, clientX: 100 });
  h.doc.fire('mousemove', { clientX: 170 }); // 7 frames would land on the key at 9 after a shift of 7: the preview stays at the last valid frame
  h.doc.fire('mouseup', { clientX: 170 }); await flush();
  assert.ok(h.intents.every((i) => i.action.delta !== 7), 'a drop onto another key is never sent');
});

test('the key menu offers no subdivide or lock for remap keys, and its delete and tint are one edit each', async () => {
  const h = await remapHarness();
  h.remapNodes()[0].fire('mousedown', { button: 0 }); h.remapNodes()[1].fire('mousedown', { button: 0, ctrlKey: true });
  h.remapNodes()[1].fire('contextmenu', { clientX: 40, clientY: 50, button: 2 });
  const menu = h.root.menus.pop(), labels = menu.items.map((i) => i.label);
  assert.ok(labels.includes('Distribute evenly') && labels.includes('Color the keys…'));
  assert.ok(!labels.includes('Subdivide (key at midpoint)') && !labels.some((l) => /Lock keyframes/.test(l || '')));
  menu.items.find((i) => i.label === 'Color the keys…').action();
  h.root.menus.pop().items.find((i) => i.label === 'Pink').action(); await flush();
  assert.deepEqual(h.intents.pop(), { li: 1, selections: [{ frame: 2 }, { frame: 6 }], action: { kind: 'color', color: '#ff6b8b' } });
  h.remapNodes()[2].fire('contextmenu', { clientX: 40, clientY: 50, button: 2 });
  const single = h.root.menus.pop();
  assert.deepEqual(h.selectedRemap(), [9], 'a menu on an unselected key selects it alone');
  single.items.find((i) => i.label === 'Delete this key').action(); await flush();
  assert.deepEqual(h.intents.pop(), { li: 1, selections: [{ frame: 9 }], action: { kind: 'delete' } });
});

test('a long selection names runs of keys, a selection on another layer replaces the first, and a lost lane drops the selection', async () => {
  const h = await remapHarness();
  const many = Array.from({ length: 40 }, (_, i) => ({ frame: i, value: i }));
  h.root.SM.timeRemapKeyFacts = (li) => (li === 1 ? { keyCount: 40, keyAtFrame: false, value: 0, keys: many } : null);
  h.root.state.totalFrames = 40;
  Range.refreshKeys(h.root);
  h.remapNodes()[0].fire('mousedown', { button: 0 }); // the key buttons are redrawn after every selection
  for (let i = 1; i <= 34; i++) h.remapNodes()[i].fire('mousedown', { button: 0, ctrlKey: true });
  h.remapNodes()[20].fire('mousedown', { button: 0, ctrlKey: true }); // toggled off: a gap splits the run
  assert.equal(h.selectedRemap().length, 34);
  h.press('Delete'); await flush();
  const sent = h.intents.pop();
  assert.deepEqual(sent.selections, [{ ranges: [[0, 19], [21, 34]] }]);
  h.root.SM.timeRemapKeyFacts = () => null; Range.refreshKeys(h.root);
  assert.deepEqual(Remap.selected(h.root), [], 'no lane: no selection');
});

test('the key route of a remap selection is the placement edit; with none selected the property route is used', async () => {
  const h = await remapHarness({ propertyKeys: [2.25] });
  const real = []; const route = Remap.route(h.root, (a) => { real.push(a); return Promise.resolve(true); });
  await route({ kind: 'delete' }); assert.deepEqual(real, [{ kind: 'delete' }]);
  h.remapNodes()[0].fire('mousedown', { button: 0 });
  await route({ kind: 'flip' });
  assert.deepEqual(h.intents, [], 'one key cannot be flipped');
  assert.match(h.root.notices.join('|'), /at least 2/);
  assert.equal(await route({ kind: 'copy' }), null);
  assert.equal(real.length, 1);
});

test('layer markers and effect-stack keys are snap candidates beside the keys, labelled as the baseline does', async () => {
  const h = await remapHarness({ propertyKeys: [3], rangeExtras: { markers: [{ frame: 10, name: 'Cut' }, { frame: 6 }], effectKeys: [8, 11] } });
  const wrap = h.doc.createElement('div'); h.doc.body.appendChild(wrap);
  h.doc.getElementById = (id) => (id === 'fg-wrap' ? wrap : wrap.children.find((c) => c.id === id) || null);
  h.root.SM = {};
  Range.refreshKeys(h.root);
  const edits = []; h.ports.editSelectedKeys = (a) => { edits.push(a); return Promise.resolve(true); };
  const drag = (to) => { h.keyNodes()[0].fire('mousedown', { button: 0, clientX: 100 }); h.doc.fire('mousemove', { clientX: 100 + to * 10 }); const label = wrap.children[0] && wrap.children[0].dataset.label; h.doc.fire('mouseup', { clientX: 100 + to * 10 }); return label; };
  assert.equal(drag(7), 'Cut · 11', 'the layer marker at 10 is the target of a key dropped on it');
  assert.deepEqual(edits.pop(), { kind: 'shift', delta: 7 });
  assert.equal(drag(5), 'Clé · 9', 'an effect key at 8 is a target');
  assert.deepEqual(edits.pop(), { kind: 'shift', delta: 5 });
  assert.equal(drag(3), 'Repère calque · 7', 'an unnamed layer marker is labelled Repère calque');
  assert.deepEqual(edits.pop(), { kind: 'shift', delta: 3 });
  assert.deepEqual(KeyGesture.snapCandidates({ state: { currentFrame: 0 } }, { tracks: [], extra: [{ frame: 5, label: 'X' }, { frame: 99, label: 'out of range' }] }, 12).map((c) => c.frame), [0, 5]);
});

test('the property key editor does not answer an interpolation shortcut with "no selection" while remap keys hold the selection', async () => {
  const h = await remapHarness();
  const seen = { noSelection: 0, edits: 0 };
  const owner = KeyEdit.install(h.doc, { ownsNativeMotion: () => true, hasSelection: () => false, edit: () => { seen.edits++; return Promise.resolve(true); },
    acceptsTarget: () => true, modalOpen: () => false, unavailable() {}, noSelection: () => { seen.noSelection++; } });
  h.press('F9');
  assert.equal(seen.noSelection, 1, 'with nothing selected the shortcut says so');
  h.remapNodes()[0].fire('mousedown', { button: 0 });
  h.press('F9'); await flush();
  assert.equal(seen.noSelection, 1, 'a remap selection takes the press');
  assert.deepEqual(h.intents.pop(), { li: 1, selections: [{ frame: 2 }], action: { kind: 'preset', preset: 'ease' } });
  assert.equal(seen.edits, 0);
  owner.dispose();
});
