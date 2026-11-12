'use strict';
// D36 (#2521): the Matte row of the native Motion inspector and its two menus (openMatteConfigMenu / openExtraMatteMenu of
// src/js/motion.js @1065e9d8). The real rows module, compositing adapter, rig command and contract run against a fake native session
// that applies the compositing edits with the owner's semantics (scene_effect_layer_edit.rs); the legacy writers THROW if the rows
// ever reach them. A pick is exactly one native compositing edit, a stale menu sends nothing.
const test = require('node:test');
const assert = require('node:assert/strict');
const Parent = require('../src/js/adapters/native-layer-parent.js');
const Compositing = require('../src/js/adapters/native-object-layer-compositing.js');
const Rows = require('../src/js/adapters/native-motion-layer-link-rows.js');
const LockMenu = require('../src/js/adapters/native-motion-key-lock-menu.js');

const OP = 'command.document.object.layer.compositing.edit';
const plain = (value) => JSON.parse(JSON.stringify(value));
const settle = async () => { for (let i = 0; i < 14; i += 1) await new Promise((resolve) => setImmediate(resolve)); };

function element(tagName) {
  const classes = new Set(), listeners = {}, kids = [];
  const el = { tagName, style: {}, dataset: {}, textContent: '', type: '', title: '', parentNode: null,
    classList: { add: (x) => classes.add(x), remove: (x) => classes.delete(x), contains: (x) => classes.has(x) },
    addEventListener(k, f) { listeners[k] = f; }, removeEventListener() {},
    replaceChildren(...next) { kids.splice(0).forEach((k) => { k.parentNode = null; }); next.forEach((k) => this.appendChild(k)); },
    appendChild(k) { if (k.parentNode) k.parentNode.removeChild(k); kids.push(k); k.parentNode = this; return k; },
    removeChild(k) { kids.splice(kids.indexOf(k), 1); k.parentNode = null; },
    getBoundingClientRect: () => ({ left: 10, top: 20, width: 9, height: 9, bottom: 29 }),
    fire(type, extra) { return listeners[type] && listeners[type](Object.assign({ target: this, preventDefault() {}, stopPropagation() {} }, extra)); } };
  Object.defineProperty(el, 'className', { get: () => [...classes].join(' '), set(v) { classes.clear(); String(v).split(/\s+/).filter(Boolean).forEach((x) => classes.add(x)); } });
  Object.defineProperty(el, 'children', { get: () => kids.slice() });
  return el;
}
const walk = (n) => [n, ...n.children.flatMap(walk)];
const named = (n, cls) => walk(n).find((x) => x.className.split(' ').includes(cls));
const label = (row) => row.children[0].textContent;
const pillText = (row) => named(row, 'mp-label').textContent;

const STRINGS = { fieldMatte: 'Cache', matteAlpha: 'Alpha', matteAlphaInverted: 'Alpha (inversé)', matteLuma: 'Luminance', matteLumaInverted: 'Luminance (inversée)',
  matteMenuSourceHdr: 'Calque de matte :', matteMenuModeHdr: 'Mode :', matteMenuRemove: 'Retirer le matte', toastNoLayerLeftForMatte: 'Plus aucun autre calque disponible comme matte' };

