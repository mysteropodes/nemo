'use strict';
// #2256 A-comp-edit-timeline part 3 (JS): the in/out bar of an entered component is editable. A drag, a Ctrl/Shift selection and the bar menu reach
// the declared native operations (layer.range.drag through the frame-edit route, layer.range.arrange through the component route) stamped with the
// held context; a bare click in Animation 2D writes nothing.
const test = require('node:test'), assert = require('node:assert/strict');
const RigCommand = require('../src/js/adapters/native-object-rig-command.js');
const RigContract = require('../src/js/adapters/native-object-rig-contract.js');
const Menu = require('../src/js/adapters/native-layer-range-menu.js');
const RangeEdit = require('../src/js/adapters/native-component-range-edit.js');
const P = 'command.document.object.';

class El {
  constructor(tag) { this.tag = tag; this.children = []; this.listeners = {}; this.style = {}; this.dataset = {}; this.attrs = {}; this.parent = null; this.cls = new Set(); this.className = ''; this.classList = { add: (c) => this.cls.add(c), toggle: (c, on) => (on ? this.cls.add(c) : this.cls.delete(c)), contains: (c) => this.cls.has(c) }; }
  appendChild(c) { c.parent = this; this.children.push(c); return c; }
  addEventListener(t, f) { (this.listeners[t] = this.listeners[t] || []).push(f); }
  removeEventListener(t, f) { this.listeners[t] = (this.listeners[t] || []).filter((x) => x !== f); }
  setAttribute(k, v) { this.attrs[k] = v; }
  fire(t, e) { (this.listeners[t] || []).slice().forEach((f) => f(Object.assign({ stopPropagation() {}, preventDefault() {}, target: this, button: 0 }, e))); }
  querySelector(sel) {
    const want = sel.replace(/^\./, '').split('.');
    const walk = (n) => { for (const c of n.children) { const have = new Set(String(c.className).split(' ')); if (want.every((w) => have.has(w))) return c; const r = walk(c); if (r) return r; } return null; };
    return walk(this);
  }
  querySelectorAll(sel) {
    const out = [], want = sel.split(' ').pop().replace(/\[.*\]/, '').replace(/^\./, '');
    const walk = (n) => n.children.forEach((c) => { if (String(c.className).split(' ').includes(want)) out.push(c); walk(c); });
    walk(this); return out;
  }
}
function harness(options = {}) {
  const sent = [], routes = [], notices = [], menus = [], ran = [];
  const doc = new El('document'); const grid = new El('div'); grid.id = 'frame-grid';
  const byId = { 'frame-grid': grid };
  doc.getElementById = (id) => byId[id] || null; doc.createElement = (t) => new El(t);
  doc.addEventListener = El.prototype.addEventListener; doc.removeEventListener = El.prototype.removeEventListener; doc.listeners = {};
  const rows = [{ layerUid: 'in_a', name: 'A' }, { layerUid: 'in_b', name: 'B' }, { layerUid: 'in_c', name: 'C' }];
  const bars = {};
  rows.forEach((r, i) => {
    const frow = new El('div'); frow.className = 'frow'; frow.dataset.layerUid = r.layerUid; frow.dataset.layer = String(i);
    const bar = new El('div'); bar.className = 'layer-inout-bar'; bar.style.left = '20px'; bar.style.width = '40px'; frow.appendChild(bar); grid.appendChild(frow); bars[r.layerUid] = bar;
  });
  let revision = 5, serial = 0;
  const pin = () => ({ instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: revision, documentSnapshotId: 's' + revision });
  const run = (route) => (action) => { routes.push(route); return Promise.resolve(action({
    dispatch: async (request) => { sent.push(request); revision += 1; return { apiVersion: 2, requestId: request.requestId, instanceId: 'i', documentId: 'd', contentRevision: revision, ok: true, result: { applied: true, historyEntriesAdded: 1 } }; },
    reconcile: async () => pin(),
  })); };
  const session = { info: pin, isActive: () => true, requestId: (label) => label + '-' + (++serial), runResourceCommand: run('scene'), runComponentCommand: run('component') };
  const context = { presented: () => (options.held === null ? null : { contextId: 'sym_a', totalFrames: 8, fps: 24 }), contextId: () => 'sym_a', layers: () => rows, projectionReady: () => true };
  const host = { state: { currentFrame: 3, activeLayerIdx: 1, appMode: options.mode || 'drawing' }, FC: 10, document: doc, NemoNativeComponentContext: context, NemoNativeObjectProject: { desktop: () => session },
    NemoNativeObjectRigCommand: RigCommand, NemoNativeObjectRigContract: RigContract, NemoNativeLayerRangeMenu: Menu,
    NemoNativeComponentFrameEdit: { run: async (action) => { ran.push(action); return { status: 'applied-presented', applied: true }; } },
    showToast: (m) => notices.push(m), showContextMenu: (x, y, items) => menus.push(items) };
  return { host, bars, sent, routes, notices, menus, ran, doc, rows };
}
const flush = () => new Promise((resolve) => setImmediate(resolve));
function bind(h, uid) { RangeEdit.bind(h.host, h.bars[uid], uid, { rawInPoint: 2, rawOutPoint: 5 }); return h.bars[uid]; }
function drag(h, target, part, moves, upX) {
  target.fire('mousedown', { clientX: 100, metaKey: false, ctrlKey: false, shiftKey: false, altKey: false });
  moves.forEach((x) => h.doc.fire('mousemove', { clientX: x }));
  h.doc.fire('mouseup', { clientX: upX === undefined ? moves[moves.length - 1] : upX });
  return flush();
}

