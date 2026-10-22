/* L10-J closed declaration of the native import batch operation (`command.document.object.import.batch`, payload command
 * `object.import.batch`; the name is the provisional one of the accepted L10-H transaction). Pure shape and receipt validation with
 * the interface of the font contract (admitted, requiresRevision, payload, result, correlate, paths); `compose(base)` joins another
 * contract of that interface. The native owner (ObjectRecord codec, id table, history) stays the only authority: records are
 * pre-checked structurally here and re-validated there. No owner, no writer, no host. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeImportContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  const OP = 'command.document.object.import.batch';
  const COMMAND = 'object.import.batch';
  // The dispatcher bounds the whole request and the whole response at 4096 bytes; the receipt must fit inside the response (L10-H).
  const MAX_REQUEST_BYTES = 4096;
  const MAX_RECEIPT_BYTES = 3072;
  const MAX_ID_BYTES = 255;
  const MIN_ANCHORS = 2, MAX_ANCHORS = 256;
  const MAX_FRAME = 4294967295;
  const SKIP_KINDS = ['layer', 'element', 'asset'];
  const PROVIDERS = ['raster-asset', 'text', 'font'];
  const CAPS = ['butt', 'round', 'square'];
  const JOINS = ['miter', 'round', 'bevel'];
  const has = (value, key) => Object.prototype.hasOwnProperty.call(value, key);
  const fail = (message) => { throw new TypeError(message); };
  const bytes = (text) => (typeof TextEncoder === 'function' ? new TextEncoder().encode(text).length : unescape(encodeURIComponent(text)).length);

  function plain(value) {
    return value !== null && typeof value === 'object' && !Array.isArray(value) &&
      (Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null);
  }
  function exact(value, required, optional, label) {
    if (!plain(value)) fail(`${label} must be a plain object`);
    const allowed = new Set([...required, ...(optional || [])]);
    if (!required.every((key) => has(value, key)) || Object.keys(value).some((key) => !allowed.has(key))) fail(`${label} has missing or unknown fields`);
    return value;
  }
  // Rust `char::is_whitespace` (Unicode White_Space): NOT JavaScript's `trim` set (U+0085 is whitespace for Rust only, U+FEFF for JS only).
  const BLANK = /^[\u0009-\u000D\u0020\u0085\u00A0\u1680\u2000-\u200A\u2028\u2029\u202F\u205F\u3000]*$/;
  // Rust `valid_id` of the id table: non-blank (`!trim().is_empty()`), at most 255 bytes, no NUL.
  function sourceId(value, label) {
    if (typeof value !== 'string' || BLANK.test(value) || bytes(value) > MAX_ID_BYTES || value.includes('\u0000')) fail(`${label} is not a valid source id`);
    return value;
  }
  function finite(value, label) { if (typeof value !== 'number' || !Number.isFinite(value)) fail(`${label} must be a finite number`); return value; }
  function list(value, label, each) {
    if (!Array.isArray(value)) fail(`${label} must be an array`);
    value.forEach((item, index) => each(item, `${label}[${index}]`));
    return value;
  }
  function xy(value, label) {
    exact(value, ['x', 'y'], [], label);
    finite(value.x, `${label}.x`); finite(value.y, `${label}.y`);
  }
  function solid(value, label) {
    if (value === null) return;
    exact(value, ['kind', 'r', 'g', 'b', 'a'], [], label);
    if (value.kind !== 'solid') fail(`${label}.kind must be solid`);
    for (const key of ['r', 'g', 'b', 'a']) { finite(value[key], `${label}.${key}`); if (value[key] < 0 || value[key] > 1) fail(`${label}.${key} is outside 0..1`); }
  }
  // Structural pre-check of the vector record family (schema 1 `editable-vector-path`, no stroke id, authored scope).
  function vectorRecord(value, label) {
    exact(value, ['schemaVersion', 'family', 'target', 'geometry'], ['fill', 'vector'], label);
    if (value.schemaVersion !== 1 || value.family !== 'editable-vector-path') fail(`${label} is not a schema-1 editable-vector-path`);
    importTarget(value.target, `${label}.target`);
    const g = exact(value.geometry, ['kind', 'closed', 'coordinateSpace', 'handleSpace', 'segments'], [], `${label}.geometry`);
    if (g.kind !== 'cubic-path' || g.coordinateSpace !== 'document-world' || g.handleSpace !== 'relative' || typeof g.closed !== 'boolean') fail(`${label}.geometry is not a relative cubic path in document-world`);
    if (!Array.isArray(g.segments) || g.segments.length < MIN_ANCHORS || g.segments.length > MAX_ANCHORS) fail(`${label}.geometry.segments must hold ${MIN_ANCHORS}..${MAX_ANCHORS} anchors`);
    g.segments.forEach((seg, i) => {
      exact(seg, ['point', 'handleIn', 'handleOut'], [], `${label}.geometry.segments[${i}]`);
      xy(seg.point, `${label}.geometry.segments[${i}].point`); xy(seg.handleIn, `${label}.geometry.segments[${i}].handleIn`); xy(seg.handleOut, `${label}.geometry.segments[${i}].handleOut`);
    });
    if (has(value, 'fill')) solid(value.fill, `${label}.fill`);
    if (has(value, 'vector')) {
      const v = exact(value.vector, ['stroke', 'hasRealStroke', 'strokeWidth', 'strokeCap', 'strokeJoin', 'opacity'], ['miterLimit', 'dashArray', 'dashOffset'], `${label}.vector`);
      solid(v.stroke, `${label}.vector.stroke`);
      if (typeof v.hasRealStroke !== 'boolean' || v.hasRealStroke !== (v.stroke !== null)) fail(`${label}.vector.hasRealStroke must agree with stroke`);
      finite(v.strokeWidth, `${label}.vector.strokeWidth`); finite(v.opacity, `${label}.vector.opacity`);
      if (!CAPS.includes(v.strokeCap) || !JOINS.includes(v.strokeJoin)) fail(`${label}.vector cap or join is not admitted`);
      if (has(v, 'miterLimit')) finite(v.miterLimit, `${label}.vector.miterLimit`);
      if (has(v, 'dashOffset')) finite(v.dashOffset, `${label}.vector.dashOffset`);
      if (has(v, 'dashArray')) list(v.dashArray, `${label}.vector.dashArray`, (n, at) => finite(n, at));
    }
  }

  // The native target is remapped by the planner; no caller chooses its new stroke identity.
  function importTarget(value, label) {
    exact(value, ['contextId', 'frameScope', 'layerUid', 'strokeId'], [], `${label}`);
    if (typeof value.contextId !== 'string' || !value.contextId) fail(`${label}.contextId must be a non-empty string`);
    if (typeof value.layerUid !== 'string' || !value.layerUid) fail(`${label}.layerUid must be a non-empty string`);
    if (value.strokeId !== '') fail(`${label}.strokeId must be empty: the owner mints it`);
    exact(value.frameScope, ['kind', 'frame'], [], `${label}.frameScope`);
    if (value.frameScope.kind !== 'authored') fail(`${label}.frameScope.kind must be authored`);
    if (!Number.isSafeInteger(value.frameScope.frame) || value.frameScope.frame < 0 || value.frameScope.frame > MAX_FRAME) fail(`${label}.frameScope.frame must be a native u32 frame`);
  }
  function boundedTag(value, label) {
    if (typeof value !== 'string' || !value.length || bytes(value) > MAX_ID_BYTES || value.includes('\u0000')) fail(`${label} is not a native tag`);
  }
  function positive(value, label) {
    if (finite(value, label) <= 0) fail(`${label} must be positive`);
  }
  function rasterItem(value, label) {
    exact(value, ['assetId', 'x', 'y', 'width', 'height'], ['opacity', 'rotation', 'isBitmapBrush', 'brushGroupId', 'isBrushTextureCopy', 'meshId'], label);
    sourceId(value.assetId, `${label}.assetId`);
    xy({ x: value.x, y: value.y }, label);
    for (const key of ['width', 'height']) positive(value[key], `${label}.${key}`);
    if (has(value, 'rotation')) finite(value.rotation, `${label}.rotation`);
    if (has(value, 'opacity') && (finite(value.opacity, `${label}.opacity`) < 0 || value.opacity > 1)) fail(`${label}.opacity is outside 0..1`);
    for (const key of ['isBitmapBrush', 'isBrushTextureCopy']) if (has(value, key) && typeof value[key] !== 'boolean') fail(`${label}.${key} must be boolean`);
    for (const key of ['brushGroupId', 'meshId']) if (has(value, key)) boundedTag(value[key], `${label}.${key}`);
  }
  const FONTS = ['Roboto-Regular', 'Roboto-Bold', 'DejaVuSans-Regular', 'DejaVuSans-Bold', 'DejaVuSerif-Regular', 'DejaVuSerif-Bold', 'Manrope-Regular', 'Inter-Regular'];
  function fontRef(value, label) {
    if (plain(value) && value.kind === 'bundled') {
      exact(value, ['kind', 'fontKey'], [], label);
      if (!FONTS.includes(value.fontKey)) fail(`${label} is not a bundled face`);
    } else {
      exact(value, ['kind', 'assetId'], [], label);
      if (value.kind !== 'asset') fail(`${label} is not a font reference`);
      sourceId(value.assetId, `${label}.assetId`);
    }
  }
  function malformedUtf16(text) {
    return Array.from(text).some(character => {
      const code = character.codePointAt(0);
      return code >= 0xD800 && code <= 0xDFFF;
    });
  }
  function textBlock(value, label) {
    exact(value, ['text', 'font', 'size', 'color', 'align', 'anchorTopLeft'], ['fixedWidth', 'fixedHeight', 'bold', 'italic', 'underline', 'strike', 'letterSpacing', 'wordSpacing', 'lineHeightMult', 'textCase'], label);
    if (typeof value.text !== 'string' || !value.text.trim() || value.text.includes('\u0000') || value.text.length > 65536 || malformedUtf16(value.text)) fail(`${label}.text is not visible native text`);
    fontRef(value.font, `${label}.font`);
    positive(value.size, `${label}.size`);
    if (typeof value.color !== 'string' || !/^#(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(value.color)) fail(`${label}.color is invalid`);
    if (!['left', 'center', 'right'].includes(value.align)) fail(`${label}.align is invalid`);
    xy(value.anchorTopLeft, `${label}.anchorTopLeft`);
    for (const key of ['fixedWidth', 'fixedHeight']) if (has(value, key) && value[key] !== null) positive(value[key], `${label}.${key}`);
    if (has(value, 'lineHeightMult')) positive(value.lineHeightMult, `${label}.lineHeightMult`);
    for (const key of ['letterSpacing', 'wordSpacing']) if (has(value, key)) finite(value[key], `${label}.${key}`);
    for (const key of ['bold', 'italic', 'underline', 'strike']) if (has(value, key) && typeof value[key] !== 'boolean') fail(`${label}.${key} must be boolean`);
    if (has(value, 'textCase') && !['none', 'upper', 'lower', 'capitalize'].includes(value.textCase)) fail(`${label}.textCase is invalid`);
  }
  function itemRows(value, key, member, check) {
    if (!has(value, key)) return;
    list(value[key], `${OP}.${key}`, (row, at) => {
      exact(row, ['sourceId', 'entry'], [], at);
      sourceId(row.sourceId, `${at}.sourceId`);
      exact(row.entry, ['target', member], [], `${at}.entry`);
      importTarget(row.entry.target, `${at}.entry.target`);
      check(row.entry[member], `${at}.entry.${member}`);
    });
  }
  const objectRows = value => value.records.concat(value.raster || [], value.text || []);
  const assetRows = value => (value.assets || []).concat(value.sources || []);
  function sourceRow(value, label) {
    exact(value, ['sourceId', 'name', 'mediaType', 'source'], [], label);
    sourceId(value.sourceId, `${label}.sourceId`);
    sourceId(value.name, `${label}.name`);
    const source = value.source;
    if (value.mediaType === 'image/png') {
      exact(source, ['kind', 'path'], [], `${label}.source`);
      if (source.kind !== 'file' || typeof source.path !== 'string' || source.path.includes('\u0000') ||
          !/^(?:\/|[A-Za-z]:[\\/]|\\\\)/.test(source.path)) fail(`${label} is not an absolute native file source`);
    } else if (value.mediaType === 'font/ttf' && plain(source) && source.kind === 'bundled') {
      fontRef(source, `${label}.source`);
    } else if (value.mediaType === 'font/ttf') {
      exact(source, ['kind', 'family', 'weight', 'italic'], [], `${label}.source`);
      if (source.kind !== 'google' || !Number.isInteger(source.weight) || source.weight < 1 || source.weight > 1000 || typeof source.italic !== 'boolean') fail(`${label} is not a native Google font source`);
      sourceId(source.family, `${label}.source.family`);
    } else fail(`${label}.mediaType is not a native import source`);
  }
  function sourcePins(value) {
    if (!has(value, 'sources')) {
      if (has(value, 'documentSnapshotId') || has(value, 'lifecycleGeneration')) fail('source pins require sources');
      return;
    }
    if (!Array.isArray(value.sources) || value.sources.length < 1 || value.sources.length > 1024) fail('sources must hold 1..1024 descriptors');
    if (typeof value.documentSnapshotId !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(value.documentSnapshotId) ||
        !Number.isSafeInteger(value.lifecycleGeneration) || value.lifecycleGeneration < 1) fail('native source pins are invalid');
    list(value.sources, `${OP}.sources`, sourceRow);
  }


  function payload(operation, value) {
    if (operation !== OP) fail('native import operation is not declared');
    exact(value, ['command', 'layers', 'records'], ['skipped', 'unavailable', 'assets', 'raster', 'text', 'sources', 'documentSnapshotId', 'lifecycleGeneration', 'objectOrder'], OP);
    if (value.command !== COMMAND) fail(`${OP} command is invalid`);
    list(value.layers, `${OP}.layers`, (layer, at) => { exact(layer, ['sourceId', 'name'], [], at); sourceId(layer.sourceId, `${at}.sourceId`); if (typeof layer.name !== 'string' || !layer.name.length) fail(`${at}.name must be a non-empty string`); });
    list(value.records, `${OP}.records`, (item, at) => { exact(item, ['sourceId', 'record'], [], at); sourceId(item.sourceId, `${at}.sourceId`); vectorRecord(item.record, `${at}.record`); });
    if (has(value, 'assets')) list(value.assets, `${OP}.assets`, (row, at) => {
      exact(row, ['sourceId', 'assetId'], [], at);
      sourceId(row.sourceId, `${at}.sourceId`); sourceId(row.assetId, `${at}.assetId`);
    });
    itemRows(value, 'raster', 'item', rasterItem);
    itemRows(value, 'text', 'block', textBlock);
    sourcePins(value);
    if (has(value, 'skipped')) list(value.skipped, `${OP}.skipped`, (skip, at) => {
      exact(skip, ['kind', 'sourceId', 'reason'], [], at); sourceId(skip.sourceId, `${at}.sourceId`);
      if (!SKIP_KINDS.includes(skip.kind)) fail(`${at}.kind is not admitted`);
      if (typeof skip.reason !== 'string' || !skip.reason.length) fail(`${at}.reason must say why the entry is skipped`);
    });
    if (has(value, 'unavailable')) list(value.unavailable, `${OP}.unavailable`, (row, at) => {
      exact(row, ['sourceId', 'need'], [], at); sourceId(row.sourceId, `${at}.sourceId`);
      if (!PROVIDERS.includes(row.need)) fail(`${at}.need is not a known provider`);
    });
    // Source ids are unique per kind (layers, strokes = records + element skips, assets): the owner's id table refuses a repeat.
    const dup = (ids, label) => { if (new Set(ids).size !== ids.length) fail(`${label} repeats a source id`); };
    dup(value.layers.map((l) => l.sourceId), 'layers');
    const skipped = value.skipped || [];
    dup(objectRows(value).map((r) => r.sourceId).concat(skipped.filter((s) => s.kind === 'element').map((s) => s.sourceId)), 'records and element skips');
    dup(skipped.filter((s) => s.kind === 'layer').map((s) => s.sourceId), 'layer skips');
    dup(assetRows(value).map(r => r.sourceId).concat(skipped.filter((s) => s.kind === 'asset').map((s) => s.sourceId)), 'assets and asset skips');
    if (has(value, 'objectOrder')) {
      list(value.objectOrder, `${OP}.objectOrder`, sourceId);
      const ids = objectRows(value).map(row => row.sourceId);
      if (value.objectOrder.length !== ids.length || new Set(value.objectOrder).size !== ids.length ||
          value.objectOrder.some(id => !ids.includes(id))) fail('objectOrder must be an exact permutation of new object source ids');
    }
  }

  function receiptArrays(value) {
    list(value.layers, 'result.layers', (row, at) => { exact(row, ['sourceId', 'layerUid'], [], at); sourceId(row.sourceId, `${at}.sourceId`); if (typeof row.layerUid !== 'string' || !row.layerUid) fail(`${at}.layerUid must be a non-empty string`); });
    list(value.objects, 'result.objects', (row, at) => { exact(row, ['sourceId', 'strokeId'], [], at); sourceId(row.sourceId, `${at}.sourceId`); if (typeof row.strokeId !== 'string' || !row.strokeId) fail(`${at}.strokeId must be a non-empty string`); });
    if (has(value, 'assets')) list(value.assets, 'result.assets', (row, at) => {
      exact(row, ['sourceId', 'assetId'], [], at);
      sourceId(row.sourceId, `${at}.sourceId`); sourceId(row.assetId, `${at}.assetId`);
    });
    list(value.skipped, 'result.skipped', (row, at) => {
      exact(row, ['kind', 'sourceId', 'reason'], [], at); sourceId(row.sourceId, `${at}.sourceId`);
      if (!SKIP_KINDS.includes(row.kind) || typeof row.reason !== 'string' || !row.reason.length) fail(`${at} is not a skip report`);
    });
  }
  function result(operation, value) {
    if (operation !== OP) fail('native import operation is not declared');
    exact(value, ['applied', 'historyEntriesAdded', 'layers', 'objects', 'skipped'], ['assets'], 'import result');
    if (typeof value.applied !== 'boolean' || value.historyEntriesAdded !== (value.applied ? 1 : 0)) fail('invalid import disposition');
    receiptArrays(value);
    if (bytes(JSON.stringify(value)) > MAX_RECEIPT_BYTES) fail('import receipt exceeds its bound');
  }
  function correlateBatch(request, value) {
    payload(OP, request.payload); result(OP, value);
    const input = request.payload, skipped = input.skipped || [];
    function mapped(wanted, actual, key, ordered) {
      return wanted.length === actual.length && new Set(actual.map(r => r.sourceId)).size === actual.length &&
        new Set(actual.map(r => r[key])).size === actual.length && wanted.every((r, i) => ordered ? actual[i].sourceId === r.sourceId : actual.some(m => m.sourceId === r.sourceId));
    }
    const layerIds = input.layers.map(r => r.sourceId).concat(skipped.filter(r => r.kind === 'layer').map(r => r.sourceId));
    if ((input.unavailable || []).length || new Set(layerIds).size !== layerIds.length ||
        value.applied !== !!(input.layers.length || objectRows(input).length || assetRows(input).length) ||
        !mapped(input.layers, value.layers, 'layerUid') || !mapped(objectRows(input), value.objects, 'strokeId', !!((input.raster || []).length || (input.text || []).length)) ||
        !mapped(assetRows(input), value.assets || [], 'assetId', true) ||
        skipped.length !== value.skipped.length || skipped.some((r,i) => ['kind','sourceId','reason'].some(k => r[k] !== value.skipped[i][k]))) {
      fail('import receipt does not match declared batch');
    }
  }
  function correlate(request, response) {
    if (response.instanceId !== request.instanceId) fail('import response owner mismatch');
    const replaced = response.ok === false && response.error && response.error.code === 'wrong_document';
    if (response.documentId !== request.documentId && !replaced) fail('import response document mismatch');
    if (!response.ok) return;
    if (response.contentRevision !== request.expectedRevision + (response.result.applied ? 1 : 0)) fail('import receipt revision mismatch');
    correlateBatch(request, response.result);
  }
  // Source ids, names and layer references are opaque authored strings that may legitimately start with data: or blob:.
  const OPAQUE = Object.freeze({ has: (at) => /^(payload|result)\./.test(at) });
  const admitted = (operation) => operation === OP;
  const own = Object.freeze({ admitted, requiresRevision: admitted, payload, result, correlate, paths: (operation) => (admitted(operation) ? OPAQUE : null) });

  // Same interface as the font contract, so one injected seam serves several contracts.
  function compose(base) {
    if (!base) return own;
    return Object.freeze({
      admitted: (op) => admitted(op) || base.admitted(op),
      requiresRevision: (op) => admitted(op) || base.requiresRevision(op),
      payload: (op, value, envelope) => (admitted(op) ? payload(op, value) : base.payload(op, value, envelope)),
      result: (op, value) => (admitted(op) ? result(op, value) : base.result(op, value)),
      correlate: (request, response) => (admitted(request.operation) ? correlate(request, response) : base.correlate(request, response)),
      paths: (op) => (admitted(op) ? OPAQUE : base.paths(op)),
    });
  }
  return Object.freeze({
    OP, COMMAND, MAX_REQUEST_BYTES, MAX_RECEIPT_BYTES, MIN_ANCHORS, MAX_ANCHORS, MAX_ID_BYTES, SKIP_KINDS, PROVIDERS,
    admitted, requiresRevision: admitted, payload, result, correlate, paths: own.paths, compose, bytes,
  });
}));
