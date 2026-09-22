/* N20 pure native-opacity protocol and v1 compatibility validation. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeOpacityContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  var READS = Object.freeze(['capabilities', 'snapshot', 'property.get', 'diagnostics.trace']);
  var WRITES = Object.freeze(['property.set', 'property.key.set', 'property.key.remove',
    'property.animation.set', 'history.undo', 'history.redo', 'diagnostics.replay']);
  var PROPERTY_WRITES = Object.freeze(WRITES.slice(0, 4));

  function clone(value) { return JSON.parse(JSON.stringify(value)); }
  function has(value, key) { return Object.prototype.hasOwnProperty.call(value, key); }
  function plain(value) {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
    var proto = Object.getPrototypeOf(value);
    return proto === Object.prototype || proto === null;
  }
  function exact(value, keys) {
    return plain(value) && Object.keys(value).sort().join(',') === keys.slice().sort().join(',') &&
      keys.every(function (key) {
        var descriptor = Object.getOwnPropertyDescriptor(value, key);
        return descriptor && has(descriptor, 'value');
      });
  }
  function bounded(value) {
    return typeof value === 'string' && /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(value);
  }
  function revision(value, positive) {
    return Number.isSafeInteger(value) && value >= (positive ? 1 : 0);
  }
  function successful(response, label) {
    if (!response || response.ok !== true) throw new Error(label + ' failed: ' +
      (response && response.error && response.error.message || 'unknown native error'));
    return response;
  }
  function expectedValue(frame, document) {
    if (!document.layers[0].motion) return document.layers[0].motionStatic.opacity[0];
    if (frame === 0) return 20;
    if (frame === 10) return 50;
    if (frame === 20) return 80;
    return null;
  }
  function validateBootstrap(receipt, binding, prepared) {
    var keys = receipt && Object.keys(receipt).sort().join(',');
    if (keys !== 'apiVersion,contentRevision,documentId,instanceId,resourceCount,viewportAvailable' ||
        receipt.apiVersion !== 2 || receipt.instanceId !== binding.instanceId ||
        receipt.documentId !== binding.documentId || receipt.contentRevision !== binding.contentRevision ||
        receipt.contentRevision !== 0 || receipt.resourceCount !== prepared.resources.length ||
        receipt.viewportAvailable !== true) {
      throw new Error('native bootstrap receipt is not the N20 viewport contract');
    }
    return receipt;
  }
  function validateSubscription(binding, connected, observedHostGeneration) {
    if (!exact(binding, ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'subscriptionId']) ||
        binding.instanceId !== connected.instanceId || binding.documentId !== connected.documentId ||
        binding.contentRevision !== connected.contentRevision || !bounded(binding.subscriptionId) ||
        !revision(binding.lifecycleGeneration, true) ||
        observedHostGeneration !== null && observedHostGeneration !== binding.lifecycleGeneration) {
      throw new Error('native revision subscription does not match the connected application');
    }
    return binding.lifecycleGeneration;
  }
  function validateIds(values) {
    return Array.isArray(values) && values.every(bounded) && new Set(values).size === values.length;
  }
  function validateCancelledTransaction(receipt) {
    if (receipt.cancelledTransactionId === null) return receipt.cancelledTransaction === null;
    var value = receipt.cancelledTransaction;
    return bounded(receipt.cancelledTransactionId) &&
      exact(value, ['transactionId', 'baseRevision', 'workingGeneration', 'workingState',
        'committedRevision', 'terminalDisposition', 'historyEntriesAdded', 'committedValue']) &&
      value.transactionId === receipt.cancelledTransactionId && revision(value.baseRevision, false) &&
      revision(value.workingGeneration, true) && value.committedRevision === null &&
      value.terminalDisposition === 'cancelled' && value.historyEntriesAdded === 0 &&
      Number.isFinite(value.committedValue) && exact(value.workingState, ['stableTarget', 'value']) &&
      exact(value.workingState.stableTarget, ['layerUid']) && bounded(value.workingState.stableTarget.layerUid) &&
      Number.isFinite(value.workingState.value);
  }
  function validateExport(value) {
    return exact(value, ['jobId', 'status', 'cleanupStatus', 'externalEffectDisposition', 'errorCode']) &&
      bounded(value.jobId) && ['succeeded', 'failed', 'cancelled'].includes(value.status) &&
      ['not_required', 'complete'].includes(value.cleanupStatus) &&
      ['none', 'contained', 'committed'].includes(value.externalEffectDisposition) &&
      (value.errorCode === null || bounded(value.errorCode));
  }
  function validateRelease(receipt, current, request, hostGeneration) {
    var keys = ['apiVersion', 'requestId', 'instanceId', 'documentId', 'contentRevision',
      'lifecycleGeneration', 'status', 'retrieved', 'authorityRemovalCompleted',
      'cancelledTransactionId', 'cancelledTransaction', 'undoDepth', 'redoDepth',
      'reconciledExports', 'cancelledPreviewWorkIds', 'unresolvedPreviewWorkIds',
      'disposedViewportWorkIds', 'reconciliationStages', 'viewportStatus',
      'reentryAvailable', 'error'];
    if (!exact(receipt, keys) || receipt.apiVersion !== 2 || receipt.requestId !== request.requestId ||
        receipt.instanceId !== current.instanceId || receipt.documentId !== current.documentId ||
        receipt.contentRevision !== current.contentRevision || receipt.status !== 'succeeded' ||
        receipt.retrieved !== false || receipt.authorityRemovalCompleted !== true ||
        receipt.reentryAvailable !== true || receipt.error !== null ||
        !revision(receipt.lifecycleGeneration, true) ||
        hostGeneration !== null && receipt.lifecycleGeneration !== hostGeneration ||
        !revision(receipt.undoDepth, false) || !revision(receipt.redoDepth, false) ||
        !validateCancelledTransaction(receipt) || !Array.isArray(receipt.reconciledExports) ||
        !receipt.reconciledExports.every(validateExport) || !validateIds(receipt.cancelledPreviewWorkIds) ||
        !validateIds(receipt.unresolvedPreviewWorkIds) || receipt.unresolvedPreviewWorkIds.length !== 0 ||
        !validateIds(receipt.disposedViewportWorkIds) ||
        !exact(receipt.reconciliationStages, ['transaction', 'exports', 'preview']) ||
        receipt.reconciliationStages.transaction !== 'complete' ||
        receipt.reconciliationStages.exports !== 'complete' ||
        receipt.reconciliationStages.preview !== 'complete' ||
        !['already_absent', 'disposed'].includes(receipt.viewportStatus)) {
      throw new Error('native release receipt is not a successful terminal receipt');
    }
    return receipt;
  }
  function validateRevisionEvent(event, identity, hostGeneration, observedHostGeneration) {
    if (!exact(event, ['instanceId', 'documentId', 'lifecycleGeneration', 'fromRevision', 'toRevision', 'requestId']) ||
        event.instanceId !== identity.instanceId || event.documentId !== identity.documentId ||
        !bounded(event.requestId) || !revision(event.lifecycleGeneration, true) ||
        !revision(event.fromRevision, false) || !revision(event.toRevision, false) ||
        event.toRevision <= event.fromRevision ||
        hostGeneration !== null && event.lifecycleGeneration !== hostGeneration ||
        observedHostGeneration !== null && event.lifecycleGeneration !== observedHostGeneration) {
      throw new Error('native revision event does not match the active lifecycle');
    }
    return event.lifecycleGeneration;
  }
  function v1Frame(prepared, value) {
    return Number.isInteger(value) && value >= 0 && value < prepared.totalFrames;
  }
  function v1Target(prepared, payload) {
    return plain(payload) && payload.layerId === prepared.layerUid && payload.property === 'opacity';
  }
  function v1PropertyFields(prepared, payload) {
    var allowed = ['layerId', 'property', 'value', 'frame', 'animated'];
    return plain(payload) && Object.keys(payload).every(function (key) { return allowed.includes(key); }) &&
      (!has(payload, 'value') || Number.isFinite(payload.value) && payload.value >= 0 && payload.value <= 100) &&
      (!has(payload, 'frame') || payload.frame === null || v1Frame(prepared, payload.frame)) &&
      (!has(payload, 'animated') || typeof payload.animated === 'boolean') && !has(payload, 'curvePoints');
  }
  function v1PropertyPayload(prepared, operation, payload) {
    if (!v1PropertyFields(prepared, payload) || !v1Target(prepared, payload)) return false;
    if (operation === 'property.get') return true;
    if (operation === 'property.set') return has(payload, 'value');
    if (operation === 'property.key.set') return v1Frame(prepared, payload.frame) && has(payload, 'value');
    if (operation === 'property.key.remove') return v1Frame(prepared, payload.frame);
    if (operation === 'property.animation.set') return has(payload, 'animated');
    return false;
  }
  function registered(operation, registrations) {
    return registrations.some(function (entry) {
      var effects = entry && entry.descriptor && entry.descriptor.effects;
      return effects && Array.isArray(effects.lifecycle) && effects.lifecycle.includes(operation);
    });
  }
  function capabilitySummary(registrations) {
    var descriptors = registrations.map(function (entry) { return clone(entry.descriptor); });
    var properties = descriptors.filter(function (descriptor) {
      return descriptor.effects && Array.isArray(descriptor.effects.lifecycle) &&
        descriptor.effects.lifecycle.includes('property.get');
    }).map(function (descriptor) {
      var value = descriptor.input && descriptor.input.properties && descriptor.input.properties.value || {};
      var lifecycle = descriptor.effects.lifecycle;
      return { id: descriptor.id, min: value.minimum, max: value.maximum,
        unit: descriptor.units && descriptor.units.value,
        animated: lifecycle.includes('property.animation.set') };
    });
    return { operations: READS.concat(WRITES), properties: properties, descriptors: descriptors,
      retryRetention: 256, traceRetention: 32, documentIdentity: 'open-document-incarnation' };
  }
  function v1PayloadValid(prepared, operation, payload) {
    if (PROPERTY_WRITES.includes(operation) || operation === 'property.get') {
      return v1PropertyPayload(prepared, operation, payload);
    }
    if (operation === 'snapshot' || operation === 'history.undo' || operation === 'history.redo' ||
        operation === 'capabilities' || operation === 'diagnostics.trace') return plain(payload);
    if (operation === 'diagnostics.replay') {
      return plain(payload) && plain(payload.request) && has(payload.request, 'operation') &&
        has(payload.request, 'payload') && PROPERTY_WRITES.includes(payload.request.operation) &&
        v1PropertyPayload(prepared, payload.request.operation, payload.request.payload);
    }
    if (operation === 'start') return plain(payload) && Number.isInteger(payload.frameIdx) && payload.frameIdx >= 0;
    if (operation === 'status' || operation === 'cancel') {
      return plain(payload) && typeof payload.jobId === 'string' && payload.jobId.length > 0;
    }
    return false;
  }
  function fingerprint(request) {
    return JSON.stringify({ apiVersion: request.apiVersion, instanceId: request.instanceId,
      documentId: request.documentId, expectedRevision: request.expectedRevision,
      operation: request.operation, payload: request.payload });
  }

  return Object.freeze({ READS: READS, WRITES: WRITES, PROPERTY_WRITES: PROPERTY_WRITES,
    clone: clone, has: has, plain: plain, bounded: bounded, successful: successful,
    expectedValue: expectedValue, validateBootstrap: validateBootstrap,
    validateSubscription: validateSubscription, validateRelease: validateRelease,
    validateRevisionEvent: validateRevisionEvent, registered: registered,
    capabilitySummary: capabilitySummary, v1PayloadValid: v1PayloadValid,
    fingerprint: fingerprint });
}));
