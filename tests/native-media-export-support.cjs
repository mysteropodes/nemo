'use strict';
// Shared builders for the L09-K export adapter tests (not a test file). The oracle is the baseline-derived data file
// written by tools/gen_oracle.js (fbe4b8a3); everything else is a plain owner double that follows ONLY the documented
// receipt rules (it decides nothing the adapters must do).
const fs = require('node:fs');
const path = require('node:path');

const oracle = JSON.parse(fs.readFileSync(path.join(__dirname, 'native-media-export-fixtures', 'baseline-oracle.json'), 'utf8'));
const identity = () => ({ instanceId: 'instance-a', documentId: 'native-document-1', contentRevision: 7 });
const SNAPSHOT = 'native-object:native-document-1:7';
function snapshotResponse(overrides) {
  return Object.assign({
    apiVersion: 2, requestId: 'snap-1', instanceId: 'instance-a', documentId: 'native-document-1', contentRevision: 7, ok: true,
    result: { atRevision: 7, documentSnapshotId: SNAPSHOT },
  }, overrides);
}
function names(count, ext) {
  return Array.from({ length: count }, (_, i) => `frame_${String(i + 1).padStart(4, '0')}.${ext}`);
}
// An override of `undefined` removes the member (absent is not the same as undefined on the wire).
const clean = (value) => { for (const key of Object.keys(value)) if (value[key] === undefined) delete value[key]; return value; };
const base = (over) => clean(Object.assign({
  jobId: 'native-export-job-1', status: 'running', pinnedRevision: 7, documentSnapshotId: SNAPSHOT, progress: 0,
  artifact: null, cleanup: { status: 'pending' }, externalEffectDisposition: 'none',
}, over));
const receipts = {
  running: (over) => base(over),
  succeeded: (files, over) => base(Object.assign({
    status: 'succeeded', progress: 1, artifact: { target: 'output/a', files }, cleanup: { status: 'complete' }, externalEffectDisposition: 'committed',
  }, over)),
  failed: (over) => base(Object.assign({
    status: 'failed', progress: 0.5, cleanup: { status: 'complete' }, externalEffectDisposition: 'none',
    error: { code: 'frame_failed', message: 'frame failed' },
  }, over)),
  cancelled: (over) => base(Object.assign({
    status: 'cancelled', progress: 0.5, cleanup: { status: 'complete' }, externalEffectDisposition: 'none',
  }, over)),
};
function response(requestValue, result, over) {
  return Object.assign({
    apiVersion: 2, requestId: requestValue.requestId, instanceId: requestValue.instanceId, documentId: requestValue.documentId,
    contentRevision: 7, ok: true, result,
  }, over);
}
function failure(requestValue, error) {
  return { apiVersion: 2, requestId: requestValue.requestId, instanceId: requestValue.instanceId, documentId: requestValue.documentId, contentRevision: 7, ok: false, error };
}
module.exports = { oracle, identity, snapshotResponse, SNAPSHOT, names, receipts, response, failure };
