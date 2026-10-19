'use strict';
// L05-K: dense-editor frame actions -> native frame-edit commands. The targeting of every action is compared with the
// VERBATIM baseline functions (fixture generated at fbe4b8a3 by tools/gen_intent_oracle.js): which layers, cells and
// frames the baseline actually changed. The session here is a port stand-in; the real session, SDK and contract run
// together in native-frame-edit-session.test.cjs and the real owners are exercised by the Rust leaves L05-C/D/E/I/J1.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const SDK = require('../src/js/adapters/native-application.js');
const Intent = require('../src/js/adapters/native-frame-edit-intent.js');
const oracle = JSON.parse(fs.readFileSync(path.join(__dirname, 'fixtures', 'native-frame-edit-intent', 'oracle.json'), 'utf8'));
const P = 'command.document.object.';
const uid = (index) => 'layer-uid-' + index;
const sorted = (list) => list.slice().sort((a, b) => a - b);
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

function pins(info) {
  return { instanceId: info.instanceId, documentId: info.documentId, lifecycleGeneration: info.lifecycleGeneration, contentRevision: info.contentRevision, documentSnapshotId: info.documentSnapshotId };
}
function lengthAfter(state, op, payload) {
  if (op === P + 'frames.insert' || op === P + 'frames.duplicate') return state.total + 1;
  if (op === P + 'frames.extend') return state.total + payload.count;
  if (op === P + 'frames.remove') return state.total - (payload.to - payload.from + 1);
  return state.total;
}
// A pinned, presented session whose editFrames answers like the shared receipt: default is an applied edit.
function rig(o) {
  o = o || {};
  const n = o.layers || 3;
  const s = { rev: 7, total: o.total || 10, frame: o.frame === undefined ? 2 : o.frame, active: o.active || 0, layerSel: o.layerSel || [], cells: o.cells || [],
    uids: Array.from({ length: n }, (_, i) => uid(i)), writes: [], presented: null };
  s.presented = s.frame;
  const info = () => ({ instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 1, contentRevision: s.rev,
    documentSnapshotId: 'native-object:document-A:' + s.rev, totalFrames: s.total, layers: s.uids.map((layerUid) => ({ layerUid })) });
  const receipt = (applied, op) => ({ apiVersion: 2, requestId: 'frame-edit-1', ok: true, instanceId: 'instance-A', documentId: 'document-A',
    contentRevision: s.rev, result: Object.assign({ applied, historyEntriesAdded: applied ? 1 : 0 }, op === P + 'frames.repeat' ? { cycledLayers: 1, skippedComponentLayers: 0 } : {}) });
  const session = {
    info,
    readPresentedObjectSession() {
      const i = info();
      return Object.assign({ available: true, presentation: Object.assign({ owner: 'native', status: 'presented', frame: s.presented }, pins(i)) }, i);
    },
    get editFrames() {
      if (o.noWrite) return undefined;
      return function (op, payload, expected) {
        s.writes.push({ op, payload: JSON.parse(JSON.stringify(payload)), expected });
        if (o.write) return o.write(s, op, payload, receipt);
        s.total = lengthAfter(s, op, payload); s.rev += 1;
        s.frame = Math.min(s.frame, s.total - 1); s.presented = s.frame;
        return receipt(true, op);
      };
    }
  };
  const ports = { currentSession: () => (o.noSession ? null : (s.replacement || session)), application: o.application || SDK, frame: () => s.frame,
    activeLayer: () => s.active, layerSelection: () => s.layerSel, cellSelection: () => s.cells, available: () => o.unavailable !== true };
  return { s, session, ports, intent: Intent.create(ports) };
}
async function runKind(r, action) {
  const result = await r.intent.run(action);
  return { result, write: r.s.writes[0] };
}

