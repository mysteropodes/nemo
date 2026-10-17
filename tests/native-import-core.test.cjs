'use strict';
// L10-J: the shared submit runner. The session double follows only the documented owner receipt rule (helpers/native-import-session.cjs);
// every expected value below is a literal taken from the accepted L10-H dispatcher test (ids, receipt members, one revision).
const test = require('node:test');
const assert = require('node:assert/strict');
const coreModule = require('../src/js/adapters/native-import-core.js');
const contract = require('../src/js/adapters/native-import-contract.js');
const { makeSession, makePorts, layerRow } = require('./helpers/native-import-session.cjs');

const record = (layerRef, x) => ({ schemaVersion: 1, family: 'editable-vector-path',
  target: { contextId: 'scene-root', frameScope: { kind: 'authored', frame: 0 }, layerUid: layerRef, strokeId: '' },
  geometry: { kind: 'cubic-path', closed: false, coordinateSpace: 'document-world', handleSpace: 'relative', segments: [
    { point: { x, y: 0 }, handleIn: { x: 0, y: 0 }, handleOut: { x: 0, y: 0 } }, { point: { x: x + 10, y: 5 }, handleIn: { x: 0, y: 0 }, handleOut: { x: 0, y: 0 } }] },
  fill: null, vector: { stroke: { kind: 'solid', r: 0, g: 0, b: 0, a: 1 }, hasRealStroke: true, strokeWidth: 2, strokeCap: 'butt', strokeJoin: 'miter', opacity: 1 } });
const mixed = () => ({
  layers: [{ sourceId: 'w', name: 'Wire' }],
  records: [{ sourceId: 'w:0', record: record('w', 1) }, { sourceId: 'w:1', record: record('w', 2) }, { sourceId: 'e:0', record: record('L0', 3) }],
  skipped: [{ kind: 'element', sourceId: 'svg:9', reason: 'unsupported-element:Raster' }], unavailable: [],
});
async function go(options, extra, batch) {
  const h = makeSession(options);
  const made = makePorts(h, extra);
  const core = coreModule.create(made.ports);
  const begun = core.begin();
  assert.equal(begun.ok, true);
  return { h, core, made, begun, result: await core.submit(begun.ctx, batch || mixed()) };
}

test('a mixed batch publishes once and the result is correlated, re-read and selected', async () => {
  const { h, result, made } = await go();
  assert.equal(h.calls.length, 1);
  assert.equal(h.state.contentRevision, 8);
  assert.deepEqual(result, {
    status: 'imported', applied: true, revision: 8, historyEntriesAdded: 1, readback: 'confirmed', selected: true,
    layers: [{ sourceId: 'w', layerUid: 'native-layer:8:0' }],
    objects: [{ sourceId: 'w:0', strokeId: 'native-stroke:8:0' }, { sourceId: 'w:1', strokeId: 'native-stroke:8:1' }, { sourceId: 'e:0', strokeId: 'native-stroke:8:2' }],
    skipped: [{ kind: 'element', sourceId: 'svg:9', reason: 'unsupported-element:Raster' }],
  });
  assert.deepEqual(h.state.layers.map((l) => l.layerUid), ['L0', 'native-layer:8:0']);
  assert.deepEqual(made.selection.value, ['native-layer:8:0']);
  assert.deepEqual(Object.keys(h.calls[0]), ['command', 'layers', 'records', 'skipped', 'unavailable']);
});

test('a batch that creates nothing is a no-op: no revision, no history entry, the skip report comes back', async () => {
  const { h, result } = await go({}, {}, { layers: [], records: [], skipped: [{ kind: 'element', sourceId: 'svg:0:1:hole:0', reason: 'unclaimed-hole' }] });
  assert.equal(result.status, 'imported');
  assert.equal(result.applied, false);
  assert.equal(result.historyEntriesAdded, 0);
  assert.equal(result.revision, 7);
  assert.equal(h.state.contentRevision, 7);
  assert.deepEqual(result.skipped, [{ kind: 'element', sourceId: 'svg:0:1:hole:0', reason: 'unclaimed-hole' }]);
  assert.equal(result.selected, false);
});

test('an element whose provider is absent is refused by the owner as a whole, naming nothing the adapter invented', async () => {
  const batch = mixed();
  batch.unavailable = [{ sourceId: 'psd:0', need: 'raster-asset' }];
  const { h, result } = await go({}, {}, batch);
  assert.deepEqual(result, { status: 'unavailable', reason: 'native_refused', code: 'unavailable' });
  assert.equal(h.state.contentRevision, 7, 'including its valid vector records');
  assert.deepEqual(h.state.layers.map((l) => l.layerUid), ['L0']);
});

