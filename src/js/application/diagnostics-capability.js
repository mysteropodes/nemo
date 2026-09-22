// The shared diagnostics inspector's own capability registration (T08/#1057).
//
// Bounded to a descriptor and its registration contract, not a bootstrap
// integration: opacity-application.js's diagnostics.trace dispatch (T05)
// is untouched, and nothing here wires this into the live
// NemoApplication.handle dispatch -- same convention as
// timelapse-capability.js (P31), that leaf's closest sibling.
//
// This synchronous classic-script projection is kept byte-for-value
// equivalent to engineering/application/capabilities/diagnostics.json by
// the focused contract test, same convention as opacity-capability.js.
var NemoDiagnosticsCapability = (function () {
  'use strict';

  function loaded(win) {
    return !!(win && win.NemoApplication && typeof win.NemoApplication.handle === 'function'
      && win.NemoOpacityApplication && typeof win.NemoOpacityApplication.meta === 'function');
  }

  // Live environment check for a caller to pass into register()'s optional
  // third argument at actual boot time. Same "not re-evaluated after
  // registration" limitation documented in timelapse-capability.js: a
  // labs flag toggled after boot leaves a registered claim stale until
  // reload.
  function computeAvailability(win) {
    return loaded(win) ? { state: 'available', reason: null } : { state: 'unavailable', reason: 'missing-dependency' };
  }

  var DESCRIPTOR = {
    schemaVersion: 1,
    id: 'diagnostics',
    version: 1,
    handlerKey: 'application.diagnostics.inspect',
    input: {
      type: 'object',
      additionalProperties: false,
      properties: {
        limit: { type: 'integer', minimum: 1, maximum: 32,
          description: 'Most recent N entries only. Omit for every retained entry (bounded to the 32-entry retention window regardless).' },
      },
    },
    output: {
      type: 'object',
      additionalProperties: false,
      properties: {
        entries: {
          type: 'array',
          items: {
            type: 'object',
            additionalProperties: false,
            properties: {
              requestId: { type: 'string' },
              operation: { type: 'string' },
              revision: { type: 'number' },
              ok: { type: 'boolean' },
            },
            required: ['requestId', 'operation', 'revision', 'ok'],
          },
        },
        retentionLimit: { type: 'integer', minimum: 1, description: 'The trace ring buffer bound (T05); entries never exceeds this.' },
      },
      required: ['entries', 'retentionLimit'],
    },
    units: {},
    // "inspect" only -- read-only by construction, distinct from opacity's
    // own diagnostics.trace/diagnostics.replay operations, which stay
    // opacity's (this capability never calls diagnostics.replay).
    effects: { kind: 'query', scope: 'none', lifecycle: ['inspect'] },
    availability: { state: 'unavailable', reason: 'missing-dependency' },
    fixture: {
      label: 'inspect the 5 most recent recorded opacity commands',
      input: { limit: 5 },
      output: { entries: [], retentionLimit: 32 },
    },
    examples: [
      { label: 'inspect every retained entry', input: {}, output: { entries: [], retentionLimit: 32 } },
      { label: 'inspect the 5 most recent recorded opacity commands', input: { limit: 5 }, output: { entries: [], retentionLimit: 32 } },
    ],
  };

  // Minimal request/response shape ({operation,payload} -> {ok,result} /
  // {ok:false,error:{code,message}}), matching timelapse-capability.js's
  // convention -- no document, no instance/revision identity of its own.
  // Internally, "inspect" reaches opacity's EXISTING diagnostics.trace
  // dispatch through win.NemoApplication.handle (the same production path
  // any other caller already uses) and only reshapes/bounds the result;
  // it never reads NemoOpacityDiagnostics' private trace state directly
  // and never calls diagnostics.replay.
  function handlerFor(win) {
    return function (request) {
      var operation = request && request.operation;
      if (operation !== 'inspect') return { ok: false, error: { code: 'unknown_operation', message: 'Unsupported operation: ' + operation } };
      var limit = request.payload && request.payload.limit;
      var identity = win.NemoOpacityApplication.meta();
      var traced = win.NemoApplication.handle({ apiVersion: 1, requestId: 'diagnostics-inspect:' + Math.random(),
        ...identity, expectedRevision: identity.revision, operation: 'diagnostics.trace', payload: {} });
      if (!traced.ok) return { ok: false, error: traced.error };
      var entries = traced.result.entries.map(function (entry) {
        return { requestId: entry.request.requestId, operation: entry.request.operation, revision: entry.revision, ok: entry.ok };
      });
      if (Number.isInteger(limit)) entries = entries.slice(-limit);
      return { ok: true, result: { entries: entries, retentionLimit: 32 } };
    };
  }

  function register(registry, handler, availability) {
    var descriptor = Object.assign({}, DESCRIPTOR, { availability: availability || DESCRIPTOR.availability });
    return registry.register(descriptor, handler);
  }

  return { DESCRIPTOR: DESCRIPTOR, computeAvailability: computeAvailability, handlerFor: handlerFor, register: register };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoDiagnosticsCapability;
