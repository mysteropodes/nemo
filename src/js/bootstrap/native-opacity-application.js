/* N20 desktop-only composition for the accepted native opacity ports. */
(function (root, factory) {
  var install = factory();
  if (typeof module === 'object' && module.exports) module.exports = install;
  else install(root);
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  return function installNativeOpacity(root) {
    var tauri = root.__TAURI__;
    if (!tauri || !tauri.core) return;
    root.NemoNativeOpacityLegacySurface.defer(root, function () {
      var transport = root.NemoApplicationMcpTransport.createNativeTauriTransport(tauri.core.invoke);
      var ports = root.NemoNativeOpacityLegacySurface.desktopPorts(root, transport);
      root.NemoOpacityApplicationCore.createNative(ports, {
        contract: root.NemoNativeOpacityContract, lifecycle: root.NemoNativeOpacityLifecycle,
        operations: root.NemoNativeOpacityOperations, motionSurface: root.NemoNativeOpacityMotionSurface
      }).install();
    });
  };
}));
