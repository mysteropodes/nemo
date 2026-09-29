/* N20H host replacement and fresh-document cache preparation. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeOpacityReplacement = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  async function buildCaches(ports, contract, application, prepared, current, requestFor) {
    var at = current.contentRevision;
    var serialized = contract.successful(await application.dispatch(requestFor(current,
      'query.document.serialize', { atRevision: at })), 'native serialize');
    if (serialized.contentRevision !== at || serialized.result.atRevision !== at) {
      throw new Error('native serialize revision mismatch');
    }
    var reads = [];
    for (var frame = 0; frame < prepared.totalFrames; frame++) reads.push(application.dispatch(
      requestFor(current, 'query.document.evaluate', { atRevision: at, contextId: 'scene-root', frame: frame })));
    var received = await Promise.all(reads), next = new Map();
    received.forEach(function (response, frame) {
      contract.successful(response, 'native evaluation');
      var result = response.result;
      if (response.contentRevision !== at || result.contentRevision !== at || result.frame !== frame ||
          result.layers.length !== 1 || result.layers[0].layerUid !== prepared.layerUid) {
        throw new Error('native evaluation identity mismatch');
      }
      var expected = contract.expectedValue(frame, serialized.result.document);
      if (expected !== null && Math.abs(result.layers[0].value - expected) > 1e-8) {
        throw new Error('native opacity characterization mismatch');
      }
      next.set(frame, result);
    });
    var composed = ports.document.composeNativeOpacity(prepared, serialized, current);
    return { identity: Object.freeze({ instanceId: current.instanceId, documentId: current.documentId,
      contentRevision: current.contentRevision }), serializeResponse: serialized,
      evaluations: next, persistence: JSON.stringify(composed) };
  }

  async function replaceHost(ports, contract, current, prepared, requestId) {
    var request = { apiVersion: 2, requestId: requestId, instanceId: current.instanceId,
      documentId: current.documentId, expectedRevision: current.contentRevision,
      projection: prepared.projection, resources: prepared.resources };
    var receipt = contract.validateReplacement(await ports.replace(request), current, request, prepared);
    return Object.freeze({ request: request, receipt: receipt });
  }

  async function bindFresh(scope, current, receipt, next) {
    var binding = await scope.ports.connect();
    if (binding.instanceId !== current.instanceId || binding.documentId !== receipt.documentId ||
        binding.contentRevision !== 0) throw new Error('replacement transport did not bind the new document');
    scope.install(next, { binding: binding, hostGeneration: null });
    var subscribed = await scope.ports.subscribeRevisions(function (event) {
      return scope.synchronize(event, next);
    });
    var hostGeneration = scope.contract.validateSubscription(subscribed, binding, next.observedHostGeneration);
    if (hostGeneration <= scope.oldHostGeneration) throw new Error('replacement reused an old revision generation');
    return Object.freeze({ binding: binding, hostGeneration: hostGeneration });
  }

  async function run(scope) {
    var next = null;
    try {
      scope.checkOld();
      var result = await replaceHost(scope.ports, scope.contract, scope.before,
        scope.prepared, scope.requestId);
      next = scope.newCycle();
      var fresh = await bindFresh(scope, scope.before, result.receipt, next);
      scope.confirm(next, fresh.hostGeneration);
      var caches = null;
      if (!next.synchronizing) {
        try { caches = await scope.buildCaches(); }
        catch (error) { if (!next.synchronizing) throw error; }
      }
      scope.finish(next, caches);
      return result.receipt;
    } catch (error) {
      throw scope.fail(next, error);
    }
  }

  return Object.freeze({ buildCaches: buildCaches, run: run });
}));
