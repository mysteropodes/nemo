'use strict';
// Execute the ordinary project-read caller with bounded fake owner replies;
// emit its untouched requests for the real Rust original-byte boundary test.
const Reads = require('../../src/js/adapters/native-object-project-reads.js');
(async () => {
  const sent = [];
  for (const contextId of ['scene-root', 'comp-a']) {
    globalThis.NemoNativeComponentContext = { contextId: () => contextId, readScope: () => contextId === 'scene-root' ? null : { contextId, totalFrames: 20 } };
    const pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 0, documentSnapshotId: 'native-object:d:0' };
    const presentation = { ...pin, owner: 'native', status: 'presented', contextId, frame: 10, workId: 'work-1', viewGeneration: 1 };
    const state = { active: true, opening: false, pending: 0, synchronizing: false, projectionReady: true, ticket: 1, info: pin, presented: { ...pin, available: true, presentation } };
    const target = { contextId, layerUid: 'L', strokeId: 's', frameScope: { kind: 'authored', frame: 0 } };
    let serial = 0;
    const reads = Reads.create({ state: () => state, settled: async () => {}, requestId: () => `caller-${++serial}`, dispatch: async (request) => {
      sent.push(request);
      const p = request.payload;
      let result;
      if (p.view) {
        const { limit, ...view } = p.view;
        const item = { stableTarget: target, closed: true, count: 3, matrix: [1, 0, 0, 1, 0, 0] };
        if (view.hitTest) Object.assign(item, { bounds: [0, 0, 10, 10], role: 'plain', filled: false });
        result = { ...view, atRevision: 0, documentSnapshotId: pin.documentSnapshotId, total: 1, nextCursor: null, preview: false, items: [item] };
      } else {
        const q = p.subselect;
        result = { atRevision: 0, documentSnapshotId: pin.documentSnapshotId, subselect: {
          stableTarget: target, closed: true, total: 3, cursor: 0, nextCursor: null, preview: false,
          segments: [0, 1, 2].map((x) => ({ point: [x, 10], handleIn: [0, 0], handleOut: [0, 0] })),
        } };
        if (q.hitTest) result.subselect.hitTest = true;
        if (q.atFrame !== undefined) result.subselect.atFrame = q.atFrame;
      }
      return { apiVersion: 2, requestId: request.requestId, instanceId: 'i', documentId: 'd', contentRevision: 0, ok: true, result };
    } });
    const selector = { contextId, layerUid: 'L', frame: 10 };
    if (contextId === 'scene-root') await reads.readSubselect(selector, pin);
    await reads.readPresentedStrokes(selector, pin, contextId === 'scene-root' ? undefined : () => false);
  }
  process.stdout.write(JSON.stringify(sent));
})().catch((error) => { console.error(error); process.exitCode = 1; });
