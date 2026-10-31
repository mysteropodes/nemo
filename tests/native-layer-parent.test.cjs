'use strict';
// #2372 U12: layer Parent A under native authority. The real adapter, rig command and contract run against a fake session;
// the real timeline.js menu builders and pickwhip run in a vm whose legacy layer rows THROW on any write and whose pushUndo
// throws. A pick is exactly one native command with the playhead frame, a refusal says why, Parent B reports unavailable and
// Link time / Follow path (#2372) are native edits of their own, none of them ever reaching the legacy layer model.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const Parent = require('../src/js/adapters/native-layer-parent.js');
const Contract = require('../src/js/adapters/native-object-rig-contract.js');

const source = (name) => fs.readFileSync(path.join(__dirname, '../src/js', name), 'utf8');
const plain = (value) => JSON.parse(JSON.stringify(value));
const OP = 'command.document.object.layer.parent.set';
const TIME = 'command.document.object.layer.timeLink.set';
const PATH = 'command.document.object.layer.followPath.set';

function rows() {
  return [
    { layerUid: 'L-a', name: 'A', kind: 'draw' },
    { layerUid: 'L-b', name: 'B', kind: 'draw', parentLayerUid: 'L-a', timeLinkUid: 'L-a' },
    { layerUid: 'L-c', name: 'C', kind: 'draw', parentLayerUid: 'L-b', followPathUid: 'L-a' },
    { layerUid: 'L-f', name: 'Folder', kind: 'folder' },
    { layerUid: 'L-n', name: 'Null', kind: 'null' },
  ];
}
function harness(options = {}) {
  const sent = [], pin = { instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 1, contentRevision: 4, documentSnapshotId: 'snap-4', layers: rows() };
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, requestId: (label) => label + '-' + (sent.length + 1),
    runResourceCommand: (action) => action({
      async dispatch(request) {
        sent.push(request);
        const base = { apiVersion: 2, requestId: request.requestId, instanceId: pin.instanceId, documentId: pin.documentId };
        if (options.refuse) return Object.assign(base, { contentRevision: pin.contentRevision, ok: false, error: { code: options.refuse, message: 'refused' } });
        return Object.assign(base, { contentRevision: pin.contentRevision + 1, ok: true, result: { applied: true, historyEntriesAdded: 1 } });
      },
      async reconcile() {},
    }) };
  return { session, sent, pin, adapter: Parent.create(session) };
}
const settle = async () => { for (let i = 0; i < 14; i += 1) await new Promise((resolve) => setImmediate(resolve)); };

test('the contract admits the parent set as one closed shape with an explicit null and refuses everything else locally', () => {
  assert.equal(Contract.admitted(OP), true);
  assert.equal(Contract.requiresRevision(OP), true);
  const base = { command: 'object.layer.parent.set', contextId: 'scene-root', layerUid: 'L', parentLayerUid: 'P', frame: 3 };
  const ok = (edit) => { const payload = Object.assign({}, base, edit); try { Contract.payload(OP, payload); return true; } catch (_) { return false; } };
  assert.equal(ok({}), true);
  assert.equal(ok({ parentLayerUid: null }), true, 'Parent A : None');
  assert.equal(ok({ parentLayerUid: '' }), false);
  assert.equal(ok({ parentLayerUid: 7 }), false);
  assert.equal(ok({ frame: -1 }), false);
  assert.equal(ok({ frame: 1.5 }), false);
  assert.equal(ok({ extra: 1 }), false);
  assert.throws(() => { const payload = Object.assign({}, base); delete payload.parentLayerUid; Contract.payload(OP, payload); }, /parentLayerUid/);
});

test('setParent is exactly one native command built from the pinned rows at execution time', async () => {
  const h = harness();
  const receipt = await h.adapter.setParent(4, 'L-a', 7);
  assert.equal(h.sent.length, 1);
  assert.equal(h.sent[0].operation, OP);
  assert.equal(h.sent[0].expectedRevision, 4);
  assert.deepEqual(plain(h.sent[0].payload), { command: 'object.layer.parent.set', contextId: 'scene-root', layerUid: 'L-n', parentLayerUid: 'L-a', frame: 7 });
  assert.equal(receipt.applied, true);
  await h.adapter.setParent(2, null, 0);
  assert.deepEqual(plain(h.sent[1].payload), { command: 'object.layer.parent.set', contextId: 'scene-root', layerUid: 'L-c', parentLayerUid: null, frame: 0 });
  await assert.rejects(h.adapter.setParent(99, 'L-a', 0), { code: 'not_found' });
  await assert.rejects(h.adapter.setParent(1, 'L-a', -2), { code: 'invalid_request' });
  assert.equal(h.sent.length, 2, 'a locally refused call sends nothing');
  assert.deepEqual([...h.adapter.descendants(0)], [1, 2], 'the chain through a layer is derived from the rows');
});

