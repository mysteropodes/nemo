'use strict';
// #2376 G26: the Elements (shape tree) panel on a native object project. The rows are the native hit-test listing of the active layer at the playhead
// (groups from groupPath, items in paint order, front first); Select / Select members drive the SAME select-canvas-intent path a Select-tool click
// does; Ungroup and group Rename are the native group commands, one request each; a plain shape (no native name) reports unavailable; the panel
// follows the project revision and a scrub reads once per settled frame. The legacy renderer (shapes-panel.js) yields and writes nothing.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Shape = require('../src/js/adapters/native-shape-tree-ui.js');
const Model = require('../src/js/adapters/native-shape-tree-model.js');
const Contract = require('../src/js/adapters/native-object-edit-contract.js');
const GroupIntent = require('../src/js/adapters/native-object-group-intent.js');
const Paper = require('./helpers/native-import-paper.cjs');

function element(tag) {
  const listeners = {}, classes = new Set();
  const el = { tagName: String(tag || 'div').toUpperCase(), children: [], style: {}, dataset: {}, attributes: {}, value: '', parentNode: null, own: '', listeners,
    classList: { add: (x) => classes.add(x), remove: (x) => classes.delete(x), contains: (x) => classes.has(x), toggle(x, on) { if (on === undefined) on = !classes.has(x); if (on) classes.add(x); else classes.delete(x); return on; } },
    addEventListener(type, fn) { (listeners[type] = listeners[type] || []).push(fn); },
    appendChild(child) { if (child.parentNode) child.parentNode.removeChild(child); this.children.push(child); child.parentNode = this; return child; },
    removeChild(child) { const i = this.children.indexOf(child); if (i >= 0) this.children.splice(i, 1); child.parentNode = null; return child; },
    replaceChildren(...kids) { this.children.slice().forEach((c) => { c.parentNode = null; }); this.children = []; kids.forEach((k) => (k.isFragment ? k.children.slice().forEach((c) => this.appendChild(c)) : this.appendChild(k))); },
    querySelector(sel) { return this.querySelectorAll(sel)[0] || null; },
    querySelectorAll(sel) { const cls = sel.slice(1), out = []; (function walk(n) { n.children.forEach((c) => { if (String(c.className).split(' ').includes(cls)) out.push(c); walk(c); }); }(this)); return out; },
    setAttribute(k, v) { this.attributes[k] = v; }, focus() {}, select() {},
    fire(type, extra) { (listeners[type] || []).slice().forEach((fn) => fn({ target: this, type, key: undefined, preventDefault() {}, stopPropagation() {}, clientX: 5, clientY: 6, ...extra })); } };
  let text = '';
  Object.defineProperty(el, 'textContent', { get() { return text + el.children.map((c) => c.textContent).join(''); }, set(v) { text = String(v); el.children.slice().forEach((c) => { c.parentNode = null; }); el.children = []; } });
  Object.defineProperty(el, 'innerHTML', { get: () => '', set(v) { el.innerHTML_set = v; } });
  Object.defineProperty(el, 'className', { get: () => Array.from(classes).join(' '), set(v) { classes.clear(); String(v).split(' ').filter(Boolean).forEach((c) => classes.add(c)); } });
  return el;
}
const target = (id) => ({ contextId: 'scene-root', layerUid: 'L', frameScope: { kind: 'authored', frame: 3 }, strokeId: id });
const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const OUTER = { id: 'outer', name: 'Body' }, INNER = { id: 'inner' };
// Paint order, bottom first: v1 and the ribbon v2 (with its linked fill) in the inner group, v3 in the outer one, a loose shape and a loose image.
const ITEMS = () => [
  { id: 'v1', role: 'plain', filled: true, groupPath: [OUTER, INNER], style: { fill: '#112233', stroke: '#ffffff' }, strokeWidth: 2 },
  { id: 'v2', role: 'ribbon', filled: false, groupPath: [OUTER, INNER], style: { stroke: '#00ff00' }, strokeWidth: 4 },
  { id: 'v2f', role: 'fill', filled: true, owner: 'v2', style: { fill: '#ff0000' } },
  { id: 'v3', role: 'plain', filled: false, groupPath: [OUTER], style: { stroke: '#0000ff' }, strokeWidth: 3 },
  { id: 'v4', role: 'plain', filled: true, style: { fill: '#abcdef' } },
  { id: 'img', role: 'raster', filled: true, style: { opacity: 1 } },
].map((o) => { const { id, ...rest } = o; return { stableTarget: target(id), closed: true, count: 4, matrix: [1, 0, 0, 1, 0, 0], bounds: [0, 0, 10, 10], ...rest,
  segments: [seg(0, 0), seg(10, 0), seg(10, 10), seg(0, 10)] }; });