test('a painted bar gets edge handles, is no longer disabled, and remembers its raw in point for the menu', () => {
  const h = harness(); const bar = bind(h, 'in_a');
  assert.equal(bar.attrs['aria-disabled'], 'false');
  assert.deepEqual(bar.children.map((c) => c.className), ['layer-inout-handle left', 'layer-inout-handle right']);
  assert.equal(bar.dataset.rawIn, '2');
  bind(h, 'in_a'); assert.equal(bar.children.length, 2, 'binding twice adds nothing');
});

test('a body drag sends ONE rangeDrag with the held context, the ordered samples and the bar selected', async () => {
  const h = harness(); const bar = bind(h, 'in_a');
  await drag(h, bar, 'both', [110, 120, 130]);
  assert.equal(h.ran.length, 1);
  assert.deepEqual(h.ran[0].gesture, { command: 'object.layer.range.drag', contextId: 'sym_a', layerUid: 'in_a', activeLayerUid: 'in_b', part: 'both',
    barSelection: [{ layerUid: 'in_a', part: 'both' }], samples: [1, 2, 3], plainClick: false, alt: false, frame: 3, selectedKeys: [] });
  assert.equal(h.ran[0].kind, 'rangeDrag');
  assert.equal(bar.style.left, '20px', 'the live preview is undone, the repaint shows the owner range');
});

test('the in and out handles drag their own edge', async () => {
  const h = harness(); const bar = bind(h, 'in_c');
  const [left, right] = bar.children;
  await drag(h, left, 'in', [90]);
  await drag(h, right, 'out', [120]);
  assert.deepEqual(h.ran.map((a) => [a.gesture.part, a.gesture.samples]), [['in', [-1]], ['out', [2]]]);
});

test('Ctrl toggles a bar and Shift selects the contiguous run; a drag of a selected bar moves the whole group with each part', async () => {
  const h = harness(); const a = bind(h, 'in_a'), b = bind(h, 'in_b'), c = bind(h, 'in_c');
  a.fire('mousedown', { clientX: 1, ctrlKey: true });
  c.fire('mousedown', { clientX: 1, ctrlKey: true });
  assert.deepEqual(RangeEdit.selection(h.host).map((e) => e.layerUid), ['in_a', 'in_c']);
  assert.equal(h.ran.length, 0, 'a modifier click is no drag');
  a.fire('mousedown', { clientX: 1, ctrlKey: true });
  assert.deepEqual(RangeEdit.selection(h.host).map((e) => e.layerUid), ['in_c'], 'Ctrl on a selected bar unselects it');
  c.fire('mousedown', { clientX: 1, shiftKey: true });
  assert.deepEqual(RangeEdit.selection(h.host).map((e) => [e.layerUid, e.part]), [['in_a', 'both'], ['in_b', 'both'], ['in_c', 'both']], 'Shift selects from the anchor');
  assert.equal(a.cls.has('sel') && b.cls.has('sel'), true);
  await drag(h, b, 'both', [105, 110]);
  assert.deepEqual(h.ran[0].gesture.barSelection.map((e) => e.layerUid), ['in_a', 'in_b', 'in_c']);
  assert.equal(h.ran[0].gesture.layerUid, 'in_b');
});

