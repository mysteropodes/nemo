/* Native project entry waits for a visible host frame before publishing Open. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeOpacityProjectEntry = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  function documentJSON(root) {
    var native = root.NemoNativeOpacityCutover;
    if (native && typeof native.blocksLegacy === 'function' && native.blocksLegacy()) {
      var json = typeof native.persistenceJSON === 'function' ? native.persistenceJSON() : null;
      if (typeof json !== 'string') throw new Error('Native document is not ready for persistence');
      return json;
    }
    return root.SM.exportJSON();
  }

  function saveFramesIfLegacy(root, saveFrames) {
    var native = root.NemoNativeOpacityCutover;
    if (!native || typeof native.blocksLegacy !== 'function' || !native.blocksLegacy()) saveFrames();
  }

  function ready(root, receipt, first) {
    if (!root.NemoNativeOpacityProject) return receipt === true;
    if (!receipt || receipt.owner !== 'native' || receipt.frame !== 0 ||
        !(receipt.status === 'presented' || !first && receipt.status === 'deferred-occluded')) return false;
    var native = root.NemoNativeOpacityCutover;
    if (!native || typeof native.isActive !== 'function' || !native.isActive() ||
        typeof native.identity !== 'function') return false;
    var current = native.identity();
    return !!current && (!first || first.instanceId === receipt.instanceId &&
      first.documentId === receipt.documentId && first.contentRevision === receipt.contentRevision &&
      first.lifecycleGeneration === receipt.lifecycleGeneration &&
      first.documentSnapshotId === receipt.documentSnapshotId) &&
      current.instanceId === receipt.instanceId && current.documentId === receipt.documentId &&
      current.contentRevision === receipt.contentRevision && typeof documentJSON(root) === 'string';
  }

  function reveal(root, first, ports) {
    // Native presentation below replaces the legacy repaint. The latter
    // schedules a render from the previous document's UI frame during reveal.
    var nativeOpen = !!root.NemoNativeOpacityProject;
    try { ports.hide(); if (!nativeOpen) ports.repaint(); }
    catch (error) { ports.show(); throw error; }
    if (!nativeOpen) return true;
    // The existing repaint crosses two frames after the start screen hides.
    // Present again after that boundary so a resize cannot clear the first frame.
    return new Promise(function (resolve) {
      ports.raf(function () { ports.raf(resolve); });
    }).then(function () {
      var native = root.NemoNativeOpacityCutover;
      if (!native || typeof native.presentPreview !== 'function') throw new Error('Native presentation is unavailable');
      return native.presentPreview(0);
    }).then(function (visible) {
      if (!ready(root, visible, first)) throw new Error('Native viewport is not current after reveal');
      return true;
    }).catch(function (error) { ports.show(); throw error; });
  }

  return Object.freeze({ documentJSON: documentJSON, saveFramesIfLegacy: saveFramesIfLegacy,
    ready: ready, reveal: reveal });
}));