// ---- baseline targeting oracle -------------------------------------------------------------------------------
// Returns true when the request payload targets exactly what the baseline changed.
function verdict(row, payload) {
  const locked = (index) => row.locked[index] === true;
  const idx = (u) => Number(u.slice('layer-uid-'.length));
  switch (row.kind) {
    case 'insertFrame': {
      const targets = payload.layerUids ? payload.layerUids.map(idx) : row.locked.map((_, i) => i);
      const effective = sorted(targets.filter((i) => !locked(i)));
      if (payload.cell !== row.cf) return false;
      if ((row.layerSel.length > 0) !== (payload.layerUids !== undefined) || (payload.layerUids && !same(payload.layerUids.map(idx), row.layerSel))) return false;
      return row.aborted === (row.layerSel.length > 0 && effective.length === 0) && (row.aborted || same(effective, sorted(row.spliced)));
    }
    case 'insertKey': {
      const targets = payload.layerUids ? payload.layerUids.map(idx) : row.locked.map((_, i) => i);
      return same(payload.cells, [row.cf]) && same(sorted(targets.filter((i) => !locked(i))), sorted(row.converted));
    }
    case 'insertKeyAt':
      return same(payload.layerUids, [uid(row.args.layer)]) && same(payload.cells, [row.args.frame]) &&
        same(row.changed, [{ layer: row.args.layer, frame: row.args.frame }]);
    case 'convertToKeyframes': {
      if (!same(payload.layerUids, [uid(row.active)])) return false;
      const wasKey = (f) => row.grid[row.active][f] === 'K';
      return same(sorted(payload.cells.filter((f) => !wasKey(f))), sorted(row.converted)) &&
        (row.cells.length ? same(sorted(payload.cells), sorted(row.cells.filter((c) => c.layer === row.active).map((c) => c.frame))) : same(payload.cells, [row.cf]));
    }
    case 'clearKey': return payload.layerUid === uid(row.active) && payload.cell === row.cf && row.cleared === true;
    case 'duplicateKey': return payload.layerUid === uid(row.active) && payload.cell === row.cf && same(row.source, [row.active]);
    case 'extendExposure': return payload.cell === row.cf && payload.count === row.added;
    case 'removeFrame': return payload.from === row.cf && payload.to === row.cf && same(row.removed, [row.cf]);
    case 'removeFrameSpan': {
      const wanted = row.cells.length ? [Math.min(...row.cells.map((c) => c.frame)), Math.max(...row.cells.map((c) => c.frame))] : [row.cf, row.cf];
      const range = []; for (let f = payload.from; f <= payload.to; f++) range.push(f);
      return same([payload.from, payload.to], wanted) && (row.removed.length === 0 || same(range, row.removed));
    }
    case 'deleteFrames': {
      const got = payload.targets.filter((t) => !locked(idx(t.layerUid))).map((t) => idx(t.layerUid) + ':' + t.frame).sort();
      return same(got, row.cleared.map((c) => c.layer + ':' + c.frame).sort()) && payload.targets.length === row.cells.length;
    }
    default: return false;
  }
}
const KIND = { insertFrame: 'insertFrame', insertKey: 'insertKey', insertKeyAt: 'insertKeyAt', convertToKeyframes: 'convertToKeyframes', clearKey: 'clearKey',
  duplicateKey: 'duplicateKey', extendExposure: 'extendExposure', removeFrame: 'removeFrame', removeFrameSpan: 'removeFrameSpan', deleteFrames: 'deleteFrames' };
const ACTIONS = { insertFrame: {}, insertKey: {}, insertKeyAt: { layer: 1, frame: 3 }, convertToKeyframes: {}, insertBlankKey: {}, duplicateKey: {}, extendExposure: { count: 2 },
  clearKey: {}, deleteFrames: {}, removeTween: { layer: 1, from: 2, to: 4 }, removeFrame: {}, removeFrameSpan: {}, moveKey: { layer: 1, from: 2, to: 4 },
  duplicateSelection: {}, repeatSelection: { times: 2 }, copyFrames: {}, cutFrames: {}, pasteFrames: {} };
const act = (kind) => Object.assign({ kind }, ACTIONS[kind]);
function rigFor(row) {
  return rig({ layers: row.grid.length, total: row.total, frame: row.cf, active: row.active, layerSel: row.layerSel, cells: row.cells });
}
function actionFor(row) {
  const action = { kind: KIND[row.kind] };
  return Object.assign(action, row.args);
}

