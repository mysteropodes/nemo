'use strict';
// #2256 Q2: the Path (Follow Path + Align) and Time (Link time) rows of the native Motion inspector, and the optional `align` member of
// command.document.object.layer.followPath.set. The real rows module, adapter, rig command and contract run against a fake native
// session; the legacy writers (setLayerFollowPath, setLayerTimeLink, pushUndo) THROW if the rows ever reach them.
const test = require('node:test');
const assert = require('node:assert/strict');
const Parent = require('../src/js/adapters/native-layer-parent.js');
const Contract = require('../src/js/adapters/native-object-rig-contract.js');
const Rows = require('../src/js/adapters/native-motion-layer-link-rows.js');
const LockMenu = require('../src/js/adapters/native-motion-key-lock-menu.js');

const PATH = 'command.document.object.layer.followPath.set';
const TIME = 'command.document.object.layer.timeLink.set';
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

function rowsOf() {
  return [
    { layerUid: 'L-a', name: 'A', kind: 'draw' },
    { layerUid: 'L-b', name: 'B', kind: 'draw', timeLinkUid: 'L-a' },
    { layerUid: 'L-c', name: 'C', kind: 'draw', followPathUid: 'L-a' },
    { layerUid: 'L-f', name: 'Folder', kind: 'folder' },
    { layerUid: 'L-n', name: 'Null', kind: 'null' },
    { layerUid: 'L-e', name: 'Effect', kind: 'effect' },
  ];
}
function harness(options = {}) {
  const sent = [], toasts = [], menus = [], refreshed = [];
  const pin = { instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 1, contentRevision: 4, documentSnapshotId: 'snap-4', layers: options.rows || rowsOf() };
  const apply = (p) => {
    const row = pin.layers.find((r) => r.layerUid === p.layerUid);
    if (p.command === 'object.layer.followPath.set') {
      if (p.targetLayerUid === null) { delete row.followPathUid; delete row.followPathAlign; return; }
      row.followPathUid = p.targetLayerUid;
      if (p.align === true) row.followPathAlign = true; else if (p.align === false) delete row.followPathAlign;
    } else if (p.sourceLayerUid === null) delete row.timeLinkUid; else row.timeLinkUid = p.sourceLayerUid;
  };
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, requestId: (label) => label + '-' + (sent.length + 1),
    runResourceCommand: (action) => action({
      async dispatch(request) {
        sent.push(request);
        const base = { apiVersion: 2, requestId: request.requestId, instanceId: pin.instanceId, documentId: pin.documentId };
        if (options.refuse) return Object.assign(base, { contentRevision: pin.contentRevision, ok: false, error: { code: options.refuse, message: 'refused' } });
        apply(request.payload); pin.contentRevision += 1;
        return Object.assign(base, { contentRevision: pin.contentRevision, ok: true, result: { applied: true, historyEntriesAdded: 1 } });
      },
      async reconcile() {},
    }) };
  // D37: the real session's editLayerPolicy refuses a stale pin before anything commits; this fake mirrors that contract.
  const policy = [];
  const editLayerPolicy = options.noPolicy ? undefined : async (payload, expected) => {
    policy.push({ payload: plain(payload), expected: plain(expected) });
    if (expected.contentRevision !== pin.contentRevision) throw new Error('Native layer policy intent is stale or invalid');
    const row = pin.layers.find((r) => r.layerUid === payload.layerUid);
    if (payload.value === null) delete row.keyLock; else row.keyLock = payload.value;
    pin.contentRevision += 1;
    return { ok: true, contentRevision: pin.contentRevision };
  };
  Object.defineProperty(session, 'editLayerPolicy', { get: () => editLayerPolicy, enumerable: true });
  const throws = (name) => () => { throw new Error('LEGACY ' + name); };
  const host = { document: { createElement: element }, state: { currentFrame: 9 }, NemoNativeObjectProject: { desktop: () => session },
    showToast: (m) => toasts.push(m), showContextMenu: (x, y, items) => menus.push({ x, y, items }), nativeCompositingRefresh: () => refreshed.push('refresh'),
    setLayerFollowPath: throws('setLayerFollowPath'), setLayerTimeLink: throws('setLayerTimeLink'), pushUndo: throws('pushUndo'), unlinkTimeLinkPreserveRange: throws('unlink'), setLayerKeyLock: throws('setLayerKeyLock') };
  return { host, sent, toasts, menus, refreshed, pin, policy, adapter: Parent.create(session) };
}

