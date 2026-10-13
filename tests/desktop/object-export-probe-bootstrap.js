(function () {
  'use strict';
  const config = window.__NEMO_OBJECT_EXPORT_PROBE_CONFIG__;
  const keys = config && Object.keys(config).sort().join(',');
  if (keys !== 'destinationDirectory,pid,runId,sourceSha,taskKey' ||
      !/^[a-f0-9-]{36}$/.test(config.runId) || !/^[a-f0-9]{40}$/.test(config.sourceSha) ||
      !/^[a-f0-9]{64}$/.test(config.taskKey) || !Number.isSafeInteger(config.pid) || config.pid <= 0 ||
      typeof config.destinationDirectory !== 'string' ||
      !config.destinationDirectory.endsWith('/reports/object-export-' + config.runId) ||
      window.__NEMO_OBJECT_EXPORT_PROBE_STARTED__) return;
  window.__NEMO_OBJECT_EXPORT_PROBE_STARTED__ = true;
  const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
  const target = { contextId: 'scene-root', frameScope: { kind: 'authored', frame: 0 }, layerUid: 'L0', strokeId: 'a' };
  const document = {
    format: 'nemo.native-object-document', formatVersion: 2, totalFrames: 1, layers: [{ layerUid: 'L0' }],
    objects: [{ schemaVersion: 1, family: 'closed-solid-fill-cubic-path', target,
      geometry: { kind: 'cubic-path', closed: true, coordinateSpace: 'document-world', handleSpace: 'relative',
        segments: [[16, 16], [120, 16], [120, 120], [16, 120]].map(([x, y]) => ({ point: { x, y }, handleIn: { x: 0, y: 0 }, handleOut: { x: 0, y: 0 } })) },
      fill: { kind: 'solid', r: 0, g: 0, b: 1, a: 1 } }],
    compositionOrder: { format: 'nemo.native-object-order-plan', formatVersion: 1, documentFormatVersion: 2,
      contexts: [{ contextId: 'scene-root', layers: [{ layerUid: 'L0', naturalPosition: 0, order: {} }],
        scopes: [{ frameScope: { kind: 'authored', frame: 0 }, elements: [{ layerUid: 'L0', strokeId: 'a', naturalPosition: 0, order: {} }] }] }],
      migration: 'explicit-plan-required', referencePolicy: 'unresolved' }
  };
  let invoke, instanceId, pins, cleanupIdentity, releaseRequest, closed = false;
  const check = value => { if (!value) throw new Error('fixed export scenario refused'); };
  async function checkpoint(phase, status, extra = {}) {
    await invoke('nemo_test_object_window_probe', { payloadJson: JSON.stringify({ runId: config.runId, phase, status, ...extra }) });
  }
  async function release() {
    if (!cleanupIdentity || closed) return;
    if (!releaseRequest) {
      let revision = cleanupIdentity.contentRevision;
      try {
        const status = await invoke('nemo_native_object_client_status');
        if (status.available === true && status.instanceId === cleanupIdentity.instanceId &&
            status.documentId === cleanupIdentity.documentId && Number.isSafeInteger(status.contentRevision) &&
            status.contentRevision >= revision) revision = status.contentRevision;
      } catch (_) { /* The bootstrap commit remains the cleanup authority. */ }
      releaseRequest = { apiVersion: 2, requestId: 'object-export-release-' + config.runId,
        instanceId: cleanupIdentity.instanceId, documentId: cleanupIdentity.documentId, expectedRevision: revision };
    }
    const receipt = await invoke('nemo_native_release', { request: releaseRequest });
    check(receipt.status === 'succeeded' && receipt.reentryAvailable === true);
    closed = true;
  }
  async function current() {
    const status = await invoke('nemo_native_object_client_status');
    check(status.available === true && status.instanceId === instanceId &&
      status.documentId === cleanupIdentity.documentId && status.contentRevision === 0 &&
      Number.isSafeInteger(status.lifecycleGeneration) && status.lifecycleGeneration > 0);
    pins = status;
    const request = { apiVersion: 2, requestId: 'object-export-read-' + config.runId,
      instanceId, documentId: pins.documentId, operation: 'query.document.object',
      payload: { atRevision: pins.contentRevision, stableTarget: target } };
    const read = await invoke('nemo_native_object_dispatch', { requestJson: JSON.stringify(request) });
    check(read.ok === true && read.instanceId === instanceId && read.documentId === pins.documentId &&
      typeof read.result?.documentSnapshotId === 'string' && read.result.documentSnapshotId.length > 0);
    pins.documentSnapshotId = read.result.documentSnapshotId;
  }
  function viewport() {
    const dpr = Number(window.devicePixelRatio) || 1;
    check(dpr >= 1 && dpr <= 4);
    return { cssBounds: { x: 32, y: 80, width: 320, height: 180 },
      physicalExtent: { width: Math.round(320 * dpr), height: Math.round(180 * dpr) },
      compositionExtent: { width: 320, height: 180 }, reportedDpr: dpr };
  }
  const outputSpec = { kind: 'frame', format: 'rgba8', width: 320, height: 180,
    colorInterpretation: 'srgb', alphaMode: 'straight' };
  async function present() {
    const request = { apiVersion: 2, requestId: 'object-export-preview-' + config.runId,
      instanceId, documentId: pins.documentId, contentRevision: 0,
      documentSnapshotId: pins.documentSnapshotId, lifecycleGeneration: pins.lifecycleGeneration,
      contextId: 'scene-root', scopeKind: 'authored', frame: 0, outputSpec, viewport: viewport() };
    for (let attempt = 0; attempt < 100; attempt++) {
      const receipt = await invoke('nemo_native_object_preview', { requestJson: JSON.stringify(request) });
      if (receipt.status === 'presented') { check(typeof receipt.workId === 'string'); return receipt; }
      check(receipt.status === 'deferred-timeout' || receipt.status === 'deferred-occluded');
      await pause(100);
    }
    throw new Error('object export preview unavailable');
  }
  function exportRequest(requestId, snapshot) {
    return { apiVersion: 2, requestId, instanceId, documentId: pins.documentId,
      contentRevision: 0, documentSnapshotId: snapshot, lifecycleGeneration: pins.lifecycleGeneration,
      contextId: 'scene-root', scopeKind: 'authored', frame: 0, outputSpec, viewport: viewport(),
      destinationDirectory: config.destinationDirectory, maximumEncodedBytes: 65536 };
  }
  async function run() {
    try {
      for (let attempt = 0; attempt < 300 && !window.__TAURI__?.core?.invoke; attempt++) await pause(100);
      check(typeof window.__TAURI__?.core?.invoke === 'function');
      invoke = window.__TAURI__.core.invoke;
      const runtime = await invoke('nemo_task_runtime');
      check(runtime.pid === config.pid && runtime.taskKey === config.taskKey &&
        runtime.windows?.length === 1 && runtime.windows[0] === 'main');
      await checkpoint('ready', 'ready');
      const identity = await invoke('nemo_mcp_identity'); instanceId = identity.instanceId;
      check(typeof instanceId === 'string' && instanceId.length > 0);
      const bootstrap = await invoke('nemo_native_object_bootstrap', { requestJson: JSON.stringify({ apiVersion: 2,
        requestId: 'object-export-bootstrap-' + config.runId, instanceId, documentJson: JSON.stringify(document) }) });
      check(bootstrap.instanceId === instanceId && typeof bootstrap.documentId === 'string' &&
        bootstrap.documentId.length > 0 && bootstrap.contentRevision === 0);
      cleanupIdentity = { instanceId, documentId: bootstrap.documentId, contentRevision: 0 };
      await current();
      await checkpoint('blue', 'presented', { workId: (await present()).workId, revision: 0 });
      let refused = false;
      try {
        await invoke('nemo_native_object_export', { requestJson: JSON.stringify(exportRequest(
          'object-export-refused-' + config.runId, pins.documentSnapshotId + '-wrong')) });
      } catch (_) { refused = true; }
      check(refused);
      await checkpoint('refused', 'refused', { revision: 0 });
      const receipt = await invoke('nemo_native_object_export', { requestJson: JSON.stringify(exportRequest(
        'object-export-' + config.runId, pins.documentSnapshotId)) });
      check(receipt.apiVersion === 2 && receipt.requestId === 'object-export-' + config.runId &&
        receipt.instanceId === instanceId && receipt.documentId === pins.documentId &&
        receipt.contentRevision === 0 && receipt.documentSnapshotId === pins.documentSnapshotId &&
        receipt.status === 'succeeded' && receipt.cleanupStatus === 'complete' &&
        receipt.externalEffectDisposition === 'committed' && receipt.errorCode == null &&
        receipt.published === true && receipt.cleanupComplete === true &&
        receipt.artifactFiles?.length === 1 && receipt.artifactFiles[0] === 'frame_0001.png');
      await checkpoint('exported', 'succeeded', { revision: 0, export: receipt });
      await release();
      await checkpoint('released', 'succeeded', { cleanupSucceeded: true });
    } catch (_) {
      if (invoke) {
        try { await release(); } catch (_) { /* Never claim cleanup after refusal. */ }
        try { await checkpoint('failed', 'operation_failed', { cleanupSucceeded: closed }); } catch (_) { /* No synthetic success. */ }
      }
    }
  }
  void run();
}());
