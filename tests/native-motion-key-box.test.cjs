'use strict';
// #2377: the key selection box: warp math (the same fixed values the engine tests pin), the drag preview/commit and the box element.
const test = require('node:test'), assert = require('node:assert/strict');
const Box = require('../src/js/adapters/native-motion-key-box.js');

const tgt = (uid) => ({ contextId: 'scene-root', layerUid: uid });
const id = (uid, property) => JSON.stringify(['scene-root', uid, false, undefined, property]);
// Three rows: opacity keys 0/10*/20/40, rotation keys 0/10*/30, scale keys 0/20* (the engine test fixture); * = selected.
function view() {
  const items = (frames, sel) => frames.map((f) => ({ frame: f, selected: sel.includes(f) }));
  return { tracks: [
    { target: tgt('A'), property: 'opacity', items: items([0, 10, 20, 40], [10]) },
    { target: tgt('A'), property: 'rotation', items: items([0, 10, 30], [10]) },
    { target: tgt('A'), property: 'scale', items: items([0, 20], [20]) }] };
}
const rows = () => Box.rowsOf(view());

test('rowsOf lists the tracks holding selected keys, with the keys that stay, in the order the rank gives', () => {
  const r = rows();
  assert.deepEqual(r.map((x) => [x.id, x.selected, x.staying]), [[id('A', 'opacity'), [10], [0, 20, 40]], [id('A', 'rotation'), [10], [0, 30]], [id('A', 'scale'), [20], [0]]]);
  const flipped = Box.rowsOf(view(), (t) => -['opacity', 'rotation', 'scale'].indexOf(t.property));
  assert.deepEqual(flipped.map((x) => x.track.property), ['scale', 'rotation', 'opacity']);
  assert.deepEqual(Box.rowsOf({ tracks: [{ target: tgt('A'), property: 'x', items: [{ frame: 1, selected: false }] }] }), []);
});

test('the warp factors are the baseline ones, pinned by the same values as the engine tests', () => {
  const to = (mode, total, grab) => Box.plan(rows(), mode, total, grab, 59).map((r) => r.to);
  assert.deepEqual(to('top', 6), [[16], [13], [20]], 'rows 0, 1, 2 of 3 move 6, 3, 0');
  assert.deepEqual(to('bottom', -3), [[10], [9], [17]], 'round(-1.5) = -1: halves round up');
  assert.deepEqual(to('right', 10), [[10], [10], [30]], 'the left edge of the selected frames stays, the right one moves the whole drag');
  assert.deepEqual(to('left', -5), [[5], [5], [20]]);
  assert.deepEqual(to('move', 3), [[13], [13], [23]]);
  const liquify = Box.plan([{ id: 'a', selected: [10, 20], staying: [] }], 'liquify', 4, 10, 59);
  assert.deepEqual(liquify[0].to, [14, 20], 'the key at the grab follows fully, the one 10 frames away (2.8 radii) not at all');
  assert.equal(Box.share('top', 5, { row: 0, rows: 1, min: 5, max: 5 }), 1, 'a single row skews as a rigid move');
  assert.equal(Box.share('right', 5, { row: 0, rows: 2, min: 5, max: 5 }), 0, 'one frame has nothing to spread');
});

test('an unreachable gesture is null: before frame 0, past the end or onto a key that stays; moved keys may pile onto one frame', () => {
  assert.equal(Box.plan(rows(), 'top', -12, 0, 59), null);
  assert.equal(Box.plan(rows(), 'top', 51, 0, 59), null);
  assert.equal(Box.plan([{ id: 'a', selected: [10], staying: [20] }, { id: 'b', selected: [10], staying: [] }], 'top', 10, 0, 59), null);
  // #2377: baseline onDragMove let dragged keys share a frame; the engine keeps one key per frame.
  assert.deepEqual(Box.plan([{ id: 'a', selected: [10, 11], staying: [] }], 'right', -1, 0, 59), [{ id: 'a', from: [10, 11], to: [10, 10] }]);
  assert.equal(Box.plan([], 'top', 1, 0, 59), null);
});

