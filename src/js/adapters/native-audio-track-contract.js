/* Pure AudioTrack metadata contract; no writable mirror, source admission or playback. */
(function(root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeAudioTrackContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function() {
  'use strict';
  const OP = 'command.document.object.audio.track.edit';
  // Rust str::trim whitespace; U+FEFF is deliberately not blank.
  const BLANK = /^[\u0009-\u000D\u0020\u0085\u00A0\u1680\u2000-\u200A\u2028\u2029\u202F\u205F\u3000]*$/;
  const has = (v, k) => Object.prototype.hasOwnProperty.call(v, k);
  const fail = message => { throw new TypeError(message); };
  function exact(v, required, optional = []) {
    if (!v || typeof v !== 'object' || Array.isArray(v) ||
        ![Object.prototype, null].includes(Object.getPrototypeOf(v))) {
      fail('audio metadata must be a plain object');
    }
    if (required.some(k => !has(v, k)) ||
        Object.keys(v).some(k => !required.includes(k) && !optional.includes(k))) {
      fail('invalid audio metadata fields');
    }
    return v;
  }
  function text(v) {
    if (typeof v !== 'string' || v.includes('\0')) fail('invalid audio metadata string');
    // JSON/Rust strings cannot preserve unpaired UTF-16 surrogates. Refuse
    // before encoding rather than silently replacing an authored identity/name.
    for (let i = 0; i < v.length; i++) {
      const unit = v.charCodeAt(i);
      if (unit >= 0xD800 && unit <= 0xDBFF) {
        const next = v.charCodeAt(++i);
        if (!(next >= 0xDC00 && next <= 0xDFFF)) fail('invalid audio Unicode string');
      } else if (unit >= 0xDC00 && unit <= 0xDFFF) fail('invalid audio Unicode string');
    }
    const size = typeof TextEncoder === 'function'
      ? new TextEncoder().encode(v).length : unescape(encodeURIComponent(v)).length;
    if (size > 255) fail('audio metadata string exceeds UTF-8 bound');
  }
  function audioId(v) {
    text(v);
    if (BLANK.test(v)) fail('invalid audio id');
  }
  function number(v) {
    if (typeof v !== 'number' || !Number.isFinite(v)) fail('audio trim must be a JSON number');
  }
  function payload(op, v) {
    if (op !== OP) fail('audio track operation is not declared');
    if (v && v.kind === 'setMuted') {
      exact(v, ['kind', 'audioId', 'muted']);
      audioId(v.audioId);
      if (typeof v.muted !== 'boolean') fail('audio muted must be boolean');
    } else if (v && v.kind === 'rename') {
      exact(v, ['kind', 'audioId', 'name']);
      audioId(v.audioId); text(v.name);
    } else if (v && v.kind === 'setTrim') {
      exact(v, ['kind', 'audioId'], ['trimStart', 'trimEnd']);
      audioId(v.audioId);
      if (!has(v, 'trimStart') && !has(v, 'trimEnd')) fail('audio trim field required');
      if (has(v, 'trimStart')) number(v.trimStart);
      if (has(v, 'trimEnd') && v.trimEnd !== null) number(v.trimEnd);
    } else if (v && v.kind === 'unset') {
      exact(v, ['kind', 'audioId', 'field']);
      audioId(v.audioId);
      if (!['name', 'muted', 'trimStart', 'trimEnd'].includes(v.field)) fail('invalid audio unset field');
    } else fail('invalid audio track metadata edit');
  }
  function result(op, v) {
    if (op !== OP) fail('audio track operation is not declared');
    exact(v, ['applied', 'historyEntriesAdded']);
    if (typeof v.applied !== 'boolean' || v.historyEntriesAdded !== (v.applied ? 1 : 0)) {
      fail('invalid audio metadata disposition');
    }
  }
  function correlate(request, response) {
    if (response.instanceId !== request.instanceId) fail('audio response owner mismatch');
    const replaced = !response.ok && response.error && response.error.code === 'wrong_document';
    if (response.documentId !== request.documentId && !replaced) fail('audio response document mismatch');
    if (response.ok) {
      result(OP, response.result);
      if (response.contentRevision !== request.expectedRevision + (response.result.applied ? 1 : 0)) {
        fail('audio response revision mismatch');
      }
    }
  }
  // Authored opaque strings are not resource ingress, even when their literal
  // text begins with data: or blob:. No other resource-shaped field is admitted.
  const OPAQUE = Object.freeze({has: at => /^payload\.(audioId|name)$/.test(at)});
  const admitted = op => op === OP;
  const own = Object.freeze({admitted, requiresRevision: admitted, payload, result,
    correlate, paths: op => admitted(op) ? OPAQUE : null});
  function compose(base) {
    if (!base) return own;
    return Object.freeze(Object.assign({}, base, {
      admitted: op => admitted(op) || base.admitted(op),
      requiresRevision: op => admitted(op) || base.requiresRevision(op),
      payload: (op, v, envelope) => admitted(op) ? payload(op, v) : base.payload(op, v, envelope),
      result: (op, v) => admitted(op) ? result(op, v) : base.result(op, v),
      correlate: (request, response) => admitted(request.operation)
        ? correlate(request, response) : base.correlate(request, response),
      paths: op => admitted(op) ? OPAQUE : base.paths(op),
    }));
  }
  return Object.freeze({OP, admitted, requiresRevision: admitted, payload, result,
    correlate, paths: own.paths, compose});
}));