function harness(options) {
  const o = options || {}, paper = Paper.load(), toasts = [], calls = [], picks = [], reads = [], updates = [];
  let revision = 1, items = ITEMS();
  const pinNow = () => ({ instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: revision, documentSnapshotId: 's' + revision, canvasW: 400, canvasH: 400,
    layers: [{ layerUid: 'L', visible: true, locked: !!o.locked }] });
  const state = { tool: 'select', appMode: 'anim2d', currentFrame: 3, activeLayerIdx: 0, layers: [{ layerUid: 'L' }], selectedStrokeIndices: [] };
  const session = { info: pinNow, isActive: () => true, blocksLegacy: () => true,
    readPresentedObjectSession: () => ({ presentation: { ...pinNow(), frame: state.currentFrame } }),
    async readPresentedStrokes(selector, _pin, wanted) {
      reads.push({ frame: selector.frame, layerUid: selector.layerUid });
      return { items: items.map((i) => { const { segments, ...rest } = i; return wanted(rest) ? { ...rest, segments } : rest; }) };
    } };
  const doc = { createElement: element, createTextNode: (t) => { const n = element('span'); n.textContent = t; return n; }, createDocumentFragment() { const f = element('frag'); f.isFragment = true; return f; },
    getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 400, height: 400 }) }) };
  const spy = Object.assign({}, Intent, { applyPick: (root, item, shift) => { picks.push([item && item.data.strokeId, !!shift]); return Intent.applyPick(root, item, shift); } });
  const verdicts = o.verdicts || {};
  const intent = { rename: async (i) => { calls.push(['rename', JSON.parse(JSON.stringify(i))]); return verdicts.rename || { ok: true, applied: true }; },
    ungroup: async (i) => { calls.push(['ungroup', JSON.parse(JSON.stringify(i))]); return verdicts.ungroup || { ok: true, applied: true }; } };
  const win = { document: doc, state, paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, selectedPaths: [], ICO_GROUP: '<svg id="g"/>', ICO_IMAGE: '<svg id="i"/>',
    updateUI() { updates.push(revision); }, SMEngineBridge: { renderNow() {} }, showToast: (t) => toasts.push(t), showContextMenu: (x, y, entries) => { win.menu = entries; },
    SM: { t: (k) => ({ hdrElements: 'Elements', autoNameShape: 'Shape', autoNameStroke: 'Stroke', autoNameGroup: 'Group', autoNameText: 'Text', elementsRename: 'Rename',
      elementsSelect: 'Select', elementsSelectMembers: 'Select members', elementsUngroup: 'Ungroup', elementsPanelEmpty: 'No elements' }[k] || k) },
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: () => true }, NemoSelectCanvasIntent: Intent, NemoNativeShapeTreeModel: Model,
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX, y: e.clientY }) } } };
  const panel = Shape.create(win, { canvas: () => spy, intent: () => intent });
  const container = element('div');
  return { win, panel, container, toasts, calls, picks, reads, updates, state, set: (v) => { items = v; }, bump: () => { revision++; },
    rows: () => container.querySelectorAll('.motion-elem-row'), names: () => container.querySelectorAll('.lnm').map((n) => n.textContent),
    selected: () => win.selectedPaths.map((p) => p.data.strokeId) };
}
const settle = async () => { for (let i = 0; i < 12; i++) await new Promise((done) => setTimeout(done, 0)); };
const shown = async (h) => { h.panel.mount(h.container); await settle(); return h; };
const row = (h, pred) => h.rows().find(pred);
const gid = (id) => (r) => r.dataset.gid === id;
const sid = (id) => (r) => r.dataset.strokeid === id;

