/* L01-J one bounded command round trip for the native vector adapters (same protocol as native-object-item-command.js, which
 * is bound to the item contract): validate before dispatch, send through the serialized native session, replay an ambiguous
 * transport failure with the SAME request id (the owner returns the stored receipt or executes once), correlate the receipt,
 * then reconcile. Holds no document state of any kind and never touches Paper. */
(function (root, factory) {
  const contract = typeof module === 'object' && module.exports ? require('./native-object-vector-contract.js') : root.NemoNativeObjectVectorContract;
  const api = factory(contract);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectVectorCommand = api;
}(typeof globalThis === 'object' ? globalThis : this, function (contract) {
  'use strict';

  function fault(code, message, extra) { const error = new Error(message); error.code = code; return Object.assign(error, extra || {}); }
  function ownerKey(pin) { return JSON.stringify([pin.instanceId, pin.documentId, pin.lifecycleGeneration]); }

  function create(session, label, operationContract) {
    const activeContract = operationContract || contract;
    if (!activeContract) throw new Error('Native vector contract is unavailable');
    function owner() {
      const pin = session.info();
      if (!session.isActive() || !pin) throw fault('unavailable', 'Native project is unavailable', { reason: 'no_native_project' });
      return pin;
    }
    function refusal(response, request) {
      const error = response.error || {};
      return fault(typeof error.code === 'string' ? error.code : 'unavailable', `Native ${label} command refused: ${error.code || 'unavailable'}`,
        { reason: (error.details && error.details.reason) || null, requestId: request.requestId, contentRevision: response.contentRevision });
    }
    async function send(ctx, request, key, finish, sent) {
      try { return await ctx.dispatch(request); } catch (first) {
        // Ambiguous transport outcome. The owner keys its receipt by request id and body: a second send returns the stored
        // response if the first executed, and executes exactly once if it did not.
        sent.second = true;
        try { return await ctx.dispatch(request); } catch (second) {
          let settled = true, after = null;
          try { await ctx.reconcile(); } catch (_) { settled = false; }
          if (settled) after = session.info();
          // The document did not advance past the revision this request was sent against: nothing was published.
          if (after && ownerKey(after) === key && after.contentRevision === request.expectedRevision) {
            throw fault('unavailable', `Native ${label} command was not delivered`, { reason: 'transport_failure', requestId: request.requestId });
          }
          throw fault(`${label}_command_indeterminate`, `Native ${label} command outcome is indeterminate`,
            { requestId: request.requestId, replay: () => replay(request, key, finish) });
        }
      }
    }
    // A replay answers the STORED receipt, whose revision is the one the original execution published; any edit published since
    // moves the session past it, so a replay reconciles WITHOUT the revision equality (late).
    async function reconcile(ctx, request, revision, late) {
      try { return await (late || revision === undefined ? ctx.reconcile() : ctx.reconcile(revision)); } catch (error) {
        if (error && typeof error.code === 'string') throw error;
        throw fault('unavailable', `Native ${label} session could not be reconciled: ${(error && error.message) || error}`, { reason: 'reconcile_failed', requestId: request.requestId });
      }
    }
    async function settle(ctx, request, key, finish, late) {
      const sent = { second: false }, response = await send(ctx, request, key, finish, sent);
      const stored = late || sent.second;
      if (!response || response.apiVersion !== 2 || response.requestId !== request.requestId || typeof response.ok !== 'boolean') {
        throw fault('uncorrelated', `Native ${label} response is uncorrelated`, { requestId: request.requestId });
      }
      if (!response.ok) { await reconcile(ctx, request, response.contentRevision, stored); throw refusal(response, request); }
      try { activeContract.result(request.operation, response.result); activeContract.correlate(request, response); } catch (bad) {
        try { await ctx.reconcile(); } catch (_) { /* the uncorrelated receipt is the reported fault */ }
        throw fault('uncorrelated', `Native ${label} receipt is uncorrelated: ${bad.message}`, { requestId: request.requestId });
      }
      await reconcile(ctx, request, response.contentRevision, stored);
      const receipt = Object.freeze(Object.assign({ requestId: request.requestId, contentRevision: response.contentRevision }, response.result));
      return finish ? finish(receipt) : receipt;
    }
    function run(action) {
      if (typeof session.runResourceCommand !== 'function') return Promise.reject(fault('unavailable', 'Native resource command route is unavailable', { reason: 'resource_route_unavailable' }));
      let started = false;
      try { owner(); } catch (error) { return Promise.reject(error); }
      let queued;
      try { queued = session.runResourceCommand((ctx) => { started = true; return action(ctx); }); } catch (error) { return Promise.reject(error); }
      // A rejection before the action ran is the shared route's own refusal (opening, synchronizing, inactive, epoch change): typed busy.
      return Promise.resolve(queued).catch((error) => {
        if (started || (error && typeof error.code === 'string')) throw error;
        throw fault('unavailable', `Native ${label} command route is busy or unavailable: ${(error && error.message) || error}`, { reason: 'native_project_busy' });
      });
    }
    function replay(request, key, finish) {
      return run((ctx) => {
        if (ownerKey(owner()) !== key) throw fault('stale_owner', 'Native project changed before the replay', { requestId: request.requestId });
        return settle(ctx, request, key, finish, true);
      });
    }
    // build(pin) runs at execution, so a queued call reads the current revision and snapshot, never values captured at click time.
    // It may throw a fault (for example a stale origin) before anything is sent.
    function execute(operation, build, finish) {
      return run((ctx) => {
        const pin = owner(), key = ownerKey(pin);
        const request = {
          apiVersion: 2, requestId: session.requestId(label), instanceId: pin.instanceId, documentId: pin.documentId,
          expectedRevision: pin.contentRevision, operation, payload: build(pin),
        };
        try { activeContract.payload(operation, request.payload); } catch (bad) { throw fault('invalid_request', bad.message, { reason: 'local_validation' }); }
        const size = activeContract.bytes(JSON.stringify(request)), limit = (typeof activeContract.requestByteLimit === 'function' ? activeContract.requestByteLimit(operation) : 4096);
        if (size > limit) {
          throw fault('request_too_large', `Native ${label} request is ${size} encoded bytes; the public transport bound is ${limit}`,
            { reason: 'request_size', bytes: size, limit });
        }
        return settle(ctx, request, key, finish);
      });
    }
    return Object.freeze({ execute });
  }
  return Object.freeze({ create, fault });
}));
