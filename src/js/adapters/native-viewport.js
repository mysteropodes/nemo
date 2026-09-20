/* N13 metadata-only adapter. N16 owns real surface/context activation. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeViewportAdapter = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  const identifier = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/;
  const terminal = new Set([
    'presented', 'stale-discarded', 'failed-validation', 'failed-device-lost',
    'failed-retry-lost', 'failed-retry-outdated', 'failed-recreate',
    'failed-reconfigure',
  ]);
  const deferred = new Set(['deferred-timeout', 'deferred-occluded']);

  function plain(value) {
    return value !== null && typeof value === 'object' &&
      (Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null);
  }
  function exact(value, keys, label) {
    if (!plain(value) || Object.keys(value).length !== keys.length ||
        !keys.every((key) => Object.prototype.hasOwnProperty.call(value, key))) {
      throw new TypeError(`${label} must contain only ${keys.join(', ')}`);
    }
    return value;
  }
  function finitePositive(value, label) {
    if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) throw new TypeError(`${label} must be finite and positive`);
    return value;
  }
  function finiteNumber(value, label) {
    if (typeof value !== 'number' || !Number.isFinite(value)) throw new TypeError(`${label} must be finite`);
    return value;
  }
  function validId(value, label) {
    if (typeof value !== 'string' || !identifier.test(value)) throw new TypeError(`${label} must be a bounded identifier`);
    return value;
  }
  function rejectPayload(value, seen) {
    if (typeof value === 'string') {
      if (/^(data|blob):/i.test(value)) throw new TypeError('pixel or binary URL payload is forbidden');
      return;
    }
    if (value === null || ['number', 'boolean', 'undefined'].includes(typeof value)) return;
    if (typeof value !== 'object' && typeof value !== 'function') throw new TypeError('unsupported payload value');
    if ((typeof ArrayBuffer !== 'undefined' && (value instanceof ArrayBuffer || ArrayBuffer.isView(value))) ||
        (typeof Buffer !== 'undefined' && Buffer.isBuffer && Buffer.isBuffer(value)) ||
        (typeof Blob !== 'undefined' && value instanceof Blob) ||
        (typeof ImageData !== 'undefined' && value instanceof ImageData) ||
        (value && (typeof value.getContext === 'function' || 'canvas' in Object(value)))) {
      throw new TypeError('pixel, image, canvas, or binary payload is forbidden');
    }
    if (!plain(value)) throw new TypeError('unadmitted object payload is forbidden');
    const visited = seen || new Set();
    if (visited.has(value)) throw new TypeError('cyclic payload is forbidden');
    visited.add(value);
    Object.keys(value).forEach((key) => rejectPayload(value[key], visited));
    visited.delete(value);
  }
  function statusOf(value) {
    if (typeof value !== 'string' || (!terminal.has(value) && !deferred.has(value))) throw new TypeError('unknown viewport receipt status');
    return value;
  }

  function createNativeViewportAdapter(transport) {
    if (!transport || typeof transport.send !== 'function' || typeof transport.dispose !== 'function') throw new TypeError('injected transport requires send and dispose');
    let closed = false;
    let newest = -1;
    let requestSequence = 0;
    const generations = new Map();
    const workGenerations = new Map();
    const receipts = new Map();
    function active() { if (closed) throw new Error('native viewport adapter is disposed'); }
    function send(kind, value) {
      active(); rejectPayload(value);
      transport.send(kind, Object.freeze({ ...value, requestSequence: requestSequence++ }));
    }
    function register(workId, viewGeneration) {
      active(); rejectPayload(workId); validId(workId, 'workId');
      if (!Number.isSafeInteger(viewGeneration) || viewGeneration < 1) throw new TypeError('viewGeneration must be a positive safe integer');
      const known = generations.get(viewGeneration);
      if (known && known !== workId) throw new Error('same generation cannot name a different WorkId');
      const knownGeneration = workGenerations.get(workId);
      if (knownGeneration && knownGeneration !== viewGeneration) throw new Error('same WorkId cannot name a different generation');
      if (generations.has(viewGeneration) && known === workId) return;
      generations.set(viewGeneration, workId); workGenerations.set(workId, viewGeneration); newest = Math.max(newest, viewGeneration);
      send('register', { workId, viewGeneration });
    }
    function configure(bounds, reportedDpr) {
      rejectPayload(bounds); exact(bounds, ['x', 'y', 'width', 'height'], 'bounds');
      ['x', 'y', 'width', 'height'].forEach((key) => {
        if (typeof bounds[key] !== 'number' || !Number.isFinite(bounds[key]) || ((key === 'width' || key === 'height') && bounds[key] <= 0)) throw new TypeError(`bounds.${key} is invalid`);
      });
      send('configure', { bounds: Object.freeze({ ...bounds }), reportedDpr: finitePositive(reportedDpr, 'reportedDpr') });
    }
    function pointer(clientX, clientY) {
      send('pointer', { clientX: finiteNumber(clientX, 'clientX'), clientY: finiteNumber(clientY, 'clientY') });
    }
    function selection(x0, y0, x1, y1) {
      const edges = [x0, y0, x1, y1];
      if (!edges.every((value) => typeof value === 'number' && Number.isFinite(value)) || x1 < x0 || y1 < y0) throw new TypeError('selection edges are invalid');
      send('selection', { x0, y0, x1, y1 });
    }
    function receive(value) {
      active(); rejectPayload(value); exact(value, ['workId', 'viewGeneration', 'status'], 'receipt');
      validId(value.workId, 'workId');
      if (!Number.isSafeInteger(value.viewGeneration) || value.viewGeneration < 1) throw new TypeError('viewGeneration must be a positive safe integer');
      const status = statusOf(value.status);
      if (generations.get(value.viewGeneration) !== value.workId) throw new Error('receipt identity is not registered');
      const frozen = Object.freeze({ workId: value.workId, viewGeneration: value.viewGeneration, status });
      const old = receipts.get(value.workId);
      if (old) {
        if (old.viewGeneration !== frozen.viewGeneration || old.status !== frozen.status) throw new Error('conflicting terminal receipt');
        return old;
      }
      if (value.viewGeneration < newest && status !== 'stale-discarded') throw new Error('older generation must be stale-discarded');
      if (value.viewGeneration === newest && status === 'stale-discarded') throw new Error('newest generation cannot be stale-discarded');
      if (deferred.has(status)) return frozen;
      receipts.set(value.workId, frozen);
      return frozen;
    }
    return Object.freeze({ configure, register, pointer, selection, receive, dispose() { if (!closed) { closed = true; transport.dispose(); } } });
  }
  return Object.freeze({ createNativeViewportAdapter });
}));
