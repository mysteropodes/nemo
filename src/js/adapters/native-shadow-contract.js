/* Field-specific shadow protocol. Native owns persisted values and history. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeShadowContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var READ = 'query.document.object.project.shadow.read';
  var EDIT = 'command.document.object.project.shadow.edit';
  var SAFE = Number.MAX_SAFE_INTEGER;
  var has = function (value, key) { return Object.prototype.hasOwnProperty.call(value, key); };
  function fail() { throw new TypeError('Invalid native shadow contract'); }
  function data(value) {
    if (!value || typeof value !== 'object' || Array.isArray(value) ||
        ![Object.prototype, null].includes(Object.getPrototypeOf(value)) ||
        Object.getOwnPropertySymbols(value).length) fail();
    var keys = Object.keys(value);
    if (Object.getOwnPropertyNames(value).length !== keys.length || keys.some(function (key) {
      return !has(Object.getOwnPropertyDescriptor(value, key), 'value');
    })) fail();
  }
  function exact(value, required, optional) {
    data(value);
    var allowed = required.concat(optional || []);
    if (required.some(function (key) { return !has(value, key); }) ||
        Object.keys(value).some(function (key) { return !allowed.includes(key); })) fail();
  }
  function integer(value, min, max) {
    if (!Number.isSafeInteger(value) || value < min || value > max) fail();
  }
  function finite(value) { if (typeof value !== 'number' || !Number.isFinite(value)) fail(); }
  function text(value) {
    if (typeof value !== 'string' || value.includes('\0')) fail();
    var bytes = 0;
    for (var i = 0; i < value.length; i++) {
      var code = value.charCodeAt(i);
      if (code >= 0xD800 && code <= 0xDBFF) {
        var low = value.charCodeAt(++i);
        if (!(low >= 0xDC00 && low <= 0xDFFF)) fail();
        bytes += 4;
      } else if (code >= 0xDC00 && code <= 0xDFFF) fail();
      else bytes += code < 128 ? 1 : code < 2048 ? 2 : 3;
    }
    if (bytes > 1024) fail();
  }
  function identifier(value) {
    if (typeof value !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(value)) fail();
  }
  function action(value) {
    data(value);
    if (value.kind === 'add') { exact(value, ['kind', 'id', 'color']); text(value.id); text(value.color); }
    else if (value.kind === 'select') { exact(value, ['kind', 'ordinal']); integer(value.ordinal, 1, SAFE); }
    else fail();
    return Object.freeze(Object.assign({}, value));
  }
  function selector(value) {
    data(value);
    if (value.kind === 'settings') exact(value, ['kind']);
    else if (value.kind === 'palettePage') {
      exact(value, ['kind', 'cursor', 'limit']); integer(value.cursor, 0, SAFE); integer(value.limit, 1, 16);
    } else fail();
  }
  function payload(operation, value) {
    if (operation === EDIT) {
      exact(value, ['command', 'action']);
      if (value.command !== 'object.project.shadow.edit') fail();
      action(value.action);
    } else if (operation === READ) {
      exact(value, ['atRevision', 'documentSnapshotId', 'contextId', 'selector']);
      integer(value.atRevision, 0, SAFE); identifier(value.documentSnapshotId);
      if (typeof value.contextId !== 'string' || !value.contextId) fail();
      selector(value.selector);
    } else fail();
  }
  function count(value) {
    integer(value.shadowColorCount, 0, SAFE);
    if (typeof value.shadowPalettePresent !== 'boolean' || !value.shadowPalettePresent && value.shadowColorCount !== 0) fail();
  }
  function settings(value) {
    exact(value, ['shadowPalettePresent', 'shadowColorCount'], ['shadowActiveId']);
    count(value); if (has(value, 'shadowActiveId')) text(value.shadowActiveId);
  }
  function page(value) {
    exact(value, ['kind', 'shadowPalettePresent', 'shadowColorCount', 'cursor', 'items', 'nextCursor']);
    if (value.kind !== 'shadowPalettePage') fail(); count(value);
    integer(value.cursor, 0, value.shadowColorCount);
    var items = value.items;
    if (!Array.isArray(items) || Object.getOwnPropertySymbols(items).length || items.length > 16 ||
        Object.getOwnPropertyNames(items).length !== items.length + 1) fail();
    for (var i = 0; i < items.length; i++) {
      var descriptor = Object.getOwnPropertyDescriptor(items, String(i));
      if (!descriptor || !has(descriptor, 'value')) fail();
      var item = descriptor.value; exact(item, ['ordinal', 'id', 'color']); text(item.id); text(item.color);
      if (item.ordinal !== value.cursor + i + 1) fail();
    }
    var end = value.cursor + items.length;
    if (!Number.isSafeInteger(end) || end > value.shadowColorCount) fail();
    if (end < value.shadowColorCount) { if (!items.length || value.nextCursor !== end) fail(); }
    else if (value.nextCursor !== null) fail();
  }
  function result(operation, value) {
    if (operation === EDIT) {
      exact(value, ['applied', 'historyEntriesAdded']);
      if (typeof value.applied !== 'boolean' || value.historyEntriesAdded !== (value.applied ? 1 : 0)) fail();
      return;
    }
    if (operation !== READ) fail();
    exact(value, ['instanceId', 'documentId', 'atRevision', 'documentSnapshotId', 'contextId', 'result']);
    ['instanceId', 'documentId', 'documentSnapshotId'].forEach(function (key) { identifier(value[key]); });
    integer(value.atRevision, 0, SAFE);
    if (value.contextId !== 'scene-root') fail();
    data(value.result);
    if (has(value.result, 'kind')) page(value.result);
    else settings(value.result);
  }
  function correlate(request, response) {
    if (typeof response.ok !== 'boolean') fail();
    integer(response.contentRevision, 0, SAFE);
    if (response.requestId !== request.requestId || response.instanceId !== request.instanceId) fail();
    var replaced = !response.ok && response.error && response.error.code === 'wrong_document';
    if (response.documentId !== request.documentId && !replaced) fail();
    if (!response.ok) return;
    result(request.operation, response.result);
    if (request.operation === EDIT) {
      if (response.contentRevision !== request.expectedRevision + (response.result.applied ? 1 : 0)) fail();
      return;
    }
    var p = request.payload, r = response.result, value = r.result, wanted = p.selector;
    if (response.contentRevision !== p.atRevision || r.instanceId !== request.instanceId || r.documentId !== request.documentId ||
        r.atRevision !== p.atRevision || r.documentSnapshotId !== p.documentSnapshotId || r.contextId !== p.contextId) fail();
    if (wanted.kind === 'settings') { if (has(value, 'kind')) fail(); }
    else if (value.kind !== 'shadowPalettePage' || value.cursor !== wanted.cursor || value.items.length > wanted.limit) fail();
  }

  function admitted(operation) { return operation === READ || operation === EDIT; }
  function paths(operation) {
    if (!admitted(operation)) return null;
    return { has: function (path) {
      return /^(?:payload\.action|result\.result\.items\.\d+)\.(?:id|color)$/.test(path) || path === 'result.result.shadowActiveId';
    } };
  }
  function compose(base) {
    function pick(operation) { if (admitted(operation)) return api; if (!base) fail(); return base; }
    return Object.freeze({
      admitted: function (operation) { return admitted(operation) || !!base && base.admitted(operation); },
      requiresRevision: function (operation) { return admitted(operation) ? operation === EDIT : !!base && base.requiresRevision(operation); },
      requestByteLimit: function (operation) { return admitted(operation) ? 4096 : base && base.requestByteLimit ? base.requestByteLimit(operation) : 4096; },
      responseByteLimit: function (operation) { return admitted(operation) ? 4096 : base && base.responseByteLimit ? base.responseByteLimit(operation) : 4096; },
      payload: function (operation, value, envelope) { return pick(operation).payload(operation, value, envelope); },
      result: function (operation, value) { return pick(operation).result(operation, value); },
      correlate: function (request, response) { return pick(request.operation).correlate(request, response); },
      paths: function (operation) { return pick(operation).paths(operation); }
    });
  }
  var api = Object.freeze({ READ: READ, EDIT: EDIT, admitted: admitted, action: action,
    requiresRevision: function (operation) { return operation === EDIT; }, payload: payload, result: result,
    correlate: correlate, paths: paths, compose: compose, join: compose,
    requestByteLimit: function () { return 4096; }, responseByteLimit: function () { return 4096; } });
  return api;
}));
