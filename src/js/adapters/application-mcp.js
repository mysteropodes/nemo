// Tauri host transports. Legacy v1 starts exactly as before; native v2 is explicit-only.
(function (root, factory) {
  'use strict';
  var host = root.window || root;
  var api = factory(host);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else host.NemoApplicationMcpTransport = api;
}(typeof globalThis === 'object' ? globalThis : this, function (window) {
  'use strict';

  function boundedIdentity(value, label) {
    if (typeof value !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(value)) {
      throw new TypeError(label + ' must be a bounded identifier');
    }
    return value;
  }

  function boundedRevision(value, label) {
    if (!Number.isSafeInteger(value) || value < 0) throw new TypeError(label + ' must be a safe revision');
    return value;
  }

  function createNativeTauriTransport(invoke) {
    if (typeof invoke !== 'function') throw new TypeError('native transport requires invoke(command, args)');
    var binding = null;

    function invalidate() { binding = null; }
    function requireBinding(request) {
      if (!binding) throw new Error('Native application transport is disconnected; discover and connect first');
      if (!request || request.apiVersion !== 2 || request.instanceId !== binding.instanceId ||
          request.documentId !== binding.documentId) {
        throw new Error('Native request does not match the connected instance and document');
      }
    }

    return Object.freeze({
      async connect() {
        invalidate();
        var status = await invoke('nemo_native_status');
        if (!status || status.apiVersion !== 2 || status.available !== true) {
          throw new Error(status && status.reason ? status.reason : 'Native application is unavailable');
        }
        binding = Object.freeze({
          instanceId: boundedIdentity(status.instanceId, 'instanceId'),
          documentId: boundedIdentity(status.documentId, 'documentId'),
          contentRevision: boundedRevision(status.contentRevision, 'contentRevision'),
        });
        return binding;
      },
      disconnect: invalidate,
      status() { return binding; },
      async dispatch(request) {
        requireBinding(request);
        try {
          var response = await invoke('nemo_native_dispatch', { request: request });
          if (!response || response.apiVersion !== 2 || response.requestId !== request.requestId ||
              response.instanceId !== binding.instanceId) {
            throw new Error('Native application response identity mismatch');
          }
          if (response.documentId !== binding.documentId ||
              (response.ok === false && response.error && response.error.code === 'wrong_document')) {
            invalidate();
          } else {
            binding = Object.freeze({
              instanceId: binding.instanceId,
              documentId: binding.documentId,
              contentRevision: boundedRevision(response.contentRevision, 'contentRevision'),
            });
          }
          return response;
        } catch (error) {
          invalidate();
          throw error;
        }
      },
    });
  }

  // Existing v1 JS loopback. It remains the production document authority until N20.
  function startLegacyTransport() {
    var tauri = window.__TAURI__;
    if (!tauri || !tauri.core || !tauri.event) return;
    var pending = new Map();
    var unlisten = [];

    async function start() {
      var service = window.NemoApplication;
      if (!service || typeof service.handle !== 'function') throw new Error('Application service unavailable');
      var identity = await tauri.core.invoke('nemo_mcp_identity');
      var initialized = service.setInstanceId(identity.instanceId);
      if (initialized && initialized.ok === false) throw new Error(initialized.error.message);
      unlisten.push(await tauri.event.listen('nemo-application-cancel', function (event) {
        var timer = pending.get(event.payload.connectionId);
        if (timer !== undefined) { clearTimeout(timer); pending.delete(event.payload.connectionId); }
      }));
      unlisten.push(await tauri.event.listen('nemo-application-request', function (event) {
        var payload = event.payload;
        // Yield once so cancellation already queued by the native transport wins
        // before a synchronous document mutation begins. Committed work is not rolled back.
        var timer = setTimeout(async function () {
          pending.delete(payload.connectionId);
          var response;
          try { response = await service.handle(payload.request); }
          catch (error) {
            // A transport fault must not invent a document identity or successful write.
            console.error('Nemo application transport failed:', error.message);
            return;
          }
          try { await tauri.core.invoke('nemo_mcp_reply', { connectionId: payload.connectionId, response: response }); }
          catch (error) { console.error('Nemo application reply failed:', error.message); }
        }, 0);
        pending.set(payload.connectionId, timer);
      }));
      await tauri.core.invoke('nemo_mcp_ready');
    }

    window.addEventListener('beforeunload', function () {
      pending.forEach(clearTimeout);
      pending.clear();
      unlisten.forEach(function (stop) { stop(); });
    });
    start().catch(function (error) { console.error('Nemo MCP unavailable:', error.message); });
  }

  startLegacyTransport();
  return Object.freeze({ createNativeTauriTransport });
}));