function dragHarness(over = {}) {
  const listeners = [], doc = { addEventListener: (t, f) => listeners.push([t, f]), removeEventListener: (t, f) => { const i = listeners.findIndex((l) => l[0] === t && l[1] === f); if (i >= 0) listeners.splice(i, 1); } };
  const r = rows(), edits = [];
  const nodes = [[id('A', 'opacity'), 10], [id('A', 'rotation'), 10], [id('A', 'scale'), 20], [id('A', 'opacity'), 20]].map(([i, f]) => ({ node: { style: {} }, id: i, frame: f }));
  const opts = Object.assign({ doc, mode: 'top', startX: 100, fc: 10, last: 59, rows: r, nodes: () => nodes, edit: (a, o) => { edits.push([a, o]); return Promise.resolve(true); }, onEnd: () => { over.ended = (over.ended || 0) + 1; } }, over);
  const drag = Box.begin(opts);
  const fire = (t, e) => listeners.filter((l) => l[0] === t).forEach((l) => l[1](e));
  return { drag, nodes, edits, listeners, fire, over };
}

test('a skew drag previews each selected key by its share, keeps the last valid drag and commits ONE warp with the landing frames', async () => {
  const h = dragHarness();
  h.fire('mousemove', { clientX: 101 });
  assert.ok(!h.nodes[0].node.style.transform, 'under 3px is still a click');
  h.fire('mousemove', { clientX: 160 }); // 6 frames
  assert.deepEqual(h.nodes.map((n) => n.node.style.transform || ''), ['translateX(60px)', 'translateX(30px)', '', ''], 'the unselected key at 20 does not move');
  h.fire('mousemove', { clientX: 20 }); // -8: the first row would land at 2 (fine), second at 6, third stays => valid; then -12 is not
  h.fire('mousemove', { clientX: -20 }); // -12: before frame 0
  assert.equal(h.drag.total(), -8, 'the last valid drag holds');
  h.fire('mousemove', { clientX: 160 });
  h.fire('mouseup', { clientX: 160 });
  assert.equal(h.edits.length, 1);
  assert.deepEqual(h.edits[0][0], { kind: 'warp', mode: 'top', total: 6 });
  assert.deepEqual(h.edits[0][1].expected({ target: tgt('A'), property: 'opacity', frames: [10] }), [16]);
  assert.deepEqual(h.edits[0][1].expected({ target: tgt('A'), property: 'scale', frames: [20] }), [20]);
  assert.deepEqual(h.edits[0][1].expected({ target: tgt('A'), property: 'other', frames: [3] }), [3], 'a track the gesture did not name keeps its frames');
  const sorted = [{ target: tgt('A'), property: 'scale' }, { target: tgt('A'), property: 'opacity' }, { target: tgt('A'), property: 'rotation' }].sort(h.edits[0][1].order);
  assert.deepEqual(sorted.map((t) => t.property), ['opacity', 'rotation', 'scale'], 'the engine reads rows in the order the selections name them');
  assert.ok(h.nodes.every((n) => !n.node.style.transform));
  assert.equal(h.listeners.length, 0);
  assert.equal(h.over.ended, 1);
});

test('the plain move commits a whole-frame shift, a liquify carries its grab frame, a space has no grab', () => {
  const move = dragHarness({ mode: 'move' });
  move.fire('mousemove', { clientX: 134 });
  move.fire('mouseup', { clientX: 134 });
  assert.deepEqual(move.edits[0][0], { kind: 'shift', delta: 3 });
  const liquify = dragHarness({ mode: 'liquify', grab: 12.5 });
  liquify.fire('mousemove', { clientX: 140 });
  liquify.fire('mouseup', { clientX: 140 });
  assert.deepEqual(liquify.edits[0][0], { kind: 'warp', mode: 'liquify', total: 4, grab: 12.5 });
  const space = dragHarness({ mode: 'right' });
  space.fire('mousemove', { clientX: 150 });
  space.fire('mouseup', { clientX: 150 });
  assert.deepEqual(space.edits[0][0], { kind: 'warp', mode: 'right', total: 5 });
});

test('zero distance and Escape commit nothing and release the gesture', () => {
  const click = dragHarness();
  click.fire('mousemove', { clientX: 102 }); click.fire('mouseup', { clientX: 102 });
  assert.deepEqual(click.edits, []);
  const back = dragHarness();
  back.fire('mousemove', { clientX: 130 }); back.fire('mousemove', { clientX: 100 }); back.fire('mouseup', { clientX: 100 });
  assert.deepEqual(back.edits, [], 'dragging back to the start is a zero-distance gesture');
  const esc = dragHarness();
  esc.fire('mousemove', { clientX: 160 });
  let prevented = false;
  esc.fire('keydown', { key: 'Escape', preventDefault() { prevented = true; }, stopPropagation() {} });
  esc.fire('mouseup', { clientX: 160 });
  assert.equal(prevented, true);
  assert.deepEqual(esc.edits, []);
  assert.ok(esc.nodes.every((n) => !n.node.style.transform));
  assert.equal(esc.listeners.length, 0);
  const gone = dragHarness();
  gone.fire('mousemove', { clientX: 160 }); gone.drag.cancel();
  assert.deepEqual(gone.edits, []);
});