test('the oracle really spans the baseline behaviours it pins', () => {
  assert.equal(oracle.baseline, 'fbe4b8a33e6d5a229f014154df2ba21988a9fabc');
  assert.deepEqual(Object.keys(oracle.kinds).sort(), ['clearKey', 'convertToKeyframes', 'deleteFrames', 'duplicateKey', 'extendExposure', 'insertFrame', 'insertKey', 'insertKeyAt', 'removeFrame', 'removeFrameSpan', 'selBounds']);
  assert.ok(oracle.rows.filter((r) => r.kind === 'insertFrame' && r.aborted).length >= 5, 'all-locked explicit selections abort in the baseline');
  assert.ok(oracle.rows.filter((r) => r.kind === 'insertFrame' && r.layerSel.length && !r.aborted).length >= 20);
  assert.ok(oracle.rows.filter((r) => r.kind === 'convertToKeyframes' && r.noSelectionOnActive).length >= 5);
  assert.ok(oracle.rows.filter((r) => r.kind === 'removeFrameSpan' && r.cells.length && r.removed.length).length >= 20);
});

test('every baseline row is reproduced by the request payload the adapter builds', async () => {
  let compared = 0;
  for (const row of oracle.rows.filter((r) => KIND[r.kind])) {
    const r = rigFor(row);
    if (row.kind === 'convertToKeyframes' && row.noSelectionOnActive) {
      const out = await r.intent.run({ kind: 'convertToKeyframes' });
      assert.deepEqual([out.status, out.code], ['unavailable', 'frame_edit_selection_required']);
      assert.equal(r.s.writes.length, 0);
      continue;
    }
    const { write } = await runKind(r, actionFor(row));
    assert.ok(write, row.kind + ' reached the session');
    assert.equal(verdict(row, write.payload), true, JSON.stringify(row));
    compared++;
  }
  assert.ok(compared >= 400, 'compared ' + compared);
});

test('selection bounds of the baseline are the bounds of repeat and of duplicate selection', async () => {
  for (const row of oracle.rows.filter((r) => r.kind === 'selBounds')) {
    const r = rigFor(row);
    const repeat = await r.intent.run({ kind: 'repeatSelection', times: 3 });
    const dup = await r.intent.run({ kind: 'duplicateSelection' });
    if (row.bounds === null) {
      assert.deepEqual([repeat.status, repeat.code, dup.code], ['unavailable', 'frame_edit_selection_required', 'frame_edit_selection_required']);
      assert.equal(r.s.writes.length, 0);
      continue;
    }
    const [first, second] = r.s.writes;
    assert.deepEqual(first.payload, { command: 'object.frames.repeat', fromLayerUid: uid(row.bounds.minL), toLayerUid: uid(row.bounds.maxL),
      fromFrame: row.bounds.minF, toFrame: row.bounds.maxF, times: 3 });
    assert.equal(second.op, P + 'frames.clipboard.duplicate');
    assert.equal(second.payload.layerUid, uid(row.active));
    assert.deepEqual(second.payload.selection.map((c) => [c.layerUid, c.frame]), row.cells.map((c) => [uid(c.layer), c.frame]));
  }
});

test('controls: a payload that drifts from the baseline targeting is rejected by the same comparison', async () => {
  let controls = 0;
  for (const row of oracle.rows.filter((r) => KIND[r.kind])) {
    const r = rigFor(row);
    if (row.kind === 'convertToKeyframes' && row.noSelectionOnActive) continue;
    const { write } = await runKind(r, actionFor(row));
    const bad = JSON.parse(JSON.stringify(write.payload));
    if (row.kind === 'insertFrame') { bad.cell += 1; }
    else if (row.kind === 'insertKey' || row.kind === 'insertKeyAt') { bad.cells = [bad.cells[0] + 1]; }
    else if (row.kind === 'convertToKeyframes') { bad.cells = bad.cells.concat([bad.cells[0] + 100]); }
    else if (row.kind === 'deleteFrames') { bad.targets = bad.targets.slice(0, -1); }
    else if (row.kind === 'removeFrameSpan') { bad.to += 1; }
    else if (row.kind === 'extendExposure') { bad.count += 1; }
    else if (row.kind === 'removeFrame') { bad.from += 1; }
    else { bad.cell = (bad.cell === undefined ? 0 : bad.cell) + 1; if (bad.cell === 1 && row.kind === 'clearKey') bad.cell = row.cf + 1; }
    assert.equal(verdict(row, bad), false, row.kind + ' drift must be caught');
    controls++;
  }
  assert.ok(controls >= 400);
  // The explicit-selection rule: dropping the selection from F5 must not still match a row that had one.
  const row = oracle.rows.find((r) => r.kind === 'insertFrame' && r.layerSel.length && !r.aborted);
  const { write } = await runKind(rigFor(row), actionFor(row));
  const widened = JSON.parse(JSON.stringify(write.payload)); delete widened.layerUids;
  assert.equal(verdict(row, widened), false);
});

