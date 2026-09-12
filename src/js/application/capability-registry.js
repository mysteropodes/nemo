// Deterministic capability registration (P05/#1007).
//
// The join `handlerKey` already promises in capability-v1.schema.json: a
// feature module registers its own descriptor together with its own handler,
// and every consumer (UI binding, JS application service, Rust MCP dispatch —
// P07/#1009) discovers the same set in the same order.
//
// Two things this deliberately does NOT do:
//
//   * It does not own handlers. A handler stays inside the feature that
//     implements it; the registry holds the reference it was handed and never
//     re-implements or wraps behaviour.
//   * It does not re-validate the whole descriptor against
//     capability-v1.schema.json. That conformance already has an owner
//     (tests/capability-v1-schema.test.cjs, with the JSON-Schema-subset
//     validator P04 shipped), and a second copy of that validator living here
//     would be exactly the two-divergent-copies trap CLAUDE.md §3 warns about.
//     What this file enforces is what *registration* requires and a schema
//     cannot express: identity uniqueness, a supported schema version, an
//     actually-present handler, and an availability claim that is true.
//
// On `effects.kind` / `effects.scope` (recorded here because it is easy to
// misread once D02/#1045 lands): those two fields classify the capability AS A
// WHOLE — the union / worst case across every stage in its `lifecycle`. They
// are NOT a per-stage claim. `opacity.json` declares `kind:"mutation"` while
// its own lifecycle also contains the read stage `property.get`; that is
// correct, not a bug. A per-stage lifecycle contract (D02's `JobLifecycle`)
// must not read these as already stage-scoped.
var NemoCapabilityRegistry = (function () {
  'use strict';

  var SUPPORTED_SCHEMA_VERSION = 1;

  // Raw pixel/geometry payloads are passed as a handle, never inlined into
  // command JSON (execution plan P05 outcome check 3; the vocabulary is
  // D02's ResourceHandle). Detected structurally rather than by size: a
  // small data: URI is just as wrong as a large one, and a byte-length
  // threshold would silently start accepting the same mistake below it.
  var OPAQUE_KEYS = ['imageData', 'pixels', 'bitmap', 'geometry', 'vertices', 'segments', 'buffer'];

  function isObject(value) { return !!value && typeof value === 'object' && !Array.isArray(value); }

  function fail(code, message, detail) {
    var error = new Error(message);
    error.code = code;
    if (detail) error.detail = detail;
    return error;
  }

  // Every rejection carries a distinct `code`, so a caller (and a test) can
  // tell WHICH rule failed instead of matching on message text.
  function describeDescriptor(descriptor) {
    if (!isObject(descriptor)) throw fail('descriptor_invalid', 'A capability descriptor must be an object.');
    if (descriptor.schemaVersion !== SUPPORTED_SCHEMA_VERSION) {
      throw fail('schema_version_unsupported',
        'Unsupported capability schemaVersion: ' + JSON.stringify(descriptor.schemaVersion)
          + ' (this registry speaks only ' + SUPPORTED_SCHEMA_VERSION + ').',
        { schemaVersion: descriptor.schemaVersion });
    }
    if (typeof descriptor.id !== 'string' || !descriptor.id) {
      throw fail('descriptor_invalid', 'A capability descriptor must carry a non-empty string id.');
    }
    if (typeof descriptor.handlerKey !== 'string' || !descriptor.handlerKey) {
      throw fail('descriptor_invalid', 'Capability "' + descriptor.id + '" declares no handlerKey.');
    }
    // "missing schema" in the outcome check: a descriptor that declares no
    // input/output shape cannot be dispatched to, so it is refused at
    // registration rather than failing later at call time.
    if (!('input' in descriptor) || !('output' in descriptor)) {
      throw fail('descriptor_schema_missing',
        'Capability "' + descriptor.id + '" declares no input/output schema.');
    }
    if (!isObject(descriptor.availability) || typeof descriptor.availability.state !== 'string') {
      throw fail('descriptor_invalid', 'Capability "' + descriptor.id + '" declares no availability.state.');
    }
    return descriptor;
  }

  function create() {
    var byId = Object.create(null);
    var handlersByKey = Object.create(null);
    var handles = Object.create(null);
    var handleSeq = 0;

    function register(descriptor, handler) {
      describeDescriptor(descriptor);
      if (Object.prototype.hasOwnProperty.call(byId, descriptor.id)) {
        throw fail('duplicate_id', 'Capability id "' + descriptor.id + '" is already registered.',
          { id: descriptor.id });
      }
      if (handler !== undefined && typeof handler !== 'function') {
        throw fail('handler_invalid',
          'Capability "' + descriptor.id + '" was registered with a non-function handler.',
          { handlerKey: descriptor.handlerKey });
      }
      var existingHandler = handlersByKey[descriptor.handlerKey];
      if (typeof handler === 'function' && typeof existingHandler === 'function' && existingHandler !== handler) {
        throw fail('handler_key_conflict',
          'Capability "' + descriptor.id + '" tried to replace the handler already registered for handlerKey "'
            + descriptor.handlerKey + '".',
          { handlerKey: descriptor.handlerKey });
      }
      // THE semantic availability check, and the reason this registry exists
      // as more than a map. A descriptor asserting `{state:"available"}` is
      // making a claim about the running system, not picking an enum value —
      // and nothing in capability-v1.schema.json can tell the difference.
      // Found reviewing P04/#1086: flipping export-job.json back to
      // "available" still passed every schema test, because it IS a valid
      // enum value. Registration is the first moment the claim meets reality.
      //
      // A descriptor may therefore register WITHOUT a handler — that is how a
      // not-yet-implemented capability (export-job.json today) declares
      // itself honestly — but only while it says `unavailable`. Claiming
      // `available` with nothing to dispatch to is refused.
      var bound = typeof handler === 'function' || typeof existingHandler === 'function';
      if (descriptor.availability.state === 'available' && !bound) {
        throw fail('availability_unbacked',
          'Capability "' + descriptor.id + '" claims availability "available" with no handler registered for handlerKey "'
            + descriptor.handlerKey + '".',
          { handlerKey: descriptor.handlerKey });
      }
      byId[descriptor.id] = { descriptor: descriptor, handlerKey: descriptor.handlerKey };
      if (typeof handler === 'function' && typeof existingHandler !== 'function') {
        handlersByKey[descriptor.handlerKey] = handler;
      }
      return descriptor.id;
    }

    // Deterministic by construction: sorted by id, so discovery does not
    // depend on the order feature modules happened to load in.
    function ids() {
      return Object.keys(byId).sort();
    }

    function list() {
      return ids().map(function (id) {
        return { id: id, handlerKey: byId[id].handlerKey,
          bound: typeof handlersByKey[byId[id].handlerKey] === 'function', descriptor: byId[id].descriptor };
      });
    }

    function get(id) {
      var entry = byId[id];
      if (!entry) throw fail('unknown_capability', 'No capability registered with id "' + id + '".', { id: id });
      return entry.descriptor;
    }

    function handlerFor(id) {
      var entry = byId[id];
      if (!entry) throw fail('unknown_capability', 'No capability registered with id "' + id + '".', { id: id });
      var handler = handlersByKey[entry.handlerKey];
      if (typeof handler !== 'function') {
        throw fail('handler_missing',
          'Capability "' + id + '" declares handlerKey "' + entry.handlerKey + '" but no handler is registered for it.',
          { handlerKey: entry.handlerKey });
      }
      return handler;
    }

    // --- opaque data (D02 ResourceHandle vocabulary) -----------------------

    function acquireHandle(ownerId, resource) {
      if (typeof ownerId !== 'string' || !ownerId) {
        throw fail('invalid_request', 'A handle requires the requestId or jobId acquiring it.');
      }
      handleSeq += 1;
      var handleId = 'handle-' + handleSeq;
      handles[handleId] = { handleId: handleId, ownerId: ownerId, disposition: 'held', resource: resource };
      return { handleId: handleId, ownerId: ownerId, disposition: 'held' };
    }

    function releaseHandle(handleId) {
      var held = handles[handleId];
      if (!held) throw fail('invalid_request', 'Unknown handle "' + handleId + '".', { handleId: handleId });
      held.disposition = 'released';
      delete held.ownerId;
      held.resource = null;
      return { handleId: handleId, disposition: 'released' };
    }

    function resolveHandle(handleId) {
      var held = handles[handleId];
      if (!held || held.disposition !== 'held') {
        throw fail('invalid_request', 'Handle "' + handleId + '" is not held.', { handleId: handleId });
      }
      return held.resource;
    }

    // Rejects a command payload that inlines raw image/geometry data instead
    // of naming a handle. Returns the payload unchanged when it is clean, so
    // a caller can use it inline.
    function assertNoInlineOpaque(payload) {
      if (!isObject(payload)) return payload;
      Object.keys(payload).forEach(function (key) {
        var value = payload[key];
        var looksOpaque = OPAQUE_KEYS.indexOf(key) !== -1
          && (Array.isArray(value) || (typeof value === 'string' && value.slice(0, 5) === 'data:'));
        if (looksOpaque) {
          throw fail('opaque_inline',
            'Payload field "' + key + '" carries raw image/geometry data; pass a handleId instead.',
            { field: key });
        }
      });
      return payload;
    }

    return {
      register: register, list: list, ids: ids, get: get, handlerFor: handlerFor,
      acquireHandle: acquireHandle, releaseHandle: releaseHandle, resolveHandle: resolveHandle,
      assertNoInlineOpaque: assertNoInlineOpaque,
    };
  }

  return { create: create, SUPPORTED_SCHEMA_VERSION: SUPPORTED_SCHEMA_VERSION, OPAQUE_KEYS: OPAQUE_KEYS };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoCapabilityRegistry;
