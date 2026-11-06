'use strict';
// #2257: the Blend row of the Motion key grid on a native project. The REAL adapters (compositing, rig command, contract, grid)
// run against a fake session that behaves like the engine for the two group edits; the REAL motion.js code (trackFor,
// renderDiscreteKeyGridRow, onDragUp) is sliced out of the source and run in a vm. The legacy path throws on any write: a
// click, a drag, a nudge and Delete must each be exactly one native command, never a pushUndo.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const Compositing = require('../src/js/adapters/native-object-layer-compositing.js');
const Grid = require('../src/js/adapters/native-blend-key-grid.js');

const motionSource = fs.readFileSync(path.join(__dirname, '../src/js/motion.js'), 'utf8');
const plain = (value) => JSON.parse(JSON.stringify(value));
const slice = (from, to) => {
  const a = motionSource.indexOf(from), b = motionSource.indexOf(to, a + 1);
  assert.ok(a > 0 && b > a, from);
  return motionSource.slice(a, b);
};
const settle = async () => { for (let i = 0; i < 20; i += 1) await new Promise((resolve) => setImmediate(resolve)); };
const COMPOSITING = 'object.layer.compositing.edit';

function rows() {
  return [
    { layerUid: 'A', name: 'A', visible: true, blendKeys: [{ frame: 2, mode: 'multiply' }, { frame: 5, mode: 'screen' }, { frame: 9, mode: 'hue' }] },
    { layerUid: 'B', name: 'B', visible: true, blendKeys: [{ frame: 3, mode: 'overlay' }] },
  ];
}
/* A session whose dispatch applies the group edits and the single removal to its rows the way the engine does (bounds 0..19, collisions refused). */
function engine(options = {}) {
  const sent = [], pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's', layers: rows() };
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, requestId: (l) => l + '-' + (sent.length + 1),
    runResourceCommand: (action) => action({
      async dispatch(request) {
        sent.push(plain(request.payload));
        const reply = (extra) => Object.assign({ apiVersion: 2, requestId: request.requestId, instanceId: 'i', documentId: 'd', contentRevision: pin.contentRevision }, extra);
        const p = request.payload, row = pin.layers.find((r) => r.layerUid === p.layerUid);
        if (options.refuse) return reply({ ok: false, error: { code: 'invalid_request', message: 'no' } });
        const frames = p.frames || [p.index];
        let keys = row.blendKeys.slice();
        const moving = keys.filter((k) => frames.includes(k.frame));
        keys = keys.filter((k) => !frames.includes(k.frame));
        if (p.edit === 'blendKeysMove') {
          const landed = moving.map((k) => ({ frame: k.frame + p.by, mode: k.mode }));
          if (landed.some((k) => k.frame < 0 || k.frame > 19 || keys.some((s) => s.frame === k.frame))) return reply({ ok: false, error: { code: 'invalid_request', message: 'no' } });
          keys = keys.concat(landed).sort((a, b) => a.frame - b.frame);
        }
        if (keys.length) row.blendKeys = keys; else delete row.blendKeys;
        pin.contentRevision += 1;
        return reply({ contentRevision: pin.contentRevision, ok: true, result: { applied: true, historyEntriesAdded: 1 } });
      },
      async reconcile() {},
    }) };
  return { session, sent, pin, comp: Compositing.create(session) };
}

/* The page: the real helpers of motion.js's key selection over the vm, a fake document that keeps the capture listeners. */
function page(e, options = {}) {
  const listeners = [], layers = e.pin.layers.map((r) => ({ layerUid: r.layerUid }));
  const logs = { refresh: 0, refused: 0, undo: 0, selections: [] };
  const win = {
    state: { layers, appMode: options.appMode || 'motion', currentFrame: 0, totalFrames: 20 },
    document: { body: {}, documentElement: {}, addEventListener: (t, fn, cap) => listeners.push({ t, fn, cap }), removeEventListener() {}, querySelector: () => null },
    nativeCompositingRefresh: () => { logs.refresh += 1; }, nativeCompositingRefused: () => { logs.refused += 1; },
  };
  win.window = win;
  const context = vm.createContext(Object.assign(win, {
    nativeCompositing: () => e.comp, NemoNativeBlendKeyGrid: Grid, state: win.state, document: win.document,
    pushUndo: () => { logs.undo += 1; throw new Error('legacy pushUndo on a native project'); },
  }));
  vm.runInContext('var _motionKeySel = []; var _keyAnchor = null;', context);
  vm.runInContext(slice('  function isKeySelected(holder, prop, key) {', '  function isMotionPropSelected'), context);
  vm.runInContext(slice('  function nativeBlendGrid() {', '  var PROP_LABEL ='), context);
  vm.runInContext('function setKeySel(sel) { _motionKeySel = sel || []; }', context);
  win.SMMotion = { keySelection: () => vm.runInContext('_motionKeySel', context), setKeySelection: (sel) => { logs.selections.push(sel.length); vm.runInContext('setKeySel', context)(sel); } };
  return { win, context, listeners, logs, layers, selected: () => vm.runInContext('_motionKeySel', context) };
}
const keydown = (key, extra = {}) => Object.assign({ key, target: { id: 'frame-grid', parentNode: null }, defaultPrevented: false, prevented: 0, stopped: 0,
  preventDefault() { this.prevented += 1; }, stopImmediatePropagation() { this.stopped += 1; } }, extra);