test('literal requests for every dense-editor action', async () => {
  const cases = [
    [{ kind: 'insertFrame' }, P + 'frames.insert', { command: 'object.frames.insert', cell: 2 }],
    [{ kind: 'insertKey' }, P + 'layer.key.promote', { command: 'object.layer.key.promote', cells: [2] }],
    [{ kind: 'insertKeyAt', layer: 1, frame: 6 }, P + 'layer.key.promote', { command: 'object.layer.key.promote', layerUids: ['layer-uid-1'], cells: [6] }],
    [{ kind: 'insertBlankKey' }, P + 'layer.key.blank', { command: 'object.layer.key.blank', layerUid: 'layer-uid-1', cell: 2 }],
    [{ kind: 'duplicateKey' }, P + 'frames.duplicate', { command: 'object.frames.duplicate', layerUid: 'layer-uid-1', cell: 2 }],
    [{ kind: 'extendExposure' }, P + 'frames.extend', { command: 'object.frames.extend', cell: 2, count: 1 }],
    [{ kind: 'extendExposure', count: 0 }, P + 'frames.extend', { command: 'object.frames.extend', cell: 2, count: 1 }],
    [{ kind: 'extendExposure', count: 4 }, P + 'frames.extend', { command: 'object.frames.extend', cell: 2, count: 4 }],
    [{ kind: 'clearKey' }, P + 'layer.key.clear', { command: 'object.layer.key.clear', layerUid: 'layer-uid-1', cell: 2 }],
    [{ kind: 'removeTween', layer: 2, from: 3, to: 7 }, P + 'layer.tween.remove', { command: 'object.layer.tween.remove', layerUid: 'layer-uid-2', from: 3, to: 7 }],
    [{ kind: 'removeFrame' }, P + 'frames.remove', { command: 'object.frames.remove', from: 2, to: 2 }],
    [{ kind: 'removeFrameSpan' }, P + 'frames.remove', { command: 'object.frames.remove', from: 2, to: 2 }],
    [{ kind: 'moveKey', layer: 0, from: 2, to: 5 }, P + 'layer.key.move', { command: 'object.layer.key.move', layerUid: 'layer-uid-0', from: 2, to: 5 }]
  ];
  for (const [action, op, payload] of cases) {
    const r = rig({ active: 1 });
    const out = await r.intent.run(action);
    assert.equal(r.s.writes.length, 1, action.kind);
    assert.equal(r.s.writes[0].op, op); assert.deepEqual(r.s.writes[0].payload, payload);
    assert.equal(out.status, 'applied-presented', JSON.stringify(out));
  }
  const grid = [{ layer: 2, frame: 4 }, { layer: 0, frame: 5 }];
  const r = rig({ active: 1, cells: grid, layerSel: [2, 0], frame: 5 });
  await r.intent.run({ kind: 'deleteFrames' }); await r.intent.run({ kind: 'repeatSelection', times: 2 }); await r.intent.run({ kind: 'insertFrame' }); await r.intent.run({ kind: 'insertKey' });
  assert.deepEqual(r.s.writes.map((w) => w.payload), [
    { command: 'object.layer.key.clear_cells', targets: [{ layerUid: 'layer-uid-2', frame: 4 }, { layerUid: 'layer-uid-0', frame: 5 }] },
    { command: 'object.frames.repeat', fromLayerUid: 'layer-uid-0', toLayerUid: 'layer-uid-2', fromFrame: 4, toFrame: 5, times: 2 },
    { command: 'object.frames.insert', cell: 5, layerUids: ['layer-uid-2', 'layer-uid-0'] },
    { command: 'object.layer.key.promote', layerUids: ['layer-uid-2', 'layer-uid-0'], cells: [5] }
  ]);
});