test('the contract admits an optional boolean align on followPath.set and nothing else new', () => {
  const ok = (edit) => { try { Contract.payload(PATH, Object.assign({ command: 'object.layer.followPath.set', contextId: 'scene-root', layerUid: 'L', targetLayerUid: 'P' }, edit)); return true; } catch (_) { return false; } };
  assert.equal(ok({}), true, 'omitted keeps the stored align');
  assert.equal(ok({ align: true }), true);
  assert.equal(ok({ align: false }), true);
  assert.equal(ok({ targetLayerUid: null, align: false }), true);
  for (const align of ['true', 1, null, {}]) assert.equal(ok({ align }), false, JSON.stringify(align));
  assert.equal(ok({ aligned: true }), false);
});

test('setFollowPath carries align only with a target and followPathAlignOf reads the pinned row', async () => {
  const h = harness();
  await h.adapter.setFollowPath(2, 'L-b', { align: true });
  assert.deepEqual(plain(h.sent[0].payload), { command: 'object.layer.followPath.set', contextId: 'scene-root', layerUid: 'L-c', targetLayerUid: 'L-b', align: true });
  await h.adapter.setFollowPath(2, 'L-b', { align: false });
  assert.equal(h.sent[1].payload.align, false);
  await h.adapter.setFollowPath(2, 'L-a');
  assert.ok(!('align' in h.sent[2].payload), 'omitted stays omitted');
  await h.adapter.setFollowPath(2, null, { align: true });
  assert.ok(!('align' in h.sent[3].payload), 'a clear carries no align');
  await assert.rejects(h.adapter.setFollowPath(99, 'L-a', { align: true }), { code: 'not_found' });
  assert.equal(h.sent.length, 4);
  const aligned = harness({ rows: [{ layerUid: 'L-a', name: 'A', kind: 'draw' }, { layerUid: 'L-b', name: 'B', kind: 'draw', followPathUid: 'L-a', followPathAlign: true }] });
  assert.equal(aligned.adapter.followPathAlignOf(1), true);
  assert.equal(aligned.adapter.followPathAlignOf(0), false);
});

test('followPathAlignSet flips the orient toggle of the stored path as one native edit and says why when it cannot', async () => {
  const h = harness();
  let changed = 0;
  assert.equal(await Parent.followPathAlignSet(h.host, h.adapter, 2, true, () => { changed += 1; }), true);
  assert.deepEqual(plain(h.sent[0].payload), { command: 'object.layer.followPath.set', contextId: 'scene-root', layerUid: 'L-c', targetLayerUid: 'L-a', align: true });
  assert.equal(h.sent[0].operation, PATH);
  assert.equal(changed, 1);
  assert.deepEqual(h.refreshed, ['refresh']);
  assert.equal(h.adapter.followPathAlignOf(2), true, 'the pinned row follows the confirmed edit');
  assert.equal(await Parent.followPathAlignSet(h.host, h.adapter, 2, false, () => {}), true);
  assert.equal(h.sent[1].payload.align, false);
  // no path followed: nothing to orient, nothing sent
  assert.equal(await Parent.followPathAlignSet(h.host, h.adapter, 0, true, () => {}), false);
  assert.equal(h.sent.length, 2);
  assert.match(h.toasts[0], /Follow Path/);
  const refused = harness({ refuse: 'invalid_request' });
  assert.equal(await Parent.followPathAlignSet(refused.host, refused.adapter, 2, true, () => { throw new Error('no refresh'); }), false);
  assert.match(refused.toasts[0], /Follow Path refused/);
  assert.deepEqual(refused.refreshed, []);
});

