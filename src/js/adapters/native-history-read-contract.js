/* #2376 G26: JS side of query.document.object.history, the live undo/redo stacks and one labelled page of entries. Pure validation;
 * it owns no history, copies no stack and issues no request. The entries are chronological (done entries oldest first, then the redoable
 * ones nearest first) and the cursor is `undoDepth`. A pinned read carries no expectedRevision. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeHistoryReadContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const OP = 'query.document.object.history', MAX_PAGE = 32, MAX_LABEL = 32;
  const IDENTIFIER = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;
  const has = (value, key) => Object.prototype.hasOwnProperty.call(value, key);
  const fail = (message) => { throw new TypeError(message); };
  const revision = (value) => Number.isSafeInteger(value) && value >= 0;
  function exact(value, required, optional, label) {
    if (!value || typeof value !== 'object' || Array.isArray(value) || ![Object.prototype, null].includes(Object.getPrototypeOf(value))) fail(label + ' must be a plain object');
    const allowed = required.concat(optional);
    if (!required.every((key) => has(value, key)) || Object.keys(value).some((key) => !allowed.includes(key))) fail(label + ' has missing or unknown fields');
    return value;
  }
  function payload(operation, value) {
    if (operation !== OP) fail('history read operation is not declared');
    exact(value, ['atRevision', 'documentSnapshotId'], ['offset', 'limit'], 'history read payload');
    if (!revision(value.atRevision) || typeof value.documentSnapshotId !== 'string' || !IDENTIFIER.test(value.documentSnapshotId)) fail('invalid history read pin');
    if (has(value, 'offset') && !revision(value.offset)) fail('invalid history page offset');
    if (has(value, 'limit') && !(Number.isSafeInteger(value.limit) && value.limit >= 1 && value.limit <= MAX_PAGE)) fail('invalid history page limit');
  }
  function result(operation, value, request) {
    if (operation !== OP) fail('history read operation is not declared');
    exact(value, ['atRevision', 'documentSnapshotId', 'undoDepth', 'redoDepth', 'offset', 'entries'], [], 'history read result');
    if (!revision(value.atRevision) || !revision(value.undoDepth) || !revision(value.redoDepth) || !revision(value.offset) ||
      typeof value.documentSnapshotId !== 'string' || !IDENTIFIER.test(value.documentSnapshotId)) fail('invalid history read result');
    if (!Array.isArray(value.entries) || value.entries.length > MAX_PAGE) fail('invalid history entries');
    if (request && (value.entries.length > (has(request, 'limit') ? request.limit : MAX_PAGE) || has(request, 'offset') && request.offset !== value.offset)) fail('history page does not answer its request');
    if (value.offset + value.entries.length > value.undoDepth + value.redoDepth) fail('history page exceeds the stacks');
    value.entries.forEach((entry, at) => {
      exact(entry, ['index', 'label'], [], 'history entry');
      if (entry.index !== value.offset + at || typeof entry.label !== 'string' || !entry.label.length || Array.from(entry.label).length > MAX_LABEL) fail('invalid history entry');
    });
  }
  function correlate(request, response) {
    if (response.requestId !== request.requestId || response.instanceId !== request.instanceId) fail('history read response owner mismatch');
    const replaced = !response.ok && response.error && response.error.code === 'wrong_document';
    if (response.documentId !== request.documentId && !replaced) fail('history read response document mismatch');
    if (!response.ok) return;
    result(OP, response.result, request.payload);
    if (response.result.atRevision !== request.payload.atRevision || response.result.documentSnapshotId !== request.payload.documentSnapshotId ||
      response.contentRevision !== request.payload.atRevision) fail('history read pin mismatch');
  }
  const admitted = (operation) => operation === OP;
  const own = Object.freeze({ admitted, requiresRevision: () => false, payload, result, correlate, paths: () => null });
  function compose(base) {
    if (!base) return own;
    return Object.freeze(Object.assign({}, base, {
      admitted: (operation) => admitted(operation) || base.admitted(operation),
      requiresRevision: (operation) => !admitted(operation) && base.requiresRevision(operation),
      payload: (operation, value, envelope) => (admitted(operation) ? payload(operation, value) : base.payload(operation, value, envelope)),
      result: (operation, value) => (admitted(operation) ? result(operation, value) : base.result(operation, value)),
      correlate: (request, response) => (admitted(request.operation) ? correlate(request, response) : base.correlate(request, response)),
      paths: (operation) => (admitted(operation) ? null : base.paths(operation)),
    }));
  }
  return Object.freeze({ OP, MAX_PAGE, admitted, requiresRevision: own.requiresRevision, payload, result, correlate, paths: own.paths, compose });
}));
