/* Closed original team-comment protocol. Native owns order, metadata and history. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeCommentsContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var READ = 'query.document.object.project.comments.read', EDIT = 'command.document.object.project.comments.edit';
  var fields = ['id', 'x', 'y', 'frame', 'authorId', 'authorName', 'authorColor', 'text', 'createdAt', 'resolved'];
  var mandatory = ['id', 'x', 'y', 'frame', 'text', 'createdAt', 'resolved'];
  var has = function (value, key) { return Object.prototype.hasOwnProperty.call(value, key); };
  function fail() { throw new TypeError('Invalid native comments contract'); }
  function data(value) {
    if (!value || typeof value !== 'object' || Array.isArray(value) || ![Object.prototype, null].includes(Object.getPrototypeOf(value)) ||
        Object.getOwnPropertySymbols(value).length) fail();
    var keys = Object.keys(value);
    if (Object.getOwnPropertyNames(value).length !== keys.length || keys.some(function (key) {
      return !has(Object.getOwnPropertyDescriptor(value, key), 'value');
    })) fail();
  }
  function exact(value, required, optional) {
    data(value);
    if (required.some(function (key) { return !has(value, key); }) || Object.keys(value).some(function (key) {
      return !required.concat(optional || []).includes(key);
    })) fail();
  }
  function integer(value, min, max) { if (!Number.isSafeInteger(value) || value < min || value > max) fail(); }
  function text(value) {
    if (typeof value !== 'string') fail();
    for (var i = 0; i < value.length; i++) {
      var c = value.charCodeAt(i);
      if (c >= 0xD800 && c <= 0xDBFF) { var next = value.charCodeAt(++i); if (!(next >= 0xDC00 && next <= 0xDFFF)) fail(); }
      else if (c >= 0xDC00 && c <= 0xDFFF) fail();
    }
  }
  function identifier(value) { if (typeof value !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(value)) fail(); }
  function comment(value, append) {
    exact(value, append ? mandatory : [], fields);
    Object.keys(value).forEach(function (key) {
      if (['x', 'y', 'frame', 'createdAt'].includes(key)) { if (typeof value[key] !== 'number' || !Number.isFinite(value[key])) fail(); }
      else if (key === 'resolved') { if (typeof value[key] !== 'boolean') fail(); }
      else text(value[key]);
    });
    if (append && !value.text.trim()) fail();
    var copy = {}; fields.forEach(function (key) { if (has(value, key)) copy[key] = value[key]; });
    return Object.freeze(copy);
  }
  function action(value) {
    data(value);
    if (value.kind === 'append') { exact(value, ['kind', 'comment']); return Object.freeze({ kind: 'append', comment: comment(value.comment, true) }); }
    if (value.kind === 'update') {
      exact(value, ['kind', 'ordinal', 'text', 'resolved']); text(value.text); if (typeof value.resolved !== 'boolean') fail();
    } else if (value.kind === 'delete') exact(value, ['kind', 'ordinal']);
    else fail();
    integer(value.ordinal, 1, Number.MAX_SAFE_INTEGER); return Object.freeze(Object.assign({}, value));
  }
  function selector(value) {
    data(value);
    if (value.kind === 'settings') exact(value, ['kind']);
    else if (value.kind === 'record') { exact(value, ['kind', 'ordinal']); integer(value.ordinal, 1, Number.MAX_SAFE_INTEGER); }
    else if (value.kind === 'commentsPage') {
      exact(value, ['kind', 'cursor', 'limit']); integer(value.cursor, 0, Number.MAX_SAFE_INTEGER); integer(value.limit, 1, 16);
    } else fail();
  }
  function payload(operation, value) {
    if (operation === EDIT) { exact(value, ['command', 'action']); if (value.command !== 'object.project.comments.edit') fail(); action(value.action); }
    else if (operation === READ) {
      exact(value, ['atRevision', 'documentSnapshotId', 'contextId', 'selector']); integer(value.atRevision, 0, Number.MAX_SAFE_INTEGER);
      identifier(value.documentSnapshotId); if (value.contextId !== 'scene-root') fail(); selector(value.selector);
    } else fail();
  }
  function settings(value) {
    exact(value, ['commentsPresent', 'commentCount']);
    if (typeof value.commentsPresent !== 'boolean') fail(); integer(value.commentCount, 0, Number.MAX_SAFE_INTEGER);
    if (!value.commentsPresent && value.commentCount !== 0) fail();
  }
  function page(value) {
    exact(value, ['kind', 'commentsPresent', 'commentCount', 'cursor', 'items', 'nextCursor']);
    if (value.kind !== 'projectCommentsPage') fail(); settings({ commentsPresent: value.commentsPresent, commentCount: value.commentCount });
    integer(value.cursor, 0, value.commentCount);
    var items = value.items;
    if (!Array.isArray(items) || Object.getOwnPropertySymbols(items).length || Object.getOwnPropertyNames(items).length !== items.length + 1 || items.length > 16) fail();
    for (var i = 0; i < items.length; i++) {
      var d = Object.getOwnPropertyDescriptor(items, String(i)); if (!d || !has(d, 'value')) fail();
      exact(d.value, ['ordinal', 'comment']); if (d.value.ordinal !== value.cursor + i + 1) fail(); comment(d.value.comment, false);
    }
    var end = value.cursor + items.length;
    if (!Number.isSafeInteger(end) || end > value.commentCount) fail();
    if (end < value.commentCount) { if (!items.length || value.nextCursor !== end) fail(); }
    else if (value.nextCursor !== null) fail();
  }
  function result(operation, value) {
    if (operation === EDIT) {
      exact(value, ['applied', 'historyEntriesAdded']); if (typeof value.applied !== 'boolean' || value.historyEntriesAdded !== Number(value.applied)) fail(); return;
    }
    if (operation !== READ) fail();
    exact(value, ['instanceId', 'documentId', 'atRevision', 'documentSnapshotId', 'contextId', 'result']);
    identifier(value.instanceId); identifier(value.documentId); identifier(value.documentSnapshotId); integer(value.atRevision, 0, Number.MAX_SAFE_INTEGER);
    if (value.contextId !== 'scene-root') fail(); data(value.result);
    if (value.result.kind === 'projectCommentRecord') {
      exact(value.result, ['kind', 'ordinal', 'comment']); integer(value.result.ordinal, 1, Number.MAX_SAFE_INTEGER); comment(value.result.comment, false);
    } else if (has(value.result, 'kind')) page(value.result); else settings(value.result);
  }
  function correlate(request, response) {
    if (!response || typeof response.ok !== 'boolean') fail(); integer(response.contentRevision, 0, Number.MAX_SAFE_INTEGER);
    if (response.requestId !== request.requestId || response.instanceId !== request.instanceId) fail();
    if (response.documentId !== request.documentId && !(!response.ok && response.error && response.error.code === 'wrong_document')) fail();
    if (!response.ok) return; result(request.operation, response.result);
    if (request.operation === EDIT) { if (response.contentRevision !== request.expectedRevision + Number(response.result.applied)) fail(); return; }
    var p = request.payload, r = response.result, v = r.result, wanted = p.selector;
    if (response.contentRevision !== p.atRevision || r.instanceId !== request.instanceId || r.documentId !== request.documentId ||
        r.atRevision !== p.atRevision || r.documentSnapshotId !== p.documentSnapshotId || r.contextId !== p.contextId) fail();
    if (wanted.kind === 'settings') { if (has(v, 'kind')) fail(); }
    else if (wanted.kind === 'record') { if (v.kind !== 'projectCommentRecord' || v.ordinal !== wanted.ordinal) fail(); }
    else if (v.kind !== 'projectCommentsPage' || v.cursor !== wanted.cursor || v.items.length > wanted.limit) fail();
  }
  function admitted(operation) { return operation === READ || operation === EDIT; }
  function paths(operation) {
    if (!admitted(operation)) return null;
    return { has: function (path) { return /^(?:payload\.action\.(?:comment\.)?|result\.result\.(?:comment\.|items\.\d+\.comment\.))(?:id|authorId|authorName|authorColor|text)$/.test(path); } };
  }
  function compose(base) {
    function pick(operation) { if (admitted(operation)) return api; if (!base) fail(); return base; }
    return Object.freeze({ admitted: function (op) { return admitted(op) || !!base && base.admitted(op); },
      requiresRevision: function (op) { return admitted(op) ? op === EDIT : !!base && base.requiresRevision(op); },
      requestByteLimit: function (op) { return admitted(op) ? 4096 : base.requestByteLimit(op); },
      responseByteLimit: function (op) { return admitted(op) ? 4096 : base.responseByteLimit(op); },
      payload: function (op, value, envelope) { return pick(op).payload(op, value, envelope); },
      result: function (op, value) { return pick(op).result(op, value); },
      correlate: function (request, response) { return pick(request.operation).correlate(request, response); },
      paths: function (op) { return pick(op).paths(op); } });
  }
  var api = Object.freeze({ READ: READ, EDIT: EDIT, comment: comment, action: action, admitted: admitted, payload: payload, result: result,
    correlate: correlate, paths: paths, compose: compose, join: compose, requiresRevision: function (op) { return op === EDIT; },
    requestByteLimit: function () { return 4096; }, responseByteLimit: function () { return 4096; } });
  return api;
}));
