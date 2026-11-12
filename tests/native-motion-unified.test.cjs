'use strict';
// #2520/#2525 C-motion-canvas-4: the unified path of a multi-element selection in Motion (baseline motion.js@1065e9d8 unifiedMotionTargets /
// buildUnifiedOverlay / onDrag 'unified'): one violet dot per key frame at the centroid plus the MEAN Position, and a drag of a dot moves every
// element's key at that frame, an element with no key there frozen at its interpolated value first, in ONE object.property.batch of setKey entries.
const test = require('node:test'), assert = require('node:assert/strict');
const Unified = require('../src/js/adapters/native-motion-unified.js');

const el = (tag) => ({ tag, attrs: {}, children: [], setAttribute(k, v) { this.attrs[k] = String(v); }, appendChild(c) { this.children.push(c); return c; } });
const tick = () => new Promise((done) => setTimeout(done, 0));
const settle = async () => { for (let i = 0; i < 8; i++) await tick(); };
const key = (frame, v) => ({ frame, v, curvePoints: [], hOut: [0, 0], hIn: [0, 0] });

function harness(options = {}) {
  const listeners = {}, toasts = [], batches = [], reads = [], shows = [];
  const pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's1', canvasW: 400, canvasH: 400 };
  const presentation = { frame: 2 };
  const keys = { A: [key(0, [0, 0]), key(10, [100, 0])], B: [key(0, [0, 0]), key(5, [10, 10])], ...options.keys };
  const sampled = { 'A@5': [50, 0], 'B@10': [20, 20], 'B@0': [0, 0], ...options.sampled };
  const path = (id, cx, cy) => ({ bounds: { center: { x: cx, y: cy } }, data: { strokeId: id, nativeLayerIndex: 0, nativeStableTarget: { contextId: 'scene-root', layerUid: 'L', strokeId: id } } });
  const sel = options.single ? [path('A', 100, 100)] : options.selection || [path('A', 100, 100), path('B', 200, 300)];
  const session = {
    readProperty: async (selector) => { const v = selector.view; reads.push(v); if (options.readGate) await options.readGate;
      if (v.kind === 'authoredKeys') { const all = keys[v.target.strokeId] || [], items = all.slice(v.offset, v.offset + v.limit); return { kind: 'authoredKeys', items, nextOffset: options.badNextOffset && v.offset + items.length < all.length ? v.offset : v.offset + items.length < all.length ? v.offset + items.length : null, trackPresent: true }; }
      return { kind: 'rawSample', value: sampled[v.target.strokeId + '@' + v.frame] || [v.frame, v.frame], frame: v.frame }; },
    editPropertyBatch: async (payload) => { batches.push(payload); return options.refuse ? { ok: true, result: { applied: false } } : { ok: true, result: { applied: true } }; } };
  const win = { state: { appMode: 'motion', playing: false, tool: 'select', activeLayerIdx: 0, selectedStrokeIndices: [] }, selectedPaths: sel, showToast: (t) => toasts.push(t),
    document: { getElementById: () => ({ contains: () => true }), createElementNS: (_n, tag) => el(tag), addEventListener: (type, fn) => { (listeners[type] = listeners[type] || []).push(fn); } },
    NemoNativeSelectOverlay: { show: () => shows.push(1) },
    NemoSelectCanvasIntent: { presentedState: () => ({ pin, presentation }), pickContext: () => 'ctx', heldId: () => 'scene-root', worldPoint: (_w, x, y) => ({ x, y, perPixel: 1 }),
      reread: async (_w, paths) => ({ complete: true, paths }) },
      NemoNativeObjectWidgetUI: { session: () => session, proofNow: () => ({ session, info: pin, selector: { contextId: 'scene-root', frame: 2 }, current: () => options.proofCurrent ? options.proofCurrent.value : true }) },
    NemoNativePresentationWait: { until: async () => true },
    NemoNativeMotionElement: { localDelta: async (_w, _i, _p, _f, world) => { if (options.deltaGate) await options.deltaGate; return world.slice(); } } };
  Unified.install(win);
  const svgBox = { V: [1, 0, 0, 1, 0, 0], pin: { canvasW: 400 }, bounds: { width: 400 } };
  const ev = (x, y, extra) => ({ button: 0, pointerId: 1, clientX: x, clientY: y, target: {}, stopImmediatePropagation() { this.stopped = true; }, preventDefault() {}, ...extra });
  const fire = (type, e) => (listeners[type] || []).forEach((fn) => fn(e));
  return { win, sel, batches, reads, toasts, shows, ev, fire, paint: () => { const svg = el('svg'); Unified.draw(win, svg, svgBox, win.selectedPaths); return svg; } };
}