// A document just big enough for the box element.
function fakeDoc() {
  const mk = () => ({ children: [], style: {}, dataset: {}, listeners: new Map(), classList: { set: new Set(), add(c) { this.set.add(c); }, remove(c) { this.set.delete(c); } }, className: '', title: '',
    appendChild(n) { n.parentNode = this; this.children.push(n); }, removeChild(n) { this.children.splice(this.children.indexOf(n), 1); n.parentNode = null; },
    addEventListener(t, f) { this.listeners.set(t, (this.listeners.get(t) || []).concat(f)); }, removeEventListener() {},
    fire(t, e) { (this.listeners.get(t) || []).forEach((f) => f(Object.assign({ preventDefault() {}, stopPropagation() {} }, e))); } });
  const doc = Object.assign(mk(), { body: mk(), createElement: mk, getElementById: () => null, elementFromPoint: () => null, defaultView: null });
  return doc;
}
function attachHarness(over = {}) {
  const doc = fakeDoc(), toasts = [], edits = [];
  const keys = [[id('A', 'opacity'), 10, true, 100], [id('A', 'rotation'), 10, true, 130], [id('A', 'scale'), 20, false, 160], [id('A', 'opacity'), 40, true, 100]]
    .map(([i, f, selected, top]) => ({ id: i, frame: f, selected, node: { style: {} }, rect: { left: f * 10, right: f * 10 + 9, top, bottom: top + 9 } }));
  const state = { keys, selected: () => keys.filter((k) => k.selected) };
  const box = Box.attach(Object.assign({ doc, host: {}, nodes: () => keys, view: () => view(), fc: () => 10, last: () => 59, edit: (a, o) => { edits.push([a, o]); return Promise.resolve(true); }, toast: (m) => toasts.push(m) }, over));
  return { doc, box, toasts, edits, keys, state };
}

test('the box covers the selected keys once they sit on two or more rows, and only then', () => {
  const h = attachHarness();
  assert.equal(h.box.update(), true);
  const el = h.doc.body.children[0];
  assert.equal(el.className, 'motion-keysel-box');
  assert.deepEqual([el.style.left, el.style.top, el.style.width, el.style.height], ['100px', '100px', '309px', '39px'], 'the union of the selected key rectangles');
  assert.deepEqual(el.children.map((c) => c.className), ['motion-keysel-fill', 'motion-keysel-edge motion-keysel-edge-top', 'motion-keysel-edge motion-keysel-edge-bottom', 'motion-keysel-edge motion-keysel-edge-left', 'motion-keysel-edge motion-keysel-edge-right']);
  assert.equal(h.box.update(), true);
  assert.equal(h.doc.body.children.length, 1, 'updating moves the same element');
  h.keys[1].selected = false; h.keys[3].selected = false;
  assert.equal(h.box.update(), false, 'one selected key is no box');
  assert.equal(h.doc.body.children.length, 0);
  h.keys[0].selected = true; h.keys[3].selected = true;
  assert.equal(h.box.update(), false, 'two rows are needed: two keys of one track are dragged as a group');
  h.box.remove();
});

test('the edges and the fill start the right gesture; a space on one frame or a skew on one row says why instead', () => {
  const h = attachHarness();
  h.box.update();
  const el = h.doc.body.children[0];
  const press = (child, e = {}) => child.fire('mousedown', Object.assign({ clientX: 100 }, e));
  press(el.children[1]);
  assert.equal(h.box.active(), true);
  h.box.cancel();
  assert.equal(h.box.active(), false, 'a cancel releases the gesture');
  press(el.children[3]);
  assert.equal(h.box.active(), true);
  h.box.cancel();
  // Every selected key on one frame: the side edges refuse.
  const same = attachHarness({ view: () => ({ tracks: [{ target: tgt('A'), property: 'opacity', items: [{ frame: 10, selected: true }] }, { target: tgt('A'), property: 'rotation', items: [{ frame: 10, selected: true }] }] }) });
  same.box.update();
  same.doc.body.children[0].children[3].fire('mousedown', { clientX: 5 });
  assert.equal(same.box.active(), false);
  assert.deepEqual(same.toasts, ['Select keys on 2 different frames to space them']);
  const one = attachHarness({ view: () => ({ tracks: [{ target: tgt('A'), property: 'opacity', items: [{ frame: 10, selected: true }, { frame: 12, selected: true }] }] }) });
  one.box.update();
  one.doc.body.children[0].children[1].fire('mousedown', { clientX: 5 });
  assert.equal(one.box.active(), false);
  assert.deepEqual(one.toasts, ['Select keys on 2 tracks to skew them']);
  const stale = attachHarness({ view: () => null });
  stale.box.update();
  stale.doc.body.children[0].children[0].fire('mousedown', { clientX: 5 });
  assert.equal(stale.box.active(), false, 'with no current read nothing starts');
});