test('the listing becomes the baseline tree: groups at their first member, nested, front first, shapes numbered by draw order, a linked fill folded', () => {
  const labels = { shape: 'Shape', stroke: 'Stroke', group: 'Group', text: 'Text', image: 'Image' };
  const tree = Model.build(ITEMS().map(({ segments, ...i }) => i), labels);
  assert.deepEqual(tree.map((r) => r.type === 'group' ? 'g:' + r.gid : r.strokeId), ['img', 'v4', 'g:outer'], 'front first; the group sits where its first member does');
  const outer = tree[2];
  assert.equal(outer.name, 'Body'); assert.deepEqual(outer.children.map((r) => r.type === 'group' ? 'g:' + r.gid : r.strokeId), ['v3', 'g:inner']);
  const inner = outer.children[1];
  assert.deepEqual([inner.name, inner.named, inner.memberIds], ['Group', false, ['v1', 'v2']], 'an unnamed group takes the automatic name');
  assert.deepEqual(outer.memberIds, ['v1', 'v2', 'v3']);
  assert.deepEqual(inner.children.map((r) => [r.strokeId, r.label, r.fill]), [['v2', 'Shape 2', '#ff0000'], ['v1', 'Shape 1', '#112233']], 'the ribbon wears its linked fill; draw-order numbers');
  assert.equal(tree.flat.length, 5, 'a linked fill is no row of its own');
  assert.equal(tree[0].label, 'Image 5');
  assert.equal(Model.signature(tree), Model.signature(Model.build(ITEMS().map(({ segments, ...i }) => i), labels)));
});

test('the panel lists exactly the native read: header, rows in order, baseline classes, only the open groups expanded', async () => {
  const h = await shown(harness());
  assert.equal(h.container.children[0].id, 'native-shapes-sec');
  assert.equal(h.container.querySelectorAll('.phdr')[0].textContent.trim().endsWith('Elements'), true);
  assert.deepEqual(h.names(), ['Image 5', 'Shape 4', 'Body'], 'collapsed groups hide their members');
  assert.deepEqual(h.reads, [{ frame: 3, layerUid: 'L' }], 'one read for the layer at the playhead');
  const body = row(h, gid('outer'));
  assert.equal(body.className, 'lrow motion-elem-row motion-elem-group'); assert.equal(body.dataset.toplevel, '1');
  body.querySelector('.larrow').fire('click');
  assert.deepEqual(h.names(), ['Image 5', 'Shape 4', 'Body', 'Stroke 3', 'Group'], 'expanding shows the outer group\'s children, front first');
  assert.equal(row(h, sid('v3')).style.paddingLeft, '44px'); assert.equal(h.reads.length, 1, 'expanding repaints from the held listing');
  row(h, gid('inner')).querySelector('.larrow').fire('click');
  assert.deepEqual(h.names().slice(5), ['Shape 2', 'Shape 1']);
  assert.equal(row(h, sid('img')).querySelector('.motion-elem-swatch').classList.contains('icon'), true, 'an image has the icon swatch');
});

test('a row click selects the item as a Select-tool click does (applyPick on a fresh presented path); Shift/Alt toggles', async () => {
  const h = await shown(harness());
  row(h, sid('v4')).fire('click'); await settle();
  assert.deepEqual(h.picks, [['v4', false]]); assert.deepEqual(h.selected(), ['v4']);
  assert.equal(row(h, sid('v4')).classList.contains('act'), false, 'the highlight follows the next host paint');
  h.panel.refresh(false);
  assert.equal(row(h, sid('v4')).classList.contains('act'), true);
  row(h, sid('img')).fire('click', { shiftKey: true }); await settle();
  assert.deepEqual(h.selected(), ['v4', 'img'], 'Shift extends the selection');
  row(h, sid('img')).fire('click', { altKey: true }); await settle();
  assert.deepEqual(h.selected(), ['v4'], 'Alt on a selected row removes it');
  assert.equal(h.calls.length, 0, 'selecting writes nothing');
});

