'use strict';
// #2256 A-comp-edit-timeline: the header "Select frames" mode inside an entered component. The real scene module (native-frame-cell-selection.js) is mounted on
// the component grid's caption by native-component-cell-selection.js with a component-scoped info; its presentation proof is the context controller's projection.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), vm = require('node:vm');
const Selection = require('../src/js/adapters/native-frame-cell-selection.js');
const CellSelection = require('../src/js/adapters/native-component-cell-selection.js');
const source = fs.readFileSync(require.resolve('./native-frame-cell-selection.test.cjs'), 'utf8');
const a = source.indexOf('// BEGIN REUSABLE NATIVE CELL HEADER FIXTURE'), b = source.indexOf('// END REUSABLE NATIVE CELL HEADER FIXTURE');
const makeCells = vm.compileFunction(source.slice(a, b) + '\nreturn selectionHarness;', ['assert', 'Selection'])(assert, Selection);

// The scene harness supplies the DOM, the session and the owner pins; the component state is layered over it.
function harness(options = {}) {
  const h = makeCells({ selectionMode: false, total: 6 });
  Selection.retire(h.root);
  const root = h.root;
  const ctx = { contextId: 'sym_a', totalFrames: 6, ready: true };
  const st = { contextId: 'sym_a', totalFrames: 6, rows: [{ layerUid: 'in_a' }, { layerUid: 'in_b' }, { layerUid: 'in_c' }] };
  root.state.activeSymbolId = 'sym_a';
  root._layerSel = [0, 1];
  root.NemoNativeFrameCellSelection = Selection;
  root.NemoNativeObjectProject = { desktop: () => h.session };
  root.NemoNativeComponentContext = {
    presented: () => (ctx.contextId ? { contextId: ctx.contextId, totalFrames: ctx.totalFrames, fps: 24 } : null),
    projectionReady: (host, info) => ctx.ready && info.contextId === ctx.contextId,
  };
  const mount = (frameOnly) => CellSelection.mount(root, st, h.caption, frameOnly === true);
  mount(false);
  let strip = h.caption.children[0];
  const x = (frame) => 100 + (frame + 0.5) * root.FC;
  return { h, root, st, ctx, mount, strip: () => (strip = h.caption.children[0]), toggle() { h.caption.children[0].children[4].fire('click'); },
    down(frame) { h.caption.children[0].fire('mousedown', { clientX: x(frame) }); }, move(frame) { h.doc.fire('mousemove', { clientX: x(frame) }); }, up(frame) { h.doc.fire('mouseup', { clientX: x(frame) }); } };
}

test('Select frames inside a component publishes the rectangular cells of the component rows and frames', () => {
  const f = harness();
  f.toggle();
  assert.equal(f.strip().children[4].attributes['aria-pressed'], 'true', 'the mode is on');
  f.down(1); f.move(3); f.up(3);
  assert.deepEqual(f.root._sel.frames, [1, 2, 3].flatMap((frame) => [0, 1].map((layer) => ({ layer, frame }))).sort((x, y) => x.layer - y.layer || x.frame - y.frame));
  assert.match(f.strip().attributes['aria-label'], /Selected frames 2 through 4/);
  assert.equal(f.h.info().contentRevision, 0, 'a selection writes nothing');
});

test('the info handed to the module is the session pins plus the held component rows and length', () => {
  const f = harness();
  const info = CellSelection.infoOf(f.root, f.st);
  assert.equal(info.contextId, 'sym_a'); assert.equal(info.totalFrames, 6);
  assert.deepEqual(info.layers, [{ layerUid: 'in_a' }, { layerUid: 'in_b' }, { layerUid: 'in_c' }]);
  ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'].forEach((k) => assert.equal(info[k], f.h.info()[k], k));
});

test('a gesture is dropped when the component changed, was left, is unproven or the owner moved', () => {
  for (const fault of ['context', 'left', 'unproven', 'revision', 'rows', 'frame', 'foreign-context']) {
    const f = harness(), prior = f.root._sel.frames;
    f.toggle(); f.down(0);
    if (fault === 'context') f.ctx.contextId = 'sym_b';
    if (fault === 'left') f.root.state.activeSymbolId = null;
    if (fault === 'unproven') f.ctx.ready = false;
    if (fault === 'revision') f.h.changeInfo({ contentRevision: 1 });
    if (fault === 'rows') f.root._layerSel = [2];
    if (fault === 'frame') f.root.state.currentFrame = 2;
    if (fault === 'foreign-context') f.root.state.activeSymbolId = 'sym_z';
    f.up(2); f.h.settle();
    assert.equal(f.root._sel.frames, prior, fault);
  }
});

test('a press while the component is not proven starts no capture, and the scene still refuses a component-less press', () => {
  const f = harness();
  f.toggle(); f.ctx.ready = false; f.down(0); f.up(1);
  assert.deepEqual(f.root._sel.frames, [], 'unproven component: nothing selected');
  f.ctx.ready = true; f.root.state.activeSymbolId = null; // the scene frame is not the component
  f.down(0); f.up(1);
  assert.deepEqual(f.root._sel.frames, [], 'a component strip never selects while the scene is presented');
});

test('a playback tick keeps the strip and moves the label; a different component resets the mode', () => {
  const f = harness();
  f.toggle();
  const node = f.strip();
  f.h.caption.textContent = 'Comp A - Frame 3 / 6'; // the grid moved the playhead in place (it rewrites the caption text)
  f.mount(true);
  assert.equal(f.h.caption.children[0], node, 'the strip is the one mounted before the tick');
  assert.equal(node.children[4].attributes['aria-pressed'], 'true');
  f.ctx.contextId = 'sym_b'; f.st.contextId = 'sym_b'; f.root.state.activeSymbolId = 'sym_b';
  f.mount(false);
  assert.equal(f.strip().children[4].attributes['aria-pressed'], 'false', 'entering another component starts in the ruler mode');
});

test('the Repeat dialog opens for the entered component selection and refuses once the component is left', () => {
  const f = harness();
  f.toggle(); f.down(0); f.up(1);
  let accepted = null;
  assert.equal(Selection.openRepeatDialog(f.root, (value) => { accepted = value; }, () => true), true);
  const overlay = f.h.doc.body.children.find((n) => n.className === 'native-repeat-dialog-overlay');
  assert.ok(overlay, 'the dialog is open');
  overlay.children[0].fire('submit');
  assert.equal(accepted, '2');
  Selection.retire(f.root);
  const g = harness(); g.toggle(); g.down(0); g.up(1); g.ctx.contextId = null;
  assert.equal(Selection.openRepeatDialog(g.root, () => {}, () => true), false, 'no component held: no dialog');
});
