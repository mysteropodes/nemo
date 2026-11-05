'use strict';
// #2524 / #2377: the Motion key batch shortcuts, their pure mappings and the editor's single-write contract.
const test = require('node:test'), assert = require('node:assert/strict');
const KeyEdit = require('../src/js/adapters/native-motion-key-edit.js');

const ev = (o) => Object.assign({ key: '', code: '', shiftKey: false, metaKey: false, ctrlKey: false, altKey: false }, o);
const tgt = { contextId: 'scene-root', layerUid: 'A' };

test('the baseline shortcut mapping: F9 family, Cmd+Alt+K/H, Delete, Alt+arrows', () => {
  const map = (o) => { const a = KeyEdit.actionForEvent(ev(o)); return a && JSON.parse(JSON.stringify(a)); };
  assert.deepEqual(map({ key: 'F9' }), { kind: 'preset', preset: 'ease' });
  assert.deepEqual(map({ key: 'F9', shiftKey: true }), { kind: 'preset', preset: 'easeIn' });
  assert.deepEqual(map({ key: 'F9', shiftKey: true, metaKey: true }), { kind: 'preset', preset: 'easeOut' });
  assert.deepEqual(map({ key: 'F9', shiftKey: true, ctrlKey: true }), { kind: 'preset', preset: 'easeOut' });
  assert.deepEqual(map({ key: 'F9', metaKey: true }), { kind: 'preset', preset: 'ease' }, 'Cmd without Shift is still Easy Ease, as in the baseline');
  assert.deepEqual(map({ key: 'k', metaKey: true, altKey: true }), { kind: 'preset', preset: 'linear' });
  assert.deepEqual(map({ key: 'K', ctrlKey: true, altKey: true }), { kind: 'preset', preset: 'linear' });
  assert.deepEqual(map({ key: '˚', code: 'KeyK', metaKey: true, altKey: true }), { kind: 'preset', preset: 'linear' }, 'Option changes event.key on macOS; the physical key still counts');
  assert.deepEqual(map({ key: 'h', metaKey: true, altKey: true }), { kind: 'preset', preset: 'hold' });
  assert.deepEqual(map({ key: 'Delete' }), { kind: 'delete' });
  assert.deepEqual(map({ key: 'Backspace' }), { kind: 'delete' });
  assert.deepEqual(map({ key: 'ArrowRight', altKey: true }), { kind: 'shift', delta: 1 });
  assert.deepEqual(map({ key: 'ArrowLeft', altKey: true }), { kind: 'shift', delta: -1 });
  for (const o of [{ key: 'ArrowRight' }, { key: 'k' }, { key: 'k', metaKey: true }, { key: 'h', altKey: true }, { key: 'F8' }, { key: 'ArrowLeft', altKey: true, metaKey: true }]) assert.equal(KeyEdit.actionForEvent(ev(o)), null, JSON.stringify(o));
});

test('expectedFrames mirrors the engine so the selection can follow the keys', () => {
  assert.deepEqual(KeyEdit.expectedFrames({ kind: 'shift', delta: -3 }, [12, 4]), [1, 9]);
  assert.deepEqual(KeyEdit.expectedFrames({ kind: 'delete' }, [1, 2]), []);
  assert.deepEqual(KeyEdit.expectedFrames({ kind: 'flip' }, [9, 1]), [1, 9]);
  assert.deepEqual(KeyEdit.expectedFrames({ kind: 'distribute' }, [3, 0, 1]), [0, 2, 3], 'halves round up');
  assert.deepEqual(KeyEdit.expectedFrames({ kind: 'distribute' }, [0, 4, 12, 24]), [0, 8, 16, 24]);
  assert.deepEqual(KeyEdit.expectedFrames({ kind: 'distribute' }, [5]), [5]);
  assert.deepEqual(KeyEdit.expectedFrames({ kind: 'preset', preset: 'ease' }, [7, 2]), [2, 7]);
});

test('canShift keeps keys inside the composition and off keys that stay', () => {
  const track = { items: [{ frame: 0, selected: true }, { frame: 4, selected: true }, { frame: 6, selected: false }, { frame: 39, selected: false }] };
  assert.equal(KeyEdit.canShift([track], 1, 40), true);
  assert.equal(KeyEdit.canShift([track], 2, 40), false, '4 would land on the key at 6');
  assert.equal(KeyEdit.canShift([track], -1, 40), false, 'frame -1');
  assert.equal(KeyEdit.canShift([track], 4, 40), true, '0 lands on 4 that also moves');
  assert.equal(KeyEdit.canShift([{ items: [{ frame: 38, selected: true }] }], 2, 40), false, 'frame 40 is past the last frame');
  assert.equal(KeyEdit.canShift([{ items: [{ frame: 38, selected: true }] }], 1, 40), true);
});

