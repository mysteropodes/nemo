/* N20 sole owner of native opacity authority, caches, queues and release. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeOpacityLifecycle = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  function create(ports, contract, replacement, exportWorkflow) {
    if (!contract) throw new TypeError('native opacity lifecycle requires the pure contract');
    if (!replacement || typeof replacement.buildCaches !== 'function') {
      throw new TypeError('native opacity lifecycle requires the replacement coordinator');
    }
    if (!exportWorkflow || typeof exportWorkflow.run !== 'function') {
      throw new TypeError('native opacity lifecycle requires the export workflow');
    }
    var phase = 'legacy', prepared = null, identity = null, generation = 0;
    var serializeResponse = null, evaluations = new Map(), persistence = null;
    var application = null, pending = Promise.resolve(), releasePromise = null, activationPromise = null;
    var cycle = null, sequence = 0, cacheFences = 0, terminal = null;
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
    function buildCaches(current) {
      return replacement.buildCaches(ports, contract, application, prepared, current, requestFor);
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
      target.preview = scoped(ports.createPreview(), ['register', 'receive', 'present']);
      target.exporter = scoped(ports.createExporter(), ['begin', 'status', 'cancel', 'observe', 'plan']);
    }
    function disposeConsumers(target) { if (!target) return;
      if (target.preview) target.preview.dispose(); if (target.exporter) target.exporter.dispose();
      target.preview = null; target.exporter = null; target.active = false; }
    function closeInstalled(target) { disposeConsumers(target);
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
      try {
        if (!identity) throw new Error('bootstrap authority removal is unverified');
        await releaseInstalled(copyIdentity(), target);
        await ports.disconnect();
        if (ports.connectionStatus() !== null) throw new Error('native transport remains connected');
        closeInstalled(target);
        phase = 'closed';
      } catch (_) {
        disposeConsumers(target);
        phase = 'indeterminate';
      }
    }
    function activate(nextPrepared) {
      if (!['legacy', 'closed'].includes(phase)) throw new Error('native opacity activation requires verified closed ownership');
      var canonical = nextPrepared && contract.frozen(nextPrepared) && ports.document.prepareNativeOpacity(nextPrepared.shell);
      if (!canonical || JSON.stringify(nextPrepared) !== JSON.stringify(canonical)) {
        throw new Error('native opacity prepared candidate is not immutable canonical admission');
      }
      terminal = null; releasePromise = null;
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
    function replace(nextPrepared) {
      if (phase !== 'native' || cacheFences || !cycle || cycle.synchronizing) {
        return Promise.reject(new Error('native replacement requires an idle active document'));
      }
      var canonical = nextPrepared && contract.frozen(nextPrepared) && ports.document.prepareNativeOpacity(nextPrepared.shell);
      if (!canonical || JSON.stringify(canonical) !== JSON.stringify(nextPrepared)) {
        return Promise.reject(new Error('native replacement candidate is not immutable canonical admission'));
      }
      var old = cycle, before = copyIdentity(), oldHostGeneration = old.hostGeneration;
      phase = 'replacing';
      return enqueue(function () { return replacement.run({ ports: ports, contract: contract,
        before: before, prepared: nextPrepared, oldHostGeneration: oldHostGeneration,
        requestId: id('n20-replace'),
        checkOld: function () { if (old.failure) throw old.failure; requireConnected(old); },
        newCycle: function () { return { session: Object.freeze({}), active: true,
          hostGeneration: null, observedHostGeneration: null, synchronizing: false,
          failure: null, preview: null, exporter: null }; },
        synchronize: synchronizeRevision,
        install: function (next, fresh) {
          identity = Object.freeze({ instanceId: fresh.binding.instanceId, documentId: fresh.binding.documentId,
            contentRevision: fresh.binding.contentRevision });
          prepared = nextPrepared; generation++; cycle = next; next.hostGeneration = fresh.hostGeneration;
          application = ports.application(); createConsumers(next); phase = 'installing';
        },
        buildCaches: function () { return buildCaches(copyIdentity()); },
        finish: function (next, caches) {
          if (next.failure || cycle !== next || ports.connectionStatus().documentId !== identity.documentId) {
            throw next.failure || new Error('native replacement changed during cache preparation');
          }
          publishCaches(caches); phase = 'native';
          disposeConsumers(old); if (ports.onDisposed) ports.onDisposed(old.session);
          if (ports.afterChange) ports.afterChange();
        },
        fail: function (next, error) {
          if (next) disposeConsumers(next);
          return failLifecycle(cycle || old, error);
        }
      }); });
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
    function requestRelease(reason) {
      if (!contract.plain(reason) || Object.keys(reason).sort().join(',') !== 'documentId,generation,kind' ||
          !Object.keys(reason).every(function (key) { return contract.has(Object.getOwnPropertyDescriptor(reason, key), 'value'); }) ||
          !contract.bounded(reason.kind)) return Promise.reject(new Error('native release request is stale or malformed'));
      if (['closed', 'release-requested', 'releasing'].includes(phase)) {
        if (terminal && JSON.stringify([reason.documentId, reason.generation, reason.kind]) === terminal.fingerprint) return releasePromise;
        return Promise.reject(new Error('native release request is stale or malformed'));
      }
      if (phase !== 'native' || !identity || reason.documentId !== identity.documentId || reason.generation !== generation) {
        return Promise.reject(new Error('native release request is stale or malformed'));
      }
      try { requireConnected(cycle); } catch (error) { return Promise.reject(error); }
      var target = cycle, drain = pending;
      terminal = { fingerprint: JSON.stringify([reason.documentId, reason.generation, reason.kind]), reason: contract.clone(reason), receipt: null };
      phase = 'release-requested';
      return releasePromise = (async function () {
        try {
          await drain;
          if (target.failure || cycle !== target || !target.active) {
            throw target.failure || new Error('native lifecycle changed before release');
          }
          requireConnected(target);
          if (cacheFences || target.synchronizing || !persistence) {
            throw new Error('native release cache is not synchronized');
          }
          var current = copyIdentity();
          phase = 'releasing';
          terminal.receipt = contract.clone(await releaseInstalled(current, target));
          await ports.disconnect();
          if (ports.connectionStatus() !== null) throw new Error('native transport remains connected');
          closeInstalled(target);
          phase = 'closed';
          return Object.freeze({ documentId: current.documentId, generation: generation,
            owner: 'none', status: 'closed' });
        } catch (error) {
          disposeConsumers(target);
          phase = 'indeterminate';
          throw error;
        }
      })();
    }
    function releaseCurrent(kind) {
      if (phase === 'legacy') return Promise.resolve(null);
      if (phase === 'installing' && activationPromise) {
        return activationPromise.then(function () { return releaseCurrent(kind); });
      }
      if (['closed', 'release-requested', 'releasing'].includes(phase) && terminal) {
        return requestRelease({ kind: kind, documentId: terminal.reason.documentId, generation: terminal.reason.generation });
      }
      if (phase !== 'native') return Promise.reject(new Error('native opacity ownership cannot release safely'));
      return requestRelease({ kind: kind, documentId: identity.documentId, generation: generation });
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
    function presentPreview(frame) {
      if (phase !== 'native') return Promise.reject(new Error('native opacity authority is not active'));
      var target = cycle;
      function verify() { requireAdmission(); if (target !== cycle || target.synchronizing) throw new Error('native opacity preview lifecycle changed or synchronization is pending'); }
      return enqueue(async function () {
        try {
          verify();
          var current = copyIdentity();
          return await target.preview.present(current.instanceId, generation, {
            documentSnapshotId: serializeResponse.result.documentSnapshotId, documentId: current.documentId,
            contentRevision: current.contentRevision, contextId: 'scene-root', frame: frame,
            quality: 'final', outputSpec: output, geometryHandle: prepared.frames[frame].geometryHandle,
          }, ports.previewHost, verify);
        } catch (error) { if (phase === 'release-requested' && target === cycle) throw error;
          throw failLifecycle(target, error); }
      });
    }
    function renderPreview(frame) {
      if (phase === 'legacy') return false;
      if (phase !== 'native') return true;
      try { requireAdmission(); } catch (_) { return true; }
      if (cacheFences || cycle.synchronizing) return true;
      presentPreview(frame).catch(function () {});
      return true;
    }
    function exportPng(destination, frames, onProgress) {
      requireReadable();
      var target = cycle;
      cacheFences++;
      return enqueue(async function () {
        try {
          requireAdmitted(target);
          var receipt = await exportWorkflow.run(ports, contract, application, target,
            copyIdentity(), prepared, requestFor, id, destination, frames, onProgress);
          requireAdmitted(target);
          return receipt;
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

    return Object.freeze({ activate: activate, replace: replace, requestRelease: requestRelease,
      releaseCurrent: releaseCurrent, getNativeIdentity: getNativeIdentity,
      isActive: function () {
        if (phase !== 'native') return false;
        try { requireAdmission(); return true; } catch (_) { return false; }
      },
      blocksLegacy: function () { return phase !== 'legacy'; }, status: function () { return phase; },
      identity: identityValue, valueAtFrame: valueAtFrame, projectSelection: projectSelection,
      renderPreview: renderPreview, presentPreview: presentPreview, exportPng: exportPng,
      persistenceJSON: function () {
        if (phase !== 'native' || !cycle || cacheFences || cycle.synchronizing) return null;
        try { requireAdmission(); } catch (_) { return null; }
        return persistence;
      },
      prepared: function () { requireReadable(); return prepared; },
      flush: function () { return releasePromise || pending; }, inspect: inspect, id: id,
      performMutation: performMutation,
      fence: function (error) { return failLifecycle(cycle, error); } });
  }

  return Object.freeze({ create: create });
}));
