/* T10 explicit catalog UI binding; Rust remains the only document/journal owner. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeReproduction = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var fixture = Object.freeze({ id: 'native-opacity-static', version: 1,
    sha256: '895ca05a43295104301c472287149b1c01aa6ef681db80dcbb79e58a55b36050' });
  var sessions = new WeakMap();
  var downloads = new WeakMap();
  function state(root) {
    if (!sessions.has(root)) sessions.set(root, { identity: null, busy: false });
    return sessions.get(root);
  }
  function plain(v) { return !!v && typeof v === 'object' && (Object.getPrototypeOf(v) === Object.prototype || Object.getPrototypeOf(v) === null); }
  function exact(v, keys) {
    if (!plain(v) || Object.keys(v).sort().join(',') !== keys.slice().sort().join(',')) throw new Error('Invalid native reproduction data.');
  }
  function integer(v) { return Number.isSafeInteger(v) && v >= 0; }
  function id(v) { return typeof v === 'string' && /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(v); }
  function available(root) { return !!(root.__TAURI__ && root.__TAURI__.core && typeof root.__TAURI__.core.invoke === 'function' && root.NemoNativeApplicationAdapter); }
  function invoke(root, command, args) {
    if (!available(root)) throw new Error('Native catalog unavailable in browser/WASM.');
    return root.__TAURI__.core.invoke(command, args);
  }
  function capture(root) { return state(root).identity; }
  function same(a, b) { return !!a && a === b; }
  function matches(identity, status) {
    return status && id(status.subscriptionId) && status.instanceId === identity.instanceId && status.documentId === identity.documentId &&
      status.lifecycleGeneration === identity.generation && integer(status.contentRevision);
  }
  async function check(root, identity) {
    if (!identity || !same(identity, capture(root))) throw new Error('Native catalog session changed.');
    // Binding is an existing identity read, not subscribe/disconnect. Native
    // status intentionally omits generation; the host binding owns that fence.
    var status;
    try { status = await invoke(root, 'nemo_native_revision_sync', { request: { action: 'binding' } }); }
    catch (_) {
      // A retired authority has no binding. Clear only after the host confirms
      // vacancy/replacement; a transient read error retains the owned token.
      var unavailable = await invoke(root, 'nemo_native_status');
      if (same(identity, capture(root)) && unavailable && unavailable.apiVersion === 2 &&
          (unavailable.available === false || unavailable.instanceId !== identity.instanceId || unavailable.documentId !== identity.documentId)) {
        state(root).identity = null; disposeDownloads(root);
      }
      throw new Error('Native catalog binding unavailable.');
    }
    if (!same(identity, capture(root)) || !matches(identity, status)) {
      if (same(identity, capture(root))) { state(root).identity = null; disposeDownloads(root); }
      throw new Error('Native catalog session changed.');
    }
    return status;
  }
  async function start(root, optIn) {
    if (optIn !== true || !available(root)) throw new Error('Explicit native catalog opt-in required.');
    var local = state(root);
    if (local.busy || local.identity) throw new Error('Native catalog session already active.');
    local.busy = true;
    try {
      var status = await invoke(root, 'nemo_native_status');
      if (!status || status.apiVersion !== 2 || status.available !== false) throw new Error('Close the active native document before starting a synthetic session.');
      var host = await invoke(root, 'nemo_mcp_identity');
      if (!host || !id(host.instanceId)) throw new Error('Native catalog identity unavailable.');
      var receipt = await invoke(root, 'nemo_native_reproduction_session', { request: {
        apiVersion: 2, instanceId: host.instanceId, optIn: true, fixture: fixture } });
      exact(receipt, ['apiVersion', 'instanceId', 'documentId', 'contentRevision', 'lifecycleGeneration',
        'origin', 'fixture', 'reproduction', 'viewportAvailable', 'resourceCount']);
      if (!receipt || receipt.apiVersion !== 2 || receipt.instanceId !== host.instanceId || !id(receipt.documentId) ||
          receipt.contentRevision !== 0 || !integer(receipt.lifecycleGeneration) || receipt.lifecycleGeneration === 0 ||
          receipt.origin !== 'embedded_catalog' || receipt.viewportAvailable !== false || receipt.resourceCount !== 0) throw new Error('Native catalog admission failed.');
      exact(receipt.fixture, ['id', 'version', 'sha256']);
      if (Object.keys(fixture).some(function (key) { return receipt.fixture[key] !== fixture[key]; })) throw new Error('Native catalog provenance unavailable.');
      exact(receipt.reproduction, ['state', 'reason', 'commandCount', 'exportable']);
      if (receipt.reproduction.state !== 'recording' || receipt.reproduction.reason !== null || receipt.reproduction.commandCount !== 0 || receipt.reproduction.exportable !== false) throw new Error('Native recording unavailable.');
      local.identity = Object.freeze({ controller: local, catalog: true, instanceId: receipt.instanceId,
        documentId: receipt.documentId, generation: receipt.lifecycleGeneration });
      await check(root, local.identity);
      return local.identity;
    } finally { local.busy = false; }
  }
  async function setOpacity(root, identity, value, requestId) {
    if (!Number.isFinite(value) || value < 0 || value > 100) throw new Error('Opacity must be between 0 and 100.');
    var local = state(root);
    if (local.busy) throw new Error('Native catalog operation pending.');
    local.busy = true;
    try {
      var status = await check(root, identity);
      var adapter = root.NemoNativeApplicationAdapter.createNativeApplicationAdapter('catalog-ui', {
        dispatch: function (request) { return invoke(root, 'nemo_native_dispatch', { request: request }); } });
      var response = await adapter.dispatch({ apiVersion: 2, requestId: requestId, instanceId: identity.instanceId,
        documentId: identity.documentId, expectedRevision: status.contentRevision, operation: 'command.document.apply',
        payload: { command: 'layer.opacity.set', stableTarget: { layerUid: 'r08_curve_layer' }, value: value } });
      if (!response.ok || response.instanceId !== identity.instanceId || response.documentId !== identity.documentId) throw new Error('Native catalog edit failed.');
      await check(root, identity);
      return response;
    } finally { local.busy = false; }
  }
  async function end(root, identity, requestId) {
    var local = state(root);
    if (local.busy) throw new Error('Native catalog operation pending.');
    local.busy = true;
    try {
      var status = await check(root, identity);
      var receipt = await invoke(root, 'nemo_native_release', { request: { apiVersion: 2, requestId: requestId,
        instanceId: identity.instanceId, documentId: identity.documentId, expectedRevision: status.contentRevision } });
      if (!same(identity, capture(root)) || !receipt || receipt.apiVersion !== 2 || receipt.requestId !== requestId ||
          receipt.instanceId !== identity.instanceId || receipt.documentId !== identity.documentId ||
          !integer(receipt.lifecycleGeneration) || receipt.lifecycleGeneration <= identity.generation ||
          receipt.status !== 'succeeded' || receipt.authorityRemovalCompleted !== true || receipt.reentryAvailable !== true) throw new Error('Native catalog release unavailable.');
      var after = await invoke(root, 'nemo_native_status');
      if (!same(identity, capture(root)) || !after || after.apiVersion !== 2 || after.instanceId !== identity.instanceId ||
          after.available !== false || (after.documentId !== null && after.documentId !== undefined)) throw new Error('Native catalog release changed.');
      local.identity = null;
      disposeDownloads(root);
      return receipt;
    } finally { local.busy = false; }
  }
  function validateBundle(bundle) {
    exact(bundle, ['format', 'formatVersion', 'apiVersion', 'fixture', 'command', 'stableTarget', 'clock', 'seed', 'versions', 'commands']);
    exact(bundle.fixture, ['id', 'version', 'sha256']); exact(bundle.stableTarget, ['layerUid']); exact(bundle.versions, ['nativeEngine']);
    if (bundle.format !== 'nemo.native-opacity-reproduction' || bundle.formatVersion !== 1 || bundle.apiVersion !== 2 ||
        Object.keys(fixture).some(function (key) { return bundle.fixture[key] !== fixture[key]; }) ||
        bundle.command !== 'layer.opacity.set' || bundle.stableTarget.layerUid !== 'r08_curve_layer' ||
        bundle.clock !== null || bundle.seed !== null || bundle.versions.nativeEngine !== '0.1.0' ||
        !Array.isArray(bundle.commands) || bundle.commands.length < 1 || bundle.commands.length > 32) throw new Error('Native reproduction format unavailable.');
    var revision = 0, opacity = 25;
    bundle.commands.forEach(function (command, index) {
      exact(command, ['id', 'expectedRevision', 'value', 'revision', 'applied']);
      var applied = command.value !== opacity;
      if (command.id !== index + 1 || command.expectedRevision !== revision || typeof command.value !== 'number' ||
          !Number.isFinite(command.value) || command.value < 0 || command.value > 100 || command.applied !== applied ||
          command.revision !== revision + (applied ? 1 : 0)) throw new Error('Native reproduction sequence invalid.');
      revision = command.revision; opacity = command.value;
    });
    var bytes = new TextEncoder().encode(JSON.stringify(bundle)).length;
    if (bytes > 3072) throw new Error('Native reproduction exceeds its byte limit.');
    return true;
  }
  function disposeDownloads(root) {
    var owned = downloads.get(root);
    if (!owned) return;
    downloads.delete(root);
    if (owned.timer !== null) root.clearTimeout(owned.timer);
    if (root.removeEventListener) root.removeEventListener('beforeunload', owned.dispose);
    root.URL.revokeObjectURL(owned.url);
  }
  function download(root, bundle, current) {
    validateBundle(bundle);
    if (typeof current !== 'function' || !current()) throw new Error('Refresh diagnostics before reporting.');
    disposeDownloads(root);
    var url = null, anchor = null, clicked = false;
    try {
      url = root.URL.createObjectURL(new root.Blob([JSON.stringify(bundle)], { type: 'application/json' }));
      var owned = { url: url, timer: null, dispose: function () { disposeDownloads(root); } };
      downloads.set(root, owned);
      anchor = root.document.createElement('a'); anchor.href = url; anchor.download = 'nemo-native-opacity-reproduction.json';
      root.document.body.appendChild(anchor);
      if (!current()) throw new Error('Refresh diagnostics before reporting.');
      anchor.click();
      // WebKit may consume the URL after click returns. Retain one bounded
      // object URL for a short handoff; failure and panel disposal revoke now.
      owned.timer = root.setTimeout(owned.dispose, 1000);
      if (root.addEventListener) root.addEventListener('beforeunload', owned.dispose);
      clicked = true;
    } finally {
      try { if (anchor) anchor.remove(); } finally { if (url !== null && !clicked) disposeDownloads(root); }
    }
  }
  return Object.freeze({ available: available, capture: capture, same: same, check: check, start: start,
    setOpacity: setOpacity, end: end, validateBundle: validateBundle, download: download, disposeDownloads: disposeDownloads });
}));
