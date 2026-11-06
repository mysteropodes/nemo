'use strict';
// The harness' own document reads (cs7-read.cjs): a stub MCP client, a disposable registry and stub identity /
// listener functions only. No app, no nemo-mcp process, no installed run.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createReader, readRegistration, OPERATION } = require('../cs7-read.cjs');
const { identity, INSTANCE, FOREIGN } = require('./cs7-h-stubs.cjs');

const BUILD = '1.2.3:abcdef';
function fixture(t, over = {}) {
  const registry = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'cs7-read-')));
  t.after(() => fs.rmSync(registry, { recursive: true, force: true }));
  const record = { instanceId: INSTANCE, port: 43210, secret: 'x'.repeat(36), buildId: BUILD, ...over.record };
  fs.writeFileSync(path.join(registry, record.instanceId + '.json'), JSON.stringify(record));
  const calls = [], state = { documentId: 'doc-1', contentRevision: 7, available: true, instanceId: INSTANCE, buildId: BUILD, ...over.state };
  const client = {
    async callTool(name, args) {
      calls.push(name === 'nemo_command' ? args.operation : name);
      if (name === 'nemo_discover') {
        return { result: { structuredContent: { instances: over.instances || [{ instanceId: state.instanceId, buildId: state.buildId,
          nativeHostStatus: { instanceId: state.instanceId, available: state.available, documentId: state.documentId, contentRevision: state.contentRevision, lifecycleGeneration: 2 } }] } } };
      }
      if (name !== 'nemo_command') throw new Error('unexpected tool ' + name);
      if (over.onCommand) over.onCommand(args, state);
      return { result: { structuredContent: { ok: true, instanceId: over.replyInstance || INSTANCE, documentId: over.replyDocument || state.documentId, result: { echoed: args.operation } } } };
    },
    close: async () => { calls.push('close'); },
  };
  const reader = createReader({ currentIdentity: over.current || (async () => ({ ...identity })), ownerIdentity: () => identity, registry: () => registry,
    appVersion: () => '1.2.3', openClient: async () => client, listeners: over.listeners || (async () => [identity.pid]) });
  return { reader, calls, state, registry };
}

test('a bound read returns the value and the status before and after, and issues only discover and query.* commands', async t => {
  const { reader, calls } = fixture(t);
  const { value, observed } = await reader.read(async ({ query, status }) => {
    assert.deepEqual([status.instanceId, status.documentId, status.contentRevision, status.lifecycleGeneration, status.pid], [INSTANCE, 'doc-1', 7, 2, identity.pid]);
    const a = await query('query.document.object.history', { atRevision: 7 });
    assert.equal(a.request.documentId, 'doc-1'); assert.equal(a.request.instanceId, INSTANCE); assert.equal(a.request.apiVersion, 2);
    return (await query('query.document.object.layer.timing', { x: 1 })).response.result.echoed;
  });
  assert.equal(value, 'query.document.object.layer.timing');
  assert.deepEqual(observed.before, observed.after);
  assert.equal(observed.reads, 2);
  assert.deepEqual(calls, ['nemo_discover', 'query.document.object.history', 'query.document.object.layer.timing', 'nemo_discover']);
});

test('only read-only query.* operations can be issued: command.*, legacy and malformed names are refused before anything is sent', async t => {
  for (const operation of ['command.document.object.undo', 'command.document.apply', 'Query.document.object', 'query', 'query.', 'query.document object', 'query.document.object; command.x',
    'nemo_command', '', 'queryx.document', undefined, null, 5, ['query.document.object']]) {
    const { reader, calls } = fixture(t);
    await assert.rejects(reader.read(async ({ query }) => { await query(operation, {}); }), /only read-only query operations/, String(operation));
    assert.ok(!calls.includes('nemo_command'), 'nothing sent for ' + String(operation));
  }
  assert.ok(OPERATION.test('query.document.object.component.read'));
  const { reader } = fixture(t);
  await assert.rejects(reader.read(async ({ query }) => query('query.document.object', 'not an object')), /invalid/);
  await assert.rejects(reader.read(async ({ query }) => query('query.document.object', { big: 'x'.repeat(9000) })), /too large/);
});