// ---- the real timeline.js menus and pickwhip ----
function timelineHarness(options = {}) {
  const h = harness(options), toasts = [], renders = [], refreshed = [];
  const trap = (row, name) => new Proxy(Object.assign({ name }, row), { set() { throw Error('legacy layer row written'); }, deleteProperty() { throw Error('legacy layer row written'); }, defineProperty() { throw Error('legacy layer row written'); } });
  const state = { activeLayerIdx: 1, currentFrame: 9, layers: h.pin.layers.map((row) => trap(row, row.name)) };
  const tripped = (name) => () => { throw Error(name + ' reached'); };
  const listeners = {};
  const pickRow = { dataset: { layer: '0' }, classList: { add() {}, remove() {} } };
  const context = { state, console, SM: { t: (k) => k }, showToast: (m) => toasts.push(m), pushUndo: tripped('pushUndo'),
    renderLayerList: () => renders.push('list'), renderTimeline: () => renders.push('timeline'), nativeCompositingRefresh: () => refreshed.push('refresh'),
    parentDescendants: () => ({}), SMMotion: { setLayerParent: tripped('legacy setLayerParent'), ensureLayerUid: tripped('ensureLayerUid') },
    SMEngineBridge: { renderNow: tripped('renderNow') },
    NemoNativeObjectProject: { desktop: () => h.session }, NemoNativeLayerParent: Parent,
    document: { createElement: () => ({ style: {}, remove() {} }), body: { appendChild() {} },
      addEventListener: (kind, fn) => { listeners[kind] = fn; }, removeEventListener: (kind) => { delete listeners[kind]; },
      elementFromPoint: () => ({ closest: () => pickRow }) } };
  context.window = context;
  const text = source('timeline.js');
  const from = text.indexOf('function startParentPickwhip(li,fromEl,ev){');
  const to = text.indexOf('window.buildTimeLinkMenuItems=buildTimeLinkMenuItems;');
  assert.ok(from > 0 && to > from, 'the menu block is locatable');
  vm.runInNewContext(text.slice(from, to), context, { filename: 'timeline.js' });
  return Object.assign(h, { toasts, renders, refreshed, state, context, listeners });
}

test('the Parent A menu on native: None and each layer are native edits at the playhead, folders and descendants are disabled', async () => {
  const t = timelineHarness();
  let changed = 0;
  const items = t.context.buildParentMenuItems(0, t.state.layers[0], () => { changed += 1; });
  assert.equal(items[0].label, 'Parent A : Aucun (parentage libre)');
  assert.equal(items[0].disabled, true, 'layer 0 has no parent to clear');
  const pick = (name) => items.find((item) => item.label && item.label.startsWith('Parent A : ' + name));
  assert.equal(pick('B').disabled, true, 'B descends from A');
  assert.match(pick('B').label, /descendant/);
  assert.equal(pick('Folder').disabled, true, 'a folder is never a parent');
  assert.equal(pick('Null').disabled, false);
  assert.ok(!items.some((item) => item.label === 'Parent A : A'), 'a layer is not listed as its own parent');
  assert.ok(items.some((item) => /Parent B/.test(item.label || '') && item.disabled === true), 'Parent B reports unavailable');
  pick('Null').action();
  await settle();
  assert.equal(t.sent.length, 1, 'exactly one native command');
  assert.deepEqual(plain(t.sent[0].payload), { command: 'object.layer.parent.set', contextId: 'scene-root', layerUid: 'L-a', parentLayerUid: 'L-n', frame: 9 });
  assert.equal(changed, 1, 'the caller refresh runs');
  assert.deepEqual(t.refreshed, ['refresh'], 'the panels refresh from the confirmed revision');
  assert.deepEqual(t.toasts, []);
  const clear = t.context.buildParentMenuItems(2, t.state.layers[2], () => {});
  assert.equal(clear[0].disabled, false, 'layer 2 has a parent');
  clear[0].action();
  await settle();
  assert.deepEqual(plain(t.sent[1].payload), { command: 'object.layer.parent.set', contextId: 'scene-root', layerUid: 'L-c', parentLayerUid: null, frame: 9 });
});

