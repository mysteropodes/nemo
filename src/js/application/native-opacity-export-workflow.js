/* Native opacity export dispatch and receipt observation. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeOpacityExportWorkflow = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  async function run(ports, contract, application, target, current, prepared,
    requestFor, id, destination, frames, onProgress, signal) {
    if (signal && signal.aborted) return { status: 'cancelled' };
    var outputHandle = id('n20-output');
    await ports.bindOutput({ apiVersion: 2, instanceId: current.instanceId,
      documentId: current.documentId, expectedRevision: current.contentRevision,
      outputHandle: outputHandle, destination: destination });
    var snapshotRequest = requestFor(current, 'query.document.snapshot.acquire',
      { atRevision: current.contentRevision });
    var snapshotResponse = contract.successful(await application.dispatch(snapshotRequest), 'native snapshot');
    if (signal && signal.aborted) return { status: 'cancelled' };
    var selected = frames.map(function (frame) { return prepared.frames[frame]; });
    var begin = target.exporter.begin(current, snapshotResponse, id('n20-export'), outputHandle, selected);
    var observed = target.exporter.observe(begin,
      contract.successful(await application.dispatch(begin), 'native export begin'));
    while (observed.receipt.status === 'running') {
      if (onProgress) onProgress(Math.round(observed.receipt.progress * selected.length), selected.length);
      if (!(signal && signal.aborted)) await ports.sleep(10);
      var statusRequest = signal && signal.aborted
        ? target.exporter.cancel(current, id('n20-export-cancel'), observed.receipt.jobId)
        : target.exporter.status(current, id('n20-export-status'), observed.receipt.jobId);
      observed = target.exporter.observe(statusRequest,
        contract.successful(await application.dispatch(statusRequest), 'native export status/cancel'));
    }
    if (observed.receipt.status === 'cancelled') return observed.receipt;
    if (observed.receipt.status !== 'succeeded') throw new Error('native export did not succeed');
    if (onProgress) onProgress(selected.length, selected.length);
    return observed.receipt;
  }

  return Object.freeze({ run: run });
}));
