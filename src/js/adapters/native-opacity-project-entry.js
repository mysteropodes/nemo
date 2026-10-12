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

  // Private bridge only. The caller owns synchronous atomic UI metadata ports.
  function createInactiveEntry(root, ports) {
    if (!ports || ['readUi', 'commit', 'remove', 'notice'].some(function (key) {
      return typeof ports[key] !== 'function';
    })) throw new TypeError('Inactive entry ports are unavailable');
    var busy = false, held = null, uncertain = false, cancelled = 0, lastToken = null, lastId = null;
    var identifier = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;
    var uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
    function integer(value, positive) { return Number.isSafeInteger(value) && value >= (positive ? 1 : 0); }
    function exact(value, keys) {
      return value && typeof value === 'object' && !Array.isArray(value) &&
        Object.keys(value).length === keys.length && keys.every(function (key) {
          var descriptor = Object.getOwnPropertyDescriptor(value, key);
          return descriptor && Object.hasOwn(descriptor, 'value');
        });
    }
    function id() {
      var value = root.crypto.randomUUID();
      if (typeof value !== 'string' || !identifier.test(value) || value === lastId) throw new Error('Request identity unavailable');
      lastId = value; return value;
    }
    function result(status) {
      try { ports.notice(status === 'prepared' ? 'Project prepared; switching remains unavailable.' :
        status === 'cancelled' ? 'Inactive project cancelled.' : 'Inactive project admission ' + status + '.'); } catch (_) {}
      return Object.freeze({ status: status });
    }
    function owner() {
      var api = surface(root), native = root.NemoNativeOpacityCutover;
      if (!api || !native || typeof native.getNativeIdentity !== 'function') throw new Error('Native A unavailable');
      var bytes = desktopJSON(root, native, api), current = identity(native), local = native.getNativeIdentity();
      if (!exact(local, ['documentId', 'generation']) || local.documentId !== current.documentId ||
          !integer(local.generation, true)) throw new Error('Local native lifetime unavailable');
      return { api: api, native: native, current: current, local: local, bytes: bytes,
        invoke: api.core.invoke, document: root.SMProjectDocument };
    }
    function ui() {
      var value = ports.readUi();
      if (!exact(value, ['activeTabId', 'name', 'path', 'frame']) ||
          typeof value.activeTabId !== 'string' || !value.activeTabId ||
          typeof value.name !== 'string' || !value.name ||
          value.path !== null && typeof value.path !== 'string' || !integer(value.frame, false)) {
        throw new Error('Active UI lifetime unavailable');
      }
      return Object.freeze({ activeTabId: value.activeTabId, name: value.name, path: value.path, frame: value.frame });
    }
    function sameOwner(a, b, content) {
      return a.api === b.api && a.invoke === b.invoke && a.native === b.native && a.document === b.document &&
        a.current.instanceId === b.current.instanceId && a.current.documentId === b.current.documentId &&
        a.local.generation === b.local.generation && (!content ||
          a.current.contentRevision === b.current.contentRevision && a.bytes === b.bytes);
    }
    function current(attempt) {
      var latest = owner(), display = ui();
      if (attempt.cancel !== cancelled || !sameOwner(attempt.owner, latest, true) ||
          Object.keys(display).some(function (key) { return display[key] !== attempt.ui[key]; })) {
        throw new Error('Inactive admission became stale');
      }
    }
    async function binding(context) {
      var value = await context.api.core.invoke('nemo_native_revision_sync', { request: { action: 'binding' } });
      if (!exact(value, ['instanceId', 'documentId', 'contentRevision', 'lifecycleGeneration', 'subscriptionId']) ||
          value.instanceId !== context.current.instanceId || value.documentId !== context.current.documentId ||
          value.contentRevision !== context.current.contentRevision || !integer(value.lifecycleGeneration, true) ||
          typeof value.subscriptionId !== 'string' || !identifier.test(value.subscriptionId)) {
        throw new Error('Native binding is stale or malformed');
      }
      return value;
    }
    function request(context, generation) {
      return { apiVersion: 2, requestId: id(), instanceId: context.current.instanceId,
        documentId: context.current.documentId, expectedRevision: context.current.contentRevision,
        lifecycleGeneration: generation };
    }
    async function discard(record) {
      try {
        var latest = owner();
        if (!sameOwner(record.owner, latest, false)) return false;
        var active = await binding(latest);
        if (active.lifecycleGeneration !== record.hostGeneration || !sameOwner(latest, owner(), true)) return false;
        var call = request(latest, active.lifecycleGeneration); call.preparationToken = record.token;
        var receipt = await latest.api.core.invoke('nemo_native_cancel_inactive_project', { request: call });
        return exact(receipt, ['apiVersion', 'requestId', 'preparationToken', 'state']) && receipt.apiVersion === 2 &&
          receipt.requestId === call.requestId && receipt.preparationToken === record.token && receipt.state === 'inactive-cancelled';
      } catch (_) { return false; }
    }
    async function open(path, picker) {
      if (busy || held || uncertain) return result('unavailable');
      busy = true;
      var attempt, record = null, sent = false;
      try {
        attempt = { owner: owner(), ui: ui(), cancel: cancelled };
        if (picker) {
          path = await attempt.owner.api.dialog.open({ title: 'Open Project', multiple: false,
            filters: [{ name: 'Nemo Project', extensions: ['json'] }] });
          current(attempt);
          if (!path) return result('cancelled');
          if (Array.isArray(path)) { if (path.length !== 1) throw new Error('One project required'); path = path[0]; }
        }
        if (typeof path !== 'string' || !path || path.length > 4096 || path === attempt.ui.path) throw new Error('Invalid inactive path');
        var bytes = await attempt.owner.api.fs.readTextFile(path); current(attempt);
        if (typeof bytes !== 'string' || new TextEncoder().encode(bytes).length > 4 * 1048576) throw new Error('Project exceeds admission limit');
        var prepared = attempt.owner.document.prepareNativeOpacity(bytes);
        var active = await binding(attempt.owner); current(attempt);
        var call = request(attempt.owner, active.lifecycleGeneration);
        call.projection = prepared.projection; call.resources = prepared.resources;
        sent = true;
        var receipt;
        try { receipt = await attempt.owner.api.core.invoke('nemo_native_prepare_inactive_project', { request: call }); }
        catch (error) {
          if (error && ['invalid_request', 'wrong_instance', 'stale_owner', 'unavailable',
              'preparation_busy', 'request_conflict', 'preparation_cancelled'].includes(error.code)) sent = false;
          throw error;
        }
        // Only a correlated token may ever be cancelled; never guess one from partial data.
        if (receipt && ['apiVersion', 'requestId', 'preparationToken'].every(function (key) {
          var d = Object.getOwnPropertyDescriptor(receipt, key); return d && Object.hasOwn(d, 'value');
        }) && receipt.apiVersion === 2 && receipt.requestId === call.requestId &&
            typeof receipt.preparationToken === 'string' && uuid.test(receipt.preparationToken)) {
          record = { owner: attempt.owner, hostGeneration: active.lifecycleGeneration, token: receipt.preparationToken }; sent = false;
        }
        if (record && record.token === lastToken) { record = null; sent = true; throw new Error('Retired token replayed'); }
        if (!record || !exact(receipt, ['apiVersion', 'requestId', 'preparationToken', 'state', 'format',
            'formatVersion', 'layerCount', 'totalFrames', 'geometryCount']) || receipt.state !== 'inactive-prepared' ||
            receipt.format !== 'nemo.native-opacity-document' || receipt.formatVersion !== 1 ||
            receipt.layerCount !== prepared.projection.layers.length || receipt.totalFrames !== prepared.projection.totalFrames ||
            receipt.geometryCount !== prepared.resources.length || record.token === lastToken) throw new Error('Inactive receipt refused');
        current(attempt);
        var finalBinding = await binding(attempt.owner); current(attempt);
        if (finalBinding.lifecycleGeneration !== record.hostGeneration) throw new Error('Host lifetime changed');
        var meta = Object.freeze({ kind: 'native-inactive', path: path, name: attempt.owner.document.baseName(path),
          preparationToken: record.token, layerCount: receipt.layerCount, totalFrames: receipt.totalFrames,
          geometryCount: receipt.geometryCount });
        // A throw/unknown return can mean an applied metadata effect. Do not retry or undo it.
        uncertain = true; held = record; record.meta = meta;
        var committed = ports.commit(meta);
        current(attempt);
        if (committed !== true && committed !== false) return result('indeterminate');
        uncertain = false;
        if (!committed) { held = null; if (!await discard(record)) { uncertain = true; return result('indeterminate'); } return result('refused'); }
        lastToken = record.token;
        return result('prepared');
      } catch (_) {
        if (uncertain || sent) { uncertain = true; return result('indeterminate'); }
        if (record && !await discard(record)) { uncertain = true; held = record; return result('indeterminate'); }
        return result(attempt && attempt.cancel !== cancelled ? 'cancelled' : 'refused');
      } finally { busy = false; }
    }
    async function close(meta) {
      if (busy || uncertain || !held || held.meta !== meta) return result('unavailable');
      busy = true;
      try {
        var original = owner(), display = ui();
        if (!await discard(held)) { uncertain = true; return result('indeterminate'); }
        if (!sameOwner(original, owner(), true) || JSON.stringify(display) !== JSON.stringify(ui())) {
          uncertain = true; return result('indeterminate');
        }
        uncertain = true;
        var removed = ports.remove(meta);
        if (removed !== true) return result('indeterminate');
        held = null; uncertain = false; return result('cancelled');
      } catch (_) { uncertain = true; return result('indeterminate'); }
      finally { busy = false; }
    }
    return Object.freeze({ openPath: function (path) { return open(path, false); },
      openPicker: function () { return open(null, true); }, close: close,
      cancelPending: function () { if (busy) cancelled++; },
      inspect: function () { return Object.freeze({ busy: busy, retained: !!held, indeterminate: uncertain }); } });
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

  return Object.freeze({ createInactiveEntry: createInactiveEntry, documentJSON: documentJSON, saveFramesIfLegacy: saveFramesIfLegacy,
    admitAutosaveBytes: admitAutosaveBytes, ready: ready, reveal: reveal });
}));
