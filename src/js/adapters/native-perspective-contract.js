/* Field-specific perspective protocol. Native owns persisted values and history. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativePerspectiveContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var READ = 'query.document.object.project.perspective.read';
  var EDIT = 'command.document.object.project.perspective.edit';
  var SAFE = Number.MAX_SAFE_INTEGER;
  var has = function (value, key) { return Object.prototype.hasOwnProperty.call(value, key); };
  function fail() { throw new TypeError('Invalid native perspective contract'); }
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
  function mode(value) { if (!['1pt', '2pt', '3pt', 'fisheye'].includes(value)) fail(); }
  function action(value) {
    data(value);
    var fields = { setEnabled: ['enabled'], setMode: ['mode'], reset: [], lockAll: ['locked'],
      move: ['ordinal', 'x', 'y'], translate: ['dx', 'dy'] };
    if (typeof value.kind !== 'string' || !has(fields, value.kind)) fail();
    exact(value, ['kind'].concat(fields[value.kind]));
    ['enabled', 'locked'].forEach(function (key) { if (has(value, key) && typeof value[key] !== 'boolean') fail(); });
    ['x', 'y', 'dx', 'dy'].forEach(function (key) { if (has(value, key)) finite(value[key]); });
    if (has(value, 'ordinal')) integer(value.ordinal, 1, SAFE);
    if (has(value, 'mode')) mode(value.mode);
    return Object.freeze(Object.assign({}, value));
  }
  function selector(value) {
    data(value);
    if (value.kind === 'settings') exact(value, ['kind']);
    else if (value.kind === 'vpPage') {
      exact(value, ['kind', 'cursor', 'limit']); integer(value.cursor, 0, SAFE); integer(value.limit, 1, 16);
    } else fail();
  }
  function payload(operation, value) {
    if (operation === EDIT) {
      exact(value, ['command', 'action']);
      if (value.command !== 'object.project.perspective.edit') fail();
      action(value.action);
    } else if (operation === READ) {
      exact(value, ['atRevision', 'documentSnapshotId', 'contextId', 'selector']);
      integer(value.atRevision, 0, SAFE); identifier(value.documentSnapshotId);
      if (typeof value.contextId !== 'string' || !value.contextId) fail();
      selector(value.selector);
    } else fail();
  }
  var counts = ['perspectiveVPsPresent', 'perspectiveVPsNull', 'perspectiveVPCount', 'effectiveVPCount', 'vpSource'];
  function count(value) {
    integer(value.perspectiveVPCount, 0, SAFE); integer(value.effectiveVPCount, 1, SAFE);
    if (typeof value.perspectiveVPsPresent !== 'boolean' || typeof value.perspectiveVPsNull !== 'boolean' ||
        !value.perspectiveVPsPresent && (value.perspectiveVPsNull || value.perspectiveVPCount !== 0) ||
        value.perspectiveVPsNull && value.perspectiveVPCount !== 0) fail();
    if (value.vpSource === 'stored') {
      if (!value.perspectiveVPCount || value.effectiveVPCount !== value.perspectiveVPCount) fail();
    } else if (value.vpSource === 'default') {
      if (value.perspectiveVPCount !== 0 || value.effectiveVPCount > 3) fail();
    } else fail();
  }
  function settings(value) {
    exact(value, counts, ['perspectiveEnabled', 'perspectiveMode', 'perspectiveDensity']); count(value);
    if (has(value, 'perspectiveEnabled') && typeof value.perspectiveEnabled !== 'boolean') fail();
    if (has(value, 'perspectiveMode')) text(value.perspectiveMode);
    if (has(value, 'perspectiveDensity')) finite(value.perspectiveDensity);
    var expected = value.perspectiveMode === '3pt' ? 3 : ['1pt', 'fisheye'].includes(value.perspectiveMode) ? 1 : 2;
    if (value.vpSource === 'default' && value.effectiveVPCount !== expected) fail();
  }
  function page(value) {
    exact(value, ['kind'].concat(counts, ['cursor', 'items', 'nextCursor']));
    if (value.kind !== 'perspectiveVPPage') fail(); count(value); integer(value.cursor, 0, value.effectiveVPCount);
    var items = value.items;
    if (!Array.isArray(items) || Object.getOwnPropertySymbols(items).length || items.length > 16 ||
        Object.getOwnPropertyNames(items).length !== items.length + 1) fail();
    for (var i = 0; i < items.length; i++) {
      var descriptor = Object.getOwnPropertyDescriptor(items, String(i));
      if (!descriptor || !has(descriptor, 'value')) fail();
      var item = descriptor.value; exact(item, ['ordinal', 'x', 'y'], ['locked']); finite(item.x); finite(item.y);
      if (has(item, 'locked') && typeof item.locked !== 'boolean' || value.vpSource === 'default' && item.locked !== false) fail();
      if (item.ordinal !== value.cursor + i + 1) fail();
    }
    var end = value.cursor + items.length;
    if (!Number.isSafeInteger(end) || end > value.effectiveVPCount) fail();
    if (end < value.effectiveVPCount) { if (!items.length || value.nextCursor !== end) fail(); }
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
    else if (value.kind !== 'perspectiveVPPage' || value.cursor !== wanted.cursor || value.items.length > wanted.limit) fail();
  }

  function admitted(operation) { return operation === READ || operation === EDIT; }
  function paths(operation) {
    if (!admitted(operation)) return null;
    return { has: function (path) {
      return path === 'result.result.perspectiveMode';
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
