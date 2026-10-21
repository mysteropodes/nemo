/* Field-specific guide protocol. Native owns persisted values and history. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeGuideContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var READ = 'query.document.object.project.guide.read';
  var EDIT = 'command.document.object.project.guide.edit';
  var SAFE = Number.MAX_SAFE_INTEGER;
  var has = function (value, key) { return Object.prototype.hasOwnProperty.call(value, key); };
  function fail() { throw new TypeError('Invalid native guide contract'); }
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
  function identifier(value) {
    if (typeof value !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(value)) fail();
  }
  function axis(value) { if (value !== 'h' && value !== 'v') fail(); }
  function action(value) {
    data(value);
    var fields = { add: ['axis', 'position'], move: ['axis', 'ordinal', 'position'], remove: ['axis', 'ordinal'] };
    if (typeof value.kind !== 'string' || !has(fields, value.kind)) fail();
    exact(value, ['kind'].concat(fields[value.kind])); axis(value.axis);
    if (has(value, 'ordinal')) integer(value.ordinal, 1, SAFE);
    if (has(value, 'position')) finite(value.position);
    return Object.freeze(Object.assign({}, value));
  }
  function selector(value) {
    data(value);
    if (value.kind === 'settings') exact(value, ['kind']);
    else if (value.kind === 'axisPage') {
      exact(value, ['kind', 'axis', 'cursor', 'limit']); axis(value.axis);
      integer(value.cursor, 0, SAFE); integer(value.limit, 1, 16);
    } else fail();
  }
  function payload(operation, value) {
    if (operation === EDIT) {
      exact(value, ['command', 'action']);
      if (value.command !== 'object.project.guide.edit') fail(); action(value.action);
    } else if (operation === READ) {
      exact(value, ['atRevision', 'documentSnapshotId', 'contextId', 'selector']);
      integer(value.atRevision, 0, SAFE); identifier(value.documentSnapshotId);
      if (typeof value.contextId !== 'string' || !value.contextId) fail(); selector(value.selector);
    } else fail();
  }
  function flag(value) { if (typeof value !== 'boolean') fail(); }
  function presence(present, count, guidesPresent) {
    flag(present); integer(count, 0, SAFE);
    if (!present && count !== 0 || !guidesPresent && present) fail();
  }
  function settings(value) {
    exact(value, ['guidesPresent', 'hPresent', 'vPresent', 'hCount', 'vCount']); flag(value.guidesPresent);
    presence(value.hPresent, value.hCount, value.guidesPresent);
    presence(value.vPresent, value.vCount, value.guidesPresent);
  }
  function page(value) {
    exact(value, ['kind', 'axis', 'guidesPresent', 'axisPresent', 'guideCount', 'cursor', 'items', 'nextCursor']);
    if (value.kind !== 'guideAxisPage') fail(); axis(value.axis); flag(value.guidesPresent);
    presence(value.axisPresent, value.guideCount, value.guidesPresent); integer(value.cursor, 0, value.guideCount);
    var items = value.items;
    if (!Array.isArray(items) || Object.getOwnPropertySymbols(items).length || items.length > 16 ||
        Object.getOwnPropertyNames(items).length !== items.length + 1) fail();
    for (var i = 0; i < items.length; i++) {
      var descriptor = Object.getOwnPropertyDescriptor(items, String(i));
      if (!descriptor || !has(descriptor, 'value')) fail();
      var item = descriptor.value; exact(item, ['ordinal', 'position']); finite(item.position);
      if (item.ordinal !== value.cursor + i + 1) fail();
    }
    var end = value.cursor + items.length;
    if (!Number.isSafeInteger(end) || end > value.guideCount) fail();
    if (end < value.guideCount) { if (!items.length || value.nextCursor !== end) fail(); }
    else if (value.nextCursor !== null) fail();
  }
  function result(operation, value) {
    if (operation === EDIT) {
      exact(value, ['applied', 'historyEntriesAdded']); flag(value.applied);
      if (value.historyEntriesAdded !== Number(value.applied)) fail();
    } else if (operation === READ) {
      exact(value, ['instanceId', 'documentId', 'atRevision', 'documentSnapshotId', 'contextId', 'result']);
      identifier(value.instanceId); identifier(value.documentId); identifier(value.documentSnapshotId); integer(value.atRevision, 0, SAFE);
      if (value.contextId !== 'scene-root') fail(); data(value.result);
      if (has(value.result, 'kind')) page(value.result); else settings(value.result);
    } else fail();
  }
  function correlate(request, response) {
    if (response.instanceId !== request.instanceId) fail();
    var replaced = response.ok === false && response.error && response.error.code === 'wrong_document';
    if (response.documentId !== request.documentId && !replaced) fail();
    if (!response.ok) return;
    result(request.operation, response.result);
    if (request.operation === EDIT) {
      if (response.contentRevision !== request.expectedRevision + Number(response.result.applied)) fail(); return;
    }
    var p = request.payload, r = response.result, value = r.result, wanted = p.selector;
    if (response.contentRevision !== p.atRevision || r.instanceId !== request.instanceId || r.documentId !== request.documentId ||
        r.atRevision !== p.atRevision || r.documentSnapshotId !== p.documentSnapshotId || r.contextId !== p.contextId) fail();
    if (wanted.kind === 'settings') { if (has(value, 'kind')) fail(); }
    else if (value.kind !== 'guideAxisPage' || value.axis !== wanted.axis || value.cursor !== wanted.cursor || value.items.length > wanted.limit) fail();
  }
  function admitted(operation) { return operation === READ || operation === EDIT; }
  function paths(operation) { return admitted(operation) ? { has: function () { return false; } } : null; }
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
