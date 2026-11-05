'use strict';
// #2371 U11: Layer Solo (the S button of a layer row, the folder row included) on a NATIVE object project. Baseline 1065e9d8
// (timeline.js toggleLayerSolo, app.js layerIsEffectivelyVisible): a session view fact, never serialized; the preview request and the
// export request carry the soloed layer uids and the engine paints only those layers. The rows are the real native-object-ui +
// layer-list presenter output; the cell is the real native-layer-solo.js. A toggle is no command: nothing is dispatched, no
// revision, history or dirty state moves, and the legacy toggleLayerSolo is never called.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const UI = require('../src/js/adapters/native-object-ui.js');
const Files = require('../src/js/adapters/native-object-project-file.js');
const Solo = require('../src/js/adapters/native-layer-solo.js');
const Exposure = require('../src/js/adapters/native-object-open-exposure.js');
const Exports = require('../src/js/adapters/native-object-project-export.js');

function collection(kids) {
  const out = Object.create({ item(i) { return kids[i] || null; } });
  kids.forEach((k, i) => { out[i] = k; });
  Object.defineProperty(out, 'length', { value: kids.length });
  return Object.freeze(out);
}
function element(tagName) {
  const classes = new Set(), listeners = {}, kids = [];
  const el = { tagName, style: {}, dataset: {}, attributes: {}, textContent: '', type: '', title: '', value: '', hidden: false, disabled: false,
    classList: { add: (x) => classes.add(x), remove: (x) => classes.delete(x), contains: (x) => classes.has(x), toggle(x, on) { if (on === undefined) on = !classes.has(x); if (on) classes.add(x); else classes.delete(x); return on; } },
    setAttribute(k, v) { this.attributes[k] = String(v); }, addEventListener(k, f) { listeners[k] = f; }, removeEventListener() {},
    replaceChildren(...next) { kids.splice(0).forEach((k) => { k.parentNode = null; }); next.forEach((k) => this.appendChild(k)); },
    appendChild(k) { if (k.parentNode) k.parentNode.removeChild(k); kids.push(k); k.parentNode = this; return k; },
    removeChild(k) { kids.splice(kids.indexOf(k), 1); k.parentNode = null; },
    querySelector: () => null, contains(k) { while (k) { if (k === this) return true; k = k.parentNode; } return false; },
    click() { let stopped = false; const r = listeners.click && listeners.click({ target: this, preventDefault() {}, stopPropagation() { stopped = true; } }); this.stopped = stopped; return r; } };
  Object.defineProperty(el, 'className', { get: () => [...classes].join(' '), set(v) { classes.clear(); String(v).split(/\s+/).filter(Boolean).forEach((x) => classes.add(x)); } });
  Object.defineProperty(el, 'children', { get: () => collection(kids) });
  Object.defineProperty(el, 'innerHTML', { set() { this.replaceChildren(); } });
  return el;
}
const settle = async () => { for (let i = 0; i < 6; i += 1) await new Promise((resolve) => setImmediate(resolve)); };

const LAYERS = [{ layerUid: 'A', name: 'A', visible: true, kind: 'draw' }, { layerUid: 'F', name: 'Folder', visible: true, kind: 'folder' },
  { layerUid: 'C', name: 'C', visible: false, kind: 'draw', parentLayerUid: 'F' }];
