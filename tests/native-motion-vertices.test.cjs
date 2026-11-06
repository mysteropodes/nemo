'use strict';
// #2520/#2525 C-motion-canvas-4: the vertex dots of an element's open Path group in Motion and the vertex drag. A dot sits where the shape is painted
// (the presented segments already carry the vtxN offsets); a drag is a local preview and ONE object.property.edit of `vtxN` on release, the world delta
// pulled back through the item's presented matrix (baseline motion.js@1065e9d8 onDrag 'vertex').
const test = require('node:test'), assert = require('node:assert/strict');
const Vertices = require('../src/js/adapters/native-motion-vertices.js');

const el = (tag) => ({ tag, attrs: {}, children: [], setAttribute(k, v) { this.attrs[k] = String(v); }, appendChild(c) { this.children.push(c); return c; } });
const tick = () => new Promise((done) => setTimeout(done, 0));
const settle = async () => { for (let i = 0; i < 6; i++) await tick(); };
const TARGET = { contextId: 'scene-root', layerUid: 'L', strokeId: 'S' };

function harness(options = {}) {
  const listeners = {}, toasts = [], edits = [], reads = [], shows = [];
  const pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's1', canvasW: 200, canvasH: 200 };
  const presentation = { frame: 3 };
  const matrix = options.matrix || [1, 0, 0, 1, 0, 0];
  const path = { segments: [[10, 10], [60, 10], [60, 60], [10, 60]].map(([x, y]) => ({ point: { x, y } })),
    data: { strokeId: 'S', nativeStableTarget: { ...TARGET, frameScope: { kind: 'authored', frame: 0 } }, nativeMatrix: matrix } };
  const session = { readProperty: async (sel) => { reads.push(sel); return { value: [1, 1] }; },
    editProperty: async (payload) => { edits.push(payload); return options.refuse ? { ok: true, result: { applied: false } } : { ok: true, result: { applied: true } }; } };
  const win = { state: { appMode: 'motion', playing: false, tool: 'select', activeLayerIdx: 0, selectedStrokeIndices: [] }, selectedPaths: [path], showToast: (t) => toasts.push(t),
    document: { getElementById: () => ({ contains: () => true }), createElementNS: (_n, tag) => el(tag), addEventListener: (type, fn) => { (listeners[type] = listeners[type] || []).push(fn); } },
    NemoNativeSelectOverlay: { show: () => shows.push(1) },
    NemoSelectCanvasIntent: { presentedState: () => ({ pin, presentation }), pickContext: () => 'ctx', heldId: () => 'scene-root',
      worldPoint: (_w, x, y) => ({ x, y, perPixel: 1 }), reread: async (_w, sel) => ({ complete: true, paths: sel }) },
    NemoNativeObjectWidgetUI: { session: () => session, proofNow: () => ({ session, info: pin, selector: { contextId: 'scene-root', frame: 3 }, current: () => true }) },
    NemoNativePresentationWait: { until: async () => true } };
  Vertices.install(win);
  const ev = (x, y, extra) => ({ button: 0, pointerId: 1, clientX: x, clientY: y, target: {}, stopImmediatePropagation() { this.stopped = true; }, preventDefault() {}, ...extra });
  const fire = (type, e) => (listeners[type] || []).forEach((fn) => fn(e));
  return { win, path, edits, reads, toasts, shows, ev, fire, listeners };
}

test('the dots are drawn only while the open Path group belongs to the selected element, one violet diamond per presented anchor', () => {
  const h = harness(), svg = el('svg'), c = { V: [1, 0, 0, 1, 0, 0], pin: { canvasW: 200 }, bounds: { width: 200 } };
  Vertices.draw(h.win, svg, c, [h.path]);
  assert.equal(svg.children.length, 0, 'no group is open: nothing is drawn');
  Vertices.open(h.win, TARGET);
  Vertices.draw(h.win, svg, c, [h.path]);
  assert.equal(svg.children.length, 4);
  assert.ok(svg.children.every((n) => n.tag === 'polygon' && n.attrs.fill === 'rgb(190,130,240)'));
  Vertices.open(h.win, { ...TARGET, strokeId: 'other' });
  const again = el('svg'); Vertices.draw(h.win, again, c, [h.path]);
  assert.equal(again.children.length, 0, 'another element\'s group is open: not this one');
  h.win.state.appMode = 'animation2d';
  Vertices.open(h.win, TARGET); const off = el('svg'); Vertices.draw(h.win, off, c, [h.path]);
  assert.equal(off.children.length, 0, 'outside Motion there are no dots');
});

test('a drag of a dot is ONE vtxN setValue at the playhead: the offset read plus the delta pulled back through the item matrix', async () => {
  const h = harness({ matrix: [2, 0, 0, 2, 5, 5] });
  Vertices.open(h.win, TARGET);
  const down = h.ev(60, 60); h.fire('pointerdown', down);
  assert.equal(down.stopped, true, 'a press on a dot never reaches the Select gesture');
  h.fire('pointermove', h.ev(70, 65)); h.fire('pointermove', h.ev(80, 70));
  h.fire('pointerup', h.ev(80, 70)); await settle();
  assert.equal(h.edits.length, 1, 'exactly one edit');
  assert.deepEqual(h.edits[0], { command: 'object.property.edit', target: TARGET, property: 'vtx2', action: { kind: 'setValue', frame: 3, values: [1 + 10, 1 + 5] } });
  assert.equal(h.reads.length, 1);
  assert.deepEqual(h.reads[0].view, { kind: 'rawSample', target: TARGET, property: 'vtx2', frame: 3 });
});

test('a press away from every dot, a click without a drag, Escape and a refused edit change nothing', async () => {
  const h = harness();
  Vertices.open(h.win, TARGET);
  const away = h.ev(150, 150); h.fire('pointerdown', away); assert.ok(!away.stopped, 'not on a dot: the press passes through');
  h.fire('pointerdown', h.ev(10, 10)); h.fire('pointerup', h.ev(10, 10)); await settle();
  assert.equal(h.edits.length, 0, 'a click is no drag');
  h.fire('pointerdown', h.ev(10, 10)); h.fire('pointermove', h.ev(30, 30)); h.fire('keydown', { key: 'Escape', stopImmediatePropagation() {}, preventDefault() {} });
  h.fire('pointerup', h.ev(30, 30)); await settle();
  assert.equal(h.edits.length, 0, 'Escape cancels');
  const r = harness({ refuse: true }); Vertices.open(r.win, TARGET);
  r.fire('pointerdown', r.ev(10, 10)); r.fire('pointermove', r.ev(30, 30)); r.fire('pointerup', r.ev(30, 30)); await settle();
  assert.equal(r.edits.length, 1); assert.deepEqual(r.toasts, [], 'a no-op edit is not an error');
});

test('a degenerate item matrix is refused with a notice and no write', async () => {
  const h = harness({ matrix: [0, 0, 0, 0, 0, 0] });
  Vertices.open(h.win, TARGET);
  h.fire('pointerdown', h.ev(10, 10)); h.fire('pointermove', h.ev(30, 30)); h.fire('pointerup', h.ev(30, 30)); await settle();
  assert.equal(h.edits.length, 0); assert.equal(h.toasts.length, 1);
});
