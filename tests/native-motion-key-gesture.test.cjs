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
  assert.deepEqual(labels(Gesture.menuItems({}, ctx)), ['Delete this key', 'Hold', '--', 'Easy Ease  (F9)', 'Easy Ease In  (Shift+F9)', 'Easy Ease Out  (Cmd+Shift+F9)', 'Linear  (Cmd+Alt+K)', 'Invert the selection', '--', 'Lock keyframes on:', '   • the in point', '   • the out point', '   • the whole layer']);
  const held = ctxFor(view([0, 4], [4]), { hold: true });
  assert.equal(Gesture.menuItems({}, held.ctx)[1].label, 'Remove hold');
});

test('two or more selected keys add distribute, flip, subdivide, colour and the selection tools', () => {
  const { ctx } = ctxFor(view([0, 4, 8], [0, 8]));
  assert.deepEqual(labels(Gesture.menuItems({}, ctx)).slice(7), ['--', 'Distribute evenly', 'Flip order', 'Subdivide (key at midpoint)', 'Color the keys…', 'Select every other one',
    'Select 1 in N…', 'Keep at random…', 'Invert the selection', '--', 'Lock keyframes on:', '   • the in point', '   • the out point', '   • the whole layer']);
});

test('Color the keys opens the baseline palette; each tint and the remover are one colour edit of the selection', () => {
  const { ctx, log } = ctxFor(view([0, 4, 8], [0, 8]));
  let shown = null;
  ctx.showMenu = (items) => { shown = items; };
  Gesture.menuItems({}, ctx).find((i) => i.label === 'Color the keys…').action();
  assert.deepEqual(labels(shown), ['Amber', 'Blue', 'Green', 'Pink', 'Purple', 'White', '--', 'Remove the color']);
  shown[1].action(); shown[7].action();
  assert.deepEqual(log.edits, [{ kind: 'color', color: '#4ea9ff' }, { kind: 'color', color: null }]);
  const host = { SM: { t: (k) => ({ colorAmber: 'Ambre', ctxRemoveColor: 'Retirer la couleur' }[k] || k) } };
  assert.deepEqual(labels(Gesture.colorItems(host, ctx)).filter((l) => l !== '--').sort().slice(0, 1), ['Ambre']);
  assert.equal(Gesture.colorItems(host, ctx).pop().label, 'Retirer la couleur');
});

