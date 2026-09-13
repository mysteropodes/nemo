'use strict';
// P21 — frame-only history entry: capture / apply lifecycle.
//
// Pure half drives application/history/frame-entry.js on plain documents.
// Characterization half runs the REAL tweens.js `pushUndoActiveFrame`,
// `undo` and `redo` sliced from source beside the real module and the real
// `_cloneStrokesForUndo`, with the UI hooks stubbed, and reproduces the
// H02/#1059 scenarios: A (capture → mutate → undo → redo, correct) and B
// (undo from a different frame — the inverse entry is recorded from the
// viewed frame, a limitation this leaf preserves rather than repairs).

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const root = path.resolve(__dirname, '..');
const modulePath = path.join(root, 'src/js/application/history/frame-entry.js');
const Entry = require(modulePath);
const moduleSource = fs.readFileSync(modulePath, 'utf8');
const tweensSource = fs.readFileSync(path.join(root, 'src/js/tweens.js'), 'utf8');

function stroke(x) { return { segments: [{ point: [x, 0], handleIn: [0, 0], handleOut: [0, 0] }], strokeColor: '#000' }; }
function frame(x, key) { return { strokes: [stroke(x)], isKeyframe: !!key, isInterpolated: false }; }
function document2(frames) {
  return { currentFrame: 0, layers: [
    { name: 'A', frames: Array.from({ length: frames }, (_, i) => frame(100 + i, i === 0)) },
    { name: 'B', frames: Array.from({ length: frames }, (_, i) => frame(300 + i, i === 0)) },
  ] };
}
const clone = (s) => (s === undefined ? undefined : JSON.parse(JSON.stringify(s)));   // like _cloneStrokesForUndo: undefined in, undefined out
const xs = (doc, f) => doc.layers.map((l) => l.frames[f].strokes[0].segments[0].point[0]);

// ---- pure lifecycle --------------------------------------------------------

test('capture: one element per layer in index order, frame-only, no type, missing frame falls back to an empty record', () => {
  const doc = document2(2);
  doc.layers[1].frames.length = 1;   // layer B has no frame 1
  const e = Entry.capture(doc, 1, clone);
  assert.deepEqual(Object.keys(e), ['frame', 'layers']);
  assert.equal(Entry.isFrameEntry(e), true);
  assert.equal(Entry.isFrameEntry({ type: 'layers' }), false);
  assert.equal(e.frame, 1);
  assert.equal(e.layers.length, 2);
  assert.deepEqual(e.layers[0], { strokes: [stroke(101)], isKeyframe: false, isInterpolated: false });
  assert.deepEqual(e.layers[1], { strokes: undefined, isKeyframe: undefined, isInterpolated: undefined }, 'a layer without that frame captures an empty record, as before');
  assert.notEqual(e.layers[0].strokes, doc.layers[0].frames[1].strokes, 'strokes go through the cloner');
});

test('apply writes the entry frame into every overlapping layer, moves the playhead, and returns the inverse; undo→redo round-trips', () => {
  const doc = document2(3);
  const before = Entry.capture(doc, 0, clone);
  doc.layers[0].frames[0].strokes[0].segments[0].point[0] = 1099;
  doc.layers[1].frames[0].strokes[0].segments[0].point[0] = 1188;
  doc.layers[1].frames[0].isKeyframe = false;
  const undone = Entry.apply(doc, before);
  assert.deepEqual(xs(doc, 0), [100, 300]);
  assert.equal(doc.layers[1].frames[0].isKeyframe, true);
  assert.deepEqual({ frame: undone.frame, moved: undone.moved }, { frame: 0, moved: false });
  assert.equal(Entry.isFrameEntry(undone.inverse), true);
  const redone = Entry.apply(doc, undone.inverse);
  assert.deepEqual(xs(doc, 0), [1099, 1188]);
  assert.equal(doc.layers[1].frames[0].isKeyframe, false);
  assert.deepEqual(xs(doc, 1), [101, 301], 'other frames untouched');
  assert.equal(Entry.isFrameEntry(redone.inverse), true);
});

test('apply only touches min(entry.layers, doc.layers) layers and installs the entry strokes by reference (as before)', () => {
  const doc = document2(1);
  const e = Entry.capture(doc, 0, clone);
  doc.layers.push({ name: 'C', frames: [frame(500, true)] });
  Entry.apply(doc, e);
  assert.deepEqual(xs(doc, 0), [100, 300, 500]);
  assert.equal(doc.layers[0].frames[0].strokes, e.layers[0].strokes);
  const short = { frame: 0, layers: [{ strokes: [stroke(7)], isKeyframe: true, isInterpolated: true }] };
  Entry.apply(doc, short);
  assert.deepEqual(xs(doc, 0), [7, 300, 500]);
});

