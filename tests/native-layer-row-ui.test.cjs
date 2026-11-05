'use strict';
// #2371 U11: the layer-row controls over the real presenter: in-place rename (double-click and F2), colour dot, folder chevron,
// drag reorder (cancel and stale safe), the row menu and the toolbar buttons. Listener and capture order come from the fake
// DOM in tests/helpers/native-layer-row-fixture.cjs; every write lands in the fake session as a recorded typed request.
const test = require('node:test'), assert = require('node:assert/strict');
const Presenter = require('../src/js/adapters/native-layer-list-presenter.js'), Ui = require('../src/js/adapters/native-layer-row-ui.js');
const Writes = require('../src/js/adapters/native-layer-row-writes.js'), F = require('./helpers/native-layer-row-fixture.cjs');

const LAYERS = [
  { layerUid: 'a', name: 'A', color: '#111111', kind: 'draw', visible: true }, { layerUid: 'b', name: 'B', color: '#222222', kind: 'draw', visible: true },
  { layerUid: 'c', name: 'C', color: '#111111', kind: 'draw', visible: true }, { layerUid: 'f', name: 'Folder', color: '#444444', kind: 'folder', folderCollapsed: false, visible: true },
  { layerUid: 'k', name: 'Instance', kind: 'component', visible: true },
];
function setup(layers = LAYERS, options) {
  const { host, list, body } = F.makeHost(), s = F.session(layers, options);
  const selections = [], swatches = [], compositing = [];
  host.NemoNativeObjectProject = { desktop: () => s, selectLayerUI: (h, index, mode, uid) => { selections.push([index, mode, uid]); h._layerSel = mode && mode.mode === 'toggle' ? h._layerSel.concat([index]) : [index]; h.state.activeLayerIdx = index; return true; } };
  host.NemoNativeObjectLayerCompositing = { desktop: () => ({ setFolderCollapsed: async (at, collapsed) => { compositing.push([at, collapsed]); s.state.layers[at].folderCollapsed = collapsed; s.state.contentRevision += 1; return { applied: true }; } }) };
  host.openLayerColorSwatches = (anchor, current, pick) => swatches.push({ anchor, current, pick });
  host.SM = { t: (k) => k, splitLayerAtPlayhead: (i) => { host.cut = i; } };
  host.NemoNativeLayerRowUi = Ui;
  function paint() {
    const info = s.info();
    const rows = Presenter.render(list, { document: host.document, info, selection: host._layerSel, shyMode: false, policyOwner: null, onPick() {}, onVisibility() {}, onPolicy() {}, onOpen() { return false; },
      rowEdit: (layer, i, row) => Ui.bind(host, layer, i, row) }).rows;
    list.replaceChildren.apply(list, rows);
    rows.forEach((row, i) => { row.box = { left: 0, top: i * 22, width: 200, height: 22 }; });
    return rows;
  }
  return { host, list, body, s, selections, swatches, compositing, paint, rows: paint() };
}
const pickOf = (row) => row.children[0];
const field = (row) => row.children.find((c) => c.className === 'native-layer-rename');
const settle = () => new Promise((r) => setTimeout(r, 5));

test('double-click renames in place: Enter commits the trimmed name as one layer.rename, the field goes away', async () => {
  const t = setup(), row = t.rows[1];
  row.fire('dblclick', { target: pickOf(row) });
  const input = field(row);
  assert.ok(input, 'a name field replaces the name'); assert.equal(input.value, 'B'); assert.equal(pickOf(row).style.display, 'none'); assert.equal(input.focused, true);
  input.value = '  Ink  ';
  input.fire('keydown', { key: 'Enter' });
  await settle();
  assert.equal(field(row), undefined); assert.notEqual(pickOf(row).style.display, 'none', 'the name is back');
  assert.deepEqual(t.s.requests.map((q) => q.payload), [{ command: 'object.layer.rename', layerUid: 'b', name: 'Ink' }]);
  assert.equal(t.s.state.layers[1].name, 'Ink');
});

