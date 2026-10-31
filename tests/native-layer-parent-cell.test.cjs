'use strict';
// #2372 U12 follow-up: the layer-row Parent cell on a NATIVE object project. The rows are the real native-object-ui + layer-list
// presenter output; the cell, the pickwhip and the menu are the real native-layer-parent.js. A pick is exactly one
// command.document.object.layer.parent.set; the label is read from the native row (parentLayerUid), never from a legacy layer.
const test = require('node:test');
const assert = require('node:assert/strict');
const UI = require('../src/js/adapters/native-object-ui.js');
const Files = require('../src/js/adapters/native-object-project-file.js');
const Parent = require('../src/js/adapters/native-layer-parent.js');

const OP = 'command.document.object.layer.parent.set';
const plain = (value) => JSON.parse(JSON.stringify(value));
const settle = async () => { for (let i = 0; i < 14; i += 1) await new Promise((resolve) => setImmediate(resolve)); };
function collection(kids) {
  const out = Object.create({ item(i) { return kids[i] || null; } });
  kids.forEach((k, i) => { out[i] = k; });
  Object.defineProperty(out, 'length', { value: kids.length });
  return Object.freeze(out);
}
function element(tagName) {
  const classes = new Set(), listeners = {}, kids = [];
  const el = { tagName, style: {}, dataset: {}, attributes: {}, textContent: '', type: '', title: '', value: '', hidden: false, disabled: false, removed: false,
    classList: { add: (x) => classes.add(x), remove: (x) => classes.delete(x), contains: (x) => classes.has(x), toggle(x, on) { if (on === undefined) on = !classes.has(x); if (on) classes.add(x); else classes.delete(x); return on; } },
    setAttribute(k, v) { this.attributes[k] = String(v); }, addEventListener(k, f) { listeners[k] = f; }, removeEventListener() {},
    replaceChildren(...next) { kids.splice(0).forEach((k) => { k.parentNode = null; }); next.forEach((k) => this.appendChild(k)); },
    appendChild(k) { if (k.parentNode) k.parentNode.removeChild(k); kids.push(k); k.parentNode = this; return k; },
    removeChild(k) { kids.splice(kids.indexOf(k), 1); k.parentNode = null; },
    remove() { this.removed = true; }, getBoundingClientRect: () => ({ left: 10, top: 20, width: 9, height: 9, bottom: 29 }),
    querySelector: () => null, contains(k) { while (k) { if (k === this) return true; k = k.parentNode; } return false; },
    closest(sel) { for (let n = this; n; n = n.parentNode) if (sel === '.' + 'native-object-layer' && n.className.split(' ').includes('native-object-layer')) return n; return null; },
    fire(type, extra) { return listeners[type] && listeners[type](Object.assign({ target: this, preventDefault() {}, stopPropagation() {} }, extra)); } };
  Object.defineProperty(el, 'className', { get: () => [...classes].join(' '), set(v) { classes.clear(); String(v).split(/\s+/).filter(Boolean).forEach((x) => classes.add(x)); } });
  Object.defineProperty(el, 'children', { get: () => collection(kids) });
  Object.defineProperty(el, 'innerHTML', { set() { this.replaceChildren(); } });
  return el;
}
function layers(parentOfC) {
  return [{ layerUid: 'L-a', name: 'Alpha', visible: true, kind: 'draw' }, { layerUid: 'L-b', name: 'Beta', visible: true, kind: 'draw', parentLayerUid: 'L-a' },
    Object.assign({ layerUid: 'L-c', name: 'Gamma', visible: true, kind: 'draw' }, parentOfC ? { parentLayerUid: parentOfC } : {}), { layerUid: 'L-f', name: 'Folder', visible: true, kind: 'folder' }];
}
function info(revision, parentOfC) {
  return { apiVersion: 2, instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 1, contentRevision: revision, documentSnapshotId: 'snap-' + revision,
    documentDigest: '4'.repeat(64), byteCount: 1000, canvasW: 320, canvasH: 180, fps: 24, totalFrames: 24, reference: null, undoDepth: 0, redoDepth: 0, layers: layers(parentOfC) };
}
function harness(options = {}) {
  const sent = [], toasts = [], menus = [], listeners = {};
  const nodes = new Map(['layer-list', 'frame-hdr', 'frame-grid', 'tl-cf', 'tl-tf', 'info-frame', 'info-badge', 'info-strokes', 'info-sel'].map((id) => [id, Object.assign(element('div'), { id })]));
  const body = element('body');
  let pinned = info(4, undefined), under = null;
  const session = { info: () => pinned, isActive: () => true, blocksLegacy: () => true, requestId: (label) => label + '-' + (sent.length + 1),
    runResourceCommand: (action) => action({
      async dispatch(request) {
        sent.push(request);
        const base = { apiVersion: 2, requestId: request.requestId, instanceId: 'instance-A', documentId: 'document-A' };
        if (options.refuse) return Object.assign(base, { contentRevision: pinned.contentRevision, ok: false, error: { code: options.refuse, message: 'refused' } });
        return Object.assign(base, { contentRevision: pinned.contentRevision + 1, ok: true, result: { applied: true, historyEntriesAdded: 1 } });
      }, async reconcile() {} }) };
  const document = { nodes, activeElement: null, body, getElementById: (id) => nodes.get(id) || null, createElement: element, createComment: () => element('#comment'),
    addEventListener: (kind, fn) => { listeners[kind] = fn; }, removeEventListener: (kind) => { delete listeners[kind]; }, elementFromPoint: () => under };
  const root = { state: { currentFrame: 9, activeLayerIdx: 0, layers: [] }, userLayers: [], document, _layerSel: [], NemoNativeObjectProjectFile: Files, NemoNativeObjectUI: UI,
    NemoNativeObjectProject: { desktop: () => session }, NemoNativeLayerParent: Parent, SM: { t: (k) => k }, showToast: (m) => toasts.push(m),
    showContextMenu: (x, y, items) => menus.push({ x, y, items }), nativeCompositingRefresh() {} };
  const ports = { projectionReady: (i) => Files.projectionReadyForUI(root, i), currentSession: () => null, selectLayer: () => true, repaint: () => true, readShyMode: () => false };
  const paint = () => { Files.projectView(root, pinned); assert.equal(UI.render(root, pinned, false, ports), true); return Array.from(nodes.get('layer-list').children); };
  root.renderLayerList = () => { pinned = info(pinned.contentRevision, pinned.layers[2].parentLayerUid); paint(); };
  return { root, sent, toasts, menus, listeners, paint, session, setUnder: (v) => { under = v; }, confirm(revision, parentOfC) { pinned = info(revision, parentOfC); } };
}
const cellOf = (row) => Array.from(row.children).find((n) => n.dataset && n.dataset.nativeParentCell === 'true');