function rowsOf() {
  return [
    { layerUid: 'L-a', name: 'A', kind: 'draw', matteMode: 'luma', matteSourceLayerUid: 'L-b', mattesMore: [{ uid: 'L-c', mode: 'alpha' }, { uid: 'L-d', mode: 'lumaInverted' }] },
    { layerUid: 'L-b', name: 'B', kind: 'draw' },
    { layerUid: 'L-c', name: 'C', kind: 'draw' },
    { layerUid: 'L-d', name: 'D', kind: 'draw' },
    { layerUid: 'L-e', name: 'E', kind: 'draw' },
  ];
}
// The owner's semantics for the compositing edits the matte menus use (scene_effect_layer_edit.rs apply()).
function applyEdit(rows, p) {
  const row = rows.find((r) => r.layerUid === p.layerUid);
  const on = () => row.matteMode && row.matteMode !== 'none';
  if (p.edit === 'matteSource') { row.matteSourceLayerUid = p.source; if (!on()) row.matteMode = 'alpha'; }
  else if (p.edit === 'matteMode') { row.matteMode = p.mode; if (p.source) row.matteSourceLayerUid = p.source; }
  else if (p.edit === 'matteClear') { delete row.matteMode; delete row.matteSourceLayerUid; delete row.mattesMore; }
  else if (p.edit === 'matteMoreAdd') { if (!on()) throw new Error('invalid_request'); row.mattesMore = (row.mattesMore || []).concat([{ uid: p.source, mode: p.mode || 'alpha' }]); }
  else if (p.edit === 'matteMoreSet') { const e = row.mattesMore[p.index]; if (!e) throw new Error('invalid_request'); if (p.source) e.uid = p.source; if (p.mode) e.mode = p.mode; }
  else if (p.edit === 'matteMoreRemove') { if (p.index >= row.mattesMore.length) throw new Error('invalid_request'); row.mattesMore.splice(p.index, 1); if (!row.mattesMore.length) delete row.mattesMore; }
  else throw new Error('unexpected edit ' + p.edit);
}
function harness(options = {}) {
  const sent = [], toasts = [], menus = [], refreshed = [];
  const pin = { instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 1, contentRevision: 4, documentSnapshotId: 'snap-4', layers: options.rows || rowsOf() };
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, requestId: (name) => name + '-' + (sent.length + 1),
    runResourceCommand: (action) => action({
      async dispatch(request) {
        sent.push(request);
        const base = { apiVersion: 2, requestId: request.requestId, instanceId: pin.instanceId, documentId: pin.documentId };
        if (options.refuse) return Object.assign(base, { contentRevision: pin.contentRevision, ok: false, error: { code: options.refuse, message: 'refused' } });
        applyEdit(pin.layers, request.payload); pin.contentRevision += 1;
        return Object.assign(base, { contentRevision: pin.contentRevision, ok: true, result: { applied: true, historyEntriesAdded: 1 } });
      },
      async reconcile() {},
    }) };
  const throws = (name) => () => { throw new Error('LEGACY ' + name); };
  const host = { SM: options.noStrings ? undefined : { t: (key) => STRINGS[key] || key }, NemoNativeMotionKeyLockMenu: LockMenu, document: { createElement: element }, state: { currentFrame: 9 },
    NemoNativeObjectProject: { desktop: () => session }, showToast: (m) => toasts.push(m), showContextMenu: (x, y, items) => menus.push({ x, y, items }),
    nativeCompositingRefresh: () => refreshed.push('refresh'), pushUndo: throws('pushUndo'), pushUndoLayers: throws('pushUndoLayers'), saveActiveLayerFrame: throws('saveActiveLayerFrame') };
  if (!options.noCompositing) host.NemoNativeObjectLayerCompositing = Compositing;
  return { host, sent, toasts, menus, refreshed, pin, adapter: Parent.create(session) };
}
const build = (h, uid, contextId = 'scene-root') => Rows.build(h.host, { contextId, layerUid: uid });
const labels = (items) => items.map((item) => (item.sep ? '--' : item.label));

test('the Matte row sits between Time and Lock with the baseline pill, a plus once a matte exists and one dim row per additional matte', () => {
  const h = harness();
  const rows = build(h, 'L-a').node.children;
  assert.equal(rows.length, 6);
  assert.deepEqual(rows.slice(0, 5).map(label), ['Path', 'Time', 'Cache', '+ Cache', '+ Cache']);
  assert.equal(named(rows[5], 'lparent').textContent.length > 0, true, 'the lock row stays last');
  const matte = rows[2];
  assert.equal(matte.className, 'lrow motion-prop-row');
  assert.equal(matte.children[0].style.minWidth, '70px');
  assert.equal(pillText(matte), 'B  ·  Luminance');
  assert.equal(named(matte, 'motion-parent-pill').className, 'lparent motion-parent-pill');
  const plus = named(matte, 'motion-matte-add');
  assert.equal(plus.tagName, 'button');
  assert.equal(plus.className, 'motion-expr-glob motion-matte-add');
  assert.equal(plus.textContent, '+');
  assert.equal(rows[3].children[0].style.opacity, '.7');
  assert.equal(pillText(rows[3]), 'C  ·  Alpha');
  assert.equal(pillText(rows[4]), 'D  ·  Luminance (inversée)');
  assert.equal(named(rows[3], 'motion-matte-add'), undefined, 'the plus is on the first matte row only');
});

