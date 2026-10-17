/* L09-K native SVG export request/receipt adapter (I41). Thin over the native SVG export job: it mints frozen v2
 * job.export.svg.* requests from a pinned snapshot and validates the job receipts against the request it minted.
 * UI-free (pickers and status are host ports), no transport, no document writer. Without a registered host route
 * every operation is typed unavailability; there is no Paper / exportSVG fallback. */
(function (root, factory) {
  const cjs = typeof module === 'object' && module.exports;
  const core = cjs ? require('./native-media-export-core.js') : root.NemoNativeMediaExportCore;
  const contract = cjs ? require('./native-media-export-contract.js') : root.NemoNativeMediaExportContract;
  const api = factory(core, contract);
  if (cjs) module.exports = api;
  else root.NemoNativeVectorExportAdapter = api;
}(typeof globalThis === 'object' ? globalThis : this, function (core, contract) {
  'use strict';

  // frames: strictly ascending source frames (at most one host job); the job publishes frame_0001.svg ... in that order.
  function build(pinned, input) {
    core.rejectRaw(input);
    core.exact(input, ['outputHandle', 'frames'], [], 'vector export input');
    const payload = {
      documentSnapshotId: pinned.documentSnapshotId,
      contextId: 'scene-root',
      outputHandle: core.id(input.outputHandle, 'outputHandle'),
      frames: contract.copyFrames(input.frames, contract.MAX_JOB_FRAMES),
    };
    contract.payload(contract.SVG_BEGIN, payload);
    return { payload, expectation: { kind: 'frames', ext: 'svg', count: payload.frames.length } };
  }

  // options: { route: true } once the host registered the export route; the default is unavailable.
  function createNativeVectorExportAdapter(options) {
    core.exact(options === undefined ? {} : options, [], ['route'], 'options');
    if (options !== undefined && options.route !== undefined && typeof options.route !== 'boolean') throw new TypeError('options.route must be boolean');
    return core.createJobAdapter({
      route: options !== undefined && options.route === true,
      operations: { begin: contract.SVG_BEGIN, status: contract.SVG_STATUS, cancel: contract.SVG_CANCEL },
      build,
    });
  }

  return core.freeze({ createNativeVectorExportAdapter });
}));