const press = (p, event) => { p.listeners.filter((l) => l.t === 'keydown' && l.cap).forEach((l) => l.fn(event)); return event; };

test('trackFor hands out the SAME key objects on every read, so a selection survives the re-render', () => {
  const e = engine(), p = page(e);
  const first = vm.runInContext('trackFor', p.context)(p.layers[0], 'blendKeys');
  const again = vm.runInContext('trackFor', p.context)(p.layers[0], 'blendKeys');
  assert.deepEqual(plain(first.keys), [{ frame: 2, mode: 'multiply' }, { frame: 5, mode: 'screen' }, { frame: 9, mode: 'hue' }]);
  first.keys.forEach((key, i) => assert.equal(again.keys[i], key, 'identity is stable'));
  assert.equal(vm.runInContext('trackFor', p.context)(p.layers[0], 'blendKeys').keys[1], first.keys[1]);
  vm.runInContext('setKeySel', p.context)([{ holder: p.layers[0], prop: 'blendKeys', key: first.keys[1] }]);
  assert.equal(vm.runInContext('isKeySelected', p.context)(p.layers[0], 'blendKeys', again.keys[1]), true);
});

test('Delete over the selected diamonds is ONE native command per layer, and the selection is gone afterwards', async () => {
  const e = engine(), p = page(e);
  const track = vm.runInContext('trackFor', p.context)(p.layers[0], 'blendKeys');
  vm.runInContext('setKeySel', p.context)([0, 2].map((i) => ({ holder: p.layers[0], prop: 'blendKeys', key: track.keys[i] })));
  const event = press(p, keydown('Delete'));
  assert.equal(event.prevented, 1, 'the press is taken: no legacy deleteSelectedKeys guard throws');
  assert.equal(event.stopped, 1);
  await settle();
  assert.deepEqual(e.sent, [{ command: COMPOSITING, layerUid: 'A', edit: 'blendKeysRemove', frames: [2, 9] }]);
  assert.deepEqual(plain(e.pin.layers[0].blendKeys), [{ frame: 5, mode: 'screen' }]);
  assert.equal(p.selected().length, 0);
  assert.equal(p.logs.refresh, 1);
  assert.equal(p.logs.undo, 0);
});

test('Alt+Arrow nudges the selection together; a refused nudge keeps the keys and the selection where they were', async () => {
  const e = engine(), p = page(e);
  const track = vm.runInContext('trackFor', p.context)(p.layers[0], 'blendKeys');
  vm.runInContext('setKeySel', p.context)([1, 2].map((i) => ({ holder: p.layers[0], prop: 'blendKeys', key: track.keys[i] })));
  press(p, keydown('ArrowRight', { altKey: true }));
  await settle();
  assert.deepEqual(e.sent[0], { command: COMPOSITING, layerUid: 'A', edit: 'blendKeysMove', frames: [5, 9], by: 1 });
  assert.deepEqual(plain(e.pin.layers[0].blendKeys), [{ frame: 2, mode: 'multiply' }, { frame: 6, mode: 'screen' }, { frame: 10, mode: 'hue' }]);
  assert.deepEqual(plain(p.selected().map((s) => s.key.frame)), [6, 10], 'the selection follows the keys');
  press(p, keydown('ArrowLeft', { altKey: true }));
  await settle();
  assert.deepEqual(e.sent[1], { command: COMPOSITING, layerUid: 'A', edit: 'blendKeysMove', frames: [6, 10], by: -1 });
  assert.deepEqual(plain(p.selected().map((s) => s.key.frame)), [5, 9]);
  for (let i = 0; i < 3; i += 1) { press(p, keydown('ArrowLeft', { altKey: true })); await settle(); }
  assert.equal(e.sent.length, 5);
  assert.equal(p.logs.refused, 1, 'the third step would land 3 on the staying key at 2');
  assert.deepEqual(plain(e.pin.layers[0].blendKeys), [{ frame: 2, mode: 'multiply' }, { frame: 3, mode: 'screen' }, { frame: 7, mode: 'hue' }]);
  assert.deepEqual(plain(p.selected().map((s) => s.key.frame)), [3, 7], 'still selected where they are');
});

