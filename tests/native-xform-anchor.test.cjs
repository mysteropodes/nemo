'use strict';
// #2372 xform-anchor-grid: the nine pivot dots work on a native selection without any legacy save. The real timeline.js dot listeners run in a
// sandbox; the pivot is a session preference (state), never stamped on the strokes nor saved, and sp-rot pivots about the named bounds point.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const Style = require('../src/js/adapters/native-selection-style.js');
const Transform = require('../src/js/adapters/native-selection-transform.js');

const TIMELINE = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'timeline.js'), 'utf8');
const TOOLS = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'tools.js'), 'utf8');
function extract(src, name) {
  const from = src.indexOf('function ' + name + '(');
  assert.ok(from >= 0, name);
  let depth = 0;
  for (let i = src.indexOf('{', from); i < src.length; i++) { if (src[i] === '{') depth++; else if (src[i] === '}' && --depth === 0) return src.slice(from, i + 1); }
  throw new Error('unterminated ' + name);
}
const KEYS = ['tl', 'tc', 'tr', 'ml', 'mc', 'mr', 'bl', 'bc', 'br'];
// bounds 20,10 .. 100,50 : named points by key
const B = { x: 20, y: 10, width: 80, height: 40 };
const POINT = { tl: [20, 10], tc: [60, 10], tr: [100, 10], ml: [20, 30], mc: [60, 30], mr: [100, 30], bl: [20, 50], bc: [60, 50], br: [100, 50] };

function harness({ native }) {
  const log = [], timers = [], matrices = [], toasts = [];
  const dots = KEYS.map((key) => { const cls = new Set(); return { dataset: { key }, classList: { toggle: (c, on) => (on ? cls.add(c) : cls.delete(c)), contains: (c) => cls.has(c) }, click: null,
    addEventListener(type, fn) { if (type === 'click') this.click = fn; } }; });
  const state = { tool: 'select', appMode: 'anim2d', xformAnchorKey: 'mc', xformAnchorCustom: null, selRotAccum: 0 };
  const stroke = { data: { nativeStableTarget: {}, nativeLayerIndex: 0, strokeId: 'a' } };
  const root = { state, selectedPaths: [stroke], showToast: (t) => toasts.push(t), updateUI() {}, setTimeout: (fn) => { timers.push(fn); return timers.length; }, clearTimeout() {},
    SMEngineBridge: { renderNow: () => log.push('renderNow') },
    NemoNativeObjectProject: { desktop: () => ({ blocksLegacy: () => native }) },
    NemoNativeObjectLabs: { desktop: () => ({}) },
    NemoSelectCanvasIntent: { pickContext: () => 'c', presentedState: () => ({ pin: {} }) },
    NemoNativeSelectOverlay: { selectionContext: () => 'c', show() {} },
    NemoNativeSelectGesture: { commitMatrix: async (_r, _p, m) => { matrices.push(m.slice()); return 'applied'; } },
    NemoNativeSelectionStyle: Style, NemoNativeSelectionTransform: Transform };
  const sandbox = { window: root, state, selectedPaths: root.selectedPaths, Point: function (x, y) { this.x = x; this.y = y; }, Object, Math, parseFloat, _nativeTyped: {},
    document: { querySelectorAll: () => dots, getElementById: () => null },
    saveActiveLayerFrame: () => { log.push('saveActiveLayerFrame'); throw new Error('NemoNativeReleaseRequired'); },
    renderTransformHandles: () => log.push('renderTransformHandles'), SMEngineBridge: root.SMEngineBridge,
    xformSelBounds: () => B, activeXformVertexMode: () => false };
  root.renderTransformHandles = sandbox.renderTransformHandles;
  vm.createContext(sandbox);
  const wire = TIMELINE.slice(TIMELINE.indexOf("document.querySelectorAll('#xform-anchor-grid .xa-dot').forEach(function(btn){\n  btn.addEventListener('click'"),
    TIMELINE.indexOf("renderXformAnchorGrid();\ndocument.querySelectorAll('#align-toolbar"));
  vm.runInContext([extract(TOOLS, 'xformAnchorPoint'), TOOLS.match(/var XFORM_ANCHOR_PROP=\{[^}]*\};/)[0], extract(TIMELINE, 'renderXformAnchorGrid'),
    extract(TIMELINE, 'xformAnchorPick'), extract(TIMELINE, 'nativeXformRead'), wire,
    // xformSelBounds in tools.js yields a Rectangle with named points; the stand-in answers the same getters
    'xformSelBounds = function(){var b=' + JSON.stringify(B) + ';b.topLeft=new Point(20,10);b.topCenter=new Point(60,10);b.topRight=new Point(100,10);b.leftCenter=new Point(20,30);b.center=new Point(60,30);b.rightCenter=new Point(100,30);b.bottomLeft=new Point(20,50);b.bottomCenter=new Point(60,50);b.bottomRight=new Point(100,50);return b;};',
  ].join('\n'), sandbox);
  const dot = (k) => dots.find((d) => d.dataset.key === k);
  return { root, state, dots, log, timers, matrices, stroke, toasts, sandbox, dot,
    click: (k) => dot(k).click({ stopPropagation() {} }),
    active: () => dots.filter((d) => d.classList.contains('xa-active')).map((d) => d.dataset.key) };
}

