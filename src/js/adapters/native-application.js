/* N15 pure v2 transport/validation adapter. N16 owns host bindings. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeApplicationAdapter = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  const operations = new Set([
    'command.document.apply',
    'query.document.opacity', 'query.document.revision', 'query.document.snapshot.acquire',
    'transaction.begin', 'transaction.update', 'transaction.commit', 'transaction.cancel',
    'transaction.status', 'history.undo', 'history.redo',
    'job.export.png.begin', 'job.export.png.status', 'job.export.png.cancel',
  ]);
  const requiredRevision = new Set([
    'command.document.apply', 'transaction.begin', 'history.undo', 'history.redo',
    'job.export.png.begin',
  ]);
  const commonErrors = new Set([
    'invalid_request', 'wrong_instance', 'wrong_document', 'stale_revision', 'busy_conflict',
    'unavailable', 'not_found', 'cancelled_before_dispatch', 'internal',
  ]);
  const jobErrors = new Set([
    'staging_failed', 'frame_failed', 'write_failed', 'publish_failed',
    'cleanup_failed', 'document_replaced',
  ]);
  const identifier = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;
  const has = (value, key) => Object.prototype.hasOwnProperty.call(value, key);

  function plain(value) {
    return value !== null && typeof value === 'object' &&
      (Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null);
  }
  function exact(value, required, optional, label) {
    if (!plain(value)) throw new TypeError(`${label} must be a plain object`);
    const allowed = new Set([...required, ...(optional || [])]);
    if (!required.every((key) => has(value, key)) || Object.keys(value).some((key) => !allowed.has(key))) {
      throw new TypeError(`${label} has missing or unknown fields`);
    }
    return value;
  }
  function id(value, label) {
    if (typeof value !== 'string' || !identifier.test(value)) throw new TypeError(`${label} must be a bounded identifier`);
    return value;
  }
  function revision(value, label) {
    if (!Number.isSafeInteger(value) || value < 0) throw new TypeError(`${label} must be a non-negative safe integer`);
    return value;
  }
  function rejectForbidden(value, seen) {
    if (typeof value === 'string') {
      if (/^(data|blob):/i.test(value)) throw new TypeError('raw or binary resource payload is forbidden');
      return;
    }
    if (value === null || ['number', 'boolean'].includes(typeof value)) return;
    if (typeof value !== 'object') throw new TypeError('request must contain JSON values only');
    if ((typeof ArrayBuffer !== 'undefined' && (value instanceof ArrayBuffer || ArrayBuffer.isView(value))) ||
        (typeof Buffer !== 'undefined' && Buffer.isBuffer && Buffer.isBuffer(value)) ||
        (typeof Blob !== 'undefined' && value instanceof Blob) ||
        (typeof ImageData !== 'undefined' && value instanceof ImageData) ||
        typeof value.getContext === 'function' || has(value, 'canvas')) {
      throw new TypeError('raw pixel, canvas, geometry, or binary resource payload is forbidden');
    }
    const visited = seen || new Set();
    if (visited.has(value)) throw new TypeError('cyclic payload is forbidden');
    visited.add(value);
    if (Array.isArray(value)) value.forEach((item) => rejectForbidden(item, visited));
    else {
      if (!plain(value)) throw new TypeError('unadmitted object payload is forbidden');
      Object.values(value).forEach((item) => rejectForbidden(item, visited));
    }
    visited.delete(value);
  }
  function empty(payload, label) { exact(payload, [], [], label); }
  function target(payload, label) {
    exact(payload, ['stableTarget'], [], label);
    exact(payload.stableTarget, ['layerUid'], [], `${label}.stableTarget`);
    id(payload.stableTarget.layerUid, `${label}.stableTarget.layerUid`);
  }
  function transactionId(payload, label) {
    exact(payload, ['transactionId'], [], label);
    id(payload.transactionId, `${label}.transactionId`);
  }
  function validatePayload(operation, payload) {
    if (operation === 'command.document.apply') {
      exact(payload, ['command', 'stableTarget', 'value'], [], 'command payload');
      if (payload.command !== 'layer.opacity.set') throw new TypeError('unsupported command');
      target({ stableTarget: payload.stableTarget }, 'command target');
      if (typeof payload.value !== 'number' || !Number.isFinite(payload.value) || payload.value < 0 || payload.value > 100) throw new TypeError('opacity value is invalid');
    } else if (operation === 'query.document.opacity') {
      exact(payload, ['stableTarget'], ['atRevision'], 'opacity query');
      target({ stableTarget: payload.stableTarget }, 'opacity query target');
      if (has(payload, 'atRevision')) revision(payload.atRevision, 'atRevision');
    } else if (operation === 'query.document.revision') {
      exact(payload, [], ['frame', 'contextId', 'viewGeneration'], 'revision query');
      if (has(payload, 'frame')) revision(payload.frame, 'frame');
      if (has(payload, 'contextId')) id(payload.contextId, 'contextId');
      if (has(payload, 'viewGeneration')) revision(payload.viewGeneration, 'viewGeneration');
    } else if (operation === 'query.document.snapshot.acquire') {
      exact(payload, [], ['atRevision'], 'snapshot query');
      if (has(payload, 'atRevision')) revision(payload.atRevision, 'atRevision');
    } else if (operation === 'transaction.begin') target(payload, 'transaction begin');
    else if (operation === 'transaction.update') {
      exact(payload, ['transactionId', 'value'], [], 'transaction update');
      id(payload.transactionId, 'transactionId');
      if (typeof payload.value !== 'number' || !Number.isFinite(payload.value) || payload.value < 0 || payload.value > 100) throw new TypeError('opacity value is invalid');
    } else if (['transaction.commit', 'transaction.cancel', 'transaction.status'].includes(operation)) transactionId(payload, operation);
    else if (['history.undo', 'history.redo'].includes(operation)) empty(payload, operation);
    else if (operation === 'job.export.png.begin') validateJobBegin(payload);
    else {
      exact(payload, ['jobId'], [], operation);
      id(payload.jobId, 'jobId');
    }
  }
  function validateJobBegin(payload) {
    exact(payload, ['contextId', 'quality', 'outputHandle', 'frames'], [], 'PNG begin');
    id(payload.contextId, 'contextId'); id(payload.outputHandle, 'outputHandle');
    if (payload.quality !== 'final') throw new TypeError('quality must be final');
    if (!Array.isArray(payload.frames) || payload.frames.length < 1 || payload.frames.length > 120) throw new TypeError('frames must be bounded');
    let prior = -1;
    payload.frames.forEach((frame) => {
      exact(frame, ['sourceFrame', 'geometryHandle'], [], 'PNG frame');
      revision(frame.sourceFrame, 'sourceFrame');
      if (frame.sourceFrame <= prior) throw new TypeError('sourceFrame values must be strictly ascending');
      prior = frame.sourceFrame;
      exact(frame.geometryHandle, ['resourceId', 'resourceVersion'], [], 'geometryHandle');
      id(frame.geometryHandle.resourceId, 'resourceId'); id(frame.geometryHandle.resourceVersion, 'resourceVersion');
    });
  }
  function validateRequest(value) {
    rejectForbidden(value);
    exact(value, ['apiVersion', 'requestId', 'instanceId', 'documentId', 'operation', 'payload'], ['expectedRevision', 'cancelledBeforeDispatch'], 'request');
    if (value.apiVersion !== 2) throw new TypeError('apiVersion must be 2');
    ['requestId', 'instanceId', 'documentId'].forEach((key) => id(value[key], key));
    if (!operations.has(value.operation)) throw new TypeError('operation is not admitted');
    const needsRevision = requiredRevision.has(value.operation);
    if (needsRevision !== has(value, 'expectedRevision')) throw new TypeError(`${value.operation} has invalid expectedRevision presence`);
    if (needsRevision) revision(value.expectedRevision, 'expectedRevision');
    if (has(value, 'cancelledBeforeDispatch') && typeof value.cancelledBeforeDispatch !== 'boolean') throw new TypeError('cancelledBeforeDispatch must be boolean');
    validatePayload(value.operation, value.payload);
    const encoded = JSON.stringify(value);
    const encodedBytes = typeof TextEncoder === 'function' ? new TextEncoder().encode(encoded).length : unescape(encodeURIComponent(encoded)).length;
    if (typeof encoded !== 'string' || encodedBytes > 4096) throw new TypeError('request exceeds 4096 encoded bytes');
    return value;
  }
  function validateError(value) {
    exact(value, ['code', 'message'], ['details'], 'error');
    if (!commonErrors.has(value.code) || typeof value.message !== 'string') throw new TypeError('invalid dispatch error');
  }
  function validateJobReceipt(value) {
    exact(value, ['jobId', 'status', 'pinnedRevision', 'documentSnapshotId', 'progress', 'artifact', 'cleanup', 'externalEffectDisposition'], ['error'], 'JobReceipt');
    id(value.jobId, 'jobId'); id(value.documentSnapshotId, 'documentSnapshotId'); revision(value.pinnedRevision, 'pinnedRevision');
    if (!['running', 'succeeded', 'failed', 'cancelled'].includes(value.status) || typeof value.progress !== 'number' || value.progress < 0 || value.progress > 1) throw new TypeError('invalid job status or progress');
    if (!['none', 'contained', 'committed', 'indeterminate'].includes(value.externalEffectDisposition)) throw new TypeError('invalid external effect disposition');
    exact(value.cleanup, ['status'], ['error'], 'cleanup');
    if (!['not_required', 'pending', 'complete', 'failed'].includes(value.cleanup.status)) throw new TypeError('invalid cleanup status');
    if (value.artifact !== null) {
      exact(value.artifact, ['target', 'files'], [], 'artifact'); id(value.artifact.target, 'artifact.target');
      if (!Array.isArray(value.artifact.files) || value.artifact.files.some((file) => typeof file !== 'string')) throw new TypeError('artifact files are invalid');
    }
    for (const error of [value.cleanup.error, value.error]) if (error !== undefined) {
      exact(error, ['code', 'message'], ['details'], 'job error');
      if (!jobErrors.has(error.code) || typeof error.message !== 'string') throw new TypeError('invalid job error');
    }
  }
  function validateResult(value, operation) {
    if (!operation) return;
    if (['command.document.apply', 'history.undo', 'history.redo'].includes(operation)) {
      exact(value, ['applied', 'historyEntriesAdded'], [], 'command result');
      if (typeof value.applied !== 'boolean' || ![0, 1].includes(value.historyEntriesAdded)) throw new TypeError('invalid command result');
    } else if (operation === 'query.document.opacity') {
      exact(value, ['atRevision', 'layerUid', 'value'], [], 'opacity result');
      revision(value.atRevision, 'atRevision'); id(value.layerUid, 'layerUid');
      if (typeof value.value !== 'number' || !Number.isFinite(value.value)) throw new TypeError('invalid opacity result');
    } else if (operation === 'query.document.revision') {
      exact(value, ['contentRevision', 'frame', 'contextId', 'viewGeneration'], [], 'revision result');
      revision(value.contentRevision, 'contentRevision');
      if (value.frame !== null) revision(value.frame, 'frame');
      if (value.contextId !== null) id(value.contextId, 'contextId');
      if (value.viewGeneration !== null) revision(value.viewGeneration, 'viewGeneration');
    } else if (operation === 'query.document.snapshot.acquire') {
      exact(value, ['atRevision', 'documentSnapshotId'], [], 'snapshot result');
      revision(value.atRevision, 'atRevision'); id(value.documentSnapshotId, 'documentSnapshotId');
    } else if (operation.startsWith('transaction.')) {
      exact(value, ['transactionId', 'baseRevision', 'workingGeneration', 'workingState', 'committedRevision', 'terminalDisposition', 'historyEntriesAdded'], [], 'transaction result');
      id(value.transactionId, 'transactionId'); revision(value.baseRevision, 'baseRevision'); revision(value.workingGeneration, 'workingGeneration');
      if (value.committedRevision !== null) revision(value.committedRevision, 'committedRevision');
      if (![null, 'succeeded', 'cancelled'].includes(value.terminalDisposition) || ![0, 1].includes(value.historyEntriesAdded)) throw new TypeError('invalid transaction disposition');
      exact(value.workingState, ['stableTarget', 'value'], [], 'workingState'); target({ stableTarget: value.workingState.stableTarget }, 'workingState target');
      if (typeof value.workingState.value !== 'number' || !Number.isFinite(value.workingState.value) || value.workingState.value < 0 || value.workingState.value > 100) throw new TypeError('invalid working opacity');
    } else if (operation.startsWith('job.export.png.')) validateJobReceipt(value);
  }
  function validateResponse(value, requestId, operation) {
    rejectForbidden(value);
    exact(value, ['apiVersion', 'requestId', 'instanceId', 'documentId', 'contentRevision', 'ok'], value.ok ? ['result'] : ['error'], 'response');
    if (value.apiVersion !== 2 || value.requestId !== requestId || typeof value.ok !== 'boolean') throw new TypeError('response identity is invalid');
    id(value.instanceId, 'instanceId'); id(value.documentId, 'documentId'); revision(value.contentRevision, 'contentRevision');
    if (value.ok) {
      if (!has(value, 'result') || has(value, 'error')) throw new TypeError('successful response must contain only result');
      validateResult(value.result, operation);
    } else {
      if (!has(value, 'error') || has(value, 'result')) throw new TypeError('failed response must contain only error');
      validateError(value.error);
    }
    return value;
  }
  function clone(value) { return JSON.parse(JSON.stringify(value)); }
  function createNativeApplicationAdapter(label, transport) {
    id(label, 'port label');
    if (!transport || typeof transport.dispatch !== 'function') throw new TypeError('injected transport requires dispatch(request)');
    return Object.freeze({
      label,
      dispatch(request) {
        validateRequest(request);
        const outbound = clone(request);
        return Promise.resolve(transport.dispatch(outbound)).then((response) => validateResponse(response, request.requestId, request.operation));
      },
    });
  }
  return Object.freeze({ createNativeApplicationAdapter, validateRequest, validateResponse });
}));