test('a selection across two layers is applied layer by layer', async () => {
  const e = engine(), p = page(e);
  const a = vm.runInContext('trackFor', p.context)(p.layers[0], 'blendKeys'), b = vm.runInContext('trackFor', p.context)(p.layers[1], 'blendKeys');
  vm.runInContext('setKeySel', p.context)([{ holder: p.layers[0], prop: 'blendKeys', key: a.keys[0] }, { holder: p.layers[1], prop: 'blendKeys', key: b.keys[0] }]);
  press(p, keydown('Backspace'));
  await settle();
  assert.deepEqual(e.sent.map((r) => [r.layerUid, r.edit, r.frames]), [['A', 'blendKeysRemove', [2]], ['B', 'blendKeysRemove', [3]]]);
  assert.equal(e.pin.layers[1].blendKeys, undefined, 'the last key removes the member');
});

test('the keys are left alone with nothing selected, in an editable field, a modal, outside Motion, or with other keys selected', async () => {
  const e = engine();
  let p = page(e);
  assert.equal(press(p, keydown('Delete')).prevented, 0, 'no selection');
  const track = vm.runInContext('trackFor', p.context)(p.layers[0], 'blendKeys');
  const select = (q) => vm.runInContext('setKeySel', q.context)([{ holder: q.layers[0], prop: 'blendKeys', key: vm.runInContext('trackFor', q.context)(q.layers[0], 'blendKeys').keys[0] }]);
  select(p);
  assert.equal(press(p, keydown('Delete', { target: { tagName: 'INPUT', id: '', parentNode: null } })).prevented, 0, 'a text field');
  p.win.document.querySelector = () => ({});
  assert.equal(press(p, keydown('Delete')).prevented, 0, 'a modal');
  p = page(e, { appMode: 'anim2d' }); select(p);
  assert.equal(press(p, keydown('Delete')).prevented, 0, 'outside Motion');
  p = page(e); select(p);
  assert.equal(press(p, keydown('ArrowRight')).prevented, 0, 'a bare arrow steps the playhead');
  assert.equal(press(p, keydown('Delete', { target: { id: 'somewhere-else', parentNode: null } })).prevented, 0, 'off the timeline');
  const other = {};
  vm.runInContext('setKeySel', p.context)([{ holder: p.layers[0], prop: 'blendKeys', key: track.keys[0] }, { holder: p.layers[0], prop: 'position', key: other }]);
  const mixed = press(p, keydown('Delete'));
  assert.equal(mixed.prevented, 0, 'other keys selected: the press stays with their owners');
  await settle();
  assert.equal(e.sent.length, 1, 'but the Blend keys are still removed');
  assert.equal(e.sent[0].frames[0], 2);
});

/* The diamond handlers and the end of a drag are the REAL motion.js code. */
function row(p, layer) {
  const node = () => ({ className: '', style: {}, children: [], dataset: {}, title: '', handlers: {}, classList: { add() {}, toggle() {} },
    appendChild(child) { this.children.push(child); child.parentNode = this; return child; }, addEventListener(type, fn) { this.handlers[type] = fn; } });
  const g = { grid: node(), goTo: [], renders: 0, marquee: 0 };
  Object.assign(p.win, { createElement: node });
  p.win.document.createElement = node;
  const ctx = p.context;
  Object.assign(ctx, { FC: 12, goToFrame: (f) => g.goTo.push(f), renderTimeline: () => { g.renders += 1; }, renderLayerList() {}, startMarquee: () => { g.marquee += 1; },
    motionDragScrollLeft: () => 0, applyMarqueeSelection() {}, SM: { t: (k) => k }, SMEngineBridge: { renderNow() {} }, showContextMenu: (x, y, items) => { g.menu = items; } });
  p.win.showContextMenu = ctx.showContextMenu;
  vm.runInContext(slice('  function renderDiscreteKeyGridRow(', '  // Image mesh accordion'), ctx);
  const comp = p.win.nativeCompositing ? ctx.nativeCompositing() : null;
  const keys = vm.runInContext('trackFor', ctx)(layer, 'blendKeys').keys;
  vm.runInContext('renderDiscreteKeyGridRow', ctx)(g.grid, layer, 'blendKeys', 'Blend', keys, (frame) => comp.removeBlendKey(p.layers.indexOf(layer), frame));
  g.row = g.grid.children[0];
  g.diamond = (frame) => g.row.children[frame].children[0];
  return g;
}

