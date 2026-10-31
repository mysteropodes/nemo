'use strict';
// #2371 Cmd/Ctrl+A on a native object project: the baseline (1065e9d8 timeline.js onKeyDown) selected every selectable item of the active layer
// (Select / Direct Select only). The native layer has no Paper children, so NemoNativeSelectGesture.selectAll reads the presented listing the
// marquee reads and commits like it; the timeline.js hook returns early when native handled and still runs the legacy branch elsewhere.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Overlay = require('../src/js/adapters/native-select-overlay.js');
const Gesture = require('../src/js/adapters/native-select-gesture.js');
const Wait = require('../src/js/adapters/native-presentation-wait.js');
const Paper = require('./helpers/native-import-paper.cjs');

function el(tag) {
  return { tag, attrs: {}, style: {}, children: [], setAttribute(k, v) { this.attrs[k] = String(v); },
    appendChild(c) { this.children.push(c); c.parentNode = this; return c; },
    remove() { if (this.parentNode) this.parentNode.children = this.parentNode.children.filter((c) => c !== this); } };
}
const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const quad = (layer, id, x0, y0, x1, y1, extra) => ({ stableTarget: { contextId: 'scene-root', layerUid: layer, frameScope: { kind: 'authored', frame: 0 }, strokeId: id },
  closed: true, count: 4, role: 'plain', filled: true, bounds: [x0, y0, x1, y1], matrix: [1, 0, 0, 1, 0, 0], segments: [seg(x0, y0), seg(x1, y0), seg(x1, y1), seg(x0, y1)],
  style: { fill: '#112233', stroke: '#445566', width: 4, opacity: 1 }, ...extra });
const tick = () => new Promise((done) => setTimeout(done, 0));

function harness(opts = {}) {
  const paper = Paper.load(), body = el('body'), toasts = [], reads = [];
  const layers = opts.layers || [{ layerUid: 'L1' }, { layerUid: 'L2' }];
  const items = opts.items || [quad('L1', 'a', 10, 10, 30, 30), quad('L1', 'b', 50, 10, 70, 30), quad('L1', 'dab', 5, 5, 8, 8, { brush: 'copy' }),
    quad('L1', 'fillc', 12, 12, 20, 20, { role: 'fill', owner: 'a' }), quad('L2', 'z', 50, 50, 70, 70)];
  let pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's1', canvasW: 200, canvasH: 200, layers };
  let readable = opts.readable !== false, redraws = 0;
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true,
    readPresentedObjectSession: (why) => { if (!readable) { if (why) why('queue-pending'); return null; } return { presentation: { ...pin, frame: 0 } }; },
    async readPresentedStrokes(selector, _pin, wanted) {
      reads.push(selector.layerUid);
      if (opts.moveOn) { opts.moveOn(); }
      return { items: items.filter((i) => i.stableTarget.layerUid === selector.layerUid).map((item) => { const { segments, ...rest } = item; return wanted(rest) ? { ...rest, segments } : rest; }) };
    } };
  const root = { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool: opts.tool || 'select', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, layers, selectedStrokeIndices: [] },
    selectedPaths: [{ data: { strokeId: 'old' } }], updateUI() { redraws++; }, SMEngineBridge: { renderNow() {} }, showToast: (t) => toasts.push(t),
    setTimeout: (fn) => globalThis.setTimeout(fn, 5), clearTimeout() {},
    document: { body, getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 200, height: 200 }) }), createElementNS: (_n, tag) => el(tag), addEventListener() {}, removeEventListener() {} },
    requestAnimationFrame: () => {},
    NemoNativeObjectProject: { desktop: () => (opts.native === false ? null : session) },
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX, y: e.clientY }), transform: () => [1, 0, 0, 1, 0, 0] } },
    NemoNativePresentationWait: Wait, NemoSelectCanvasIntent: Intent, NemoNativeSelectOverlay: Overlay, userLayers: [] };
  return { root, toasts, reads, body, redraws: () => redraws, setReadable: (v) => { readable = v; }, bump: () => { pin = { ...pin, contentRevision: 2, documentSnapshotId: 's2' }; } };
}
const ids = (h) => h.root.selectedPaths.map((p) => p.data.strokeId);

