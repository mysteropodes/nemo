'use strict';
// #2371 U11 / D36: Null, Effect, Guide, Folder and Effector creation and folder membership of the native layer list, over the real
// presenter, row controls, writes and kinds module. Every gesture must reach the fake native session as ONE typed request; the shipped
// timeline.js entry points must be thin delegations (no layer-list writer survives there).
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), vm = require('node:vm');
globalThis.NemoNativeObjectLayerLifecycleIntent = require('../src/js/adapters/native-object-layer-lifecycle-intent.js');
const Presenter = require('../src/js/adapters/native-layer-list-presenter.js'), RowUi = require('../src/js/adapters/native-layer-row-ui.js');
const Kinds = require('../src/js/adapters/native-layer-kinds-ui.js'), F = require('./helpers/native-layer-row-fixture.cjs');

const LAYERS = [
  { layerUid: 'a', name: 'A', color: '#111111', kind: 'draw', visible: true }, { layerUid: 'b', name: 'B', color: '#222222', kind: 'draw', visible: true, hasDuplicator: true },
  { layerUid: 'c', name: 'C', color: '#111111', kind: 'draw', visible: true }, { layerUid: 'f', name: 'Folder', color: '#444444', kind: 'folder', folderCollapsed: true, visible: true },
  { layerUid: 'g', name: 'G', kind: 'draw', visible: true, parentLayerUid: 'f' },
];
function setupAll(layers = LAYERS, options) {
  const { host, list, body } = F.makeHost(), s = F.session(layers, options), selections = [];
  host.NemoNativeObjectProject = { desktop: () => s, selectLayerUI: (h, index, mode, uid) => { selections.push([index, uid]); h._layerSel = [index]; h.state.activeLayerIdx = index; return true; } };
  host.SM = { t: (k) => ({ layerKindFolder: 'Dossier' })[k] || k };
  host.NemoNativeLayerRowUi = RowUi; host.NemoNativeLayerKindsUi = Kinds;
  function paint() {
    const rows = Presenter.render(list, { document: host.document, info: s.info(), selection: host._layerSel, shyMode: false, policyOwner: null, onPick() {}, onVisibility() {}, onPolicy() {},
      onOpen() { return false; }, rowEdit: (layer, i, row) => RowUi.bind(host, layer, i, row) }).rows;
    list.replaceChildren.apply(list, rows);
    rows.forEach((row, i) => { row.box = { left: 0, top: i * 22, width: 200, height: 22 }; });
    return rows;
  }
  return { host, list, body, s, selections, paint, rows: paint() };
}
const setup = setupAll;
const settle = () => new Promise((r) => setTimeout(r, 5));
const payloads = (t) => t.s.requests.map((q) => q.payload);

test('a Null over no selection sits at the canvas centre, is named after the first free Layer number and ends up selected', async () => {
  const t = setup(LAYERS.slice(0, 3)); t.host._layerSel = [];
  const r = await Kinds.add(t.host, 'null');
  assert.equal(r.ok, true); assert.equal(r.layerUid, 'new-1');
  assert.deepEqual(payloads(t), [{ command: 'object.layer.kind.create', kind: 'null', name: 'Null 1', frame: 3, pos: [160, 90] }]);
  assert.deepEqual(t.selections, [[3, 'new-1']], 'the new layer is the selection');
  assert.equal(t.s.requests.length, 1, 'one gesture, one request');
  // "Layer 1" taken: the next free number
  const u = setup([{ layerUid: 'x', name: 'Layer 1', kind: 'draw' }]); u.host._layerSel = [];
  await Kinds.add(u.host, 'guide');
  assert.equal(payloads(u)[0].name, 'Guide 2');
});

