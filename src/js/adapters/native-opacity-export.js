/* N18 dormant pinned PNG receipt consumer. N20 owns activation. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeOpacityExportAdapter = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  const identifier = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;
  const statuses = new Set(['running', 'succeeded', 'failed', 'cancelled']);
  const cleanupStatuses = new Set(['not_required', 'pending', 'complete', 'failed']);
  const external = new Set(['none', 'contained', 'committed', 'indeterminate']);
  const commonErrors = new Set([
    'invalid_request', 'wrong_instance', 'wrong_document', 'stale_revision', 'busy_conflict',
    'unavailable', 'not_found', 'cancelled_before_dispatch', 'internal',
  ]);
  const jobErrors = new Set([
    'staging_failed', 'frame_failed', 'write_failed', 'publish_failed',
    'cleanup_failed', 'document_replaced',
  ]);
  const fixedOutputSpec = Object.freeze({
    kind: 'frame', format: 'rgba8', width: 320, height: 180,
    colorInterpretation: 'srgb', alphaMode: 'straight',
  });
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
    for (const key of Object.keys(value)) {
      const descriptor = Object.getOwnPropertyDescriptor(value, key);
      if (!descriptor || !has(descriptor, 'value')) throw new TypeError(`${label}.${key} must not be an accessor`);
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
  function rejectRaw(value, seen) {
    if (typeof value === 'string') {
      if (/^(data|blob):/i.test(value)) throw new TypeError('pixel or binary URL payload is forbidden');
      return;
    }
    if (value === null || ['number', 'boolean', 'undefined'].includes(typeof value)) return;
    if (typeof value === 'function' || typeof value !== 'object') throw new TypeError('callbacks and unsupported payload values are forbidden');
    if ((typeof ArrayBuffer !== 'undefined' && (value instanceof ArrayBuffer || ArrayBuffer.isView(value))) ||
        (typeof Buffer !== 'undefined' && Buffer.isBuffer && Buffer.isBuffer(value)) ||
        (typeof Blob !== 'undefined' && value instanceof Blob) ||
        (typeof ImageData !== 'undefined' && value instanceof ImageData) ||
        has(value, 'getContext') || has(value, 'canvas')) {
      throw new TypeError('pixel, image, canvas, or binary payload is forbidden');
    }
    const visited = seen || new Set();
    if (visited.has(value)) throw new TypeError('cyclic payload is forbidden');
    visited.add(value);
    if (!Array.isArray(value) && !plain(value)) throw new TypeError('unadmitted object payload is forbidden');
    for (const key of Object.keys(value)) {
      const descriptor = Object.getOwnPropertyDescriptor(value, key);
      if (!descriptor || !has(descriptor, 'value')) throw new TypeError('accessor payload is forbidden');
      rejectRaw(descriptor.value, visited);
    }
    visited.delete(value);
  }
  function freeze(value) { return Object.freeze(value); }
  function clone(value) {
    if (value === null || ['string', 'number', 'boolean'].includes(typeof value)) return value;
    if (Array.isArray(value)) return freeze(value.map(clone));
    return freeze(Object.fromEntries(Object.keys(value).map((key) => [key, clone(value[key])]))) ;
  }
  function identity(value) {
    rejectRaw(value); exact(value, ['instanceId', 'documentId', 'contentRevision'], [], 'identity');
    return freeze({
      instanceId: id(value.instanceId, 'identity.instanceId'),
      documentId: id(value.documentId, 'identity.documentId'),
      contentRevision: revision(value.contentRevision, 'identity.contentRevision'),
    });
  }
  function copyFrames(value) {
    rejectRaw(value);
    if (!Array.isArray(value) || value.length < 1 || value.length > 120) throw new TypeError('frames must contain 1..120 entries');
    let previous = -1;
    return freeze(value.map((frame, index) => {
      exact(frame, ['sourceFrame', 'geometryHandle'], [], `frames[${index}]`);
      exact(frame.geometryHandle, ['resourceId', 'resourceVersion'], [], `frames[${index}].geometryHandle`);
      const sourceFrame = revision(frame.sourceFrame, `frames[${index}].sourceFrame`);
      if (sourceFrame <= previous) throw new TypeError('frames must be strictly ascending by sourceFrame');
      previous = sourceFrame;
      return freeze({
        sourceFrame,
        geometryHandle: freeze({
          resourceId: id(frame.geometryHandle.resourceId, `frames[${index}].geometryHandle.resourceId`),
          resourceVersion: id(frame.geometryHandle.resourceVersion, `frames[${index}].geometryHandle.resourceVersion`),
        }),
      });
    }));
  }
  function snapshot(response, current) {
    rejectRaw(response);
    exact(response, ['apiVersion', 'requestId', 'instanceId', 'documentId', 'contentRevision', 'ok', 'result'], [], 'snapshot response');
    id(response.requestId, 'snapshot response.requestId');
    if (response.apiVersion !== 2 || response.ok !== true || id(response.instanceId, 'snapshot response.instanceId') !== current.instanceId ||
        id(response.documentId, 'snapshot response.documentId') !== current.documentId || revision(response.contentRevision, 'snapshot response.contentRevision') !== current.contentRevision) {
      throw new Error('snapshot response does not match the supplied identity');
    }
    exact(response.result, ['atRevision', 'documentSnapshotId'], [], 'snapshot response.result');
    const atRevision = revision(response.result.atRevision, 'snapshot response.result.atRevision');
    if (atRevision !== current.contentRevision) throw new Error('snapshot revision does not match identity contentRevision');
    return freeze({ documentId: current.documentId, atRevision, documentSnapshotId: id(response.result.documentSnapshotId, 'snapshot response.result.documentSnapshotId') });
  }
  function request(current, requestId, operation, payload, expectedRevision) {
    const projected = {
      apiVersion: 2,
      requestId: id(requestId, 'requestId'),
      instanceId: current.instanceId,
      documentId: current.documentId,
      ...(expectedRevision === undefined ? {} : { expectedRevision }),
      operation,
      payload: freeze(payload),
    };
    const encoded = JSON.stringify(projected);
    const bytes = typeof TextEncoder === 'function'
      ? new TextEncoder().encode(encoded).length
      : unescape(encodeURIComponent(encoded)).length;
    if (typeof encoded !== 'string' || bytes > 4096) {
      throw new RangeError('staged v2 request exceeds 4096 encoded bytes');
    }
    return freeze(projected);
  }
  function error(value, codes, label) {
    exact(value, ['code', 'message'], ['details'], 'response.error');
    if (typeof value.message !== 'string') throw new TypeError('response.error.message must be a string');
    const code = id(value.code, 'response.error.code');
    if (!codes.has(code)) throw new TypeError(`${label} code is not admitted`);
    return freeze({ code, message: value.message, ...(has(value, 'details') ? { details: clone(value.details) } : {}) });
  }
  function receipt(value) {
    exact(value, ['jobId', 'status', 'pinnedRevision', 'documentSnapshotId', 'progress', 'artifact', 'cleanup', 'externalEffectDisposition'], ['error'], 'job receipt');
    if (!statuses.has(value.status) || typeof value.progress !== 'number' || !Number.isFinite(value.progress) || value.progress < 0 || value.progress > 1 || !external.has(value.externalEffectDisposition)) throw new TypeError('job receipt is invalid');
    exact(value.cleanup, ['status'], ['error'], 'job receipt.cleanup');
    if (!cleanupStatuses.has(value.cleanup.status)) throw new TypeError('job receipt.cleanup.status is invalid');
    if (has(value.cleanup, 'error') && value.cleanup.status !== 'failed') {
      throw new TypeError('job receipt.cleanup.error requires failed cleanup');
    }
    const artifact = value.artifact === null ? null : (() => {
      exact(value.artifact, ['target', 'files'], [], 'job receipt.artifact');
      if (!Array.isArray(value.artifact.files) || value.artifact.files.some((file) => typeof file !== 'string')) {
        throw new TypeError('job receipt.artifact.files must contain only strings');
      }
      return freeze({ target: id(value.artifact.target, 'job receipt.artifact.target'), files: freeze([...value.artifact.files]) });
    })();
    const cleanup = freeze({ status: value.cleanup.status, ...(has(value.cleanup, 'error') ? { error: error(value.cleanup.error, jobErrors, 'job cleanup error') } : {}) });
    return freeze({ jobId: id(value.jobId, 'job receipt.jobId'), status: value.status, pinnedRevision: revision(value.pinnedRevision, 'job receipt.pinnedRevision'), documentSnapshotId: id(value.documentSnapshotId, 'job receipt.documentSnapshotId'), progress: value.progress, artifact, cleanup, externalEffectDisposition: value.externalEffectDisposition, ...(has(value, 'error') ? { error: error(value.error, jobErrors, 'job error') } : {}) });
  }
  function terminalInvariant(value) {
    const cleanupFailed = value.cleanup.status === 'failed';
    const topLevelCleanupFailed = has(value, 'error') && value.error.code === 'cleanup_failed';
    if (cleanupFailed !== topLevelCleanupFailed || (cleanupFailed &&
        (!has(value.cleanup, 'error') || value.cleanup.error.code !== 'cleanup_failed' ||
         value.externalEffectDisposition !== 'indeterminate'))) {
      throw new Error('cleanup_failed requires failed cleanup and an indeterminate external effect');
    }
    if (value.status === 'running') {
      if (value.artifact !== null || value.cleanup.status !== 'pending' || value.externalEffectDisposition !== 'none' || has(value, 'error')) throw new Error('running job must retain pending cleanup, no artifact, no error, and no external effect');
      return;
    }
    if (value.status === 'succeeded') {
      if (value.progress !== 1 || value.artifact === null || value.cleanup.status !== 'complete' || value.externalEffectDisposition !== 'committed' || has(value, 'error')) throw new Error('successful job must publish one complete artifact after complete cleanup');
      return;
    }
    if (value.artifact !== null || value.cleanup.status === 'pending' || (value.status === 'failed' && !has(value, 'error'))) throw new Error('failed or cancelled job cannot expose an artifact or pending cleanup');
    if (value.status === 'cancelled' && (value.cleanup.status !== 'complete' || value.externalEffectDisposition !== 'none' || (has(value, 'error') && value.error.code !== 'document_replaced'))) throw new Error('cancelled job requires complete cleanup, no external effect, and at most document_replaced');
    if (value.status === 'failed' && !['complete', 'failed'].includes(value.cleanup.status)) throw new Error('failed job cleanup must be complete or failed');
    if (value.status === 'failed' && value.cleanup.status === 'complete' && value.externalEffectDisposition !== 'none') throw new Error('contained failed job must expose no external effect');
  }

  function createNativeOpacityExportAdapter() {
    const pending = new Map();
    const jobs = new Map();
    const requests = new Map();
    function remember(value) {
      const old = requests.get(value.requestId);
      if (old) throw new Error('requestId reuse is forbidden by the staged consumer');
      requests.set(value.requestId, value);
      return value;
    }
    function begin(identityValue, snapshotResponse, requestId, outputHandle, frames) {
      const current = identity(identityValue);
      const pinned = snapshot(snapshotResponse, current);
      const copiedFrames = copyFrames(frames);
      const payload = freeze({ contextId: 'scene-root', quality: 'final', outputHandle: id(outputHandle, 'outputHandle'), frames: copiedFrames });
      const projected = remember(request(current, requestId, 'job.export.png.begin', payload, pinned.atRevision));
      pending.set(projected.requestId, freeze({ request: projected, snapshot: pinned, outputSpec: fixedOutputSpec }));
      return projected;
    }
    function currentJob(identityValue, requestId, jobId, operation) {
      const current = identity(identityValue);
      const session = jobs.get(id(jobId, 'jobId'));
      if (!session || session.snapshot.documentId !== current.documentId || session.request.instanceId !== current.instanceId) throw new Error('job is not bound to this identity');
      return remember(request(current, requestId, operation, freeze({ jobId: session.receipt.jobId })));
    }
    function status(identityValue, requestId, jobId) { return currentJob(identityValue, requestId, jobId, 'job.export.png.status'); }
    function cancel(identityValue, requestId, jobId) { return currentJob(identityValue, requestId, jobId, 'job.export.png.cancel'); }
    function observe(requestValue, response) {
      rejectRaw(requestValue); rejectRaw(response);
      const projected = requests.get(id(requestValue.requestId, 'request.requestId'));
      if (!projected || projected !== requestValue) throw new Error('only this adapter\'s frozen request may be observed');
      exact(response, ['apiVersion', 'requestId', 'instanceId', 'documentId', 'contentRevision', 'ok'], ['result', 'error'], 'response');
      if (response.apiVersion !== 2 || response.requestId !== projected.requestId || response.instanceId !== projected.instanceId || response.documentId !== projected.documentId || !Number.isSafeInteger(response.contentRevision) || response.contentRevision < 0) throw new Error('response identity is invalid');
      if (response.ok === false) {
        if (has(response, 'result') || !has(response, 'error')) throw new Error('failed response must carry only error');
        const failed = freeze({ request: projected, ok: false, error: error(response.error, commonErrors, 'dispatch error') });
        pending.delete(projected.requestId);
        return failed;
      }
      if (response.ok !== true || has(response, 'error') || !has(response, 'result')) throw new Error('successful response must carry only result');
      const received = receipt(response.result);
      terminalInvariant(received);
      const pendingSession = pending.get(projected.requestId);
      const boundSession = projected.payload.jobId === undefined
        ? null : jobs.get(projected.payload.jobId);
      if (projected.payload.jobId !== undefined && projected.payload.jobId !== received.jobId) throw new Error('job receipt does not match the observed jobId');
      const old = jobs.get(received.jobId);
      let session = pendingSession || boundSession;
      if (!session && old && old.request === projected) session = old;
      if (!session) throw new Error('job receipt has no retained export plan');
      if (old && old !== session) throw new Error('jobId cannot be rebound to a different retained export plan');
      if (session.snapshot.documentSnapshotId !== received.documentSnapshotId || session.snapshot.atRevision !== received.pinnedRevision) throw new Error('job receipt does not retain the pinned snapshot and revision');
      if (received.status === 'succeeded' && received.artifact.target !== session.request.payload.outputHandle) {
        throw new Error('successful artifact target must match the retained output handle');
      }
      if (old) {
        if (JSON.stringify(old.receipt) === JSON.stringify(received)) return old;
        if (old.receipt.status !== 'running') throw new Error('terminal job receipt conflicts with retained receipt');
        if (received.progress < old.receipt.progress) throw new Error('job progress cannot regress across lifecycle observations');
      }
      session = freeze({ request: session.request, snapshot: session.snapshot, outputSpec: session.outputSpec, receipt: received });
      jobs.set(received.jobId, session);
      pending.delete(projected.requestId);
      return session;
    }
    function plan(jobId) { return jobs.get(id(jobId, 'jobId')) || null; }
    return freeze({ begin, status, cancel, observe, plan });
  }

  return freeze({ createNativeOpacityExportAdapter });
}));