test('a drag of the selected diamonds is one native move on release, with no legacy checkpoint', async () => {
  const e = engine(), p = page(e), g = row(p, p.layers[0]);
  assert.ok(g.diamond(2) && g.diamond(5) && g.diamond(9) && !g.diamond(3));
  // Click the diamond at 5, then press on it again to start the group drag (the generic handler of motion.js).
  g.diamond(5).handlers.mousedown({ stopPropagation() {}, preventDefault() {}, clientX: 100, metaKey: false, ctrlKey: false, shiftKey: false });
  assert.equal(p.selected().length, 1);
  assert.equal(p.selected()[0].key.frame, 5);
  assert.equal(p.logs.undo, 0, 'no legacy pushUndo');
  assert.equal(g.goTo[0], 5);
  const drag = p.context._motionKeyDrag;
  assert.ok(drag && drag.group);
  assert.equal(drag.keys[0].origFrame, 5);
  drag.keys[0].key.frame = 8; // what onDragMove does live
  const onDragUp = slice('  function onDragUp() {', "  document.addEventListener('mousemove', onDragMove);");
  Object.assign(p.context, { onLayerSkewUp: () => false, stopMotionAutoScroll() {}, endMarquee() {}, clearMotionSnapGuide() {}, flushMotionDragTimelineRender() {}, loadFrame() {} });
  vm.runInContext(onDragUp, p.context);
  vm.runInContext('onDragUp', p.context)();
  await settle();
  assert.deepEqual(e.sent, [{ command: COMPOSITING, layerUid: 'A', edit: 'blendKeysMove', frames: [5], by: 3 }]);
  assert.deepEqual(plain(e.pin.layers[0].blendKeys), [{ frame: 2, mode: 'multiply' }, { frame: 8, mode: 'screen' }, { frame: 9, mode: 'hue' }]);
  assert.deepEqual(plain(p.selected().map((s) => s.key.frame)), [8], 'the selection follows the key');
  assert.equal(p.context.window._motionKeyDrag, null);
});

test('releasing a drag where it started writes nothing; a refused drag puts the diamond back', async () => {
  const e = engine({ refuse: true }), p = page(e), g = row(p, p.layers[0]);
  g.diamond(5).handlers.mousedown({ stopPropagation() {}, preventDefault() {}, clientX: 100 });
  const drag = p.context._motionKeyDrag;
  const onDragUp = slice('  function onDragUp() {', "  document.addEventListener('mousemove', onDragMove);");
  Object.assign(p.context, { onLayerSkewUp: () => false, stopMotionAutoScroll() {}, endMarquee() {}, clearMotionSnapGuide() {}, flushMotionDragTimelineRender() {}, loadFrame() {} });
  vm.runInContext(onDragUp, p.context);
  vm.runInContext('onDragUp', p.context)();
  await settle();
  assert.equal(e.sent.length, 0, 'not moved: no command');
  p.context._motionKeyDrag = drag; p.context.window._motionKeyDrag = drag;
  drag.keys[0].key.frame = 7;
  vm.runInContext('onDragUp', p.context)();
  await settle();
  assert.equal(e.sent.length, 1);
  assert.equal(p.logs.refused, 1);
  assert.equal(drag.keys[0].key.frame, 5, 'the live copy is put back');
  assert.deepEqual(plain(e.pin.layers[0].blendKeys.map((k) => k.frame)), [2, 5, 9]);
  assert.deepEqual(plain(p.selected().map((s) => s.key.frame)), [5]);
});

test('the context-menu delete of a diamond is one native removal without the legacy checkpoint', async () => {
  const e = engine(), p = page(e), g = row(p, p.layers[0]);
  g.diamond(9).handlers.contextmenu({ preventDefault() {}, stopPropagation() {}, clientX: 1, clientY: 2 });
  g.menu[0].action();
  await settle();
  assert.equal(p.logs.undo, 0);
  assert.deepEqual(e.sent, [{ command: COMPOSITING, layerUid: 'A', edit: 'blendKeyRemove', index: 9 }]);
});
