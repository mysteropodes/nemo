/* N17 pure v2 opacity intent projectors. N20 owns production activation. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeOpacityEditorAdapter = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

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
    if (typeof value !== 'string' || !identifier.test(value)) {
      throw new TypeError(`${label} must be a bounded identifier`);
    }
    return value;
  }
  function revision(value, label) {
    if (!Number.isSafeInteger(value) || value < 0) {
      throw new TypeError(`${label} must be a non-negative safe integer`);
    }
    return value;
  }
  function opacity(value) {
    if (typeof value !== 'number' || !Number.isFinite(value) || value < 0 || value > 100) {
      throw new TypeError('opacity value must be finite and in the range 0..100');
    }
    return value;
  }
  function deepFreeze(value) {
    if (value && typeof value === 'object' && !Object.isFrozen(value)) {
      Object.values(value).forEach(deepFreeze);
      Object.freeze(value);
    }
    return value;
  }
  function clone(value) { return JSON.parse(JSON.stringify(value)); }
  function identity(value) {
    exact(value, ['instanceId', 'documentId', 'contentRevision'], [], 'identity');
    return {
      instanceId: id(value.instanceId, 'identity.instanceId'),
      documentId: id(value.documentId, 'identity.documentId'),
      contentRevision: revision(value.contentRevision, 'identity.contentRevision'),
    };
  }
  function selection(value, operation) {
    exact(value, ['stableTarget', 'opacityMode'], [], 'selection entry');
    exact(value.stableTarget, ['layerUid'], [], 'selection entry stableTarget');
    const layerUid = id(value.stableTarget.layerUid, 'selection entry layerUid');
    if (!['static', 'keyed'].includes(value.opacityMode)) {
      throw new TypeError('selection entry opacityMode must be static or keyed');
    }
    if (value.opacityMode !== 'static') {
      throw new Error(`${operation} is unavailable for read-only keyed opacity`);
    }
    return layerUid;
  }
  function request(identityValue, requestId, operation, payload, expectedRevision) {
    const current = identity(identityValue);
    const projected = {
      apiVersion: 2,
      requestId: id(requestId, 'requestId'),
      instanceId: current.instanceId,
      documentId: current.documentId,
    };
    if (expectedRevision) projected.expectedRevision = current.contentRevision;
    projected.operation = operation;
    projected.payload = payload;
    return deepFreeze(projected);
  }
  function transactionRequest(identityValue, requestId, operation, transactionId, value) {
    const payload = { transactionId: id(transactionId, 'transactionId') };
    if (value !== undefined) payload.value = opacity(value);
    return request(identityValue, requestId, operation, payload, false);
  }

  function queryOpacity(identityValue, requestId, selectionEntry, atRevision) {
    const layerUid = selection(selectionEntry, 'query.document.opacity');
    const payload = { stableTarget: { layerUid } };
    if (atRevision !== undefined) payload.atRevision = revision(atRevision, 'atRevision');
    return request(identityValue, requestId, 'query.document.opacity', payload, false);
  }
  function setOpacity(identityValue, requestId, selectionEntry, value) {
    const layerUid = selection(selectionEntry, 'command.document.apply');
    return request(identityValue, requestId, 'command.document.apply', {
      command: 'layer.opacity.set', stableTarget: { layerUid }, value: opacity(value),
    }, true);
  }
  function beginGesture(identityValue, requestId, selectionEntry) {
    const layerUid = selection(selectionEntry, 'transaction.begin');
    return request(identityValue, requestId, 'transaction.begin', {
      stableTarget: { layerUid },
    }, true);
  }
  function updateGesture(identityValue, requestId, transactionId, value) {
    return transactionRequest(identityValue, requestId, 'transaction.update', transactionId, value);
  }
  function commitGesture(identityValue, requestId, transactionId) {
    return transactionRequest(identityValue, requestId, 'transaction.commit', transactionId);
  }
  function cancelGesture(identityValue, requestId, transactionId) {
    return transactionRequest(identityValue, requestId, 'transaction.cancel', transactionId);
  }
  function undo(identityValue, requestId) {
    return request(identityValue, requestId, 'history.undo', {}, true);
  }
  function redo(identityValue, requestId) {
    return request(identityValue, requestId, 'history.redo', {}, true);
  }
  function cancelBeforeDispatch(value) {
    exact(value, ['apiVersion', 'requestId', 'instanceId', 'documentId', 'operation', 'payload'],
      ['expectedRevision', 'cancelledBeforeDispatch'], 'projected request');
    if (value.apiVersion !== 2) throw new TypeError('projected request apiVersion must be 2');
    const copy = clone(value);
    copy.cancelledBeforeDispatch = true;
    return deepFreeze(copy);
  }

  return Object.freeze({
    queryOpacity,
    setOpacity,
    beginGesture,
    updateGesture,
    commitGesture,
    cancelGesture,
    undo,
    redo,
    cancelBeforeDispatch,
  });
}));