test('the Path and Time rows render the baseline pills, the Align button only beside a followed path', () => {
  const h = harness();
  const followed = Rows.build(h.host, { contextId: 'scene-root', layerUid: 'L-c' });
  const [path, time] = followed.node.children;
  assert.equal(path.children[0].textContent, 'Path');
  assert.equal(named(path, 'mp-label').textContent, 'A');
  assert.ok(named(path, 'motion-parent-pill') && !named(path, 'none'), 'a followed path is not dimmed');
  assert.equal(named(path, 'mp-align-btn').textContent, 'Align');
  assert.ok(!named(path, 'mp-align-btn').className.split(' ').includes('on'));
  assert.equal(time.children[0].textContent, 'Time');
  assert.equal(time.children[1].textContent, '—');
  assert.ok(named(time, 'none'));
  const linked = Rows.build(h.host, { contextId: 'scene-root', layerUid: 'L-b' });
  assert.equal(linked.node.children[1].children[1].textContent, 'A');
  assert.equal(named(linked.node.children[0], 'mp-label').textContent, '—');
  assert.equal(named(linked.node.children[0], 'mp-align-btn'), undefined, 'no path, no Align');
  const aligned = harness({ rows: [{ layerUid: 'L-a', name: 'A', kind: 'draw' }, { layerUid: 'L-b', name: 'B', kind: 'draw', followPathUid: 'L-a', followPathAlign: true }] });
  assert.ok(named(Rows.build(aligned.host, { contextId: 'scene-root', layerUid: 'L-b' }).node, 'mp-align-btn').className.split(' ').includes('on'));
});

test('the rows are hidden for folder, Null and effect layers, for an element and for an unknown layer', () => {
  const h = harness();
  for (const uid of ['L-f', 'L-n', 'L-e', 'nope']) assert.equal(Rows.build(h.host, { contextId: 'scene-root', layerUid: uid }), null, uid);
  assert.equal(Rows.build(h.host, { contextId: 'scene-root', layerUid: 'L-a', strokeId: 's1' }), null);
  assert.equal(Rows.build({ document: h.host.document }, { contextId: 'scene-root', layerUid: 'L-a' }), null, 'no native project');
  assert.notEqual(Rows.build(h.host, { contextId: 'scene-root', layerUid: 'L-a' }), null);
});

test('the Path pill opens the native picker, a pick and the right-click clear are native edits, the pill repaints', async () => {
  const h = harness();
  const rows = Rows.build(h.host, { contextId: 'scene-root', layerUid: 'L-c' });
  const pill = named(rows.node.children[0], 'motion-parent-pill');
  pill.fire('click');
  assert.equal(h.menus.length, 1);
  assert.equal(h.menus[0].x, 10);
  assert.equal(h.menus[0].y, 31);
  const items = h.menus[0].items;
  assert.equal(items[0].label, 'Chemin : Aucun');
  assert.equal(items[0].disabled, false);
  items.find((item) => item.label === 'Chemin : B').action();
  await settle();
  assert.deepEqual(plain(h.sent[0].payload), { command: 'object.layer.followPath.set', contextId: 'scene-root', layerUid: 'L-c', targetLayerUid: 'L-b' });
  assert.equal(named(rows.node.children[0], 'mp-label').textContent, 'B', 'the row re-reads the confirmed rows');
  named(rows.node.children[0], 'motion-parent-pill').fire('contextmenu');
  await settle();
  assert.deepEqual(plain(h.sent[1].payload), { command: 'object.layer.followPath.set', contextId: 'scene-root', layerUid: 'L-c', targetLayerUid: null });
  assert.equal(named(rows.node.children[0], 'mp-label').textContent, '—');
  assert.equal(named(rows.node.children[0], 'mp-align-btn'), undefined);
  named(rows.node.children[0], 'motion-parent-pill').fire('contextmenu');
  await settle();
  assert.equal(h.sent.length, 2, 'nothing to clear');
  assert.deepEqual(h.toasts, []);
});

test('the Align button toggles the orient flag through the native edit and repaints its state', async () => {
  const h = harness();
  const rows = Rows.build(h.host, { contextId: 'scene-root', layerUid: 'L-c' });
  named(rows.node, 'mp-align-btn').fire('click');
  await settle();
  assert.deepEqual(plain(h.sent[0].payload), { command: 'object.layer.followPath.set', contextId: 'scene-root', layerUid: 'L-c', targetLayerUid: 'L-a', align: true });
  assert.ok(named(rows.node, 'mp-align-btn').className.split(' ').includes('on'));
  named(rows.node, 'mp-align-btn').fire('click');
  await settle();
  assert.equal(h.sent[1].payload.align, false);
  assert.ok(!named(rows.node, 'mp-align-btn').className.split(' ').includes('on'));
  assert.equal(h.refreshed.length, 2);
  const refused = harness({ refuse: 'unavailable' });
  const failing = Rows.build(refused.host, { contextId: 'scene-root', layerUid: 'L-c' });
  named(failing.node, 'mp-align-btn').fire('click');
  await settle();
  assert.match(refused.toasts[0], /Follow Path refused/);
  assert.ok(!named(failing.node, 'mp-align-btn').className.split(' ').includes('on'), 'a refusal leaves the button as it was');
});

