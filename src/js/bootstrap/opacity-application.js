// Bind the application service to existing document, history and display ports.
(function (root) {
  'use strict';
  var domain = NemoOpacityDomain;
  function newId() { return root.crypto.randomUUID(); }
  function ensureIds() { root.state.layers.forEach(function (layer) { root.SMMotion.ensureLayerUid(layer); }); }
  function read(layer, frame) { return root.SMMotion.valueAtFrame(layer, 'opacity', frame); }
  function refresh(layer) {
    var index = root.state.layers.indexOf(layer);
    if (index >= 0 && root.state.appMode === 'motion') root.SM.setActiveLayer(index, true);
    if (root.renderLayerList) root.renderLayerList();
    if (root.renderTimeline) root.renderTimeline();
    if (root.updateUI) root.updateUI();
    if (root.SMEngineBridge) root.SMEngineBridge.renderNow();
  }
  function write(operation, layer, payload) {
    var frame = root.state.currentFrame;
    if (operation === 'property.set') return domain.setValue(layer, [payload.value], frame);
    if (operation === 'property.key.set') return domain.setKeyAtFrame(layer, payload.frame, [payload.value], payload.curvePoints);
    if (operation === 'property.key.remove') return domain.removeKeyAtFrame(layer, payload.frame);
    if (operation === 'property.animation.set') return domain.setAnimated(layer, payload.animated, frame, read(layer, frame));
    throw new Error('Unsupported opacity mutation');
  }
  function history(action) {
    var stack = root.state[action === 'undo' ? 'undoStack' : 'redoStack'];
    var count = stack.length;
    root[action]();
    return stack.length < count;
  }
  var registry = root.NemoCapabilities || (root.NemoCapabilities = NemoCapabilityRegistry.create());
  var app = NemoOpacityApplicationCore.create({
    newId: newId, state: function () { return root.state; }, valueAtFrame: read, write: write,
    context: function () { return JSON.stringify([root.state.activeSymbolId || null, root.state.activeMontageViewId || null]); },
    canMutate: function () { return !root._scrubLiveActive; },
    snapshot: function () { return { layers: root.state.layers.map(function (layer) {
      return { id: layer.layerUid, name: layer.name || '', opacity: read(layer, root.state.currentFrame)[0] };
    }), frame: root.state.currentFrame, totalFrames: root.state.totalFrames }; },
    history: { checkpoint: function () { root.pushUndo(); },
      undo: function () { return history('undo'); }, redo: function () { return history('redo'); } },
    afterMutation: refresh,
    capabilities: function () { return registry.list(); }
  });
  ensureIds();
  // P06: opacity registers itself, and the application entry point reaches its
  // handler THROUGH the registry instead of closing over `app.handle`. The
  // lookup happens per call rather than being cached here, so there is exactly
  // one place that decides which handler serves a request — changing the
  // registration changes dispatch, and no stale reference survives it.
  NemoOpacityCapability.register(registry, app.handle);
  // P19/#1021: the export job registers the same way — its own descriptor with
  // its own handler — and drives the ONE session export.js memoises, so the
  // export dialog, the render queue and an MCP client share a lifecycle instead
  // of running two exporters. The session is resolved per call, never captured.
  var exportCapability = NemoExportSvgSequence.capability({
    session: function () { return (root.SMExport && root.SMExport.svgSequenceJob) ? root.SMExport.svgSequenceJob() : null; },
    meta: app.meta,
  });
  registry.register(exportCapability.descriptor, exportCapability.handler);
  // Dispatch picks the capability by operation instead of assuming there is
  // only ever one. Opacity stays the DEFAULT because it also serves the
  // service-wide operations that belong to no single feature (`capabilities`,
  // `snapshot`, `history.*`, `diagnostics.*`) and are deliberately absent from
  // its own effects.lifecycle; every other capability claims exactly the
  // operations its descriptor declares. Resolved per call from the registry,
  // for the same reason the handler lookup is (P06): changing a registration
  // changes dispatch, and no stale table survives it. Two capabilities
  // claiming one operation is a loud failure, not a silent mis-route.
  function capabilityFor(operation) {
    var found = null;
    registry.list().forEach(function (entry) {
      if (entry.id === NemoOpacityCapability.DESCRIPTOR.id) return;
      var lifecycle = (entry.descriptor.effects && entry.descriptor.effects.lifecycle) || [];
      if (lifecycle.indexOf(operation) < 0) return;
      if (found) throw new Error('Capabilities "' + found + '" and "' + entry.id + '" both claim the operation "' + operation + '".');
      found = entry.id;
    });
    return found || NemoOpacityCapability.DESCRIPTOR.id;
  }
  root.NemoApplication = {
    handle: function (request) { return registry.handlerFor(capabilityFor(request && request.operation))(request); },
    setInstanceId: app.setInstanceId,
    capabilities: function () { return registry.list(); },
  };
  root.NemoOpacityApplication = {
    meta: app.meta,
    historyChanged: app.historyChanged,
    documentChanged: function () {
      root.state.undoStack = []; root.state.undoLabels = [];
      root.state.redoStack = []; root.state.redoLabels = [];
      ensureIds(); app.documentChanged();
      if (root.renderHistoryPanelIfOpen) root.renderHistoryPanelIfOpen();
    },
    legacy: function (kind, holder, values, frame, curvePoints) {
      var result;
      if (kind === 'set') result = domain.setValue(holder, values, root.state.currentFrame);
      else if (kind === 'key-current') result = domain.setKeyAtFrame(holder, root.state.currentFrame, values);
      else if (kind === 'key-frame') result = domain.setKeyAtFrame(holder, frame, values, curvePoints);
      else if (kind === 'remove') result = domain.removeKeyAtFrame(holder, frame);
      else if (kind === 'animated') result = domain.setAnimated(holder, values, root.state.currentFrame, read(holder, root.state.currentFrame));
      else throw new Error('Unsupported opacity edit');
      app.changed();
      // Existing UI handlers own their gesture checkpoint and repaint schedule.
      return result;
    }
  };
  // These classic-script functions are global bindings. Wrapping them here
  // covers keyboard/UI history and API history without a second history stack.
  [['pushUndoLayers', 'undoStack'], ['pushUndoActiveFrame', 'undoStack'], ['undo', 'undoStack'], ['redo', 'redoStack']].forEach(function (entry) {
    var original = root[entry[0]];
    if (typeof original !== 'function') return;
    root[entry[0]] = function () {
      var stack = root.state[entry[1]], length = stack.length, last = stack[length - 1];
      var result = original.apply(this, arguments);
      stack = root.state[entry[1]];
      if (stack.length !== length || stack[stack.length - 1] !== last) app.historyChanged();
      return result;
    };
  });
})(window);