test('a refused Parent A pick says why and refreshes nothing', async () => {
  for (const [code, pattern] of [['unavailable', /kept in place natively/], ['invalid_request', /cycle/], ['not_found', /no longer/], ['stale_revision', /refused/]]) {
    const t = timelineHarness({ refuse: code });
    let changed = 0;
    t.context.buildParentMenuItems(0, t.state.layers[0], () => { changed += 1; }).find((item) => /Null/.test(item.label || '')).action();
    await settle();
    assert.equal(t.sent.length, 1, code);
    assert.equal(t.toasts.length, 1, code);
    assert.match(t.toasts[0], pattern, code);
    assert.equal(changed, 0, code);
    assert.deepEqual(t.refreshed, [], code);
  }
});

test('the contract admits Link time and Follow path as closed shapes with an explicit null and refuses everything else locally', () => {
  for (const op of [TIME, PATH]) { assert.equal(Contract.admitted(op), true); assert.equal(Contract.requiresRevision(op), true); }
  const ok = (op, payload) => { try { Contract.payload(op, payload); return true; } catch (_) { return false; } };
  const link = (edit) => Object.assign({ command: 'object.layer.timeLink.set', contextId: 'scene-root', layerUid: 'L', sourceLayerUid: 'S', frame: 3 }, edit);
  assert.equal(ok(TIME, link({})), true);
  assert.equal(ok(TIME, link({ sourceLayerUid: null })), true, 'Time: None');
  assert.equal(ok(TIME, link({ mode: 'in', srcAnchor: 'out' })), true);
  for (const edit of [{ sourceLayerUid: '' }, { mode: 'sideways' }, { srcAnchor: 'both' }, { frame: -1 }, { frame: 1.5 }, { extra: 1 }, { command: 'object.layer.followPath.set' }])
    assert.equal(ok(TIME, link(edit)), false, JSON.stringify(edit));
  assert.throws(() => { const payload = link({}); delete payload.sourceLayerUid; Contract.payload(TIME, payload); }, /sourceLayerUid/);
  const follow = (edit) => Object.assign({ command: 'object.layer.followPath.set', contextId: 'scene-root', layerUid: 'L', targetLayerUid: 'P' }, edit);
  assert.equal(ok(PATH, follow({})), true);
  assert.equal(ok(PATH, follow({ targetLayerUid: null })), true, 'Chemin : Aucun');
  for (const edit of [{ targetLayerUid: '' }, { targetLayerUid: 7 }, { frame: 0 }, { extra: 1 }]) assert.equal(ok(PATH, follow(edit)), false, JSON.stringify(edit));
});

test('setTimeLink and setFollowPath are exactly one native command built from the pinned rows at execution time', async () => {
  const h = harness();
  await h.adapter.setTimeLink(4, 'L-a', 7, { mode: 'in', srcAnchor: 'out' });
  assert.equal(h.sent[0].operation, TIME);
  assert.equal(h.sent[0].expectedRevision, 4);
  assert.deepEqual(plain(h.sent[0].payload), { command: 'object.layer.timeLink.set', contextId: 'scene-root', layerUid: 'L-n', sourceLayerUid: 'L-a', frame: 7, mode: 'in', srcAnchor: 'out' });
  await h.adapter.setTimeLink(1, null, 2, { mode: 'in' });
  assert.deepEqual(plain(h.sent[1].payload), { command: 'object.layer.timeLink.set', contextId: 'scene-root', layerUid: 'L-b', sourceLayerUid: null, frame: 2 }, 'Time: None carries no mode');
  await h.adapter.setFollowPath(2, 'L-a');
  assert.equal(h.sent[2].operation, PATH);
  assert.deepEqual(plain(h.sent[2].payload), { command: 'object.layer.followPath.set', contextId: 'scene-root', layerUid: 'L-c', targetLayerUid: 'L-a' });
  await h.adapter.setFollowPath(2, null);
  assert.equal(h.sent[3].payload.targetLayerUid, null);
  await assert.rejects(h.adapter.setTimeLink(99, 'L-a', 0), { code: 'not_found' });
  await assert.rejects(h.adapter.setTimeLink(1, 'L-a', -1), { code: 'invalid_request' });
  await assert.rejects(h.adapter.setFollowPath(99, 'L-a'), { code: 'not_found' });
  assert.equal(h.sent.length, 4, 'a locally refused call sends nothing');
  assert.equal(h.adapter.timeLinkUidOf(1), 'L-a');
  assert.equal(h.adapter.timeLinkUidOf(0), null);
  assert.equal(h.adapter.followPathUidOf(2), 'L-a');
  // time links only: B follows A in time, so A cannot follow B; the parent chain is irrelevant here
  assert.equal(h.adapter.timeLinkWouldCycle(0, 1), true);
  assert.equal(h.adapter.timeLinkWouldCycle(1, 0), false);
  assert.equal(h.adapter.timeLinkWouldCycle(0, 2), false);
});