test('a bare click writes nothing in Animation 2D; in Motion it is a plain click', async () => {
  let h = harness(); let bar = bind(h, 'in_a');
  await drag(h, bar, 'both', [], 100);
  assert.equal(h.ran.length, 0);
  h = harness({ mode: 'motion' }); bar = bind(h, 'in_a');
  await drag(h, bar, 'both', [], 101);
  assert.equal(h.ran.length, 1);
  assert.deepEqual([h.ran[0].gesture.plainClick, h.ran[0].gesture.samples], [true, []]);
});

test('outside a component, with a refused gesture or without the layer: nothing is sent or a notice is shown', async () => {
  let h = harness({ held: null }); let bar = bind(h, 'in_a');
  await drag(h, bar, 'both', [120]);
  assert.equal(h.ran.length, 0);
  h = harness(); bar = bind(h, 'in_a');
  h.host.NemoNativeComponentFrameEdit.run = async () => ({ status: 'unavailable', code: 'frame_edit_stale_intent' });
  await drag(h, bar, 'both', [120]);
  assert.equal(h.notices.length, 1);
});

test('the bar menu is the scene menu: Reset sends one layer.range.arrange with the held context on the component route', async () => {
  const h = harness(); const bar = bind(h, 'in_a');
  bar.fire('contextmenu', { clientX: 5, clientY: 6 });
  const items = h.menus[0];
  assert.deepEqual(items.filter((m) => !m.sep).map((m) => m.label), ['Réinitialiser (pleine durée)', 'Inverser la sélection'], 'no Parent in Time inside a component');
  items[0].action();
  await flush(); await flush();
  assert.deepEqual([h.sent[0].operation, h.sent[0].payload], [P + 'layer.range.arrange', { command: 'object.layer.range.arrange', contextId: 'sym_a', layerUids: ['in_a'], action: 'reset', frame: 3 }]);
  assert.deepEqual([...new Set(h.routes)], ['component']);
});

test('with two bars selected the menu offers the batch edits, each one request over the listed layers', async () => {
  const h = harness(); const a = bind(h, 'in_a'); bind(h, 'in_c');
  h.bars.in_c.dataset.rawIn = '7';
  RangeEdit.setSelection(h.host, [{ layerUid: 'in_a', part: 'both' }, { layerUid: 'in_c', part: 'both' }]);
  a.fire('contextmenu', { clientX: 1, clientY: 1 });
  const labels = h.menus[0].filter((m) => !m.sep).map((m) => m.label);
  assert.ok(labels.includes('Aligner à gauche (in)') && labels.includes('Distribuer uniformément') && labels.includes('Inverser l’ordre (flip)'), labels.join('|'));
  h.menus[0].find((m) => m.label === 'Aligner à gauche (in)').action();
  await flush(); await flush();
  assert.deepEqual(h.sent[0].payload, { command: 'object.layer.range.arrange', contextId: 'sym_a', layerUids: ['in_a', 'in_c'], action: 'alignLeft', frame: 3 });
});

test('a menu built on a component that is no longer entered writes nothing', async () => {
  const h = harness(); const bar = bind(h, 'in_a');
  bar.fire('contextmenu', { clientX: 1, clientY: 1 });
  h.host.NemoNativeComponentContext.contextId = () => 'sym_b';
  h.menus[0][0].action();
  await flush(); await flush();
  assert.equal(h.sent.length, 0);
  assert.equal(h.notices.length, 1);
});