test('a layer without a matte shows the none pill and no plus; the implicit source is the layer directly above in the stack order', () => {
  const h = harness();
  const bare = build(h, 'L-b').node.children[2];
  assert.equal(pillText(bare), '—');
  assert.equal(named(bare, 'motion-parent-pill').className, 'lparent motion-parent-pill none');
  assert.equal(named(bare, 'motion-matte-add'), undefined);
  assert.equal(build(h, 'L-b').node.children.length, 4, 'path, time, matte, lock: no extra rows');
  const implicit = harness({ rows: [{ layerUid: 'L-a', name: 'A', kind: 'draw', matteMode: 'alpha' }, { layerUid: 'L-b', name: 'B', kind: 'draw' }] });
  assert.equal(pillText(build(implicit, 'L-a').node.children[2]), 'B  ·  Alpha', 'no stored source: the next row, as the baseline shows it');
});

test('without the native compositing module or strings the rows still render and fall back to plain names', () => {
  const none = harness({ noCompositing: true });
  assert.equal(build(none, 'L-a').node.children.length, 3, 'no matte row when the host has no compositing module');
  const plainHost = harness({ noStrings: true });
  assert.equal(pillText(build(plainHost, 'L-a').node.children[2]), 'B  ·  luma');
});

test('the first matte menu is the baseline list: source header, the other layers with the check on the source, mode header, four modes, Remove', () => {
  const h = harness();
  const rows = build(h, 'L-a');
  named(rows.node.children[2], 'motion-parent-pill').fire('click', { clientX: 70, clientY: 80 });
  assert.equal(h.menus.length, 1);
  assert.deepEqual([h.menus[0].x, h.menus[0].y], [70, 80], 'opens at the click, like the baseline');
  const items = h.menus[0].items;
  assert.deepEqual(labels(items), ['Calque de matte :', '  B  ✓', '  C', '  D', '  E', '--', 'Mode :', '  Alpha', '  Alpha (inversé)', '  Luminance  ✓', '  Luminance (inversée)', '--', 'Retirer le matte']);
  assert.equal(items[0].disabled, true);
  assert.equal(items[6].disabled, true);
  assert.equal(h.sent.length, 0, 'opening writes nothing');
  const none = harness();
  named(build(none, 'L-b').node.children[2], 'motion-parent-pill').fire('click');
  assert.equal(none.menus[0].x, 10, 'without click coordinates it anchors under the pill');
  assert.equal(labels(none.menus[0].items).includes('Retirer le matte'), false, 'no Remove without a matte');
  assert.equal(none.menus[0].items.some((item) => item.label && item.label.endsWith('✓')), false);
});

test('a source pick, a mode pick and Remove are each ONE native compositing edit, and the rows repaint from the confirmed rows', async () => {
  const h = harness();
  const rows = build(h, 'L-a');
  named(rows.node.children[2], 'motion-parent-pill').fire('click');
  h.menus[0].items.find((item) => item.label === '  D').action();
  await settle();
  assert.equal(h.sent.length, 1);
  assert.equal(h.sent[0].operation, OP);
  assert.equal(h.sent[0].expectedRevision, 4);
  assert.deepEqual(plain(h.sent[0].payload), { command: 'object.layer.compositing.edit', layerUid: 'L-a', edit: 'matteSource', source: 'L-d' });
  assert.equal(pillText(rows.node.children[2]), 'D  ·  Luminance');
  assert.equal(h.refreshed.length, 1, 'the layer list and timeline are asked to repaint');
  named(rows.node.children[2], 'motion-parent-pill').fire('click');
  h.menus[1].items.find((item) => item.label === '  Alpha (inversé)').action();
  await settle();
  assert.deepEqual(plain(h.sent[1].payload), { command: 'object.layer.compositing.edit', layerUid: 'L-a', edit: 'matteMode', mode: 'alphaInverted' });
  assert.equal(pillText(rows.node.children[2]), 'D  ·  Alpha (inversé)');
  named(rows.node.children[2], 'motion-parent-pill').fire('click');
  h.menus[2].items.find((item) => item.label === 'Retirer le matte').action();
  await settle();
  assert.deepEqual(plain(h.sent[2].payload), { command: 'object.layer.compositing.edit', layerUid: 'L-a', edit: 'matteClear' });
  assert.equal(h.sent.length, 3);
  assert.equal(rows.node.children.length, 4, 'the additional matte rows go with the matte');
  assert.equal(pillText(rows.node.children[2]), '—');
});

