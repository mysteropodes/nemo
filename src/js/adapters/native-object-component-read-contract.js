/* #2256: JS side of query.document.object.component.read, the read-only native component model. Pure validation; it owns no
 * document, copies no model and issues no request. One pinned read answers one of four selectors: the contexts of the
 * document (scene-root first, then every component in table order), one context's summary, a page of its layers, or the page
 * of scene layers that instance one component. Pages are at most 16 items and the native owner also cuts them by the 4096-byte
 * response bound, so `nextCursor` (the first unread index, null at the end) is the only trustworthy end of a page.
 * A pinned read carries no expectedRevision. Mirrors nemo-mcp/src/native_component_read_contract.rs. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectComponentReadContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const OP = 'query.document.object.component.read', SCENE = 'scene-root', MAX_PAGE = 16, MAX_ID = 128;
  const KINDS = ['contexts', 'context', 'layers', 'instances'];
  const LAYER_KINDS = ['drawing', 'folder', 'effect', 'widget', 'null', 'instance'];
  const IDENTIFIER = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;
  const has = (value, key) => Object.prototype.hasOwnProperty.call(value, key);
  const fail = (message) => { throw new TypeError(message); };
  const revision = (value) => Number.isSafeInteger(value) && value >= 0;
  const uint32 = (value) => Number.isSafeInteger(value) && value >= 0 && value <= 4294967295;
  const idText = (value) => typeof value === 'string' && value.length > 0 && value.length <= MAX_ID && !value.includes('\0');
  const nameOk = (value) => value === null || typeof value === 'string';
  function exact(value, required, label) {
    if (!value || typeof value !== 'object' || Array.isArray(value) || ![Object.prototype, null].includes(Object.getPrototypeOf(value))) fail(label + ' must be a plain object');
    if (!required.every((key) => has(value, key)) || Object.keys(value).some((key) => !required.includes(key))) fail(label + ' has missing or unknown fields');
    return value;
  }
  function pageArgs(selector) {
    if (!revision(selector.cursor)) fail('invalid component page cursor');
    if (!Number.isSafeInteger(selector.limit) || selector.limit < 1 || selector.limit > MAX_PAGE) fail('invalid component page limit');
  }
  function selectorOk(selector) {
    if (!selector || typeof selector !== 'object' || !KINDS.includes(selector.kind)) fail('invalid component read selector');
    if (selector.kind === 'contexts') { exact(selector, ['kind', 'cursor', 'limit'], 'component selector'); pageArgs(selector); return; }
    if (selector.kind === 'context') exact(selector, ['kind', 'contextId'], 'component selector');
    else { exact(selector, ['kind', 'contextId', 'cursor', 'limit'], 'component selector'); pageArgs(selector); }
    if (!idText(selector.contextId)) fail('invalid component context id');
  }
  function payload(operation, value) {
    if (operation !== OP) fail('component read operation is not declared');
    exact(value, ['atRevision', 'documentSnapshotId', 'selector'], 'component read payload');
    if (!revision(value.atRevision) || typeof value.documentSnapshotId !== 'string' || !IDENTIFIER.test(value.documentSnapshotId)) fail('invalid component read pin');
    selectorOk(value.selector);
  }
  function contextItem(item, first) {
    exact(item, ['contextId', 'contextKind', 'name', 'totalFrames', 'layerCount'], 'component context');
    const scene = item.contextId === SCENE;
    if (!idText(item.contextId) || scene !== first || item.contextKind !== (scene ? 'scene' : 'component') || !nameOk(item.name) ||
      (scene && item.name !== null) || !uint32(item.totalFrames) || !uint32(item.layerCount)) fail('invalid component context');
  }
  function layerItem(item) {
    exact(item, ['layerUid', 'name', 'layerKind'], 'component layer');
    if (!idText(item.layerUid) || !nameOk(item.name) || !LAYER_KINDS.includes(item.layerKind)) fail('invalid component layer');
  }
  function instanceItem(item) {
    exact(item, ['layerUid'], 'component instance');
    if (!idText(item.layerUid)) fail('invalid component instance');
  }
  // The cursor answers the request, the page is within its limit, contiguous and inside the total, and nextCursor is the first unread index.
  function page(value, selector, countKey, each) {
    const count = value[countKey], cursor = value.cursor, items = value.items;
    if (!revision(count) || !revision(cursor) || !Array.isArray(items) || items.length > MAX_PAGE) fail('invalid component page');
    if (cursor !== selector.cursor || items.length > selector.limit || cursor + items.length > count) fail('component page does not answer its request');
    const end = cursor + items.length;
    if (value.nextCursor === null ? end < count : !(revision(value.nextCursor) && value.nextCursor === end && end < count && items.length > 0)) fail('invalid component next cursor');
    items.forEach((item, at) => each(item, cursor === 0 && at === 0));
  }
  function result(operation, value, request) {
    if (operation !== OP) fail('component read operation is not declared');
    if (!value || typeof value !== 'object' || !KINDS.includes(value.kind)) fail('invalid component read result');
    const head = ['atRevision', 'documentSnapshotId', 'kind'];
    if (!revision(value.atRevision) || typeof value.documentSnapshotId !== 'string' || !IDENTIFIER.test(value.documentSnapshotId)) fail('invalid component read result pin');
    const selector = request && request.selector;
    if (request && (value.atRevision !== request.atRevision || value.documentSnapshotId !== request.documentSnapshotId || value.kind !== selector.kind)) fail('component read result does not answer its pin');
    const sameContext = () => { if (!idText(value.contextId) || (selector && value.contextId !== selector.contextId)) fail('component read result names another context'); };
    const pagedCall = (countKey, each) => page(value, selector || { cursor: value.cursor, limit: MAX_PAGE }, countKey, each);
    if (value.kind === 'contexts') { exact(value, head.concat(['contextCount', 'cursor', 'items', 'nextCursor']), 'component contexts result'); pagedCall('contextCount', contextItem); return; }
    if (value.kind === 'layers') { exact(value, head.concat(['contextId', 'layerCount', 'cursor', 'items', 'nextCursor']), 'component layers result'); sameContext(); pagedCall('layerCount', layerItem); return; }
    if (value.kind === 'instances') {
      exact(value, head.concat(['contextId', 'instanceCount', 'cursor', 'items', 'nextCursor']), 'component instances result');
      sameContext();
      if (value.contextId === SCENE) fail('the scene is not instanced');
      pagedCall('instanceCount', instanceItem); return;
    }
    exact(value, head.concat(['contextId', 'contextKind', 'name', 'totalFrames', 'layerCount', 'fps', 'instanceCount', 'exposedPropertyCount', 'hasCameraKeys']), 'component context result');
    sameContext();
    const scene = value.contextId === SCENE;
    if (value.contextKind !== (scene ? 'scene' : 'component') || !nameOk(value.name) || !(value.fps === null || (typeof value.fps === 'number' && Number.isFinite(value.fps) && value.fps > 0)) ||
      !uint32(value.totalFrames) || !uint32(value.layerCount) || !uint32(value.instanceCount) || !uint32(value.exposedPropertyCount) || typeof value.hasCameraKeys !== 'boolean' ||
      (scene && (value.name !== null || value.fps !== null || value.exposedPropertyCount !== 0))) fail('invalid component context summary');
  }
  function correlate(request, response) {
    if (response.requestId !== request.requestId || response.instanceId !== request.instanceId) fail('component read response owner mismatch');
    const replaced = !response.ok && response.error && response.error.code === 'wrong_document';
    if (response.documentId !== request.documentId && !replaced) fail('component read response document mismatch');
    if (!response.ok) return;
    result(OP, response.result, request.payload);
    if (response.contentRevision !== request.payload.atRevision) fail('component read pin mismatch');
  }
  const admitted = (operation) => operation === OP;
  // A layer name is authored text: it may legitimately start with data: or blob:, which the request scan would otherwise refuse.
  const OPAQUE = Object.freeze({ has: (at) => /^result\.(?:name|items\.\d+\.name)$/.test(at) });
  const own = Object.freeze({ admitted, requiresRevision: () => false, payload, result, correlate, paths: () => OPAQUE });
  function compose(base) {
    if (!base) return own;
    return Object.freeze(Object.assign({}, base, {
      admitted: (operation) => admitted(operation) || base.admitted(operation),
      requiresRevision: (operation) => !admitted(operation) && base.requiresRevision(operation),
      payload: (operation, value, envelope) => (admitted(operation) ? payload(operation, value) : base.payload(operation, value, envelope)),
      result: (operation, value) => (admitted(operation) ? result(operation, value) : base.result(operation, value)),
      correlate: (request, response) => (admitted(request.operation) ? correlate(request, response) : base.correlate(request, response)),
      paths: (operation) => (admitted(operation) ? OPAQUE : base.paths(operation)),
    }));
  }
  return Object.freeze({ OP, SCENE, MAX_PAGE, LAYER_KINDS, admitted, requiresRevision: own.requiresRevision, payload, result, correlate, paths: own.paths, compose });
}));
