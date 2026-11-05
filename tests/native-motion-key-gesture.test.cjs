'use strict';
// #2524 / #2377: the key context menu, the selection-only operations and the drag preview/commit.
const test = require('node:test'), assert = require('node:assert/strict');
const Gesture = require('../src/js/adapters/native-motion-key-gesture.js');

const tgt = { contextId: 'scene-root', layerUid: 'A' };
function view(frames, selectedFrames, extra) {
  const tracks = [{ target: tgt, property: 'opacity', items: frames.map((f) => ({ frame: f, selected: selectedFrames.includes(f) })) }].concat(extra || []);
  const accepted = [];
  return { tracks, accepted, selectKeys(keys) { if (keys.length > 4096) return false; accepted.push(keys.map((k) => [k.target.property, k.frame])); return true; } };
}
function ctxFor(v, over) {
  const log = { edits: [], toasts: [], selectOnly: 0, refresh: 0 };
  return { log, ctx: Object.assign({ view: v, hold: false, edit: (a) => { log.edits.push(a); return true; }, selectOnly: () => { log.selectOnly++; return true; },
    toast: (m) => log.toasts.push(m), refresh: () => { log.refresh++; }, prompt: () => null }, over) };
}
const labels = (items) => items.map((i) => (i.sep ? '--' : i.label));

test('one selected key: delete, hold, the four presets and invert, with the baseline wording', () => {
  const { ctx } = ctxFor(view([0, 4, 8], [4]));
  assert.deepEqual(labels(Gesture.menuItems({}, ctx)), ['Delete this key', 'Hold', '--', 'Easy Ease  (F9)', 'Easy Ease In  (Shift+F9)', 'Easy Ease Out  (Cmd+Shift+F9)', 'Linear  (Cmd+Alt+K)', 'Invert the selection']);
  const held = ctxFor(view([0, 4], [4]), { hold: true });
  assert.equal(Gesture.menuItems({}, held.ctx)[1].label, 'Remove hold');
});

test('two or more selected keys add distribute, flip, subdivide and the selection tools', () => {
  const { ctx } = ctxFor(view([0, 4, 8], [0, 8]));
  assert.deepEqual(labels(Gesture.menuItems({}, ctx)).slice(7), ['--', 'Distribute evenly', 'Flip order', 'Subdivide (key at midpoint)', 'Select every other one', 'Select 1 in N…', 'Keep at random…', 'Invert the selection']);
});

test('every item is one batch edit and Delete first narrows to the clicked key', () => {
  const { ctx, log } = ctxFor(view([0, 4, 8], [0, 8]));
  const by = Object.fromEntries(Gesture.menuItems({}, ctx).filter((i) => !i.sep).map((i) => [i.label, i]));
  by['Easy Ease Out  (Cmd+Shift+F9)'].action(); by['Linear  (Cmd+Alt+K)'].action(); by['Distribute evenly'].action(); by['Flip order'].action(); by['Subdivide (key at midpoint)'].action(); by.Hold.action();
  assert.deepEqual(log.edits, [{ kind: 'preset', preset: 'easeOut' }, { kind: 'preset', preset: 'linear' }, { kind: 'distribute' }, { kind: 'flip' }, { kind: 'subdivide' }, { kind: 'preset', preset: 'hold' }]);
  log.edits.length = 0;
  by['Delete this key'].action();
  assert.equal(log.selectOnly, 1);
  assert.deepEqual(log.edits, [{ kind: 'delete' }]);
  const refused = ctxFor(view([0, 4], [0, 4]), { selectOnly: () => false });
  assert.equal(Gesture.menuItems({}, refused.ctx)[0].action(), false);
  assert.deepEqual(refused.log.edits, []);
});

test('select every Nth, keep random and invert are selection-only over the key read', () => {
  const v = view([0, 2, 4, 6, 8], [0, 2, 4, 6, 8]);
  assert.equal(Gesture.selectEveryNth(v, 2), true);
  assert.deepEqual(v.accepted.pop(), [['opacity', 0], ['opacity', 4], ['opacity', 8]]);
  Gesture.selectEveryNth(v, 'junk');
  assert.deepEqual(v.accepted.pop().length, 3, 'a bad N means every other one');
  Gesture.selectEveryNth(v, 3);
  assert.deepEqual(v.accepted.pop(), [['opacity', 0], ['opacity', 6]]);
  Gesture.keepRandom(v, 40, () => 0);
  assert.deepEqual(v.accepted.pop(), [['opacity', 0], ['opacity', 2]], '40% of 5 keeps 2, drawn first from the front');
  Gesture.keepRandom(v, 0, () => 0.99);
  assert.equal(v.accepted.pop().length, 3, 'zero means the default 50% and at least one key survives');
  const small = view([0, 2], [0, 2]);
  Gesture.keepRandom(small, 1, () => 0);
  assert.equal(small.accepted.pop().length, 1);
  const inv = view([0, 2, 4], [2], [{ target: tgt, property: 'rotation', items: [{ frame: 1, selected: false }] }]);
  assert.equal(Gesture.invert(inv), true);
  assert.deepEqual(inv.accepted.pop(), [['opacity', 0], ['opacity', 4], ['rotation', 1]]);
  const wide = view(Array.from({ length: 40 }, (_, i) => i), []);
  assert.equal(Gesture.invert(wide), true, 'baseline has no key limit: 40 keys select');
  assert.equal(Gesture.invert(view(Array.from({ length: 5000 }, (_, i) => i), [])), false, 'only a runaway selection is refused');
});

