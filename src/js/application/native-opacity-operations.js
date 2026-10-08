/* N20 lifecycle-keyed UI transitions; owns no document authority. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeOpacityOperations = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  function create(lifecycle, ports, contract, viewportModule, v1Module) {
    if (!contract) throw new TypeError('native opacity operations require the pure contract');
    if (!viewportModule || typeof viewportModule.create !== 'function') {
      throw new Error('native opacity viewport scheduler is unavailable');
    }
    if (!v1Module || typeof v1Module.create !== 'function') {
      throw new Error('native opacity v1 compatibility module is unavailable');
    }
    // Extension capabilities outlive a document; opening is one import at a time.
    var extensionExposed = false, installation = null, opening = false;
    var pendingOpen = null, publishedSession = null, admissionStarted = false;
    var viewport = viewportModule.create(lifecycle, ports,
      function () { return opening; },
      function (session) { return publishedSession === session; });
    var v1 = v1Module.create(lifecycle, ports, contract);

    function ensureActive() {
      if (!lifecycle.isActive()) throw new Error('native opacity authority is not active');
      return lifecycle.inspect();
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
    var handleV1 = v1.handle;

    function reenterLegacy() { return false; }
    function disposeSession(session) {
      v1.disposeSession(session);
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
        var visible = await viewport.presentPreview(0, false, true);
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
        if (token.replacing) {
          lifecycle.fence(error);
          ports.surface.blockPublication();
        }
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
      var wasNative = lifecycle.inspect().phase === 'native';
      if (opening || pendingOpen || !['legacy', 'closed', 'native'].includes(lifecycle.inspect().phase)) return false;
      if (extensionExposed || ports.surface.extensionOpen()) {
        ports.surface.toast(extensionExposed
          ? 'Reload or restart Nemo before enabling native opacity after using scripts or plugins.'
          : 'Close every script panel and plugin, then retry native opacity.');
        return false;
      }
      opening = true;
      admissionStarted = true;
      try {
        var admitted = wasNative ? await lifecycle.replace(candidate) : await lifecycle.activate(candidate);
        if (!admitted) return false;
        viewport.reset();
        var first = await viewport.presentPreview(0, allowOccludedAdmission === true);
        var current = lifecycle.inspect();
        if (!first || first.owner !== 'native' || !current.session ||
            !['presented', 'deferred-occluded'].includes(first.status)) {
          if (wasNative) {
            lifecycle.fence(new Error('replacement first frame was not presented'));
            ports.surface.blockPublication();
          }
          return false;
        }
        pendingOpen = { first: first, session: current.session, generation: current.generation,
          opacityMode: candidate.opacityMode, layerUid: candidate.layerUid, replacing: wasNative, finishing: false };
        return first;
      } catch (error) {
        if (lifecycle.inspect().phase === 'indeterminate') ports.surface.blockPublication();
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
        getNativeIdentity: lifecycle.getNativeIdentity,
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
