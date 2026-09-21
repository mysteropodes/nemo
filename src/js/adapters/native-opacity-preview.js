/* N18 dormant native preview receipt consumer. N20 owns activation. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeOpacityPreviewAdapter = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  const identifier = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;
  const terminal = new Set([
    'presented', 'stale-discarded', 'failed-validation', 'failed-device-lost',
    'failed-retry-lost', 'failed-retry-outdated', 'failed-recreate',
    'failed-reconfigure',
  ]);
  const deferred = new Set(['deferred-timeout', 'deferred-occluded']);

  const has = (value, key) => Object.prototype.hasOwnProperty.call(value, key);
  function plain(value) {
    return value !== null && typeof value === 'object' &&
      (Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null);
  }
  function exact(value, keys, label) {
    if (!plain(value) || Object.keys(value).length !== keys.length ||
        !keys.every((key) => has(value, key))) {
      throw new TypeError(`${label} must contain only ${keys.join(', ')}`);
    }
    for (const key of keys) {
      const descriptor = Object.getOwnPropertyDescriptor(value, key);
      if (!descriptor || !has(descriptor, 'value')) throw new TypeError(`${label}.${key} must not be an accessor`);
    }
    return value;
  }
  function id(value, label) {
    if (typeof value !== 'string' || !identifier.test(value)) throw new TypeError(`${label} must be a bounded identifier`);
    return value;
  }
  function revision(value, label, positive) {
    if (!Number.isSafeInteger(value) || value < (positive ? 1 : 0)) {
      throw new TypeError(`${label} must be a ${positive ? 'positive' : 'non-negative'} safe integer`);
    }
    return value;
  }
  function outputSpec(value) {
    exact(value, ['kind', 'format', 'width', 'height', 'colorInterpretation', 'alphaMode'], 'metadata.outputSpec');
    const projected = freeze({
      kind: id(value.kind, 'metadata.outputSpec.kind'),
      format: id(value.format, 'metadata.outputSpec.format'),
      width: revision(value.width, 'metadata.outputSpec.width', true),
      height: revision(value.height, 'metadata.outputSpec.height', true),
      colorInterpretation: id(value.colorInterpretation, 'metadata.outputSpec.colorInterpretation'),
      alphaMode: id(value.alphaMode, 'metadata.outputSpec.alphaMode'),
    });
    if (projected.kind !== 'frame' || projected.format !== 'rgba8' ||
        projected.width !== 320 || projected.height !== 180 ||
        projected.colorInterpretation !== 'srgb' || projected.alphaMode !== 'straight') {
      throw new TypeError('metadata.outputSpec must match the fixed native opacity output contract');
    }
    return projected;
  }
  function geometryHandle(value) {
    exact(value, ['resourceId', 'resourceVersion'], 'metadata.geometryHandle');
    return freeze({
      resourceId: id(value.resourceId, 'metadata.geometryHandle.resourceId'),
      resourceVersion: id(value.resourceVersion, 'metadata.geometryHandle.resourceVersion'),
    });
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
    if (!plain(value)) throw new TypeError('unadmitted object payload is forbidden');
    const visited = seen || new Set();
    if (visited.has(value)) throw new TypeError('cyclic payload is forbidden');
    visited.add(value);
    for (const key of Object.keys(value)) {
      const descriptor = Object.getOwnPropertyDescriptor(value, key);
      if (!descriptor || !has(descriptor, 'value')) throw new TypeError('accessor payload is forbidden');
      rejectRaw(descriptor.value, visited);
    }
    visited.delete(value);
  }
  function freeze(value) { return Object.freeze(value); }
  function metadata(value) {
    rejectRaw(value);
    exact(value, [
      'documentSnapshotId', 'documentId', 'contentRevision', 'contextId', 'frame',
      'quality', 'outputSpec', 'geometryHandle',
    ], 'metadata');
    if (value.quality !== 'final') throw new TypeError('metadata.quality must be final');
    return freeze({
      documentSnapshotId: id(value.documentSnapshotId, 'metadata.documentSnapshotId'),
      documentId: id(value.documentId, 'metadata.documentId'),
      contentRevision: revision(value.contentRevision, 'metadata.contentRevision', false),
      contextId: id(value.contextId, 'metadata.contextId'),
      frame: revision(value.frame, 'metadata.frame', false),
      quality: 'final',
      outputSpec: outputSpec(value.outputSpec),
      geometryHandle: geometryHandle(value.geometryHandle),
    });
  }
  function schedule(value, label) {
    rejectRaw(value);
    exact(value, ['workId', 'viewGeneration'], label);
    return freeze({
      workId: id(value.workId, `${label}.workId`),
      viewGeneration: revision(value.viewGeneration, `${label}.viewGeneration`, true),
    });
  }
  function receipt(value) {
    rejectRaw(value);
    exact(value, ['workId', 'viewGeneration', 'status'], 'receipt');
    const projected = schedule({ workId: value.workId, viewGeneration: value.viewGeneration }, 'receipt');
    if (typeof value.status !== 'string' || (!terminal.has(value.status) && !deferred.has(value.status))) {
      throw new TypeError('receipt.status is unknown');
    }
    return freeze({ ...projected, status: value.status });
  }

  function createNativeOpacityPreviewAdapter() {
    let closed = false;
    let newest = 0;
    const byWork = new Map();
    const byGeneration = new Map();
    const receipts = new Map();
    function active() { if (closed) throw new Error('native opacity preview adapter is disposed'); }
    function register(value, scheduled) {
      active();
      const meta = metadata(value);
      const work = schedule(scheduled, 'schedule');
      const old = byWork.get(work.workId);
      if (old) {
        if (old.viewGeneration !== work.viewGeneration || old.documentSnapshotId !== meta.documentSnapshotId ||
            old.documentId !== meta.documentId || old.contentRevision !== meta.contentRevision ||
            old.contextId !== meta.contextId || old.frame !== meta.frame || old.quality !== meta.quality ||
            JSON.stringify(old.outputSpec) !== JSON.stringify(meta.outputSpec) ||
            JSON.stringify(old.geometryHandle) !== JSON.stringify(meta.geometryHandle)) {
          throw new Error('workId cannot be rebound to different preview metadata');
        }
        return old;
      }
      const priorWork = byGeneration.get(work.viewGeneration);
      if (priorWork && priorWork !== work.workId) throw new Error('viewGeneration cannot name a different workId');
      const binding = freeze({ ...meta, ...work });
      byWork.set(work.workId, binding);
      byGeneration.set(work.viewGeneration, work.workId);
      newest = Math.max(newest, work.viewGeneration);
      return binding;
    }
    function receive(value) {
      active();
      const received = receipt(value);
      const binding = byWork.get(received.workId);
      if (!binding || binding.viewGeneration !== received.viewGeneration) throw new Error('receipt identity is not registered');
      const projected = freeze({ ...binding, status: received.status });
      const old = receipts.get(received.workId);
      if (old) {
        if (old.status !== projected.status) throw new Error('conflicting terminal receipt');
        return old;
      }
      if (received.viewGeneration < newest && projected.status !== 'stale-discarded') {
        throw new Error('older generation must be stale-discarded');
      }
      if (received.viewGeneration === newest && projected.status === 'stale-discarded') {
        throw new Error('newest generation cannot be stale-discarded');
      }
      if (!deferred.has(projected.status)) receipts.set(received.workId, projected);
      return projected;
    }
    return freeze({
      register,
      receive,
      dispose() { closed = true; },
    });
  }

  return freeze({ createNativeOpacityPreviewAdapter });
}));