// N20: install the native controller only in the desktop shell. Browser
// startup keeps the legacy application exactly as above.
(function (root) {
  'use strict';
  var tauri = root.__TAURI__;
  if (!tauri || !tauri.core) return;
  setTimeout(function () {
    var invoke = tauri.core.invoke;
    var transport = root.NemoApplicationMcpTransport.createNativeTauriTransport(invoke);
    var application = null;
    var preview = root.NemoNativeOpacityPreviewAdapter.createNativeOpacityPreviewAdapter();
    var exporter = root.NemoNativeOpacityExportAdapter.createNativeOpacityExportAdapter();
    var legacyImport = root.SM.importJSON.bind(root.SM);
    function mapping() {
      var canvas = root.document.getElementById('drawing-canvas');
      var bounds = canvas.getBoundingClientRect();
      var dpr = root.devicePixelRatio || 1;
      if (!(bounds.width > 0 && bounds.height > 0 && dpr > 0)) throw new Error('native viewport is unavailable');
      return { cssBounds: { x: bounds.left, y: bounds.top, width: bounds.width, height: bounds.height },
        physicalExtent: { width: Math.max(1, Math.round(bounds.width * dpr)),
          height: Math.max(1, Math.round(bounds.height * dpr)) },
        compositionExtent: { width: 320, height: 180 }, reportedDpr: dpr };
    }
    function refresh() {
      if (root.renderLayerList) root.renderLayerList();
      if (root.renderTimeline) root.renderTimeline();
      if (root.updateUI) root.updateUI();
      if (root.SMEngineBridge) root.SMEngineBridge.renderNow();
    }
    var controller = root.NemoOpacityApplicationCore.createNative({
      document: root.SMProjectDocument,
      editor: root.NemoNativeOpacityEditorAdapter,
      selection: root.NemoNativeOpacitySelectionAdapter,
      preview: preview, exporter: exporter,
      bootstrap: async function (prepared) {
        var host = await invoke('nemo_mcp_identity');
        return invoke('nemo_native_bootstrap', { request: { apiVersion: 2,
          instanceId: host.instanceId, projection: prepared.projection,
          resources: prepared.resources, outputBindings: [], viewport: mapping() } });
      },
      connect: function () { return transport.connect(); },
      disconnect: function () { transport.disconnect(); application = null; },
      application: function () {
        application = application || root.NemoNativeApplicationAdapter
          .createNativeApplicationAdapter('ui', transport);
        return application;
      },
      release: function (request) { return invoke('nemo_native_release', { request: request }); },
      previewHost: function (request) { return invoke('nemo_native_preview', { request: request }); },
      bindOutput: function (request) { return invoke('nemo_native_bind_output', { request: request }); },
      legacyImport: legacyImport,
      currentFrame: function () { return root.state.currentFrame; },
      afterChange: refresh,
      sleep: function (ms) { return new Promise(function (resolve) { setTimeout(resolve, ms); }); }
    });
    root.SMNativeEditGuard.install(controller);
    root.NemoNativeOpacityCutover = controller;
    root.NemoNativeOpacityProject = Object.freeze({
      async importJSON(json, silent) {
        if (controller.blocksLegacy()) await controller.release({ kind: 'document-replacement' });
        var prepared;
        try { prepared = root.SMProjectDocument.prepareNativeOpacity(json); }
        catch (_) { return legacyImport(json, silent); }
        var imported = legacyImport(json, silent);
        if (!imported) return imported;
        if (!await controller.activate(prepared) && root.showToast) {
          root.showToast('Native opacity activation failed; this document remains legacy-owned.');
        }
        return imported;
      },
      release: function (kind) { return controller.release({ kind: kind || 'document-replacement' }); }
    });
    var legacyHandle = root.NemoApplication.handle;
    root.NemoApplication.handle = function (request) {
      var nativeResult = controller.handleV1(request);
      return nativeResult === null ? legacyHandle(request) : nativeResult;
    };
    var legacyMeta = root.NemoOpacityApplication.meta;
    root.NemoOpacityApplication.meta = function () {
      var current = controller.identity();
      return current ? { instanceId: current.instanceId, documentId: current.documentId,
        revision: current.contentRevision } : legacyMeta();
    };
    var legacyOpacity = root.NemoOpacityApplication.legacy;
    root.NemoOpacityApplication.legacy = function () {
      var nativeResult = controller.legacyIntent.apply(null, arguments);
      return nativeResult === null ? legacyOpacity.apply(null, arguments) : nativeResult;
    };
    var resizeTimer = null;
    root.addEventListener('resize', function () {
      if (!controller.isActive()) return;
      clearTimeout(resizeTimer); resizeTimer = setTimeout(async function () {
        var current = controller.identity();
        try { await invoke('nemo_native_viewport_resize', { request: {
          apiVersion: 2, instanceId: current.instanceId, viewport: mapping() } }); }
        catch (_) { controller.requestRelease({ kind: 'viewport-resize-failed' }).catch(function () {}); }
      }, 50);
    });
  }, 0);
})(window);