test('the menu prompts, then selects; Cancel and refusal leave the selection untouched or say so', () => {
  const v = view([0, 2, 4, 6], [0, 2, 4, 6]);
  const asked = [];
  const { ctx, log } = ctxFor(v, { prompt: (m, d) => { asked.push([m, d]); return '2'; } });
  const items = Object.fromEntries(Gesture.menuItems({}, ctx).filter((i) => !i.sep).map((i) => [i.label, i]));
  items['Select 1 in N…'].action();
  assert.deepEqual(asked, [['Keep one key out of how many?', '3']]);
  assert.deepEqual(v.accepted.pop(), [['opacity', 0], ['opacity', 4]]);
  items['Keep at random…'].action();
  assert.equal(asked[1][0], 'What percentage of the selection to keep?');
  assert.equal(log.refresh, 2);
  const cancelled = ctxFor(view([0, 2], [0, 2]));
  Gesture.menuItems({}, cancelled.ctx).find((i) => i.label === 'Select 1 in N…').action();
  assert.deepEqual(cancelled.ctx.view.accepted, []);
  const bad = ctxFor(Object.assign(view([0, 2], [0, 2]), { selectKeys: () => false }));
  Gesture.menuItems({}, bad.ctx).find((i) => i.label === 'Select every other one').action();
  assert.deepEqual(bad.log.toasts, ['The key selection was not accepted.']);
});

function dragHarness(over = {}) {
  const listeners = [], doc = { addEventListener: (t, f) => listeners.push([t, f]), removeEventListener: (t, f) => { const i = listeners.findIndex((l) => l[0] === t && l[1] === f); if (i >= 0) listeners.splice(i, 1); } };
  const nodes = [{ style: {} }, { style: {} }], edits = [];
  const tracks = [{ items: [{ frame: 0, selected: true }, { frame: 4, selected: true }, { frame: 8, selected: false }] }];
  const drag = Gesture.beginDrag(Object.assign({ doc, startX: 100, fc: 10, total: 40, nodes: () => nodes, tracks: () => tracks, edit: (a) => { edits.push(a); return Promise.resolve(true); }, onEnd: () => { over.ended = (over.ended || 0) + 1; } }, over));
  const fire = (t, e) => listeners.filter((l) => l[0] === t).forEach((l) => l[1](e));
  return { drag, nodes, edits, listeners, fire, over };
}

test('a drag previews the whole-frame shift on the buttons and commits ONE shift on release', async () => {
  const h = dragHarness();
  h.fire('mousemove', { clientX: 101 });
  assert.ok(!h.nodes[0].style.transform, 'under 3px is still a click');
  h.fire('mousemove', { clientX: 118 });
  assert.equal(h.nodes[0].style.transform, 'translateX(20px)');
  assert.equal(h.nodes[1].style.transform, 'translateX(20px)');
  assert.equal(h.drag.delta(), 2);
  h.fire('mousemove', { clientX: 142 });
  assert.equal(h.drag.delta(), 2, 'a shift of 4 would put the key at 4 onto the key at 8: the last valid shift holds');
  h.fire('mousemove', { clientX: 150 });
  assert.equal(h.drag.delta(), 5, 'past the collision a free shift is accepted again');
  h.fire('mousemove', { clientX: 60 });
  assert.equal(h.drag.delta(), 5, 'frame -4 is before the first frame');
  h.fire('mouseup', { clientX: 150 });
  assert.deepEqual(h.edits, [{ kind: 'shift', delta: 5 }]);
  assert.equal(h.nodes[0].style.transform, '', 'the preview is cleared');
  assert.equal(h.listeners.length, 0, 'listeners are released');
  assert.equal(h.over.ended, 1);
});

test('zero distance and Escape commit nothing; a cancel releases the gesture', () => {
  const click = dragHarness();
  click.fire('mousemove', { clientX: 102 });
  click.fire('mouseup', { clientX: 102 });
  assert.deepEqual(click.edits, []);
  const back = dragHarness();
  back.fire('mousemove', { clientX: 130 });
  back.fire('mousemove', { clientX: 100 });
  back.fire('mouseup', { clientX: 100 });
  assert.deepEqual(back.edits, [], 'dragging back to the start is a zero-distance gesture');
  const esc = dragHarness();
  esc.fire('mousemove', { clientX: 130 });
  let prevented = false;
  esc.fire('keydown', { key: 'Escape', preventDefault() { prevented = true; }, stopPropagation() {} });
  assert.equal(prevented, true);
  assert.deepEqual(esc.edits, []);
  assert.equal(esc.nodes[0].style.transform, '');
  esc.fire('mouseup', { clientX: 130 });
  assert.deepEqual(esc.edits, [], 'a release after the cancel does not commit');
  const gone = dragHarness();
  gone.fire('mousemove', { clientX: 130 });
  gone.drag.cancel();
  assert.deepEqual(gone.edits, []);
  assert.equal(gone.listeners.length, 0);
  const stale = dragHarness({ tracks: () => null });
  stale.fire('mousemove', { clientX: 150 });
  stale.fire('mouseup', { clientX: 150 });
  assert.deepEqual(stale.edits, [], 'with no current read nothing moves');
});
