/* N20 lifecycle-keyed UI and v1/MCP adaptation; owns no document authority. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeOpacityOperations = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  function create(lifecycle, ports, contract) {
    if (!contract) throw new TypeError('native opacity operations require the pure contract');
    var v1ByLifecycle = new WeakMap();
    // Extension capabilities can outlive a document; this latch lasts for the page.
    // All retry/trace and pending resize work, in contrast, belongs to one session.
    var extensionExposed = false, reentryDepth = 0, resize = null, installation = null;

    function ensureActive() {
      if (!lifecycle.isActive()) throw new Error('native opacity authority is not active');
      return lifecycle.inspect();
    }
    function stateFor(key) {
      var state = v1ByLifecycle.get(key);
      if (!state) {
        state = { retained: new Map(), pending: null, trace: [] };
        v1ByLifecycle.set(key, state);
      }
      return state;
    }
    function setOpacity(layerUid, value, requestId, mode) {
      var current = ensureActive();
      if (current.prepared.opacityMode === 'keyed') {
        lifecycle.releaseFor('keyed-opacity-edit').catch(function () {});
        return Promise.reject(new Error('keyed native opacity is read-only until release completes'));
      }
      return lifecycle.performMutation(function (identity) {
        return ports.editor.setOpacity(identity, requestId || lifecycle.id(mode === 'ui' ? 'n20-ui-set' : 'n20-set'),
          { stableTarget: { layerUid: layerUid }, opacityMode: 'static' }, value);
      }, mode);
    }
    function history(action, requestId, mode) {
      ensureActive();
      return lifecycle.performMutation(function (identity) {
        return ports.editor[action](identity, requestId || lifecycle.id('n20-' + (mode === 'ui' ? 'ui-' : '') + action));
      }, mode);
    }
    function legacyIntent(kind, holder, values) {
      var current = lifecycle.inspect();
      if (current.phase === 'legacy') return null;
      if (current.phase !== 'native') return false;
      if (kind === 'set' && current.prepared.opacityMode === 'static') {
        setOpacity(holder.layerUid, values[0], null, 'ui').catch(function () {});
      } else {
        lifecycle.releaseFor('opacity-' + kind).catch(function () {});
      }
      return false;
    }
    function registrations() {
      return typeof ports.capabilities === 'function' ? ports.capabilities() : [];
    }
    function handleV1(requestValue) {
      var observed = lifecycle.inspect();
      if (observed.phase === 'legacy') return null;
      requestValue = requestValue || {};
      function envelope() {
        var latest = lifecycle.inspect().identity;
        return latest || { instanceId: requestValue.instanceId || '',
          documentId: requestValue.documentId || '', contentRevision: 0 };
      }
      function response(ok, value) {
        var current = envelope();
        var out = { apiVersion: 1,
          requestId: typeof requestValue.requestId === 'string' ? requestValue.requestId : '',
          instanceId: current.instanceId, documentId: current.documentId,
          revision: current.contentRevision, ok: ok };
        out[ok ? 'result' : 'error'] = value;
        return out;
      }
      function failure(code, message) { return response(false, { code: code, message: message }); }
      function unavailable(error) {
        return failure('unavailable', error && error.message || 'Native authority does not admit this operation.');
      }
      var operation = requestValue.operation;
      var write = contract.WRITES.includes(operation);
      var listed = registrations();
      if (!contract.plain(requestValue) || requestValue.apiVersion !== 1 ||
          typeof requestValue.requestId !== 'string' || !requestValue.requestId ||
          requestValue.requestId.length > 128 ||
          (!contract.READS.includes(operation) && !write && !contract.registered(operation, listed)) ||
          !contract.plain(requestValue.payload)) {
        return failure('invalid_request', 'Application request is malformed.');
      }
      if (requestValue.cancelled === true) return failure('cancelled', 'Cancelled before dispatch.');
      if (!observed.identity || !observed.session) {
        return unavailable(new Error('Native opacity ownership is not dispatchable.'));
      }
      var suppliedInstance = contract.has(requestValue, 'instanceId');
      var suppliedDocument = contract.has(requestValue, 'documentId');
      if (write && (requestValue.instanceId !== observed.identity.instanceId ||
          requestValue.documentId !== observed.identity.documentId) || !write &&
          (suppliedInstance && requestValue.instanceId != null && requestValue.instanceId !== observed.identity.instanceId ||
          suppliedDocument && requestValue.documentId != null && requestValue.documentId !== observed.identity.documentId)) {
        return failure('wrong_document', 'Request targets a different document or instance.');
      }
      var body;
      try { body = contract.fingerprint(requestValue); }
      catch (_) { return failure('invalid_request', 'Request must be JSON serializable.'); }
      if (body.length > 65536) return failure('invalid_request', 'Request is too large.');
      var state = stateFor(observed.session);
      var prior = write && state.retained.get(requestValue.requestId);
      if (prior) {
        if (prior.body !== body) return failure('invalid_request', 'requestId was reused with a changed body.');
        if (prior.pending) return unavailable(new Error('The original request is still in progress.'));
        return contract.clone(prior.result);
      }
      if (observed.phase !== 'native') {
        return unavailable(new Error('Native opacity ownership is not dispatchable.'));
      }
      if (write && state.pending) return unavailable(new Error('A document edit is already in progress.'));
      if (write && (!Number.isSafeInteger(requestValue.expectedRevision) ||
          requestValue.expectedRevision !== observed.identity.contentRevision)) {
        return failure('stale_revision', 'Read a current snapshot before writing.');
      }
      if (observed.busy) return unavailable(new Error('Native opacity synchronization is pending.'));
      function remember(result) {
        var entry = state.retained.get(requestValue.requestId);
        if (entry) { entry.result = contract.clone(result); entry.pending = false; }
        state.trace.push({ request: contract.clone(requestValue), revision: result.revision, ok: result.ok });
        if (state.trace.length > 32) state.trace.shift();
        return result;
      }
      function reserve(pending) {
        var entry = { body: body, pending: !!pending, result: null };
        state.retained.set(requestValue.requestId, entry);
        if (state.retained.size > 256) {
          var oldest = state.retained.keys().next().value;
          if (oldest !== requestValue.requestId) state.retained.delete(oldest);
        }
        return entry;
      }
      function retainCompleted(result) { reserve(false); return remember(result); }
      function nativeFailure(nativeResponse) {
        var error = nativeResponse && nativeResponse.error ||
          { code: 'unavailable', message: 'Native command was rejected.' };
        var code = error.code;
        if ((operation === 'history.undo' || operation === 'history.redo') &&
            (code === 'not_found' || code === 'unavailable')) code = 'history_unavailable';
        else if (code === 'busy_conflict') code = 'unavailable';
        else if (code === 'cancelled_before_dispatch') code = 'cancelled';
        else if (code === 'wrong_instance') code = 'wrong_document';
        return response(false, { code: code, message: error.message });
      }
      if (!contract.v1PayloadValid(observed.prepared, operation, requestValue.payload) ||
          operation === 'diagnostics.replay' && requestValue.requestId.length > 120) {
        var invalid = failure('invalid_request', 'Operation payload is invalid or targets unsupported state.');
        return write ? retainCompleted(invalid) : invalid;
      }
      if (!lifecycle.isActive()) {
        return unavailable(new Error('Native opacity ownership is not dispatchable.'));
      }
      var payload = requestValue.payload;
      var frame = operation === 'snapshot' || payload.frame == null ? ports.currentFrame() : payload.frame;
      if (operation === 'capabilities') return response(true, contract.capabilitySummary(listed));
      if (operation === 'diagnostics.trace') return response(true, { entries: contract.clone(state.trace) });
      try {
        if (operation === 'property.get') return response(true, { layerId: payload.layerId,
          property: 'opacity', value: lifecycle.valueAtFrame(payload.layerId, frame)[0] });
        if (operation === 'snapshot') return response(true, { layers: [{ id: observed.prepared.layerUid,
          name: 'R08 rectangle', opacity: lifecycle.valueAtFrame(observed.prepared.layerUid, frame)[0] }],
          frame: frame, totalFrames: observed.prepared.totalFrames });
      } catch (error) { return unavailable(error); }
      if (operation === 'property.set' && observed.prepared.opacityMode === 'static' ||
          operation === 'history.undo' || operation === 'history.redo') {
        var entry = reserve(true);
        var internalRequestId = lifecycle.id('n20-v1');
        var work = lifecycle.performMutation(function (current) {
          return operation === 'property.set' ? ports.editor.setOpacity(current, internalRequestId,
            { stableTarget: { layerUid: payload.layerId }, opacityMode: 'static' }, payload.value) :
            ports.editor[operation.slice(8)](current, internalRequestId);
        }, 'v1').then(function (nativeResponse) {
          if (nativeResponse.ok !== true) {
            var transient = ['busy_conflict', 'unavailable', 'cancelled_before_dispatch']
              .includes(nativeResponse.error && nativeResponse.error.code);
            var mappedFailure = nativeFailure(nativeResponse);
            if (transient) { state.retained.delete(requestValue.requestId); return mappedFailure; }
            return remember(mappedFailure);
          }
          var mappedSuccess = operation === 'property.set' ? response(true, {
            layerId: payload.layerId, property: 'opacity',
            value: lifecycle.valueAtFrame(payload.layerId, frame)[0]
          }) : response(true, { applied: nativeResponse.result.applied });
          return remember(mappedSuccess);
        }, function (error) {
          state.retained.delete(requestValue.requestId);
          return unavailable(error);
        }).finally(function () {
          if (state.pending === work) state.pending = null;
        });
        entry.pending = true;
        state.pending = work;
        return work;
      }
      if (write || contract.registered(operation, listed)) {
        var released = failure('unavailable', 'Native authority must release before this edit.');
        if (write) retainCompleted(released);
        lifecycle.releaseFor('mcp-' + operation).catch(function () {});
        return released;
      }
      return unavailable();
    }

    function reenterLegacy(bytes, silent) {
      reentryDepth++;
      try { return ports.legacyImport(bytes, silent); }
      finally { reentryDepth--; }
    }
    function disposeSession(session) {
      v1ByLifecycle.delete(session);
      if (resize && resize.session === session) {
        ports.surface.cancel(resize.timer);
        resize = null;
      }
    }
    function allowLegacy(kind) {
      if (reentryDepth) return true;
      try { return ports.surface.allow(kind) === true; }
      catch (_) { return false; }
    }
    async function importJSON(json, silent) {
      if (lifecycle.blocksLegacy()) await lifecycle.releaseCurrent('document-replacement');
      var candidate;
      try { candidate = ports.document.prepareNativeOpacity(json); }
      catch (_) { return ports.legacyImport(json, silent); }
      if (extensionExposed || ports.surface.extensionOpen()) {
        ports.surface.toast(extensionExposed
          ? 'Reload or restart Nemo before enabling native opacity after using scripts or plugins.'
          : 'Close every script panel and plugin, then retry native opacity.');
        return ports.legacyImport(json, silent);
      }
      var imported = ports.legacyImport(json, silent);
      if (imported && !await lifecycle.activate(candidate)) {
        ports.surface.toast('Native opacity activation failed; this document remains legacy-owned.');
      }
      return imported;
    }
    function resizeViewport() {
      var observed = lifecycle.inspect(), token;
      try { token = lifecycle.getNativeIdentity(); } catch (_) { return; }
      if (!token || !observed.session) return;
      if (resize) ports.surface.cancel(resize.timer);
      var job = { session: observed.session, timer: null };
      resize = job;
      job.timer = ports.surface.defer(async function () {
        if (resize !== job || lifecycle.inspect().session !== job.session) return;
        resize = null;
        var currentToken;
        try { currentToken = lifecycle.getNativeIdentity(); } catch (_) { return; }
        if (!currentToken || currentToken.documentId !== token.documentId ||
            currentToken.generation !== token.generation) return;
        try { await ports.surface.resize(lifecycle.identity()); }
        catch (_) {
          if (lifecycle.inspect().session === job.session) lifecycle.requestRelease({
            kind: 'viewport-resize-failed', documentId: token.documentId,
            generation: token.generation }).catch(function () {});
        }
      }, 50);
    }
    function install() {
      if (installation) return installation;
      var surface = ports.surface, disposers = [];
      surface.installGuard(lifecycle);
      function wrap(target, names, prefix, admitted) {
        disposers.push(surface.wrap(target, names, prefix, allowLegacy, admitted));
      }
      // Bare legacy callbacks still reach the require/throw save/history choke;
      // replace the global entry points as well as the published SM aliases.
      wrap('global', [
        'convertSelectionToObjectDuplicator', 'convertLayerToComponent', 'convertLayersToComponent',
        'splitLayerIntoElements', 'convertComponentToLayer', 'convertLayerToLFSGroup',
        'convertLFSGroupToLayer', 'setElemHidden', 'insertFrame', 'insertKeyframe',
        'insertKeyframeAt', 'insertBlankKeyframe', 'clearKeyframe', 'removeTweenSpan',
        'convertToKeyframes', 'removeFrameSpan'
      ], 'app-');
      wrap('timeline', [
        'setPointType', 'booleanOp', 'generateTweens', 'harmonizeAfterEdit',
        'insertFrame', 'insertKeyframe', 'insertBlankKeyframe', 'removeFrame',
        'clearKeyframe', 'convertToKeyframes', 'removeFrameSpan', 'removeTweenSpan',
        'duplicateSelectedFrames', 'setFps', 'setCanvasSize', 'setCanvasBg',
        'setCanvasClip', 'setSafetyZones', 'setCurve', 'setResamplePts', 'setTweenStep',
        'setWorkArea', 'setTotalFrames', 'toggleFolderLayerCollapsed',
        'addLayer', 'addNullLayer', 'reparentLayersIntoFolder', 'removeLayerFromFolder',
        'addEffectLayer', 'addGuideLayer', 'addJoystickLayer', 'addSliderLayer',
        'addFolderLayer', 'deleteLayer', 'trimToWorkArea', 'setLayerKeyLock',
        'toggleLayerMotionBlur', 'toggleMotionBlurComp', 'setMotionBlurSettings',
        'toggleLayerShy', 'toggleShyMode', 'splitLayerAtPlayhead', 'duplicateLayer',
        'toggleLayerVis', 'toggleLayerLock', 'toggleLayerSolo', 'renameLayer',
        'reorderLayer', 'reorderLayersAtGap', 'reorderLayersBatch',
        'applyStrokeProfile', 'convertActiveLayerToComponent', 'convertComponentToLayer',
        'convertActiveLayerToLFSGroup', 'convertActiveLayerToStrokeFillShadow',
        'convertLFSGroupToLayer', 'propagateLFSFill', 'splitLayerIntoElementsCore',
        'splitLayerIntoElements', 'mergeLayersIntoOne', 'convertSelectionToObjectDuplicator',
        'setSymbolPlayMode', 'setSymbolSpeed', 'setSymbolSingleFrame', 'setSymbolPlacedAt',
        'moveFrames', 'cutFrames', 'pasteFrames', 'deleteSelectedFrames',
        'moveKeyframe', 'shiftLayerFrames', 'deleteSelStrokes', 'duplicateKeyframe',
        'extendExposure', 'flipHorizontal', 'flipVertical', 'enterSymbol', 'exitToScene',
        'closeSymbolTab', 'enterMontageView', 'exitMontageView'
      ], 'timeline-');
      wrap('motion', [
        'toggleAnimated', 'setValue', 'setKeyAtCurrentFrame', 'setKeyAtFrame',
        'removeKeyAtCurrentFrame', 'applyCurveToSelection', 'nudgeSelectedKeys',
        'deleteSelectedKeys', 'pasteKeys', 'setLayerValue', 'setExpressionCode',
        'applyExprCode', 'setExprEnabled', 'setExprGlobals', 'addTextAnimator',
        'removeTextAnimator', 'writeColorToHolder', 'writeElementColorFromPicker',
        'setLayerParent', 'setLayerParentB', 'setLayerFollowPath', 'upsertBlendKeyAt',
        'removeBlendKeyAt', 'upsertParentKeyAt', 'removeParentKeyAt',
        'shiftLayerMotionKeys', 'enableTimeRemap', 'disableTimeRemap',
        'setPathVertexOffset', 'setMeshVertexOffset', 'distributeKeys', 'flipKeys',
        'shiftKeySelection', 'onEaseSegChanged', 'setKeyInterp', 'applyEasyEase'
      ], 'motion-');
      function exposeExtension() { extensionExposed = true; }
      wrap('script', ['run', 'openFile', 'api'], 'script-', exposeExtension);
      wrap('plugin', ['openFile', 'loadArchive', 'loadFiles'], 'plugin-', exposeExtension);
      disposers.push(surface.publish(Object.freeze({ allow: allowLegacy }), Object.freeze({
        blocksLegacy: lifecycle.blocksLegacy, isActive: lifecycle.isActive,
        prepared: lifecycle.prepared, identity: lifecycle.identity, projectSelection: lifecycle.projectSelection,
        persistenceJSON: lifecycle.persistenceJSON, renderPreview: lifecycle.renderPreview,
        exportPng: lifecycle.exportPng, releaseCurrent: lifecycle.releaseCurrent,
        historyFromUi: function (action, requestId) { return history(action, requestId, 'ui'); }
      }), Object.freeze({ importJSON: importJSON,
        release: function (kind) { return lifecycle.releaseCurrent(kind || 'document-replacement'); }
      }), Object.freeze({ handle: handleV1, legacy: legacyIntent, meta: function () {
        var current = lifecycle.identity();
        return current ? { instanceId: current.instanceId, documentId: current.documentId,
          revision: current.contentRevision } : null;
      } }), resizeViewport));
      installation = Object.freeze({ dispose: function () {
        if (lifecycle.blocksLegacy()) throw new Error('native opacity must release before surface disposal');
        disposers.slice().reverse().forEach(function (dispose) { dispose(); });
        installation = null;
      } });
      return installation;
    }

    return Object.freeze({
      setOpacity: function (layerUid, value, requestId) {
        return setOpacity(layerUid, value, requestId, 'direct');
      },
      setOpacityFromUi: function (layerUid, value, requestId) {
        return setOpacity(layerUid, value, requestId, 'ui');
      },
      history: function (action, requestId) { return history(action, requestId, 'direct'); },
      historyFromUi: function (action, requestId) { return history(action, requestId, 'ui'); },
      legacyIntent: legacyIntent, handleV1: handleV1,
      reenterLegacy: reenterLegacy, disposeSession: disposeSession, install: install
    });
  }

  return Object.freeze({ create: create });
}));