test('every native row carries the baseline Parent cell: native parent name or the dash, pickwhip first, baseline classes', () => {
  const h = harness(), rows = h.paint();
  h.root.state.currentFrame = 9;
  const cells = rows.map(cellOf);
  assert.ok(cells.every(Boolean), 'one cell per row');
  assert.deepEqual(cells.map((c) => c.children[1].textContent), ['—', 'Alpha', '—', '—']);
  assert.equal(cells[0].className, 'lparent none');
  assert.equal(cells[1].className, 'lparent');
  assert.equal(cells[0].children[0].className, 'lpick');
  assert.equal(cells[1].title, 'Parent : Alpha — cliquer pour changer');
  assert.equal(cells[0].title, 'Aucun parent — cliquer pour en choisir un');
  assert.equal(cells[0].style.order, '1');
});

test('the label follows the confirmed native revision, not a legacy layer row', () => {
  const h = harness();
  h.root.state.layers = [{ layerUid: 'L-c', parentLayerUid: 'WRONG', name: 'legacy' }];
  h.paint();
  h.confirm(5, 'L-b');
  const rows = h.paint();
  assert.equal(cellOf(rows[2]).children[1].textContent, 'Beta');
  assert.equal(cellOf(rows[2]).className, 'lparent');
});

test('without the native parent module the rows have no cell (legacy rows are unchanged)', () => {
  const h = harness();
  delete h.root.NemoNativeLayerParent;
  assert.ok(h.paint().every((row) => !cellOf(row)));
});

