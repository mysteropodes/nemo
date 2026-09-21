// The opacity command service owns validation, revision and retry boundaries.
// Concrete document/history/render bindings are supplied by bootstrap.
var NemoOpacityApplicationCore = (function () {
  'use strict';
  var READS = ['capabilities', 'snapshot', 'property.get', 'diagnostics.trace'];
  var WRITES = ['property.set', 'property.key.set', 'property.key.remove', 'property.animation.set', 'history.undo', 'history.redo', 'diagnostics.replay'];
  var PROPERTY_WRITES = WRITES.slice(0, 4);
  function clone(value) { return JSON.parse(JSON.stringify(value)); }
  function object(value) { return !!value && typeof value === 'object' && !Array.isArray(value); }

  function create(ports) {
    var identity = { instanceId: ports.newId(), documentId: ports.newId(), revision: 0 };
    var assigned = false, retained = new Map(), trace = [], editing = false;
    var context = ports.context(), frame = ports.state().currentFrame;
    function response(request, ok, value) {
      var result = { apiVersion: 1, requestId: request && request.requestId || '',
        instanceId: identity.instanceId, documentId: identity.documentId, revision: identity.revision, ok: ok };
      result[ok ? 'result' : 'error'] = value;
      return result;
    }
    function fail(request, code, message) { return response(request, false, { code: code, message: message }); }
    function touch() { identity.revision++; }
    function documentChanged() {
      identity.documentId = ports.newId();
      identity.revision = 0;
      retained.clear(); trace.length = 0;
      context = ports.context(); frame = ports.state().currentFrame;
    }
    function setInstanceId(id) {
      if (typeof id !== 'string' || !id) return fail({}, 'invalid_request', 'instanceId must be non-empty.');
      if (assigned && identity.instanceId !== id) return fail({}, 'INSTANCE_ID_CONFLICT', 'Instance identity is already assigned.');
      identity.instanceId = id; assigned = true;
      return response({}, true, { instanceId: id });
    }
    function validate(request) {
      if (!object(request) || request.apiVersion !== 1 || typeof request.requestId !== 'string'
          || !request.requestId || request.requestId.length > 128 || !object(request.payload)
          || !READS.concat(WRITES).includes(request.operation)) return ['invalid_request', 'Invalid application request.'];
      if (request.cancelled === true) return ['cancelled', 'Cancelled before dispatch.'];
      if ((request.instanceId != null && request.instanceId !== identity.instanceId)
          || (request.documentId != null && request.documentId !== identity.documentId)) return ['wrong_document', 'Request targets a different document or instance.'];
      if (WRITES.includes(request.operation) && (request.instanceId !== identity.instanceId
          || request.documentId !== identity.documentId)) return ['wrong_document', 'Writes require current instance and document identity.'];
      return null;
    }
    function fingerprint(request) {
      return JSON.stringify({ apiVersion: request.apiVersion, instanceId: request.instanceId,
        documentId: request.documentId, expectedRevision: request.expectedRevision,
        operation: request.operation, payload: request.payload });
    }
    function remember(request, body, result) {
      if (!WRITES.includes(request.operation)) return;
      retained.set(request.requestId, { body: body, result: clone(result) });
      if (retained.size > 256) retained.delete(retained.keys().next().value);
      trace.push({ request: clone(request), revision: result.revision, ok: result.ok });
      if (trace.length > 32) trace.shift();
    }
    function property(request) {
      var p = request.payload, state = ports.state();
      if (p.property !== 'opacity' || typeof p.layerId !== 'string' || !p.layerId) return { error: 'Only an identified layer opacity is supported.' };
      var matches = state.layers.filter(function (layer) { return layer.layerUid === p.layerId; });
      if (matches.length !== 1) return { error: 'Layer identity is missing or ambiguous.' };
      if (p.frame != null && (!Number.isFinite(p.frame) || p.frame < 0 || p.frame >= state.totalFrames)) return { error: 'Frame is outside the document.' };
      if (['property.key.set', 'property.key.remove'].includes(request.operation) && !Number.isInteger(p.frame)) return { error: 'Key frame must be an integer.' };
      if (['property.set', 'property.key.set'].includes(request.operation)
          && (!Number.isFinite(p.value) || p.value < 0 || p.value > 100)) return { error: 'Opacity must be in the range 0..100.' };
      if (request.operation === 'property.animation.set' && typeof p.animated !== 'boolean') return { error: 'animated must be boolean.' };
      var layer = matches[0], track = layer.motion && layer.motion.opacity;
      if (layer.motion != null && !object(layer.motion) || layer.motionStatic != null && !object(layer.motionStatic)) return { error: 'Opacity storage is malformed.' };
      if (WRITES.includes(request.operation) && (layer.locked || layer.keyLock)) return { error: 'Layer is locked.' };
      if (p.curvePoints != null) return { error: 'Curve editing is not exposed by this capability.' };
      if (track && (!Array.isArray(track.keys) || track.keys.some(function (key) { return !key || !Number.isFinite(key.frame) || !Array.isArray(key.v); }))) return { error: 'Opacity track is malformed.' };
      return { layer: layer, frame: p.frame == null ? state.currentFrame : p.frame };
    }
    function readProperty(layer, frame) {
      return { layerId: layer.layerUid, property: 'opacity', value: ports.valueAtFrame(layer, frame)[0] };
    }
    function capabilitySummary() {
      var registrations = typeof ports.capabilities === 'function' ? ports.capabilities() : [];
      var descriptors = registrations.map(function (entry) { return clone(entry.descriptor); });
      var properties = descriptors.filter(function (descriptor) {
        return descriptor.effects && Array.isArray(descriptor.effects.lifecycle)
          && descriptor.effects.lifecycle.includes('property.get');
      }).map(function (descriptor) {
        var value = descriptor.input && descriptor.input.properties && descriptor.input.properties.value || {};
        var lifecycle = descriptor.effects.lifecycle;
        return { id: descriptor.id, min: value.minimum, max: value.maximum,
          unit: descriptor.units && descriptor.units.value,
          animated: lifecycle.includes('property.animation.set') };
      });
      return { operations: READS.concat(WRITES), properties: properties, descriptors: descriptors,
        retryRetention: 256, traceRetention: 32, documentIdentity: 'open-document-incarnation' };
    }
    function perform(request) {
      var op = request.operation;
      if (op === 'capabilities') return response(request, true, capabilitySummary());
      if (op === 'snapshot') return response(request, true, ports.snapshot());
      if (op === 'diagnostics.trace') return response(request, true, { entries: clone(trace) });
      if (op === 'diagnostics.replay') {
        var recorded = request.payload.request;
        if (!object(recorded) || !object(recorded.payload) || !PROPERTY_WRITES.includes(recorded.operation) || request.requestId.length > 120) return fail(request, 'invalid_request', 'Replay requires one recorded property command.');
        var replay = { apiVersion: 1, requestId: request.requestId + ':replay', instanceId: identity.instanceId,
          documentId: identity.documentId, expectedRevision: identity.revision,
          operation: recorded.operation, payload: clone(recorded.payload) };
        var replayed = handle(replay);
        return replayed.ok ? response(request, true, { replay: replayed }) : fail(request, replayed.error.code, replayed.error.message);
      }
      if (op === 'history.undo' || op === 'history.redo') {
        var previous = identity.revision;
        if (!ports.history[op === 'history.undo' ? 'undo' : 'redo']()) return fail(request, 'history_unavailable', 'No history entry applies in this context.');
        if (identity.revision === previous) touch();
        return response(request, true, { applied: true });
      }
      var target = property(request);
      if (target.error) return fail(request, 'invalid_request', target.error);
      if (op === 'property.get') return response(request, true, readProperty(target.layer, target.frame));
      var before = identity.revision;
      ports.history.checkpoint();
      ports.write(op, target.layer, request.payload);
      if (identity.revision === before) touch();
      ports.afterMutation(target.layer);
      return response(request, true, readProperty(target.layer, target.frame));
    }
    function handle(request) {
      if (ports.context() !== context) documentChanged();
      if (ports.state().currentFrame !== frame) { frame = ports.state().currentFrame; touch(); }
      var invalid = validate(request);
      if (invalid) return fail(request, invalid[0], invalid[1]);
      var body;
      try { body = fingerprint(request); } catch (_) { return fail(request, 'invalid_request', 'Request must be JSON serializable.'); }
      if (body.length > 65536) return fail(request, 'invalid_request', 'Request is too large.');
      var prior = retained.get(request.requestId);
      if (prior) return prior.body === body ? clone(prior.result) : fail(request, 'invalid_request', 'requestId was reused with a changed body.');
      if (WRITES.includes(request.operation) && (!Number.isSafeInteger(request.expectedRevision)
          || request.expectedRevision !== identity.revision)) return fail(request, 'stale_revision', 'Read a current snapshot before writing.');
      if (WRITES.includes(request.operation) && !ports.canMutate()) return fail(request, 'unavailable', 'An interactive gesture owns the document.');
      if (editing) return fail(request, 'unavailable', 'A document edit is already in progress.');
      // Replay re-enters only with a validated property command, never replay/history.
      editing = request.operation !== 'diagnostics.replay';
      var result;
      try { result = perform(request); }
      finally { editing = false; }
      remember(request, body, result);
      return result;
    }
    return { handle: handle, setInstanceId: setInstanceId, documentChanged: documentChanged,
      historyChanged: touch, changed: touch, meta: function () { return Object.assign({}, identity); } };
  }
  // N20 native authority controller. Platform calls and accepted N19 adapters
  // are injected; this owns only atomic authority state and pinned caches.
  function createNative(ports) {
    var phase = 'legacy', prepared = null, identity = null, generation = 0;
    var serializeResponse = null, evaluations = new Map(), persistence = null;
    var application = null, pending = Promise.resolve(), releasePromise = null, activationPromise = null;
    var sequence = 0, mutationCount = 0;
    var output = Object.freeze({ kind: 'frame', format: 'rgba8', width: 320, height: 180,
      colorInterpretation: 'srgb', alphaMode: 'straight' });
    function id(prefix) { sequence++; return prefix + '-' + sequence; }
    function copyIdentity() { return identity && { instanceId: identity.instanceId,
      documentId: identity.documentId, contentRevision: identity.contentRevision }; }
    function request(operation, payload) { return { apiVersion: 2, requestId: id('n20-read'),
      instanceId: identity.instanceId, documentId: identity.documentId, operation: operation, payload: payload }; }
    function enqueue(work) { var next = pending.then(work); pending = next.catch(function () {}); return next; }
    function requireNative() {
      if (phase !== 'native' || !identity || !application) throw new Error('native opacity authority is not active');
    }
    function successful(response, label) {
      if (!response || response.ok !== true) throw new Error(label + ' failed: ' +
        (response && response.error && response.error.message || 'unknown native error'));
      return response;
    }
    function expectedValue(frame, document) {
      if (!document.layers[0].motion) return document.layers[0].motionStatic.opacity[0];
      if (frame === 0) return 20; if (frame === 10) return 50; if (frame === 20) return 80;
      return null;
    }
    async function refresh() {
      var at = identity.contentRevision;
      var serialized = successful(await application.dispatch(request('query.document.serialize', { atRevision: at })), 'native serialize');
      if (serialized.contentRevision !== at || serialized.result.atRevision !== at) throw new Error('native serialize revision mismatch');
      var reads = [];
      for (var frame = 0; frame < prepared.totalFrames; frame++) reads.push(application.dispatch(
        request('query.document.evaluate', { atRevision: at, contextId: 'scene-root', frame: frame })));
      var received = await Promise.all(reads), next = new Map();
      received.forEach(function (response, frame) {
        successful(response, 'native evaluation'); var result = response.result;
        if (response.contentRevision !== at || result.contentRevision !== at || result.frame !== frame ||
            result.layers.length !== 1 || result.layers[0].layerUid !== prepared.layerUid) {
          throw new Error('native evaluation identity mismatch');
        }
        var expected = expectedValue(frame, serialized.result.document);
        if (expected !== null && Math.abs(result.layers[0].value - expected) > 1e-8) {
          throw new Error('native opacity characterization mismatch');
        }
        next.set(frame, result);
      });
      var composed = ports.document.composeNativeOpacity(prepared, serialized, identity);
      serializeResponse = serialized; evaluations = next; persistence = JSON.stringify(composed);
    }
    function validateBootstrap(receipt, binding) {
      var keys = receipt && Object.keys(receipt).sort().join(',');
      if (keys !== 'apiVersion,contentRevision,documentId,instanceId,resourceCount,viewportAvailable' ||
          receipt.apiVersion !== 2 || receipt.instanceId !== binding.instanceId ||
          receipt.documentId !== binding.documentId || receipt.contentRevision !== binding.contentRevision ||
          receipt.contentRevision !== 0 || receipt.resourceCount !== prepared.resources.length ||
          receipt.viewportAvailable !== true) throw new Error('native bootstrap receipt is not the N20 viewport contract');
    }
    function validateRelease(receipt, current) {
      if (!receipt || receipt.apiVersion !== 2 || receipt.instanceId !== current.instanceId ||
          receipt.documentId !== current.documentId || receipt.contentRevision !== current.contentRevision ||
          receipt.status !== 'succeeded' || receipt.authorityRemovalCompleted !== true ||
          receipt.reentryAvailable !== true || receipt.error !== null ||
          !Number.isSafeInteger(receipt.lifecycleGeneration) || receipt.lifecycleGeneration < 1) {
        throw new Error('native release receipt is not a successful terminal receipt');
      }
      return receipt;
    }
    async function releaseInstalled(current) {
      return validateRelease(await ports.release({ apiVersion: 2, requestId: id('n20-release'),
        instanceId: current.instanceId, documentId: current.documentId,
        expectedRevision: current.contentRevision, cancelledBeforeDispatch: false }), current);
    }
    async function rollbackBootstrap() {
      if (!identity) { phase = 'legacy'; prepared = null; return; }
      try { await releaseInstalled(copyIdentity()); ports.disconnect(); phase = 'legacy'; }
      catch (_) { phase = 'indeterminate'; }
      identity = null; application = null; serializeResponse = null; evaluations = new Map(); persistence = null;
      if (phase === 'legacy') prepared = null;
    }
    function activate(nextPrepared) {
      if (phase !== 'legacy') throw new Error('native opacity activation requires legacy ownership');
      prepared = nextPrepared; phase = 'installing';
      var work = (async function () {
        try {
          var bootstrap = await ports.bootstrap(prepared);
          identity = Object.freeze({ instanceId: bootstrap.instanceId, documentId: bootstrap.documentId,
            contentRevision: bootstrap.contentRevision });
          var binding = await ports.connect();
          validateBootstrap(bootstrap, binding); application = ports.application(); await refresh();
          generation++; phase = 'native'; if (ports.afterChange) ports.afterChange(); return true;
        } catch (error) {
          await rollbackBootstrap(); if (phase === 'indeterminate') throw error; return false;
        }
      })();
      activationPromise = work;
      return work.finally(function () { if (activationPromise === work) activationPromise = null; });
    }
    function mutate(project) {
      requireNative(); mutationCount++;
      return enqueue(async function () {
        try {
          requireNative();
          var response = successful(await application.dispatch(project(copyIdentity())), 'native mutation');
          identity = Object.freeze({ instanceId: identity.instanceId, documentId: identity.documentId,
            contentRevision: response.contentRevision });
          await refresh(); if (ports.afterChange) ports.afterChange(); return response;
        } catch (error) { phase = 'indeterminate'; throw error; }
        finally { mutationCount--; }
      });
    }
    function setOpacity(layerUid, value, requestId) {
      requireNative();
      if (prepared && prepared.opacityMode === 'keyed') {
        requestRelease({ kind: 'keyed-opacity-edit', documentId: identity.documentId,
          generation: generation }).catch(function () {});
        return Promise.reject(new Error('keyed native opacity is read-only until release completes'));
      }
      return mutate(function (current) { return ports.editor.setOpacity(current, requestId || id('n20-set'),
        { stableTarget: { layerUid: layerUid }, opacityMode: 'static' }, value); });
    }
    function history(action, requestId) {
      return mutate(function (current) { return ports.editor[action](current, requestId || id('n20-' + action)); });
    }
    function release(reason) {
      if (phase === 'legacy') return Promise.resolve(null);
      if (phase === 'installing' && activationPromise) return activationPromise.then(function () { return release(reason); });
      if (releasePromise) return releasePromise;
      releasePromise = pending.then(async function () {
        requireNative(); phase = 'releasing'; var current = copyIdentity(), legacyBytes = persistence;
        try {
          await releaseInstalled(current); ports.disconnect();
          if (!ports.legacyImport(legacyBytes, true)) throw new Error('composed native document could not re-enter legacy');
          phase = 'legacy'; identity = null; application = null; prepared = null;
          serializeResponse = null; evaluations = new Map(); persistence = null;
          var mapped = Object.freeze({ documentId: current.documentId, generation: generation,
            owner: 'legacy', status: 'released' });
          releasePromise = null; return mapped;
        } catch (error) { phase = 'indeterminate'; throw error; }
      });
      pending = releasePromise.catch(function () {}); return releasePromise;
    }
    function requestRelease(reason) { return release(reason); }
    function getNativeIdentity() {
      if (phase === 'legacy') return null;
      if (phase === 'native' || phase === 'releasing') return { documentId: identity.documentId, generation: generation };
      throw new Error('native opacity ownership is not safely observable');
    }
    function valueAtFrame(layerUid, frame) {
      if (!identity || !evaluations.has(frame)) throw new Error('native opacity evaluation is unavailable');
      var layers = evaluations.get(frame).layers.filter(function (layer) { return layer.layerUid === layerUid; });
      if (layers.length !== 1) throw new Error('native opacity layer identity is unavailable');
      return [layers[0].value];
    }
    function projectSelection(descriptor, frame) { return ports.selection.projectSelection(evaluations.get(frame), descriptor); }
    function renderPreview(frame) {
      if (phase === 'legacy') return false; if (phase !== 'native') return true;
      enqueue(async function () {
        requireNative();
        var snapshot = serializeResponse.result.documentSnapshotId, geometry = prepared.frames[frame].geometryHandle;
        var metadata = { documentSnapshotId: snapshot, documentId: identity.documentId,
          contentRevision: identity.contentRevision, contextId: 'scene-root', frame: frame,
          quality: 'final', outputSpec: output, geometryHandle: geometry };
        var receipt = await ports.previewHost({ apiVersion: 2, instanceId: identity.instanceId,
          documentId: identity.documentId, contentRevision: identity.contentRevision,
          documentSnapshotId: snapshot, contextId: 'scene-root', frame: frame,
          quality: 'final', outputSpec: output, geometryHandle: geometry });
        ports.preview.register(metadata, { workId: receipt.workId,
          viewGeneration: receipt.viewGeneration });
        ports.preview.receive(receipt);
      }).catch(function () { phase = 'indeterminate'; });
      return true;
    }
    function exportPng(destination, frames, onProgress) {
      return enqueue(async function () {
        requireNative(); var current = copyIdentity(), outputHandle = id('n20-output');
        await ports.bindOutput({ apiVersion: 2, instanceId: current.instanceId,
          documentId: current.documentId, expectedRevision: current.contentRevision,
          outputHandle: outputHandle, destination: destination });
        var snapshotRequest = request('query.document.snapshot.acquire', { atRevision: current.contentRevision });
        var snapshotResponse = successful(await application.dispatch(snapshotRequest), 'native snapshot');
        var selected = frames.map(function (frame) { return prepared.frames[frame]; });
        var begin = ports.exporter.begin(current, snapshotResponse, id('n20-export'), outputHandle, selected);
        var observed = ports.exporter.observe(begin, successful(await application.dispatch(begin), 'native export begin'));
        while (observed.receipt.status === 'running') {
          if (onProgress) onProgress(Math.round(observed.receipt.progress * selected.length), selected.length);
          await ports.sleep(10);
          var status = ports.exporter.status(current, id('n20-export-status'), observed.receipt.jobId);
          observed = ports.exporter.observe(status, successful(await application.dispatch(status), 'native export status'));
        }
        if (observed.receipt.status !== 'succeeded') throw new Error('native export did not succeed');
        if (onProgress) onProgress(selected.length, selected.length); return observed.receipt;
      });
    }
    function legacyIntent(kind, holder, values) {
      if (phase === 'legacy') return null; if (phase !== 'native') return false;
      if (kind === 'set' && prepared.opacityMode === 'static') setOpacity(holder.layerUid, values[0]).catch(function () {});
      else requestRelease({ kind: 'opacity-' + kind, documentId: identity.documentId,
        generation: generation }).catch(function () {});
      return false;
    }
    function v1(requestValue) {
      if (phase === 'legacy') return null;
      requestValue = requestValue || {};
      var envelope = identity || { instanceId: requestValue.instanceId || '',
        documentId: requestValue.documentId || '', contentRevision: 0 };
      function response(ok, value) { var out = { apiVersion: 1, requestId: requestValue.requestId || '',
        instanceId: envelope.instanceId, documentId: envelope.documentId, revision: envelope.contentRevision, ok: ok };
        out[ok ? 'result' : 'error'] = value; return out; }
      function unavailable(error) { return response(false, { code: 'unavailable',
        message: error && error.message || 'Native authority does not admit this operation.' }); }
      if (phase !== 'native') return unavailable(new Error('Native opacity ownership is not dispatchable.'));
      var write = ['property.set', 'property.key.set', 'property.key.remove', 'property.animation.set', 'history.undo', 'history.redo'].indexOf(requestValue.operation) >= 0;
      if (requestValue.instanceId && requestValue.instanceId !== identity.instanceId ||
          requestValue.documentId && requestValue.documentId !== identity.documentId ||
          write && requestValue.expectedRevision !== identity.contentRevision) {
        return response(false, { code: 'stale_revision', message: 'Native identity or revision is stale.' });
      }
      var p = requestValue.payload || {}, frame = p.frame == null ? ports.currentFrame() : p.frame;
      if (requestValue.operation === 'property.get') return response(true, { layerId: p.layerId, property: 'opacity', value: valueAtFrame(p.layerId, frame)[0] });
      if (requestValue.operation === 'snapshot') return response(true, { layers: [{ id: prepared.layerUid,
        name: 'R08 rectangle', opacity: valueAtFrame(prepared.layerUid, frame)[0] }], frame: frame, totalFrames: 21 });
      if (requestValue.operation === 'property.set') return setOpacity(p.layerId, p.value, requestValue.requestId).then(function () {
        return response(true, { layerId: p.layerId, property: 'opacity', value: valueAtFrame(p.layerId, frame)[0] }); }, unavailable);
      if (requestValue.operation === 'history.undo' || requestValue.operation === 'history.redo') {
        return history(requestValue.operation.slice(8), requestValue.requestId).then(function (result) {
          return response(true, { applied: result.result.applied }); }, unavailable);
      }
      if (write || requestValue.operation === 'start' || requestValue.operation === 'diagnostics.replay') { requestRelease({ kind: 'mcp-' + requestValue.operation,
        documentId: identity.documentId, generation: generation }).catch(function () {});
        return response(false, { code: 'unavailable', message: 'Native authority must release before this edit.' }); }
      return requestValue.operation === 'capabilities' || requestValue.operation === 'diagnostics.trace' ? null : unavailable();
    }
    return Object.freeze({ activate: activate, release: release, requestRelease: requestRelease,
      getNativeIdentity: getNativeIdentity, isActive: function () { return phase === 'native'; },
      blocksLegacy: function () { return phase !== 'legacy'; }, status: function () { return phase; },
      identity: copyIdentity, valueAtFrame: valueAtFrame, projectSelection: projectSelection,
      setOpacity: setOpacity, history: history, renderPreview: renderPreview, exportPng: exportPng,
      legacyIntent: legacyIntent, handleV1: v1, persistenceJSON: function () { return mutationCount ? null : persistence; },
      flush: function () { return pending; }, prepared: function () { return prepared; } });
  }
  return { create: create, createNative: createNative };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoOpacityApplicationCore;