test('the Link time menu on native: Time: None and each layer are native edits, the stored source is ticked and a cycle is disabled', async () => {
  const t = timelineHarness();
  let changed = 0;
  const items = t.context.buildTimeLinkMenuItems(1, t.state.layers[1], () => { changed += 1; });
  assert.equal(items[0].label, 'Time: None (unlink, position kept)');
  assert.equal(items[0].disabled, false, 'layer 1 is linked');
  const pick = (name) => items.find((item) => item.label && item.label.startsWith('Link time to: ' + name));
  assert.match(pick('A').label, /✓/);
  assert.equal(pick('A').disabled, true, 'the stored source');
  assert.ok(!items.some((item) => item.label === 'Link time to: B'), 'a layer is not listed as its own source');
  assert.equal(pick('C').disabled, false);
  pick('C').action();
  await settle();
  assert.equal(t.sent.length, 1, 'exactly one native command');
  assert.deepEqual(plain(t.sent[0].payload), { command: 'object.layer.timeLink.set', contextId: 'scene-root', layerUid: 'L-b', sourceLayerUid: 'L-c', frame: 9, mode: 'both' });
  assert.equal(changed, 1);
  assert.deepEqual(t.refreshed, ['refresh']);
  assert.deepEqual(t.toasts, ['Temps lié à « C »']);
  items[0].action();
  await settle();
  assert.deepEqual(plain(t.sent[1].payload), { command: 'object.layer.timeLink.set', contextId: 'scene-root', layerUid: 'L-b', sourceLayerUid: null, frame: 9 });
  assert.equal(t.toasts[1], 'Lien temporel retiré');
  const root = t.context.buildTimeLinkMenuItems(0, t.state.layers[0], () => {});
  assert.equal(root[0].disabled, true, 'layer 0 has no link to clear');
  const cyc = root.find((item) => /Link time to: B/.test(item.label || ''));
  assert.match(cyc.label, /\(cycle\)/);
  assert.equal(cyc.disabled, true, 'B follows A in time, so A cannot follow B');
  assert.equal(t.sent.length, 2);
});

test('a refused Link time or Follow path pick says why and refreshes nothing', async () => {
  for (const [code, pattern] of [['unavailable', /resolved exactly/], ['invalid_request', /cycle/], ['not_found', /no longer/], ['stale_revision', /refused/]]) {
    const t = timelineHarness({ refuse: code });
    let changed = 0;
    t.context.buildTimeLinkMenuItems(2, t.state.layers[2], () => { changed += 1; }).find((item) => /Link time to: Null/.test(item.label || '')).action();
    await settle();
    assert.equal(t.sent.length, 1, code);
    assert.match(t.toasts[0], pattern, code);
    assert.equal(changed, 0, code);
    assert.deepEqual(t.refreshed, [], code);
  }
  const t = timelineHarness({ refuse: 'invalid_request' });
  t.context.buildFollowPathMenuItems(0, t.state.layers[0], () => {}).find((item) => item.label === 'Chemin : B').action();
  await settle();
  assert.match(t.toasts[0], /Follow Path refused.*Null or effect/);
});

test('the Follow Path menu on native: only a layer that can carry a path is a target, and None is a native edit', async () => {
  const t = timelineHarness();
  let changed = 0;
  const items = t.context.buildFollowPathMenuItems(2, t.state.layers[2], () => { changed += 1; });
  assert.equal(items[0].label, 'Chemin : Aucun');
  assert.equal(items[0].disabled, false, 'layer 2 follows A');
  const pick = (name) => items.slice(1).find((item) => item.label && item.label.startsWith('Chemin : ' + name));
  assert.equal(pick('A').disabled, true, 'the stored target');
  assert.equal(pick('Folder').disabled, true);
  assert.equal(pick('Null').disabled, true);
  assert.match(pick('Null').label, /sans tracé/);
  assert.ok(!items.some((item) => item.label === 'Chemin : C'), 'a layer is not listed as its own path');
  pick('B').action();
  await settle();
  assert.deepEqual(plain(t.sent[0].payload), { command: 'object.layer.followPath.set', contextId: 'scene-root', layerUid: 'L-c', targetLayerUid: 'L-b' });
  assert.equal(changed, 1);
  items[0].action();
  await settle();
  assert.deepEqual(plain(t.sent[1].payload), { command: 'object.layer.followPath.set', contextId: 'scene-root', layerUid: 'L-c', targetLayerUid: null });
  assert.deepEqual(t.refreshed, ['refresh', 'refresh']);
  assert.deepEqual(t.toasts, []);
  const free = t.context.buildFollowPathMenuItems(0, t.state.layers[0], () => {});
  assert.equal(free[0].disabled, true, 'nothing to clear on layer 0');
});