test('Select members selects every member of the group (nested ones included); a group click does the same', async () => {
  const h = await shown(harness());
  row(h, gid('outer')).fire('contextmenu');
  assert.deepEqual(h.win.menu.filter((m) => m.label).map((m) => m.label), ['Rename', 'Select members', 'Ungroup']);
  h.win.menu.find((m) => m.label === 'Select members').action(); await settle();
  assert.deepEqual(h.selected(), ['v1', 'v2', 'v3'], 'the three leaves, in listing order');
  assert.deepEqual(h.picks.map((p) => p[1]), [false, true, true], 'one replacing pick then additions');
  h.panel.refresh(false);
  assert.equal(row(h, gid('outer')).classList.contains('act'), true, 'a group row is lit while any member is selected');
  row(h, sid('v4')).fire('contextmenu'); assert.deepEqual(h.win.menu.map((m) => m.label), ['Rename', 'Select']);
  row(h, sid('v4')).fire('click'); await settle();
  row(h, gid('outer')).fire('click'); await settle();
  assert.deepEqual(h.selected(), ['v1', 'v2', 'v3']);
});

test('a locked layer is never selected from the panel: a notice, nothing selected', async () => {
  const h = await shown(harness({ locked: true }));
  row(h, sid('v4')).fire('click'); await settle();
  assert.deepEqual(h.selected(), []); assert.equal(h.toasts.length, 1);
});

test('Ungroup of a top-level group is ONE group.ungroup request naming a member; a nested group reports unavailable and sends nothing', async () => {
  const h = await shown(harness());
  row(h, gid('outer')).fire('contextmenu');
  h.win.menu.find((m) => m.label === 'Ungroup').action(); await settle();
  assert.deepEqual(h.calls, [['ungroup', { layerUid: 'L', contextId: 'scene-root', strokeIds: ['v1'] }]], 'one request: the engine dissolves the outermost group of that member');
  assert.equal(h.updates.length, 1, 'the host repaints after the applied edit');
  row(h, gid('outer')).querySelector('.larrow').fire('click');
  row(h, gid('inner')).fire('contextmenu');
  h.win.menu.find((m) => m.label === 'Ungroup').action(); await settle();
  assert.equal(h.calls.length, 1, 'no second request'); assert.match(h.toasts.at(-1), /top-level group/);
});

test('the real group intent builds the exact ungroup and rename requests the native contract admits', async () => {
  const sent = [];
  const ports = { pin: () => ({ instanceId: 'i', documentId: 'd', contentRevision: 7 }), requestId: (l) => l + '-1', declared: (op) => Contract.admitted(op),
    submit: async (request) => { sent.push(request); return { apiVersion: 2, requestId: request.requestId, ok: true, contentRevision: 8, result: { applied: true, historyEntriesAdded: 1 } }; } };
  const intent = GroupIntent.createGroupIntent(ports);
  const h = harness();
  const panel = Shape.create(h.win, { canvas: () => Intent, intent: () => intent });
  panel.mount(h.container); await settle();
  row(h, gid('outer')).fire('contextmenu'); h.win.menu.find((m) => m.label === 'Ungroup').action(); await settle();
  row(h, gid('outer')).fire('dblclick'); row(h, gid('outer')).querySelector('.lnm').children[0].value = 'Torso';
  row(h, gid('outer')).querySelector('.lnm').children[0].fire('keydown', { key: 'Enter' }); await settle();
  assert.deepEqual(sent.map((r) => [r.operation, r.expectedRevision, r.payload]), [
    ['command.document.object.group.ungroup', 7, { command: 'object.group.ungroup', layerUid: 'L', contextId: 'scene-root', strokeIds: ['v1'] }],
    ['command.document.object.group.rename', 7, { command: 'object.group.rename', layerUid: 'L', contextId: 'scene-root', groupId: 'outer', name: 'Torso' }]]);
});

test('Rename on a group is group.rename with the typed name; an unchanged name, an empty one and Escape send nothing', async () => {
  const h = await shown(harness());
  const edit = (id, value, key) => { const r = row(h, gid(id)); r.fire('dblclick'); const input = r.querySelector('.lnm').children[0]; input.value = value; input.fire('keydown', { key }); };
  edit('outer', 'Torso', 'Enter'); await settle();
  assert.deepEqual(h.calls, [['rename', { layerUid: 'L', contextId: 'scene-root', groupId: 'outer', name: 'Torso' }]]);
  edit('outer', 'Body', 'Enter'); edit('outer', '  ', 'Enter'); edit('outer', 'Nope', 'Escape'); await settle();
  assert.equal(h.calls.length, 1, 'the stored name, a blank and a cancelled edit send nothing');
  row(h, gid('outer')).querySelector('.larrow').fire('click');
  edit('inner', 'Arm', 'Enter'); await settle();
  assert.deepEqual(h.calls[1], ['rename', { layerUid: 'L', contextId: 'scene-root', groupId: 'inner', name: 'Arm' }], 'an unnamed nested group can be named too');
});