function metadata(generation) {
  return { apiVersion: 2, instanceId: 'instance', documentId: 'document', lifecycleGeneration: generation || 1, contentRevision: 3, documentSnapshotId: 'document-snapshot-3',
    documentDigest: '4'.repeat(64), byteCount: 1000, canvasW: 320, canvasH: 180, fps: 24, totalFrames: 24, reference: null, undoDepth: 0, redoDepth: 0, layers: LAYERS.map((l) => ({ ...l })) };
}
function harness({ native = true, refuse = false } = {}) {
  const nodes = new Map(['layer-list', 'frame-hdr', 'frame-grid', 'tl-cf', 'tl-tf', 'info-frame', 'info-badge', 'info-strokes', 'info-sel'].map((id) => [id, Object.assign(element('div'), { id })]));
  const doc = { nodes, activeElement: null, getElementById: (id) => nodes.get(id) || null, createElement: element, createComment: () => element('#comment'), addEventListener() {}, removeEventListener() {} };
  let info = metadata(1);
  const presented = [], toasts = [], dispatched = [];
  const session = { isActive: () => true, info: () => info, blocksLegacy: () => true,
    presentPreview: (frame, superseded) => { presented.push([frame, superseded, Solo.read(root, info).slice()]); return refuse ? Promise.reject(new Error('refused')) : Promise.resolve({}); },
    dispatch: (...a) => { dispatched.push(a); return Promise.resolve({ ok: true }); } };
  let legacy = 0;
  const root = { state: { currentFrame: 5, activeLayerIdx: 0, appMode: 'drawing' }, userLayers: [], document: doc, _layerSel: [], NemoNativeObjectProjectFile: Files, NemoNativeObjectUI: UI,
    NemoNativeLayerSolo: Solo, showToast: (m) => toasts.push(m), SM: { toggleLayerSolo: () => { legacy += 1; } },
    NemoNativeObjectProject: { desktop: () => (native ? session : null) } };
  const render = () => {
    Files.projectView(root, info);
    const ports = { projectionReady: (i) => Files.projectionReadyForUI(root, i), currentSession: () => null, selectLayer: () => true, repaint: () => true, readShyMode: () => false };
    assert.equal(UI.render(root, info, false, ports), true);
    return Array.from(nodes.get('layer-list').children);
  };
  return { root, render, presented, toasts, dispatched, legacy: () => legacy, reopen: (g) => { info = metadata(g); } };
}
const cellOf = (row) => Array.from(row.children).find((c) => c.dataset && c.dataset.nativeLayerSolo === 'true');

test('every row carries the baseline S cell after eye and lock, the folder row too, and the four controls stay first', () => {
  const h = harness(), rows = h.render();
  assert.equal(rows.length, 3);
  rows.forEach((row) => {
    const cell = cellOf(row);
    assert.ok(cell, 'an S cell on ' + row.dataset.layerUid);
    assert.equal(cell.className, 'lico solo-btn off');
    assert.equal(cell.textContent, 'S');
    assert.equal(cell.title, 'Solo layer (hide all others)');
    assert.equal(cell.style.order, '2', 'after the eye (order 2, earlier in the row) and the lock');
    assert.ok(Array.from(row.children).indexOf(cell) >= 4, 'the pick, eye and two policy buttons keep the first four positions');
  });
  assert.equal(rows[1].dataset.layerUid, 'F', 'the folder layer is a row like any other');
});

test('a click lights the cell, adds the layer to the sorted set, asks for one more preview and dispatches nothing', async () => {
  const h = harness(), rows = h.render();
  h.root.state.currentFrame = 5;
  const f = cellOf(rows[1]), a = cellOf(rows[0]);
  f.click();
  assert.equal(f.stopped, true, 'the click does not reach the row (baseline stopPropagation)');
  assert.equal(f.className, 'lico solo-btn on');
  assert.deepEqual(Solo.read(h.root, h.root.NemoNativeObjectProject.desktop().info()), ['F']);
  a.click();
  assert.deepEqual(h.presented.map((p) => p[2]), [['F'], ['A', 'F']], 'the set is sorted and each toggle presents once at the playhead');
  assert.deepEqual(h.presented.map((p) => [p[0], p[1]]), [[5, true], [5, true]]);
  await settle();
  assert.deepEqual(h.dispatched, [], 'no engine command, so no revision, history entry or dirty mark');
  assert.equal(h.legacy(), 0, 'the legacy toggleLayerSolo is never called');
  assert.deepEqual(h.toasts, []);
  // A rebuilt row reads the state again; a second click clears it.
  const again = cellOf(h.render()[1]);
  assert.equal(again.className, 'lico solo-btn on');
  again.click();
  assert.equal(cellOf(h.render()[1]).className, 'lico solo-btn off');
  assert.deepEqual(h.presented.at(-1)[2], ['A']);
});

