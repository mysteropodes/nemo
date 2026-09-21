/* N17 read-only stable-ID selection projection. N20 owns production activation. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeOpacitySelectionAdapter = api;
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
  function opacity(value, label) {
    if (typeof value !== 'number' || !Number.isFinite(value) || value < 0 || value > 100) {
      throw new TypeError(`${label} must be finite and in the range 0..100`);
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

  function projectSelection(evaluation, descriptor) {
    exact(evaluation,
      ['documentSnapshotId', 'documentId', 'contentRevision', 'contextId', 'frame', 'layers'],
      [], 'evaluation');
    const documentSnapshotId = id(evaluation.documentSnapshotId, 'documentSnapshotId');
    const documentId = id(evaluation.documentId, 'documentId');
    const contextId = id(evaluation.contextId, 'contextId');
    const contentRevision = revision(evaluation.contentRevision, 'contentRevision');
    const frame = revision(evaluation.frame, 'frame');
    if (!Array.isArray(evaluation.layers)) throw new TypeError('evaluation.layers must be an array');
    const evaluated = new Map();
    evaluation.layers.forEach((layer, index) => {
      exact(layer, ['layerUid', 'value'], [], `evaluation.layers[${index}]`);
      const layerUid = id(layer.layerUid, `evaluation.layers[${index}].layerUid`);
      if (evaluated.has(layerUid)) throw new TypeError(`evaluation repeats layerUid ${layerUid}`);
      evaluated.set(layerUid, opacity(layer.value, `evaluation.layers[${index}].value`));
    });

    exact(descriptor, ['activeLayerUid', 'selected'], [], 'selection descriptor');
    if (!Array.isArray(descriptor.selected)) throw new TypeError('selection descriptor.selected must be an array');
    const selectedIds = new Set();
    const selected = descriptor.selected.map((entry, index) => {
      exact(entry, ['layerUid', 'opacityMode'], [], `selection descriptor.selected[${index}]`);
      const layerUid = id(entry.layerUid, `selection descriptor.selected[${index}].layerUid`);
      if (!['static', 'keyed'].includes(entry.opacityMode)) {
        throw new TypeError('opacityMode must be static or keyed');
      }
      if (selectedIds.has(layerUid)) throw new TypeError(`selection repeats layerUid ${layerUid}`);
      if (!evaluated.has(layerUid)) throw new TypeError(`selection layerUid ${layerUid} is absent from evaluation`);
      selectedIds.add(layerUid);
      return {
        stableTarget: { layerUid },
        opacityMode: entry.opacityMode,
        editable: entry.opacityMode === 'static',
        value: evaluated.get(layerUid),
      };
    });
    let activeTarget = null;
    if (descriptor.activeLayerUid !== null) {
      const activeLayerUid = id(descriptor.activeLayerUid, 'activeLayerUid');
      if (!selectedIds.has(activeLayerUid)) throw new TypeError('activeLayerUid must name one selected layer');
      activeTarget = { layerUid: activeLayerUid };
    }
    return deepFreeze({
      documentSnapshotId,
      documentId,
      contentRevision,
      contextId,
      frame,
      activeTarget,
      selected,
    });
  }

  return Object.freeze({ projectSelection });
}));
