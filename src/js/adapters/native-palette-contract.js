/* Field-specific palette protocol. Native owns persisted values and history. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativePaletteContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var READ = 'query.document.object.project.palette.read';
  var EDIT = 'command.document.object.project.palette.edit';
  var SAFE = Number.MAX_SAFE_INTEGER;
  var has = function (value, key) { return Object.prototype.hasOwnProperty.call(value, key); };
  function fail() { throw new TypeError('Invalid native palette contract'); }
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
    var fields = {
      create: ['id', 'name'], rename: ['ordinal', 'name'], duplicate: ['ordinal', 'id', 'name'],
      remove: ['ordinal'], select: ['ordinal'], appendColor: ['ordinal', 'color'],
      removeColor: ['ordinal', 'index'], moveColor: ['ordinal', 'from', 'to']
    };
    if (typeof value.kind !== 'string' || !has(fields, value.kind)) fail();
    exact(value, ['kind'].concat(fields[value.kind]), value.kind === 'create' ? ['color'] : []);
    ['id', 'name', 'color'].forEach(function (key) { if (has(value, key)) text(value[key]); });
    ['ordinal', 'index', 'from', 'to'].forEach(function (key) {
      if (has(value, key)) integer(value[key], 1, SAFE);
    });
    return Object.freeze(Object.assign({}, value));
  }
  function selector(value) {
    data(value);
    var fields = { settings: [], palettePage: ['cursor', 'limit'], colorPage: ['ordinal', 'cursor', 'limit'] };
    if (typeof value.kind !== 'string' || !has(fields, value.kind)) fail();
    exact(value, ['kind'].concat(fields[value.kind]));
    if (value.kind !== 'settings') { integer(value.cursor, 0, SAFE); integer(value.limit, 1, 16); }
    if (value.kind === 'colorPage') integer(value.ordinal, 1, SAFE);
  }
  function payload(operation, value) {
    if (operation === EDIT) {
      exact(value, ['command', 'action']);
      if (value.command !== 'object.project.palette.edit') fail();
      action(value.action);
    } else if (operation === READ) {
      exact(value, ['atRevision', 'documentSnapshotId', 'contextId', 'selector']);
      integer(value.atRevision, 0, SAFE); identifier(value.documentSnapshotId);
      if (typeof value.contextId !== 'string' || !value.contextId) fail();
      selector(value.selector);
    } else fail();
  }
  function count(value) {
    integer(value.paletteCount, 0, SAFE);
    if (typeof value.palettesPresent !== 'boolean' || !value.palettesPresent && value.paletteCount !== 0) fail();
  }
  function settings(value) {
    exact(value, ['palettesPresent', 'paletteCount', 'effectiveActivePaletteIdx'], ['activePaletteIdx']);
    count(value); finite(value.effectiveActivePaletteIdx);
    if (has(value, 'activePaletteIdx')) finite(value.activePaletteIdx);
    var effectiveCount = value.palettesPresent ? value.paletteCount : 1;
    var expected = has(value, 'activePaletteIdx') && value.activePaletteIdx < effectiveCount ? value.activePaletteIdx : 0;
    if (value.effectiveActivePaletteIdx !== expected) fail();
  }
  function items(value) {
    if (!Array.isArray(value) || Object.getOwnPropertySymbols(value).length ||
        Object.getOwnPropertyNames(value).length !== value.length + 1 || value.length > 16) fail();
    for (var i = 0; i < value.length; i++) {
      var descriptor = Object.getOwnPropertyDescriptor(value, String(i));
      if (!descriptor || !has(descriptor, 'value')) fail();
    }
  }
  function page(value) {
    data(value);
    var colors = value.kind === 'paletteColorPage';
    if (!colors && value.kind !== 'palettePage') fail();
    exact(value, colors ? ['kind', 'paletteOrdinal', 'colorCount', 'cursor', 'items', 'nextCursor'] :
      ['kind', 'palettesPresent', 'paletteCount', 'cursor', 'items', 'nextCursor']);
    if (colors) { integer(value.paletteOrdinal, 1, SAFE); integer(value.colorCount, 0, SAFE); }
    else count(value);
    var total = colors ? value.colorCount : value.paletteCount;
    integer(value.cursor, 0, total); items(value.items);
    var end = value.cursor + value.items.length;
    if (!Number.isSafeInteger(end) || end > total) fail();
    for (var i = 0; i < value.items.length; i++) {
      var item = value.items[i];
      if (colors) {
        exact(item, ['index', 'color']); text(item.color);
        if (item.index !== value.cursor + i + 1) fail();
      } else {
        exact(item, ['ordinal', 'id', 'name', 'colorCount']); text(item.id); text(item.name);
        integer(item.colorCount, 0, SAFE);
        if (item.ordinal !== value.cursor + i + 1) fail();
      }
    }
    if (end < total) { if (!value.items.length || value.nextCursor !== end) fail(); }
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
    else {
      if (value.kind !== (wanted.kind === 'colorPage' ? 'paletteColorPage' : 'palettePage') ||
          value.cursor !== wanted.cursor || value.items.length > wanted.limit ||
          wanted.kind === 'colorPage' && value.paletteOrdinal !== wanted.ordinal) fail();
    }
  }
  function admitted(operation) { return operation === READ || operation === EDIT; }
  function paths(operation) {
    if (!admitted(operation)) return null;
    return { has: function (path) {
      return /^(?:payload\.action|result\.result\.items\.\d+)\.(?:id|name|color)$/.test(path);
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
