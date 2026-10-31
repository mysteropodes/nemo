/* Private Save As coordination only; no producer, owner, filesystem or UI state. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectSaveAsWorkflow = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  function object(value) { return value !== null && typeof value === 'object' && !Array.isArray(value); }
  function fields(value, names) { return object(value) && Object.keys(value).length === names.length &&
    names.every(function (name) { return Object.prototype.hasOwnProperty.call(value, name); }); }
  function id(value) { return typeof value === 'string' && /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(value); }
  function integer(value, minimum) { return Number.isSafeInteger(value) && value >= minimum; }
  function result(status, reason, pin, path, receipt) {
    var value = { status: status, eligibleCurrentSave: status === 'published-current' };
    if (reason) value.reason = reason;
    if (pin) value.pin = pin;
    if (path) value.destinationPath = path;
    if (receipt) value.receipt = receipt;
    return Object.freeze(value);
  }

  // This consumes an admitted presenter port. Client status alone is insufficient;
  // no ordinary object Open/presentation producer is implemented by this helper.
  function presented(value) {
    if (!object(value) || value.apiVersion !== 2 || value.available !== true || value.kind !== 'native-object' ||
        !id(value.instanceId) || !id(value.documentId) || !id(value.documentSnapshotId) ||
        !integer(value.contentRevision, 0) || !integer(value.lifecycleGeneration, 1)) return null;
    var p = value.presentation;
    if (!object(p) || p.owner !== 'native' || p.status !== 'presented' || p.contextId !== 'scene-root' ||
        !integer(p.frame, 0) || !integer(p.viewGeneration, 1) || !id(p.workId) ||
        p.instanceId !== value.instanceId || p.documentId !== value.documentId ||
        p.documentSnapshotId !== value.documentSnapshotId || p.contentRevision !== value.contentRevision ||
        p.lifecycleGeneration !== value.lifecycleGeneration) return null;
    return Object.freeze({ instanceId: value.instanceId, documentId: value.documentId,
      contentRevision: value.contentRevision, lifecycleGeneration: value.lifecycleGeneration,
      documentSnapshotId: value.documentSnapshotId });
  }
  async function current(ports) {
    try { return presented(await ports.readPresentedObjectSession()); }
    catch (_) { return null; }
  }
  function matches(a, b) {
    return !!(a && b && a.instanceId === b.instanceId && a.documentId === b.documentId &&
      a.contentRevision === b.contentRevision && a.lifecycleGeneration === b.lifecycleGeneration &&
      a.documentSnapshotId === b.documentSnapshotId);
  }
  function pinResponse(value, request, pin) {
    if (!fields(value, ['apiVersion', 'requestId', 'instanceId', 'documentId', 'contentRevision', 'ok', 'result']) ||
        value.apiVersion !== 2 || value.requestId !== request.requestId ||
        value.instanceId !== pin.instanceId || value.documentId !== pin.documentId ||
        value.contentRevision !== pin.contentRevision || value.ok !== true) return false;
    var r = value.result;
    if (request.operation === 'query.document.project.info') return fields(r, ['atRevision', 'documentSnapshotId', 'documentDigest', 'byteCount']) &&
      r.atRevision === pin.contentRevision && r.documentSnapshotId === pin.documentSnapshotId &&
      typeof r.documentDigest === 'string' && /^[a-f0-9]{64}$/.test(r.documentDigest) &&
      integer(r.byteCount, 1) && r.byteCount <= 32 * 1024 * 1024;
    return fields(r, ['atRevision', 'documentSnapshotId', 'documentJson']) &&
      r.atRevision === pin.contentRevision && r.documentSnapshotId === pin.documentSnapshotId &&
      typeof r.documentJson === 'string' && r.documentJson.length > 0;
  }
  function diskReceipt(value, request, project) {
    if (!fields(value, ['apiVersion', 'requestId', 'instanceId', 'documentId', 'atRevision',
        'documentSnapshotId', 'byteCount', 'published', 'cleanupPending']) ||
        value.apiVersion !== request.apiVersion || value.requestId !== request.requestId ||
        value.instanceId !== request.instanceId || value.documentId !== request.documentId ||
        value.atRevision !== request.atRevision || value.documentSnapshotId !== request.documentSnapshotId ||
        !integer(value.byteCount, 1) || value.byteCount > (project ? 32 * 1024 * 1024 : 4096) ||
        typeof value.published !== 'boolean' || typeof value.cleanupPending !== 'boolean') return null;
    // Detach the receipt from its port; do not retain a mutable native response.
    return Object.freeze({ apiVersion: value.apiVersion, requestId: value.requestId,
      instanceId: value.instanceId, documentId: value.documentId, atRevision: value.atRevision,
      documentSnapshotId: value.documentSnapshotId, byteCount: value.byteCount,
      published: value.published, cleanupPending: value.cleanupPending });
  }
  function cancelled(options) {
    try { return !!(options.signal && options.signal.aborted) ||
      (typeof options.isCurrent === 'function' && options.isCurrent() !== true); }
    catch (_) { return true; }
  }

  async function run(ports, options) {
    options = options || {};
    if (!object(ports) || !id(options.requestId) || !id(options.pinRequestId) ||
        options.requestId === options.pinRequestId || typeof ports.acquirePin !== 'function' ||
        typeof ports.pickDestination !== 'function' || typeof ports.savePinned !== 'function') {
      return result('refused', 'invalid-workflow-input');
    }
    // Keep request identity and the chosen authority ports stable across awaits.
    options = { requestId: options.requestId, pinRequestId: options.pinRequestId, project: options.project === true,
      replace: options.replace === true, signal: options.signal, isCurrent: options.isCurrent };
    ports = { readPresentedObjectSession: typeof ports.readPresentedObjectSession === 'function'
        ? ports.readPresentedObjectSession.bind(ports) : function () { return null; },
      acquirePin: ports.acquirePin.bind(ports), pickDestination: ports.pickDestination.bind(ports),
      savePinned: ports.savePinned.bind(ports) };
    if (cancelled(options)) return result('cancelled', 'cancelled-before-acquisition');
    var pin = await current(ports);
    if (cancelled(options)) return result('cancelled', 'cancelled-before-acquisition');
    if (!pin) return result('unavailable', 'current-presented-object-session-unavailable');
    var acquire = Object.freeze({ apiVersion: 2, requestId: options.pinRequestId,
      instanceId: pin.instanceId, documentId: pin.documentId,
      operation: options.project ? 'query.document.project.info' : 'query.document.object.serialize', payload: Object.freeze({ atRevision: pin.contentRevision }) });
    try {
      // Native bytes are inspected only as an opaque successful serializer result.
      // They are never parsed, assembled, retained, or supplied to the file writer.
      if (!pinResponse(await ports.acquirePin(acquire), acquire, pin)) return result('refused', 'native-pin-refused');
    } catch (_) { return result('refused', 'native-pin-acquisition-failed'); }
    if (cancelled(options)) return result('cancelled', 'cancelled-before-picker');
    var beforePicker = await current(ports);
    if (cancelled(options)) return result('cancelled', 'cancelled-before-picker');
    if (!matches(pin, beforePicker)) return result('stale-before-dispatch', 'pin-changed-before-picker');
    var path;
    try { path = await ports.pickDestination(pin); }
    catch (_) { return result('refused', 'picker-failed'); }
    if (path === null || path === undefined || path === '') return result('cancelled', 'picker-cancelled');
    if (typeof path !== 'string' || path[0] !== '/' || path.indexOf('\0') !== -1) return result('refused', 'invalid-destination');
    if (cancelled(options)) return result('cancelled', 'cancelled-before-dispatch');
    var beforeDispatch = await current(ports);
    if (cancelled(options)) return result('cancelled', 'cancelled-before-dispatch');
    if (!matches(pin, beforeDispatch)) return result('stale-before-dispatch', 'pin-changed-during-picker');
    var fieldsOut = { apiVersion: 2, requestId: options.requestId,
      instanceId: pin.instanceId, documentId: pin.documentId, atRevision: pin.contentRevision,
      documentSnapshotId: pin.documentSnapshotId, destinationPath: path };
    if (options.replace) fieldsOut.replace = true; // in-place Save of a titled project only
    var request = Object.freeze(fieldsOut);
    var receipt;
    try { receipt = diskReceipt(await ports.savePinned(request), request, options.project); }
    catch (_) { return result('indeterminate', 'publication-response-unavailable', pin, path); }
    if (!receipt) return result('indeterminate', 'publication-response-uncorrelated', pin, path);
    if (!receipt.published) return result('refused', 'native-publication-refused', pin, path, receipt);
    if (receipt.cleanupPending) return result('published-cleanup-pending', 'native-cleanup-pending', pin, path, receipt);
    var latest = await current(ports);
    if (cancelled(options) || !matches(pin, latest)) return result('published-stale', 'current-save-not-acknowledged', pin, path, receipt);
    return result('published-current', null, pin, path, receipt);
  }

  return Object.freeze({ run: run });
}));
