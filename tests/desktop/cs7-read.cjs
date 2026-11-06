'use strict';
// Harness-issued, read-only document reads for the installed rows. A row may pass only on observations the
// harness made itself of the candidate under test, so the harness (never the operator) asks the running app
// for document state, through the route any MCP client uses: the bundled `nemo-mcp` process, pointed at the
// candidate's own isolated registry, calling `nemo_discover` and `nemo_command` (native v2 `query.*`).
//
// What binds a reply to the candidate. The public route exposes no pid or executable hash, only the registry
// record (instanceId, port, buildId) and the native host status (instanceId, documentId, contentRevision,
// lifecycleGeneration). Without a Rust change the strongest available binding is therefore built from these:
//   1. every read batch is preceded and followed by the owner identity read (pid, sourceSha, tree, driverSha,
//      executableSha256 from the owner status) and by a registry/status read; any difference from the identity
//      captured at start refuses the batch;
//   2. the registry is the candidate's own task-scoped directory and must hold exactly ONE record;
//   3. the discovered instance must be that record (instanceId, buildId, whose version part is the candidate's
//      app version) and its native host must be available;
//   4. the OS-level listener on the record's port must be exactly the candidate's pid (lsof), which ties the
//      MCP endpoint to the owner pid even though the route itself does not report one;
//   5. every ok reply must carry the same instanceId and documentId, and the document revision, lifecycle
//      generation and build id must be identical before and after the batch.
// A reply from another running Nemo can satisfy none of these. Any mismatch throws (fail closed).
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { randomUUID } = require('node:crypto');
const { isDeepStrictEqual: same } = require('node:util');

function check(value, message) { if (!value) throw new Error(message); }
const plain = value => !!value && typeof value === 'object' && !Array.isArray(value);
const OPERATION = /^query(\.[a-z][a-zA-Z0-9]*){1,6}$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const MAX_READS = 96, MAX_REQUEST = 8 * 1024;

function structured(response) {
  check(plain(response) && plain(response.result) && !response.error, 'MCP route returned no result');
  if (plain(response.result.structuredContent)) return response.result.structuredContent;
  for (const item of response.result.content || []) {
    if (item?.type === 'text') { try { const value = JSON.parse(item.text); if (plain(value)) return value; } catch { /* next block */ } }
  }
  throw new Error('MCP route returned no structured content');
}

// Pids listening on a loopback TCP port, from the operating system rather than from the app.
function listenerPids(port) {
  check(Number.isSafeInteger(port) && port > 0 && port < 65536, 'registry port invalid');
  const out = execFileSync('/usr/sbin/lsof', ['-nP', '-iTCP:' + port, '-sTCP:LISTEN', '-Fp'], { encoding: 'utf8', timeout: 5000 });
  return [...new Set(out.split('\n').filter(line => /^p[1-9][0-9]*$/.test(line)).map(line => Number(line.slice(1))))];
}

// One record, strict shape. The directory is the candidate's own isolated registry.
function readRegistration(directory) {
  check(fs.realpathSync(directory) === directory && fs.lstatSync(directory).isDirectory(), 'candidate registry directory required');
  const names = fs.readdirSync(directory).filter(name => name.endsWith('.json'));
  check(names.length === 1, 'exactly one candidate registration required');
  const file = path.join(directory, names[0]), stat = fs.lstatSync(file);
  check(stat.isFile() && !stat.isSymbolicLink() && stat.size > 0 && stat.size <= 4096, 'candidate registration invalid');
  const record = JSON.parse(fs.readFileSync(file, 'utf8'));
  check(plain(record) && UUID.test(record.instanceId) && names[0] === record.instanceId + '.json' &&
    Number.isSafeInteger(record.port) && typeof record.buildId === 'string' && record.buildId.length > 0, 'candidate registration shape invalid');
  return { instanceId: record.instanceId, port: record.port, buildId: record.buildId };
}

// currentIdentity(): the owner status read (async, throws on any difference from the checkout/package).
// ownerIdentity(): the identity captured when the session started.
// registry(): absolute path of the candidate's registry directory. appVersion(): the candidate's app version.
// openClient(): async, returns an initialized MCP client ({callTool, close}); called once, lazily.
function createReader({ currentIdentity, ownerIdentity, registry, appVersion, openClient, listeners = listenerPids }) {
  let client = null, busy = false, closed = false;
  async function bind() {
    const identity = await currentIdentity();
    check(same(identity, ownerIdentity()), 'installed candidate changed');
    const record = readRegistration(registry());
    check(record.buildId.startsWith(appVersion() + ':'), 'registered build is not the candidate build');
    client ||= await openClient();
    const found = structured(await client.callTool('nemo_discover', {}));
    check(Array.isArray(found.instances) && found.instances.length === 1, 'exactly one discovered application required');
    const [instance] = found.instances, native = instance?.nativeHostStatus;
    check(plain(instance) && instance.instanceId === record.instanceId && instance.buildId === record.buildId, 'discovered application is not the candidate');
    check(plain(native) && native.instanceId === record.instanceId && native.available === true && typeof native.documentId === 'string' &&
      native.documentId.length > 0 && Number.isSafeInteger(native.contentRevision), 'candidate has no open native document');
    const pids = await listeners(record.port);
    check(Array.isArray(pids) && pids.length === 1 && pids[0] === identity.pid, 'MCP endpoint is not owned by the candidate process');
    return { instanceId: record.instanceId, buildId: record.buildId, documentId: native.documentId, contentRevision: native.contentRevision,
      lifecycleGeneration: Number.isSafeInteger(native.lifecycleGeneration) ? native.lifecycleGeneration : null, pid: identity.pid };
  }
  return {
    // fn({ query, status }) -> value. `query(operation, payload)` issues ONE read-only `query.*` operation and
    // returns { request, response } with the request exactly as sent. Nothing else can be sent.
    async read(fn) {
      check(typeof fn === 'function' && !closed && !busy, 'read unavailable');
      busy = true;
      try {
        const before = await bind();
        let issued = 0;
        const query = async (operation, payload = {}) => {
          check(typeof operation === 'string' && OPERATION.test(operation), 'only read-only query operations may be issued');
          check(plain(payload) && issued < MAX_READS, 'read request invalid or too many reads');
          const request = { apiVersion: 2, requestId: randomUUID(), instanceId: before.instanceId, documentId: before.documentId, operation, payload };
          check(Buffer.byteLength(JSON.stringify(request)) <= MAX_REQUEST, 'read request too large');
          issued += 1;
          const response = structured(await client.callTool('nemo_command', request));
          check(response.ok !== true || (response.instanceId === before.instanceId && response.documentId === before.documentId),
            'reply is not from the candidate document');
          return { request: structuredClone(request), response };
        };
        const value = await fn({ query, status: { ...before } });
        const after = await bind();
        check(same(before, after), 'candidate document or process changed during the read');
        return { value, observed: { before, after, reads: issued } };
      } finally { busy = false; }
    },
    async close() {
      closed = true;
      if (client) { const c = client; client = null; await c.close(); }
    },
  };
}

module.exports = { createReader, listenerPids, readRegistration, structured, OPERATION };
