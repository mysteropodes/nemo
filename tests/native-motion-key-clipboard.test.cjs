'use strict';
// #2524 / #2377: Motion key copy and paste on a native project (baseline motion.js copySelectedKeys / pasteKeys and the timeline.js key branch).
const test = require('node:test'), assert = require('node:assert/strict');
const Clip = require('../src/js/adapters/native-motion-key-clipboard.js');
const KeyEdit = require('../src/js/adapters/native-motion-key-edit.js');

const A = { contextId: 'scene-root', layerUid: 'A' }, B = { contextId: 'scene-root', layerUid: 'B', strokeId: 'ink' };
const LINEAR = [{ x: 0, y: 0 }, { x: 1, y: 1 }];
const view = () => ({ pin: { contentRevision: 1 }, tracks: [
  { target: A, property: 'position', items: [{ frame: 4, selected: true, v: [1, 2], curvePoints: LINEAR }, { frame: 6, selected: false, v: [9, 9] }, { frame: 10, selected: true, v: [3, 4], hold: true }] },
  { target: B, property: 'rotation', items: [{ frame: 8, selected: true, v: [45] }] },
  { target: A, property: 'timeRemap', inert: true, items: [{ frame: 5, selected: true, v: [3] }] }] });

test('the packet keeps each selected key relative to the earliest one with its value, hold, curve and holder', () => {
  Clip.clear();
  const packet = Clip.build(view());
  assert.equal(packet.count, 3, 'the remap lane and unselected keys are not copied');
  assert.deepEqual(packet.tracks, [
    { target: A, property: 'position', keys: [{ dt: 0, v: [1, 2], curvePoints: LINEAR }, { dt: 6, v: [3, 4], hold: true }] },
    { target: B, property: 'rotation', keys: [{ dt: 4, v: [45] }] }]);
  assert.equal(Clip.build({ tracks: [{ target: A, property: 'position', items: [{ frame: 1, selected: false, v: [0, 0] }] }] }), null);
  assert.notEqual(packet.tracks[0].target, A, 'the packet owns its copy');
});

test('the paste payload sends a source layer that is gone to the active layer, as the baseline did', () => {
  const packet = Clip.build(view());
  const kept = Clip.payload(packet, 20, ['A', 'B'], { contextId: 'scene-root', layerUid: 'A' });
  assert.deepEqual(kept, { command: 'object.property.keys.paste', frame: 20, tracks: packet.tracks });
  const moved = Clip.payload(packet, 20, ['A'], { contextId: 'scene-root', layerUid: 'A' });
  assert.deepEqual(moved.tracks.map((t) => [t.target, t.property]), [[A, 'position'], [{ contextId: 'scene-root', layerUid: 'A' }, 'rotation']]);
  const merged = Clip.payload(packet, 0, ['C'], { contextId: 'scene-root', layerUid: 'C' });
  assert.equal(merged.tracks.length, 2, 'two properties of the fallback layer stay two tracks');
  const same = Clip.payload({ tracks: [{ target: A, property: 'position', keys: [{ dt: 0, v: [1, 1] }] }, { target: { contextId: 'scene-root', layerUid: 'B' }, property: 'position', keys: [{ dt: 2, v: [2, 2] }] }] }, 5, ['C'], { contextId: 'scene-root', layerUid: 'C' });
  assert.deepEqual(same.tracks, [{ target: { contextId: 'scene-root', layerUid: 'C' }, property: 'position', keys: [{ dt: 0, v: [1, 1] }, { dt: 2, v: [2, 2] }] }], 'one property from two gone layers becomes one track');
});

function harness(options = {}) {
  const log = { writes: [], reopens: [], notes: [] };
  const ports = {
    readKeyTimelines: () => (options.noView ? null : (options.view || view())),
    context: () => ({ frame: 20, layers: ['A', 'B'], active: { contextId: 'scene-root', layerUid: 'A' } }),
    async pasteKeys(payload) { log.writes.push(JSON.parse(JSON.stringify(payload))); if (options.fail) throw new Error('refused'); return { applied: options.applied !== false, historyEntriesAdded: 1 }; },
    async reopen(target, property, frames) { log.reopens.push({ target, property, frames }); },
    notify: (m) => log.notes.push(m), translate: (k, f) => f, localKeys: () => [1],
  };
  return { log, editor: KeyEdit.createEditor(ports) };
}

test('copy then paste is one write at the playhead; the drawers return and the pasted tracks open', async () => {
  Clip.clear();
  const h = harness();
  assert.equal(KeyEdit.hasClip(), false);
  assert.equal(await h.editor.edit({ kind: 'copy' }), true);
  assert.equal(KeyEdit.hasClip(), true);
  assert.equal(h.log.writes.length, 0, 'a copy writes nothing');
  assert.equal(await h.editor.edit({ kind: 'paste' }), true);
  assert.equal(h.log.writes.length, 1);
  assert.equal(h.log.writes[0].frame, 20);
  assert.deepEqual(h.log.writes[0].tracks.map((t) => [t.property, t.keys.map((k) => k.dt)]), [['position', [0, 6]], ['rotation', [4]]]);
  assert.deepEqual(h.log.reopens.map((r) => [r.property, r.frames]), [['position', [4, 10]], ['rotation', [8]], ['timeRemap', [5]]], 'the open drawers come back with their selection');
  Clip.clear();
});

