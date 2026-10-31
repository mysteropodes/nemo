/* L04-N shared intent runner for the component and duplicator adapters. It owns no document, no history and no
 * fallback writer: every intent becomes ONE closed typed request pinned to the presented native revision and is
 * submitted through an injected port; refusal, staleness and absence are typed results, never retries. */
(function (root, factory) {
  const contract = typeof module === 'object' && module.exports ? require('./native-object-component-contract.js') : root.NemoNativeObjectComponentContract;
  const api = factory(contract);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectComponentIntent = api;
}(typeof globalThis === 'object' ? globalThis : this, function (contract) {
  'use strict';

  const MAX_REQUEST_BYTES = 4096;
  const identifier = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;
  const refused = /^Native command refused: ([a-z_]+)/;
  const PORTS = ['pin', 'declared', 'submit', 'requestId'];

  function failure(operation, code, extra) { return Object.freeze(Object.assign({ ok: false, operation, code }, extra)); }
  function safeRevision(value) { return Number.isSafeInteger(value) && value >= 0; }
  function pinOk(pin) {
    return !!pin && typeof pin.instanceId === 'string' && typeof pin.documentId === 'string' && safeRevision(pin.contentRevision);
  }
  // Layer identities of a pin in document order, or null when the pin carries no usable projection.
  function layerIds(pin) {
    return pin && Array.isArray(pin.layers) && pin.layers.every((layer) => layer && typeof layer.layerUid === 'string') ? pin.layers.map((layer) => layer.layerUid) : null;
  }
  // Presentation ids that no longer name a record of the current native projection.
  function stale(wanted, current) {
    const live = new Set(Array.isArray(current) ? current : []);
    return (Array.isArray(wanted) ? wanted : []).filter((value) => !live.has(value));
  }

  // The caller supplies the declared members (optional ones may be absent); `command` is added here, nothing else is forwarded.
  function build(operation, input) {
    const keys = contract.members(operation), optional = contract.optionalMembers(operation);
    if (!input || typeof input !== 'object' || Array.isArray(input)) throw new TypeError('intent input must be a plain object');
    const own = Object.keys(input);
    if (own.some((key) => !keys.includes(key)) || keys.some((key) => !optional.includes(key) && !own.includes(key))) throw new TypeError('intent input has missing or unknown fields');
    const payload = { command: contract.commandOf(operation) };
    keys.forEach((key) => { if (own.includes(key)) payload[key] = input[key]; });
    return payload;
  }

  function createIntentRunner(ports) {
    if (!ports || PORTS.some((name) => typeof ports[name] !== 'function')) throw new TypeError('intent runner requires pin, declared, submit and requestId ports');
    let tail = Promise.resolve();

    async function execute(operation, input, label) {
      if (!contract.admitted(operation)) return failure(operation, 'unavailable', { reason: 'operation_not_declared' });
      let declared = false;
      try { declared = ports.declared(operation) === true; } catch (_) { declared = false; }
      if (!declared) return failure(operation, 'unavailable', { reason: 'native_declaration_absent' });
      let pin = null;
      try { pin = ports.pin(); } catch (_) { pin = null; }
      if (!pinOk(pin)) return failure(operation, 'unavailable', { reason: 'no_presented_native_session' });
      let request;
      try {
        const body = build(operation, input);
        contract.payload(operation, body);
        const requestId = ports.requestId(label);
        if (typeof requestId !== 'string' || !identifier.test(requestId)) throw new TypeError('request id is not a bounded identifier');
        request = {
          apiVersion: 2, requestId, instanceId: pin.instanceId, documentId: pin.documentId,
          operation, expectedRevision: pin.contentRevision, payload: JSON.parse(JSON.stringify(body)),
        };
      } catch (error) { return failure(operation, 'invalid_request', { reason: String(error && error.message || error) }); }
      if (contract.bytes(JSON.stringify(request)) > MAX_REQUEST_BYTES) return failure(operation, 'request_too_large', { limit: MAX_REQUEST_BYTES });
      let response;
      try { response = await ports.submit(request, pin); } catch (error) {
        // The route refused before it ran the action (opening, synchronizing, inactive): nothing was sent.
        if (error && error.notSent === true) return failure(operation, 'unavailable', { reason: error.reason || 'native_project_busy', requestId: request.requestId });
        const match = refused.exec(String(error && error.message || ''));
        // A refused command was classified by the owner; any other rejection cannot say whether it committed.
        if (match) return failure(operation, match[1], { requestId: request.requestId, stale: match[1] === 'stale_revision' });
        return failure(operation, 'transport_failed', { requestId: request.requestId, indeterminate: true, message: String(error && error.message || error) });
      }
      if (!response || response.requestId !== request.requestId || typeof response.ok !== 'boolean') {
        return failure(operation, 'transport_failed', { requestId: request.requestId, indeterminate: true, message: 'uncorrelated native response' });
      }
      if (response.ok && (!response.result || typeof response.result !== 'object' || Array.isArray(response.result))) {
        return failure(operation, 'transport_failed', { requestId: request.requestId, indeterminate: true, message: 'native response carries no result' });
      }
      // The contract judges the receipt again at this edge: owner and document, the exact revision advance, the receipt shape.
      // A refusal committed nothing, so only a success is judged.
      try { if (response.ok) { contract.correlate(request, response); contract.result(operation, response.result); } } catch (bad) {
        return failure(operation, 'transport_failed', { requestId: request.requestId, indeterminate: true, message: 'uncorrelated native receipt: ' + bad.message });
      }
      if (!response.ok) {
        const code = response.error && response.error.code || 'unavailable';
        return failure(operation, code, { requestId: request.requestId, revision: response.contentRevision, stale: code === 'stale_revision', message: response.error && response.error.message });
      }
      let after = null;
      try { after = ports.pin(); } catch (_) { after = null; }
      const outcome = {
        ok: true, operation, requestId: request.requestId, applied: response.result.applied === true,
        revision: response.contentRevision, pin: pinOk(after) ? after : null,
        synchronized: pinOk(after) && after.contentRevision === response.contentRevision,
      };
      if (typeof response.result.layerUid === 'string') outcome.layerUid = response.result.layerUid;
      return Object.freeze(outcome);
    }
    // One pinned mutation at a time. `exclusive(body)` runs `body(exec)` in the runner's turn, so the caller's own reads
    // (stale-id check, default arguments) and the pin read by `exec` all see what the predecessor left behind.
    function exclusive(body) {
      const next = tail.then(() => body((operation, input, label) => execute(operation, input, label || 'component')));
      tail = next.then(() => undefined, () => undefined);
      return next;
    }
    const run = (operation, input, label) => exclusive((exec) => exec(operation, input, label));
    return Object.freeze({ run, exclusive });
  }

  // Maps the presented native Project session to the intent ports. The common resource route of the session
  // (`runResourceCommand`, the renamed `runFontResource`) runs one action with `dispatch` and `reconcile`; while the
  // session offers neither, every intent is typed unavailability and nothing falls back to a legacy writer.
  function routeOf(session) {
    if (!session) return null;
    if (typeof session.runResourceCommand === 'function') return session.runResourceCommand.bind(session);
    if (typeof session.runFontResource === 'function') return session.runFontResource.bind(session);
    return null;
  }
  function portsFromSession(session) {
    const present = () => !!session && typeof session.isActive === 'function' && session.isActive() === true;
    return {
      pin: () => (present() ? session.info() : null),
      declared: (operation) => contract.admitted(operation) && present() && routeOf(session) !== null,
      requestId: (label) => session.requestId(label),
      // #2256: a pinned read goes through the same single resource queue as the commands and reconciles nothing (it changes nothing).
      readDeclared: () => present() && routeOf(session) !== null,
      async read(request) {
        let started = false;
        try {
          return await routeOf(session)(async (ctx) => { started = true; return ctx.dispatch(request); });
        } catch (error) {
          if (!started) throw Object.assign(new Error(String(error && error.message || error)), { notSent: true, reason: 'native_project_busy' });
          throw error;
        }
      },
      async submit(request) {
        let started = false;
        const route = routeOf(session);
        try {
          return await route(async (ctx) => {
            started = true;
            let response;
            try { response = await ctx.dispatch(request); } catch (error) {
              // A transport error cannot establish whether the command committed: resynchronize, then surface it unchanged.
              try { await ctx.reconcile(); } catch (_) { /* the original error is the answer */ }
              throw error;
            }
            if (!response || response.requestId !== request.requestId || typeof response.ok !== 'boolean') return response;
            await (response.ok ? ctx.reconcile(response.contentRevision) : ctx.reconcile());
            return response;
          });
        } catch (error) {
          if (!started) throw Object.assign(new Error(String(error && error.message || error)), { notSent: true, reason: 'native_project_busy' });
          throw error;
        }
      },
    };
  }
  return Object.freeze({ createIntentRunner, portsFromSession, routeOf, layerIds, stale, failure, MAX_REQUEST_BYTES });
}));