test('a refused edit is a notice and a resolved status, never a throw or a legacy write', async () => {
  const h = await shown(harness({ verdicts: { ungroup: { ok: false, code: 'stale_revision' }, rename: { ok: false, code: 'unavailable' } } }));
  row(h, gid('outer')).fire('contextmenu'); h.win.menu.find((m) => m.label === 'Ungroup').action(); await settle();
  assert.match(h.toasts.at(-1), /changed/); assert.equal(h.updates.length, 0);
  const r = row(h, gid('outer')); r.fire('dblclick'); const input = r.querySelector('.lnm').children[0]; input.value = 'X'; input.fire('keydown', { key: 'Enter' }); await settle();
  assert.match(h.toasts.at(-1), /could not be renamed/);
});

test('Rename on a plain shape reports unavailable: no input, no request, no legacy shapeNames write', async () => {
  const h = await shown(harness());
  row(h, sid('v4')).fire('dblclick');
  assert.equal(row(h, sid('v4')).querySelector('.lnm').children.length, 0, 'no inline input opens');
  row(h, sid('v4')).fire('contextmenu'); h.win.menu.find((m) => m.label === 'Rename').action();
  assert.equal(h.toasts.length, 2); assert.match(h.toasts[0], /not available on a native project/);
  assert.equal(h.calls.length, 0); assert.equal(h.state.layers[0].shapeNames, undefined);
});

test('the panel follows the project revision: a new pin reads the layer again and repaints the changed tree', async () => {
  const h = await shown(harness());
  h.panel.refresh(false); await settle(); assert.equal(h.reads.length, 1, 'the same key reads nothing again');
  h.set(ITEMS().filter((i) => i.stableTarget.strokeId !== 'v4')); h.bump(); h.panel.refresh(false); await settle();
  assert.equal(h.reads.length, 2); assert.deepEqual(h.names(), ['Image 4', 'Body']);
  h.set([]); h.bump(); h.panel.refresh(false); await settle();
  assert.equal(h.container.textContent.includes('No elements'), true, 'an empty layer says so');
});

test('a scrub is frame-only: the same frame repaints nothing, a run of frames reads once for the latest, unchanged rows keep their DOM', async () => {
  const h = await shown(harness());
  const before = h.rows();
  h.panel.refresh(true); await settle();
  assert.equal(h.reads.length, 1); assert.ok(h.rows().every((r, i) => r === before[i]), 'frameOnly with the same key rebuilds nothing');
  for (const f of [4, 5, 6, 7, 8]) { h.state.currentFrame = f; h.panel.refresh(true); }
  await settle();
  assert.deepEqual(h.reads.map((r) => r.frame), [3, 8], 'the frames of a scrub collapse: the settled frame is read, not one per tick');
  assert.ok(h.rows().every((r, i) => r === before[i]), 'an identical listing at another frame keeps the rows as they were');
});

test('the legacy renderer yields to the native presenter and writes nothing (shapes-panel.js)', () => {
  const src = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'shapes-panel.js'), 'utf8');
  const writes = [], list = { set innerHTML(v) { writes.push(v); }, appendChild() { writes.push('child'); } };
  const win = { addEventListener() {}, NemoNativeShapeTree: { owns: () => true }, SMMotion: { buildShapeTree() { writes.push('tree'); return []; } }, state: { activeLayerIdx: 0, layers: [{}] } };
  const ctx = vm.createContext({ window: win, document: { getElementById: () => list, createElement: () => ({ style: {} }) }, state: win.state, SM: { t: (k) => k } });
  vm.runInContext(src, ctx);
  win.renderShapesPanel();
  assert.deepEqual(writes, [], 'a native project is never rendered by the legacy Elements panel');
});