test('Escape, an empty name and an unchanged name write nothing; blur commits like Enter', async () => {
  const t = setup(), row = t.rows[0];
  for (const [value, key] of [['X', 'Escape'], ['   ', 'Enter'], ['A', 'Enter']]) {
    row.fire('dblclick', { target: pickOf(row) });
    const input = field(row); input.value = value; input.fire('keydown', { key });
    assert.equal(field(row), undefined, key);
  }
  assert.equal(t.s.requests.length, 0);
  row.fire('dblclick', { target: pickOf(row) });
  const input = field(row); input.value = 'Blurred'; input.fire('blur', {});
  await settle();
  assert.equal(t.s.requests[0].payload.name, 'Blurred');
  // a second double-click while the field is open does not stack a second field
  const again = setup(), r = again.rows[2];
  r.fire('dblclick', { target: pickOf(r) }); r.fire('dblclick', { target: pickOf(r) });
  assert.equal(r.children.filter((c) => c.className === 'native-layer-rename').length, 1);
});

test('the field swallows its own pointer events so a drag or a pick never starts under it; a component row is not renamed', () => {
  const t = setup(), row = t.rows[0];
  row.fire('dblclick', { target: pickOf(row) });
  const input = field(row), down = input.fire('mousedown', { button: 0 });
  assert.equal(down.stopped, true);
  t.host.fire('mousemove', { clientX: 5, clientY: 60 });
  assert.equal(t.body.children.length, 0, 'no ghost');
  const comp = t.rows[4];
  comp.fire('dblclick', { target: pickOf(comp) });
  assert.equal(field(comp), undefined);
  const eye = t.rows[1].children[1];
  t.rows[1].fire('dblclick', { target: eye });
  assert.equal(field(t.rows[1]), undefined, 'a double-click on the eye is not a rename');
});

test('F2 (cmdRenameLayer) opens the field of the active layer; other actions and a missing native project are not consumed', () => {
  const t = setup();
  t.host.state.activeLayerIdx = 2; t.host._layerSel = [2];
  const e = { preventDefault() { this.prevented = true; } };
  assert.equal(Ui.shortcut(t.host, 'cmdRenameLayer', e), true); assert.equal(e.prevented, true);
  assert.ok(field(t.rows[2]));
  assert.equal(Ui.shortcut(t.host, 'cmdNextFrame', e), false);
  const bare = F.makeHost().host;
  bare.NemoNativeObjectProject = { desktop: () => null };
  assert.equal(Ui.shortcut(bare, 'cmdRenameLayer', e), false, 'legacy keeps its own path off-native');
  assert.equal(Ui.toolbar(bare, 'delete'), false);
});

test('the colour dot opens the swatches; a pick recolours the whole selection when the layer is one of several', async () => {
  const t = setup(), dot = t.rows[0].children.find((c) => /layer-color-dot/.test(c.className));
  t.host._layerSel = [0, 2]; t.host.state.activeLayerIdx = 0;
  const e = dot.fire('click', {});
  assert.equal(e.stopped, true); assert.equal(t.swatches.length, 1); assert.equal(t.swatches[0].current, '#111111');
  t.swatches[0].pick('#aabbcc');
  await settle();
  assert.deepEqual(t.s.requests[0].payload, { command: 'object.layer.color.set', layerUid: 'a', color: '#aabbcc', selected: ['a', 'c'] });
  assert.match(t.host.toasts[0], /2/);
  // a dot outside the selection recolours just that layer
  const other = t.rows[1].children.find((c) => /layer-color-dot/.test(c.className));
  other.fire('click', {}); t.swatches[1].pick('#010203');
  await settle();
  assert.deepEqual(t.s.requests[1].payload, { command: 'object.layer.color.set', layerUid: 'b', color: '#010203' });
});

test('Shift+click on the dot selects every layer wearing that colour', () => {
  const t = setup(), dot = t.rows[0].children.find((c) => /layer-color-dot/.test(c.className));
  dot.fire('click', { shiftKey: true });
  assert.deepEqual(t.selections.map((s) => [s[0], s[1] && s[1].mode || null]), [[0, null], [2, 'toggle']]);
  assert.equal(t.swatches.length, 0); assert.equal(t.s.requests.length, 0);
});