test('a Null or Folder over a selection adopts it (no anchor: the engine centres it); Effect and Guide never do', async () => {
  const t = setup(), pick = () => { t.host._layerSel = [0, 2, 0, 99]; };
  pick(); await Kinds.add(t.host, 'null');
  assert.deepEqual(payloads(t)[0], { command: 'object.layer.kind.create', kind: 'null', name: 'Null 1', frame: 3, children: ['a', 'c'] });
  pick(); await Kinds.add(t.host, 'folder');
  assert.equal(payloads(t)[1].name, 'Dossier 1'); assert.deepEqual(payloads(t)[1].children, ['a', 'c']);
  pick(); await Kinds.add(t.host, 'effect');
  assert.deepEqual(payloads(t)[2], { command: 'object.layer.kind.create', kind: 'effect', name: 'Effet 1', frame: 3 }, 'no position, no children');
  pick(); await Kinds.add(t.host, 'guide');
  assert.deepEqual(payloads(t)[3], { command: 'object.layer.kind.create', kind: 'guide', name: 'Guide 1', frame: 3, pos: [160, 90] });
  assert.equal(t.s.state.layers.find((l) => l.layerUid === 'a').parentLayerUid, 'new-2', 'the adopted layer is parented (here to the Folder made last)');
});

test('an effector needs a duplicator host: the menu offers it only there, a plain layer gets the notice and nothing is sent', async () => {
  const t = setup();
  const labels = (i) => Kinds.rowItems(t.host, i, t.rows[i]).map((item) => item.label);
  assert.ok(labels(1).includes('Add an effector layer')); assert.ok(!labels(0).includes('Add an effector layer'));
  const item = Kinds.rowItems(t.host, 1, t.rows[1]).find((i) => i.label === 'Add an effector layer');
  item.action(); await settle();
  assert.deepEqual(payloads(t), [{ command: 'object.layer.kind.create', kind: 'effector', name: 'Effector 1', frame: 3, target: 'b', pos: [160, 90] }]);
  assert.ok(t.host.toasts.includes('Effector layer added'));
  const none = setup(); await Kinds.addEffector(none.host, 'a');
  assert.equal(none.s.requests.length, 0); assert.deepEqual(none.host.toasts, ['Enable the duplicator on this layer first']);
});

test('moving layers into a folder is one reparent: the folder and other folders are not counted, the toast names the folder', async () => {
  const t = setup();
  await Kinds.moveIntoFolder(t.host, [0, 2, 3], 3);
  assert.deepEqual(payloads(t), [{ command: 'object.layer.folder.reparent', layerUids: ['a', 'c', 'f'], folderUid: 'f', frame: 3 }]);
  assert.deepEqual(t.host.toasts, ['2 layers moved into “Folder”']);
  const one = setup(); await Kinds.moveIntoFolder(one.host, [0], 3);
  assert.deepEqual(one.host.toasts, ['Layer moved into “Folder”']);
  const only = setup(); const r = await Kinds.moveIntoFolder(only.host, [3], 3);
  assert.equal(only.s.requests.length, 0, 'a folder alone is nothing to move'); assert.equal(r.status, 'unavailable');
  const plain = setup(); await Kinds.moveIntoFolder(plain.host, [0], 2);
  assert.equal(plain.s.requests.length, 0, 'a plain layer is not a folder');
});

test('Remove from the folder is one reparent to none, only for a member, with the baseline notice', async () => {
  const t = setup();
  await Kinds.leaveFolder(t.host, 0);
  assert.equal(t.s.requests.length, 0, 'a layer outside any folder is left alone');
  const items = Kinds.rowItems(t.host, 4, t.rows[4]);
  assert.equal(items.find((i) => i.label === 'Remove from the folder').disabled, false);
  assert.equal(Kinds.rowItems(t.host, 0, t.rows[0]).find((i) => i.label === 'Remove from the folder').disabled, true);
  await Kinds.leaveFolder(t.host, 4);
  assert.deepEqual(payloads(t), [{ command: 'object.layer.folder.reparent', layerUids: ['g'], folderUid: null, frame: 3 }]);
  assert.deepEqual(t.host.toasts, ['Layer removed from the folder']);
});

