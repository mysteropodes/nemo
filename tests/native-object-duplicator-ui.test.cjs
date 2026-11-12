'use strict';
// #2256 Q3: the Duplicator column cell and the "Object-mode Duplicator..." entry on a NATIVE object project. The rows are the real
// native-object-ui + layer-list presenter output; the cell and entry are the real native-object-duplicator-ui.js over the real
// native-object-duplicator.js intents. State is read from the native row (hasDuplicator); a click is exactly one
// command.document.object.duplicator.set, the entry exactly one duplicator.objects; the legacy writers are never called.
const test = require('node:test');
const assert = require('node:assert/strict');
const UI = require('../src/js/adapters/native-object-ui.js');
const Files = require('../src/js/adapters/native-object-project-file.js');
const Parent = require('../src/js/adapters/native-layer-parent.js');

const Duplicator = require('../src/js/adapters/native-object-duplicator.js');
const DupUi = require('../src/js/adapters/native-object-duplicator-ui.js');

const SET = 'command.document.object.duplicator.set', OBJECTS = 'command.document.object.duplicator.objects';
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
function layers(parentOfC, extra) {
  const rows = [{ layerUid: 'L-a', name: 'Alpha', visible: true, kind: 'draw' }, { layerUid: 'L-b', name: 'Beta', visible: true, kind: 'draw', parentLayerUid: 'L-a' },
    Object.assign({ layerUid: 'L-c', name: 'Gamma', visible: true, kind: 'draw' }, parentOfC ? { parentLayerUid: parentOfC } : {}), { layerUid: 'L-f', name: 'Folder', visible: true, kind: 'folder' }];
  return rows.map((row) => Object.assign(row, extra && extra[row.layerUid]));
}
function info(revision, parentOfC, extra) {
  return { apiVersion: 2, instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 1, contentRevision: revision, documentSnapshotId: 'snap-' + revision,
    documentDigest: '4'.repeat(64), byteCount: 1000, canvasW: 320, canvasH: 180, fps: 24, totalFrames: 24, reference: null, undoDepth: 0, redoDepth: 0, layers: layers(parentOfC, extra) };
}
function harness(options = {}) {
  const sent = [], toasts = [], menus = [], listeners = {};
  let repaints = 0;
  const nodes = new Map(['layer-list', 'frame-hdr', 'frame-grid', 'tl-cf', 'tl-tf', 'info-frame', 'info-badge', 'info-strokes', 'info-sel'].map((id) => [id, Object.assign(element('div'), { id })]));
  const body = element('body');
  let pinned = info(4, undefined, options.rows), under = null;
  const session = { info: () => pinned, isActive: () => true, blocksLegacy: () => options.blocks !== false, requestId: (label) => label + '-' + (sent.length + 1),
    runResourceCommand: (action) => action({
      async dispatch(request) {
        sent.push(request);
        const base = { apiVersion: 2, requestId: request.requestId, instanceId: 'instance-A', documentId: 'document-A' };
        if (options.refuse) return Object.assign(base, { contentRevision: pinned.contentRevision, ok: false, error: { code: options.refuse, message: options.refuseMessage || 'refused' } });
        return Object.assign(base, { contentRevision: pinned.contentRevision + 1, ok: true, result: request.operation === OBJECTS ? { applied: true, historyEntriesAdded: 1, layerUid: 'native-layer:5:0' } : { applied: true, historyEntriesAdded: 1 } });
      }, async reconcile() {} }) };
  const document = { nodes, activeElement: null, body, getElementById: (id) => nodes.get(id) || null, createElement: element, createComment: () => element('#comment'),
    addEventListener: (kind, fn) => { listeners[kind] = fn; }, removeEventListener: (kind) => { delete listeners[kind]; }, elementFromPoint: () => under };
  const root = { state: { currentFrame: 9, activeLayerIdx: 0, layers: [] }, userLayers: [], document, _layerSel: [], NemoNativeObjectProjectFile: Files, NemoNativeObjectUI: UI,
    NemoNativeObjectProject: { desktop: () => session }, NemoNativeLayerParent: Parent, NemoNativeObjectDuplicator: Duplicator, NemoNativeObjectDuplicatorUi: DupUi,
    SM: { t: (k) => ({ dupColTitle: 'DUP-TITLE', ctxObjectDuplicatorEllipsis: 'OBJ-DUP', toastObjectDuplicatorCreated: 'created {n}' })[k] || k }, showToast: (m) => toasts.push(m),
    showContextMenu: (x, y, items) => menus.push({ x, y, items }), nativeCompositingRefresh() {} };
  const ports = { projectionReady: (i) => Files.projectionReadyForUI(root, i), currentSession: () => null, selectLayer: () => true, repaint: () => true, readShyMode: () => false };
  const paint = () => { Files.projectView(root, pinned); assert.equal(UI.render(root, pinned, false, ports), true); return Array.from(nodes.get('layer-list').children); };
  root.renderLayerList = () => { repaints += 1; pinned = info(pinned.contentRevision, pinned.layers[2].parentLayerUid, options.rows); paint(); };
  return { root, sent, toasts, menus, listeners, paint, repaints: () => repaints, session, setUnder: (v) => { under = v; }, confirm(revision, parentOfC) { pinned = info(revision, parentOfC); } };
}
const cellOf = (row) => Array.from(row.children).find((n) => n.dataset && n.dataset.nativeDuplicatorCell === 'true');
const ROWS = { 'L-a': { hasDuplicator: true, dupSourceMode: 'shape' }, 'L-c': { hasDuplicator: true, dupSourceMode: 'objects' } };
const spies = (root) => {
  const calls = [];
  root.toggleLayerDuplicator = () => calls.push('toggleLayerDuplicator');
  root.convertSelectionToObjectDuplicator = () => calls.push('convertSelectionToObjectDuplicator');
  root.SMMotion = { toggleLayerDuplicator: () => calls.push('SMMotion.toggleLayerDuplicator') };
  root.SM.convertSelectionToObjectDuplicator = () => calls.push('SM.convertSelectionToObjectDuplicator');
  return calls;
};
const menuOf = (h, rows, li) => { h.menus.length = 0; rows[li].fire('contextmenu', { clientX: 4, clientY: 6 }); return h.menus[0].items; };

