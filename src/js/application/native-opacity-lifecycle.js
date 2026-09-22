/* N20 sole owner of native opacity authority, caches, queues and release. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeOpacityLifecycle = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  function create(ports, contract) {
    if (!contract) throw new TypeError('native opacity lifecycle requires the pure contract');
    var phase = 'legacy', prepared = null, identity = null, generation = 0;
    var serializeResponse = null, evaluations = new Map(), persistence = null;
    var application = null, pending = Promise.resolve(), releasePromise = null, activationPromise = null;
    var cycle = null, sequence = 0, cacheFences = 0;
    var output = Object.freeze({ kind: 'frame', format: 'rgba8', width: 320, height: 180,
      colorInterpretation: 'srgb', alphaMode: 'straight' });

    function id(prefix) { sequence++; return prefix + '-' + sequence; }
    function copyIdentity() { return identity && Object.freeze({ instanceId: identity.instanceId,
      documentId: identity.documentId, contentRevision: identity.contentRevision }); }
    function requestFor(current, operation, payload) { return { apiVersion: 2,
      requestId: id('n20-read'), instanceId: current.instanceId, documentId: current.documentId,
      operation: operation, payload: payload }; }
    function enqueue(work) { var next = pending.then(work); pending = next.catch(function () {}); return next; }
    function failLifecycle(target, error) { if (target) target.failure = target.failure || error;
      if (cycle === target && phase !== 'legacy') phase = 'indeterminate'; return error; }
    function requireConnected(target) {
      var connected = null;
      try { connected = typeof ports.connectionStatus === 'function' ? ports.connectionStatus() : null; }
      catch (error) { throw failLifecycle(target, error); }
      if (!connected || !identity || connected.instanceId !== identity.instanceId ||
          connected.documentId !== identity.documentId) {
        throw failLifecycle(target, new Error('native opacity transport is disconnected'));
      }
      return connected;
    }
    function requireAdmission() {
      if (phase !== 'native' || !identity || !application || !cycle || !cycle.active) {
        throw new Error('native opacity authority is not active');
      }
      requireConnected(cycle);
    }
    function requireAdmitted(target) {
      if (target !== cycle || !target.active || !identity || !application ||
          !['installing', 'native', 'release-requested'].includes(phase)) {
        throw new Error('native opacity lifecycle was superseded');
      }
      requireConnected(target);
    }
    function requireReadable() { requireAdmission(); if (cacheFences || cycle.synchronizing) {
      throw new Error('native opacity synchronization is pending'); } }
    async function buildCaches(current) {
      var at = current.contentRevision;
      var serialized = contract.successful(await application.dispatch(requestFor(current,
        'query.document.serialize', { atRevision: at })), 'native serialize');
      if (serialized.contentRevision !== at || serialized.result.atRevision !== at) throw new Error('native serialize revision mismatch');
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
        if (expected !== null && Math.abs(result.layers[0].value - expected) > 1e-8) throw new Error('native opacity characterization mismatch');
        next.set(frame, result);
      });
      var composed = ports.document.composeNativeOpacity(prepared, serialized, current);
      return { identity: Object.freeze({ instanceId: current.instanceId, documentId: current.documentId,
        contentRevision: current.contentRevision }), serializeResponse: serialized,
        evaluations: next, persistence: JSON.stringify(composed) };
    }
    function publishCaches(next) { identity = next.identity; serializeResponse = next.serializeResponse;
      evaluations = next.evaluations; persistence = next.persistence; }
    function scoped(raw, names) {
      var open = true, target = raw, consumer = {};
      names.forEach(function (name) { consumer[name] = function () {
        if (!open || !target) throw new Error('native opacity lifecycle consumer is disposed');
        return target[name].apply(target, arguments); }; });
      consumer.dispose = function () {
        if (!open) return; open = false;
        if (target && typeof target.dispose === 'function') target.dispose();
        target = null; };
      return Object.freeze(consumer);
    }
    function createConsumers(target) {
      target.preview = scoped(ports.createPreview(), ['register', 'receive']);
      target.exporter = scoped(ports.createExporter(), ['begin', 'status', 'cancel', 'observe', 'plan']);
    }
    function disposeConsumers(target) { if (!target) return;
      if (target.preview) target.preview.dispose(); if (target.exporter) target.exporter.dispose();
      target.preview = null; target.exporter = null; target.active = false; }
    function clearInstalled(target) { disposeConsumers(target);
      if (target && ports.onDisposed) ports.onDisposed(target.session);
      identity = null; application = null; prepared = null; serializeResponse = null;
      evaluations = new Map(); persistence = null; cacheFences = 0;
      if (cycle === target) cycle = null; }
    async function releaseInstalled(current, target) {
      var request = { apiVersion: 2, requestId: id('n20-release'), instanceId: current.instanceId,
        documentId: current.documentId, expectedRevision: current.contentRevision,
        cancelledBeforeDispatch: false };
      return contract.validateRelease(await ports.release(request), current, request, target.hostGeneration);
    }
    function synchronizeRevision(event, target) {
      if (cycle !== target || !target.active || !['installing', 'native'].includes(phase)) {
        return Promise.reject(new Error('native revision callback belongs to a closed lifecycle'));
      }
      try {
        target.observedHostGeneration = contract.validateRevisionEvent(event, identity,
          target.hostGeneration, target.observedHostGeneration);
        if (target.synchronizing) throw new Error('native revision synchronization is already pending');
      } catch (error) { return Promise.reject(failLifecycle(target, error)); }
      target.synchronizing = true; cacheFences++;
      return enqueue(async function () {
        var fenced = true;
        try {
          requireAdmitted(target);
          if (event.fromRevision !== identity.contentRevision) throw new Error('native revision event is out of order');
          var next = await buildCaches({ instanceId: identity.instanceId, documentId: identity.documentId,
            contentRevision: event.toRevision });
          requireAdmitted(target); publishCaches(next);
          target.synchronizing = false; cacheFences--;
          fenced = false;
          if (!cacheFences && !target.synchronizing && ports.afterChange) ports.afterChange();
          return event;
        } catch (error) { throw failLifecycle(target, error); }
        finally { if (fenced) { target.synchronizing = false; cacheFences--; } }
      });
    }
    async function rollbackBootstrap() {
      var target = cycle;
      if (!identity) {
        disposeConsumers(target);
        phase = 'legacy';
        prepared = null;
        cycle = null;
        return;
      }
      try {
        await releaseInstalled(copyIdentity(), target);
        await ports.disconnect();
        clearInstalled(target);
        phase = 'legacy';
      } catch (_) {
        disposeConsumers(target);
        phase = 'indeterminate';
      }
    }
    function activate(nextPrepared) {
      if (phase !== 'legacy') throw new Error('native opacity activation requires legacy ownership');
      prepared = nextPrepared;
      phase = 'installing';
      var work = (async function () {
        try {
          var bootstrap = await ports.bootstrap(prepared);
          identity = Object.freeze({ instanceId: bootstrap.instanceId, documentId: bootstrap.documentId,
            contentRevision: bootstrap.contentRevision });
          generation++;
          var target = cycle = { session: Object.freeze({}), active: true, hostGeneration: null, observedHostGeneration: null,
            synchronizing: false, failure: null, preview: null, exporter: null };
          var binding = await ports.connect();
          contract.validateBootstrap(bootstrap, binding, prepared);
          application = ports.application();
          createConsumers(target);
          var subscribed = await ports.subscribeRevisions(function (event) {
            return synchronizeRevision(event, target);
          });
          target.hostGeneration = contract.validateSubscription(subscribed, binding,
            target.observedHostGeneration);
          await enqueue(async function () {
            requireAdmitted(target);
            var next = await buildCaches(copyIdentity());
            requireAdmitted(target);
            publishCaches(next);
          });
          if (target.failure) throw target.failure;
          phase = 'native';
          if (ports.afterChange) ports.afterChange();
          return true;
        } catch (error) {
          await rollbackBootstrap();
          if (phase === 'indeterminate') throw error;
          return false;
        }
      })();
      activationPromise = work;
      return work.finally(function () { if (activationPromise === work) activationPromise = null; });
    }
    function performMutation(project, mode) {
      try {
        requireAdmission();
        if (mode !== 'ui' && (cacheFences || cycle.synchronizing)) {
          throw new Error('native opacity synchronization is pending');
        }
      } catch (error) { return Promise.reject(error); }
      var target = cycle;
      cacheFences++;
      return enqueue(async function () {
        var fenced = true;
        try {
          requireAdmitted(target);
          var before = copyIdentity();
          var response = await application.dispatch(project(before));
          requireAdmitted(target);
          if (!response || response.instanceId !== before.instanceId || response.documentId !== before.documentId) {
            throw new Error('native mutation response identity mismatch');
          }
          if (mode === 'v1' && response.ok !== true) {
            if (response.contentRevision !== before.contentRevision) {
              throw new Error('rejected native mutation advanced the document');
            }
            if (response.error && ['wrong_instance', 'wrong_document', 'internal'].includes(response.error.code)) {
              throw new Error('native mutation returned an authority-breaking error: ' + response.error.code);
            }
            return response;
          }
          contract.successful(response, 'native mutation');
          var next = await buildCaches({ instanceId: before.instanceId, documentId: before.documentId,
            contentRevision: response.contentRevision });
          requireAdmitted(target);
          publishCaches(next);
          cacheFences--;
          fenced = false;
          if (!cacheFences && !target.synchronizing && ports.afterChange) ports.afterChange();
          return response;
        } catch (error) { throw failLifecycle(target, error); }
        finally { if (fenced) cacheFences--; }
      });
    }
    function validReleaseReason(reason) {
      return contract.plain(reason) && Object.keys(reason).sort().join(',') === 'documentId,generation,kind' &&
        contract.bounded(reason.kind) && reason.documentId === identity.documentId && reason.generation === generation;
    }
    function requestRelease(reason) {
      if (phase === 'release-requested' || phase === 'releasing') {
        if (identity && validReleaseReason(reason) && releasePromise) return releasePromise;
        return Promise.reject(new Error('native release request is stale or malformed'));
      }
      if (phase !== 'native' || !identity || !validReleaseReason(reason)) {
        return Promise.reject(new Error('native release request is stale or malformed'));
      }
      try { requireConnected(cycle); } catch (error) { return Promise.reject(error); }
      var target = cycle, drain = pending;
      phase = 'release-requested';
      releasePromise = (async function () {
        try {
          await drain;
          if (target.failure || cycle !== target || !target.active) {
            throw target.failure || new Error('native lifecycle changed before release');
          }
          requireConnected(target);
          if (cacheFences || target.synchronizing || !persistence) {
            throw new Error('native release cache is not synchronized');
          }
          var current = copyIdentity(), legacyBytes = persistence;
          phase = 'releasing';
          await releaseInstalled(current, target);
          await ports.disconnect();
          clearInstalled(target);
          phase = 'legacy';
          if (!ports.legacyImport(legacyBytes, true)) {
            throw new Error('composed native document could not re-enter legacy');
          }
          var mapped = Object.freeze({ documentId: current.documentId, generation: generation,
            owner: 'legacy', status: 'released' });
          releasePromise = null;
          return mapped;
        } catch (error) {
          disposeConsumers(target);
          phase = 'indeterminate';
          throw error;
        }
      })();
      return releasePromise;
    }
    function releaseFor(kind) {
      requireAdmission();
      return requestRelease({ kind: kind, documentId: identity.documentId, generation: generation });
    }
    function releaseCurrent(kind) {
      if (phase === 'legacy') return Promise.resolve(null);
      if (phase === 'installing' && activationPromise) {
        return activationPromise.then(function (active) { return active ? releaseCurrent(kind) : null; });
      }
      if (phase === 'release-requested' || phase === 'releasing') return releasePromise;
      if (phase !== 'native') return Promise.reject(new Error('native opacity ownership cannot release safely'));
      return releaseFor(kind);
    }
    function getNativeIdentity() {
      if (phase === 'legacy') return null;
      if (['native', 'release-requested', 'releasing'].includes(phase)) {
        requireConnected(cycle);
        return Object.freeze({ documentId: identity.documentId, generation: generation });
      }
      throw new Error('native opacity ownership is not safely observable');
    }
    function valueAtFrame(layerUid, frame) {
      requireReadable();
      if (!evaluations.has(frame)) throw new Error('native opacity evaluation is unavailable');
      var layers = evaluations.get(frame).layers.filter(function (layer) { return layer.layerUid === layerUid; });
      if (layers.length !== 1) throw new Error('native opacity layer identity is unavailable');
      return [layers[0].value];
    }
    function projectSelection(descriptor, frame) {
      requireReadable();
      return ports.selection.projectSelection(evaluations.get(frame), descriptor);
    }
    function renderPreview(frame) {
      if (phase === 'legacy') return false;
      if (phase !== 'native') return true;
      try { requireAdmission(); } catch (_) { return true; }
      if (cacheFences || cycle.synchronizing) return true;
      var target = cycle;
      enqueue(async function () {
        try {
          requireAdmitted(target);
          var snapshot = serializeResponse.result.documentSnapshotId;
          var geometry = prepared.frames[frame].geometryHandle;
          var metadata = { documentSnapshotId: snapshot, documentId: identity.documentId,
            contentRevision: identity.contentRevision, contextId: 'scene-root', frame: frame,
            quality: 'final', outputSpec: output, geometryHandle: geometry };
          var receipt = await ports.previewHost({ apiVersion: 2, instanceId: identity.instanceId,
            documentId: identity.documentId, contentRevision: identity.contentRevision,
            documentSnapshotId: snapshot, contextId: 'scene-root', frame: frame,
            quality: 'final', outputSpec: output, geometryHandle: geometry });
          requireAdmitted(target);
          target.preview.register(metadata, { workId: receipt.workId, viewGeneration: receipt.viewGeneration });
          target.preview.receive(receipt);
        } catch (error) { throw failLifecycle(target, error); }
      });
      return true;
    }
    function exportPng(destination, frames, onProgress) {
      requireReadable();
      var target = cycle;
      cacheFences++;
      return enqueue(async function () {
        try {
          requireAdmitted(target);
          var current = copyIdentity(), outputHandle = id('n20-output');
          await ports.bindOutput({ apiVersion: 2, instanceId: current.instanceId,
            documentId: current.documentId, expectedRevision: current.contentRevision,
            outputHandle: outputHandle, destination: destination });
          var snapshotRequest = requestFor(current, 'query.document.snapshot.acquire',
            { atRevision: current.contentRevision });
          var snapshotResponse = contract.successful(await application.dispatch(snapshotRequest), 'native snapshot');
          var selected = frames.map(function (frame) { return prepared.frames[frame]; });
          var begin = target.exporter.begin(current, snapshotResponse, id('n20-export'), outputHandle, selected);
          var observed = target.exporter.observe(begin,
            contract.successful(await application.dispatch(begin), 'native export begin'));
          while (observed.receipt.status === 'running') {
            if (onProgress) onProgress(Math.round(observed.receipt.progress * selected.length), selected.length);
            await ports.sleep(10);
            var statusRequest = target.exporter.status(current, id('n20-export-status'), observed.receipt.jobId);
            observed = target.exporter.observe(statusRequest,
              contract.successful(await application.dispatch(statusRequest), 'native export status'));
          }
          if (observed.receipt.status !== 'succeeded') throw new Error('native export did not succeed');
          if (onProgress) onProgress(selected.length, selected.length);
          return observed.receipt;
        } catch (error) { throw failLifecycle(target, error); }
        finally { cacheFences--; }
      });
    }
    function inspect() {
      return Object.freeze({ phase: phase, prepared: prepared, identity: copyIdentity(), generation: generation,
        session: cycle && cycle.session, busy: !!(cacheFences || cycle && cycle.synchronizing) });
    }
    function identityValue() {
      if (phase === 'legacy') return null;
      if (phase === 'installing') return copyIdentity();
      if (['native', 'release-requested', 'releasing'].includes(phase)) {
        requireConnected(cycle);
        return copyIdentity();
      }
      throw new Error('native opacity identity is indeterminate');
    }

    return Object.freeze({ activate: activate, requestRelease: requestRelease,
      releaseCurrent: releaseCurrent, releaseFor: releaseFor, getNativeIdentity: getNativeIdentity,
      isActive: function () {
        if (phase !== 'native') return false;
        try { requireAdmission(); return true; } catch (_) { return false; }
      },
      blocksLegacy: function () { return phase !== 'legacy'; }, status: function () { return phase; },
      identity: identityValue, valueAtFrame: valueAtFrame, projectSelection: projectSelection,
      renderPreview: renderPreview, exportPng: exportPng,
      persistenceJSON: function () {
        if (phase !== 'native' || !cycle || cacheFences || cycle.synchronizing) return null;
        try { requireAdmission(); } catch (_) { return null; }
        return persistence;
      },
      prepared: function () { requireReadable(); return prepared; },
      flush: function () { return releasePromise || pending; }, inspect: inspect, id: id,
      performMutation: performMutation });
  }

  return Object.freeze({ create: create });
}));
