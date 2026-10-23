/* Closed workspace metadata protocol. Native is the only persistent owner. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeWorkspaceContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var READ = 'query.document.object.project.workspace.read', EDIT = 'command.document.object.project.workspace.edit';
  var fields = ['layerPanelWidth', 'toolsPanelWidth', 'propsPanelWidth', 'propsPanelCollapsed', 'timelineHeight', 'panelOrder', 'motionColumns'];
  var has = function (value, key) { return Object.prototype.hasOwnProperty.call(value, key); };
  function fail() { throw new TypeError('Invalid native workspace contract'); }
  function data(value) {
    if (!value || typeof value !== 'object' || Array.isArray(value) ||
        ![Object.prototype, null].includes(Object.getPrototypeOf(value)) || Object.getOwnPropertySymbols(value).length) fail();
    var keys = Object.keys(value);
    if (Object.getOwnPropertyNames(value).length !== keys.length || keys.some(function (key) {
      return !has(Object.getOwnPropertyDescriptor(value, key), 'value');
    })) fail();
  }
  function exact(value, required, optional) {
    data(value);
    if (required.some(function (key) { return !has(value, key); }) ||
        Object.keys(value).some(function (key) { return !required.concat(optional || []).includes(key); })) fail();
  }
  function integer(value) { if (!Number.isSafeInteger(value) || value < 0) fail(); }
  function identifier(value) { if (typeof value !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(value)) fail(); }
  function text(value) {
    if (typeof value !== 'string') fail();
    for (var i = 0; i < value.length; i++) {
      var code = value.charCodeAt(i);
      if (code >= 0xD800 && code <= 0xDBFF) {
        var low = value.charCodeAt(++i); if (!(low >= 0xDC00 && low <= 0xDFFF)) fail();
      } else if (code >= 0xDC00 && code <= 0xDFFF) fail();
    }
  }
  function workspace(value, capture) {
    exact(value, capture ? fields.slice(0, 6) : [], capture ? ['motionColumns'] : fields);
    Object.keys(value).forEach(function (key) {
      var item = value[key];
      if (key === 'propsPanelCollapsed') { if (typeof item !== 'boolean') fail(); }
      else if (key === 'panelOrder') {
        if (item !== null) {
          if (!Array.isArray(item) || Object.getOwnPropertySymbols(item).length || Object.keys(item).length !== item.length ||
              Object.getOwnPropertyNames(item).length !== item.length + 1) fail();
          for (var i = 0; i < item.length; i++) {
            var descriptor = Object.getOwnPropertyDescriptor(item, String(i));
            if (!descriptor || !has(descriptor, 'value')) fail(); text(descriptor.value);
          }
        }
      } else if (item !== null) text(item);
    });
    var copy = {};
    fields.forEach(function (key) { if (has(value, key)) copy[key] = Array.isArray(value[key]) ? Object.freeze(value[key].slice()) : value[key]; });
    return Object.freeze(copy);
  }
  function action(value) {
    exact(value, ['kind', 'workspace']); if (value.kind !== 'capture') fail();
    return Object.freeze({ kind: 'capture', workspace: workspace(value.workspace, true) });
  }
  function payload(operation, value) {
    if (operation === EDIT) { exact(value, ['command', 'action']); if (value.command !== 'object.project.workspace.edit') fail(); action(value.action); }
    else if (operation === READ) {
      exact(value, ['atRevision', 'documentSnapshotId', 'contextId', 'selector']); integer(value.atRevision); identifier(value.documentSnapshotId);
      if (value.contextId !== 'scene-root') fail(); exact(value.selector, ['kind']); if (value.selector.kind !== 'record') fail();
    } else fail();
  }
  function result(operation, value) {
    if (operation === EDIT) {
      exact(value, ['applied', 'historyEntriesAdded']);
      if (typeof value.applied !== 'boolean' || value.historyEntriesAdded !== (value.applied ? 1 : 0)) fail(); return;
    }
    if (operation !== READ) fail();
    exact(value, ['instanceId', 'documentId', 'atRevision', 'documentSnapshotId', 'contextId', 'result']);
    identifier(value.instanceId); identifier(value.documentId); integer(value.atRevision); identifier(value.documentSnapshotId);
    if (value.contextId !== 'scene-root') fail();
    var record = value.result; exact(record, ['workspacePresent'], ['workspace']);
    if (typeof record.workspacePresent !== 'boolean' || has(record, 'workspace') !== record.workspacePresent) fail();
    if (record.workspacePresent) workspace(record.workspace, false);
  }
  function correlate(request, response) {
    if (typeof response.ok !== 'boolean') fail(); integer(response.contentRevision);
    if (response.requestId !== request.requestId || response.instanceId !== request.instanceId) fail();
    if (response.documentId !== request.documentId && !(!response.ok && response.error && response.error.code === 'wrong_document')) fail();
    if (!response.ok) return; result(request.operation, response.result);
    if (request.operation === EDIT) {
      if (response.contentRevision !== request.expectedRevision + (response.result.applied ? 1 : 0)) fail(); return;
    }
    var p = request.payload, r = response.result;
    if (response.contentRevision !== p.atRevision || r.instanceId !== request.instanceId || r.documentId !== request.documentId ||
        r.atRevision !== p.atRevision || r.documentSnapshotId !== p.documentSnapshotId || r.contextId !== p.contextId) fail();
  }
  function equal(a, b) { return JSON.stringify(workspace(a, false)) === JSON.stringify(workspace(b, false)); }
  function admitted(operation) { return operation === READ || operation === EDIT; }
  function paths(operation) {
    if (!admitted(operation)) return null;
    return { has: function (path) {
      return fields.filter(function (key) { return key !== 'propsPanelCollapsed' && key !== 'panelOrder'; })
        .some(function (key) { return path === 'result.result.workspace.' + key || path === 'payload.action.workspace.' + key; }) || /^(?:result\.result|payload\.action)\.workspace\.panelOrder\.\d+$/.test(path);
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
  var api = Object.freeze({ READ: READ, EDIT: EDIT, workspace: workspace, action: action, equal: equal, admitted: admitted,
    requiresRevision: function (operation) { return operation === EDIT; }, payload: payload, result: result, correlate: correlate,
    paths: paths, compose: compose, join: compose, requestByteLimit: function () { return 4096; }, responseByteLimit: function () { return 4096; } });
  return api;
}));