test('two documents are independent: an entry captured from one never reaches the other', () => {
  const a = document2(1), b = document2(1);
  const ea = Entry.capture(a, 0, clone);
  b.layers[0].frames[0].strokes[0].segments[0].point[0] = 42;
  Entry.apply(a, ea);
  assert.deepEqual(xs(a, 0), [100, 300]);
  assert.deepEqual(xs(b, 0), [42, 300]);
});

test('H02 limitation 1 (preserved): applying from a different frame records the VIEWED frame as the inverse', () => {
  const doc = document2(4);
  const e0 = Entry.capture(doc, 0, clone);
  doc.layers[0].frames[0].strokes[0].segments[0].point[0] = 1599;
  doc.currentFrame = 3;
  const r = Entry.apply(doc, e0);
  assert.deepEqual({ frame: r.frame, moved: r.moved }, { frame: 0, moved: true });
  assert.deepEqual(xs(doc, 0), [100, 300], 'frame 0 is restored correctly');
  // The inverse is frame 3's content, not frame 0's pre-restore content — H02 Scenario B, kept as-is.
  assert.equal(r.inverse.frame, 3);
  assert.deepEqual(r.inverse.layers.map((l) => l.strokes[0].segments[0].point[0]), [103, 303]);
});

test('H02 limitation 2 (preserved): frame entries carry no context and apply regardless of the active symbol/montage', () => {
  const doc = Object.assign(document2(1), { activeSymbolId: 'sym-1', activeMontageViewId: null });
  const e = Entry.capture(doc, 0, clone);
  assert.equal('symbolId' in e, false); assert.equal('montageViewId' in e, false);
  doc.activeSymbolId = null;
  doc.layers[0].frames[0].strokes[0].segments[0].point[0] = 9;
  Entry.apply(doc, e);
  assert.deepEqual(xs(doc, 0), [100, 300], 'no guard: the entry is applied to whatever document is handed in');
});

// ---- characterization of the real tweens.js functions ----------------------

function slice(source, startMarker, endMarker, label) {
  const start = source.indexOf(startMarker);
  assert.notEqual(start, -1, `${label}: start marker not found — the source moved`);
  const end = source.indexOf(endMarker, start + startMarker.length);
  assert.notEqual(end, -1, `${label}: end marker not found — the source moved`);
  return source.slice(start, end);
}

function tweensContext(doc) {
  const hooks = { loadFrame: [], toasts: [], saved: 0, labels: 0 };
  const state = Object.assign({ undoStack: [], undoLabels: [], redoStack: [], redoLabels: [], maxUndo: 50, activeSymbolId: null, activeMontageViewId: null }, doc);
  const context = {
    console, JSON, Object, Array, Math, state, window: null,
    _HEAVY_STROKE_FIELDS: ['src', 'bitmapPressureProfile'], _scrubLiveActive: false,
    saveActiveLayerFrame() { hooks.saved++; }, _actionLabelNow() { hooks.labels++; return 'label' + hooks.labels; },
    loadFrame(i) { hooks.loadFrame.push(i); }, renderOS() {}, renderArcs() {}, updateUI() {},
    showToast(m) { hooks.toasts.push(m); }, SM: { t(k) { return k; } },
    layersSnapshotNow() { throw new Error('type:layers path must not be taken by frame entries'); },
    restoreLayersSnapshot() { throw new Error('type:layers path must not be taken by frame entries'); },
    _undoContextLabel() { return ''; },
  };
  context.window = context;
  vm.createContext(context);
  vm.runInContext(moduleSource, context, { filename: 'frame-entry.js' });
  const parts = [
    slice(tweensSource, 'function _cloneStrokesForUndo(strokes){', '\n// Frame-scoped undo entry', '_cloneStrokesForUndo'),
    slice(tweensSource, 'function pushUndoActiveFrame(){', '\n// ', 'pushUndoActiveFrame'),
    slice(tweensSource, 'function undo(){', '\n// Both branches below', 'undo'),
    slice(tweensSource, 'function redo(){', '\n\n// ---- MANUAL INBETWEEN', 'redo'),
  ];
  vm.runInContext(parts.join('\n'), context, { filename: 'tweens-history.js' });
  return { ctx: context, state, hooks };
}