test('the click menu lists Parent A at native names, folders and descendants disabled, Parent B disabled, and a pick is one command', async () => {
  const h = harness(), rows = h.paint();
  h.root.state.currentFrame = 9;
  cellOf(rows[0]).fire('click');
  assert.equal(h.menus.length, 1);
  assert.deepEqual([h.menus[0].x, h.menus[0].y], [10, 31]);
  const items = h.menus[0].items;
  const entry = (name) => items.find((i) => i.label && i.label.startsWith('Parent A : ' + name));
  assert.equal(entry('Beta').disabled, true, 'Beta descends from Alpha');
  assert.equal(entry('Folder').disabled, true);
  assert.equal(entry('Gamma').disabled, false);
  assert.ok(items.some((i) => /Parent B/.test(i.label || '') && i.disabled === true));
  entry('Gamma').action();
  await settle();
  assert.equal(h.sent.length, 1);
  assert.equal(h.sent[0].operation, OP);
  assert.deepEqual(plain(h.sent[0].payload), { command: 'object.layer.parent.set', contextId: 'scene-root', layerUid: 'L-a', parentLayerUid: 'L-c', frame: 9 });
});

test('a pickwhip drop on another native row is exactly one command; drops on nothing, itself, a descendant or a folder send none', async () => {
  const h = harness(), rows = h.paint(), cell = cellOf(rows[0]);
  h.root.state.currentFrame = 9;
  const start = (li) => cellOf(rows[li]).children[0].fire('mousedown', { clientX: 1, clientY: 1 });
  start(0);
  assert.equal(typeof h.listeners.mouseup, 'function', 'the drag is armed');
  h.setUnder(null); h.listeners.mouseup({ clientX: 5, clientY: 5 });
  assert.equal(h.sent.length, 0);
  assert.equal(h.listeners.mouseup, undefined, 'the drag disarmed');
  for (const target of [0, 1, 3]) { start(0); h.setUnder(rows[target]); h.listeners.mouseup({ clientX: 5, clientY: 5 }); await settle(); assert.equal(h.sent.length, 0, 'row ' + target); }
  start(0);
  h.setUnder(rows[2].children[0]); // a descendant element inside the Gamma row
  h.listeners.mouseup({ clientX: 30, clientY: 30 });
  await settle();
  assert.equal(h.sent.length, 1);
  assert.deepEqual(plain(h.sent[0].payload), { command: 'object.layer.parent.set', contextId: 'scene-root', layerUid: 'L-a', parentLayerUid: 'L-c', frame: 9 });
  assert.ok(cell);
});

test('right-click clears a native parent in one command and does nothing on a parentless row', async () => {
  const h = harness(), rows = h.paint();
  h.root.state.currentFrame = 9;
  cellOf(rows[0]).fire('contextmenu');
  await settle();
  assert.equal(h.sent.length, 0, 'nothing to clear');
  cellOf(rows[1]).fire('contextmenu');
  await settle();
  assert.equal(h.sent.length, 1);
  assert.deepEqual(plain(h.sent[0].payload), { command: 'object.layer.parent.set', contextId: 'scene-root', layerUid: 'L-b', parentLayerUid: null, frame: 9 });
});

test('a refused pick says why and sends no second command', async () => {
  const h = harness({ refuse: 'unavailable' }), rows = h.paint();
  cellOf(rows[0]).fire('click');
  h.menus[0].items.find((i) => /Gamma/.test(i.label || '')).action();
  await settle();
  assert.equal(h.sent.length, 1);
  assert.match(h.toasts[0], /rotated or scaled/);
});
