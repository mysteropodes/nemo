/* Field-specific Draw symmetry protocol. Native owns persisted values and history. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeSymmetryContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var READ = 'query.document.object.project.symmetry.read';
  var EDIT = 'command.document.object.project.symmetry.edit';
  var SAFE = Number.MAX_SAFE_INTEGER;
  var MODES = ['y', 'x', 'free', 'radial'];
  var RAW = ['symmetryEnabled', 'symmetryMode', 'symmetryAxis', 'symmetryRadialCenter', 'symmetryRadialSectors', 'symmetryExtend'];
  var has = function (value, key) { return Object.prototype.hasOwnProperty.call(value, key); };
  function fail() { throw new TypeError('Invalid native symmetry contract'); }
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
  function integer(value, min, max) { if (!Number.isSafeInteger(value) || value < min || value > max) fail(); }
  function finite(value) { if (typeof value !== 'number' || !Number.isFinite(value)) fail(); }
  function bool(value) { if (typeof value !== 'boolean') fail(); }
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
  function axis(value) { exact(value, ['x1', 'y1', 'x2', 'y2']); ['x1', 'y1', 'x2', 'y2'].forEach(function (k) { finite(value[k]); }); }
  function point(value) { exact(value, ['x', 'y']); finite(value.x); finite(value.y); }
  function action(value) {
    data(value);
    var kind = value.kind;
    if (kind === 'setEnabled') { exact(value, ['kind', 'enabled']); bool(value.enabled); }
    else if (kind === 'setMode') { exact(value, ['kind', 'mode']); if (!MODES.includes(value.mode)) fail(); }
    else if (kind === 'resetGuide') exact(value, ['kind']);
    else if (kind === 'setAxis') { exact(value, ['kind', 'x1', 'y1', 'x2', 'y2']); ['x1', 'y1', 'x2', 'y2'].forEach(function (k) { finite(value[k]); }); }
    else if (kind === 'setRadialCenter') { exact(value, ['kind', 'x', 'y']); finite(value.x); finite(value.y); }
    else if (kind === 'setSectors') { exact(value, ['kind', 'sectors']); finite(value.sectors); }
    else if (kind === 'setExtend') { exact(value, ['kind', 'extend']); bool(value.extend); }
    else fail();
    return Object.freeze(Object.assign({}, value));
  }
  function payload(operation, value) {
    if (operation === EDIT) {
      exact(value, ['command', 'action']);
      if (value.command !== 'object.project.symmetry.edit') fail();
      action(value.action);
    } else if (operation === READ) {
      exact(value, ['atRevision', 'documentSnapshotId', 'contextId', 'selector']);
      integer(value.atRevision, 0, SAFE); identifier(value.documentSnapshotId);
      if (typeof value.contextId !== 'string' || !value.contextId) fail();
      exact(value.selector, ['kind']); if (value.selector.kind !== 'settings') fail();
    } else fail();
  }
  function settings(value) {
    exact(value, ['effective'], RAW);
    var e = value.effective;
    exact(e, ['enabled', 'mode', 'axis', 'radialCenter', 'radialSectors', 'extend']);
    bool(e.enabled); text(e.mode); if (!e.mode) fail(); axis(e.axis); point(e.radialCenter); finite(e.radialSectors); bool(e.extend);
    if (has(value, 'symmetryEnabled')) bool(value.symmetryEnabled);
    if (has(value, 'symmetryMode')) text(value.symmetryMode);
    if (has(value, 'symmetryAxis') && value.symmetryAxis !== null) axis(value.symmetryAxis);
    if (has(value, 'symmetryRadialCenter') && value.symmetryRadialCenter !== null) point(value.symmetryRadialCenter);
    if (has(value, 'symmetryRadialSectors')) finite(value.symmetryRadialSectors);
    if (has(value, 'symmetryExtend')) bool(value.symmetryExtend);
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
    settings(value.result);
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
    var p = request.payload, r = response.result;
    if (response.contentRevision !== p.atRevision || r.instanceId !== request.instanceId || r.documentId !== request.documentId ||
        r.atRevision !== p.atRevision || r.documentSnapshotId !== p.documentSnapshotId || r.contextId !== p.contextId) fail();
  }
  function admitted(operation) { return operation === READ || operation === EDIT; }
  function paths(operation) {
    if (!admitted(operation)) return null;
    return { has: function (path) { return path === 'result.result.symmetryMode' || path === 'result.result.effective.mode'; } };
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
  var api = Object.freeze({ READ: READ, EDIT: EDIT, MODES: MODES, admitted: admitted, action: action,
    requiresRevision: function (operation) { return operation === EDIT; }, payload: payload, result: result,
    correlate: correlate, paths: paths, compose: compose, join: compose,
    requestByteLimit: function () { return 4096; }, responseByteLimit: function () { return 4096; } });
  return api;
}));