test('the Time pill opens the Link time picker and the right-click is Time: None, both native edits', async () => {
  const h = harness();
  const rows = Rows.build(h.host, { contextId: 'scene-root', layerUid: 'L-b' });
  const pill = rows.node.children[1].children[1];
  pill.fire('click');
  const items = h.menus[0].items;
  assert.equal(items[0].label, 'Time: None (unlink, position kept)');
  assert.equal(items[0].disabled, false, 'B is linked');
  items.find((item) => item.label === 'Link time to: C').action();
  await settle();
  assert.deepEqual(plain(h.sent[0].payload), { command: 'object.layer.timeLink.set', contextId: 'scene-root', layerUid: 'L-b', sourceLayerUid: 'L-c', frame: 9, mode: 'both' });
  assert.equal(h.sent[0].operation, TIME);
  assert.equal(rows.node.children[1].children[1].textContent, 'C');
  rows.node.children[1].children[1].fire('contextmenu');
  await settle();
  assert.deepEqual(plain(h.sent[1].payload), { command: 'object.layer.timeLink.set', contextId: 'scene-root', layerUid: 'L-b', sourceLayerUid: null, frame: 9 });
  assert.equal(rows.node.children[1].children[1].textContent, '—');
  rows.node.children[1].children[1].fire('contextmenu');
  await settle();
  assert.equal(h.sent.length, 2, 'an unlinked layer has nothing to clear');
  assert.equal(h.toasts.at(-1), 'Lien temporel retiré');
});

test('inside an entered component the rows show and use refuses with the owner invalid_request text, sending nothing', async () => {
  const h = harness();
  const rows = Rows.build(h.host, { contextId: 'component:sym-1', layerUid: 'inner-1' });
  assert.ok(rows, 'the rows are shown');
  assert.equal(rows.node.children[0].children[1].children[0].textContent, '—');
  named(rows.node.children[0], 'motion-parent-pill').fire('click');
  named(rows.node.children[0], 'motion-parent-pill').fire('contextmenu');
  rows.node.children[1].children[1].fire('click');
  rows.node.children[1].children[1].fire('contextmenu');
  await settle();
  assert.equal(h.sent.length, 0);
  assert.equal(h.menus.length, 0);
  assert.equal(h.toasts.length, 4);
  assert.match(h.toasts[0], /Follow Path refused/);
  assert.match(h.toasts[2], /Link time refused.*scene layers/);
});

test('a disposed rows object paints nothing and a layer that became a folder empties the rows', () => {
  const h = harness();
  const rows = Rows.build(h.host, { contextId: 'scene-root', layerUid: 'L-c' });
  h.pin.layers[2].kind = 'folder';
  assert.equal(rows.refresh(), false);
  assert.equal(rows.node.children.length, 0);
  h.pin.layers[2].kind = 'draw';
  assert.equal(rows.refresh(), true);
  assert.equal(rows.node.children.length, 3);
  rows.dispose();
  assert.equal(rows.refresh(), false);
  assert.equal(rows.node.children.length, 0);
});

// ---- D37 / #2522: the layer key-lock row and its "Lock keyframes on:" menu ----
const lockPill = (rows) => rows.node.children[2].children[1];
const lockRows = () => [{ layerUid: 'L-a', name: 'A', kind: 'draw' }, { layerUid: 'L-b', name: 'B', kind: 'draw', keyLock: 'out' }, { layerUid: 'L-f', name: 'Folder', kind: 'folder' }];