test('the fill moves, Cmd/Ctrl + the fill liquifies around the pressed frame; the box forwards right-clicks to the key under it', () => {
  const h = attachHarness();
  h.box.update();
  const el = h.doc.body.children[0], listeners = [];
  const edits = h.edits;
  h.doc.addEventListener = (t, f) => listeners.push([t, f]);
  h.doc.removeEventListener = () => {};
  h.doc.getElementById = (name) => (name === 'frame-grid' ? { getBoundingClientRect: () => ({ left: 50 }) } : null);
  const fire = (t, e) => listeners.filter((l) => l[0] === t).forEach((l) => l[1](e));
  el.children[0].fire('mousedown', { clientX: 150, metaKey: true });
  fire('mousemove', { clientX: 190 });
  fire('mouseup', { clientX: 190 });
  assert.deepEqual(edits[0][0], { kind: 'warp', mode: 'liquify', total: 4, grab: 10 }, 'the grab frame is the pressed x over the grid, in frames');
  el.children[0].fire('mousedown', { clientX: 150 });
  fire('mousemove', { clientX: 180 });
  fire('mouseup', { clientX: 180 });
  assert.deepEqual(edits[1][0], { kind: 'shift', delta: 3 });
  // Right-click: the box steps aside for one hit-test and hands the event to what is under it.
  const seen = [];
  const under = { dispatchEvent: (e) => seen.push(e) };
  h.doc.elementFromPoint = () => { seen.push(el.children.map((c) => c.style.pointerEvents).concat(el.style.pointerEvents)); return under; };
  global.MouseEvent = class { constructor(type, init) { this.type = type; Object.assign(this, init); } };
  let prevented = false;
  el.fire('contextmenu', { clientX: 7, clientY: 8, preventDefault() { prevented = true; } });
  delete global.MouseEvent;
  assert.deepEqual(seen[0], ['none', 'none', 'none', 'none', 'none', 'none'], 'the box and its surfaces do not take the hit-test');
  assert.deepEqual([seen[1].type, seen[1].clientX, seen[1].clientY], ['contextmenu', 7, 8]);
  assert.equal(prevented, true);
  assert.deepEqual(el.children.map((c) => c.style.pointerEvents), [undefined, undefined, undefined, undefined, undefined], 'the surfaces take pointer events again');
});

test('following the grid: the box is re-placed on scroll and gone on dispose', () => {
  const doc = fakeDoc(), scrolls = [];
  doc.addEventListener = (t, f) => scrolls.push([t, f]);
  doc.removeEventListener = (t, f) => { const i = scrolls.findIndex((s) => s[1] === f); if (i >= 0) scrolls.splice(i, 1); };
  const keys = [[id('A', 'opacity'), 10, 100], [id('A', 'rotation'), 10, 130]].map(([i, f, top]) => ({ id: i, frame: f, selected: true, node: { style: {} }, rect: { left: f * 10, right: f * 10 + 9, top, bottom: top + 9 } }));
  const box = Box.attach({ doc, host: {}, nodes: () => keys, view: () => view(), fc: () => 10, last: () => 59, edit: () => Promise.resolve(true), toast() {} });
  box.update();
  const el = doc.body.children[0];
  keys.forEach((k) => { k.rect = { left: k.rect.left, right: k.rect.right, top: k.rect.top - 40, bottom: k.rect.bottom - 40 }; });
  scrolls.filter((s) => s[0] === 'scroll').forEach((s) => s[1]());
  assert.equal(el.style.top, '60px');
  box.dispose();
  assert.equal(doc.body.children.length, 0);
  assert.equal(scrolls.length, 0, 'the scroll listener is released');
});