test('picking a source on a layer with no matte lets the owner default to Alpha; picking a mode freezes the adjacent source in the same write', async () => {
  const h = harness();
  const bare = build(h, 'L-b');
  named(bare.node.children[2], 'motion-parent-pill').fire('click');
  h.menus[0].items.find((item) => item.label === '  E').action();
  await settle();
  assert.deepEqual(plain(h.sent[0].payload), { command: 'object.layer.compositing.edit', layerUid: 'L-b', edit: 'matteSource', source: 'L-e' });
  assert.equal(pillText(bare.node.children[2]), 'E  ·  Alpha');
  const fresh = harness();
  const row = build(fresh, 'L-c');
  named(row.node.children[2], 'motion-parent-pill').fire('click');
  fresh.menus[0].items.find((item) => item.label === '  Luminance').action();
  await settle();
  assert.deepEqual(plain(fresh.sent[0].payload), { command: 'object.layer.compositing.edit', layerUid: 'L-c', edit: 'matteMode', mode: 'luma', source: 'L-d' });
});

test('the plus adds the first layer not already in the stack as an Alpha additional matte, and says so when none is left', async () => {
  const h = harness();
  const rows = build(h, 'L-a');
  named(rows.node.children[2], 'motion-matte-add').fire('click');
  await settle();
  assert.deepEqual(plain(h.sent[0].payload), { command: 'object.layer.compositing.edit', layerUid: 'L-a', edit: 'matteMoreAdd', source: 'L-e', mode: 'alpha' });
  assert.equal(rows.node.children.length, 7, 'a third additional row appears');
  assert.equal(pillText(rows.node.children[5]), 'E  ·  Alpha');
  named(rows.node.children[2], 'motion-matte-add').fire('click');
  assert.equal(h.sent.length, 1, 'every other layer is in the stack: nothing is sent');
  assert.deepEqual(h.toasts, ['Plus aucun autre calque disponible comme matte']);
});

test('the additional matte menu is the same two lists plus Remove; each pick edits only that entry by index', async () => {
  const h = harness();
  const rows = build(h, 'L-a');
  named(rows.node.children[4], 'motion-parent-pill').fire('click', { clientX: 5, clientY: 6 });
  const items = h.menus[0].items;
  assert.deepEqual(labels(items), ['Calque de matte :', '  B', '  C', '  D  ✓', '  E', '--', 'Mode :', '  Alpha', '  Alpha (inversé)', '  Luminance', '  Luminance (inversée)  ✓', '--', 'Retirer le matte']);
  items.find((item) => item.label === '  E').action();
  await settle();
  assert.deepEqual(plain(h.sent[0].payload), { command: 'object.layer.compositing.edit', layerUid: 'L-a', edit: 'matteMoreSet', index: 1, source: 'L-e' });
  assert.equal(pillText(rows.node.children[4]), 'E  ·  Luminance (inversée)');
  named(rows.node.children[3], 'motion-parent-pill').fire('click');
  h.menus[1].items.find((item) => item.label === '  Luminance').action();
  await settle();
  assert.deepEqual(plain(h.sent[1].payload), { command: 'object.layer.compositing.edit', layerUid: 'L-a', edit: 'matteMoreSet', index: 0, mode: 'luma' });
  assert.equal(pillText(rows.node.children[3]), 'C  ·  Luminance');
  named(rows.node.children[3], 'motion-parent-pill').fire('click');
  h.menus[2].items.find((item) => item.label === 'Retirer le matte').action();
  await settle();
  assert.deepEqual(plain(h.sent[2].payload), { command: 'object.layer.compositing.edit', layerUid: 'L-a', edit: 'matteMoreRemove', index: 0 });
  assert.equal(rows.node.children.length, 5, 'one additional row left, and it is the former second');
  assert.equal(pillText(rows.node.children[3]), 'E  ·  Luminance (inversée)');
  assert.equal(pillText(rows.node.children[2]), 'B  ·  Luminance', 'the first matte is untouched');
});