test('every native row carries the baseline Duplicator cell: motion-col-duplicator, off unless the native row says hasDuplicator', () => {
  const h = harness({ rows: ROWS }), rows = h.paint();
  const cells = rows.map(cellOf);
  assert.ok(cells.every(Boolean), 'one cell per row');
  assert.deepEqual(cells.map((c) => c.className), ['lico motion-col-duplicator', 'lico motion-col-duplicator off', 'lico motion-col-duplicator', 'lico motion-col-duplicator off']);
  assert.equal(cells[0].title, 'DUP-TITLE');
  assert.equal(cells[0].style.order, '1');
  assert.equal(cells[2].dataset.dupSourceMode, 'objects');
  assert.equal(cells[0].dataset.dupSourceMode, undefined);
  assert.equal(rows[0].children[0].type, 'button', 'the four controls stay first');
});

test('the cell state follows the confirmed native read, not a legacy layer flag', () => {
  const h = harness();
  h.root.state.layers = [{ layerUid: 'L-b', duplicator: { mode: 'grid' } }];
  assert.ok(h.paint().every((row) => cellOf(row).className.endsWith(' off')));
  h.root.state.layers = [];
  const next = harness({ rows: { 'L-b': { hasDuplicator: true, dupSourceMode: 'shape' } } });
  assert.deepEqual(next.paint().map((row) => cellOf(row).className.endsWith(' off')), [true, false, true, true]);
});

test('without the duplicator ui module the rows have no cell (legacy rows are unchanged)', () => {
  const h = harness();
  delete h.root.NemoNativeObjectDuplicatorUi;
  assert.ok(h.paint().every((row) => !cellOf(row)));
});

test('clicking an off cell is exactly one duplicator.set enable with a fresh seed 0..999999', async () => {
  const h = harness({ rows: ROWS }), rows = h.paint(), calls = spies(h.root);
  cellOf(rows[1]).fire('click');
  await settle();
  assert.equal(h.sent.length, 1);
  assert.equal(h.sent[0].operation, SET);
  const payload = plain(h.sent[0].payload);
  assert.deepEqual(Object.keys(payload).sort(), ['command', 'enabled', 'layerUid', 'seed']);
  assert.deepEqual([payload.command, payload.layerUid, payload.enabled], ['object.duplicator.set', 'L-b', true]);
  assert.ok(Number.isSafeInteger(payload.seed) && payload.seed >= 0 && payload.seed <= 999999);
  assert.equal(h.repaints(), 1, 'repainted from the confirmed revision');
  assert.deepEqual(h.toasts, []);
  assert.deepEqual(calls, [], 'no legacy writer');
});

test('clicking an on cell is exactly one duplicator.set disable, with no seed', async () => {
  const h = harness({ rows: ROWS }), rows = h.paint(), calls = spies(h.root);
  cellOf(rows[0]).fire('click');
  await settle();
  assert.equal(h.sent.length, 1);
  assert.deepEqual(plain(h.sent[0].payload), { command: 'object.duplicator.set', layerUid: 'L-a', enabled: false });
  assert.deepEqual(calls, []);
});

test('a second click while the first is in flight sends nothing more', async () => {
  const h = harness({ rows: ROWS }), rows = h.paint();
  cellOf(rows[1]).fire('click');
  cellOf(rows[1]).fire('click');
  await settle();
  assert.equal(h.sent.length, 1);
});

