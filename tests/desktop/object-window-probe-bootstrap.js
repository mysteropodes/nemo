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
    format: 'nemo.native-object-document', formatVersion: 2, totalFrames: 1,
    layers: [{ layerUid: 'L0', timing: { format: 'nemo.native-layer-timing', formatVersion: 1,
      family: 'drawing', contextId: 'scene-root', keys: [{ frame: 0 }], samples: [] } }],
    objects: [{ schemaVersion: 1, family: 'closed-solid-fill-cubic-path', target,
      geometry: { kind: 'cubic-path', closed: true, coordinateSpace: 'document-world', handleSpace: 'relative',
        segments: [[16, 16], [120, 16], [120, 120], [16, 120]].map(([x, y]) => ({ point: { x, y }, handleIn: { x: 0, y: 0 }, handleOut: { x: 0, y: 0 } })) },
      fill: { kind: 'solid', r: 0, g: 0, b: 1, a: 1 } }],
    compositionOrder: { format: 'nemo.native-object-order-plan', formatVersion: 1, documentFormatVersion: 2,
      contexts: [{ contextId: 'scene-root', layers: [{ layerUid: 'L0', naturalPosition: 0, order: {} }],
        scopes: [{ frameScope: { kind: 'authored', frame: 0 }, elements: [{ layerUid: 'L0', strokeId: 'a', naturalPosition: 0, order: {} }] }] }],
      migration: 'explicit-plan-required', referencePolicy: 'unresolved' }
  };
  const sibling = JSON.parse(JSON.stringify(document.objects[0]));
  sibling.target.strokeId = 'b';
  sibling.fill = { kind: 'solid', r: 0, g: 1, b: 0, a: 1 };
  sibling.geometry.segments.forEach(segment => { segment.point.x += 144; });
  document.objects.push(sibling);
  document.compositionOrder.contexts[0].scopes[0].elements.push({ layerUid: 'L0', strokeId: 'b', naturalPosition: 1, order: {} });
  function canonical(value) {
    if (Array.isArray(value)) return '[' + value.map(canonical).join(',') + ']';
    if (value && typeof value === 'object') return '{' + Object.keys(value).sort().map(key => JSON.stringify(key) + ':' + canonical(value[key])).join(',') + '}';
    return JSON.stringify(value);
  }
  // HostResult rejects with a serialized NativeApplicationError {code,message,details?}.
  // Retain only this finite code vocabulary, never messages or serialized bodies.
  const refusalCodes = ['invalid_request', 'unavailable', 'wrong_instance', 'wrong_document',
    'stale_revision', 'stale_owner', 'cancelled_before_dispatch', 'cleanup_failed', 'revision_unacknowledged'];
  let invoke, instanceId, pins, cleanupIdentity, releaseRequest, closed = false, ordinal = 0, stage = 'invoke-ready';
  const check = value => { if (!value) throw new Error('fixed scenario refused'); };
  let surfaceUI, aperture, canvasBounds, welcome, previousDisplay, restored = false, underlayPresented = false;
  async function preparePresentation() {
    stage = 'presentation-ready';
    function ready() {
      const dom = window.document;
      return dom && dom.documentElement && dom.getElementById('start-screen') &&
        dom.getElementById('drawing-canvas') &&
        typeof window.NemoNativeViewportUnderlay?.desktop === 'function' &&
        typeof window.stageLayer?.visible === 'boolean' && typeof window.paper?.view?.update === 'function';
    }
    for (let attempt = 0; attempt < 300 && !ready(); attempt++) await pause(100);
    check(ready());
    check(!window.document.documentElement.classList.contains('nemo-native-underlay'));
    welcome = window.document.getElementById('start-screen');
    previousDisplay = { value: welcome.style.getPropertyValue('display'), priority: welcome.style.getPropertyPriority('display') };
    surfaceUI = window.NemoNativeViewportUnderlay.desktop(window);
    // Keep .hid unchanged: first-launch tutorial polling observes that class.
    welcome.style.setProperty('display', 'none', 'important');
    // The read below synchronously measures layout after this fixed overlay change.
    stage = 'presentation-geometry';
    const r = window.document.getElementById('drawing-canvas').getBoundingClientRect(), html = window.document.documentElement;
    check([r.left, r.top, r.width, r.height, html.clientWidth, html.clientHeight].every(Number.isFinite) &&
      r.left >= 0 && r.top >= 0 && r.width >= 320 && r.height >= 180 &&
      r.left + r.width <= html.clientWidth && r.top + r.height <= html.clientHeight);
    canvasBounds = { left: r.left, top: r.top, width: r.width, height: r.height, rootWidth: html.clientWidth, rootHeight: html.clientHeight };
    aperture = Object.freeze({ x: r.left + (r.width - 320) / 2, y: r.top + (r.height - 180) / 2, width: 320, height: 180 });
  }
  function restorePresentation() {
    if (restored) return;
    stage = 'presentation-restore';
    try { if (surfaceUI) surfaceUI.surfaceChanged(null); }
    finally {
      if (previousDisplay) {
        if (previousDisplay.value) welcome.style.setProperty('display', previousDisplay.value, previousDisplay.priority);
        else welcome.style.removeProperty('display');
      }
    }
    restored = true;
  }

  async function checkpoint(phase, status, frame, failureStage) {
    const payload = { runId: config.runId, phase, status };
    if (frame) { payload.workId = frame.workId; payload.revision = pins.contentRevision; }
    if (phase === 'failed') { payload.cleanupSucceeded = closed; payload.failureStage = failureStage; }
    else stage = phase + '-checkpoint';
    await invoke('nemo_test_object_window_probe', { payloadJson: JSON.stringify(payload) });
  }
  async function current(expectedRevision) {
    stage = 'status-invoke';
    const status = await invoke('nemo_native_object_client_status');
    stage = 'status-pins';
    check(status.available === true && status.instanceId === instanceId && status.documentId === cleanupIdentity.documentId &&
      status.contentRevision === expectedRevision && Number.isSafeInteger(status.lifecycleGeneration) &&
      (!pins || status.lifecycleGeneration === pins.lifecycleGeneration));
    cleanupIdentity.contentRevision = status.contentRevision;
    pins = status;
    for (let index = 0; index < document.objects.length; index++) {
      const expected = JSON.parse(JSON.stringify(document.objects[index]));
      if (index === 0 && (expectedRevision === 1 || expectedRevision === 3)) expected.fill = { kind: 'solid', r: 1, g: 0, b: 0, a: 1 };
      const diagnostic = index === 0 ? 'primary-query' : 'sibling-query';
      const read = await dispatch('query.document.object', { atRevision: expectedRevision, stableTarget: expected.target }, false, diagnostic);
      stage = diagnostic + '-snapshot';
      check(typeof read.result.documentSnapshotId === 'string' && read.result.documentSnapshotId.length > 0);
      stage = diagnostic + '-record';
      check(canonical(read.result.object) === canonical(expected));
      stage = diagnostic + '-snapshot';
      if (index === 0) pins.documentSnapshotId = read.result.documentSnapshotId;
      else check(read.result.documentSnapshotId === pins.documentSnapshotId);
    }
  }
  async function dispatch(operation, payload, mutation, diagnostic) {
    const request = { apiVersion: 2, requestId: 'object-probe-' + config.runId + '-' + (++ordinal),
      instanceId, documentId: pins.documentId, operation, payload };
    if (mutation) request.expectedRevision = pins.contentRevision;
    stage = diagnostic + '-invoke';
    const result = await invoke('nemo_native_object_dispatch', { requestJson: JSON.stringify(request) });
    stage = diagnostic + '-correlation';
    check(result.ok === true && result.apiVersion === request.apiVersion && result.requestId === request.requestId &&
      result.instanceId === instanceId && result.documentId === cleanupIdentity.documentId &&
      result.contentRevision === pins.contentRevision + (mutation ? 1 : 0));
    if (mutation) {
      check(Number.isSafeInteger(result.contentRevision) && result.contentRevision >= cleanupIdentity.contentRevision);
      cleanupIdentity.contentRevision = result.contentRevision;
    }
    return result;
  }
  function verifyPresentationGeometry() {
    stage = 'presentation-geometry';
    const r = window.document.getElementById('drawing-canvas').getBoundingClientRect(), html = window.document.documentElement;
    check(['left', 'top', 'width', 'height'].every(key => r[key] === canvasBounds[key]) &&
      html.clientWidth === canvasBounds.rootWidth && html.clientHeight === canvasBounds.rootHeight &&
      html.classList.contains('nemo-native-underlay') === underlayPresented);
    check([r.left, r.top, r.width, r.height].every(Number.isFinite) && aperture.x >= r.left && aperture.y >= r.top &&
      aperture.x + aperture.width <= r.left + r.width && aperture.y + aperture.height <= r.top + r.height && aperture.x >= 0 && aperture.y >= 0 &&
      aperture.x + aperture.width <= html.clientWidth && aperture.y + aperture.height <= html.clientHeight);
    if (underlayPresented) check(['left', 'top', 'right', 'bottom'].every((edge, index) =>
      parseFloat(html.style.getPropertyValue('--nemo-native-' + edge)) ===
        [aperture.x, aperture.y, aperture.x + aperture.width, aperture.y + aperture.height][index]));
  }
  async function present() {
    verifyPresentationGeometry();
    stage = 'preview-dpr';
    const dpr = Number(window.devicePixelRatio) || 1;
    check(dpr >= 1 && dpr <= 4);
    const request = { apiVersion: 2, requestId: 'object-probe-preview-' + (++ordinal), instanceId,
      documentId: pins.documentId, contentRevision: pins.contentRevision, documentSnapshotId: pins.documentSnapshotId,
      lifecycleGeneration: pins.lifecycleGeneration, contextId: 'scene-root', scopeKind: 'authored', frame: 0,
      outputSpec: { kind: 'frame', format: 'rgba8', width: 320, height: 180, colorInterpretation: 'srgb', alphaMode: 'straight' },
      viewport: { cssBounds: aperture, physicalExtent: { width: Math.round(320 * dpr), height: Math.round(180 * dpr) },
        compositionExtent: { width: 320, height: 180 }, reportedDpr: dpr } };
    for (let attempt = 0; attempt < 100; attempt++) {
      stage = 'preview-invoke';
      const receipt = await invoke('nemo_native_object_preview', { requestJson: JSON.stringify(request) });
      stage = 'preview-receipt';
      if (receipt.status === 'presented') {
        check(typeof receipt.workId === 'string' && receipt.workId.length > 0 && receipt.workId.length <= 128 &&
          Number.isSafeInteger(receipt.viewGeneration) && receipt.viewGeneration > 0);
        verifyPresentationGeometry();
        stage = 'presentation-underlay';
        surfaceUI.surfaceChanged(Object.assign({}, request, receipt), aperture);
        underlayPresented = true;
        return receipt;
      }
      check(receipt.status === 'deferred-timeout' || receipt.status === 'deferred-occluded');
      await pause(100);
    }
    throw new Error('presentation unavailable');
  }
  async function release() {
    if (!cleanupIdentity || closed) return;
    if (!releaseRequest) {
      let revision = cleanupIdentity.contentRevision;
      try {
        const status = await invoke('nemo_native_object_client_status');
        if (status.available === true && status.instanceId === cleanupIdentity.instanceId && status.documentId === cleanupIdentity.documentId &&
            Number.isSafeInteger(status.contentRevision) && status.contentRevision >= revision) revision = status.contentRevision;
      } catch (_) { /* The original commit receipt remains the cleanup authority. */ }
      releaseRequest = { apiVersion: 2, requestId: 'object-probe-release-' + config.runId,
        instanceId: cleanupIdentity.instanceId, documentId: cleanupIdentity.documentId, expectedRevision: revision };
    }
    stage = 'cleanup-invoke';
    const receipt = await invoke('nemo_native_release', { request: releaseRequest });
    stage = 'cleanup-receipt';
    check(receipt.status === 'succeeded' && receipt.reentryAvailable === true);
    closed = true;
  }
  async function run() {
    try {
      for (let attempt = 0; attempt < 300 && !window.__TAURI__?.core?.invoke; attempt++) await pause(100);
      check(typeof window.__TAURI__?.core?.invoke === 'function');
      invoke = window.__TAURI__.core.invoke;
      stage = 'runtime-invoke';
      const runtime = await invoke('nemo_task_runtime');
      stage = 'runtime-pins';
      check(runtime.pid === config.pid && runtime.taskKey === config.taskKey && runtime.windows?.length === 1 && runtime.windows[0] === 'main');
      await checkpoint('ready', 'ready');
      await preparePresentation();
      stage = 'identity-invoke';
      const identity = await invoke('nemo_mcp_identity');
      stage = 'identity-pins'; instanceId = identity.instanceId;
      check(typeof instanceId === 'string');
      stage = 'bootstrap-invoke';
      const bootstrap = await invoke('nemo_native_object_bootstrap', { requestJson: JSON.stringify({ apiVersion: 2,
        requestId: 'object-probe-bootstrap-' + config.runId, instanceId, documentJson: JSON.stringify(document) }) });
      stage = 'bootstrap-receipt';
      check(bootstrap.instanceId === instanceId && typeof bootstrap.documentId === 'string' && bootstrap.documentId.length > 0 &&
        Number.isSafeInteger(bootstrap.contentRevision) && bootstrap.contentRevision >= 0);
      cleanupIdentity = { instanceId, documentId: bootstrap.documentId, contentRevision: bootstrap.contentRevision };
      await current(0); await checkpoint('blue', 'presented', await present());
      await dispatch('command.document.object.fill.set', { command: 'object.fill.set', stableTarget: target,
        fill: { kind: 'solid', r: 1, g: 0, b: 0, a: 1 } }, true, 'fill');
      await current(1); await checkpoint('red', 'presented', await present());
      await dispatch('command.document.object.undo', { command: 'object.undo' }, true, 'undo');
      await current(2); await checkpoint('undo', 'presented', await present());
      await dispatch('command.document.object.redo', { command: 'object.redo' }, true, 'redo');
      await current(3); await checkpoint('redo', 'presented', await present());
      await release(); restorePresentation(); await checkpoint('released', 'succeeded');
    } catch (error) {
      let code;
      try { if (error && typeof error === 'object') code = Object.getOwnPropertyDescriptor(error, 'code')?.value; } catch (_) { /* Unrecognized rejection retains the stage alone. */ }
      const failureStage = stage + (stage.endsWith('-invoke') && refusalCodes.includes(code) ? ':' + code : '');
      if (invoke) {
        try { await release(); } catch (_) { /* Native/process cleanup stays open. */ }
        try { restorePresentation(); } catch (_) { /* Original failure remains authoritative. */ }
        try { await checkpoint('failed', 'operation_failed', null, failureStage); } catch (_) { /* Never synthesize a success. */ }
      }
    }
  }
  void run();
}());