test('the folder chevron folds the folder through the compositing edit', async () => {
  const t = setup(), arrow = t.rows[3].children.find((c) => /larrow/.test(c.className));
  assert.equal(arrow.title, 'Replier le dossier');
  arrow.fire('click', {});
  await settle();
  assert.deepEqual(t.compositing, [[3, true]]);
});

test('drag: a vertical pull picks the layer up, the drop is one layer.reorder at the right gap, the trailing click is swallowed', async () => {
  const t = setup(), pick = pickOf(t.rows[0]);
  pick.fire('mousedown', { button: 0, clientX: 10, clientY: 5 });
  t.host.fire('mousemove', { clientX: 10, clientY: 7 });
  assert.equal(t.body.children.length, 0, 'under 4px is still a click');
  t.host.fire('mousemove', { clientX: 10, clientY: 60 }); // row 2 (top 44), lower half → below c
  assert.equal(t.body.children.length, 2, 'ghost and drop line'); assert.equal(t.rows[0].style.opacity, '0.42');
  const line = t.body.children.find((c) => c.className === 'layer-drop-indicator');
  assert.equal(line.style.top, '66px');
  t.host.fire('mouseup', { clientX: 10, clientY: 60 });
  await settle();
  assert.equal(t.host._layerDragJustEnded, true); assert.equal(t.body.children.length, 0); assert.equal(t.rows[0].style.opacity, '');
  assert.deepEqual(t.s.requests.map((q) => q.payload), [{ command: 'object.layer.reorder', layerUids: ['a'], gap: 3, selection: { active: 'a', selected: ['a'] } }]);
  assert.equal(t.s.state.layers.map((l) => l.layerUid).join(''), 'bcafk');
  // pointer events drive the same gesture
  const p = setup(); pickOf(p.rows[2]).fire('pointerdown', { button: 0, clientX: 0, clientY: 50 });
  p.host.fire('pointermove', { clientX: 0, clientY: 2 }); p.host.fire('pointerup', {});
  await settle();
  assert.equal(p.s.requests[0].payload.gap, 0);
});

test('drag of a row inside a multi-selection moves the whole block; a drop on its own place writes nothing', async () => {
  const t = setup();
  t.host._layerSel = [0, 1]; t.host.state.activeLayerIdx = 0;
  pickOf(t.rows[1]).fire('mousedown', { button: 0, clientX: 0, clientY: 30 });
  t.host.fire('mousemove', { clientX: 0, clientY: 100 }); t.host.fire('mouseup', {});
  await settle();
  assert.deepEqual(t.s.requests[0].payload.layerUids, ['a', 'b']); assert.deepEqual(t.s.requests[0].payload.selection, { active: 'a', selected: ['a', 'b'] });
  assert.equal(t.s.requests[0].payload.gap, 5);
  const same = setup();
  pickOf(same.rows[1]).fire('mousedown', { button: 0, clientX: 0, clientY: 28 });
  same.host.fire('mousemove', { clientX: 0, clientY: 24 }); same.host.fire('mousemove', { clientX: 0, clientY: 45 }); same.host.fire('mouseup', {});
  await settle();
  assert.equal(same.s.requests.length, 0, 'dropping a layer next to itself is not a gesture');
});

