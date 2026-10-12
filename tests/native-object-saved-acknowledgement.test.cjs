'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const baseline = require('../src/js/adapters/project-dirty-baseline.js');
const ids = { saveRequestId: 'save-1', serializerRequestId: 'serialize-1' };
const A = '{"native":"A ☃","reference":"preserved"}\n', B = '{"native":"B","reference":"preserved"}\n';
const pin = { instanceId: 'instance-A', documentId: 'document-A', contentRevision: 7,
  lifecycleGeneration: 3, documentSnapshotId: 'snapshot-A' };
function deferred() { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }
function present() { return { apiVersion: 2, available: true, kind: 'native-object', ...pin,
  presentation: { owner: 'native', status: 'presented', contextId: 'scene-root', frame: 0,
    viewGeneration: 1, workId: 'native-work-1', ...pin } }; }
function input() {
  return { written: { status: 'published-current', eligibleCurrentSave: true, pin: { ...pin },
    destinationPath: '/owned/A.json', receipt: { apiVersion: 2, requestId: ids.saveRequestId,
      instanceId: pin.instanceId, documentId: pin.documentId, atRevision: 7,
      documentSnapshotId: pin.documentSnapshotId, byteCount: Buffer.byteLength(A), published: true, cleanupPending: false } },
    serialization: { apiVersion: 2, requestId: ids.serializerRequestId, instanceId: pin.instanceId,
      documentId: pin.documentId, contentRevision: 7, ok: true,
      result: { atRevision: 7, documentSnapshotId: pin.documentSnapshotId, documentJson: A } } };
}
// Explicitly unsafe test composition: this is NOT an existing product sink.
// It proves why point-in-time E4A eligibility alone cannot authorize UI commit.
function unsafe(ports) {
  return { begin: options => ({ ...options, ui: ports.readUISession() }), isCurrent: () => true,
    async acknowledge(ticket, value) {
      await ports.readPresentedObjectSession();
      ports.commitSavedMetadata({ ui: ticket.ui, pin: value.written.pin, destinationPath: value.written.destinationPath,
        documentJson: value.serialization.result.documentJson, receipt: value.written.receipt });
      return { status: 'acknowledged', acknowledged: true };
    } };
}
const create = process.env.N25E4B1A_BASELINE === '1' ? unsafe : ports =>
  require('../src/js/application/native-object-saved-acknowledgement.js').create(ports);