test('every request the adapter emits is accepted by the shared application client with the pinned revision', async () => {
  assert.deepEqual(Intent.KINDS.slice().sort(), Object.keys(ACTIONS).sort());
  let emitted = 0;
  for (const kind of Intent.KINDS) {
    const r = rig({ active: 1, cells: [{ layer: 1, frame: 3 }], layerSel: [1] });
    const out = await r.intent.run(act(kind));
    if (r.s.writes.length) {
      const w = r.s.writes[0];
      assert.doesNotThrow(() => SDK.validateRequest({ apiVersion: 2, requestId: 'req-1', instanceId: 'instance-A', documentId: 'document-A', operation: w.op, expectedRevision: 7, payload: w.payload }), kind);
      assert.equal(w.expected.contentRevision, 7);
      emitted++;
    } else assert.equal(out.status, 'unavailable', kind);
  }
  assert.equal(emitted, 15, 'fifteen actions reach a write; only copy, cut and paste do not');
});

test('statuses: applied, unchanged, unconfirmed and applied-unconfirmed are decided from the receipt and the published state', async () => {
  let r = rig({ frame: 2 });
  assert.deepEqual({ ...await r.intent.run({ kind: 'insertFrame' }) }, { status: 'applied-presented', applied: true, playhead: 3 });
  r = rig({ frame: 2 });
  assert.deepEqual({ ...await r.intent.run({ kind: 'duplicateKey' }) }, { status: 'applied-presented', applied: true, playhead: 3 });
  r = rig({ frame: 2 });
  assert.deepEqual({ ...await r.intent.run({ kind: 'extendExposure', count: 3 }) }, { status: 'applied-presented', applied: true });
  assert.equal(r.s.total, 13);
  r = rig({ frame: 9, total: 10 });
  assert.deepEqual({ ...await r.intent.run({ kind: 'removeFrame' }) }, { status: 'applied-presented', applied: true, playhead: 8 });
  assert.equal(r.s.total, 9); assert.equal(r.s.frame, 8, 'the editor clamps its playhead into the shorter timeline');
  r = rig({ write: (s, op, payload, receipt) => receipt(false) });
  assert.deepEqual({ ...await r.intent.run({ kind: 'clearKey' }) }, { status: 'unchanged', applied: false });
  const broken = [
    ['revision skipped', (s, op, p, receipt) => { s.rev += 2; s.total += 1; return Object.assign(receipt(true), { contentRevision: s.rev }); }, 'unconfirmed'],
    ['history count wrong', (s, op, p, receipt) => { s.rev += 1; s.total += 1; const x = receipt(true); x.result.historyEntriesAdded = 0; return x; }, 'unconfirmed'],
    ['foreign document', (s, op, p, receipt) => { s.rev += 1; s.total += 1; return Object.assign(receipt(true), { documentId: 'other' }); }, 'unconfirmed'],
    ['extra result member', (s, op, p, receipt) => { s.rev += 1; s.total += 1; const x = receipt(true); x.result.extra = 1; return x; }, 'unconfirmed'],
    ['ok false receipt', (s, op, p, receipt) => Object.assign(receipt(true), { ok: false }), 'unconfirmed'],
    ['length not published', (s, op, p, receipt) => { s.rev += 1; return receipt(true); }, 'applied-unconfirmed'],
    ['layers reordered', (s, op, p, receipt) => { s.rev += 1; s.total += 1; s.uids.reverse(); return receipt(true); }, 'applied-unconfirmed'],
    ['not presented', (s, op, p, receipt) => { s.rev += 1; s.total += 1; s.presented = 0; return receipt(true); }, 'applied-unconfirmed'],
    ['playhead past the new end', (s, op, p, receipt) => { s.rev += 1; s.total += 1; s.frame = 40; s.presented = 40; return receipt(true); }, 'applied-unconfirmed']
  ];
  for (const [label, write, status] of broken) {
    r = rig({ write });
    assert.equal((await r.intent.run({ kind: 'insertFrame' })).status, status, label);
  }
});

