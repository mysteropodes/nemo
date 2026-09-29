/* N20 lifecycle-keyed UI and v1/MCP adaptation; owns no document authority. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeOpacityOperations = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  function create(lifecycle, ports, contract, viewportModule) {
    if (!contract) throw new TypeError('native opacity operations require the pure contract');
    if (!viewportModule || typeof viewportModule.create !== 'function') {
      throw new Error('native opacity viewport scheduler is unavailable');
    }
    var v1ByLifecycle = new WeakMap();
    // Extension capabilities outlive a document; opening is one import at a time.
    var extensionExposed = false, installation = null, opening = false;
    var pendingOpen = null, publishedSession = null, admissionStarted = false;
    var viewport = viewportModule.create(lifecycle, ports, function () { return opening; });

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
        return Promise.reject(new Error('keyed native opacity is read-only; editing is unavailable'));
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
        var released = failure('unavailable', 'Native authority does not admit this edit.');
        if (write) retainCompleted(released);
        return released;
      }
      return unavailable();
    }

    function reenterLegacy() { return false; }
    function disposeSession(session) {
      v1ByLifecycle.delete(session);
      viewport.disposeSession(session);
      if (pendingOpen && pendingOpen.session === session) pendingOpen = null;
      if (publishedSession === session) publishedSession = null;
      admissionStarted = false;
    }
    function afterChange() {
      if (opening || pendingOpen) return;
      var current = lifecycle.inspect();
      if (admissionStarted && current.phase === 'native' && current.session !== publishedSession) return;
      if (publishedSession && current.phase === 'native' && current.session === publishedSession) {
        ports.surface.refreshUiProjection(lifecycle.persistenceJSON());
      }
      if (ports.afterChange) ports.afterChange();
    }
    function sameReceipt(a, b) {
      return !!a && !!b && a.owner === 'native' && b.owner === 'native' &&
        a.frame === 0 && b.frame === 0 &&
        a.instanceId === b.instanceId && a.documentId === b.documentId &&
        a.contentRevision === b.contentRevision &&
        a.lifecycleGeneration === b.lifecycleGeneration &&
        a.documentSnapshotId === b.documentSnapshotId;
    }
    function currentOpen(token) {
      var current = lifecycle.inspect();
      return current.phase === 'native' && !current.busy && current.session === token.session &&
        current.generation === token.generation && current.identity &&
        current.identity.instanceId === token.first.instanceId &&
        current.identity.documentId === token.first.documentId &&
        current.identity.contentRevision === token.first.contentRevision;
    }
    async function finishOpenAfterReveal(first) {
      var token = pendingOpen;
      if (!token || token.finishing || first !== token.first || !currentOpen(token)) {
        throw new Error('Native Open is stale or already published');
      }
      token.finishing = true;
      try {
        var visible = await viewport.presentPreview(0);
        if (!sameReceipt(token.first, visible) || visible.status !== 'presented' ||
            !currentOpen(token)) throw new Error('Native viewport is not current after reveal');
        var json = lifecycle.persistenceJSON();
        if (typeof json !== 'string' || !currentOpen(token)) {
          throw new Error('Native document is not ready for UI projection');
        }
        var projection = ports.document.prepareNativeOpacity(json);
        if (projection.opacityMode !== token.opacityMode || projection.layerUid !== token.layerUid ||
            !currentOpen(token)) throw new Error('Native UI projection changed identity');
        var saved = ports.surface.snapshotUiProjection();
        try {
          ports.surface.installUiProjection(projection.shell);
          ports.surface.paintUiProjection();
          if (!currentOpen(token)) throw new Error('Native Open changed during UI projection');
        } catch (error) {
          ports.surface.restoreUiProjection(saved);
          try { ports.surface.paintUiProjection(); } catch (_) {}
          throw error;
        }
        publishedSession = token.session;
        pendingOpen = null;
        return visible;
      } catch (error) {
        if (pendingOpen === token) pendingOpen = null;
        throw error;
      }
    }
    function allowLegacy(kind) {
      try { return ports.surface.allow(kind) === true; }
      catch (_) { return false; }
    }
    async function importJSON(json, silent, allowOccludedAdmission) {
      var candidate;
      try { candidate = ports.document.prepareNativeOpacity(json); }
      catch (_) { return false; }
      if (opening || pendingOpen || !['legacy', 'closed'].includes(lifecycle.inspect().phase)) return false;
      if (extensionExposed || ports.surface.extensionOpen()) {
        ports.surface.toast(extensionExposed
          ? 'Reload or restart Nemo before enabling native opacity after using scripts or plugins.'
          : 'Close every script panel and plugin, then retry native opacity.');
        return false;
      }
      opening = true;
      admissionStarted = true;
      viewport.reset();
      try {
        if (await lifecycle.activate(candidate) !== true) return false;
        var first = await viewport.presentPreview(0, allowOccludedAdmission === true);
        var current = lifecycle.inspect();
        if (!first || first.owner !== 'native' || !current.session ||
            !['presented', 'deferred-occluded'].includes(first.status)) return false;
        pendingOpen = { first: first, session: current.session, generation: current.generation,
          opacityMode: candidate.opacityMode, layerUid: candidate.layerUid, finishing: false };
        return first;
      } catch (error) {
        return false;
      } finally {
        opening = false;
      }
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
        persistenceJSON: lifecycle.persistenceJSON, renderPreview: viewport.renderPreview,
        presentPreview: viewport.presentPreview,
        exportPng: lifecycle.exportPng, releaseCurrent: lifecycle.releaseCurrent,
        historyFromUi: function (action, requestId) { return history(action, requestId, 'ui'); }
      }), Object.freeze({ importJSON: importJSON, finishOpenAfterReveal: finishOpenAfterReveal,
        release: function (kind) { return lifecycle.releaseCurrent(kind || 'document-replacement'); }
      }), Object.freeze({ handle: handleV1, legacy: legacyIntent, meta: function () {
        var current = lifecycle.identity();
        return current ? { instanceId: current.instanceId, documentId: current.documentId,
          revision: current.contentRevision } : null;
      } }), viewport.resizeViewport));
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
      reenterLegacy: reenterLegacy, disposeSession: disposeSession,
      afterChange: afterChange, install: install
    });
  }

  return Object.freeze({ create: create });
}));
