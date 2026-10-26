/* Draw symmetry use case; the existing Labs floating panel and guide stay the only UI.
 * Native owns the six settings. On desktop the original state fields are a presentation
 * projection, rewritten only from a confirmed native read; original writers route here. */
(function (root, factory) {
  var common = typeof module === 'object' && module.exports;
  var api = factory(common ? require('./native-symmetry-desktop.js') : root.NemoNativeSymmetryDesktop);
  if (common) module.exports = api;
  else { root.NemoNativeSymmetryUI = api; api.install(root); }
}(typeof globalThis === 'object' ? globalThis : this, function (Desktop) {
  'use strict';
  var installed = new WeakMap();
  var keys = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  function install(root) {
    if (installed.has(root)) return installed.get(root);
    var retired = false, pending = false, loading = null, refreshNeeded = false, epoch = 0, busyStamp = null, failedRead = null;
    function native() { return !!(root.__TAURI__ && root.__TAURI__.core); }
    function current() { return root.NemoNativeObjectProject && root.NemoNativeObjectProject.desktop(root); }
    function available(session, info) {
      return native() && !retired && session === current() && !!root.NemoNativeObjectProjectFile &&
        root.NemoNativeObjectProjectFile.projectionReadyForUI(root, info) === true;
    }
    var model = Desktop.create({ currentSession: current, available: available, publish: function () { return true; } });
    function notice() {
      if (typeof root.showToast === 'function') root.showToast('Symmetry changes are not confirmed. Wait for synchronization before trying again.');
    }
    function project(value) {
      var e = value && value.symmetry.settings.effective, s = root.state;
      if (!e || !s || retired || model.read() !== value) return;
      s.symmetryEnabled = e.enabled; s.symmetryMode = e.mode;
      s.symmetryAxis = { x1: e.axis.x1, y1: e.axis.y1, x2: e.axis.x2, y2: e.axis.y2 };
      s.symmetryRadialCenter = { x: e.radialCenter.x, y: e.radialCenter.y };
      s.symmetryRadialSectors = e.radialSectors; s.symmetryExtend = e.extend;
    }
    function repaint() {
      if (typeof root.renderLabsFloatPanel === 'function') root.renderLabsFloatPanel();
      if (root.SMEngineBridge && typeof root.SMEngineBridge.renderNow === 'function') root.SMEngineBridge.renderNow();
    }
    function identity() {
      try {
        var session = current(), info = session && session.info(), pin = {};
        if (!session || !session.isActive() || !info) return null;
        keys.forEach(function (key) { pin[key] = info[key]; });
        return { session: session, info: info, pin: pin, epoch: epoch, ticket: session.readSubscriptionGeneration() };
      } catch (_) { return null; }
    }
    function changed(stamp, next) {
      return !stamp || stamp.session !== next.session || stamp.epoch !== next.epoch || stamp.ticket !== next.ticket ||
        keys.some(function (key) { return stamp.pin[key] !== next.pin[key]; });
    }
    function sameReadOwner(stamp) {
      try {
        var next = identity();
        return !retired && !pending && !!next && !changed(stamp, next) && available(next.session, next.info);
      } catch (_) { return false; }
    }
    function readStamp() {
      var stamp = identity(), shown;
      if (!stamp || !available(stamp.session, stamp.info)) return null;
      shown = stamp.session.readPresentedObjectSession();
      var p = shown && shown.presentation;
      if (!p || p.owner !== 'native' || p.status !== 'presented' ||
          !keys.every(function (key) { return shown[key] === stamp.pin[key] && p[key] === stamp.pin[key]; })) return null;
      stamp.presentation = p; return stamp;
    }
    function settled() {
      busyStamp = null;
      var refresh = refreshNeeded; refreshNeeded = false; project(model.read()); repaint();
      if (refresh && !retired) presented();
    }
    function presented() {
      if (!native() || retired) return Promise.resolve(null);
      if (pending || loading) {
        var next = identity();
        if (next && changed(busyStamp, next) && available(next.session, next.info)) refreshNeeded = true;
        return loading || Promise.resolve(null);
      }
      var stamp;
      try { stamp = readStamp(); } catch (_) {}
      if (!stamp) return Promise.resolve(null);
      var value = model.read(); if (value) { project(value); return Promise.resolve(value); }
      if (failedRead && sameReadOwner(failedRead) && failedRead.presentation === stamp.presentation) return Promise.resolve(null);
      failedRead = null; busyStamp = stamp;
      loading = model.refresh().then(function (value) {
        if (sameReadOwner(stamp)) failedRead = null;
        return value;
      }).catch(function () {
        try { if (sameReadOwner(stamp)) failedRead = stamp; } catch (_) {}
        return null;
      }).finally(function () { loading = null; settled(); });
      return loading;
    }
    async function edit(action) {
      var origin = model.read();
      if (retired || pending || loading || !origin) {
        notice(); project(origin); repaint(); if (!origin) presented();
        return { status: 'unavailable' };
      }
      busyStamp = identity(); pending = true;
      try {
        var result = await model.edit(action, origin);
        if (!['applied-presented', 'unchanged'].includes(result.status) || model.read() !== result.projection) notice();
        return result;
      } catch (error) {
        notice(); return { status: 'unconfirmed', code: error && error.code || 'unavailable' };
      } finally {
        pending = false;
        // An unconfirmed edit leaves no cached read; re-read so the guide returns to native truth.
        if (!model.read()) refreshNeeded = true;
        settled();
      }
    }
    // Original writers ask first; true means native owns the gesture on this host.
    function route(action) {
      if (!native() || retired) return false;
      edit(action); return true;
    }
    function dispose() {
      if (retired) return;
      retired = true; refreshNeeded = false; failedRead = null; model.dispose();
      if (typeof root.removeEventListener === 'function') root.removeEventListener('pagehide', dispose);
    }
    var controller = Object.freeze({ read: model.read, edit: edit, route: route, presented: presented, native: native,
      isPending: function () { return pending || !!loading; },
      willProject: function () { epoch++; failedRead = null; if (!pending) model.invalidate(); }, dispose: dispose });
    installed.set(root, controller); root.NemoNativeSymmetryController = controller;
    if (typeof root.addEventListener === 'function') root.addEventListener('pagehide', dispose, { once: true });
    return controller;
  }
  return Object.freeze({ install: install });
}));
