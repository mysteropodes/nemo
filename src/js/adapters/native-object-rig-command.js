/* One bounded command round trip for the lane-3 adapters (mesh, widget, rig): validate before dispatch, send through the
 * serialized native session, replay an ambiguous transport failure with the SAME request id (the owner returns the stored
 * receipt or executes once), correlate the receipt, then reconcile. Holds no document state of any kind and has no
 * fallback writer: an absent route, project or declaration is a typed unavailability. */
(function (root, factory) {
  const contract = typeof module === 'object' && module.exports ? require('./native-object-rig-contract.js') : root.NemoNativeObjectRigContract;
  const api = factory(contract);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectRigCommand = api;
}(typeof globalThis === 'object' ? globalThis : this, function (defaultContract) {
  'use strict';
  const MAX_REQUEST_BYTES = 4096;
  function fault(code, message, extra) { const error = new Error(message); error.code = code; return Object.assign(error, extra || {}); }
  function ownerKey(pin) { return JSON.stringify([pin.instanceId, pin.documentId, pin.lifecycleGeneration]); }
  // The serialized dispatch + reconcile route of the native project session. `runResourceCommand` is its neutral name;
  // `runFontResource` is the same function under the name it was first exported with.
  function routeOf(session) {
    if (typeof session.runResourceCommand === 'function') return (action) => session.runResourceCommand(action);
    if (typeof session.runFontResource === 'function') return (action) => session.runFontResource(action);
    return null;
  }
  function create(session, label, contract, limit) {
    contract = contract || defaultContract;
    const maxBytes = Number.isSafeInteger(limit) && limit > 0 ? limit : MAX_REQUEST_BYTES;
    if (!contract) throw new Error('Native object rig contract is unavailable');
    const route = routeOf(session);

    function owner() {
      const pin = session.info();
      if (!session.isActive() || !pin) throw fault('unavailable', 'Native project is unavailable', { reason: 'no_native_project' });
      return pin;
    }
    function refusal(response, request) {
      const error = response.error || {};
      return fault(typeof error.code === 'string' ? error.code : 'unavailable', 'Native ' + label + ' command refused: ' + (error.code || 'unavailable'),
        { reason: error.details && error.details.reason || null, requestId: request.requestId, contentRevision: response.contentRevision });
    }
    async function send(ctx, request, key, finish, sent) {
      try { return await ctx.dispatch(request); } catch (first) {
        // Ambiguous transport outcome. The owner keys its receipt by request id and body: a second send returns the
        // stored response if the first executed, and executes exactly once if it did not.
        sent.second = true;
        try { return await ctx.dispatch(request); } catch (second) {
          let settled = true, after = null;
          try { await ctx.reconcile(); } catch (_) { settled = false; }
          if (settled) after = session.info();
          // The document did not advance past the revision this request was sent against: nothing was published.
          if (after && ownerKey(after) === key && after.contentRevision === request.expectedRevision) {
            throw fault('unavailable', 'Native ' + label + ' command was not delivered', { reason: 'transport_failure', requestId: request.requestId });
          }
          throw fault(label + '_command_indeterminate', 'Native ' + label + ' command outcome is indeterminate',
            { requestId: request.requestId, replay: () => replay(request, key, finish) });
        }
      }
    }
    // A replay answers the STORED receipt, whose revision is the one the original execution published; any edit published
    // since moves the session past it, so a replay reconciles WITHOUT the revision equality (late).
    async function reconcile(ctx, request, revision, late) {
      try { return await (late || revision === undefined ? ctx.reconcile() : ctx.reconcile(revision)); } catch (error) {
        if (error && typeof error.code === 'string') throw error;
        throw fault('unavailable', 'Native ' + label + ' session could not be reconciled: ' + (error && error.message || error), { reason: 'reconcile_failed', requestId: request.requestId });
      }
    }
    async function settle(ctx, request, key, finish, late) {
      const sent = { second: false };
      const response = await send(ctx, request, key, finish, sent);
      late = late || sent.second;
      if (!response || response.apiVersion !== 2 || response.requestId !== request.requestId || typeof response.ok !== 'boolean') {
        throw fault('uncorrelated', 'Native ' + label + ' response is uncorrelated', { requestId: request.requestId });
      }
      if (!response.ok) { await reconcile(ctx, request, response.contentRevision, late); throw refusal(response, request); }
      try { contract.result(request.operation, response.result); contract.correlate(request, response); } catch (bad) {
        try { await ctx.reconcile(); } catch (_) { /* the typed uncorrelated refusal below is the answer */ }
        throw fault('uncorrelated', 'Native ' + label + ' receipt is uncorrelated: ' + bad.message, { requestId: request.requestId });
      }
      await reconcile(ctx, request, response.contentRevision, late);
      const receipt = Object.freeze(Object.assign({ requestId: request.requestId, contentRevision: response.contentRevision }, response.result));
      return finish ? finish(receipt) : receipt;
    }
    function run(action) {
      if (!route) return Promise.reject(fault('unavailable', 'Native resource command route is unavailable', { reason: 'resource_route_unavailable' }));
      let started = false;
      try { owner(); } catch (error) { return Promise.reject(error); }
      let queued;
      try { queued = route((ctx) => { started = true; return action(ctx); }); } catch (error) { return Promise.reject(error); }
      // A rejection before the action ran is the shared route's own refusal (opening, synchronizing, inactive, epoch change): typed busy.
      return Promise.resolve(queued).catch((error) => {
        if (started || (error && typeof error.code === 'string')) throw error;
        throw fault('unavailable', 'Native ' + label + ' command route is busy or unavailable: ' + (error && error.message || error), { reason: 'native_project_busy' });
      });
    }
    // Re-sends the exact request of an indeterminate outcome, and only to the same native project.
    function replay(request, key, finish) {
      return run((ctx) => {
        if (ownerKey(owner()) !== key) throw fault('stale_owner', 'Native project changed before the replay', { requestId: request.requestId });
        return settle(ctx, request, key, finish, true);
      });
    }
    // build(pin) runs at execution, so a queued call reads the current revision and never a value captured at click time.
    function execute(operation, build, finish) {
      return run((ctx) => {
        const pin = owner(), key = ownerKey(pin);
        // The payload is judged before a request id is minted: a locally refused call consumes nothing.
        const payload = build(pin, ctx);
        try { contract.payload(operation, payload); } catch (bad) { throw fault('invalid_request', bad.message, { reason: 'local_validation' }); }
        const request = { apiVersion: 2, requestId: session.requestId(label), instanceId: pin.instanceId, documentId: pin.documentId,
          expectedRevision: pin.contentRevision, operation, payload };
        if (contract.bytes(JSON.stringify(request)) > maxBytes) throw fault('invalid_request', 'Native ' + label + ' request exceeds ' + maxBytes + ' encoded bytes', { reason: 'request_size' });
        return settle(ctx, request, key, finish);
      });
    }
    return Object.freeze({ execute });
  }
  return Object.freeze({ create, fault, MAX_REQUEST_BYTES });
}));
