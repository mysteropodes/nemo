// The opacity command service owns validation, revision and retry boundaries.
// Concrete document/history/render bindings are supplied by bootstrap.
var NemoOpacityApplicationCore = (function () {
  'use strict';
  var READS = ['capabilities', 'snapshot', 'property.get', 'diagnostics.trace'];
  var WRITES = ['property.set', 'property.key.set', 'property.key.remove', 'property.animation.set', 'history.undo', 'history.redo', 'diagnostics.replay'];
  var PROPERTY_WRITES = WRITES.slice(0, 4);
  function clone(value) { return JSON.parse(JSON.stringify(value)); }
  function object(value) { return !!value && typeof value === 'object' && !Array.isArray(value); }

  function create(ports) {
    var identity = { instanceId: ports.newId(), documentId: ports.newId(), revision: 0 };
    var assigned = false, retained = new Map(), trace = [], editing = false;
    var context = ports.context(), frame = ports.state().currentFrame;
    function response(request, ok, value) {
      var result = { apiVersion: 1, requestId: request && request.requestId || '',
        instanceId: identity.instanceId, documentId: identity.documentId, revision: identity.revision, ok: ok };
      result[ok ? 'result' : 'error'] = value;
      return result;
    }
    function fail(request, code, message) { return response(request, false, { code: code, message: message }); }
    function touch() { identity.revision++; }
    function documentChanged() {
      identity.documentId = ports.newId();
      identity.revision = 0;
      retained.clear(); trace.length = 0;
      context = ports.context(); frame = ports.state().currentFrame;
    }
    function setInstanceId(id) {
      if (typeof id !== 'string' || !id) return fail({}, 'invalid_request', 'instanceId must be non-empty.');
      if (assigned && identity.instanceId !== id) return fail({}, 'INSTANCE_ID_CONFLICT', 'Instance identity is already assigned.');
      identity.instanceId = id; assigned = true;
      return response({}, true, { instanceId: id });
    }
    function validate(request) {
      if (!object(request) || request.apiVersion !== 1 || typeof request.requestId !== 'string'
          || !request.requestId || request.requestId.length > 128 || !object(request.payload)
          || !READS.concat(WRITES).includes(request.operation)) return ['invalid_request', 'Invalid application request.'];
      if (request.cancelled === true) return ['cancelled', 'Cancelled before dispatch.'];
      if ((request.instanceId != null && request.instanceId !== identity.instanceId)
          || (request.documentId != null && request.documentId !== identity.documentId)) return ['wrong_document', 'Request targets a different document or instance.'];
      if (WRITES.includes(request.operation) && (request.instanceId !== identity.instanceId
          || request.documentId !== identity.documentId)) return ['wrong_document', 'Writes require current instance and document identity.'];
      return null;
    }
    function fingerprint(request) {
      return JSON.stringify({ apiVersion: request.apiVersion, instanceId: request.instanceId,
        documentId: request.documentId, expectedRevision: request.expectedRevision,
        operation: request.operation, payload: request.payload });
    }
    function remember(request, body, result) {
      if (!WRITES.includes(request.operation)) return;
      retained.set(request.requestId, { body: body, result: clone(result) });
      if (retained.size > 256) retained.delete(retained.keys().next().value);
      trace.push({ request: clone(request), revision: result.revision, ok: result.ok });
      if (trace.length > 32) trace.shift();
    }
    function property(request) {
      var p = request.payload, state = ports.state();
      if (p.property !== 'opacity' || typeof p.layerId !== 'string' || !p.layerId) return { error: 'Only an identified layer opacity is supported.' };
      var matches = state.layers.filter(function (layer) { return layer.layerUid === p.layerId; });
      if (matches.length !== 1) return { error: 'Layer identity is missing or ambiguous.' };
      if (p.frame != null && (!Number.isFinite(p.frame) || p.frame < 0 || p.frame >= state.totalFrames)) return { error: 'Frame is outside the document.' };
      if (['property.key.set', 'property.key.remove'].includes(request.operation) && !Number.isInteger(p.frame)) return { error: 'Key frame must be an integer.' };
      if (['property.set', 'property.key.set'].includes(request.operation)
          && (!Number.isFinite(p.value) || p.value < 0 || p.value > 100)) return { error: 'Opacity must be in the range 0..100.' };
      if (request.operation === 'property.animation.set' && typeof p.animated !== 'boolean') return { error: 'animated must be boolean.' };
      var layer = matches[0], track = layer.motion && layer.motion.opacity;
      if (layer.motion != null && !object(layer.motion) || layer.motionStatic != null && !object(layer.motionStatic)) return { error: 'Opacity storage is malformed.' };
      if (WRITES.includes(request.operation) && (layer.locked || layer.keyLock)) return { error: 'Layer is locked.' };
      if (p.curvePoints != null) return { error: 'Curve editing is not exposed by this capability.' };
      if (track && (!Array.isArray(track.keys) || track.keys.some(function (key) { return !key || !Number.isFinite(key.frame) || !Array.isArray(key.v); }))) return { error: 'Opacity track is malformed.' };
      return { layer: layer, frame: p.frame == null ? state.currentFrame : p.frame };
    }
    function readProperty(layer, frame) {
      return { layerId: layer.layerUid, property: 'opacity', value: ports.valueAtFrame(layer, frame)[0] };
    }
    function perform(request) {
      var op = request.operation;
      if (op === 'capabilities') return response(request, true, { operations: READS.concat(WRITES),
        properties: [{ id: 'opacity', min: 0, max: 100, unit: 'percent', animated: true }],
        retryRetention: 256, traceRetention: 32, documentIdentity: 'open-document-incarnation' });
      if (op === 'snapshot') return response(request, true, ports.snapshot());
      if (op === 'diagnostics.trace') return response(request, true, { entries: clone(trace) });
      if (op === 'diagnostics.replay') {
        var recorded = request.payload.request;
        if (!object(recorded) || !object(recorded.payload) || !PROPERTY_WRITES.includes(recorded.operation) || request.requestId.length > 120) return fail(request, 'invalid_request', 'Replay requires one recorded property command.');
        var replay = { apiVersion: 1, requestId: request.requestId + ':replay', instanceId: identity.instanceId,
          documentId: identity.documentId, expectedRevision: identity.revision,
          operation: recorded.operation, payload: clone(recorded.payload) };
        var replayed = handle(replay);
        return replayed.ok ? response(request, true, { replay: replayed }) : fail(request, replayed.error.code, replayed.error.message);
      }
      if (op === 'history.undo' || op === 'history.redo') {
        var previous = identity.revision;
        if (!ports.history[op === 'history.undo' ? 'undo' : 'redo']()) return fail(request, 'history_unavailable', 'No history entry applies in this context.');
        if (identity.revision === previous) touch();
        return response(request, true, { applied: true });
      }
      var target = property(request);
      if (target.error) return fail(request, 'invalid_request', target.error);
      if (op === 'property.get') return response(request, true, readProperty(target.layer, target.frame));
      var before = identity.revision;
      ports.history.checkpoint();
      ports.write(op, target.layer, request.payload);
      if (identity.revision === before) touch();
      ports.afterMutation(target.layer);
      return response(request, true, readProperty(target.layer, target.frame));
    }
    function handle(request) {
      if (ports.context() !== context) documentChanged();
      if (ports.state().currentFrame !== frame) { frame = ports.state().currentFrame; touch(); }
      var invalid = validate(request);
      if (invalid) return fail(request, invalid[0], invalid[1]);
      var body;
      try { body = fingerprint(request); } catch (_) { return fail(request, 'invalid_request', 'Request must be JSON serializable.'); }
      if (body.length > 65536) return fail(request, 'invalid_request', 'Request is too large.');
      var prior = retained.get(request.requestId);
      if (prior) return prior.body === body ? clone(prior.result) : fail(request, 'invalid_request', 'requestId was reused with a changed body.');
      if (WRITES.includes(request.operation) && (!Number.isSafeInteger(request.expectedRevision)
          || request.expectedRevision !== identity.revision)) return fail(request, 'stale_revision', 'Read a current snapshot before writing.');
      if (WRITES.includes(request.operation) && !ports.canMutate()) return fail(request, 'unavailable', 'An interactive gesture owns the document.');
      if (editing) return fail(request, 'unavailable', 'A document edit is already in progress.');
      // Replay re-enters only with a validated property command, never replay/history.
      editing = request.operation !== 'diagnostics.replay';
      var result;
      try { result = perform(request); }
      finally { editing = false; }
      remember(request, body, result);
      return result;
    }
    return { handle: handle, setInstanceId: setInstanceId, documentChanged: documentChanged,
      historyChanged: touch, changed: touch, meta: function () { return Object.assign({}, identity); } };
  }
  return { create: create };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoOpacityApplicationCore;