test('the folder chevron toggles through the compositing edit of the row writes', async () => {
  const t = setup(), written = [];
  t.host.NemoNativeObjectLayerCompositing = { desktop: () => ({ setFolderCollapsed: async (at, collapsed) => { written.push([at, collapsed]); return { applied: true }; } }) };
  await Kinds.toggleFolder(t.host, 3);
  assert.deepEqual(written, [[3, false]]);
  assert.equal((await Kinds.toggleFolder(t.host, 0)).status, 'unavailable', 'only a folder folds');
});

test('the Insert submenu leads the row menu and offers every kind through the SM entry points', async () => {
  const t = setup(), calls = [];
  ['addLayer', 'addNullLayer', 'addFolderLayer', 'addEffectLayer', 'addGuideLayer', 'addJoystickLayer', 'addSliderLayer'].forEach((n) => { t.host.SM[n] = () => calls.push(n); });
  const items = RowUi.menuItems(t.host, 1);
  assert.equal(items[0].label, 'ctxMenuInsert'.replace('ctxMenuInsert', 'Insert…'));
  items[0].action();
  const submenu = t.host.menus[t.host.menus.length - 1];
  assert.deepEqual(submenu.map((i) => i.label), ['Insert a layer', 'Insert a Null layer', 'Insert a folder', 'Insert an effect layer', 'Insert a Guide layer', 'Insert a Joystick', 'Insert a Slider']);
  submenu.forEach((i) => i.action());
  assert.deepEqual(calls, ['addLayer', 'addNullLayer', 'addFolderLayer', 'addEffectLayer', 'addGuideLayer', 'addJoystickLayer', 'addSliderLayer']);
});

test('dropping on the middle band of a folder row moves into it; the edges stay a reorder; a folder never nests', async () => {
  const t = setup();
  t.rows[0].children[0].fire('mousedown', { button: 0, clientX: 0, clientY: 5 });
  t.host.fire('mousemove', { clientX: 0, clientY: 3 * 22 + 11 }); // the middle of the folder row
  assert.ok(t.rows[3].className.includes('folder-drop-target'), 'the folder row is marked');
  assert.equal(t.body.children.some((c) => c.className === 'layer-drop-indicator'), false, 'no gap line while a folder is the target');
  t.host.fire('mouseup', {}); await settle();
  assert.deepEqual(payloads(t), [{ command: 'object.layer.folder.reparent', layerUids: ['a'], folderUid: 'f', frame: 3 }]);
  assert.equal(t.rows[3].className.includes('folder-drop-target'), false, 'the mark is gone');
  const edge = setup();
  edge.rows[0].children[0].fire('mousedown', { button: 0, clientX: 0, clientY: 5 });
  edge.host.fire('mousemove', { clientX: 0, clientY: 3 * 22 + 2 }); edge.host.fire('mouseup', {}); await settle();
  assert.equal(payloads(edge)[0].command, 'object.layer.reorder', 'the top edge of a folder is an ordinary gap');
  const dragged = setup();
  dragged.rows[3].children[0].fire('mousedown', { button: 0, clientX: 0, clientY: 3 * 22 + 5 });
  dragged.host.fire('mousemove', { clientX: 0, clientY: 11 });
  assert.equal(dragged.rows[3].className.includes('folder-drop-target') || dragged.rows[0].className.includes('folder-drop-target'), false);
});

