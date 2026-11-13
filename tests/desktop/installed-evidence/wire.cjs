'use strict';
// The agent's only channel to the candidate: the package's own nemo-mcp over stdio JSON-RPC (tools nemo_discover and
// nemo_command). A Wire wraps an already initialized MCP client ({callTool, close}), refuses to talk to anything but the
// pinned candidate instance, judges every raw reply before a row sees it, and keeps an operation log (digests only).
//
// Failure kinds, all `EvidenceError`: `foreign` (the reply or the discovered instance is not the candidate, fatal for the
// run), `malformed` (the reply is not a well formed transport reply), `unavailable` (the operation is not offered, the row
// is `unrun`), `transport` (the client failed; fatal). A refusal by the product is NOT an error here: it is an observation
// (`ok: false`) that the row asserts on.
const crypto = require('node:crypto');
const { randomUUID } = require('node:crypto');
const { structured } = require('../cs7-read.cjs');

class EvidenceError extends Error {
  constructor(kind, message) { super(message); this.name = 'EvidenceError'; this.kind = kind; }
}
const plain = value => !!value && typeof value === 'object' && !Array.isArray(value);
const safeInt = value => Number.isSafeInteger(value) && value >= 0;

function canonical(value) {
  if (Array.isArray(value)) return '[' + value.map(canonical).join(',') + ']';
  if (plain(value)) return '{' + Object.keys(value).sort().map(k => JSON.stringify(k) + ':' + canonical(value[k])).join(',') + '}';
  return JSON.stringify(value);
}
const digest = value => crypto.createHash('sha256').update(canonical(value)).digest('hex');

// The structured reply of a tool call, or a malformed refusal. nemo-mcp itself answers `{ok:false,error}` for requests
// it refuses before dispatch; those carry no instance fields and are returned as `frontDoor`.
function replyOf(response) {
  let content;
  try { content = structured(response); } catch { throw new EvidenceError('malformed', 'tool call returned no structured reply'); }
  if (!plain(content)) throw new EvidenceError('malformed', 'reply is not an object');
  return content;
}