test('Scenario A (real tweens.js): capture → mutate both layers → undo → redo restores exactly', () => {
  const { ctx, state, hooks } = tweensContext(document2(2));
  state.layers[0].frames[0].strokes[0].src = 'data:image/png;base64,AAA';   // a heavy field, shared by reference
  ctx.pushUndoActiveFrame();
  assert.equal(state.undoStack.length, 1); assert.equal(state.undoLabels.length, 1); assert.equal(hooks.saved, 1);
  assert.equal(state.undoStack[0].layers[0].strokes[0].src, state.layers[0].frames[0].strokes[0].src, 'heavy string shared by reference');
  assert.equal(state.layers[0].frames[0].strokes[0].src, 'data:image/png;base64,AAA', 'live tree restored after the clone');
  state.layers[0].frames[0].strokes[0].segments[0].point[0] = 1099;
  state.layers[1].frames[0].strokes[0].segments[0].point[0] = 1188;
  ctx.undo();
  assert.deepEqual(xs(state, 0), [100, 300]);
  assert.deepEqual([state.undoStack.length, state.redoStack.length, JSON.parse(JSON.stringify(state.redoLabels))], [0, 1, ['label1']]);
  assert.deepEqual(hooks.loadFrame, [0]);
  ctx.redo();
  assert.deepEqual(xs(state, 0), [1099, 1188]);
  assert.deepEqual([state.undoStack.length, state.redoStack.length, JSON.parse(JSON.stringify(state.undoLabels))], [1, 0, ['label1']]);
  assert.deepEqual(hooks.toasts, []);
});

test('Scenario B (real tweens.js): undo from another frame jumps back and restores, but the redo entry is the viewed frame', () => {
  const { ctx, state, hooks } = tweensContext(document2(4));
  ctx.pushUndoActiveFrame();                                               // captures frame 0 at 100/300
  state.layers[0].frames[0].strokes[0].segments[0].point[0] = 1599;
  state.currentFrame = 3;
  ctx.undo();
  assert.equal(state.currentFrame, 0);
  assert.deepEqual(xs(state, 0), [100, 300]);
  assert.deepEqual(hooks.loadFrame, [0]);
  assert.equal(state.redoStack[0].frame, 3, 'H02 Scenario B, preserved: redo entry captured from the viewed frame');
  ctx.redo();
  assert.equal(state.currentFrame, 3, 'redo jumps to frame 3 and rewrites it with its own content (no-op), frame 0 stays restored');
  assert.deepEqual(xs(state, 0), [100, 300]);
});

test('empty stacks toast and change nothing; one capture per call is exactly one entry', () => {
  const { ctx, state, hooks } = tweensContext(document2(1));
  ctx.undo(); ctx.redo();
  assert.deepEqual(hooks.toasts, ['toastNothingToUndo', 'toastNothingToRedo']);
  ctx.pushUndoActiveFrame(); ctx.pushUndoActiveFrame();
  assert.equal(state.undoStack.length, 2);
  ctx._scrubLiveActive = true; ctx.pushUndoActiveFrame();
  assert.equal(state.undoStack.length, 2, 'the scrub guard is unchanged');
});

test('routing guard: tweens.js captures and applies through the module; the duplicated inline restore blocks are gone', () => {
  assert.equal((tweensSource.match(/NemoFrameHistoryEntry\.apply\(state,s\)/g) || []).length, 2, 'undo and redo');
  assert.equal((tweensSource.match(/NemoFrameHistoryEntry\.capture\(state,state\.currentFrame,_cloneStrokesForUndo\)/g) || []).length, 1);
  assert.doesNotMatch(tweensSource, /var cur=\{frame:state\.currentFrame,layers:\[\]\}/, 'inline inverse construction retired');
  assert.doesNotMatch(tweensSource, /tf\.isInterpolated=s\.layers\[i2\]\.isInterpolated/, 'inline frame write retired');
  for (const name of ['function pushUndoActiveFrame(){', 'function undo(){', 'function redo(){']) assert.ok(tweensSource.includes(name), name + ' keeps its name (bootstrap monkey-patches by name)');
  const code = moduleSource.split('\n').filter((l) => !/^\s*\/\//.test(l)).join('\n');
  assert.doesNotMatch(code, /\bwindow\.|\bstate\.|\bSM[A-Z]|loadFrame|renderOS|updateUI/, 'the module reads no globals and owns no UI refresh');
});