test('the set belongs to the project lifecycle: another lifecycle, instance or document starts empty (a closed project leaks nothing)', () => {
  const h = harness(), rows = h.render();
  cellOf(rows[0]).click();
  const info = h.root.NemoNativeObjectProject.desktop().info();
  assert.deepEqual(Solo.read(h.root, info), ['A']);
  h.reopen(2);
  const next = h.root.NemoNativeObjectProject.desktop().info();
  assert.deepEqual(Solo.read(h.root, next), [], 'a new lifecycle generation');
  assert.equal(cellOf(h.render()[0]).className, 'lico solo-btn off');
  assert.deepEqual(Solo.read(h.root, { ...info, documentId: 'other' }), []);
  assert.deepEqual(Solo.read(h.root, null), []);
  cellOf(h.render()[0]).click();
  Solo.clear(h.root);
  assert.deepEqual(Solo.read(h.root, next), [], 'clear forgets everything');
});

test('a refused Solo view puts the set back, unlights the cell, says so once and presents the previous view again', async () => {
  const h = harness({ refuse: true }), rows = h.render();
  const cell = cellOf(rows[0]);
  cell.click();
  await settle();
  assert.deepEqual(Solo.read(h.root, h.root.NemoNativeObjectProject.desktop().info()), []);
  assert.equal(cell.className, 'lico solo-btn off');
  assert.deepEqual(h.toasts, ['Solo could not be shown.']);
  assert.equal(h.presented.length, 2, 'the refused view and the restored one');
  assert.deepEqual(h.presented[1][2], [], 'the restored request carries no set');
});

test('with no native project the click is not ours: no state, no preview', async () => {
  const h = harness({ native: false });
  assert.equal(await Solo.toggle(h.root, 'A'), false);
  assert.deepEqual(h.presented, []);
  assert.deepEqual(Solo.read(h.root, metadata(1)), []);
});

test('the preview request carries soloLayers only when something is soloed, and the receipt must echo exactly that set', () => {
  const soloed = ['B', 'A', 'B'];
  const view = Exposure.navigation(() => 'canvas', null, null, null, null, null, null, () => soloed);
  const info = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1 };
  const selected = view.read(info);
  assert.deepEqual(selected.solo, ['A', 'B'], 'sorted and unique');
  const request = {};
  Exposure.requestView(request, selected);
  assert.deepEqual(request.soloLayers, ['A', 'B']);
  const none = {};
  Exposure.requestView(none, Exposure.navigation(() => 'canvas').read(info));
  assert.equal(Object.hasOwn(none, 'soloLayers'), false);
  const receipt = { status: 'presented', workId: 'w1', viewGeneration: 1, view: undefined, displayFilter: 'none', soloLayers: ['A', 'B'] };
  const inspect = (r, wanted) => Exposure.inspectReceipt(r, true, false, 'canvas', undefined, 'none', undefined, false, false, false, wanted);
  assert.equal(inspect(receipt, ['A', 'B']).failure, null);
  assert.deepEqual(inspect(receipt, ['A', 'B']).soloLayers, ['A', 'B']);
  assert.equal(inspect({ ...receipt, soloLayers: ['A'] }, ['A', 'B']).failure, 'solo-layers', 'another set than asked');
  const dropped = { ...receipt }; delete dropped.soloLayers;
  assert.equal(inspect(dropped, ['A', 'B']).failure, 'solo-layers', 'a renderer that did not do what was asked');
  assert.equal(inspect(receipt, []).failure, 'solo-layers', 'an echo nobody asked for');
  assert.equal(inspect(dropped, []).failure, null);
  assert.equal(inspect(receipt, ['A', 'bad id']).failure, 'solo-layers');
  // The set is part of the view: another set is another presentation.
  assert.equal(Exposure.sameView({ mode: 'canvas', navigation: { zoom: 1, pan: [0, 0] }, displayFilter: 'none', solo: ['A'] },
    { mode: 'canvas', navigation: { zoom: 1, pan: [0, 0] }, displayFilter: 'none', solo: ['A', 'B'] }), false);
  assert.equal(Exposure.sameView({ mode: 'canvas', navigation: { zoom: 1, pan: [0, 0] }, displayFilter: 'none', solo: ['B', 'A'] },
    { mode: 'canvas', navigation: { zoom: 1, pan: [0, 0] }, displayFilter: 'none', solo: ['A', 'B'] }), true);
  const shown = Exposure.presentedFrame({ workId: 'w1', viewGeneration: 1, view: {}, displayFilter: 'none', soloLayers: ['A', 'B'] }, { contextId: 'scene-root', instanceId: 'i', documentId: 'd', contentRevision: 1, documentSnapshotId: 's', lifecycleGeneration: 1 }, 5);
  assert.deepEqual(shown.soloLayers, ['A', 'B']);
  const plain = Exposure.presentedFrame({ workId: 'w1', viewGeneration: 1, view: {}, displayFilter: 'none' }, { contextId: 'scene-root', instanceId: 'i', documentId: 'd', contentRevision: 1, documentSnapshotId: 's', lifecycleGeneration: 1 }, 5);
  assert.equal(Object.hasOwn(plain, 'soloLayers'), false);
});

