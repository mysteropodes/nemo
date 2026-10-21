/* Original custom brush catalog protocol; no drawing provider or writable mirror. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeCustomBrushContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var READ = 'query.document.object.project.customBrushPreset.read';
  var EDIT = 'command.document.object.project.customBrushPreset.edit';
  var numbers = [
    'bristleCount', 'dashGap', 'edgeNoise', 'fixedAngle', 'grain', 'nibSize', 'noise', 'opacity', 'opacityJitter', 'polySides',
    'pressureEnd', 'pressureMid', 'pressureStart', 'rotationJitter', 'roughness', 'roundness', 'scatter', 'scribbleAngleSpread',
    'scribbleCount', 'scribbleLen', 'scribbleLenJitter', 'scribbleSpread', 'scribbleWidth', 'sharpness', 'sizeJitter',
    'spaceJitter', 'spacing', 'tipCorner', 'valueJitter', 'savedBrushSize'
  ];
  var strings = ['rotationMode', 'scatterDistribution', 'tipShape', 'label'];
  function has(value, key) { return Object.prototype.hasOwnProperty.call(value, key); }
  function fail() { throw new TypeError('Invalid native custom brush catalog contract'); }
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
    if (required.some(function (key) { return !has(value, key); }) || Object.keys(value).some(function (key) {
      return !required.concat(optional || []).includes(key);
    })) fail();
  }
  function integer(value, min) { if (!Number.isSafeInteger(value) || value < min) fail(); }
  function finite(value) { if (typeof value !== 'number' || !Number.isFinite(value)) fail(); }
  function text(value) {
    if (typeof value !== 'string') fail();
    for (var i = 0; i < value.length; i++) {
      var n = value.charCodeAt(i);
      if (n >= 0xD800 && n <= 0xDBFF) {
        var low = value.charCodeAt(++i);
        if (!(low >= 0xDC00 && low <= 0xDFFF)) fail();
      } else if (n >= 0xDC00 && n <= 0xDFFF) fail();
    }
  }
  function array(value) {
    if (!Array.isArray(value) || Object.getOwnPropertySymbols(value).length || Object.keys(value).length !== value.length ||
        Object.getOwnPropertyNames(value).length !== value.length + 1) fail();
    for (var i = 0; i < value.length; i++) {
      var descriptor = Object.getOwnPropertyDescriptor(value, String(i));
      if (!descriptor || !has(descriptor, 'value')) fail();
    }
  }
  function pair(value) { array(value); if (value.length !== 2) fail(); value.forEach(finite); }
  function preset(value) {
    exact(value, [], numbers.concat(strings, ['markerTip', 'customStamp']));
    numbers.forEach(function (key) { if (has(value, key)) finite(value[key]); });
    strings.forEach(function (key) { if (has(value, key)) text(value[key]); });
    if (has(value, 'markerTip') && typeof value.markerTip !== 'boolean') fail();
    if (has(value, 'customStamp')) {
      var stamp = value.customStamp;
      exact(stamp, ['segments'], ['closed', 'pointCount']);
      array(stamp.segments);
      if (has(stamp, 'closed') && typeof stamp.closed !== 'boolean') fail();
      if (has(stamp, 'pointCount')) finite(stamp.pointCount);
      stamp.segments.forEach(function (segment) {
        exact(segment, ['point'], ['handleIn', 'handleOut']);
        pair(segment.point);
        ['handleIn', 'handleOut'].forEach(function (key) { if (has(segment, key)) pair(segment[key]); });
      });
    }
    return value;
  }
  function action(value) {
    data(value);
    if (value.kind === 'upsert') {
      exact(value, ['kind', 'id', 'preset']);
      text(value.id);
      preset(value.preset);
    } else if (value.kind === 'delete') {
      exact(value, ['kind', 'id']);
      text(value.id);
    } else fail();
    return value;
  }
  function selector(value) {
    data(value);
    if (value.kind === 'settings') exact(value, ['kind']);
    else if (value.kind === 'record') { exact(value, ['kind', 'id']); text(value.id); }
    else if (value.kind === 'catalogPage') {
      exact(value, ['kind', 'cursor', 'limit']);
      integer(value.cursor, 0);
      integer(value.limit, 1);
      if (value.limit > 16) fail();
    } else fail();
  }
  function identifier(value) {
    if (typeof value !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(value)) fail();
  }
  function payload(operation, value) {
    if (operation === EDIT) {
      exact(value, ['command', 'action']);
      if (value.command !== 'object.project.customBrushPreset.edit') fail();
      action(value.action);
    } else if (operation === READ) {
      exact(value, ['atRevision', 'documentSnapshotId', 'contextId', 'selector']);
      integer(value.atRevision, 0);
      identifier(value.documentSnapshotId);
      if (value.contextId !== 'scene-root') fail();
      selector(value.selector);
    } else fail();
  }
  function counts(value) {
    if (typeof value.customBrushPresetsPresent !== 'boolean') fail();
    integer(value.presetCount, 0);
    if (!value.customBrushPresetsPresent && value.presetCount !== 0) fail();
  }
  function result(operation, value) {
    if (operation === EDIT) {
      exact(value, ['applied', 'historyEntriesAdded']);
      if (typeof value.applied !== 'boolean' || value.historyEntriesAdded !== Number(value.applied)) fail();
      return;
    }
    if (operation !== READ) fail();
    exact(value, ['instanceId', 'documentId', 'atRevision', 'documentSnapshotId', 'contextId', 'result']);
    identifier(value.instanceId);
    identifier(value.documentId);
    integer(value.atRevision, 0);
    identifier(value.documentSnapshotId);
    if (value.contextId !== 'scene-root') fail();
    var r = value.result;
    data(r);
    if (r.kind === 'customBrushPresetRecord') {
      exact(r, ['kind', 'id', 'preset']);
      text(r.id);
      preset(r.preset);
      return;
    }
    if (!has(r, 'kind')) { exact(r, ['customBrushPresetsPresent', 'presetCount']); counts(r); return; }
    exact(r, ['kind', 'customBrushPresetsPresent', 'presetCount', 'cursor', 'items', 'nextCursor']);
    if (r.kind !== 'customBrushPresetPage') fail();
    counts(r);
    integer(r.cursor, 0);
    array(r.items);
    if (r.items.length > 16 || r.cursor > r.presetCount) fail();
    var seen = new Set();
    r.items.forEach(function (item, i) {
      exact(item, ['ordinal', 'id'], ['label']);
      if (item.ordinal !== r.cursor + i + 1) fail();
      text(item.id);
      if (has(item, 'label')) text(item.label);
      if (seen.has(item.id)) fail();
      seen.add(item.id);
    });
    var end = r.cursor + r.items.length;
    if (!Number.isSafeInteger(end) || end > r.presetCount) fail();
    if (end < r.presetCount) {
      if (!r.items.length || r.nextCursor !== end) fail();
    } else if (r.nextCursor !== null) fail();
  }
  function correlate(request, response) {
    if (typeof response.ok !== 'boolean') fail();
    integer(response.contentRevision, 0);
    if (response.requestId !== request.requestId || response.instanceId !== request.instanceId) fail();
    if (response.documentId !== request.documentId && !( !response.ok && response.error && response.error.code === 'wrong_document')) fail();
    if (!response.ok) return;
    result(request.operation, response.result);
    if (request.operation === EDIT) {
      if (response.contentRevision !== request.expectedRevision + Number(response.result.applied)) fail();
      return;
    }
    var p = request.payload, r = response.result, v = r.result, s = p.selector;
    if (response.contentRevision !== p.atRevision || r.instanceId !== request.instanceId || r.documentId !== request.documentId ||
        r.atRevision !== p.atRevision || r.documentSnapshotId !== p.documentSnapshotId || r.contextId !== p.contextId) fail();
    if (s.kind === 'settings') { if (has(v, 'kind')) fail(); }
    else if (s.kind === 'record') { if (v.kind !== 'customBrushPresetRecord' || v.id !== s.id) fail(); }
    else if (v.kind !== 'customBrushPresetPage' || v.cursor !== s.cursor || v.items.length > s.limit) fail();
  }
  function admitted(operation) { return operation === READ || operation === EDIT; }
  function paths(operation) {
    if (!admitted(operation)) return null;
    return { has: function (path) {
      return /^(?:payload\.(?:action|selector)\.id|result\.result\.(?:id|items\.\d+\.(?:id|label)))$/.test(path) ||
        /^(?:payload\.action|result\.result)\.preset\.(?:rotationMode|scatterDistribution|tipShape|label)$/.test(path);
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
      correlate: function (q, r) { return pick(q.operation).correlate(q, r); },
      paths: function (operation) { return pick(operation).paths(operation); }
    });
  }
  var api = Object.freeze({
    READ: READ, EDIT: EDIT, preset: preset, action: action, admitted: admitted, payload: payload, result: result,
    correlate: correlate, paths: paths, requiresRevision: function (operation) { return operation === EDIT; },
    requestByteLimit: function () { return 4096; }, responseByteLimit: function () { return 4096; }, compose: compose, join: compose
  });
  return api;
}));
