/* L09-K public v2 contract of the native SVG and media export jobs: begin payloads, job receipt, correlation.
 * Pure validation: no state, no transport. The adapters build their payloads through it and the common SDK joins it
 * through compose(); the native owner stays the only authority and repeats every rule on the pinned snapshot. */
(function (root, factory) {
  const core = typeof module === 'object' && module.exports ? require('./native-media-export-core.js') : root.NemoNativeMediaExportCore;
  const api = factory(core);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeMediaExportContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function (core) {
  'use strict';

  const SVG_BEGIN = 'job.export.svg.begin', SVG_STATUS = 'job.export.svg.status', SVG_CANCEL = 'job.export.svg.cancel';
  const MEDIA_BEGIN = 'job.export.media.begin', MEDIA_STATUS = 'job.export.media.status', MEDIA_CANCEL = 'job.export.media.cancel';
  const BEGINS = [SVG_BEGIN, MEDIA_BEGIN];
  const OPS = new Set([SVG_BEGIN, SVG_STATUS, SVG_CANCEL, MEDIA_BEGIN, MEDIA_STATUS, MEDIA_CANCEL]);
  const KINDS = ['still', 'sequence', 'video'];
  const FORMATS = ['png', 'svg'];
  // MAX_OBJECT_EXPORT_FRAMES (export_job.rs): frames of one host job. Sequences of more frames are several jobs of
  // the native plan; one receipt lists at most MAX_SEQUENCE_FRAMES names (a response is at most 4096 bytes).
  const MAX_JOB_FRAMES = 32;
  const MAX_SEQUENCE_FRAMES = core.MAX_LISTED_FILES;
  const MAX_DIMENSION = 4096; // object export OutputSpec bound (native_application_contract.rs admit_object)
  const FIXED_SPEC = Object.freeze({ kind: 'frame', format: 'rgba8', colorInterpretation: 'srgb', alphaMode: 'straight' });
  function fail(message) { throw new TypeError(message); }
  function oneOf(value, list, label) { if (!list.includes(value)) fail(`${label} is not admitted`); return value; }
  function dimension(value, label) {
    if (!Number.isSafeInteger(value) || value < 1 || value > MAX_DIMENSION) fail(`${label} must be an integer in 1..${MAX_DIMENSION}`);
    return value;
  }

  // Strictly ascending u32 frames, at most `max`; returns a frozen copy.
  function copyFrames(list, max) {
    core.rejectRaw(list);
    if (!Array.isArray(list) || list.length < 1 || list.length > max) fail(`frames must contain 1..${max} entries`);
    let previous = -1;
    return core.freeze(list.map((value, index) => {
      core.frame(value, `frames[${index}]`);
      if (value <= previous) fail('frames must be strictly ascending');
      previous = value;
      return value;
    }));
  }
  function svgBegin(payload) {
    core.exact(payload, ['documentSnapshotId', 'contextId', 'outputHandle', 'frames'], [], 'SVG begin');
    core.id(payload.documentSnapshotId, 'documentSnapshotId');
    if (payload.contextId !== 'scene-root') fail('contextId must be scene-root');
    core.id(payload.outputHandle, 'outputHandle');
    copyFrames(payload.frames, MAX_JOB_FRAMES);
  }
  // `range` members absent mean the project work area (baseline exportFrameRange: `opts.start !== undefined ? opts.start : waIn`).
  // Both present: the SVG job's invalid_range rule (start <= end). A still names its frame (start === end).
  function range(value, kind) {
    core.exact(value, [], ['start', 'end'], 'range');
    const start = Object.prototype.hasOwnProperty.call(value, 'start') ? core.frame(value.start, 'range.start') : null;
    const end = Object.prototype.hasOwnProperty.call(value, 'end') ? core.frame(value.end, 'range.end') : null;
    if (start !== null && end !== null && end < start) fail('range.end must not precede range.start');
    if (kind === 'still' && (start === null || end === null || start !== end)) fail('a still names exactly one frame');
    if (kind === 'sequence' && start !== null && end !== null && end - start + 1 > MAX_SEQUENCE_FRAMES) {
      fail(`a sequence receipt lists at most ${MAX_SEQUENCE_FRAMES} frames`);
    }
    return { start, end };
  }
  function outputSpec(value) {
    core.exact(value, ['kind', 'format', 'width', 'height', 'colorInterpretation', 'alphaMode'], [], 'outputSpec');
    if (Object.keys(FIXED_SPEC).some((key) => value[key] !== FIXED_SPEC[key])) fail('outputSpec must be the straight-alpha sRGB rgba8 frame');
    dimension(value.width, 'outputSpec.width'); dimension(value.height, 'outputSpec.height');
  }
  // `fps || state.fps` and `MP4_QUALITY_QV[quality || 'high'] || '65'` (export.js): 0 and '' fall back, any other word is a key.
  function video(value) {
    core.exact(value, [], ['fps', 'quality'], 'video');
    if (Object.prototype.hasOwnProperty.call(value, 'fps') && (typeof value.fps !== 'number' || !Number.isFinite(value.fps) || value.fps < 0)) fail('video.fps must be a finite non-negative number');
    if (Object.prototype.hasOwnProperty.call(value, 'quality') && (typeof value.quality !== 'string' || !/^[A-Za-z0-9_-]{0,32}$/.test(value.quality))) fail('video.quality must be a short word');
  }
  function mediaBegin(payload) {
    core.exact(payload, ['documentSnapshotId', 'contextId', 'outputHandle', 'output', 'range'], ['outputSpec', 'video'], 'media begin');
    core.id(payload.documentSnapshotId, 'documentSnapshotId');
    if (payload.contextId !== 'scene-root') fail('contextId must be scene-root');
    core.id(payload.outputHandle, 'outputHandle');
    core.exact(payload.output, ['kind', 'frameFormat'], [], 'output');
    const kind = oneOf(payload.output.kind, KINDS, 'output.kind');
    const format = oneOf(payload.output.frameFormat, FORMATS, 'output.frameFormat');
    if (kind === 'video' && format !== 'png') fail('a video is made of PNG frames');
    range(payload.range, kind);
    const hasSpec = Object.prototype.hasOwnProperty.call(payload, 'outputSpec');
    const wantsSpec = format === 'png';
    if (wantsSpec !== hasSpec) fail(wantsSpec ? 'PNG frames require outputSpec' : 'SVG frames carry no outputSpec');
    if (hasSpec) outputSpec(payload.outputSpec);
    if (Object.prototype.hasOwnProperty.call(payload, 'video')) {
      if (kind !== 'video') fail('video options belong to a video export');
      video(payload.video);
    }
  }
  function jobId(payload, label) { core.exact(payload, ['jobId'], [], label); core.id(payload.jobId, 'jobId'); }
  function payload(operation, value) {
    if (operation === SVG_BEGIN) return svgBegin(value);
    if (operation === MEDIA_BEGIN) return mediaBegin(value);
    return jobId(value, operation);
  }
  function result(operation, value) {
    core.rejectRaw(value);
    core.terminalInvariant(core.receipt(value));
  }
  // Responses must belong to the request owner; a job receipt pins the requested snapshot or names the requested job.
  function correlate(q, r) {
    if (r.instanceId !== q.instanceId) fail('response owner mismatch');
    if (!r.ok) return;
    if (r.documentId !== q.documentId) fail('response document mismatch');
    if (BEGINS.includes(q.operation)) {
      if (r.result.pinnedRevision !== q.expectedRevision || r.result.documentSnapshotId !== q.payload.documentSnapshotId) fail('job receipt does not retain the requested snapshot');
    } else if (r.result.jobId !== q.payload.jobId) fail('job receipt names another job');
  }
  const own = Object.freeze({
    admitted: (operation) => OPS.has(operation),
    requiresRevision: (operation) => BEGINS.includes(operation),
    payload, result, correlate,
    paths: () => new Set(),
  });
  // The common SDK consumes one contract object; this composes it in front of the existing one without a second SDK seam.
  function compose(base) {
    if (!base) return own;
    return Object.freeze({
      admitted: (op) => own.admitted(op) || base.admitted(op),
      requiresRevision: (op) => own.admitted(op) ? own.requiresRevision(op) : base.requiresRevision(op),
      payload: (op, p, e) => own.admitted(op) ? own.payload(op, p) : base.payload(op, p, e),
      result: (op, v) => own.admitted(op) ? own.result(op, v) : base.result(op, v),
      correlate: (q, r) => own.admitted(q.operation) ? own.correlate(q, r) : base.correlate(q, r),
      paths: (op) => own.admitted(op) ? own.paths(op) : base.paths(op),
    });
  }
  return Object.freeze(Object.assign({
    compose, copyFrames, KINDS, FORMATS, MAX_JOB_FRAMES, MAX_SEQUENCE_FRAMES, MAX_DIMENSION, FIXED_SPEC,
    SVG_BEGIN, SVG_STATUS, SVG_CANCEL, MEDIA_BEGIN, MEDIA_STATUS, MEDIA_CANCEL,
  }, own));
}));