test('refusals, lost transports and replaced sessions keep their own status', async () => {
  let r = rig({ write: () => { throw new Error('Native command refused: stale_revision'); } });
  assert.deepEqual({ ...await r.intent.run({ kind: 'insertKey' }) }, { status: 'refused', code: 'stale_revision' });
  r = rig({ write: () => { throw new Error('Native command refused: unavailable'); } });
  assert.equal((await r.intent.run({ kind: 'insertKey' })).code, 'unavailable');
  r = rig({ write: () => { throw new Error('Native frame edit intent is stale'); } });
  assert.deepEqual({ ...await r.intent.run({ kind: 'insertKey' }) }, { status: 'unavailable', code: 'frame_edit_stale_intent' });
  r = rig({ write: () => { throw new Error('Native frame edit intent is invalid'); } });
  assert.equal((await r.intent.run({ kind: 'insertKey' })).code, 'frame_edit_request_invalid');
  r = rig({ write: () => Promise.reject(new Error('IPC lost')) });
  assert.deepEqual({ ...await r.intent.run({ kind: 'insertKey' }) }, { status: 'unconfirmed' });
  r = rig({ write: (s, op, p, receipt) => { s.rev += 1; s.total += 1; s.replacement = Object.assign({}, r.session); return receipt(true); } });
  assert.deepEqual({ ...await r.intent.run({ kind: 'insertFrame' }) }, { status: 'applied-unconfirmed', applied: true });
});

test('one action at a time, and a retired adapter refuses everything', async () => {
  let release; const gate = new Promise((resolve) => { release = resolve; });
  const r = rig({ write: async (s, op, p, receipt) => { await gate; s.total = lengthAfter(s, op, p); s.rev += 1; return receipt(true); } });
  const first = r.intent.run({ kind: 'insertFrame' });
  assert.equal(r.intent.isPending(), true);
  assert.deepEqual({ ...await r.intent.run({ kind: 'insertKey' }) }, { status: 'busy' });
  release();
  assert.equal((await first).status, 'applied-presented');
  assert.equal(r.intent.isPending(), false);
  assert.equal((await r.intent.run({ kind: 'insertKey' })).status, 'applied-presented');
  r.intent.dispose();
  assert.deepEqual({ ...await r.intent.run({ kind: 'insertKey' }) }, { status: 'unavailable', code: 'frame_edit_retired' });
  let finish; const slow = rig({ write: (s, op, p, receipt) => new Promise((resolve) => { finish = () => { s.rev += 1; s.total += 1; resolve(receipt(true)); }; }) });
  const pending = slow.intent.run({ kind: 'insertFrame' });
  slow.intent.dispose(); finish();
  assert.deepEqual({ ...await pending }, { status: 'applied-unconfirmed', applied: true });
});

test('actions native has no operation for report typed unavailability before any session or selection is read', async () => {
  const hostile = { currentSession() { throw new Error('must not consult'); }, application: SDK, frame() { throw new Error('no'); }, activeLayer() { throw new Error('no'); },
    layerSelection() { throw new Error('no'); }, cellSelection() { throw new Error('no'); }, available() { throw new Error('no'); } };
  const intent = Intent.create(hostile);
  for (const kind of ['copyFrames', 'cutFrames', 'pasteFrames']) assert.deepEqual({ ...await intent.run({ kind }) }, { status: 'unavailable', code: 'frame_edit_packet_unavailable' });
  assert.deepEqual({ ...await intent.run({ kind: 'insertBlankKey' }) }, { status: 'unavailable', code: 'frame_edit_session_unavailable' }, 'F7 now requires a real presented session');
  // The shared client really declares none of them, so no later wiring can reach a write by accident.
  for (const operation of [P + 'frames.clipboard.paste', P + 'frames.clipboard.cut', P + 'frames.clipboard.copy', P + 'layer.key.insert_blank']) {
    assert.throws(() => SDK.validateRequest({ apiVersion: 2, requestId: 'req-1', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 1, operation, payload: { command: 'x' } }), /not admitted/);
  }
});