test('refusals and faults are typed statuses and are never retried', async () => {
  for (const [fault, expected] of [['stale', { status: 'stale', code: 'stale_revision' }], ['busy', { status: 'refused', code: 'busy_conflict' }]]) {
    const { h, result } = await go({ fault });
    assert.deepEqual(result, expected);
    assert.equal(h.calls.length, 1, 'no retry');
  }
  const lost = await go({ fault: 'transport' });
  assert.equal(lost.result.status, 'unconfirmed');
  assert.equal(lost.result.indeterminate, true);
  assert.equal(lost.h.calls.length, 1);
  const locked = await go({ layers: [layerRow('L0', 'Layer 1', { locked: true })] });
  assert.deepEqual(locked.result, { status: 'unavailable', reason: 'native_refused', code: 'unavailable' });
  const unknown = mixed();
  unknown.records[2].record.target.layerUid = 'nope';
  assert.deepEqual((await go({}, {}, unknown)).result, { status: 'refused', code: 'not_found' });
});

test('a second import while one is in flight is busy, and the first still completes', async () => {
  let release;
  const gate = new Promise((resolve) => { release = resolve; });
  const h = makeSession({ gate });
  const core = coreModule.create(makePorts(h).ports);
  const first = core.submit(core.begin().ctx, mixed());
  assert.equal(core.isPending(), true);
  assert.deepEqual(core.begin(), { ok: false, status: 'busy' });
  release();
  assert.equal((await first).status, 'imported');
  assert.equal(core.isPending(), false);
  assert.equal(core.begin().ok, true);
});

test('the context is re-checked at submit: a session or revision that moved on is stale and sends nothing', async () => {
  const h = makeSession();
  const core = coreModule.create(makePorts(h).ports);
  const begun = core.begin();
  h.state.contentRevision = 9;
  assert.deepEqual(await core.submit(begun.ctx, mixed()), { status: 'stale', reason: 'context_changed' });
  assert.equal(h.calls.length, 0);
  const h2 = makeSession();
  const begun2 = coreModule.create(makePorts(h2).ports).begin();
  const other = makeSession();
  const switched = coreModule.create(makePorts(h2, { currentSession: () => other.session }).ports);
  assert.deepEqual(await switched.submit(begun2.ctx, mixed()), { status: 'stale', reason: 'context_changed' });
  // The session's own pre-dispatch check (its pin differs from the one passed) is stale too, never unconfirmed.
  const h3 = makeSession();
  const ctx3 = coreModule.create(makePorts(h3).ports).begin().ctx;
  const raising = Object.assign({}, ctx3, { write: () => { throw new Error('Native import intent is stale or invalid'); } });
  assert.deepEqual(await coreModule.create(makePorts(h3).ports).submit(raising, mixed()), { status: 'stale', reason: 'intent_stale' });
});

test('an invalid batch is refused locally and nothing is sent', async () => {
  const h = makeSession();
  const core = coreModule.create(makePorts(h).ports);
  const ctx = core.begin().ctx;
  const named = mixed();
  named.records[0].record.target.strokeId = 'minted-by-caller';
  assert.equal((await core.submit(ctx, named)).status, 'invalid');
  const dup = mixed();
  dup.layers.push({ sourceId: 'w', name: 'Again' });
  assert.match((await core.submit(ctx, dup)).reason, /repeats a source id/);
  assert.equal(h.calls.length, 0);
});

test('the 4096-byte request bound is enforced before dispatch and counts the whole envelope', async () => {
  const h = makeSession();
  const core = coreModule.create(makePorts(h).ports);
  const ctx = core.begin().ctx;
  const big = mixed();
  for (let i = 0; i < 8; i++) big.records.push({ sourceId: `r:${i}`, record: record('L0', i) });
  const out = await core.submit(ctx, big);
  assert.equal(out.status, 'too_large');
  assert.equal(out.limit, 4096);
  assert.ok(out.bytes > 4096);
  assert.equal(h.calls.length, 0);
  const small = await core.submit(ctx, mixed());
  assert.equal(small.status, 'imported');
});

test('a receipt that does not account for the batch is unconfirmed, never success (the document did publish)', async () => {
  const cases = {
    'object dropped': (r) => { r.result.objects.pop(); },
    'object added': (r) => { r.result.objects.push({ sourceId: 'x', strokeId: 'native-stroke:8:9' }); },
    'objects reordered': (r) => { r.result.objects.reverse(); },
    'layer renamed in the report': (r) => { r.result.layers[0].sourceId = 'other'; },
    'skip dropped': (r) => { r.result.skipped = []; },
    'skip reason changed': (r) => { r.result.skipped[0].reason = 'x'; },
    'duplicate stroke id': (r) => { r.result.objects[1].strokeId = r.result.objects[0].strokeId; },
    'revision advanced twice': (r) => { r.contentRevision += 1; },
    'history entry missing': (r) => { r.result.historyEntriesAdded = 0; },
    'applied false with records': (r) => { r.result.applied = false; r.result.historyEntriesAdded = 0; r.contentRevision = 7; },
    'wrong api version': (r) => { r.apiVersion = 1; },
    'other owner': (r) => { r.instanceId = 'instance-B'; },
    'other document': (r) => { r.documentId = 'document-B'; },
    'extra result member': (r) => { r.result.document = {}; },
    'missing result member': (r) => { delete r.result.skipped; },
    'not ok': (r) => { r.ok = false; },
  };
  for (const [name, edit] of Object.entries(cases)) {
    const { result } = await go({ tamper: (r) => { edit(r); return r; } });
    assert.equal(result.status, 'unconfirmed', name);
    assert.ok(!('layers' in result) || result.status !== 'imported', name);
  }
});