function editorHarness(options = {}) {
  const log = { writes: [], reopens: [], notes: [] };
  let local = 1;
  const tracks = options.tracks || [{ target: tgt, property: 'opacity', items: [{ frame: 0, selected: true }, { frame: 4, selected: false }, { frame: 12, selected: true }] }];
  const ports = {
    readKeyTimelines: () => (options.noView ? null : { pin: { contentRevision: 1 }, tracks }),
    localKeys: () => [local],
    async editPropertyTrack(payload, pin, isCurrent) {
      log.writes.push({ payload: JSON.parse(JSON.stringify(payload)), pin, current: isCurrent() });
      if (options.fail) throw new Error('native refusal');
      return { applied: options.applied !== false, historyEntriesAdded: options.applied === false ? 0 : 1 };
    },
    async reopen(target, property, frames) { log.reopens.push({ target, property, frames }); },
    notify: (m) => log.notes.push(m),
    translate: (key, fallback) => fallback,
  };
  return { log, editor: KeyEdit.createEditor(ports), bump: () => { local++; }, ports };
}

test('one edit is one write over every selected key, then the drawers return with the moved selection', async () => {
  const h = editorHarness();
  assert.equal(await h.editor.edit({ kind: 'shift', delta: 3 }), true);
  assert.equal(h.log.writes.length, 1);
  assert.deepEqual(h.log.writes[0].payload, { command: 'object.property.track.edit', selections: [{ target: tgt, property: 'opacity', frame: 0 }, { target: tgt, property: 'opacity', frame: 12 }], action: { kind: 'shift', delta: 3 } });
  assert.equal(h.log.writes[0].current, true);
  assert.deepEqual(h.log.reopens, [{ target: tgt, property: 'opacity', frames: [3, 15] }]);
});

test('a refused or unchanged edit still restores the drawers on the old frames', async () => {
  const failed = editorHarness({ fail: true });
  assert.equal(await failed.editor.edit({ kind: 'shift', delta: 1 }), false);
  assert.deepEqual(failed.log.reopens[0].frames, [0, 12]);
  assert.match(failed.log.notes.join('|'), /cannot move there/);
  const same = editorHarness({ applied: false });
  assert.equal(await same.editor.edit({ kind: 'shift', delta: 0 }), true);
  assert.deepEqual(same.log.reopens[0].frames, [0, 12]);
  const del = editorHarness();
  assert.equal(await del.editor.edit({ kind: 'delete' }), true);
  assert.deepEqual(del.log.reopens[0].frames, [], 'deleted keys leave nothing to select');
});

test('no selected key resolves null; flip and distribute need two; more than 32 are refused; nothing is written', async () => {
  const none = editorHarness({ tracks: [{ target: tgt, property: 'opacity', items: [{ frame: 0, selected: false }] }] });
  assert.equal(await none.editor.edit({ kind: 'delete' }), null);
  assert.equal(await editorHarness({ noView: true }).editor.edit({ kind: 'delete' }), null);
  const one = editorHarness({ tracks: [{ target: tgt, property: 'opacity', items: [{ frame: 0, selected: true }] }] });
  assert.equal(await one.editor.edit({ kind: 'flip' }), false);
  assert.equal(await one.editor.edit({ kind: 'distribute' }), false);
  assert.deepEqual(one.log.notes, ['Select at least 2 keys', 'Select at least 2 keys']);
  const many = editorHarness({ tracks: [{ target: tgt, property: 'opacity', items: Array.from({ length: 33 }, (_, i) => ({ frame: i, selected: true })) }] });
  assert.equal(await many.editor.edit({ kind: 'delete' }), false);
  for (const h of [none, one, many]) assert.equal(h.log.writes.length, 0);
  // A request over the 4096 byte bound is refused here rather than at the transport.
  const long = { contextId: 'scene-root', layerUid: 'L'.repeat(200), strokeId: 'S'.repeat(60) };
  const big = editorHarness({ tracks: [{ target: long, property: 'opacity', items: Array.from({ length: 20 }, (_, i) => ({ frame: i, selected: true })) }] });
  assert.equal(await big.editor.edit({ kind: 'delete' }), false);
  assert.equal(big.log.writes.length, 0);
});

test('the queued intent is current only while the local selection is unchanged', async () => {
  const h = editorHarness();
  let judged = null;
  h.ports.editPropertyTrack = async (payload, pin, isCurrent) => { judged = [isCurrent(), (h.bump(), isCurrent())]; return { applied: true, historyEntriesAdded: 1 }; };
  const editor = KeyEdit.createEditor(h.ports);
  await editor.edit({ kind: 'preset', preset: 'ease' });
  assert.deepEqual(judged, [true, false]);
});