test('invalid intents, missing sessions and an undeclared operation never reach a write', async () => {
  const bad = [undefined, null, 'insertFrame', [], {}, { kind: 'unknown' }, { kind: 'insertFrame', extra: 1 }, { kind: 'insertKeyAt', layer: 1 }, { kind: 'removeTween', layer: 0, from: 1 },
    { kind: 'moveKey', layer: 0, from: 1, to: 2, x: 0 }, { kind: 'repeatSelection' }, { kind: 'extendExposure', count: 'x' }, { kind: 'extendExposure', count: 1000 },
    { kind: 'insertKeyAt', layer: 9, frame: 1 }, { kind: 'insertKeyAt', layer: 1, frame: 99 }, { kind: 'removeTween', layer: -1, from: 0, to: 1 }, { kind: 'moveKey', layer: 0, from: 1.5, to: 2 }];
  for (const action of bad) {
    const r = rig({ cells: [{ layer: 0, frame: 1 }] });
    const out = await r.intent.run(action);
    assert.equal(out.status, 'unavailable'); assert.equal(out.code, 'frame_edit_invalid_intent', JSON.stringify(action));
    assert.equal(r.s.writes.length, 0);
  }
  assert.equal((await rig({ noSession: true }).intent.run({ kind: 'insertFrame' })).code, 'frame_edit_session_unavailable');
  assert.equal((await rig({ unavailable: true }).intent.run({ kind: 'insertFrame' })).code, 'frame_edit_session_unavailable');
  assert.equal((await rig({ noWrite: true }).intent.run({ kind: 'insertFrame' })).code, 'frame_edit_session_unavailable');
  const notPresented = rig(); notPresented.s.presented = 5;
  assert.equal((await notPresented.intent.run({ kind: 'insertFrame' })).code, 'frame_edit_session_unavailable');
  for (const [label, opts] of [['frame past the end', { frame: 10 }], ['active layer out of range', { active: 3 }], ['selected layer repeated', { layerSel: [1, 1] }],
    ['selected layer out of range', { layerSel: [4] }], ['cell frame out of range', { cells: [{ layer: 0, frame: 10 }] }], ['cell repeated', { cells: [{ layer: 0, frame: 1 }, { layer: 0, frame: 1 }] }]]) {
    const r = rig(opts);
    assert.equal((await r.intent.run({ kind: 'insertFrame' })).code, 'frame_edit_invalid_intent', label);
    assert.equal(r.s.writes.length, 0);
  }
  const undeclared = rig({ application: { validateRequest() { throw new TypeError('operation is not admitted'); } } });
  assert.deepEqual({ ...await undeclared.intent.run({ kind: 'insertFrame' }) }, { status: 'unavailable', code: 'frame_edit_declaration_missing' });
  assert.equal(undeclared.s.writes.length, 0);
  const cells = []; for (let l = 0; l < 3; l++) for (let f = 0; f < 10; f++) cells.push({ layer: l, frame: f });
  const huge = rig({ cells, total: 400 });
  for (let f = 10; f < 400; f++) huge.s.cells.push({ layer: 0, frame: f });
  assert.equal((await huge.intent.run({ kind: 'deleteFrames' })).code, 'frame_edit_request_invalid');
  assert.equal(huge.s.writes.length, 0);
  for (const ports of [null, {}, { currentSession() {} }]) assert.throws(() => Intent.create(ports), /requires port/);
});