test('a dot click on a native selection never saves through the legacy writer and repaints the active dot', () => {
  const h = harness({ native: true });
  for (const k of KEYS) {
    assert.doesNotThrow(() => h.click(k), k);
    assert.deepEqual(h.active(), [k], 'exactly the picked dot is active');
    assert.equal(h.state.xformAnchorKey, k);
  }
  assert.ok(!h.log.includes('saveActiveLayerFrame'), 'saveActiveLayerFrame never reached');
  assert.equal(h.log.filter((l) => l === 'renderTransformHandles').length, KEYS.length, 'the handles are redrawn on every pick');
  assert.equal('xformAnchorKey' in h.stroke.data, false, 'nothing is stamped on the selected item');
});

test('a preset pick overrides a custom Alt anchor on a native selection', () => {
  const h = harness({ native: true });
  h.state.xformAnchorCustom = [5, 5];
  assert.deepEqual(h.active(), [], 'no dot is active while a custom anchor rules');
  h.click('tr');
  assert.equal(h.state.xformAnchorCustom, null);
  assert.deepEqual(h.active(), ['tr']);
});

test('sp-rot on a native selection pivots about the chosen dot: the batch matrix turns about the named bounds point', async () => {
  for (const k of KEYS) {
    const h = harness({ native: true });
    h.click(k);
    const read = h.sandbox.nativeXformRead();
    assert.deepEqual([read.anchor.x, read.anchor.y], POINT[k], 'the job anchor is the named point of the selection bounds: ' + k);
    const done = Transform.edit(h.root, { field: 'rot', value: 90, lock: false, vertex: false, read: h.sandbox.nativeXformRead });
    h.timers.splice(0).forEach((fn) => fn());
    assert.equal((await done).status, 'applied');
    const [a, b, c, d, e, f] = h.matrices[0];
    assert.deepEqual([Math.round(a), Math.round(b), Math.round(c), Math.round(d)], [0, 1, -1, 0]);
    const [px, py] = POINT[k];
    assert.ok(Math.abs(a * px + c * py + e - px) < 1e-9 && Math.abs(b * px + d * py + f - py) < 1e-9, 'the anchor is the fixed point of the turn: ' + k);
  }
});

test('a legacy project keeps stamping the strokes and saving the frame, unchanged', () => {
  const h = harness({ native: false });
  h.stroke.data = {};
  h.state.xformAnchorCustom = [1, 2];
  assert.throws(() => h.click('br'), /NemoNativeReleaseRequired/, 'the legacy path still reaches saveActiveLayerFrame');
  assert.equal(h.stroke.data.xformAnchorKey, 'br');
  assert.equal(h.state.xformAnchorKey, 'br');
  assert.equal(h.state.xformAnchorCustom, null);
  assert.deepEqual(h.log, ['saveActiveLayerFrame']);
});