test('a reply from a different identity is refused: another process, instance, document, build or registry', async t => {
  const cases = {
    'another pid owns the endpoint': { listeners: async () => [identity.pid + 1] },
    'two processes listen': { listeners: async () => [identity.pid, identity.pid + 1] },
    'listener lookup fails': { listeners: async () => { throw new Error('lsof failed'); } },
    'owner identity differs (pid)': { current: async () => ({ ...identity, pid: identity.pid + 1 }) },
    'owner identity differs (executable)': { current: async () => ({ ...identity, executableSha256: 'e'.repeat(64) }) },
    'owner identity differs (source)': { current: async () => ({ ...identity, sourceSha: 'f'.repeat(40) }) },
    'discovered instance is not the registration': { state: { instanceId: FOREIGN } },
    'discovered build is not the registration': { state: { buildId: BUILD + 'x' } },
    'registered build is another version': { record: { buildId: '9.9.9:abcdef' }, state: { buildId: '9.9.9:abcdef' } },
    'two applications are discovered': { instances: [{ instanceId: INSTANCE }, { instanceId: FOREIGN }] },
    'no native document': { state: { available: false } },
    'a reply names another instance': { replyInstance: FOREIGN },
    'a reply names another document': { replyDocument: 'doc-2' },
  };
  for (const [why, over] of Object.entries(cases)) {
    const { reader } = fixture(t, over);
    await assert.rejects(reader.read(async ({ query }) => { await query('query.document.object', {}); return 1; }), Error, why);
  }
});

test('the document, revision, process or identity changing during a batch refuses the batch', async t => {
  let f = fixture(t, { onCommand: (args, state) => { state.contentRevision += 1; } });
  await assert.rejects(f.reader.read(async ({ query }) => { await query('query.document.object', {}); }), /changed during the read/);
  f = fixture(t, { onCommand: (args, state) => { state.documentId = 'doc-2'; } });
  await assert.rejects(f.reader.read(async ({ query }) => { await query('query.document.object', {}); }), /not from the candidate document|changed during the read/);
  f = fixture(t, { onCommand: (args, state) => { state.buildId = BUILD + 'x'; } });
  await assert.rejects(f.reader.read(async ({ query }) => { await query('query.document.object', {}); }), /not the candidate|not from the candidate/);
  let n = 0;
  f = fixture(t, { current: async () => ({ ...identity, pid: ++n > 1 ? identity.pid + 1 : identity.pid }) });
  await assert.rejects(f.reader.read(async () => 1), /candidate changed/);
});

test('the registry must hold exactly one well-formed record for the candidate', async t => {
  const { registry } = fixture(t);
  fs.writeFileSync(path.join(registry, FOREIGN + '.json'), JSON.stringify({ instanceId: FOREIGN, port: 1, secret: 'x'.repeat(36), buildId: BUILD }));
  assert.throws(() => readRegistration(registry), /exactly one/);
  fs.rmSync(path.join(registry, FOREIGN + '.json'));
  assert.equal(readRegistration(registry).instanceId, INSTANCE);
  fs.writeFileSync(path.join(registry, INSTANCE + '.json'), JSON.stringify({ instanceId: INSTANCE, port: '1', buildId: BUILD }));
  assert.throws(() => readRegistration(registry), /shape invalid/);
  fs.rmSync(path.join(registry, INSTANCE + '.json'));
  assert.throws(() => readRegistration(registry), /exactly one/);
});

test('reads do not nest or run after close, and close shuts the client', async t => {
  const { reader, calls } = fixture(t);
  await assert.rejects(reader.read(async () => reader.read(async () => 1)), /read unavailable/);
  assert.equal((await reader.read(async () => 'ok')).value, 'ok');
  await reader.close();
  assert.ok(calls.includes('close'));
  await assert.rejects(reader.read(async () => 1), /read unavailable/);
});
