/* Draw symmetry use case; the existing Labs floating panel and guide stay the only UI.
 * Native owns the six settings. On desktop the original state fields are presentation:
 * an original gesture applies its local effect immediately (as the baseline did) and queues
 * one native edit; confirmed native values replace the fields once the queue drains, and any
 * refusal restores the last confirmed values before re-reading. */
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
    var retired = false, loading = null, refreshNeeded = false, epoch = 0, busyStamp = null, failedRead = null;
    var queue = [], draining = null, held = 0, confirmed = null;
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
    function write(e) {
      var s = root.state; if (!e || !s || retired) return;
      s.symmetryEnabled = e.enabled; s.symmetryMode = e.mode;
      s.symmetryAxis = { x1: e.axis.x1, y1: e.axis.y1, x2: e.axis.x2, y2: e.axis.y2 };
      s.symmetryRadialCenter = { x: e.radialCenter.x, y: e.radialCenter.y };
      s.symmetryRadialSectors = e.radialSectors; s.symmetryExtend = e.extend;
    }
    function repaint() {
      if (typeof root.renderLabsFloatPanel === 'function') root.renderLabsFloatPanel();
      if (root.SMEngineBridge && typeof root.SMEngineBridge.renderNow === 'function') root.SMEngineBridge.renderNow();
    }
    // Pending local intent stays on screen until its native edit settles; a gesture in progress is never overwritten.
    function quiet() { return !held && !queue.length && !draining; }
    // The fallback is the last confirmed read of THIS document lifetime, never another document's.
    function owner(info) { return info ? [info.instanceId, info.documentId, info.lifecycleGeneration].join('|') : null; }
    // Whether the guide fields already show these confirmed values.
    function shown(e) {
      var s = root.state, a = s && s.symmetryAxis, c = s && s.symmetryRadialCenter;
      return !!s && s.symmetryEnabled === e.enabled && s.symmetryMode === e.mode && !!a && a.x1 === e.axis.x1 && a.y1 === e.axis.y1 &&
        a.x2 === e.axis.x2 && a.y2 === e.axis.y2 && !!c && c.x === e.radialCenter.x && c.y === e.radialCenter.y &&
        s.symmetryRadialSectors === e.radialSectors && s.symmetryExtend === e.extend;
    }
    // Every native presentation calls presented(), and a repaint asks native for another preview: values already on
    // screen are not repainted, or presentation never settles (#2343 B1). Initial, changed and restored values are.
    function project() {
      var value = model.read(), session = current(), now = owner(session && session.info());
      if (value) confirmed = { owner: owner(value.info), effective: value.symmetry.settings.effective };
      var e = confirmed && confirmed.owner === now ? confirmed.effective : null;
      if (e && quiet() && !shown(e)) { write(e); repaint(); }
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
        return !retired && !draining && !!next && !changed(stamp, next) && available(next.session, next.info);
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
      var refresh = refreshNeeded; refreshNeeded = false; project();
      if (refresh && !retired) presented();
    }
    function presented() {
      if (!native() || retired) return Promise.resolve(null);
      if (draining || loading) {
        var next = identity();
        if (next && changed(busyStamp, next) && available(next.session, next.info)) refreshNeeded = true;
        return loading || Promise.resolve(null);
      }
      var value = model.read(); if (value) { project(); return Promise.resolve(value); }
      return load();
    }
    // Start or join one pinned read; the queue also uses it while it is draining.
    function load() {
      if (loading) return loading;
      var stamp;
      try { stamp = readStamp(); } catch (_) {}
      if (!stamp || retired) return Promise.resolve(null);
      var value = model.read(); if (value) return Promise.resolve(value);
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
    // Later queued intents were built on the refused one; settling then shows the last confirmed values.
    function refuse() { queue = []; notice(); refreshNeeded = true; }
    async function run() {
      while (queue.length && !retired) {
        var action = queue.shift();
        for (var attempt = 0; ; attempt++) {
          var origin = model.read();
          if (!origin) { await load(); origin = model.read(); }
          if (!origin || retired) { refuse(); return; }
          busyStamp = identity();
          var result;
          try { result = await model.edit(action, origin); } catch (_) { result = { status: 'unconfirmed' }; }
          if (['applied-presented', 'unchanged'].includes(result.status) && model.read() === result.projection) break;
          // Native applied nothing (another writer moved the revision, say): re-read once and retry on the new one.
          if (result.status === 'unconfirmed' && result.code === 'stale_revision' && attempt === 0) { model.invalidate(); continue; }
          refuse(); return;
        }
      }
    }
    function drain() {
      if (!draining) draining = run().finally(function () { draining = null; settled(); });
      return draining;
    }
    // Original writers call this after their local effect; true means native owns the gesture on this host.
    // Every action is an absolute setting, so a queued, not yet started action of the same kind is replaced.
    function route(action) {
      if (!native() || retired) return false;
      var last = queue[queue.length - 1];
      if (last && last.kind === action.kind) queue[queue.length - 1] = action; else queue.push(action);
      failedRead = null; drain(); return true;
    }
    function hold() { if (native() && !retired) held++; }
    function release() { if (held > 0 && --held === 0 && quiet()) project(); }
    function dispose() {
      if (retired) return;
      retired = true; queue = []; refreshNeeded = false; failedRead = null; model.dispose();
      if (typeof root.removeEventListener === 'function') root.removeEventListener('pagehide', dispose);
    }
    var controller = Object.freeze({ read: model.read, route: route, hold: hold, release: release, presented: presented, native: native,
      whenSettled: function () { return draining || loading || Promise.resolve(null); },
      isPending: function () { return !!draining || !!loading || queue.length > 0; },
      willProject: function () { epoch++; failedRead = null; if (!draining) model.invalidate(); }, dispose: dispose });
    installed.set(root, controller); root.NemoNativeSymmetryController = controller;
    if (typeof root.addEventListener === 'function') root.addEventListener('pagehide', dispose, { once: true });
    return controller;
  }
  return Object.freeze({ install: install });
}));
