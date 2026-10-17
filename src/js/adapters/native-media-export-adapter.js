/* L09-K native media export request/receipt adapter (I16b): still, image sequence and video outputs of the pinned
 * snapshot. Thin over the native media export job (G44): it mints frozen v2 job.export.media.* requests and validates the
 * receipts, including an independent check of the published file names and count. UI-free (the picker and the status line
 * are host ports), no transport, no document writer. Exports are silent (baseline); a missing link, a missing decoder or
 * an unavailable host route is typed, never a blank frame and never a legacy fallback. */
(function (root, factory) {
  const cjs = typeof module === 'object' && module.exports;
  const core = cjs ? require('./native-media-export-core.js') : root.NemoNativeMediaExportCore;
  const contract = cjs ? require('./native-media-export-contract.js') : root.NemoNativeMediaExportContract;
  const api = factory(core, contract);
  if (cjs) module.exports = api;
  else root.NemoNativeMediaExportAdapter = api;
}(typeof globalThis === 'object' ? globalThis : this, function (core, contract) {
  'use strict';

  const AUDIO_DISPOSITION = 'silent_baseline';
  const has = (value, key) => Object.prototype.hasOwnProperty.call(value, key);

  // input: { outputHandle, kind, frameFormat, range: { start?, end? }, size?: { width, height }, video?: { fps?, quality? } }
  // range members absent = the project work area; PNG frames need size (the pinned OutputSpec), SVG frames take none.
  function build(pinned, input) {
    core.rejectRaw(input);
    core.exact(input, ['outputHandle', 'kind', 'frameFormat', 'range'], ['size', 'video'], 'media export input');
    core.exact(input.range, [], ['start', 'end'], 'range');
    const payload = {
      documentSnapshotId: pinned.documentSnapshotId,
      contextId: 'scene-root',
      outputHandle: core.id(input.outputHandle, 'outputHandle'),
      output: { kind: input.kind, frameFormat: input.frameFormat },
      range: { ...(has(input.range, 'start') ? { start: input.range.start } : {}), ...(has(input.range, 'end') ? { end: input.range.end } : {}) },
    };
    if (has(input, 'size')) {
      core.exact(input.size, ['width', 'height'], [], 'size');
      payload.outputSpec = { ...contract.FIXED_SPEC, width: input.size.width, height: input.size.height };
    }
    if (has(input, 'video')) {
      core.exact(input.video, [], ['fps', 'quality'], 'video');
      payload.video = { ...(has(input.video, 'fps') ? { fps: input.video.fps } : {}), ...(has(input.video, 'quality') ? { quality: input.video.quality } : {}) };
    }
    contract.payload(contract.MEDIA_BEGIN, payload);
    const { start, end } = payload.range;
    const known = start !== undefined && end !== undefined;
    const expectation = input.kind === 'video'
      ? { kind: 'video' }
      : { kind: 'frames', ext: input.frameFormat, count: known ? end - start + 1 : null };
    return { payload, expectation };
  }

  // options: { route: true } once the host registered the export route; the default is unavailable.
  function createNativeMediaExportAdapter(options) {
    core.exact(options === undefined ? {} : options, [], ['route'], 'options');
    if (options !== undefined && options.route !== undefined && typeof options.route !== 'boolean') throw new TypeError('options.route must be boolean');
    return core.createJobAdapter({
      route: options !== undefined && options.route === true,
      operations: { begin: contract.MEDIA_BEGIN, status: contract.MEDIA_STATUS, cancel: contract.MEDIA_CANCEL },
      build,
    });
  }

  return core.freeze({ createNativeMediaExportAdapter, AUDIO_DISPOSITION });
}));
