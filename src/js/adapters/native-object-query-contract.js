'use strict';
// Pure closed-shape validation shared by the native application SDK and the
// Project's read adapter. It owns no session, state, dispatch or query endpoint.
(function (root, factory) {
  const subselect = typeof module === 'object' && module.exports ? require('./native-subselect-contract.js') : root.NemoNativeSubselectContract;
  const api = factory(subselect);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectQueryContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Subselect) {
  const PAGE_KINDS = ['outline', 'verts', 'tris', 'offsets'];
  const own = (v, k) => Object.prototype.hasOwnProperty.call(v, k);
  const object = (v) => !!v && typeof v === 'object' && !Array.isArray(v) &&
    (Object.getPrototypeOf(v) === Object.prototype || Object.getPrototypeOf(v) === null);
  const exact = (v, required, optional) => object(v) && Object.keys(v).length >= required.length &&
    required.every((k) => own(v, k)) && Object.keys(v).every((k) => required.includes(k) || optional.includes(k));
  const text = (v) => typeof v === 'string' && v.length > 0;
  const bounded = (v) => typeof v === 'string' && /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(v);
  const integer = (v, min, max) => Number.isSafeInteger(v) && v >= min && (max === undefined || v <= max);
  const finite = (v) => typeof v === 'number' && Number.isFinite(v);
  function point(v) { return Array.isArray(v) && v.length === 2 && v.every(finite); }
  function points(v) { return Array.isArray(v) && v.every(point); }
  function fail(message) { throw new TypeError(message); }
  function rect(v) {
    return exact(v, ['x', 'y', 'width', 'height', 'rotation'], []) &&
      finite(v.x) && finite(v.y) && finite(v.width) && v.width > 0 && finite(v.height) && v.height > 0 && finite(v.rotation);
  }
  function target(v) {
    return exact(v, ['contextId', 'layerUid', 'frameScope', 'strokeId'], []) && text(v.contextId) && text(v.layerUid) &&
      text(v.strokeId) && exact(v.frameScope, ['kind', 'frame'], []) && ['authored', 'reference'].includes(v.frameScope.kind) &&
      integer(v.frameScope.frame, 0, 4294967295);
  }
  function meshMeta(v) {
    if (v === null) return true;
    if (!exact(v, ['meshId', 'outlineCount', 'vertexCount', 'triangleIndexCount', 'offsetsPresent'], ['cols', 'rows']) ||
      !text(v.meshId) || !integer(v.outlineCount, 0) || !integer(v.vertexCount, 0) || !integer(v.triangleIndexCount, 0) ||
      v.triangleIndexCount % 3 !== 0 || typeof v.offsetsPresent !== 'boolean' || v.outlineCount > v.vertexCount) return false;
    return (!own(v, 'cols') || integer(v.cols, 1, 64)) && (!own(v, 'rows') || integer(v.rows, 1, 64));
  }
  function imageView(v) {
    if (!exact(v, ['kind', 'contextId', 'layerUid', 'frame', 'workId', 'viewGeneration', 'page'], []) ||
      v.kind !== 'imageMesh' || !text(v.contextId) || !text(v.layerUid) || !integer(v.frame, 0, 4294967295) ||
      !bounded(v.workId) || !integer(v.viewGeneration, 1)) return false;
    const p = v.page;
    if (exact(p, ['kind'], []) && p.kind === 'selection') return true;
    return exact(p, ['kind', 'meshId', 'cursor', 'limit'], []) && PAGE_KINDS.includes(p.kind) && text(p.meshId) &&
      integer(p.cursor, 0, 4294967295) && integer(p.limit, 1, 16);
  }
  function imagePayload(v) {
    return exact(v, ['atRevision', 'documentSnapshotId', 'view'], []) && integer(v.atRevision, 0) &&
      text(v.documentSnapshotId) && imageView(v.view) || fail('invalid image-mesh query payload');
  }
  function presentationPayload(v) {
    return exact(v, ['atRevision', 'view'], []) && integer(v.atRevision, 0) &&
      exact(v.view, ['kind'], []) && v.view.kind === 'presentation' || fail('invalid presentation query payload');
  }
  function presentationResult(v) {
    return exact(v, ['kind', 'atRevision', 'documentSnapshotId', 'contextId', 'frame', 'workId', 'viewGeneration'], []) &&
      v.kind === 'presentation' && integer(v.atRevision, 0) && bounded(v.documentSnapshotId) && bounded(v.contextId) &&
      integer(v.frame, 0, 4294967295) && bounded(v.workId) && integer(v.viewGeneration, 1) || fail('invalid presentation query result');
  }
  // #2255: the widget presentation view (every widget layer's pad, puck and axes at the presented frame).
  const WIDGET_REASONS = ['control', 'expression', 'sample', 'parent', 'parent_geometry', 'canvas'];
  function widgetView(v) {
    return exact(v, ['kind', 'contextId', 'frame', 'workId', 'viewGeneration', 'cursor', 'limit'], []) && v.kind === 'widget' &&
      v.contextId === 'scene-root' && integer(v.frame, 0, 4294967295) && bounded(v.workId) && integer(v.viewGeneration, 1) &&
      integer(v.cursor, 0, 1024) && integer(v.limit, 1, 4);
  }
  function widgetPayload(v) {
    return exact(v, ['atRevision', 'documentSnapshotId', 'view'], []) && integer(v.atRevision, 0) &&
      text(v.documentSnapshotId) && widgetView(v.view) || fail('invalid widget query payload');
  }
  function widgetAxis(a) {
    return exact(a, ['key', 'min', 'max', 'rest', 'label', 'value'], []) && typeof a.key === 'string' && typeof a.label === 'string' &&
      finite(a.min) && finite(a.max) && finite(a.rest) && (a.value === null || finite(a.value));
  }
  function widgetItem(i) {
    if (!exact(i, ['layerUid', 'visible', 'locked', 'kind', 'x', 'y', 'center', 'angle', 'size', 'puck', 'rest', 'anchor', 'linear', 'unavailable'], []) ||
      !text(i.layerUid) || typeof i.visible !== 'boolean' || typeof i.locked !== 'boolean' || !['joystick', 'slider'].includes(i.kind) ||
      !widgetAxis(i.x) || (i.kind === 'slider' ? i.y !== null : !widgetAxis(i.y))) return false;
    const placed = i.center !== null, reason = i.unavailable;
    if (reason === null ? !(placed && point(i.puck)) : reason === 'control' ? !(placed && i.puck === null) :
      !(WIDGET_REASONS.includes(reason) && !placed && i.puck === null)) return false;
    return placed ? point(i.center) && point(i.size) && point(i.rest) && point(i.anchor) && finite(i.angle) && Array.isArray(i.linear) &&
      i.linear.length === 4 && i.linear.every(finite) : i.size === null && i.rest === null && i.angle === null && i.anchor === null && i.linear === null;
  }
  function widgetResult(v) {
    return exact(v, ['kind', 'atRevision', 'documentSnapshotId', 'contextId', 'frame', 'workId', 'viewGeneration', 'cursor',
      'total', 'nextCursor', 'items'], []) && v.kind === 'widget' && integer(v.atRevision, 0) && text(v.documentSnapshotId) &&
      v.contextId === 'scene-root' && integer(v.frame, 0, 4294967295) && bounded(v.workId) && integer(v.viewGeneration, 1) &&
      integer(v.cursor, 0, 1024) && integer(v.total, 0, 1024) && (v.nextCursor === null || integer(v.nextCursor, 0, 1024)) &&
      Array.isArray(v.items) && v.items.length <= 4 && v.items.every(widgetItem) || fail('invalid widget query result');
  }
  function widgetCorrelate(p, v) {
    widgetPayload(p); widgetResult(v);
    const end = v.cursor + v.items.length;
    if (v.atRevision !== p.atRevision || v.documentSnapshotId !== p.documentSnapshotId || ['contextId', 'frame', 'workId', 'viewGeneration', 'cursor']
      .some((k) => v[k] !== p.view[k]) || v.items.length > p.view.limit || end > v.total || (end === v.total ? v.nextCursor !== null : v.nextCursor !== end))
      fail('widget query result does not match its request');
    return true;
  }
  // #2255: the rig presentation view (bones, vertices, IK chains, binds, bound vertices and one vertex's weights of one layer).
  const RIG_PARTS = { bones: 8, vertices: 16, chains: 8, binds: 8, bound: 64, weights: 8 };
  const RIG_REASONS = ['expression', 'sample', 'parent', 'motion'], RIG_STATES = ['posed', 'skipped', 'unlinked', 'unavailable'];
  const id128 = (v) => typeof v === 'string' && v.length > 0 && v.length <= 128, u32 = (v) => integer(v, 0, 4294967295);
  const orNull = (v, check) => v === null || check(v);
  const boneRef = (r) => exact(r, ['boneId', 'vi'], []) && id128(r.boneId) && u32(r.vi);
  const RIG_ITEMS = {
    bones: (i) => exact(i, ['boneId', 'closed', 'radius', 'count', 'center'], []) && id128(i.boneId) && typeof i.closed === 'boolean' &&
      orNull(i.radius, finite) && integer(i.count, 2, 256) && point(i.center),
    vertices: (i) => exact(i, ['boneId', 'vi', 'point', 'handleIn', 'handleOut'], []) && id128(i.boneId) && u32(i.vi) && point(i.point) &&
      point(i.handleIn) && point(i.handleOut),
    chains: (i) => exact(i, ['endKey', 'root', 'joint', 'end', 'l1', 'l2', 'flip'], []) && typeof i.endKey === 'string' && i.endKey.length > 0 &&
      i.endKey.length <= 160 && boneRef(i.root) && boneRef(i.joint) && boneRef(i.end) && finite(i.l1) && finite(i.l2) && typeof i.flip === 'boolean',
    binds: (i) => exact(i, ['index', 'strokeId', 'meshId', 'points', 'rotate', 'state'], []) && u32(i.index) && (i.strokeId === null) !== (i.meshId === null) &&
      orNull(i.strokeId, id128) && orNull(i.meshId, id128) && u32(i.points) && typeof i.rotate === 'boolean' && RIG_STATES.includes(i.state),
    bound: (i) => Array.isArray(i) && i.length === 4 && u32(i[0]) && u32(i[1]) && finite(i[2]) && finite(i[3]),
    weights: (i) => exact(i, ['entry', 'boneId', 'ptIndex', 'offset', 'w'], []) && u32(i.entry) && id128(i.boneId) && (i.ptIndex === null) !== (i.offset === null) &&
      orNull(i.ptIndex, u32) && orNull(i.offset, finite) && finite(i.w),
  };
  function rigView(v) {
    return exact(v, ['kind', 'contextId', 'frame', 'workId', 'viewGeneration', 'layerUid', 'part', 'bind', 'vertex', 'cursor', 'limit'], []) && v.kind === 'rig' &&
      v.contextId === 'scene-root' && u32(v.frame) && bounded(v.workId) && integer(v.viewGeneration, 1) && id128(v.layerUid) && own(RIG_PARTS, v.part) &&
      u32(v.bind) && u32(v.vertex) && (v.part === 'weights' || (v.bind === 0 && v.vertex === 0)) && integer(v.cursor, 0, 100000) && integer(v.limit, 1, RIG_PARTS[v.part]);
  }
  function rigPayload(v) {
    return exact(v, ['atRevision', 'documentSnapshotId', 'view'], []) && integer(v.atRevision, 0) && text(v.documentSnapshotId) &&
      rigView(v.view) || fail('invalid rig query payload');
  }
  function rigResult(v) {
    return exact(v, ['kind', 'atRevision', 'documentSnapshotId', 'contextId', 'frame', 'workId', 'viewGeneration', 'layerUid', 'part', 'bind', 'vertex',
      'cursor', 'total', 'nextCursor', 'unavailable', 'items'], []) && v.kind === 'rig' && integer(v.atRevision, 0) && text(v.documentSnapshotId) &&
      v.contextId === 'scene-root' && u32(v.frame) && bounded(v.workId) && integer(v.viewGeneration, 1) && id128(v.layerUid) && own(RIG_PARTS, v.part) &&
      u32(v.bind) && u32(v.vertex) && integer(v.cursor, 0, 100000) && integer(v.total, 0, 100000) && orNull(v.nextCursor, (n) => integer(n, 0, 100000)) &&
      orNull(v.unavailable, (r) => RIG_REASONS.includes(r)) && (v.unavailable === null || v.total === 0) && Array.isArray(v.items) &&
      v.items.length <= RIG_PARTS[v.part] && v.items.every(RIG_ITEMS[v.part]) || fail('invalid rig query result');
  }
  function rigCorrelate(p, v) {
    rigPayload(p); rigResult(v);
    const end = v.cursor + v.items.length;
    if (v.atRevision !== p.atRevision || v.documentSnapshotId !== p.documentSnapshotId || ['contextId', 'frame', 'workId', 'viewGeneration', 'layerUid', 'part',
      'bind', 'vertex', 'cursor'].some((k) => v[k] !== p.view[k]) || v.items.length > p.view.limit || end > v.total || (end === v.total ? v.nextCursor !== null : v.nextCursor !== end))
      fail('rig query result does not match its request');
    return true;
  }
  function pageResult(p, v) {
    if (!exact(v, ['kind', 'cursor', 'total', 'nextCursor', 'items'], []) || v.kind !== p.kind ||
      !integer(v.cursor, 0, 4294967295) || !integer(v.total, 0) ||
      !(v.nextCursor === null || integer(v.nextCursor, 0, 4294967295)) || !Array.isArray(v.items) || v.items.length > p.limit || v.cursor !== p.cursor) return false;
    const end = v.cursor + v.items.length;
    if (!Number.isSafeInteger(end) || end > v.total || (v.nextCursor === null ? end !== v.total : v.nextCursor !== end || end >= v.total)) return false;
    if (p.kind === 'tris') return v.items.every((n) => integer(n, 0, 4294967295));
    return points(v.items);
  }
  function imageResult(v) {
    if (!exact(v, ['kind', 'atRevision', 'documentSnapshotId', 'contextId', 'layerUid', 'frame', 'workId',
      'viewGeneration', 'target', 'rect', 'mesh', 'page'], []) || v.kind !== 'imageMesh' ||
      !integer(v.atRevision, 0) || !text(v.documentSnapshotId) || !text(v.contextId) || !text(v.layerUid) ||
      !integer(v.frame, 0, 4294967295) || !bounded(v.workId) || !integer(v.viewGeneration, 1) || !target(v.target) ||
      v.target.contextId !== v.contextId || v.target.layerUid !== v.layerUid || !rect(v.rect) || !meshMeta(v.mesh)) return false;
    if (exact(v.page, ['kind'], []) && v.page.kind === 'selection') return true;
    const p = v.page;
    if (!exact(p, ['kind', 'cursor', 'total', 'nextCursor', 'items'], []) || !PAGE_KINDS.includes(p.kind) ||
      !integer(p.cursor, 0, 4294967295) || !integer(p.total, 0) || !(p.nextCursor === null || integer(p.nextCursor, 0, 4294967295)) ||
      !Array.isArray(p.items)) return false;
    const count = p.kind === 'outline' ? v.mesh && v.mesh.outlineCount : p.kind === 'verts' ? v.mesh && v.mesh.vertexCount :
      p.kind === 'tris' ? v.mesh && v.mesh.triangleIndexCount : v.mesh && v.mesh.vertexCount;
    const viewPage = { kind: p.kind, cursor: p.cursor, limit: 16 };
    if (v.mesh === null || !pageResult(viewPage, p) || p.total !== count) return false;
    if (p.kind === 'tris' && p.items.some((i) => i >= v.mesh.vertexCount)) return false;
    return true;
  }
  function payload(value, legacyValidator) {
    if (object(value) && own(value, 'subselect')) return Subselect.readPayload(value);
    if (object(value) && object(value.view) && value.view.kind === 'subselect') return Subselect.viewPayload(value);
    if (object(value) && object(value.view) && value.view.kind === 'presentation') return presentationPayload(value);
    if (object(value) && object(value.view) && value.view.kind === 'imageMesh') return imagePayload(value);
    if (object(value) && object(value.view) && value.view.kind === 'widget') return widgetPayload(value);
    if (object(value) && object(value.view) && value.view.kind === 'rig') return rigPayload(value);
    if (typeof legacyValidator === 'function') return legacyValidator(value);
    if (legacyValidator) {
      legacyValidator.exact(value, ['atRevision', 'stableTarget'], [], 'object query');
      legacyValidator.revision(value.atRevision, 'atRevision');
      return legacyValidator.objectTarget(value.stableTarget);
    }
    fail('legacy project-read payload validator is required');
  }
  function result(value, legacyValidator) {
    if (object(value) && own(value, 'subselect')) return Subselect.readResult(value);
    if (object(value) && value.kind === 'subselect') return Subselect.viewResult(value);
    if (object(value) && value.kind === 'presentation') return presentationResult(value);
    if (object(value) && value.kind === 'imageMesh') { if (!imageResult(value)) fail('invalid image-mesh query result'); return; }
    if (object(value) && value.kind === 'widget') return widgetResult(value);
    if (object(value) && value.kind === 'rig') return rigResult(value);
    if (typeof legacyValidator === 'function') return legacyValidator(value);
    if (legacyValidator) {
      legacyValidator.exact(value, ['atRevision', 'documentSnapshotId', 'object'], [], 'object read result');
      legacyValidator.revision(value.atRevision, 'atRevision');
      legacyValidator.objectUid(value.documentSnapshotId);
      return legacyValidator.objectRecord(value.object);
    }
    fail('legacy project-read result validator is required');
  }
  function correlate(p, value, legacyCorrelator, documentId) {
    if (object(p) && own(p, 'subselect')) return Subselect.readCorrelate(p, value, documentId);
    if (object(p) && object(p.view) && p.view.kind === 'subselect') return Subselect.viewCorrelate(p, value, documentId);
    if (object(p) && object(p.view) && p.view.kind === 'presentation') {
      presentationPayload(p); presentationResult(value);
      if (value.atRevision !== p.atRevision) fail('presentation query result does not match its request');
      if (documentId !== undefined && value.documentSnapshotId !== `native-object:${documentId}:${p.atRevision}`)
        fail('presentation snapshot mismatch');
      return true;
    }
    if (object(p) && object(p.view) && p.view.kind === 'widget') {
      const w = object(value) && own(value, 'result') ? value.result : value;
      if (documentId !== undefined && w.documentSnapshotId !== `native-object:${documentId}:${p.atRevision}`) fail('widget snapshot mismatch');
      return widgetCorrelate(p, w);
    }
    if (object(p) && object(p.view) && p.view.kind === 'rig') {
      const w = object(value) && own(value, 'result') ? value.result : value;
      if (documentId !== undefined && w.documentSnapshotId !== `native-object:${documentId}:${p.atRevision}`) fail('rig snapshot mismatch');
      return rigCorrelate(p, w);
    }
    if (!object(p) || !object(p.view) || p.view.kind !== 'imageMesh') {
      if (typeof legacyCorrelator === 'function') return legacyCorrelator(p, value);
      const expected = p.stableTarget, actual = value.object.target;
      if (value.atRevision !== p.atRevision || ['contextId', 'layerUid', 'strokeId'].some((k) => actual[k] !== expected[k]) ||
        ['kind', 'frame'].some((k) => actual.frameScope[k] !== expected.frameScope[k])) fail('object read selector mismatch');
      return true;
    }
    const v = object(value) && own(value, 'result') ? value.result : value;
    if (!imageResult(v) || v.atRevision !== p.atRevision || v.documentSnapshotId !== p.documentSnapshotId ||
      v.contextId !== p.view.contextId || v.layerUid !== p.view.layerUid || v.frame !== p.view.frame ||
      v.workId !== p.view.workId || v.viewGeneration !== p.view.viewGeneration || v.page.kind !== p.view.page.kind ||
      (p.view.page.kind !== 'selection' && (v.page.cursor !== p.view.page.cursor || v.page.total > 0 && v.page.items.length > p.view.page.limit ||
        !v.mesh || v.mesh.meshId !== p.view.page.meshId || p.view.page.kind === 'offsets' && !v.mesh.offsetsPresent)))
      fail('image-mesh query result does not match its request');
    return true;
  }
  const paths = new Set(['payload.stableTarget.layerUid', 'payload.stableTarget.strokeId',
    'result.object.target.layerUid', 'result.object.target.strokeId', 'payload.documentSnapshotId', 'payload.view.contextId',
    'payload.view.layerUid', 'payload.view.workId', 'payload.view.page.meshId', 'result.documentSnapshotId', 'result.contextId',
    'result.layerUid', 'result.workId', 'result.target.contextId', 'result.target.layerUid',
    'result.target.strokeId', 'result.mesh.meshId']);
  ['result.subselect.stableTarget.contextId','result.subselect.stableTarget.layerUid','result.subselect.stableTarget.strokeId',
    'payload.view.preview.stableTarget.contextId','payload.view.preview.stableTarget.layerUid','payload.view.preview.stableTarget.strokeId'].forEach(p=>paths.add(p));
  const knownPath = paths.has.bind(paths);
  paths.has = p => knownPath(p) || /^result\.items\.[0-7]\.stableTarget\.(contextId|layerUid|strokeId)$/.test(p) || /^result\.items\.[0-3]\.layerUid$/.test(p) ||
    /^result\.items\.\d+\.(boneId|strokeId|meshId|endKey|(root|joint|end)\.boneId)$/.test(p);
  return Object.freeze({ payload, result, correlate, paths });
}));