test('dragging a member out lands it at the gap and leaves the folder in the SAME command', async () => {
  const OPEN = LAYERS.map((l) => (l.layerUid === 'f' ? { ...l, folderCollapsed: false } : l)), setup = (layers = OPEN) => setupAll(layers);
  const t = setup();
  t.rows[4].children[0].fire('mousedown', { button: 0, clientX: 0, clientY: 4 * 22 + 5 });
  t.host.fire('mousemove', { clientX: 0, clientY: 22 + 3 }); // upper half of row b: gap 1
  t.host.fire('mouseup', {}); await settle();
  assert.deepEqual(payloads(t), [{ command: 'object.layer.folder.reparent', layerUids: ['g'], folderUid: null, frame: 3, reorder: { gap: 1, selection: { active: 'g', selected: ['g'] } } }]);
  assert.equal(t.s.state.layers[1].layerUid, 'g'); assert.equal(t.s.state.layers[1].parentLayerUid, null);
  // a member dropped exactly where it already is still leaves its folder
  const same = setup();
  same.rows[4].children[0].fire('mousedown', { button: 0, clientX: 0, clientY: 4 * 22 + 5 });
  same.host.fire('mousemove', { clientX: 0, clientY: 4 * 22 + 20 }); same.host.fire('mouseup', {}); await settle();
  assert.equal(payloads(same).length, 1); assert.equal(payloads(same)[0].folderUid, null);
});

test('a refusal toasts, selects nothing and writes no second request; no native project and an entered component answer unavailable', async () => {
  const refused = setup(LAYERS, { refuse: 'unavailable' }); refused.host._layerSel = [];
  const r = await Kinds.add(refused.host, 'null');
  assert.equal(r.ok, false); assert.equal(r.code, 'unavailable'); assert.deepEqual(refused.selections, []);
  assert.equal(refused.host.toasts.length, 1); assert.match(refused.host.toasts[0], /not available/);
  const off = setup(); off.s.blocksLegacy = () => false;
  assert.equal((await Kinds.add(off.host, 'null')).status, 'unavailable'); assert.equal(off.s.requests.length, 0);
  const inside = setup(); inside.host.NemoNativeComponentContext = { presented: () => true };
  assert.equal((await Kinds.add(inside.host, 'guide')).status, 'unavailable'); assert.equal(inside.s.requests.length, 0);
  const bare = {}; assert.equal((await Kinds.add(bare, 'null')).status, 'unavailable');
});

test('the shipped timeline.js entry points are one-line delegations: no layer-list writer survives there', async () => {
  const source = fs.readFileSync(require('node:path').join(__dirname, '../src/js/timeline.js'), 'utf8');
  const names = { addNullLayer: ['add', 'null'], addFolderLayer: ['add', 'folder'], addEffectLayer: ['add', 'effect'], addGuideLayer: ['add', 'guide'],
    addJoystickLayer: ['addWidget', 'joystick'], addSliderLayer: ['addWidget', 'slider'], toggleFolderLayerCollapsed: ['toggleFolder'], reparentLayersIntoFolder: ['moveIntoFolder'], removeLayerFromFolder: ['leaveFolder'] };
  const t = setup(); t.host._layerSel = [];
  const seen = [];
  t.host.NemoNativeLayerKindsUi = new Proxy({}, { get: (_, fn) => (...args) => { seen.push([fn, ...args.slice(1)]); return Promise.resolve({}); } });
  t.host.window = t.host;
  for (const [name, expect] of Object.entries(names)) {
    const line = source.split('\n').find((l) => l.startsWith('  ' + name + ':function('));
    assert.ok(line, name); assert.equal(line.split('\n').length, 1);
    assert.doesNotMatch(line, /createUserLayer|state\.layers|setLayerParent|pushUndoLayers/, name);
    const fn = vm.runInNewContext('({' + line.trim().replace(/,$/, '') + '})', { window: t.host, Promise })[name];
    seen.length = 0; await fn(0, 3);
    assert.equal(seen[0][0], expect[0], name); if (expect[1]) assert.equal(seen[0][1], expect[1], name);
  }
  const bare = vm.runInNewContext('({' + source.split('\n').find((l) => l.startsWith('  addNullLayer:function(')).trim().replace(/,$/, '') + '})', { window: {}, Promise });
  assert.equal((await bare.addNullLayer()).status, 'unavailable');
  const effector = fs.readFileSync(require('node:path').join(__dirname, '../src/js/effector-layer.js'), 'utf8');
  assert.doesNotMatch(effector.slice(effector.indexOf('function addEffectorLayer'), effector.indexOf('On-canvas marker')), /createUserLayer|state\.layers|motionStatic/);
});
