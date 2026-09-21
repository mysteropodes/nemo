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
    var subscription = null;
    var epoch = 0;

    function invalidate() {
      epoch += 1;
      binding = null;
      var prior = subscription;
      subscription = null;
      if (!prior) return Promise.resolve();
      prior.stops.forEach(function (stop) { stop(); });
      if (window.removeEventListener) window.removeEventListener('beforeunload', invalidate);
      return invoke('nemo_native_revision_sync', {
        request: { action: 'disconnect', subscriptionId: prior.identity.subscriptionId },
      }).catch(function () { /* Local disposal never restores native authority. */ });
    }

    function exactEvent(value) {
      var keys = ['documentId', 'fromRevision', 'instanceId', 'lifecycleGeneration', 'requestId', 'toRevision'];
      if (!value || JSON.stringify(Object.keys(value).sort()) !== JSON.stringify(keys)) throw new Error('Malformed native revision event');
      boundedIdentity(value.instanceId, 'instanceId');
      boundedIdentity(value.documentId, 'documentId');
      boundedIdentity(value.requestId, 'requestId');
      boundedRevision(value.lifecycleGeneration, 'lifecycleGeneration');
      boundedRevision(value.fromRevision, 'fromRevision');
      boundedRevision(value.toRevision, 'toRevision');
      if (value.lifecycleGeneration === 0 || value.toRevision <= value.fromRevision) throw new Error('Invalid native revision advance');
      return Object.freeze(Object.assign({}, value));
    }

    async function subscribeRevisions(listen, synchronize) {
      if (!binding || subscription) throw new Error('Native revision subscription requires one connected transport');
      if (typeof listen !== 'function' || typeof synchronize !== 'function') throw new TypeError('Native revision listener and synchronization callback are required');
      var ticket = epoch;
      var identity = await invoke('nemo_native_revision_sync', { request: { action: 'binding' } });
      if (ticket !== epoch || subscription || identity.instanceId !== binding.instanceId || identity.documentId !== binding.documentId ||
          identity.contentRevision !== binding.contentRevision) throw new Error('Native revision binding is stale');
      boundedIdentity(identity.subscriptionId, 'subscriptionId');
      boundedRevision(identity.lifecycleGeneration, 'lifecycleGeneration');
      if (!identity.lifecycleGeneration) throw new Error('Native revision generation is unavailable');
      var current = { identity: Object.freeze(Object.assign({}, identity)), stops: [], inFlight: false,
        revision: identity.contentRevision, localAdvances: new Set() };
      subscription = current;
      async function receive(message) {
        if (subscription !== current) return;
        try {
          var event = exactEvent(message.payload);
          if (current.inFlight || event.instanceId !== identity.instanceId || event.documentId !== identity.documentId ||
              event.lifecycleGeneration !== identity.lifecycleGeneration) {
            throw new Error('Duplicate, out-of-order or stale native revision event');
          }
          current.inFlight = true;
          // A host-completed UI write can have an invoke reply queued behind this event.
          // Fence later writes immediately and drain only those already dispatched.
          if (current.localAdvances.size) await Promise.all(Array.from(current.localAdvances));
          if (subscription !== current || ticket !== epoch) return;
          if (event.fromRevision !== current.revision) throw new Error('Out-of-order native revision event');
          // The consumer fences stale reads before its first await and resolves only after exact synchronization.
          var synchronized = exactEvent(await synchronize(event));
          if (subscription !== current || ticket !== epoch) return;
          if (Object.keys(event).some(function (key) { return event[key] !== synchronized[key]; })) throw new Error('Native revision synchronization identity mismatch');
          binding = Object.freeze({ instanceId: identity.instanceId, documentId: identity.documentId, contentRevision: event.toRevision });
          current.revision = event.toRevision;
          current.inFlight = false;
          await invoke('nemo_native_revision_sync', { request: {
            action: 'acknowledge', subscriptionId: identity.subscriptionId, event: event,
          } });
        } catch (error) {
          if (subscription === current) await invalidate();
        }
      }
      try {
        for (var name of ['nemo-native-revision', 'nemo-native-revision-disconnected']) {
          var callback = name === 'nemo-native-revision' ? receive : function (event) {
            if (subscription === current && event.payload.subscriptionId === identity.subscriptionId) return invalidate();
          };
          var stop = await listen(name, callback);
          if (subscription !== current) { stop(); throw new Error('Native subscription was disconnected'); }
          current.stops.push(stop);
        }
        var registered = await invoke('nemo_native_revision_sync', { request: { action: 'subscribe', binding: identity } });
        if (subscription !== current) {
          await invoke('nemo_native_revision_sync', { request: { action: 'disconnect', subscriptionId: identity.subscriptionId } });
          throw new Error('Native subscription was disconnected during registration');
        }
        if (Object.keys(identity).some(function (key) { return registered[key] !== identity[key]; })) throw new Error('Native subscription registration mismatch');
        if (window.addEventListener) window.addEventListener('beforeunload', invalidate);
        return current.identity;
      } catch (error) {
        if (subscription === current) await invalidate();
        throw error;
      }
    }
    function requireBinding(request) {
      if (!binding) throw new Error('Native application transport is disconnected; discover and connect first');
      if (!request || request.apiVersion !== 2 || request.instanceId !== binding.instanceId ||
          request.documentId !== binding.documentId) {
        throw new Error('Native request does not match the connected instance and document');
      }
    }

    return Object.freeze({
      async connect() {
        var disposal = invalidate();
        var ticket = epoch;
        await disposal;
        var status = await invoke('nemo_native_status');
        if (ticket !== epoch) throw new Error('Native connection was superseded');
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
      subscribeRevisions: subscribeRevisions,
      status() { return binding; },
      async dispatch(request) {
        requireBinding(request);
        var ticket = epoch;
        var advances = request.operation.startsWith('command.') || request.operation.startsWith('history.') || request.operation === 'transaction.commit';
        if (subscription && subscription.inFlight && advances) throw new Error('Native revision synchronization is pending');
        var local = subscription, complete, completion;
        if (local && advances) {
          completion = new Promise(function (resolve) { complete = resolve; });
          local.localAdvances.add(completion);
        }
        try {
          var response = await invoke('nemo_native_dispatch', { request: request });
          if (ticket !== epoch || !binding) throw new Error('Native response belongs to a disconnected transport');
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
              contentRevision: Math.max(binding.contentRevision, boundedRevision(response.contentRevision, 'contentRevision')),
            });
            if (subscription && advances && response.ok) subscription.revision = Math.max(subscription.revision, response.contentRevision);
          }
          return response;
        } catch (error) {
          if (ticket === epoch) await invalidate();
          throw error;
        } finally {
          if (complete) { local.localAdvances.delete(completion); complete(); }
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