test('selectAll lists only the selectable items of the active layer and commits them', async () => {
  const h = harness();
  const r = Gesture.selectAll(h.root);
  assert.equal(await r, true);
  assert.deepEqual(ids(h), ['a', 'b'], 'no texture dab, no linked fill companion, nothing of the other layer');
  assert.deepEqual(h.reads, ['L1']);
  assert.deepEqual(h.root.selectedPaths.map((p) => p.data.nativeLayerIndex), [0, 0]);
  assert.deepEqual(h.root.state.selectedStrokeIndices, []);
  assert.ok(h.redraws() >= 1);
  assert.ok(h.body.children.some((c) => c.tag === 'svg'), 'the selection overlay is shown');
  assert.deepEqual(h.toasts, []);
  assert.deepEqual(h.root.userLayers, [], 'never touches userLayers');
});

test('Direct Select also selects all; other tools do not handle it', async () => {
  const sub = harness({ tool: 'subselect' });
  assert.equal(await Gesture.selectAll(sub.root), true);
  assert.deepEqual(ids(sub), ['a', 'b']);
  const brush = harness({ tool: 'brush' });
  assert.equal(Gesture.selectAll(brush.root), false);
  assert.deepEqual(ids(brush), ['old'], 'selection untouched');
});

test('a non-native project is not handled (the legacy path runs)', () => {
  const h = harness({ native: false });
  assert.equal(Gesture.selectAll(h.root), false);
  assert.deepEqual(ids(h), ['old']);
});

test('a locked or hidden active layer shows one toast and changes nothing', async () => {
  for (const flag of [{ locked: true }, { visible: false }]) {
    const h = harness({ layers: [{ layerUid: 'L1', ...flag }, { layerUid: 'L2' }] });
    assert.equal(await Gesture.selectAll(h.root), true);
    assert.equal(h.toasts.length, 1);
    assert.deepEqual(ids(h), ['old']);
    assert.deepEqual(h.reads, [], 'a locked or hidden layer is never read');
  }
});

test('an unreadable (stale/busy) presentation shows one toast and changes nothing', async () => {
  const h = harness({ readable: false });
  assert.equal(await Gesture.selectAll(h.root), true);
  assert.equal(h.toasts.length, 1);
  assert.deepEqual(ids(h), ['old']);
  assert.deepEqual(h.reads, []);
});

test('the queue-pending window is waited out, then it selects', async () => {
  const h = harness({ readable: false });
  const p = Gesture.selectAll(h.root);
  await new Promise((d) => setTimeout(d, 20)); h.setReadable(true);
  assert.equal(await p, true);
  assert.deepEqual(ids(h), ['a', 'b']);
});

test('a revision that moves on while reading selects nothing, silently', async () => {
  const h = harness({ moveOn: () => { h.bump(); } });
  assert.equal(await Gesture.selectAll(h.root), true);
  assert.deepEqual(ids(h), ['old']);
  assert.deepEqual(h.toasts, []);
});

test('the Cmd+A hook in timeline.js asks the native module first and returns when it handled', () => {
  const src = fs.readFileSync(require('node:path').join(__dirname, '../src/js/timeline.js'), 'utf8');
  const hook = src.indexOf('NemoNativeSelectGesture.selectAll(window)'), legacy = src.indexOf("(state.tool==='select'||state.tool==='subselect')&&userLayers[state.activeLayerIdx]");
  assert.ok(hook > 0 && legacy > hook, 'the native call precedes the legacy branch');
  assert.match(src.slice(hook, hook + 160), /event\.preventDefault\(\);\s*return;/);
});
