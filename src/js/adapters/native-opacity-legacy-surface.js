/* N20 stateless legacy mechanics. The caller owns policy, tokens, timers and receipts. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else {
    root.NemoNativeOpacityLegacySurface = api;
    if (typeof root.n20AllowLegacyWrite === 'undefined') root.n20AllowLegacyWrite = function (kind) {
      return api.allowLegacyWrite(root, kind);
    };
    if (typeof root.n20RequireLegacyWrite === 'undefined') root.n20RequireLegacyWrite = function (kind) {
      return api.requireLegacyWrite(root, kind, root.n20AllowLegacyWrite);
    };
  }
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  function allowLegacyWrite(root, kind, central) {
    try {
      if (central !== undefined) return typeof central === 'function' && central(kind) === true;
      var admission = root && root.NemoNativeOpacityLegacyAdmission;
      return admission === undefined || !!admission && typeof admission.allow === 'function' &&
        admission.allow(kind) === true;
    } catch (_) { return false; }
  }
  function requireLegacyWrite(root, kind, central) {
    if (allowLegacyWrite(root, kind, central)) return true;
    var error = new Error('Native document authority must release before this legacy edit.');
    error.name = 'NemoNativeReleaseRequired';
    throw error;
  }
  function wrapWriter(target, name, allow, kind, admitted) {
    var original = target && target[name];
    if (typeof original !== 'function') return function () {};
    function wrapped() {
      if (allow(kind) !== true) return false;
      if (admitted) admitted();
      return original.apply(this, arguments);
    }
    target[name] = wrapped;
    return function () { if (target[name] === wrapped) target[name] = original; };
  }
  function mapping(root) {
    var bounds = root.document.getElementById('drawing-canvas').getBoundingClientRect();
    var dpr = root.devicePixelRatio || 1;
    if (!(bounds.width > 0 && bounds.height > 0 && dpr > 0)) throw new Error('native viewport is unavailable');
    return { cssBounds: { x: bounds.left, y: bounds.top, width: bounds.width, height: bounds.height },
      physicalExtent: { width: Math.max(1, Math.round(bounds.width * dpr)),
        height: Math.max(1, Math.round(bounds.height * dpr)) },
      compositionExtent: { width: 320, height: 180 }, reportedDpr: dpr };
  }
  function publish(root, admission, cutover, project, handlers, resize) {
    var original = Object.freeze({ admission: root.NemoNativeOpacityLegacyAdmission,
      cutover: root.NemoNativeOpacityCutover, project: root.NemoNativeOpacityProject,
      handle: root.NemoApplication.handle, meta: root.NemoOpacityApplication.meta,
      legacy: root.NemoOpacityApplication.legacy });
    root.NemoNativeOpacityLegacyAdmission = admission;
    root.NemoNativeOpacityCutover = cutover;
    root.NemoNativeOpacityProject = project;
    var handle = root.NemoApplication.handle = function (request) {
      var result = handlers.handle(request);
      return result === null ? original.handle(request) : result;
    };
    var meta = root.NemoOpacityApplication.meta = function () {
      return handlers.meta() || original.meta();
    };
    var legacy = root.NemoOpacityApplication.legacy = function () {
      var result = handlers.legacy.apply(null, arguments);
      return result === null ? original.legacy.apply(null, arguments) : result;
    };
    function onError(event) {
      if (event && event.error && event.error.name === 'NemoNativeReleaseRequired') event.preventDefault();
    }
    root.addEventListener('error', onError, true);
    root.addEventListener('resize', resize);
    return function () {
      root.removeEventListener('error', onError, true);
      root.removeEventListener('resize', resize);
      if (root.NemoNativeOpacityLegacyAdmission === admission) root.NemoNativeOpacityLegacyAdmission = original.admission;
      if (root.NemoNativeOpacityCutover === cutover) root.NemoNativeOpacityCutover = original.cutover;
      if (root.NemoNativeOpacityProject === project) root.NemoNativeOpacityProject = original.project;
      if (root.NemoApplication.handle === handle) root.NemoApplication.handle = original.handle;
      if (root.NemoOpacityApplication.meta === meta) root.NemoOpacityApplication.meta = original.meta;
      if (root.NemoOpacityApplication.legacy === legacy) root.NemoOpacityApplication.legacy = original.legacy;
    };
  }
  function desktopPorts(root, transport) {
    var tauri = root.__TAURI__, invoke = tauri.core.invoke;
    return Object.freeze({
      document: root.SMProjectDocument, editor: root.NemoNativeOpacityEditorAdapter,
      selection: root.NemoNativeOpacitySelectionAdapter,
      createPreview: function () { return root.NemoNativeOpacityPreviewAdapter.createNativeOpacityPreviewAdapter(); },
      createExporter: function () { return root.NemoNativeOpacityExportAdapter.createNativeOpacityExportAdapter(); },
      bootstrap: async function (prepared) {
        var host = await invoke('nemo_mcp_identity');
        return invoke('nemo_native_bootstrap', { request: { apiVersion: 2,
          instanceId: host.instanceId, projection: prepared.projection,
          resources: prepared.resources, outputBindings: [], viewport: mapping(root) } });
      },
      connect: function () { return transport.connect(); },
      connectionStatus: function () { return transport.status(); },
      subscribeRevisions: function (synchronize) {
        if (!tauri.event || typeof tauri.event.listen !== 'function') {
          return Promise.reject(new Error('native revision event port is unavailable'));
        }
        return transport.subscribeRevisions(tauri.event.listen.bind(tauri.event), synchronize);
      },
      disconnect: function () { return transport.disconnect(); },
      application: function () { return root.NemoNativeApplicationAdapter.createNativeApplicationAdapter('ui', transport); },
      release: function (request) { return invoke('nemo_native_release', { request: request }); },
      previewHost: function (request) { return invoke('nemo_native_preview', { request: request }); },
      bindOutput: function (request) { return invoke('nemo_native_bind_output', { request: request }); },
      legacyImport: root.SM.importJSON.bind(root.SM),
      capabilities: function () { return root.NemoApplication.capabilities(); },
      currentFrame: function () { return root.state.currentFrame; },
      afterChange: function () {
        if (root.renderLayerList) root.renderLayerList();
        if (root.renderTimeline) root.renderTimeline();
        if (root.updateUI) root.updateUI();
        if (root.SMEngineBridge) root.SMEngineBridge.renderNow();
      },
      sleep: function (ms) { return new Promise(function (resolve) { root.setTimeout(resolve, ms); }); },
      surface: Object.freeze({
        installGuard: function (controller) {
          if (!root.SMEngineBridge || typeof root.SMEngineBridge !== 'object') {
            throw new Error('native opacity cutover requires the accepted engine bridge');
          }
          root.SMEngineBridge.nativeEditGuard = root.SMNativeEditGuard.install(controller);
        },
        allow: function (kind) {
          var bridge = root.SMEngineBridge;
          if (!bridge || !Object.prototype.hasOwnProperty.call(bridge, 'nativeEditGuard')) return true;
          return !!bridge.nativeEditGuard && typeof bridge.nativeEditGuard.allow === 'function' &&
            bridge.nativeEditGuard.allow(kind) === true;
        },
        wrap: function (target, names, prefix, allow, admitted) {
          var targets = { global: root, timeline: root.SM, motion: root.SMMotion, script: root.SMScript, plugin: root.SMPlugin };
          var receipts = Object.freeze(names.map(function (name) {
            return wrapWriter(targets[target], name, allow, prefix + name, admitted);
          }));
          return function () { receipts.forEach(function (dispose) { dispose(); }); };
        },
        extensionOpen: function () {
          return !!(root.SMPlugin && typeof root.SMPlugin.openCount === 'function' && root.SMPlugin.openCount() > 0) ||
            !!(root.SMPanelUI && typeof root.SMPanelUI.openCount === 'function' && root.SMPanelUI.openCount() > 0);
        },
        publish: function () { return publish.apply(null, [root].concat(Array.from(arguments))); },
        toast: function (message) { if (root.showToast) root.showToast(message); },
        defer: function (callback, ms) { return root.setTimeout(callback, ms); },
        cancel: function (timer) { root.clearTimeout(timer); },
        resize: function (current) { return invoke('nemo_native_viewport_resize', {
          request: { apiVersion: 2, instanceId: current.instanceId, viewport: mapping(root) } }); }
      })
    });
  }

  return Object.freeze({ allowLegacyWrite: allowLegacyWrite, requireLegacyWrite: requireLegacyWrite,
    wrapWriter: wrapWriter,
    desktopPorts: desktopPorts, defer: function (root, callback) { return root.setTimeout(callback, 0); } });
}));