test('a stale menu cannot commit: a changed matte, a moved layer and a removed layer', async () => {
  const h = harness();
  const rows = build(h, 'L-a');
  // The first matte changed under an open menu: nothing is sent, the user is told, the rows re-read.
  named(rows.node.children[2], 'motion-parent-pill').fire('click');
  const staleFirst = h.menus[0].items;
  h.pin.layers[0].matteSourceLayerUid = 'L-c';
  staleFirst.find((item) => item.label === '  D').action();
  await settle();
  assert.equal(h.sent.length, 0);
  assert.deepEqual(h.toasts, ['The matte changed; open the menu again.']);
  assert.equal(pillText(rows.node.children[2]), 'C  ·  Luminance');
  // An additional matte whose entry changed.
  named(rows.node.children[3], 'motion-parent-pill').fire('click');
  const staleExtra = h.menus[1].items;
  h.pin.layers[0].mattesMore.splice(0, 1);
  staleExtra.find((item) => item.label === 'Retirer le matte').action();
  await settle();
  assert.equal(h.sent.length, 0, 'the entry at that index is now another matte');
  // The layer moved in the stack: the write follows the layer's uid, never the old index.
  const moved = harness();
  const movedRows = build(moved, 'L-b');
  named(movedRows.node.children[2], 'motion-parent-pill').fire('click');
  const items = moved.menus[0].items;
  moved.pin.layers.unshift({ layerUid: 'L-new', name: 'New', kind: 'draw' });
  items.find((item) => item.label === '  E').action();
  await settle();
  assert.equal(moved.sent[0].payload.layerUid, 'L-b');
  // The layer is gone.
  const gone = harness();
  const goneRows = build(gone, 'L-b');
  named(goneRows.node.children[2], 'motion-parent-pill').fire('click');
  const goneItems = gone.menus[0].items;
  gone.pin.layers.splice(1, 1);
  goneItems.find((item) => item.label === '  E').action();
  await settle();
  assert.equal(gone.sent.length, 0);
  assert.equal(gone.toasts.length, 1);
});

test('a refused write toasts, leaves the stored matte alone and the pill keeps showing it', async () => {
  const h = harness({ refuse: 'invalid_request' });
  const rows = build(h, 'L-a');
  named(rows.node.children[2], 'motion-parent-pill').fire('click');
  h.menus[0].items.find((item) => item.label === '  D').action();
  await settle();
  assert.equal(h.sent.length, 1);
  assert.deepEqual(h.toasts, ['Native layer compositing command was refused.']);
  assert.equal(pillText(rows.node.children[2]), 'B  ·  Luminance');
  assert.equal(h.refreshed.length, 0);
});

test('inside an entered component the pill refuses with a toast and sends nothing; the legacy writers are never reached', () => {
  const h = harness();
  const rows = build(h, 'L-a', 'component:sym-1');
  assert.ok(rows, 'the rows still render');
  named(rows.node.children[2], 'motion-parent-pill').fire('click');
  assert.equal(h.menus.length, 0);
  assert.equal(h.sent.length, 0);
  assert.deepEqual(h.toasts, ['Matte is only available on scene layers.']);
  assert.equal(pillText(rows.node.children[2]), '—');
});

test('a disposed rows object paints nothing', async () => {
  const h = harness();
  const rows = build(h, 'L-a');
  named(rows.node.children[2], 'motion-parent-pill').fire('click');
  const items = h.menus[0].items;
  rows.dispose();
  items.find((item) => item.label === '  D').action();
  await settle();
  assert.equal(rows.node.children.length, 0);
});

test('the compositing adapter lists the additional mattes index-aligned with the stored list and never aliases the pinned rows', () => {
  const h = harness();
  const comp = Compositing.create({ info: () => h.pin, isActive: () => true, blocksLegacy: () => true });
  const list = comp.mattesMoreOf(0);
  assert.deepEqual(list, [{ uid: 'L-c', mode: 'alpha' }, { uid: 'L-d', mode: 'lumaInverted' }]);
  list[0].uid = 'x';
  assert.equal(h.pin.layers[0].mattesMore[0].uid, 'L-c');
  assert.deepEqual(comp.mattesMoreOf(1), []);
  assert.deepEqual(comp.mattesMoreOf(99), []);
});
