/* N20 desktop-only composition for the accepted native opacity ports. */
(function (root, factory) {
  var install = factory(typeof module === 'object' && module.exports ? require('../adapters/native-layer-playhead-cut-ui.js') : undefined);
  if (typeof module === 'object' && module.exports) module.exports = install;
  else install(root);
}(typeof globalThis === 'object' ? globalThis : this, function (CutUI) {
  'use strict';

  var bindings = new WeakMap();
  function bindObjectUI(root) {
    var project = root.NemoNativeObjectProject, previous = bindings.get(root);
    if (!project || typeof project.bindUIComposition !== 'function') throw new Error('Native Project UI binding is unavailable');
    if (previous) {
      if (previous.project !== project || previous.document !== root.document) throw new Error('Native Project UI binding changed');
      return project.bindUIComposition(root, previous.compose);
    }
    var compose = function (base, policy) {
      var child = root.NemoNativeLayerPlayheadCutUI || CutUI, remap = root.NemoNativeTimeRemapUI;
      if (!child || typeof child.compose !== 'function') throw new Error('Native cut UI composition is unavailable');
      // Without the Time Remap child its legacy entries stay disconnected writers.
      var composed = child.compose(root, base, policy);
      // #2256: the ordinary Convert to Component entry sends the native convert transaction (legacy documents keep theirs).
      var convert = root.NemoNativeComponentConvertUI;
      if (convert && typeof convert.compose === 'function') composed = convert.compose(root, base, composed);
      // #2256: entering / leaving a Component (tabs, row double-click) sends the native workspace transition.
      var enter = root.NemoNativeComponentEnterUI;
      if (enter && typeof enter.compose === 'function') composed = enter.compose(root, base, composed);
      return remap && typeof remap.compose === 'function' ? remap.compose(root, base, composed) : composed;
    };
    project.bindUIComposition(root, compose);
    bindings.set(root, {project:project, document:root.document, compose:compose});return true;
  }
  function installNativeOpacity(root) {
    var tauri = root.__TAURI__;
    if (!tauri || !tauri.core) return;
    if (root.NemoNativeObjectUI && typeof root.NemoNativeObjectUI.createPolicyPorts === 'function') bindObjectUI(root);
    root.NemoNativeOpacityLegacySurface.defer(root, function () {
      var transport = root.NemoApplicationMcpTransport.createNativeTauriTransport(tauri.core.invoke);
      var ports = root.NemoNativeOpacityLegacySurface.desktopPorts(root, transport);
      root.NemoOpacityApplicationCore.createNative(ports, {
        contract: root.NemoNativeOpacityContract, lifecycle: root.NemoNativeOpacityLifecycle,
        replacement: root.NemoNativeOpacityReplacement,
        exportWorkflow: root.NemoNativeOpacityExportWorkflow,
        previewWorkflow: root.NemoNativeOpacityPreviewWorkflow,
        operations: root.NemoNativeOpacityOperations, viewport: root.NemoNativeOpacityViewport,
        v1: root.NemoNativeOpacityV1,
        motionSurface: root.NemoNativeOpacityMotionSurface
      }).install();
    });
  }
  installNativeOpacity.bindObjectUI = bindObjectUI;
  return Object.freeze(installNativeOpacity);
}));
