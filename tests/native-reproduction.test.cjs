'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const reproduction = require('../src/js/adapters/native-reproduction.js');
const application = require('../src/js/adapters/native-application.js');
const fixture = { id: 'native-opacity-static', version: 1, sha256: '895ca05a43295104301c472287149b1c01aa6ef681db80dcbb79e58a55b36050' };
function bundle() {
  return { format: 'nemo.native-opacity-reproduction', formatVersion: 1, apiVersion: 2, fixture: { ...fixture },
    command: 'layer.opacity.set', stableTarget: { layerUid: 'r08_curve_layer' }, clock: null, seed: null,
    versions: { nativeEngine: '0.1.0' }, commands: [{ id: 1, expectedRevision: 0, value: 40, revision: 1, applied: true }] };
}
function host() {
  const calls = [], downloads = [], revoked = [], timers = new Map();
  const status = { apiVersion: 2, instanceId: 'instance', documentId: null, lifecycleGeneration: 0, contentRevision: 0, available: false };
  const root = { NemoNativeApplicationAdapter: application, Blob,
    setTimeout(fn) { timers.set(timers.size + 1, fn); return timers.size; }, clearTimeout(id) { timers.delete(id); },
    URL: { createObjectURL(blob) { downloads.push(blob); return 'blob:owned'; }, revokeObjectURL(url) { revoked.push(url); } },
    document: { createElement() { return { click() {}, remove() {} }; }, body: { appendChild() {} } } };
  let invoke = async (command, args) => {
    calls.push({ command, args });
    if (command === 'nemo_native_status') return { ...status };
    if (command === 'nemo_native_revision_sync') { assert.deepEqual(args.request, { action: 'binding' }); return { ...status, subscriptionId: 'read-binding' }; }
    if (command === 'nemo_mcp_identity') return { instanceId: 'instance' };
    if (command === 'nemo_native_reproduction_session') {
      assert.deepEqual(args.request, { apiVersion: 2, instanceId: 'instance', optIn: true, fixture });
      Object.assign(status, { available: true, documentId: 'catalog', lifecycleGeneration: status.lifecycleGeneration + 1, contentRevision: 0 });
      return { apiVersion: 2, instanceId: status.instanceId, documentId: status.documentId, contentRevision: 0, lifecycleGeneration: status.lifecycleGeneration,
        origin: 'embedded_catalog', fixture, reproduction: { state: 'recording', reason: null, commandCount: 0, exportable: false }, viewportAvailable: false, resourceCount: 0 };
    }
    if (command === 'nemo_native_dispatch') {
      const r = args.request;
      assert.equal(r.documentId, 'catalog'); assert.equal(r.operation, 'command.document.apply');
      status.contentRevision++;
      return { apiVersion: 2, requestId: r.requestId, instanceId: r.instanceId, documentId: r.documentId, contentRevision: status.contentRevision, ok: true, result: { applied: true, historyEntriesAdded: 1 } };
    }
    assert.equal(command, 'nemo_native_release');
    Object.assign(status, { available: false, documentId: null, lifecycleGeneration: status.lifecycleGeneration + 1 });
    return { apiVersion: 2, requestId: args.request.requestId, instanceId: 'instance', documentId: 'catalog', contentRevision: status.contentRevision,
      lifecycleGeneration: status.lifecycleGeneration, status: 'succeeded', authorityRemovalCompleted: true, reentryAvailable: true };
  };
  root.__TAURI__ = { core: { invoke: (...args) => invoke(...args) } };
  return { root, calls, status, downloads, revoked, flush() { for (const fn of [...timers.values()]) fn(); }, setInvoke(fn) { const prior = invoke; invoke = fn; return prior; } };
}
test('catalog requires explicit opt-in and idle authority; no ordinary import or browser admission', async () => {
  const h = host();
  assert.equal(reproduction.capture(h.root), null);
  await assert.rejects(reproduction.start(h.root, false)); assert.equal(h.calls.length, 0);
  h.status.available = true; h.status.documentId = 'ordinary';
  await assert.rejects(reproduction.start(h.root, true));
  assert.ok(h.calls.every(c => c.command === 'nemo_native_status'));
  await assert.rejects(reproduction.start({}, true));
});
test('explicit session edits and release use its exact native identity and generation', async () => {
  const h = host(); const identity = await reproduction.start(h.root, true);
  assert.equal(reproduction.capture(h.root), identity);
  await reproduction.setOpacity(h.root, identity, 40, 'edit');
  const r = h.calls.find(c => c.command === 'nemo_native_dispatch').args.request;
  assert.equal(r.expectedRevision, 0); assert.deepEqual(r.payload, { command: 'layer.opacity.set', stableTarget: { layerUid: 'r08_curve_layer' }, value: 40 });
  await reproduction.end(h.root, identity, 'end'); assert.equal(reproduction.capture(h.root), null);
  const next = await reproduction.start(h.root, true); assert.notEqual(next, identity); assert.ok(next.generation > identity.generation);
  await assert.rejects(reproduction.setOpacity(h.root, identity, 60, 'stale'));
});
test('replacement and generation changes fence writes and release before dispatch', async () => {
  for (const key of ['documentId', 'instanceId', 'lifecycleGeneration']) {
    const h = host(); const identity = await reproduction.start(h.root, true);
    h.status[key] = key === 'lifecycleGeneration' ? 3 : 'replacement';
    await assert.rejects(reproduction.setOpacity(h.root, identity, 40, 'edit'));
    await assert.rejects(reproduction.end(h.root, identity, 'end'));
    assert.ok(h.calls.every(c => !['nemo_native_dispatch', 'nemo_native_release'].includes(c.command)));
  }
});
test('bundle is closed, bounded native v2, with independently checked command continuity', () => {
  assert.equal(reproduction.validateBundle(bundle()), true);
  for (const mutate of [b => b.privatePath = '/private/artist', b => b.fixture.sha256 = 'a'.repeat(64), b => b.apiVersion = 1,
    b => b.commands = [], b => b.commands[0].revision = 8, b => b.commands[0].applied = false,
    b => b.commands[0].requestId = 'private', b => b.clock = 1, b => b.versions.nativeEngine = '/private/secret',
    b => b.commands = Array.from({ length: 33 }, () => b.commands[0])]) {
    const b = bundle(); mutate(b); assert.throws(() => reproduction.validateBundle(b));
  }
  const b = bundle(); b.commands.push({ id: 2, expectedRevision: 1, value: 40, revision: 1, applied: false });
  assert.equal(reproduction.validateBundle(b), true);
  const large = bundle(); large.commands = Array.from({ length: 32 }, (_, i) => ({ id: i + 1, expectedRevision: i ? 1 : 0,
    value: 50.12345678912345, revision: 1, applied: i === 0 }));
  assert.ok(Buffer.byteLength(JSON.stringify(large)) > 3072);
  assert.throws(() => reproduction.validateBundle(large), /byte limit/);
});
test('delayed admission is never repeated and a retired binding cannot become a current session', async () => {
  const h = host(); let complete;
  const prior = h.setInvoke(async (command, args) => {
    if (command !== 'nemo_native_reproduction_session') return prior(command, args);
    const result = await prior(command, args);
    return new Promise(resolve => { complete = () => resolve(result); });
  });
  const pending = reproduction.start(h.root, true); await new Promise(resolve => setImmediate(resolve));
  await assert.rejects(reproduction.start(h.root, true));
  assert.equal(h.calls.filter(c => c.command === 'nemo_native_reproduction_session').length, 1);
  h.status.lifecycleGeneration += 2; complete(); await assert.rejects(pending);
  assert.equal(reproduction.capture(h.root), null);
});
test('binding race and in-flight operation fence all later writes and End', async () => {
  const h = host(); const identity = await reproduction.start(h.root, true); let complete;
  const prior = h.setInvoke(async (command, args) => {
    if (command !== 'nemo_native_revision_sync') return prior(command, args);
    return new Promise(resolve => { complete = () => resolve({ ...h.status, subscriptionId: 'binding' }); });
  });
  const pending = reproduction.setOpacity(h.root, identity, 40, 'first'); await new Promise(resolve => setImmediate(resolve));
  await assert.rejects(reproduction.setOpacity(h.root, identity, 50, 'second'));
  await assert.rejects(reproduction.end(h.root, identity, 'end'));
  h.status.lifecycleGeneration += 2; complete(); await assert.rejects(pending);
  assert.equal(h.calls.filter(c => c.command === 'nemo_native_dispatch').length, 0);
});
test('retired host binding clears only the owned token; transient binding failure retains it', async () => {
  for (const retired of [false, true]) {
    const h = host(); const identity = await reproduction.start(h.root, true);
    const prior = h.setInvoke((command, args) => command === 'nemo_native_revision_sync'
      ? Promise.reject(new Error('binding unavailable')) : prior(command, args));
    if (retired) { h.status.available = false; h.status.documentId = null; }
    await assert.rejects(reproduction.check(h.root, identity));
    assert.equal(reproduction.capture(h.root), retired ? null : identity);
    assert.equal(h.calls.filter(c => c.command === 'nemo_native_release').length, 0);
  }
});
test('download validates before allocation and always removes/revokes its own object URL', async () => {
  const h = host(); reproduction.download(h.root, bundle(), () => true);
  assert.equal(h.downloads.length, 1); assert.deepEqual(h.revoked, []); h.flush(); assert.deepEqual(h.revoked, ['blob:owned']);
  assert.deepEqual(JSON.parse(await h.downloads[0].text()), bundle());
  assert.throws(() => reproduction.download(h.root, { ...bundle(), private: 'sentinel' }, () => true));
  assert.throws(() => reproduction.download(h.root, bundle(), () => false)); assert.equal(h.downloads.length, 1);
  h.root.document.createElement = () => ({ click() { throw new Error('blocked'); }, remove() {} });
  assert.throws(() => reproduction.download(h.root, bundle(), () => true)); assert.equal(h.revoked.length, 2);
  h.root.document.createElement = () => ({ click() {}, remove() {} });
  reproduction.download(h.root, bundle(), () => true); reproduction.disposeDownloads(h.root);
  assert.equal(h.revoked.length, 3); h.flush(); assert.equal(h.revoked.length, 3);
});