test('two selected elements with Position keys draw one dot per key frame at the centroid plus the mean Position, the current frame in orange', async () => {
  const h = harness();
  assert.equal(h.paint().children.length, 0, 'nothing is drawn until the keys are read');
  await settle();
  assert.ok(h.shows.length >= 1, 'the overlay is asked to redraw when the path lands');
  const svg = h.paint(), line = svg.children.find((n) => n.tag === 'polyline'), dots = svg.children.filter((n) => n.tag === 'circle');
  // centroid (150, 200); frames 0, 5, 10: mean of [A, B] = [0,0], [(50+10)/2, 5], [(100+20)/2, 10]
  assert.equal(line.attrs.points, '150,200 180,205 210,210');
  assert.deepEqual(dots.map((d) => [d.attrs.cx, d.attrs.cy]), [['150', '200'], ['180', '205'], ['210', '210']]);
  assert.equal(dots.filter((d) => d.attrs.fill === 'rgb(255,170,40)').length, 0, 'frame 2 is not a key frame of either element');
  assert.ok(h.reads.every((v) => v.kind === 'authoredKeys' || v.kind === 'rawSample'), 'bounded reads only');
  assert.equal(h.reads.filter((v) => v.kind === 'rawSample').length, 2, 'only the frames an element has no key on are sampled (A@5, B@10)');
});

test('a single element, or elements without two keyed Positions, have no unified path', async () => {
  const single = harness({ single: true }); single.paint(); await settle(); assert.equal(single.paint().children.length, 0);
  const one = harness({ keys: { B: [] } }); one.paint(); await settle(); assert.equal(one.paint().children.length, 0, 'only one element has keys');
  const off = harness(); off.win.state.appMode = 'animation2d'; off.paint(); await settle(); assert.equal(off.paint().children.length, 0);
});

test('dragging a dot is ONE batch of setKey: every element\'s key at that frame moves, an unkeyed element is frozen at its interpolated value first', async () => {
  const h = harness();
  h.paint(); await settle();
  const down = h.ev(210, 210); h.fire('pointerdown', down);
  assert.equal(down.stopped, true, 'a press on a dot never reaches the Select gesture');
  h.fire('pointermove', h.ev(230, 216)); h.fire('pointermove', h.ev(240, 216)); h.fire('pointerup', h.ev(240, 216)); await settle();
  assert.equal(h.batches.length, 1, 'one batch');
  assert.deepEqual(h.batches[0], { command: 'object.property.batch', edits: [
    { target: { contextId: 'scene-root', layerUid: 'L', strokeId: 'A' }, property: 'position', action: { kind: 'setKey', frame: 10, values: [130, 6] } },
    { target: { contextId: 'scene-root', layerUid: 'L', strokeId: 'B' }, property: 'position', action: { kind: 'setKey', frame: 10, values: [50, 26] } }] });
});

test('a press away from the dots, a click, Escape and a refused batch write nothing and say nothing', async () => {
  const h = harness();
  h.paint(); await settle();
  const away = h.ev(300, 40); h.fire('pointerdown', away); assert.ok(!away.stopped);
  h.fire('pointerdown', h.ev(180, 205)); h.fire('pointerup', h.ev(180, 205)); await settle();
  assert.equal(h.batches.length, 0, 'a click is no drag');
  h.fire('pointerdown', h.ev(180, 205)); h.fire('pointermove', h.ev(200, 205)); h.fire('keydown', { key: 'Escape', stopImmediatePropagation() {}, preventDefault() {} }); h.fire('pointerup', h.ev(200, 205)); await settle();
  assert.equal(h.batches.length, 0, 'Escape cancels');
  const r = harness({ refuse: true }); r.paint(); await settle();
  r.fire('pointerdown', r.ev(150, 200)); r.fire('pointermove', r.ev(170, 200)); r.fire('pointerup', r.ev(170, 200)); await settle();
  assert.equal(r.batches.length, 1); assert.deepEqual(r.toasts, []);
});

