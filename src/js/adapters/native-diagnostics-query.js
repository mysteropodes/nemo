/* T08B read-only host binding. No subscription, retained trace, or document writer. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeDiagnosticsQuery = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  function capture(root) {
    try {
      var catalog = root.NemoNativeReproduction && root.NemoNativeReproduction.capture(root);
      if (catalog) return catalog;
      var controller = root.NemoNativeOpacityCutover;
      if (!controller || !controller.isActive()) return null;
      var view = controller.getNativeIdentity(), identity = controller.identity();
      if (!view || !identity || view.documentId !== identity.documentId ||
          !Number.isSafeInteger(view.generation) || view.generation < 0) return null;
      return Object.freeze({ controller: controller, generation: view.generation,
        instanceId: identity.instanceId, documentId: identity.documentId });
    } catch (_) { return null; }
  }
  function same(a, b) {
    return !!(a && b && a.controller === b.controller && a.generation === b.generation &&
      a.instanceId === b.instanceId && a.documentId === b.documentId);
  }
  function available(root) {
    return !!(root.__TAURI__ && root.__TAURI__.core && typeof root.__TAURI__.core.invoke === 'function' &&
      root.NemoApplicationMcpTransport && root.NemoNativeApplicationAdapter);
  }
  async function recent(root, identity, requestId) {
    if (!available(root) || !same(identity, capture(root))) throw new Error('Native diagnostics unavailable.');
    if (identity.catalog) await root.NemoNativeReproduction.check(root, identity);
    // A private read connection must never disconnect the editor's revision subscription.
    var transport = root.NemoApplicationMcpTransport.createNativeTauriTransport(root.__TAURI__.core.invoke);
    var connected = await transport.connect();
    if (identity.catalog) await root.NemoNativeReproduction.check(root, identity);
    if (!same(identity, capture(root)) || connected.instanceId !== identity.instanceId ||
        connected.documentId !== identity.documentId) throw new Error('Native diagnostics identity changed.');
    var adapter = root.NemoNativeApplicationAdapter.createNativeApplicationAdapter('diagnostics-panel', transport);
    var response = await adapter.dispatch({ apiVersion: 2, requestId: requestId,
      instanceId: identity.instanceId, documentId: identity.documentId,
      operation: 'query.diagnostics.recent', payload: {} });
    if (identity.catalog) await root.NemoNativeReproduction.check(root, identity);
    if (!same(identity, capture(root))) throw new Error('Native diagnostics identity changed.');
    if (!response.ok) throw new Error('Native diagnostics query failed.');
    return response;
  }
  async function report(root, identity, token, requestId) {
    if (!available(root) || !identity || !identity.catalog || !same(identity, capture(root))) throw new Error('Native synthetic reproduction unavailable.');
    await root.NemoNativeReproduction.check(root, identity);
    var transport = root.NemoApplicationMcpTransport.createNativeTauriTransport(root.__TAURI__.core.invoke);
    var connected = await transport.connect();
    await root.NemoNativeReproduction.check(root, identity);
    if (!same(identity, capture(root)) || connected.instanceId !== identity.instanceId || connected.documentId !== identity.documentId) throw new Error('Native report identity changed.');
    var adapter = root.NemoNativeApplicationAdapter.createNativeApplicationAdapter('diagnostics-report', transport);
    var response = await adapter.dispatch({ apiVersion: 2, requestId: requestId, instanceId: identity.instanceId,
      documentId: identity.documentId, operation: 'query.reproduction.report', payload: token });
    var current = await root.NemoNativeReproduction.check(root, identity);
    if (!same(identity, capture(root)) || !response.ok || current.contentRevision !== token.expectedContentRevision) throw new Error('Refresh diagnostics; native report unavailable or stale.');
    root.NemoNativeReproduction.validateBundle(response.result.bundle);
    return response.result.bundle;
  }
  return Object.freeze({ capture: capture, same: same, available: available, recent: recent, report: report });
}));