test('a paste opens the pasted tracks that had no drawer so the new keys show', async () => {
  Clip.clear();
  const h = harness();
  await h.editor.edit({ kind: 'copy' });
  const empty = harness({ noView: true });
  assert.equal(await empty.editor.edit({ kind: 'paste' }), true);
  assert.deepEqual(empty.log.reopens.map((r) => [r.property, r.frames]), [['position', []], ['rotation', []]]);
  Clip.clear();
});

test('with nothing selected the copy resolves null and keeps the old packet; an empty clipboard pastes nothing', async () => {
  Clip.clear();
  const h = harness({ noView: true });
  assert.equal(await h.editor.edit({ kind: 'copy' }), null);
  assert.equal(await h.editor.edit({ kind: 'paste' }), null);
  assert.equal(h.log.writes.length, 0);
  const full = harness();
  await full.editor.edit({ kind: 'copy' });
  assert.equal(await h.editor.edit({ kind: 'copy' }), null);
  assert.equal(KeyEdit.hasClip(), true, 'a copy with no selection leaves the held packet');
  Clip.clear();
});

test('a refused paste says so and still brings the drawers back; an unchanged paste is not a failure', async () => {
  Clip.clear();
  const h = harness({ fail: true });
  await h.editor.edit({ kind: 'copy' });
  assert.equal(await h.editor.edit({ kind: 'paste' }), false);
  assert.deepEqual(h.log.notes, ['The keys could not be pasted there.']);
  assert.deepEqual(h.log.reopens.map((r) => r.property), ['position', 'rotation', 'timeRemap']);
  const same = harness({ applied: false });
  assert.equal(await same.editor.edit({ kind: 'paste' }), true);
  assert.deepEqual(same.log.reopens.map((r) => r.property), ['position', 'rotation', 'timeRemap'], 'no extra drawers when nothing was written');
  Clip.clear();
});

test('a selection beyond the paste bounds is refused at copy time and the held packet stays', async () => {
  Clip.clear();
  const big = { tracks: [{ target: A, property: 'position', items: Array.from({ length: Clip.MAX_KEYS + 1 }, (_, i) => ({ frame: i, selected: true, v: [i, i] })) }] };
  const h = harness({ view: big });
  assert.equal(await h.editor.edit({ kind: 'copy' }), false);
  assert.deepEqual(h.log.notes, ['The selection spans too many properties for one edit.']);
  assert.equal(KeyEdit.hasClip(), false);
});

test('a paste whose request would exceed the bound writes nothing', async () => {
  Clip.clear();
  const curves = Array.from({ length: 64 }, (_, i) => ({ x: i / 64, y: i / 64 }));
  const items = Array.from({ length: 200 }, (_, i) => ({ frame: i, selected: true, v: [i + 0.123456789, i + 0.987654321], curvePoints: curves }));
  const h = harness({ view: { tracks: [{ target: A, property: 'position', items }] } });
  assert.equal(await h.editor.edit({ kind: 'copy' }), false, 'copy checks the same bound');
  assert.equal(h.log.writes.length, 0);
});

function fakeDoc() {
  const listeners = [];
  return { listeners, addEventListener: (type, fn, capture) => listeners.push({ type, fn, c: capture }), removeEventListener: (type, fn) => { const i = listeners.findIndex((l) => l.fn === fn); if (i >= 0) listeners.splice(i, 1); } };
}

test('the arbiter asks the installed key owner, and only while the native project owns Motion', async () => {
  const doc = fakeDoc(), actions = [];
  let owns = true, selected = true;
  const handle = KeyEdit.install(doc, { ownsNativeMotion: () => owns, hasSelection: () => selected, acceptsTarget: () => true, modalOpen: () => false,
    edit: async (a) => { actions.push(a.kind); return true; } });
  assert.equal(KeyEdit.current(doc), handle);
  assert.equal(handle.hasSelection(), true);
  await handle.run({ kind: 'copy' });
  assert.deepEqual(actions, ['copy']);
  selected = false;
  assert.equal(KeyEdit.current(doc).hasSelection(), false);
  owns = false;
  assert.equal(KeyEdit.current(doc), null, 'off the native owner the legacy selection is read instead');
  assert.equal(KeyEdit.current({}), null);
  handle.dispose();
  assert.equal(KeyEdit.current(doc), null);
  assert.equal(await handle.run({ kind: 'paste' }), null, 'a retired owner does nothing');
});