test('17 selected keyed elements preserve the unified path and are read through bounded pages', async () => {
  const selection = Array.from({ length: 17 }, (_, i) => ({
    bounds: { center: { x: i, y: i } },
    data: { strokeId: `S${i}`, nativeLayerIndex: 0, nativeStableTarget: { contextId: 'scene-root', layerUid: 'L', strokeId: `S${i}` } }
  }));
  const keys = Object.fromEntries(selection.map((p) => [p.data.strokeId, [key(0, [0, 0]), key(10, [10, 0])]]));
  const h = harness({ selection, keys });
  h.paint(); await settle();
  const svg = h.paint();
  assert.equal(svg.children.length, 3, 'one polyline and two path dots');
  assert.equal(svg.children.find((n) => n.tag === 'polyline').attrs.points, '8,8 18,8', 'centroid includes the complete selection');
  assert.equal(h.reads.filter((v) => v.kind === 'authoredKeys').length, 17);
  assert.deepEqual(h.toasts, []);
  assert.equal(h.batches.length, 0);
});

test('many unkeyed selected elements still contribute to the baseline centroid', async () => {
  const selection = [
    { bounds: { center: { x: 100, y: 100 } }, data: { strokeId: 'A', nativeLayerIndex: 0, nativeStableTarget: { contextId: 'scene-root', layerUid: 'L', strokeId: 'A' } } },
    { bounds: { center: { x: 200, y: 200 } }, data: { strokeId: 'B', nativeLayerIndex: 0, nativeStableTarget: { contextId: 'scene-root', layerUid: 'L', strokeId: 'B' } } },
    ...Array.from({ length: 100 }, (_, i) => ({ bounds: { center: { x: 0, y: 0 } }, data: { strokeId: `U${i}`, nativeLayerIndex: 0, nativeStableTarget: { contextId: 'scene-root', layerUid: 'L', strokeId: `U${i}` } } }))
  ];
  const h = harness({ selection });
  h.paint(); await settle();
  assert.equal(h.paint().children.length, 4);
  assert.equal(h.paint().children.find((n) => n.tag === 'polyline').attrs.points, '2.9411764705882355,2.9411764705882355 32.94117647058823,7.9411764705882355 62.94117647058823,12.941176470588236', 'centroid includes all 102 selected elements');
  assert.equal(h.reads.filter((v) => v.kind === 'authoredKeys').length, 102);
  assert.deepEqual(h.toasts, []);
});

test('33 authored Position keys are paged completely and keep drawing', async () => {
  const tooMany = Array.from({ length: 33 }, (_, frame) => key(frame, [frame, 0]));
  const h = harness({ keys: { A: tooMany } });
  h.paint(); await settle();
  assert.equal(h.paint().children.length, 34, '33 union frames produce 33 dots and one line');
  assert.deepEqual(h.toasts, []);
  assert.equal(h.batches.length, 0);
  assert.deepEqual(h.reads.filter((v) => v.kind === 'authoredKeys' && v.target.strokeId === 'A').map((v) => v.offset), [0, 8, 16, 24, 32]);
  assert.ok(h.reads.filter((v) => v.kind === 'authoredKeys').every((v) => v.limit === 8));
});

test('61 complete union frames keep drawing within the read budget', async () => {
  const keys = {
    A: Array.from({ length: 31 }, (_, frame) => key(frame, [frame, 0])),
    B: Array.from({ length: 30 }, (_, frame) => key(frame + 31, [frame, 0]))
  };
  const h = harness({ keys });
  h.paint(); await settle();
  assert.equal(h.paint().children.length, 62, '61 key frames plus one line');
  assert.deepEqual(h.toasts, []);
  assert.equal(h.batches.length, 0);
});

