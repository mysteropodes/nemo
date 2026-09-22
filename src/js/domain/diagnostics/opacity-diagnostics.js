// Bounded write-trace and replay-envelope preparation for an application
// core (T05). Moved verbatim from opacity-application.js's trace/
// diagnostics.replay handling -- same 32-entry ring buffer (oldest entry
// shifted out first), same clone-on-write/clone-on-read discipline, same
// eligibility check (only a recorded property-write command may be
// replayed), so every existing trace/replay observation is preserved.
//
// Contract: this module never dispatches a request. remember() only
// records a write's shape after the caller's own writer has already
// applied it; prepareReplay() only validates a recorded command and shapes
// the envelope the caller would need to re-enter its own handler with --
// it returns that envelope, it never calls anything itself. That is what
// keeps a detached trace record from becoming a second way to mutate the
// document: replaying one still means the caller re-running its own single
// validated write path, exactly as before extraction.
var NemoOpacityDiagnostics = (function () {
  'use strict';
  var LIMIT = 32;
  function clone(value) { return JSON.parse(JSON.stringify(value)); }
  function object(value) { return !!value && typeof value === 'object' && !Array.isArray(value); }

  // replayableOperations: the caller's own list of write operations eligible
  // for replay (opacity-application.js passes its PROPERTY_WRITES -- history
  // and diagnostics.replay itself are deliberately excluded there already).
  function create(replayableOperations) {
    var trace = [];
    function remember(request, result) {
      trace.push({ request: clone(request), revision: result.revision, ok: result.ok });
      if (trace.length > LIMIT) trace.shift();
    }
    function entries() { return clone(trace); }
    function clear() { trace.length = 0; }
    // identity: {instanceId, documentId, revision} of the application core
    // at replay time -- the shaped envelope always targets the CURRENT
    // identity, never the one recorded at trace time.
    function prepareReplay(request, identity) {
      var recorded = request.payload && request.payload.request;
      if (!object(recorded) || !object(recorded.payload) || !replayableOperations.includes(recorded.operation)
          || request.requestId.length > 120) {
        return { error: 'Replay requires one recorded property command.' };
      }
      return { envelope: { apiVersion: 1, requestId: request.requestId + ':replay', instanceId: identity.instanceId,
        documentId: identity.documentId, expectedRevision: identity.revision,
        operation: recorded.operation, payload: clone(recorded.payload) } };
    }
    return { remember: remember, entries: entries, clear: clear, prepareReplay: prepareReplay };
  }
  return { create: create, LIMIT: LIMIT };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoOpacityDiagnostics;
