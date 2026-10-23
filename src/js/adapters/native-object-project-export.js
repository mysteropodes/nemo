/* Private export request/receipt adapter. The project session lends its owned
 * queue and immutable metadata; Rust owns rendering, artifacts and publication. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectProjectExport = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  function integer(value, minimum) {
    return Number.isSafeInteger(value) && value >= minimum;
  }

  function run(ports, destination, frames, onProgress, signal, format) {
    var requested = Array.isArray(frames) ? frames.slice() : null;
    return ports.enqueue(async function (pin) {
      if (!requested || requested.length === 0 || requested.length > 9999 ||
          !Array.from(requested).every(function (frame, index) {
            return integer(frame, 0) && frame < pin.totalFrames &&
              (index === 0 || requested[index - 1] < frame);
          })) throw new Error('Native object export frame range is invalid');
      var sequence = requested.length > 32;
      if (sequence && (format !== 'png' || !requested.every(function (frame, index) {
        return frame === requested[0] + index;
      }))) throw new Error('Native object export frame range is invalid');
      if (signal && signal.aborted) return { status: 'cancelled' };

      var request = ports.frameRequest(requested[0], pin);
      if (sequence) request.frameRange = { start: requested[0], end: requested[requested.length - 1] };
      else if (requested.length > 1) request.frames = requested;
      var manifest = requested.map(function (_, index) {
        return 'frame_' + String(index + 1).padStart(4, '0') + '.' + format;
      });
      if (format === 'svg') request.format = 'svg';
      request.destinationDirectory = destination;
      request.maximumEncodedBytes = 4 * 1024 * 1024;
      var receipt = await ports.invoke(request);
      if (!receipt || receipt.apiVersion !== 2 || receipt.requestId !== request.requestId ||
          receipt.instanceId !== request.instanceId || receipt.documentId !== request.documentId ||
          receipt.contentRevision !== request.contentRevision ||
          receipt.documentSnapshotId !== request.documentSnapshotId ||
          receipt.published !== true || receipt.cleanupComplete !== true ||
          receipt.status !== 'succeeded' || receipt.cleanupStatus !== 'complete' ||
          receipt.externalEffectDisposition !== 'committed' || receipt.errorCode !== null ||
          !Array.isArray(receipt.artifactFiles) || receipt.artifactFiles.length !== manifest.length ||
          !manifest.every(function (name, index) { return receipt.artifactFiles[index] === name; })) {
        throw new Error('Native export publication is unconfirmed');
      }
      if (onProgress) onProgress(requested.length, requested.length);
      return receipt;
    });
  }

  return Object.freeze({ run: run });
}));