test('the entry is disabled for a single selection and enabled for a multi-selection that includes the row', () => {
  const h = harness(), rows = h.paint();
  h.root._layerSel = [1];
  assert.equal(menuOf(h, rows, 1).find((i) => i.label === 'OBJ-DUP').disabled, true);
  h.root._layerSel = [0, 2];
  assert.equal(menuOf(h, rows, 1).find((i) => i.label === 'OBJ-DUP').disabled, true, 'the row is outside the selection');
  assert.equal(menuOf(h, rows, 2).find((i) => i.label === 'OBJ-DUP').disabled, false);
});

test('the entry sends exactly one duplicator.objects naming the selection in document order, de-duplicated', async () => {
  const h = harness(), rows = h.paint(), calls = spies(h.root);
  h.root._layerSel = [2, 0, 2];
  menuOf(h, rows, 2).find((i) => i.label === 'OBJ-DUP').action();
  await settle();
  assert.equal(h.sent.length, 1);
  assert.equal(h.sent[0].operation, OBJECTS);
  const payload = plain(h.sent[0].payload);
  assert.deepEqual([payload.command, payload.layerUids], ['object.duplicator.objects', ['L-a', 'L-c']]);
  assert.ok(Number.isSafeInteger(payload.seed) && payload.seed >= 0 && payload.seed <= 999999);
  assert.deepEqual(h.toasts, ['created 2']);
  assert.equal(h.repaints(), 1);
  assert.deepEqual(calls, []);
});

test('a refused toggle or conversion toasts the owner reason, writes nothing further and does not repaint', async () => {
  for (const code of ['invalid_request', 'unavailable']) {
    const h = harness({ rows: ROWS, refuse: code, refuseMessage: 'A locked layer cannot host a duplicator.' }), rows = h.paint(), calls = spies(h.root);
    cellOf(rows[1]).fire('click');
    await settle();
    h.root._layerSel = [0, 1];
    menuOf(h, rows, 0).find((i) => i.label === 'OBJ-DUP').action();
    await settle();
    assert.equal(h.sent.length, 2, 'one request each, none retried');
    assert.deepEqual(h.toasts, ['Duplicator refused: A locked layer cannot host a duplicator.', 'Duplicator refused: A locked layer cannot host a duplicator.']);
    assert.equal(h.repaints(), 0);
    assert.deepEqual(calls, []);
  }
});

test('a stale layer, a lone-layer conversion and a missing native project are typed toasts with no request', async () => {
  const h = harness(), rows = h.paint();
  await DupUi.toggle(h.root, 'L-gone');
  h.root._layerSel = [1];
  await DupUi.convertSelection(h.root, [1]);
  assert.equal(h.sent.length, 0);
  assert.deepEqual(h.toasts, ['The layer is no longer in the project.', 'Duplicator refused: Select at least 2 layers']);
  const off = harness({ blocks: false }), offRows = off.paint();
  cellOf(offRows[0]).fire('click');
  await settle();
  assert.equal(off.sent.length, 0);
  assert.deepEqual(off.toasts, ['Native duplicator is unavailable.']);
  assert.ok(rows.length === 4);
});

test('the project read model (metadata contract) admits hasDuplicator/dupSourceMode only in their closed shapes and keeps them in the layer table', () => {
  const Metadata = require('../src/js/adapters/native-object-project-metadata.js');
  const request = { requestId: 'model', instanceId: 'instance', documentId: 'document' };
  const base = (layers) => ({ apiVersion: 2, requestId: 'model', instanceId: 'instance', documentId: 'document', lifecycleGeneration: 1, contentRevision: 0,
    documentSnapshotId: 'snapshot-0', documentDigest: 'a'.repeat(64), byteCount: 1000, canvasW: 320, canvasH: 180, fps: 24, totalFrames: 24, reference: null,
    undoDepth: 0, redoDepth: 0, layers });
  const read = Metadata.decode(base([{ layerUid: 'a', hasDuplicator: true, dupSourceMode: 'objects' }, { layerUid: 'b', hasDuplicator: true }, { layerUid: 'c' }]), request);
  assert.deepEqual(plain(read.layers), [{ layerUid: 'a', hasDuplicator: true, dupSourceMode: 'objects' }, { layerUid: 'b', hasDuplicator: true }, { layerUid: 'c' }]);
  for (const extra of [{ hasDuplicator: false }, { hasDuplicator: 1 }, { hasDuplicator: true, dupSourceMode: 'grid' }, { dupSourceMode: 'shape' }]) {
    assert.throws(() => Metadata.decode(base([{ layerUid: 'a', ...extra }]), request), /uncorrelated/, JSON.stringify(extra));
  }
  const root = { state: {}, userLayers: [], selectedPaths: [] };
  Files.projectView(root, read);
  assert.equal(Files.projectionMatches(root, read), true, 'the layer table admits the new members');
});