test('no legacy or Paper writer is reachable from the adapter, statically or at run time', async () => {
  const source = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'adapters', 'native-frame-edit-intent.js'), 'utf8');
  for (const pattern of [/\binsertFrame\s*\(/, /\binsertKeyframe\w*\s*\(/, /\binsertBlankKeyframe\s*\(/, /\bremoveFrame\w*\s*\(/, /\bclearKeyframe\s*\(/, /\bconvertToKeyframes\s*\(/,
    /\bduplicateSelectedFrames\s*\(/, /\bmoveKeyframe\s*\(/, /\bwindow\b/, /\bSM\b/, /pushUndo/, /saveAllLayerFrames/, /loadFrame/, /n20AllowLegacyWrite/, /\bpaper\b/i, /localStorage/]) {
    assert.doesNotMatch(source, pattern);
  }
  const names = ['insertFrame', 'insertKeyframe', 'insertBlankKeyframe', 'removeFrame', 'removeFrameSpan', 'clearKeyframe', 'convertToKeyframes', 'duplicateSelectedFrames', 'pushUndo', 'saveAllLayerFrames'];
  const calls = [];
  names.forEach((name) => { globalThis[name] = () => { calls.push(name); }; });
  globalThis.window = { SM: new Proxy({}, { get: (_, name) => () => { calls.push('SM.' + String(name)); } }) };
  try {
    for (const kind of Intent.KINDS) await rig({ active: 1, cells: [{ layer: 1, frame: 3 }], layerSel: [1] }).intent.run(act(kind));
  } finally { names.forEach((name) => { delete globalThis[name]; }); delete globalThis.window; }
  assert.deepEqual(calls, []);
});

test('repeat intent exposes only native confirmed cycle/skip counts and does not equate growth with cycled layers', async()=>{
  const selected=[{layer:0,frame:2}];
  for(const applied of [true,false]) {
    const r=rig({cells:selected,write:(s,op,p,receipt)=>{
      if(applied){s.rev++;s.total+=2;}
      const value=receipt(applied,op);value.result.cycledLayers=0;value.result.skippedComponentLayers=1;return value;
    }});
    const out=await r.intent.run({kind:'repeatSelection',times:2});
    assert.equal(out.status,applied?'applied-presented':'unchanged');
    assert.equal(out.applied,applied);assert.equal(out.cycledLayers,0);assert.equal(out.skippedComponentLayers,1);
  }
  for(const mutate of [r=>{delete r.result.cycledLayers;},r=>{r.result.skippedComponentLayers=-1;},r=>{r.result.extra=1;}]) {
    const r=rig({cells:selected,write:(s,op,p,receipt)=>{s.rev++;const value=receipt(true,op);mutate(value);return value;}});
    assert.deepEqual(await r.intent.run({kind:'repeatSelection',times:2}),{status:'unconfirmed'});
  }
});


test('F7 targets only the current active UID/frame and checkpoints repeatedly without a playhead hint', async () => {
  const r = rig({ active: 1, frame: 3, layerSel: [2, 0], cells: [{ layer: 0, frame: 7 }] });
  for (let revision = 7; revision < 9; revision++) {
    assert.deepEqual({ ...await r.intent.run({ kind: 'insertBlankKey' }) }, { status: 'applied-presented', applied: true });
    assert.deepEqual(r.s.writes.at(-1).payload, { command: 'object.layer.key.blank', layerUid: 'layer-uid-1', cell: 3 });
    assert.equal(r.s.writes.at(-1).op, P + 'layer.key.blank');
    assert.equal(r.s.writes.at(-1).expected.contentRevision, revision);
  }
  assert.equal(r.s.rev, 9); assert.equal(r.s.total, 10); assert.equal(r.s.frame, 3);
  assert.deepEqual(r.s.layerSel, [2, 0]); assert.deepEqual(r.s.cells, [{ layer: 0, frame: 7 }]);
});

test('F7 preserves missing declaration/session, stale, refusal, busy and unconfirmed dispositions', async () => {
  for (const options of [{ noSession: true }, { noWrite: true }, { unavailable: true }]) {
    const r = rig(options);
    assert.equal((await r.intent.run({ kind: 'insertBlankKey' })).code, 'frame_edit_session_unavailable');
    assert.equal(r.s.writes.length, 0);
  }
  const absent = rig({ application: { validateRequest() { throw new TypeError('not admitted'); } } });
  assert.equal((await absent.intent.run({ kind: 'insertBlankKey' })).code, 'frame_edit_declaration_missing');
  assert.equal(absent.s.writes.length, 0);
  for (const [message, expected] of [['Native frame edit intent is stale', { status: 'unavailable', code: 'frame_edit_stale_intent' }],
    ['Native command refused: unavailable', { status: 'refused', code: 'unavailable' }], ['IPC lost', { status: 'unconfirmed' }]]) {
    const r = rig({ write() { throw new Error(message); } });
    assert.deepEqual({ ...await r.intent.run({ kind: 'insertBlankKey' }) }, expected);
    assert.equal(r.s.writes.length, 1);
  }
  const noop = rig({ write: (s, op, p, receipt) => receipt(false, op) });
  assert.deepEqual({ ...await noop.intent.run({ kind: 'insertBlankKey' }) }, { status: 'unconfirmed' });
  assert.equal(noop.s.writes.length, 1); assert.equal(noop.s.rev, 7); assert.equal(noop.s.total, 10);
  assert.equal(noop.s.frame, 2); assert.equal(noop.intent.isPending(), false);
  let release; const gate = new Promise(resolve => { release = resolve; });
  const held = rig({ write: async (s, op, p, receipt) => { await gate; s.rev++; return receipt(true, op); } });
  const first = held.intent.run({ kind: 'insertBlankKey' });
  assert.deepEqual({ ...await held.intent.run({ kind: 'insertBlankKey' }) }, { status: 'busy' });
  assert.equal(held.s.writes.length, 1); release();
  assert.equal((await first).status, 'applied-presented'); assert.equal(held.s.rev, 8);
});
