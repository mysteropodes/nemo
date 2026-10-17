/* Public v2 contract of the lane-7 bitmap-brush resource operations: publish one admitted embedded PNG asset as a
 * brush bitmap in the native charged store, and read the brush part of one pinned scene. Pure validation: no state,
 * no transport, no pixels. The SDK forbids raw pixel payloads, so a request names ids and a size only; the bytes
 * reach the native owner as a project asset (asset attach), never through this contract. Joined by the common SDK
 * through compose(); the native owner stays the only authority and repeats every rule on the original bytes. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeBitmapBrushResourcesContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var PUBLISH = 'command.document.object.brush.bitmap.publish';
  var QUERY = 'query.document.object.brush.bitmap';
  var OPS = [PUBLISH, QUERY];
  // bakeToCanvas clamps every bake canvas to MAX_TEX = 4096 per side (src/js/bitmap-brush.js at the baseline).
  var MAX_SIDE = 4096;
  var MAX_FRAME = 4294967295;
  var MAX_ENTRIES = 4096;
  var VERSION = /^[1-9][0-9]{0,19}$/;
  var IDENTIFIER = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;

  function fail(message) { throw new TypeError(message); }
  function plain(v) { return !!v && typeof v === 'object' && !Array.isArray(v); }
  function has(v, k) { return Object.prototype.hasOwnProperty.call(v, k); }
  function exact(v, required, label) {
    if (!plain(v)) fail(label + ' must be an object');
    if (!required.every(function (k) { return has(v, k); }) || Object.keys(v).some(function (k) { return required.indexOf(k) < 0; })) {
      fail(label + ' fields invalid');
    }
  }
  function int(v, min, max) {
    if (!Number.isSafeInteger(v) || v < min || v > (max === undefined ? Number.MAX_SAFE_INTEGER : max)) fail('integer invalid');
  }
  function bool(v) { if (typeof v !== 'boolean') fail('boolean invalid'); }
  function id(v) { if (typeof v !== 'string' || !IDENTIFIER.test(v)) fail('identifier invalid'); }
  // Opaque authored strings: non-blank, no NUL, at most 255 UTF-8 bytes (the native and font-contract rule).
  function opaque(v) {
    if (typeof v !== 'string' || !v.trim() || v.indexOf('\0') >= 0 || new TextEncoder().encode(v).length > 255) fail('opaque string invalid');
  }
  // A u64 counter in decimal: 1..18446744073709551615 (equal-length digit strings compare numerically).
  function version(v) {
    if (typeof v !== 'string' || !VERSION.test(v) || (v.length === 20 && v > '18446744073709551615')) fail('version invalid');
  }
  function side(v) { int(v, 1, MAX_SIDE); }
  // The decoded RGBA cost of a bitmap: width * height * 4 (what the baseline image budget accounts per registration).
  function bytesFor(width, height) { return width * height * 4; }
  function validSide(v) { return Number.isSafeInteger(v) && v >= 1 && v <= MAX_SIDE; }
  // The size an encoded PNG declares in its IHDR (signature, then the IHDR chunk: width and height are big-endian u32 at 16 and 20);
  // null when the bytes are not a PNG header. The bytes are otherwise forwarded untouched.
  var PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a], IHDR = [0x49, 0x48, 0x44, 0x52];
  function pngSize(bytes) {
    if (!(bytes instanceof Uint8Array) || bytes.length < 24) return null;
    if (PNG_SIGNATURE.some(function (b, i) { return bytes[i] !== b; }) || IHDR.some(function (b, i) { return bytes[12 + i] !== b; })) return null;
    var view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    return { width: view.getUint32(16), height: view.getUint32(20) };
  }
  // The order of the native ordered map: UTF-8 byte order, not UTF-16 code units.
  function compareUtf8(a, b) {
    var x = new TextEncoder().encode(a), y = new TextEncoder().encode(b);
    for (var i = 0; i < x.length && i < y.length; i++) if (x[i] !== y[i]) return x[i] - y[i];
    return x.length - y.length;
  }
  function bitmap(v) {
    side(v.width); side(v.height); version(v.version);
    if (v.bytes !== bytesFor(v.width, v.height)) fail('bitmap cost is not width * height * 4');
  }

  function payload(op, p) {
    if (op === PUBLISH) {
      exact(p, ['command', 'documentSnapshotId', 'assetId', 'brushId', 'width', 'height'], 'brush publish');
      if (p.command !== 'object.brush.bitmap.publish') fail('brush command invalid');
      id(p.documentSnapshotId); opaque(p.assetId); opaque(p.brushId); side(p.width); side(p.height);
      return;
    }
    if (!plain(p) || (p.action !== 'inspect' && p.action !== 'scene')) fail('brush query action invalid');
    if (p.action === 'inspect') {
      exact(p, ['action', 'atRevision', 'documentSnapshotId', 'brushId'], 'brush inspect');
      int(p.atRevision, 0); id(p.documentSnapshotId); opaque(p.brushId);
      return;
    }
    exact(p, ['action', 'atRevision', 'documentSnapshotId', 'contextId', 'frame', 'offset', 'limit'], 'brush scene');
    int(p.atRevision, 0); id(p.documentSnapshotId); id(p.contextId); int(p.frame, 0, MAX_FRAME); int(p.offset, 0, MAX_ENTRIES);
    if (p.limit !== 1) fail('brush scene page bound invalid');
  }

  function entry(e) {
    var ink = has(e, 'ink');
    exact(e, ['sourceObjectIndex', 'layerUid', 'strokeId', 'brushId', ink ? 'ink' : 'unavailable'], 'scene entry');
    int(e.sourceObjectIndex, 0, MAX_ENTRIES * 64); opaque(e.layerUid); opaque(e.strokeId); opaque(e.brushId);
    if (ink) { exact(e.ink, ['version', 'width', 'height', 'bytes'], 'scene ink'); bitmap(e.ink); }
    else { exact(e.unavailable, ['code'], 'scene unavailable'); opaque(e.unavailable.code); }
  }

  function result(op, v) {
    if (op === PUBLISH) {
      exact(v, ['applied', 'historyEntriesAdded', 'brushId', 'version', 'width', 'height', 'bytes'], 'publish result');
      bool(v.applied);
      // A runtime resource publication never adds a history entry and never changes the document revision.
      if (v.historyEntriesAdded !== 0) fail('history disposition invalid');
      opaque(v.brushId); bitmap(v);
      return;
    }
    if (has(v, 'brush')) {
      exact(v, ['atRevision', 'documentSnapshotId', 'brush'], 'inspect result');
      int(v.atRevision, 0); id(v.documentSnapshotId);
      var b = v.brush;
      if (!plain(b)) fail('brush row invalid');
      if (b.resident === false) { exact(b, ['brushId', 'resident'], 'brush row'); opaque(b.brushId); return; }
      exact(b, ['brushId', 'resident', 'version', 'width', 'height', 'bytes'], 'brush row');
      if (b.resident !== true) fail('brush residency invalid');
      opaque(b.brushId); bitmap(b);
      return;
    }
    exact(v, ['atRevision', 'documentSnapshotId', 'contextId', 'frame', 'offset', 'nextOffset', 'totalEntries', 'entries'], 'scene result');
    int(v.atRevision, 0); id(v.documentSnapshotId); id(v.contextId); int(v.frame, 0, MAX_FRAME);
    int(v.offset, 0, MAX_ENTRIES); int(v.totalEntries, 0, MAX_ENTRIES);
    if (!Array.isArray(v.entries) || v.offset > v.totalEntries || v.entries.length !== (v.offset < v.totalEntries ? 1 : 0)) fail('scene page invalid');
    var next = v.offset + v.entries.length;
    if (v.nextOffset !== (next < v.totalEntries ? next : null)) fail('scene next offset invalid');
    v.entries.forEach(entry);
  }

  function correlate(q, r) {
    if (r.instanceId !== q.instanceId || r.documentId !== q.documentId) fail('response owner mismatch');
    if (!r.ok) return;
    var p = q.payload, v = r.result;
    if (q.operation === PUBLISH) {
      if (r.contentRevision !== q.expectedRevision) fail('a brush publication does not change the revision');
      if (v.brushId !== p.brushId || v.width !== p.width || v.height !== p.height) fail('publish receipt names another bitmap');
      return;
    }
    if (r.contentRevision !== p.atRevision || v.atRevision !== p.atRevision || v.documentSnapshotId !== p.documentSnapshotId) fail('brush read snapshot mismatch');
    if (p.action === 'inspect') { if (!v.brush || v.brush.brushId !== p.brushId) fail('brush read selector mismatch'); return; }
    if (v.contextId !== p.contextId || v.frame !== p.frame || v.offset !== p.offset) fail('scene read selector mismatch');
  }

  // Strings that are authored identifiers and may look like data:/blob: URLs; every other string stays subject to the SDK's binary-payload rule.
  var PATHS = {
    has: function (at) {
      return at === 'payload.assetId' || at === 'payload.brushId' || at === 'result.brushId' || at === 'result.brush.brushId' ||
        /^result\.entries\.\d+\.(?:layerUid|strokeId|brushId|unavailable\.code)$/.test(at);
    },
  };
  function paths() { return PATHS; }

  var own = Object.freeze({
    admitted: function (op) { return OPS.indexOf(op) >= 0; },
    requiresRevision: function (op) { return op === PUBLISH; },
    payload: payload, result: result, correlate: correlate, paths: paths,
  });
  // The common SDK consumes one contract object; this composes it in front of the others without a second SDK seam.
  function compose(base) {
    if (!base) return own;
    return Object.freeze({
      admitted: function (op) { return own.admitted(op) || base.admitted(op); },
      requiresRevision: function (op) { return own.admitted(op) ? own.requiresRevision(op) : base.requiresRevision(op); },
      payload: function (op, p, e) { return own.admitted(op) ? own.payload(op, p) : base.payload(op, p, e); },
      result: function (op, v) { return own.admitted(op) ? own.result(op, v) : base.result(op, v); },
      correlate: function (q, r) { return own.admitted(q.operation) ? own.correlate(q, r) : base.correlate(q, r); },
      paths: function (op) { return own.admitted(op) ? own.paths(op) : base.paths(op); },
    });
  }
  return Object.freeze(Object.assign({
    compose: compose, PUBLISH: PUBLISH, QUERY: QUERY, MAX_SIDE: MAX_SIDE, MAX_ENTRIES: MAX_ENTRIES, bytesFor: bytesFor, validSide: validSide, pngSize: pngSize, compareUtf8: compareUtf8,
  }, own));
}));