function fakeDocument() {
  const listeners = [];
  return { listeners, addEventListener: (t, f, c) => listeners.push({ t, f, c }), removeEventListener: (t, f) => { const i = listeners.findIndex((l) => l.f === f); if (i >= 0) listeners.splice(i, 1); } };
}
function press(doc, o) {
  const event = ev(Object.assign({ target: { tagName: 'DIV' }, defaultPrevented: false, prevented: false, stopped: false }, o));
  event.preventDefault = () => { event.prevented = true; }; event.stopImmediatePropagation = () => { event.stopped = true; };
  doc.listeners.filter((l) => l.t === 'keydown').forEach((l) => l.f(event));
  return event;
}
function installHarness(over = {}) {
  const doc = fakeDocument(), edits = [], flags = { unavailable: 0 };
  const ports = Object.assign({ ownsNativeMotion: () => true, hasSelection: () => true, acceptsTarget: () => true, modalOpen: () => false,
    unavailable: () => { flags.unavailable++; }, edit: (a) => { edits.push(a); return Promise.resolve(true); } }, over);
  return { doc, edits, flags, handle: KeyEdit.install(doc, ports) };
}

test('the keyboard route is a capture listener that takes a press only with a key selection', async () => {
  const h = installHarness();
  assert.equal(h.doc.listeners[0].c, true);
  const e = press(h.doc, { key: 'F9', shiftKey: true });
  assert.equal(e.prevented && e.stopped, true);
  await Promise.resolve();
  assert.deepEqual(h.edits, [{ kind: 'preset', preset: 'easeIn' }]);
  press(h.doc, { key: 'Delete' }); press(h.doc, { key: 'ArrowLeft', altKey: true });
  assert.deepEqual(h.edits.map((a) => a.kind), ['preset', 'delete', 'shift']);
  h.handle.dispose();
  assert.equal(h.doc.listeners.length, 0);
});

test('with no key selected, in a field, in a modal, off the timeline or off native Motion the press is left alone', () => {
  for (const [over, target] of [[{ hasSelection: () => false }], [{ ownsNativeMotion: () => false }], [{ modalOpen: () => true }], [{ acceptsTarget: () => false }],
    [{ hasSelection: () => { throw new Error('x'); } }], [{}, { tagName: 'INPUT' }], [{}, { tagName: 'DIV', isContentEditable: true }]]) {
    const h = installHarness(over);
    const e = press(h.doc, { key: 'Delete', target: target || { tagName: 'DIV' } });
    assert.equal(e.prevented || e.stopped, false, JSON.stringify(Object.keys(over)));
    assert.deepEqual(h.edits, []);
  }
  const h = installHarness();
  const e = press(h.doc, { key: 'Delete', defaultPrevented: true });
  assert.equal(e.stopped, false);
  const plain = press(h.doc, { key: 'ArrowRight' });
  assert.equal(plain.prevented, false, 'bare arrows keep moving the playhead');
});

test('an interpolation shortcut with no key selected says so; Delete and the arrows are left to their other owners', () => {
  const notes = [];
  const h = installHarness({ hasSelection: () => false, noSelection: () => notes.push('none') });
  const f9 = press(h.doc, { key: 'F9' });
  assert.equal(f9.prevented && f9.stopped, true);
  const del = press(h.doc, { key: 'Delete' });
  const nudge = press(h.doc, { key: 'ArrowRight', altKey: true });
  assert.equal(del.prevented || del.stopped || nudge.prevented || nudge.stopped, false);
  assert.deepEqual(notes, ['none']);
  assert.deepEqual(h.edits, []);
  const off = installHarness({ hasSelection: () => false, ownsNativeMotion: () => false, noSelection: () => notes.push('off') });
  assert.equal(press(off.doc, { key: 'F9' }).prevented, false);
  assert.deepEqual(notes, ['none']);
});

test('an edit that finds nothing selected reports the route unavailable; a refusal does not', async () => {
  const h = installHarness({ edit: () => Promise.resolve(null) });
  press(h.doc, { key: 'F9' });
  await new Promise((r) => setImmediate(r));
  assert.equal(h.flags.unavailable, 1);
  const g = installHarness({ edit: () => Promise.resolve(false) });
  press(g.doc, { key: 'F9' });
  await new Promise((r) => setImmediate(r));
  assert.equal(g.flags.unavailable, 0);
  assert.throws(() => KeyEdit.install(g.doc, { ownsNativeMotion() {}, hasSelection() {}, edit() {} }), /already owned/);
});