test('a result the session cannot confirm is reported as imported but unconfirmed and is never selected', async () => {
  const hidden = await go({ hideCreated: true });
  assert.equal(hidden.result.status, 'imported');
  assert.equal(hidden.result.readback, 'unconfirmed');
  assert.equal(hidden.result.selected, false);
  assert.deepEqual(hidden.made.selection.value, ['L0']);
  const locked = await go({ createdExtra: { locked: true } });
  assert.equal(locked.result.readback, 'unconfirmed');
  assert.equal(locked.result.selected, false);
  const refusedSelect = await go({}, { select: () => false });
  assert.equal(refusedSelect.result.readback, 'confirmed');
  assert.equal(refusedSelect.result.selected, false);
  let moved = false;
  const h = makeSession();
  const ports = makePorts(h).ports;
  const original = ports.selection;
  const core = coreModule.create(Object.assign({}, ports, { selection: () => { const v = original(); return moved ? ['L9'] : v; } }));
  const begun = core.begin();
  moved = true;
  const late = await core.submit(begun.ctx, mixed());
  assert.equal(late.status, 'imported');
  assert.equal(late.selected, false, 'a selection that changed during the import is left alone');
});

test('the re-read and the selection are guarded: a revision that moved after the receipt, or a selection that changed, is never reported as confirmed or selected', async () => {
  // The document moves once more right after the receipt was produced: the re-read no longer matches the receipt.
  const moved = makeSession({ tamper: (receipt) => { moved.state.contentRevision += 1; return receipt; } });
  const movedCore = coreModule.create(makePorts(moved).ports);
  const late = await movedCore.submit(movedCore.begin().ctx, mixed());
  assert.equal(late.status, 'imported');
  assert.equal(late.revision, 8, 'the receipt revision is reported as received');
  assert.deepEqual([late.readback, late.selected], ['unconfirmed', false]);
  // The user selects another layer while the import is in flight: the new layer is created but the selection is left alone.
  const h = makeSession({ layers: [layerRow('L0', 'Layer 1'), layerRow('L1', 'Layer 2')] });
  const made = makePorts(h);
  const core = coreModule.create(made.ports);
  const begun = core.begin();
  made.selection.value = ['L1'];
  const kept = await core.submit(begun.ctx, mixed());
  assert.deepEqual([kept.status, kept.readback, kept.selected], ['imported', 'confirmed', false]);
  assert.deepEqual(made.selection.value, ['L1'], 'select() was never asked to move the selection');
});

test('a no-op receipt must repeat the declared skip report, and a receipt that created nothing for a creating batch is refused', async () => {
  const noop = () => ({ layers: [], records: [], skipped: [{ kind: 'element', sourceId: 'svg:0:1:hole:0', reason: 'unclaimed-hole' }] });
  for (const [name, edit] of Object.entries({
    'skip dropped': (r) => { r.result.skipped = []; },
    'skip reason changed': (r) => { r.result.skipped[0].reason = 'other'; },
    'skip source changed': (r) => { r.result.skipped[0].sourceId = 'svg:0:1:hole:1'; },
    'layer invented': (r) => { r.result.layers.push({ sourceId: 'x', layerUid: 'native-layer:7:0' }); },
  })) {
    const out = await go({ tamper: (r) => { edit(r); return r; } }, {}, noop());
    assert.equal(out.result.status, 'unconfirmed', name);
  }
  assert.equal((await go({}, {}, noop())).result.status, 'imported', 'the unmodified no-op receipt is accepted');
});

test('an absent or unpresented native session is typed unavailable; a disposed runner stays unavailable', () => {
  const h = makeSession({ declared: false });
  const core = coreModule.create(makePorts(h).ports);
  assert.deepEqual(core.begin(), { ok: false, status: 'unavailable', reason: 'native_declaration_absent' });
  const h2 = makeSession();
  assert.equal(coreModule.create(makePorts(h2, { available: () => false }).ports).begin().reason, 'no_presented_native_session');
  assert.equal(coreModule.create(makePorts(h2, { frame: () => 99 }).ports).begin().reason, 'no_presented_native_session');
  assert.equal(coreModule.create(makePorts(h2, { currentSession: () => { throw new Error('x'); } }).ports).begin().reason, 'no_presented_native_session');
  const live = coreModule.create(makePorts(h2).ports);
  live.dispose();
  assert.deepEqual(live.begin(), { ok: false, status: 'unavailable', reason: 'retired' });
  assert.equal(contract.MAX_REQUEST_BYTES, 4096);
});