function createWire({ client, expect, now = () => Date.now(), sink = () => {} }) {
  if (!client || typeof client.callTool !== 'function') throw new EvidenceError('transport', 'an MCP client is required');
  const log = [];
  let sequence = 0, state = null;

  async function call(tool, args) {
    try { return await client.callTool(tool, args); } catch (error) {
      throw new EvidenceError('transport', 'MCP client failed: ' + (error && error.message ? String(error.message).slice(0, 120) : 'unknown'));
    }
  }

  // The one application, bound to the candidate. `expect` = {instanceId, buildId} from the verified registry record.
  async function discover() {
    const found = replyOf(await call('nemo_discover', {}));
    if (!Array.isArray(found.instances) || found.instances.length !== 1) throw new EvidenceError('foreign', 'exactly one discovered application required');
    const [instance] = found.instances, native = instance && instance.nativeHostStatus;
    if (!plain(instance) || typeof instance.instanceId !== 'string' || typeof instance.buildId !== 'string') throw new EvidenceError('malformed', 'discovery record malformed');
    if (expect && (instance.instanceId !== expect.instanceId || instance.buildId !== expect.buildId)) {
      throw new EvidenceError('foreign', 'discovered application is not the candidate');
    }
    const hasDocument = plain(native) && native.available === true && typeof native.documentId === 'string' && native.documentId.length > 0;
    if (hasDocument && (native.instanceId !== instance.instanceId || !safeInt(native.contentRevision))) throw new EvidenceError('malformed', 'native host status malformed');
    state = { instanceId: instance.instanceId, buildId: instance.buildId,
      documentId: hasDocument ? native.documentId : null, contentRevision: hasDocument ? native.contentRevision : null,
      lifecycleGeneration: hasDocument && Number.isSafeInteger(native.lifecycleGeneration) ? native.lifecycleGeneration : null };
    return { ...state };
  }

  // Send one operation. version 1: project.* through nemo_command; version 2: command.* / query.* / job.*.
  async function send(version, operation, payload, options = {}) {
    // Every request is built from a fresh discovery, so the revision it expects and the instance it names are read from
    // the candidate itself, never remembered.
    await discover();
    const request = { apiVersion: version, requestId: options.requestId || randomUUID(), instanceId: state.instanceId, operation, payload };
    const write = options.write === true;
    if (state.documentId && !options.noDocument) request.documentId = state.documentId;
    if (write && state.documentId && !options.noDocument) request.expectedRevision = options.expectedRevision ?? state.contentRevision;
    // The request-level cancellation of the v2 envelope (nemo-mcp contract.rs NativeApplicationRequest.cancelled_before_dispatch): the
    // owner must answer a typed `cancelled_before_dispatch` refusal and publish nothing.
    if (options.cancelledBeforeDispatch === true) request.cancelledBeforeDispatch = true;
    const started = now();
    const reply = replyOf(await call('nemo_command', request));
    const entry = { seq: ++sequence, version, operation, requestDigest: digest(request), replyDigest: digest(reply),
      ok: reply.ok === true, errorCode: plain(reply.error) && typeof reply.error.code === 'string' ? reply.error.code : null,
      revision: version === 2 ? reply.contentRevision ?? null : reply.revision ?? null, ms: now() - started };
    log.push(entry);
    sink({ ...entry, request, reply });
    return judge(version, request, reply, options);
  }

  function judge(version, request, reply, options) {
    if (typeof reply.ok !== 'boolean') throw new EvidenceError('malformed', 'reply has no boolean ok');
    const frontDoor = reply.apiVersion === undefined && reply.requestId === undefined && reply.instanceId === undefined;
    if (frontDoor) {
      // nemo-mcp's own refusal before anything reached the application.
      if (reply.ok || !plain(reply.error) || typeof reply.error.code !== 'string') throw new EvidenceError('malformed', 'front-door reply malformed');
      const code = reply.error.code;
      if (code === 'unsupported_capability' || (code === 'unavailable' && !plain(reply.error.details))) {
        throw new EvidenceError('unavailable', request.operation + ' is not available: ' + String(reply.error.message || code).slice(0, 160));
      }
      return { ok: false, error: reply.error, frontDoor: true, request, reply };
    }
    if (reply.apiVersion !== version || reply.requestId !== request.requestId) throw new EvidenceError('malformed', 'reply does not answer this request');
    if (reply.instanceId !== request.instanceId) throw new EvidenceError('foreign', 'reply is from another instance');
    const projectEntry = version === 1 && (request.operation === 'project.new' || request.operation === 'project.open');
    // A refusal that names no document at all ("No native project is open.", documentId "") says no project is open: it is the app
    // answering for the document that was released, not another document answering.
    const noProject = reply.ok === false && reply.documentId === '' && plain(reply.error) && reply.error.code === 'unavailable' && reply.error.message === 'No native project is open.';
    if (!projectEntry && !noProject && request.documentId !== undefined && reply.documentId !== request.documentId) throw new EvidenceError('foreign', 'reply is from another document');
    const revision = version === 2 ? reply.contentRevision : reply.revision;
    if (!safeInt(revision)) throw new EvidenceError('malformed', 'reply carries no revision');
    if (reply.ok === true) {
      if (!plain(reply.result) || reply.error !== undefined) throw new EvidenceError('malformed', 'ok reply has no result object');
      return { ok: true, result: reply.result, revision, request, reply };
    }
    if (!plain(reply.error) || typeof reply.error.code !== 'string' || typeof reply.error.message !== 'string' || reply.result !== undefined) {
      throw new EvidenceError('malformed', 'refusal has no typed error');
    }
    if (reply.error.code === 'unsupported_capability') throw new EvidenceError('unavailable', request.operation + ' is not supported');
    return { ok: false, error: reply.error, revision, request, reply };
  }

  return {
    discover,
    v1: (operation, payload, options = {}) => send(1, operation, payload, { write: true, ...options }),
    command: (operation, payload, options = {}) => send(2, operation, payload, { write: true, ...options }),
    query: (operation, payload, options = {}) => send(2, operation, payload, { ...options, write: false }),
    operations: () => log.map(entry => ({ ...entry })),
    mark: () => log.length,
    since: mark => log.slice(mark).map(entry => ({ ...entry })),
    state: () => (state ? { ...state } : null),
  };
}

module.exports = { createWire, EvidenceError, digest, canonical, plain, safeInt };
