'use strict';
// #2377: dragging a Motion key connector (baseline motion.js connector mousedown / onDragMove @1065e9d8).
const test = require('node:test'), assert = require('node:assert/strict');
const Gesture = require('../src/js/adapters/native-motion-key-gesture.js');
const Drag = require('../src/js/adapters/native-motion-connector-drag.js');

const target = { contextId: 'scene-root', layerUid: 'A' };
const address = (track, frame) => ({ target: { kind: 'property', target: track.target, property: track.property }, frame });
function harness(frames, a, b, altKey) {
  const listeners = [], doc = { addEventListener: (t, f) => listeners.push([t, f]), removeEventListener: (t, f) => { const i = listeners.findIndex((l) => l[0] === t && l[1] === f); if (i >= 0) listeners.splice(i, 1); } };
  const fire = (t, e) => listeners.filter((l) => l[0] === t).forEach((l) => l[1](e));
  const track = { target, property: 'position', items: frames.map((f) => ({ frame: f, selected: f === a || f === b })) };
  const log = { selections: [], edits: [], previews: [] };
  const nodes = [{ style: {}, frame: a }, { style: {}, frame: b }];
  const view = () => ({ tracks: [track], selectKeys(keys) { log.selections.push(keys.map((k) => k.frame)); return true; } });
  const handle = Drag.start({ gesture: Gesture, doc, event: { clientX: 100, altKey: !!altKey }, fc: 10, total: 40, track, a: track.items.find((k) => k.frame === a), b: track.items.find((k) => k.frame === b),
    address, view, edit: (action) => { log.edits.push(action); return Promise.resolve(true); }, refresh: () => log.previews.push('refresh'),
    nodes: (moving) => nodes.filter((n) => moving.includes(n.frame)), onEnd: () => { log.ended = true; } });
  return { handle, fire, log, nodes };
}

test('the pair may move until a key outside it, or an end of the timeline, is reached', () => {
  const items = [0, 4, 8, 12, 20].map((frame) => ({ frame }));
  const at = (frames, a, b, retime) => Drag.limits(frames, { frame: a }, { frame: b }, 40, retime);
  assert.deepEqual(at(items, 4, 8, false), { lo: -3, hi: 3 }, 'one frame short of the key before and of the key after');
  assert.deepEqual(at(items, 0, 4, false), { lo: 0, hi: 3 }, 'the first key cannot leave frame 0');
  assert.deepEqual(at([{ frame: 30 }, { frame: 38 }], 30, 38, false), { lo: -30, hi: 1 }, 'nor the last frame');
  assert.deepEqual(at(items, 4, 8, true), { lo: -3, hi: 3 }, 'a retime keeps the second key after the first and before the key after it');
  assert.deepEqual(at(items, 8, 12, true), { lo: -3, hi: 7 });
  assert.equal(at(items, 4, 9, false), null, 'keys that are not on the track cannot be dragged');
});

test('a drag moves both end keys by one shared delta: ONE shift on release, the pair stays selected', async () => {
  const h = harness([0, 4, 8, 12], 4, 8, false);
  h.fire('mousemove', { clientX: 120 });
  assert.deepEqual(h.nodes.map((n) => n.style.transform), ['translateX(20px)', 'translateX(20px)']);
  h.fire('mousemove', { clientX: 300 });
  assert.equal(h.handle.delta(), 3, 'the clamp stops the pair one frame before the key at 12');
  h.fire('mousemove', { clientX: 20 });
  assert.equal(h.handle.delta(), -3, 'and one frame after the key at 0');
  h.fire('mouseup', {});
  await Promise.resolve();
  assert.deepEqual(h.log.edits, [{ kind: 'shift', delta: -3 }]);
  assert.deepEqual(h.log.selections, [], 'a plain drag leaves the selection of the press alone');
  assert.equal(h.log.ended, true);
});

test('Alt+drag retimes: the first key stays, only the second key moves, and the pair is selected again after', async () => {
  const h = harness([0, 4, 8, 12], 4, 8, true);
  h.fire('mousemove', { clientX: 130 });
  assert.deepEqual(h.nodes.map((n) => n.style.transform), [undefined, 'translateX(30px)'], 'only the second key previews');
  h.fire('mousemove', { clientX: 10 });
  assert.equal(h.handle.delta(), -3, 'the second key never reaches the first');
  h.fire('mousemove', { clientX: 117 });
  h.fire('mouseup', {});
  await new Promise((r) => setTimeout(r, 0));
  assert.deepEqual(h.log.edits, [{ kind: 'shift', delta: 2 }]);
  assert.deepEqual(h.log.selections, [[8], [4, 10]], 'the second key alone for the edit, then the pair at its new frames');
});

test('an Escape or a click without a move commits nothing', async () => {
  const h = harness([0, 4, 8], 4, 8, false);
  h.fire('mousemove', { clientX: 150 });
  h.fire('keydown', { key: 'Escape', preventDefault() {}, stopPropagation() {} });
  assert.deepEqual(h.log.edits, []);
  const click = harness([0, 4, 8], 4, 8, false);
  click.fire('mouseup', {});
  assert.deepEqual(click.log.edits, []);
});