test('the pickwhip drop on native is one native Parent A edit and never the legacy setter', async () => {
  const t = timelineHarness();
  const fromEl = { getBoundingClientRect: () => ({ left: 0, top: 0, width: 4, height: 4 }) };
  t.context.startParentPickwhip(2, fromEl, { stopPropagation() {}, preventDefault() {}, clientX: 1, clientY: 1 });
  assert.equal(typeof t.listeners.mouseup, 'function', 'the drag is armed');
  t.listeners.mouseup({ clientX: 30, clientY: 30 });
  await settle();
  assert.equal(t.sent.length, 1);
  assert.deepEqual(plain(t.sent[0].payload), { command: 'object.layer.parent.set', contextId: 'scene-root', layerUid: 'L-c', parentLayerUid: 'L-a', frame: 9 });
  assert.deepEqual(t.renders.slice(0, 2), ['list', 'timeline']);
});

test('the legacy setters of motion.js refuse Parent A/B on a native project, and Link time / Follow path route to the native adapter', () => {
  const text = source('motion.js');
  const body = (name) => { const at = text.indexOf('  function ' + name + '('); assert.ok(at > 0, name); return text.slice(at, text.indexOf('\n  }\n', at) + 4); };
  for (const [name, args, notice] of [['setLayerParent', '0, "x"', 'Parent A'], ['setLayerParentB', '0, "x"', 'Parent B']]) {
    for (const native of [true, false]) {
      const toasts = [], rows = [new Proxy({}, { set() { throw Error('legacy layer row written'); } }), {}];
      const context = { showToast: (m) => toasts.push(m), state: { layers: rows }, nativeLayerParent: native ? () => ({}) : null, SM: { t: (k) => k } };
      context.window = context;
      vm.runInNewContext(body(name) + '\ntry { this.result = ' + name + '(' + args + '); } catch (error) { this.threw = error.message; }', context);
      if (native) {
        assert.deepEqual(toasts, [notice + ' is not available on a native project yet.'], name);
        assert.equal(context.threw, undefined, name + ' wrote nothing');
      } else {
        assert.notDeepEqual(toasts, [notice + ' is not available on a native project yet.'], name + ' keeps the legacy path off a native project');
        assert.notEqual(context.threw || 'returned', undefined);
      }
    }
  }
  for (const [name, args, expected] of [['setLayerFollowPath', '0, "L-x"', ['followPathSet', 0, 'L-x']], ['setLayerFollowPath', '0, null', ['followPathSet', 0, null]],
    ['setLayerTimeLink', '0, 1', ['timeLinkSet', 0, 1, 'both', undefined]], ['setLayerTimeLink', '0, 1, "in", "out"', ['timeLinkSet', 0, 1, 'in', 'out']]]) {
    const toasts = [], calls = [], redraws = [], rows = [new Proxy({}, { set() { throw Error('legacy layer row written'); } }), new Proxy({}, { set() { throw Error('legacy layer row written'); } })];
    const api = { followPathSet: (...a) => { calls.push(['followPathSet', ...a.slice(2, 4)]); a[a.length - 1](); }, timeLinkSet: (...a) => { calls.push(['timeLinkSet', ...a.slice(2, 6)]); a[a.length - 1](); } };
    const context = { showToast: (m) => toasts.push(m), state: { layers: rows }, nativeLayerParent: () => ({ api, adapter: {} }), SM: { t: (k) => k },
      renderLayerList: () => redraws.push('list'), renderTimeline: () => redraws.push('timeline'), pushUndo: () => { throw Error('pushUndo reached'); } };
    context.window = context;
    vm.runInNewContext(body(name) + '\ntry { this.result = ' + name + '(' + args + '); } catch (error) { this.threw = error.message; }', context);
    assert.equal(context.threw, undefined, name + ' wrote nothing and undid nothing');
    assert.deepEqual(calls, [expected], name);
    assert.deepEqual(redraws, ['list', 'timeline'], name + ' redraws from the confirmed edit');
    assert.deepEqual(toasts, [], name);
  }
});