test('the per-key lock rows act on the selected keys only: the active mode clears, another sets', () => {
  const locked = view([0, 4, 8], [0, 8]);
  locked.tracks[0].items[0].lockTo = 'in'; locked.tracks[0].items[2].lockTo = 'in';
  const { ctx, log } = ctxFor(locked);
  const rows = Gesture.menuItems({}, ctx).filter((i) => !i.sep && i.label.startsWith('   •'));
  assert.deepEqual(rows.map((r) => r.label), ['   • the in point  ✓', '   • the out point', '   • the whole layer']);
  rows.forEach((r) => r.action());
  assert.deepEqual(log.edits, [{ kind: 'lockTo', mode: null }, { kind: 'lockTo', mode: 'out' }, { kind: 'lockTo', mode: 'layer' }]);
  const mixed = view([0, 4], [0, 4]);
  mixed.tracks[0].items[0].lockTo = 'in';
  assert.ok(!Gesture.menuItems({}, ctxFor(mixed).ctx).some((i) => /✓/.test(i.label || '')), 'a check needs EVERY selected key to follow the mode');
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

test('snapFrame takes the nearest candidate inside the reach of 8 screen pixels, at least one frame', () => {
  const c = [{ frame: 10, label: 'a' }, { frame: 12, label: 'b' }, { frame: 30, label: 'c' }];
  assert.equal(Gesture.snapFrame(11.4, c, 10).frame, 12, 'the nearer of 10 and 12');
  assert.equal(Gesture.snapFrame(20, c, 10), null, 'ten frames from anything');
  assert.deepEqual(Gesture.snapFrame(11, c, 10), { frame: 10, label: 'a', dist: 1 }, 'ties go to the first candidate');
  assert.equal(Gesture.snapFrame(15, c, 10), null, 'a reach of 1 at 10 px per frame');
  assert.equal(Gesture.snapFrame(15, c, 2).frame, 12, 'a zoomed-out timeline reaches 4 frames');
  assert.equal(Gesture.snapFrame(13, [], 10), null);
});

test('snap candidates: playhead, work area, markers then the keys that stay, first label per frame, inside the composition', () => {
  const host = { state: { currentFrame: 5, waIn: 2, waOut: 50 }, SM: { markers: { list: () => [{ frame: 5, name: 'dup' }, { frame: 9, name: 'Beat' }, { frame: 12 }, { frame: 99, name: 'out' }] } } };
  const v = view([0, 4, 20], [4], [{ target: tgt, property: 'rotation', items: [{ frame: 7, selected: false }] }]);
  assert.deepEqual(Gesture.snapCandidates(host, v, 40), [
    { frame: 5, label: 'Tête de lecture' }, { frame: 2, label: 'Début zone' }, { frame: 9, label: 'Beat' }, { frame: 12, label: 'Repère' },
    { frame: 0, label: 'Clé' }, { frame: 20, label: 'Clé' }, { frame: 7, label: 'Clé' }], 'the selected key at 4 and the marker past the end are not candidates; waOut 50 is outside');
  assert.deepEqual(Gesture.snapCandidates({}, null, 40), [], 'a host with nothing readable adds nothing');
  assert.deepEqual(Gesture.snapCandidates({ state: { currentFrame: 1 }, SM: { markers: { list() { throw new Error('x'); } } } }, null, 40), [{ frame: 1, label: 'Tête de lecture' }]);
});

function snapHarness(over = {}) {
  const wrap = { children: [], appendChild(n) { n.parentNode = wrap; wrap.children.push(n); }, removeChild(n) { wrap.children.splice(wrap.children.indexOf(n), 1); n.parentNode = null; } };
  const doc = { listeners: [], addEventListener(t, f) { this.listeners.push([t, f]); }, removeEventListener(t, f) { const i = this.listeners.findIndex((l) => l[1] === f); if (i >= 0) this.listeners.splice(i, 1); },
    getElementById: (id) => (id === 'fg-wrap' ? wrap : wrap.children.find((c) => c.id === id) || null), createElement: () => ({ style: {}, dataset: {} }) };
  const nodes = [{ style: {} }], edits = [], tracks = [{ items: [{ frame: 4, selected: true }, { frame: 8, selected: false }, { frame: 30, selected: false }] }];
  let enabled = true;
  const drag = Gesture.beginDrag(Object.assign({ doc, startX: 0, fc: 10, total: 40, nodes: () => nodes, tracks: () => tracks, edit: (a) => { edits.push(a); return Promise.resolve(true); },
    snap: { first: 4, on: () => enabled, frames: () => [{ frame: 20, label: 'Repère' }, { frame: 8, label: 'Clé' }] } }, over));
  const fire = (t, e) => doc.listeners.filter((l) => l[0] === t).forEach((l) => l[1](e));
  return { drag, nodes, edits, fire, wrap, off() { enabled = false; } };
}

test('a drag snaps the grabbed key to a candidate, shows the guide line and commits the snapped shift', () => {
  const h = snapHarness();
  h.fire('mousemove', { clientX: 148 }); // raw +15 -> frame 19, snaps to the marker at 20 (shift 16)
  assert.equal(h.drag.delta(), 16);
  const guide = h.wrap.children[0];
  assert.equal(guide.id, 'motion-snap-guide');
  assert.equal(guide.style.left, '205px', 'frame 20 at 10 px per frame, centred');
  assert.equal(guide.dataset.label, 'Repère · 21');
  assert.equal(h.nodes[0].style.transform, 'translateX(160px)');
  h.fire('mousemove', { clientX: 90 }); // frame 13: nothing within a frame
  assert.equal(h.wrap.children.length, 0, 'the guide goes when nothing is snapped');
  assert.equal(h.drag.delta(), 9);
  h.fire('mousemove', { clientX: 148 });
  h.fire('mouseup', { clientX: 148 });
  assert.deepEqual(h.edits, [{ kind: 'shift', delta: 16 }]);
  assert.equal(h.wrap.children.length, 0, 'the guide is removed on release');
});

test('Cmd or Ctrl and the preference turn the snap off; a snap onto a key that stays falls back to the raw frame', () => {
  const held = snapHarness();
  held.fire('mousemove', { clientX: 148, metaKey: true });
  assert.equal(held.drag.delta(), 15);
  held.fire('mousemove', { clientX: 148, ctrlKey: true });
  assert.equal(held.drag.delta(), 15);
  assert.equal(held.wrap.children.length, 0);
  const pref = snapHarness(); pref.off();
  pref.fire('mousemove', { clientX: 148 });
  assert.equal(pref.drag.delta(), 15, 'the saved preference disables it');
  // Raw +3 lands on frame 7; the candidate at 8 holds a key that stays, so the engine would refuse the snap.
  const blocked = snapHarness();
  blocked.fire('mousemove', { clientX: 30 });
  assert.equal(blocked.drag.delta(), 3, 'the raw frame is used and no guide is left');
  assert.equal(blocked.wrap.children.length, 0);
  const none = snapHarness(); none.drag.cancel();
  assert.deepEqual(none.edits, []);
});