test('non-advancing authored key pagination fails visibly without looping', async () => {
  const h = harness({ badNextOffset: true, keys: { A: Array.from({ length: 17 }, (_, frame) => key(frame, [frame, 0])) } });
  h.paint(); await settle();
  assert.deepEqual(h.toasts, ['Unified Motion path unavailable: native Position key pagination did not advance.']);
  assert.equal(h.reads.filter((v) => v.kind === 'authoredKeys' && v.target.strokeId === 'A').length, 1);
  assert.equal(h.batches.length, 0);
});

test('the measured native read budget stops pathological selections with one notice', async () => {
  const selection = Array.from({ length: 1025 }, (_, i) => ({
    bounds: { center: { x: 0, y: 0 } },
    data: { strokeId: `U${i}`, nativeLayerIndex: 0, nativeStableTarget: { contextId: 'scene-root', layerUid: 'L', strokeId: `U${i}` } }
  }));
  const h = harness({ selection });
  h.paint(); await settle(); h.paint();
  assert.equal(h.reads.length, 1024);
  assert.deepEqual(h.toasts, ['Unified Motion path unavailable: this selection requires more than 1,024 native property reads.']);
  assert.equal(h.batches.length, 0);
});

test('a path with more than 64 keyed targets draws but refuses the atomic drag', async () => {
  const selection = Array.from({ length: 65 }, (_, i) => ({
    bounds: { center: { x: 100, y: 100 } },
    data: { strokeId: `S${i}`, nativeLayerIndex: 0, nativeStableTarget: { contextId: 'scene-root', layerUid: 'L', strokeId: `S${i}` } }
  }));
  const keys = Object.fromEntries(selection.map((p) => [p.data.strokeId, [key(0, [0, 0]), key(10, [10, 0])]]));
  const h = harness({ selection, keys });
  h.paint(); await settle();
  assert.equal(h.paint().children.length, 3, 'large valid target selections remain visible');
  const down = h.ev(110, 100); h.fire('pointerdown', down);
  assert.equal(down.stopped, true, 'the unified dot gesture remains owned by this path');
  h.fire('pointermove', h.ev(120, 100)); h.fire('pointerup', h.ev(120, 100)); await settle();
  assert.equal(h.batches.length, 0, 'the native 64-edit cap is checked before sending a batch');
  assert.deepEqual(h.toasts, ['Unified Motion path can be drawn, but moving it is limited to 64 keyed elements.']);
});

test('a read that loses its presented owner never installs stale unified data or writes', async () => {
  let release;
  const proofCurrent = { value: true }, readGate = new Promise((resolve) => { release = resolve; });
  const h = harness({ proofCurrent, readGate });
  h.paint(); await tick();
  proofCurrent.value = false;
  release(); await settle();
  assert.equal(h.paint().children.length, 0);
  assert.deepEqual(h.toasts, []);
  assert.equal(h.batches.length, 0);
});

test('a read whose selection changes never issues the next page or installs stale data', async () => {
  let release;
  const readGate = new Promise((resolve) => { release = resolve; });
  const h = harness({ keys: { A: Array.from({ length: 17 }, (_, frame) => key(frame, [frame, 0])) }, readGate });
  h.paint(); await tick();
  h.win.selectedPaths = [h.sel[1], h.sel[0]];
  release(); await settle();
  assert.equal(h.reads.filter((v) => v.kind === 'authoredKeys' && v.target.strokeId === 'A').length, 1);
  assert.deepEqual(h.toasts, []);
  assert.equal(h.batches.length, 0);
});

test('a drag whose selection changes during an awaited transform never writes', async () => {
  let release;
  const deltaGate = new Promise((resolve) => { release = resolve; });
  const h = harness({ deltaGate });
  h.paint(); await settle();
  h.fire('pointerdown', h.ev(210, 210));
  h.fire('pointermove', h.ev(230, 216));
  h.fire('pointerup', h.ev(230, 216)); await tick();
  h.win.selectedPaths = [h.sel[1], h.sel[0]];
  release(); await settle();
  assert.equal(h.batches.length, 0);
  assert.ok(h.toasts.includes('The document changed during the drag — nothing was changed.'));
});