test('the lock row shows the stored mode, the baseline menu order and the check on the active mode', () => {
  const h = harness({ rows: lockRows() });
  const none = Rows.build(h.host, { contextId: 'scene-root', layerUid: 'L-a' });
  assert.deepEqual(none.node.children.map((row) => row.children[0].textContent), ['Path', 'Time', 'Lock keyframes on']);
  assert.equal(lockPill(none).textContent, '—');
  assert.ok(lockPill(none).className.split(' ').includes('none'));
  lockPill(none).fire('click');
  assert.deepEqual(h.menus[0].items.map((item) => item.label), ['Lock keyframes on:', '   • the in point', '   • the out point', '   • the whole layer']);
  assert.equal(h.menus[0].items[0].disabled, true, 'the header row only titles the group');
  const locked = Rows.build(h.host, { contextId: 'scene-root', layerUid: 'L-b' });
  assert.equal(lockPill(locked).textContent, 'out point');
  lockPill(locked).fire('click');
  assert.deepEqual(h.menus[1].items.map((item) => item.label), ['Lock keyframes on:', '   • the in point', '   • the out point  ✓', '   • the whole layer']);
  assert.equal(h.sent.length, 0, 'opening a menu writes nothing');
  assert.equal(h.policy.length, 0);
});

test('picking a mode writes it through the native layer policy; picking the active mode clears it', async () => {
  const h = harness({ rows: lockRows() });
  const rows = Rows.build(h.host, { contextId: 'scene-root', layerUid: 'L-a' });
  lockPill(rows).fire('click');
  assert.equal(await h.menus[0].items[1].action(), true);
  assert.deepEqual(h.policy[0].payload, { command: 'object.layer.policy.edit', layerUid: 'L-a', flag: 'keyLock', value: 'in' });
  assert.equal(h.policy[0].expected.contentRevision, 4, 'the pin the menu was built from');
  assert.equal(h.adapter.keyLockOf(0), 'in');
  assert.equal(lockPill(rows).textContent, 'in point', 'the row repaints from the confirmed project rows');
  lockPill(rows).fire('click');
  assert.match(h.menus[1].items[1].label, /✓$/);
  assert.equal(await h.menus[1].items[1].action(), true);
  assert.equal(h.policy[1].payload.value, null, 'the active row clears');
  assert.equal(h.adapter.keyLockOf(0), null);
  assert.equal(lockPill(rows).textContent, '—');
  lockPill(rows).fire('click');
  assert.equal(await h.menus[2].items[3].action(), true);
  assert.equal(h.policy[2].payload.value, 'layer');
  assert.equal(h.sent.length, 0, 'no rig command is involved');
  assert.deepEqual(h.toasts, []);
});

test('a menu built before the project changed cannot commit, and an unavailable policy route is a toast', async () => {
  const h = harness({ rows: lockRows() });
  const rows = Rows.build(h.host, { contextId: 'scene-root', layerUid: 'L-a' });
  lockPill(rows).fire('click');
  h.pin.contentRevision += 1; // another edit lands while the menu is open
  assert.equal(await h.menus[0].items[2].action(), false);
  assert.equal(h.adapter.keyLockOf(0), null, 'the stale pick published nothing');
  assert.match(h.toasts[0], /Key lock was refused or the project changed/);
  const off = harness({ rows: lockRows(), noPolicy: true });
  const offRows = Rows.build(off.host, { contextId: 'scene-root', layerUid: 'L-a' });
  lockPill(offRows).fire('click');
  assert.equal(await off.menus[0].items[1].action(), false);
  assert.equal(off.policy.length, 0);
  assert.equal(off.toasts.length, 1, 'no legacy setLayerKeyLock fallback');
});

test('the lock menu builder is the single source of labels and clears the active mode', () => {
  const picks = [];
  const items = LockMenu.items({}, (m) => m === 'layer', (next) => picks.push(next));
  assert.deepEqual(items.map((i) => i.label), ['Lock keyframes on:', '   • the in point', '   • the out point', '   • the whole layer  ✓']);
  items[1].action(); items[3].action();
  assert.deepEqual(picks, ['in', null]);
  const translated = LockMenu.items({ SM: { t: (key) => ({ ctxLockKeyframesOnColon: 'Verrouiller :', ctxKeyLockInPoint: '   • entrée' })[key] || key } }, () => false, () => {});
  assert.deepEqual(translated.map((i) => i.label), ['Verrouiller :', '   • entrée', '   • the out point', '   • the whole layer']);
});
