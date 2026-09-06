/* Versioned, transport-neutral application boundary for the first Motion slice. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  if (root) root.SMApplicationCore = api;
}(typeof window !== 'undefined' ? window : globalThis, function () {
  'use strict';

  var API_VERSION = 1;

  function clone(value) { return JSON.parse(JSON.stringify(value)); }
  function same(a, b) { return JSON.stringify(a) === JSON.stringify(b); }
  function error(code, message) { return { code: code, message: message }; }

  function create(port) {
    if (!port || typeof port.snapshot !== 'function') throw new TypeError('A document port is required');
    var revision = 0;
    var requests = Object.create(null);
    var writing = false;

    function response(request, ok, result, failure) {
      var out = { apiVersion: API_VERSION, requestId: request && request.requestId, documentId: port.documentId(), revision: revision, ok: ok };
      if (ok) out.result = result;
      else out.error = failure;
      return out;
    }
    function reject(request, code, message) { return response(request || {}, false, null, error(code, message)); }
    function validRequest(request) {
      if (!request || typeof request !== 'object') return error('invalid_request', 'request must be an object');
      if (request.apiVersion !== API_VERSION) return error('unsupported_api_version', 'apiVersion must be 1');
      if (typeof request.requestId !== 'string' || !request.requestId) return error('invalid_request_id', 'requestId is required');
      if (request.documentId !== port.documentId()) return error('document_mismatch', 'documentId is not active');
      if (!Number.isInteger(request.expectedRevision)) return error('invalid_revision', 'expectedRevision must be an integer');
      if (request.expectedRevision !== revision) return error('stale_revision', 'expectedRevision does not match the current revision');
      return null;
    }
    function layerFor(args) {
      if (!args || typeof args.layerId !== 'string' || !args.layerId) return { error: error('invalid_layer', 'layerId is required') };
      var layer = port.layer(args.layerId);
      return layer ? { layer: layer } : { error: error('layer_not_found', 'layerId does not exist in the active composition') };
    }
    function opacity(args) {
      var v = args && args.value;
      if (typeof v !== 'number' || !isFinite(v) || v < 0 || v > 100) return error('invalid_opacity', 'opacity must be a finite percent between 0 and 100');
      return null;
    }
    function frame(args, required) {
      if (args && args.frame === undefined && !required) return null;
      var value = args && args.frame;
      if (!Number.isInteger(value) || value < 0 || value >= port.totalFrames()) return error('invalid_frame', 'frame must be a zero-based frame in the active composition');
      return null;
    }
    function commit(request, mutation) {
      var result = mutation();
      if (!result.changed) return response(request, true, result.result || { changed: false });
      revision++;
      return response(request, true, result.result || { changed: true });
    }
    function run(request) {
      var invalid = validRequest(request);
      if (invalid) return response(request || {}, false, null, invalid);
      var args = request.args || {};
      if (request.command === 'property.set' || request.command === 'property.keyframe') {
        if (args.property !== 'opacity') return reject(request, 'unsupported_property', 'only opacity is supported by v1');
        var target = layerFor(args); if (target.error) return response(request, false, null, target.error);
        var badOpacity = opacity(args); if (badOpacity) return response(request, false, null, badOpacity);
        var animated = port.isAnimated(target.layer);
        var needsFrame = request.command === 'property.keyframe' || animated;
        var badFrame = frame(args, needsFrame); if (badFrame) return response(request, false, null, badFrame);
        return commit(request, function () {
          if (!port.wouldSetOpacity(target.layer, args.value, args.frame, request.command === 'property.keyframe')) return { changed: false };
          port.beginHistory();
          writing = true;
          var changed;
          try { changed = port.setOpacity(target.layer, args.value, args.frame, request.command === 'property.keyframe'); }
          finally { writing = false; }
          if (!changed) return { changed: false };
          port.refresh();
          return { changed: true, result: { changed: true } };
        });
      }
      if (request.command === 'history.undo' || request.command === 'history.redo') {
        return commit(request, function () {
          var outcome = request.command === 'history.undo' ? port.undo() : port.redo();
          if (outcome && outcome.blocked) return { changed: false, result: { changed: false, refused: 'history_context' } };
          if (!(outcome && outcome.changed)) return { changed: false, result: { changed: false } };
          port.refresh();
          return { changed: true, result: { changed: true } };
        });
      }
      return reject(request, 'unsupported_command', 'command is not supported by v1');
    }
    function dispatch(request) {
      var id = request && request.requestId;
      var fingerprint = request && typeof request === 'object' ? JSON.stringify(request) : null;
      if (id && requests[id]) {
        if (!same(requests[id].fingerprint, fingerprint)) return reject(request, 'request_id_reused', 'requestId was already used with different content');
        return clone(requests[id].response);
      }
      var out = run(request);
      if (id) requests[id] = { fingerprint: fingerprint, response: clone(out) };
      return out;
    }
    function query() { return clone(port.snapshot(revision)); }
    // UI writes call this after their existing transaction. It prevents an API
    // client from writing against a revision the artist has already changed.
    function observeUiMutation() { revision++; return revision; }
    // Motion's existing controls already begin their one history transaction
    // before calling their shared writer. This keeps that history intact while
    // making the final opacity write pass through this application boundary.
    function applyUiOpacity(layer, values) {
      if (writing) return false;
      writing = true;
      var changed;
      try { changed = port.setOpacity(layer, values[0], undefined, false); }
      finally { writing = false; }
      if (changed) { revision++; port.refresh(); }
      return changed;
    }
    return { dispatch: dispatch, query: query, isDispatching: function () { return writing; }, applyUiOpacity: applyUiOpacity, observeUiMutation: observeUiMutation, revision: function () { return revision; } };
  }
  return { API_VERSION: API_VERSION, create: create };
}));
