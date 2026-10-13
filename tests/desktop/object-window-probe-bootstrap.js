(function () {
  'use strict';
  const config = window.__NEMO_OBJECT_WINDOW_PROBE_CONFIG__;
  const keys = config && Object.keys(config).sort().join(',');
  if (keys !== 'pid,runId,sourceSha,taskKey' || !/^[a-f0-9-]{36}$/.test(config.runId) ||
      !/^[a-f0-9]{40}$/.test(config.sourceSha) || !/^[a-f0-9]{64}$/.test(config.taskKey) ||
      !Number.isSafeInteger(config.pid) || config.pid <= 0 || window.__NEMO_OBJECT_WINDOW_PROBE_STARTED__) return;
  window.__NEMO_OBJECT_WINDOW_PROBE_STARTED__ = true;
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
  let invoke, instanceId, pins, closed = false, ordinal = 0;
  const check = value => { if (!value) throw new Error('fixed scenario refused'); };
  async function checkpoint(phase, status, frame) {
    const payload = { runId: config.runId, phase, status };
    if (frame) { payload.workId = frame.workId; payload.revision = pins.contentRevision; }
    if (phase === 'failed') payload.cleanupSucceeded = closed;
    await invoke('nemo_test_object_window_probe', { payloadJson: JSON.stringify(payload) });
  }
  async function current() {
    pins = await invoke('nemo_native_object_client_status');
    check(pins.available === true && pins.instanceId === instanceId && Number.isSafeInteger(pins.contentRevision) && Number.isSafeInteger(pins.lifecycleGeneration));
    const read = await dispatch('query.document.object', { atRevision: pins.contentRevision, stableTarget: target }, false);
    check(typeof read.result.documentSnapshotId === 'string');
    pins.documentSnapshotId = read.result.documentSnapshotId;
  }
  async function dispatch(operation, payload, mutation) {
    const request = { apiVersion: 2, requestId: 'object-probe-' + config.runId + '-' + (++ordinal),
      instanceId, documentId: pins.documentId, operation, payload };
    if (mutation) request.expectedRevision = pins.contentRevision;
    const result = await invoke('nemo_native_object_dispatch', { requestJson: JSON.stringify(request) });
    check(result.ok === true && result.instanceId === instanceId && result.documentId === pins.documentId);
    return result;
  }
  async function present() {
    const dpr = Number(window.devicePixelRatio) || 1;
    check(dpr >= 1 && dpr <= 4);
    const request = { apiVersion: 2, requestId: 'object-probe-preview-' + (++ordinal), instanceId,
      documentId: pins.documentId, contentRevision: pins.contentRevision, documentSnapshotId: pins.documentSnapshotId,
      lifecycleGeneration: pins.lifecycleGeneration, contextId: 'scene-root', scopeKind: 'authored', frame: 0,
      outputSpec: { kind: 'frame', format: 'rgba8', width: 320, height: 180, colorInterpretation: 'srgb', alphaMode: 'straight' },
      viewport: { cssBounds: { x: 32, y: 80, width: 320, height: 180 }, physicalExtent: { width: Math.round(320 * dpr), height: Math.round(180 * dpr) },
        compositionExtent: { width: 320, height: 180 }, reportedDpr: dpr } };
    for (let attempt = 0; attempt < 100; attempt++) {
      const receipt = await invoke('nemo_native_object_preview', { requestJson: JSON.stringify(request) });
      if (receipt.status === 'presented') { check(typeof receipt.workId === 'string'); return receipt; }
      check(receipt.status === 'deferred-timeout' || receipt.status === 'deferred-occluded');
      await pause(100);
    }
    throw new Error('presentation unavailable');
  }
  async function release() {
    if (!pins || closed) return;
    const status = await invoke('nemo_native_object_client_status');
    check(status.available === true && status.instanceId === instanceId && status.documentId === pins.documentId);
    const receipt = await invoke('nemo_native_release', { request: { apiVersion: 2,
      requestId: 'object-probe-release-' + config.runId, instanceId, documentId: status.documentId, expectedRevision: status.contentRevision } });
    check(receipt.status === 'succeeded' && receipt.reentryAvailable === true);
    closed = true;
  }
  async function run() {
    try {
      for (let attempt = 0; attempt < 300 && !window.__TAURI__?.core?.invoke; attempt++) await pause(100);
      check(typeof window.__TAURI__?.core?.invoke === 'function');
      invoke = window.__TAURI__.core.invoke;
      const runtime = await invoke('nemo_task_runtime');
      check(runtime.pid === config.pid && runtime.taskKey === config.taskKey && runtime.windows?.length === 1 && runtime.windows[0] === 'main');
      await checkpoint('ready', 'ready');
      const identity = await invoke('nemo_mcp_identity'); instanceId = identity.instanceId;
      check(typeof instanceId === 'string');
      await invoke('nemo_native_object_bootstrap', { requestJson: JSON.stringify({ apiVersion: 2,
        requestId: 'object-probe-bootstrap-' + config.runId, instanceId, documentJson: JSON.stringify(document) }) });
      await current(); check(pins.contentRevision === 0); await checkpoint('blue', 'presented', await present());
      await dispatch('command.document.object.fill.set', { command: 'object.fill.set', stableTarget: target,
        fill: { kind: 'solid', r: 1, g: 0, b: 0, a: 1 } }, true);
      await current(); check(pins.contentRevision === 1); await checkpoint('red', 'presented', await present());
      await dispatch('command.document.object.undo', { command: 'object.undo' }, true);
      await current(); check(pins.contentRevision === 2); await checkpoint('undo', 'presented', await present());
      await dispatch('command.document.object.redo', { command: 'object.redo' }, true);
      await current(); check(pins.contentRevision === 3); await checkpoint('redo', 'presented', await present());
      await release(); await checkpoint('released', 'succeeded');
    } catch (_) {
      if (invoke) {
        try { await release(); } catch (_) { /* Native/process cleanup stays open. */ }
        try { await checkpoint('failed', 'operation_failed'); } catch (_) { /* Never synthesize a success. */ }
      }
    }
  }
  void run();
}());