test('drag is cancel and stale safe: Escape, a project that moved on, a vanished layer and button presses write nothing', async () => {
  const esc = setup();
  pickOf(esc.rows[0]).fire('mousedown', { button: 0, clientX: 0, clientY: 5 });
  esc.host.fire('mousemove', { clientX: 0, clientY: 70 });
  esc.host.document.fire('keydown', { key: 'Escape' });
  assert.equal(esc.body.children.length, 0); assert.equal(esc.rows[0].style.opacity, '');
  esc.host.fire('mouseup', {}); await settle();
  assert.equal(esc.s.requests.length, 0);
  const moved = setup();
  pickOf(moved.rows[0]).fire('mousedown', { button: 0, clientX: 0, clientY: 5 });
  moved.host.fire('mousemove', { clientX: 0, clientY: 70 });
  moved.s.state.contentRevision += 1; moved.s.state.documentSnapshotId = 'other';
  moved.host.fire('mouseup', {}); await settle();
  assert.equal(moved.s.requests.length, 0); assert.match(moved.host.toasts[0], /cancelled/);
  const gone = setup();
  pickOf(gone.rows[0]).fire('mousedown', { button: 0, clientX: 0, clientY: 5 });
  gone.host.fire('mousemove', { clientX: 0, clientY: 70 });
  gone.s.state.layers.shift();
  gone.host.fire('mouseup', {}); await settle();
  assert.equal(gone.s.requests.length, 0);
  const eye = setup();
  eye.rows[0].children[1].fire('mousedown', { button: 0, clientX: 0, clientY: 5 });
  eye.host.fire('mousemove', { clientX: 0, clientY: 70 });
  assert.equal(eye.body.children.length, 0, 'a button press never starts a drag');
  const right = setup(); pickOf(right.rows[0]).fire('mousedown', { button: 2, clientX: 0, clientY: 5 });
  right.host.fire('mousemove', { clientX: 0, clientY: 70 });
  assert.equal(right.body.children.length, 0);
});

test('the row menu duplicates, deletes, renames, splits and colours; a right-click outside the selection selects the row first', async () => {
  const t = setup();
  t.host._layerSel = [0]; t.host.state.activeLayerIdx = 0;
  const items = Ui.menuItems(t.host, 2);
  assert.deepEqual(t.selections[0].slice(0, 1), [2], 'the clicked row becomes the selection');
  const labels = items.filter((i) => !i.sep).map((i) => i.label);
  assert.deepEqual(labels, ['Duplicate the layer', 'Delete the layer', 'Rename', 'Split into layers (one shape per layer)', 'Cut at the playhead  (⌘⇧D)', 'Layer colour']);
  const by = (n) => items.find((i) => i.label === n);
  by('Delete the layer').action(); await settle();
  assert.deepEqual(t.s.requests[0].payload.selection, { active: 'c', selected: ['c'] }, 'only what the click named');
  const again = Ui.menuItems(t.host, 0);
  again.find((i) => i.label === 'Duplicate the layer').action(); await settle();
  assert.equal(t.s.requests[1].payload.command, 'object.layer.duplicate');
  again.find((i) => i.label === 'Split into layers (one shape per layer)').action(); await settle();
  assert.equal(t.s.requests[2].payload.command, 'object.layer.split'); assert.equal(t.s.requests[2].payload.frame, 3);
  again.find((i) => /Cut at the playhead/.test(i.label)).action(); assert.equal(t.host.cut, 0);
  again.find((i) => i.label === 'Rename').action();
  assert.ok(field(t.rows[0]), 'Rename opens the field on the clicked row');
  const comp = Ui.menuItems(t.host, 4);
  assert.equal(comp.find((i) => /Split into layers/.test(i.label)).disabled, true, 'a component cannot be split');
  assert.ok(Ui.menuItems(t.host, 3).some((i) => /dossier/.test(i.label || '')));
});

test('toolbar delete and duplicate act on the selection and report refusals; the last layer is protected', async () => {
  const t = setup();
  t.host._layerSel = [1, 2]; t.host.state.activeLayerIdx = 1;
  assert.equal(Ui.toolbar(t.host, 'delete'), true); await settle();
  assert.deepEqual(t.s.requests[0].payload.selection, { active: 'b', selected: ['b', 'c'] });
  t.host._layerSel = [0]; t.host.state.activeLayerIdx = 0;
  assert.equal(Ui.toolbar(t.host, 'duplicate'), true); await settle();
  assert.equal(t.s.requests[1].payload.layerUid, 'a');
  const one = setup([{ layerUid: 'x', name: 'X' }]);
  one.host._layerSel = [0];
  Ui.toolbar(one.host, 'delete'); await settle();
  assert.equal(one.s.requests.length, 0); assert.equal(one.host.toasts[0], 'The last layer cannot be deleted');
  const locked = setup(LAYERS, { refuse: 'unavailable' });
  Ui.toolbar(locked.host, 'duplicate'); await settle();
  assert.match(locked.host.toasts[0], /locked/);
});