test('the export request carries the same set (the baseline export honours layer solo) and is plain without it', async () => {
  const seen = [];
  const make = (solo, ext) => ({ enqueue: (action) => action({ totalFrames: 10 }), frameRequest: (frame) => ({ requestId: 'r', instanceId: 'i', documentId: 'd', contentRevision: 1, documentSnapshotId: 's', frame }),
    soloLayers: solo, invoke: (request) => { seen.push(request); return Promise.resolve({ apiVersion: 2, requestId: 'r', instanceId: 'i', documentId: 'd', contentRevision: 1, documentSnapshotId: 's',
      published: true, cleanupComplete: true, status: 'succeeded', cleanupStatus: 'complete', externalEffectDisposition: 'committed', errorCode: null, artifactFiles: ['frame_0001.' + (ext || 'png')] }); } });
  await Exports.run(make(() => ['L1', 'L2']), '/tmp/out', [0], null, null, 'png');
  assert.deepEqual(seen[0].soloLayers, ['L1', 'L2']);
  await Exports.run(make(() => []), '/tmp/out', [0], null, null, 'png');
  assert.equal(Object.hasOwn(seen[1], 'soloLayers'), false);
  await Exports.run(make(undefined, 'svg'), '/tmp/out', [0], null, null, 'svg');
  assert.equal(Object.hasOwn(seen[2], 'soloLayers'), false);
});

test('the session wiring reads the set from the solo module, never from a legacy layer flag', () => {
  const dir = path.join(__dirname, '..', 'src', 'js', 'adapters'), read = (f) => fs.readFileSync(path.join(dir, f), 'utf8');
  assert.match(read('native-object-project-desktop.js'), /soloLayers:function\(info\)\{var solo=root\.NemoNativeLayerSolo;return solo&&typeof solo\.read==='function'\?solo\.read\(root,info\):\[\];\}/);
  assert.match(read('native-object-project.js'), /ports\.hideShadowGuides,ports\.soloLayers\)/);
  assert.match(read('native-object-project.js'), /selected\.hideShadowGuides,selected\.solo\)/);
  assert.match(read('native-object-project.js'), /frameRequest:frameRequest,soloLayers:ports\.soloLayers,/);
  assert.match(read('native-object-ui.js'), /soloCell:root\.NemoNativeLayerSolo\?/);
  assert.doesNotMatch(read('native-layer-solo.js'), /toggleLayerSolo\s*\(|\.solo\s*=/);
  assert.match(fs.readFileSync(path.join(__dirname, '..', 'src', 'index.html'), 'utf8'), /<script src="js\/adapters\/native-layer-solo\.js"><\/script>/);
});
