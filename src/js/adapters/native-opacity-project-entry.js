/* Native project entry waits for a visible host frame before publishing Open. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeOpacityProjectEntry = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  function surface(root) {
    var marked = '__TAURI__' in root, location = root.location;
    var packaged = false;
    if (location !== undefined) {
      if (!location || typeof location.protocol !== 'string' ||
          typeof location.hostname !== 'string' || !location.hostname) {
        throw new Error('Persistence surface is unknown');
      }
      packaged = location.protocol === 'tauri:';
      if (packaged ? location.hostname !== 'localhost' || location.port !== '' :
          location.protocol !== 'http:' && location.protocol !== 'https:') {
        throw new Error('Persistence surface is unknown');
      }
    } else if (!marked) throw new Error('Persistence surface is unknown');
    if (!marked && !packaged) return null;
    var api = root.__TAURI__;
    if (!api || !api.core || typeof api.core.invoke !== 'function') {
      throw new Error('Desktop persistence API is unavailable');
    }
    return api;
  }
  function identity(native) {
    var value = native.identity();
    if (!value || typeof value.instanceId !== 'string' || !value.instanceId ||
        typeof value.documentId !== 'string' || !value.documentId ||
        !Number.isSafeInteger(value.contentRevision) || value.contentRevision < 0) {
      throw new Error('Native persistence identity is unavailable');
    }
    return { instanceId: value.instanceId, documentId: value.documentId,
      contentRevision: value.contentRevision };
  }
  function desktopJSON(root, native, api) {
    if (!native || typeof native.blocksLegacy !== 'function' ||
        typeof native.isActive !== 'function' || typeof native.identity !== 'function' ||
        typeof native.persistenceJSON !== 'function' ||
        native.blocksLegacy() !== true || native.isActive() !== true) {
      throw new Error('Native desktop persistence is unavailable');
    }
    var before = identity(native), json = native.persistenceJSON(), after = identity(native);
    if (typeof json !== 'string' || !json || before.instanceId !== after.instanceId ||
        before.documentId !== after.documentId || before.contentRevision !== after.contentRevision ||
        root.NemoNativeOpacityCutover !== native || surface(root) !== api ||
        native.blocksLegacy() !== true || native.isActive() !== true) {
      throw new Error('Native desktop persistence is fenced or stale');
    }
    return json;
  }
  function documentJSON(root, saveFrames) {
    var api = surface(root), native = root.NemoNativeOpacityCutover;
    if (api) return desktopJSON(root, native, api);
    if ('NemoNativeOpacityCutover' in root) {
      var blocked = native && typeof native.blocksLegacy === 'function' ? native.blocksLegacy() : null;
      if (blocked === true) return desktopJSON(root, native, api);
      if (blocked !== false) throw new Error('Native persistence ownership is unknown');
    }
    if (saveFrames) saveFrames();
    return root.SM.exportJSON();
  }

  function saveFramesIfLegacy(root, saveFrames) {
    var api = surface(root), native = root.NemoNativeOpacityCutover;
    if (api) { desktopJSON(root, native, api); return; }
    if ('NemoNativeOpacityCutover' in root) {
      var blocked = native && typeof native.blocksLegacy === 'function' ? native.blocksLegacy() : null;
      if (blocked === true) { desktopJSON(root, native, api); return; }
      if (blocked !== false) throw new Error('Native persistence ownership is unknown');
    }
    saveFrames();
  }

  function admitAutosaveBytes(root, bytes) {
    try {
      var api=surface(root),native=root.NemoNativeOpacityCutover;
      if(!api&&!('NemoNativeOpacityProject' in root)) {
        if(!('NemoNativeOpacityCutover' in root))return true;
        if(native&&typeof native.blocksLegacy==='function'&&native.blocksLegacy()===false)
          return root.NemoNativeOpacityCutover===native&&surface(root)===null&&!('NemoNativeOpacityProject' in root);
      }
      return typeof bytes==='string'&&bytes.length>0&&desktopJSON(root,native,api)===bytes;
    }catch(error){return false;}
  }

  function ready(root, receipt, first) {
    if (!root.NemoNativeOpacityProject) return receipt === true;
    if (!receipt || receipt.owner !== 'native' || receipt.frame !== 0 ||
        receipt.status !== 'presented') return false;
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

  function afterRevealBoundary(ports) {
    return new Promise(function (resolve, reject) {
      var settled = false;
      // WebKit may suspend RAF while an embedded native viewport covers its
      // content. The timer only starts the final presentation attempt; it
      // never publishes Open without a strictly presented native receipt.
      var fallback = setTimeout(finish, 200);
      function finish() {
        if (settled) return;
        settled = true;
        clearTimeout(fallback);
        resolve();
      }
      function fail(error) {
        if (settled) return;
        settled = true;
        clearTimeout(fallback);
        reject(error);
      }
      try {
        ports.raf(function () {
          try { ports.raf(finish); } catch (error) { fail(error); }
        });
      } catch (error) { fail(error); }
    });
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
    return afterRevealBoundary(ports).then(function () {
      var project = root.NemoNativeOpacityProject;
      if (!project || typeof project.finishOpenAfterReveal !== 'function') {
        throw new Error('Native Open publication is unavailable');
      }
      return project.finishOpenAfterReveal(first);
    }).then(function () {
      // finishOpenAfterReveal owns the final receipt, identity checks, UI
      // projection and rollback as one terminal publication operation.
      return true;
    }).catch(function (error) { ports.show(); throw error; });
  }

  return Object.freeze({ documentJSON: documentJSON, saveFramesIfLegacy: saveFramesIfLegacy,
    admitAutosaveBytes: admitAutosaveBytes, ready: ready, reveal: reveal });
}));
