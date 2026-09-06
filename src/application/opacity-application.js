// Versioned application boundary for the first migrated property.  This is a
// thin command layer over the persisted layer record, never a second model.
(function (root) {
  'use strict';
  var DOMAIN = root.NemoOpacityDomain;
  var retained = Object.create(null), traces = [], dispatching = false;
  function stable(value, fallback) { return value || fallback; }
  function meta() {
    var state = root.state;
    if (!state.__nemoOpacityMeta) state.__nemoOpacityMeta = { instanceId: 'nemo-instance-1', documentId: 'nemo-document-1', revision: 0 };
    return state.__nemoOpacityMeta;
  }
  function copy(value) { return JSON.parse(JSON.stringify(value)); }
  function response(request, ok, result, error) {
    var m = meta();
    var out = { apiVersion: 1, requestId: request && request.requestId || '', instanceId: m.instanceId, documentId: m.documentId, revision: m.revision, ok: ok };
    if (ok) out.result = result || {};
    else out.error = error;
    return out;
  }
  function fail(request, code, message) { return response(request, false, null, { code: code, message: message }); }
  function body(request) { return JSON.stringify({ apiVersion: request.apiVersion, instanceId: request.instanceId, documentId: request.documentId, expectedRevision: request.expectedRevision, operation: request.operation, payload: request.payload }); }
  function mutation(operation) { return operation === 'property.set' || operation === 'property.key.set' || operation === 'property.key.remove' || operation === 'property.animation.set' || operation === 'history.undo' || operation === 'history.redo' || operation === 'diagnostics.replay'; }
  function validate(request) {
    if (!request || request.apiVersion !== 1 || typeof request.requestId !== 'string' || !request.requestId || typeof request.operation !== 'string' || !request.payload || typeof request.payload !== 'object') return 'Request must contain apiVersion 1, requestId, operation, and payload.';
    if (mutation(request.operation)) {
      var m = meta();
      if (request.instanceId !== m.instanceId || request.documentId !== m.documentId) return 'Request targets a different document.';
      if (!Number.isInteger(request.expectedRevision) || request.expectedRevision !== m.revision) return 'Request revision is stale.';
    }
    return null;
  }
  function opacityPayload(request) {
    var p = request.payload || {};
    if (p.property !== 'opacity' || typeof p.layerId !== 'string') return null;
    return p;
  }
  function touch() { meta().revision++; }
  function execute(request) {
    if (request && request.cancelled === true) return fail(request, 'cancelled', 'Request was cancelled before dispatch.');
    var fingerprint = request && body(request), prior = request && retained[request.requestId];
    // A retained request is authoritative even when the caller now has an
    // older revision. That is what makes network retries safe.
    if (prior) return prior.fingerprint === fingerprint ? copy(prior.response) : fail(request, 'invalid_request', 'requestId was reused for a different request.');
    var invalid = validate(request);
    if (invalid) return fail(request || {}, invalid.indexOf('revision') >= 0 ? 'stale_revision' : invalid.indexOf('different') >= 0 ? 'wrong_document' : 'invalid_request', invalid);
    var motion = root.SMMotion, state = root.state, p = opacityPayload(request), result;
    if (!motion || !state) return fail(request, 'unavailable', 'Motion application is unavailable.');
    if (request.operation === 'capabilities') result = { operations: ['capabilities', 'snapshot', 'property.get', 'property.set', 'property.key.set', 'property.key.remove', 'property.animation.set', 'history.undo', 'history.redo', 'diagnostics.trace', 'diagnostics.replay'], properties: ['opacity'] };
    else if (request.operation === 'snapshot') result = DOMAIN.snapshot(state, motion);
    else if (request.operation === 'diagnostics.trace') result = { entries: traces.slice(-32) };
    else if (request.operation === 'diagnostics.replay') {
      if (!request.payload.request || typeof request.payload.request !== 'object') return fail(request, 'invalid_request', 'Replay requires a recorded request.');
      var replay = copy(request.payload.request); replay.requestId = request.requestId + ':replay'; replay.instanceId = meta().instanceId; replay.documentId = meta().documentId; replay.expectedRevision = meta().revision;
      result = { replay: execute(replay) };
    } else if (request.operation === 'history.undo' || request.operation === 'history.redo') {
      if (typeof root[request.operation === 'history.undo' ? 'undo' : 'redo'] !== 'function') return fail(request, 'unavailable', 'History is unavailable.');
      root[request.operation === 'history.undo' ? 'undo' : 'redo'](); touch(); result = { applied: true };
    } else {
      if (!p) return fail(request, 'invalid_request', 'Only layer opacity is supported.');
      var layer = DOMAIN.findLayer(state, p.layerId); if (!layer) return fail(request, 'invalid_request', 'Unknown layerId.');
      if (request.operation === 'property.get') {
        result = { layerId: p.layerId, property: 'opacity', value: motion.valueAtFrame(layer, 'opacity', p.frame == null ? state.currentFrame : p.frame)[0] };
      } else {
      if ((request.operation === 'property.set' || request.operation === 'property.key.set') && !DOMAIN.number(p.value)) return fail(request, 'invalid_request', 'Opacity value must be finite.');
      if ((request.operation === 'property.key.set' || request.operation === 'property.key.remove') && !Number.isInteger(p.frame)) return fail(request, 'invalid_request', 'Keyframe frame must be an integer.');
      if (request.operation === 'property.animation.set' && typeof p.animated !== 'boolean') return fail(request, 'invalid_request', 'animated must be a boolean.');
      if (!mutation(request.operation)) return fail(request, 'invalid_request', 'Unknown operation.');
      dispatching = true;
      try {
        if (typeof root.pushUndo === 'function') root.pushUndo();
        if (request.operation === 'property.set') motion._opacityDirect.setValue(layer, [p.value]);
        else if (request.operation === 'property.key.set') motion._opacityDirect.setKeyAtFrame(layer, p.frame, [p.value]);
        else if (request.operation === 'property.key.remove') motion._opacityDirect.removeKeyAtFrame(layer, p.frame);
        else if (request.operation === 'property.animation.set') motion._opacityDirect.setAnimated(layer, !!p.animated);
      } finally { dispatching = false; }
      touch(); result = { layerId: p.layerId, property: 'opacity', value: motion.valueAtFrame(layer, 'opacity', p.frame == null ? state.currentFrame : p.frame)[0] };
      }
    }
    var out = response(request, true, result); retained[request.requestId] = { fingerprint: fingerprint, response: copy(out) };
    traces.push({ requestId: request.requestId, operation: request.operation, revision: out.revision, ok: out.ok });
    return out;
  }
  function legacy(kind, layer, value, frame) {
    // Legacy UI already creates its own undo step. It uses the same writer as
    // handle(), but deliberately avoids a second snapshot for one gesture.
    var direct = root.SMMotion && root.SMMotion._opacityDirect;
    if (!direct) return null;
    dispatching = true;
    try {
      var result;
      if (kind === 'set') result = direct.setValue(layer, value);
      else if (kind === 'key-current') result = direct.setKeyAtCurrentFrame(layer, value);
      else if (kind === 'key-frame') result = direct.setKeyAtFrame(layer, frame, value);
      else if (kind === 'remove') result = direct.removeKeyAtFrame(layer, frame);
      else if (kind === 'animated') result = direct.setAnimated(layer, value);
      touch();
      return result;
    } finally { dispatching = false; }
  }
  var api = { handle: execute };
  root.NemoApplication = api;
  root.NemoOpacityApplication = { isDispatching: function () { return dispatching; }, legacy: legacy, meta: meta };
})(window);