function fixture() {
  const effects = [], dirty = baseline.create(), handle = {};
  let ui = { handle, generation: 2 }, current = present(), commit = value => {
    effects.push(['commit', value.destinationPath, value.documentJson]); dirty.markSaved(value.documentJson); return true;
  }, readCurrent = () => current, ready = false;
  const ports = { readUISession: () => ui, readPresentedObjectSession: () => readCurrent(),
    commitSavedMetadata(value) { const result = commit(value); if (result === true) ready = true; return result; },
    markSavedIndeterminate(value) { if (ui.handle === value.ui.handle && ui.generation === value.ui.generation) { ready = false; effects.push(['indeterminate']); return true; } return false; },
    pickDestination() { throw Error('picker forbidden'); }, savePinned() { throw Error('writer forbidden'); },
    exportJSON() { throw Error('Paper forbidden'); }, remove() { throw Error('delete forbidden'); },
    retry() { throw Error('retry forbidden'); }, autosave() { throw Error('autosave forbidden'); } };
  const helper = create(ports), ticket = helper.begin(ids);
  return { helper, ticket, ports, effects, dirty, ready: () => ready, ui: () => ui,
    setUI: value => { ui = value; }, current: () => current, setCurrent: value => { current = value; },
    setRead: value => { readCurrent = value; }, setCommit: value => { commit = value; } };
}
test('matching genuine object publication acknowledges literal native bytes once', async () => {
  const h = fixture();
  assert.equal((await h.helper.acknowledge(h.ticket, input())).status, 'acknowledged');
  assert.deepEqual(h.effects, [['commit', '/owned/A.json', A]]);
  assert.equal(h.dirty.isDirtyBytes(A), false);
  assert.equal(h.ready(), true);
});
const invalid = [
  ['stale written result', x => { x.written.status = 'published-stale'; x.written.eligibleCurrentSave = false; }],
  ['cleanup pending', x => { x.written.receipt.cleanupPending = true; }],
  ['unpublished', x => { x.written.receipt.published = false; }],
  ['receipt save ID', x => { x.written.receipt.requestId = 'another'; }],
  ['serializer ID', x => { x.serialization.requestId = ids.saveRequestId; }],
  ['serializer document', x => { x.serialization.documentId = 'other'; }],
  ['serializer snapshot', x => { x.serialization.result.documentSnapshotId = 'other'; }],
  ['serializer bytes/receipt mismatch', x => { x.serialization.result.documentJson = B; }],
  ['invalid destination', x => { x.written.destinationPath = 'relative.json'; }],
  ['unsafe written revision', x => { x.written.pin.contentRevision = Number.MAX_SAFE_INTEGER + 1; }],
  ['oversize bytes', x => { x.serialization.result.documentJson = 'a'.repeat(4097); }],
];
for (const [name, change] of invalid) test(`${name} has zero saved UI effects`, async () => {
  const h = fixture(), value = input(); change(value);
  const result = await h.helper.acknowledge(h.ticket, value);
  assert.notEqual(result.status, 'acknowledged'); assert.deepEqual(h.effects, []);
});
for (const field of ['instanceId', 'documentId', 'contentRevision', 'lifecycleGeneration', 'documentSnapshotId']) {
  test(`held presented read with ${field} drift cannot acknowledge`, async () => {
    const h = fixture(), gate = deferred(); h.setRead(() => gate.promise);
    const pending = h.helper.acknowledge(h.ticket, input());
    const changed = present(); changed[field] = typeof changed[field] === 'number' ? changed[field] + 1 : 'other';
    changed.presentation[field] = changed[field];
    gate.resolve(changed); await pending; assert.deepEqual(h.effects, []);
  });
}
test('original UI handle is required even when native pin is unchanged before invocation', async () => {
  const h = fixture(); h.setUI({ handle: {}, generation: 2 });
  await h.helper.acknowledge(h.ticket, input()); assert.deepEqual(h.effects, []);
});
test('abort during final awaited read prevents every commit', async () => {
  const h = fixture(), gate = deferred(), controller = new AbortController(); h.setRead(() => gate.promise);
  const pending = h.helper.acknowledge(h.ticket, input(), { signal: controller.signal });
  controller.abort(); gate.resolve(present()); await pending; assert.deepEqual(h.effects, []);
});
test('UI generation replacement during awaited read prevents every commit', async () => {
  const h = fixture(), gate = deferred(); h.setRead(() => gate.promise);
  const pending = h.helper.acknowledge(h.ticket, input());
  h.setUI({ handle: h.ui().handle, generation: 3 }); gate.resolve(present()); await pending;
  assert.deepEqual(h.effects, []);
});
for (const kind of ['status-only', 'opacity', 'deferred']) test(`${kind} is not a current presented object`, async () => {
  const h = fixture(), state = present();
  if (kind === 'status-only') delete state.presentation;
  if (kind === 'opacity') state.kind = 'native-opacity';
  if (kind === 'deferred') state.presentation.status = 'deferred';
  h.setCurrent(state); await h.helper.acknowledge(h.ticket, input()); assert.deepEqual(h.effects, []);
});
test('frame/view advance alone permits acknowledgement of unchanged content', async () => {
  const h = fixture(); Object.assign(h.current().presentation, { frame: 12, viewGeneration: 8, workId: 'native-work-9' });
  assert.equal((await h.helper.acknowledge(h.ticket, input())).status, 'acknowledged');
});
test('P36 native bytes A/edit B/undo A/new revision/redo B remain content based', async () => {
  const h = fixture(); await h.helper.acknowledge(h.ticket, input());
  assert.equal(h.dirty.isDirtyBytes(A), false); assert.equal(h.dirty.isDirtyBytes(B), true);
  h.current().contentRevision = 9;
  assert.equal(h.dirty.isDirtyBytes(A), false); assert.equal(h.dirty.isDirtyBytes(B), true);
  const other = fixture(), replacement = present(); replacement.documentId = 'other-document'; replacement.presentation.documentId = 'other-document'; other.setCurrent(replacement);
  await other.helper.acknowledge(other.ticket, input()); assert.deepEqual(other.effects, []);
});
test('duplicate and reentrant acknowledgements consume an original ticket once', async () => {
  const h = fixture(); let reentrant;
  h.setCommit(value => { h.effects.push(['commit', value.documentJson]);
    reentrant = h.helper.acknowledge(h.ticket, input()); return true; });
  assert.equal((await h.helper.acknowledge(h.ticket, input())).status, 'acknowledged');
  assert.equal((await reentrant).status, 'refused');
  assert.equal((await h.helper.acknowledge(h.ticket, input())).status, 'refused');
  assert.deepEqual(h.effects, [['commit', A]]);
  assert.throws(() => h.helper.begin(ids), /already/);
});
test('concurrent acknowledgements on the same held read commit at most once', async () => {
  const h = fixture(), gate = deferred(); h.setRead(() => gate.promise);
  const a = h.helper.acknowledge(h.ticket, input()), b = h.helper.acknowledge(h.ticket, input());
  gate.resolve(present()); const results = await Promise.all([a, b]);
  assert.equal(results.filter(x => x.acknowledged).length, 1);
  assert.deepEqual(h.effects, [['commit', '/owned/A.json', A]]);
});
test('new pre-workflow attempt supersedes an older pending acknowledgement', async () => {
  const h = fixture(), gate = deferred(); h.setRead(() => gate.promise);
  const pending = h.helper.acknowledge(h.ticket, input());
  h.helper.begin({ saveRequestId: 'save-2', serializerRequestId: 'serialize-2' });
  gate.resolve(present()); await pending; assert.deepEqual(h.effects, []);
});
test('input and port mutation across await cannot retarget frozen bytes or metadata callback', async () => {
  const h = fixture(), gate = deferred(), value = input(); h.setRead(() => gate.promise);
  const pending = h.helper.acknowledge(h.ticket, value);
  value.written.destinationPath = '/owned/B.json'; value.serialization.result.documentJson = B;
  h.ports.commitSavedMetadata = () => { throw Error('retargeted callback'); };
  gate.resolve(present()); assert.equal((await pending).status, 'acknowledged');
  assert.deepEqual(h.effects, [['commit', '/owned/A.json', A]]);
});
for (const kind of ['throw', 'thenable', 'undefined']) test(`${kind} callback outcome is indeterminate, not generic rollback`, async () => {
  const h = fixture(); h.setCommit(() => {
    h.effects.push(['partial metadata']);
    if (kind === 'throw') throw Error('UI update failed');
    if (kind === 'thenable') return Promise.resolve(true);
  });
  assert.equal((await h.helper.acknowledge(h.ticket, input())).status, 'written-indeterminate');
  assert.equal(h.ready(), false);
  assert.deepEqual(h.effects, [['partial metadata'], ['indeterminate']]);
  await h.helper.acknowledge(h.ticket, input());
  assert.equal(h.effects.length, 2, 'no retry of an uncertain consumed UI commit');
});
test('atomic sink refusal has zero effects and does not force dirty state', async () => {
  const h = fixture(); h.setCommit(() => false);
  assert.equal((await h.helper.acknowledge(h.ticket, input())).status, 'written-stale');
  assert.deepEqual(h.effects, []);
});
test('replacement during failing callback is not dirtied by the previous UI attempt', async () => {
  const h = fixture(); h.setCommit(() => { h.setUI({ handle: {}, generation: 3 }); throw Error('UI changed'); });
  assert.equal((await h.helper.acknowledge(h.ticket, input())).status, 'written-indeterminate');
  assert.deepEqual(h.effects, []);
});
test('abort triggered by a currentness callback is observed before commit', async () => {
  const h = fixture(), controller = new AbortController();
  await h.helper.acknowledge(h.ticket, input(), { signal: controller.signal,
    isCurrent() { controller.abort(); return true; } });
  assert.deepEqual(h.effects, []);
});
test('invalid request IDs cannot manufacture an original UI attempt', () => {
  const h = fixture();
  assert.throws(() => h.helper.begin({ saveRequestId: 'same', serializerRequestId: 'same' }), /Distinct/);
  assert.throws(() => h.helper.begin({ saveRequestId: '', serializerRequestId: 's' }), /Distinct/);
});
test('foreign helper cannot acknowledge another controller ticket', async () => {
  const h = fixture(), other = fixture();
  await other.helper.acknowledge(h.ticket, input()); assert.deepEqual(other.effects, []);
});
test('publication flags are certified only by a completely correlated receipt and native bytes', async () => {
  const h = fixture();
  assert.equal((await h.helper.acknowledge({}, input())).written, null);
  assert.equal((await h.helper.acknowledge(h.ticket, {})).written, null);
  const bad = input(); bad.written.receipt.requestId = 'wrong-save';
  assert.equal((await h.helper.acknowledge(h.ticket, bad)).written, null);
  assert.deepEqual(h.effects, []);
});
for (const state of ['unpublished', 'cleanup', 'stale']) test(`${state} keeps distinct correlated disk evidence without UI commit`, async () => {
  const h = fixture(), value = input();
  value.written.eligibleCurrentSave = false;
  if (state === 'unpublished') { value.written.status = 'refused'; value.written.receipt.published = false; }
  if (state === 'cleanup') { value.written.status = 'published-cleanup-pending'; value.written.reason = 'cleanup'; value.written.receipt.cleanupPending = true; }
  if (state === 'stale') { value.written.status = 'published-stale'; value.written.reason = 'owner changed'; }
  const result = await h.helper.acknowledge(h.ticket, value);
  assert.equal(result.written, state !== 'unpublished');
  assert.equal(result.status, state === 'unpublished' ? 'not-published' : state === 'cleanup' ? 'written-cleanup-pending' : 'written-stale');
  assert.deepEqual(h.effects, []);
});
test('non-ASCII baseline uses native UTF-8 byte count rather than JavaScript UTF-16 length', async () => {
  const h = fixture(), value = input();
  assert.notEqual(A.length, value.written.receipt.byteCount);
  value.written.receipt.byteCount = A.length;
  const result = await h.helper.acknowledge(h.ticket, value);
  assert.equal(result.written, null); assert.equal(result.acknowledged, false);
  assert.deepEqual(h.effects, []);
});
test('actual E4A immutable publication feeds the private acknowledgement without another writer', async () => {
  const h = fixture(), flow = require('../src/js/application/native-object-save-as-workflow.js');
  let serialization, writes = 0;
  const written = await flow.run({ readPresentedObjectSession: () => h.current(),
    acquirePin(request) { serialization = input().serialization; assert.equal(request.requestId, ids.serializerRequestId); return serialization; },
    pickDestination: async () => '/owned/A.json',
    savePinned(request) { writes++; assert.equal(request.requestId, ids.saveRequestId); return input().written.receipt; }
  }, { requestId: ids.saveRequestId, pinRequestId: ids.serializerRequestId, isCurrent: () => h.helper.isCurrent(h.ticket) });
  assert.equal(written.status, 'published-current'); assert.equal(Object.isFrozen(written), true);
  assert.equal((await h.helper.acknowledge(h.ticket, { written, serialization })).status, 'acknowledged');
  assert.equal(writes, 1);
  assert.deepEqual(h.effects, [['commit', '/owned/A.json', A]]);
});
test('after-await UI reader cancellation is fenced before synchronous commit', async () => {
  const h = fixture(), controller = new AbortController(); let reads = 0;
  const original = h.ports.readUISession;
  // A new controller captures its fixed reader while the existing controller
  // fixture remains untouched, exercising callback binding and cancellation.
  const ports = { ...h.ports, readUISession() { reads++; if (reads === 3) controller.abort(); return original(); } };
  const helper = create(ports), ticket = helper.begin(ids);
  await helper.acknowledge(ticket, input(), { signal: controller.signal });
  assert.deepEqual(h.effects, []);
});
test('unknown/malformed receipt never certifies publication even if its flag says true', async () => {
  const h = fixture(), value = input(); value.written.receipt.extra = 'untrusted';
  assert.equal((await h.helper.acknowledge(h.ticket, value)).written, null);
  assert.deepEqual(h.effects, []);
});
test('a copied mutable UI record cannot turn generation drift into equality', async () => {
  const h = fixture(), original = h.ui(), gate = deferred(); h.setRead(() => gate.promise);
  const pending = h.helper.acknowledge(h.ticket, input()); original.generation++;
  gate.resolve(present()); await pending; assert.deepEqual(h.effects, []);
});
