/* Shadow metadata use case; the existing brush popover remains the only UI. */
(function (root, factory) {
  var common = typeof module === 'object' && module.exports;
  var api = factory(common ? require('./native-shadow-desktop.js') : root.NemoNativeShadowDesktop);
  if (common) module.exports = api;
  else { root.NemoNativeShadowUI = api; api.install(root); }
}(typeof globalThis === 'object' ? globalThis : this, function (Desktop) {
  'use strict';
  var installed = new WeakMap();
  function install(root) {
    if (installed.has(root)) return installed.get(root);
    var retired = false, pending = false, loading = null, refreshNeeded = false, epoch = 0, busyStamp = null;
    var armIntentGeneration = 0, failedRead = null;
    var keys = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
    function native() { return !!(root.__TAURI__ && root.__TAURI__.core); }
    function current() { return root.NemoNativeObjectProject && root.NemoNativeObjectProject.desktop(root); }
    function available(session, info) {
      return native() && !retired && session === current() && !!root.NemoNativeObjectProjectFile &&
        root.NemoNativeObjectProjectFile.projectionReadyForUI(root, info) === true;
    }
    var model = Desktop.create({ currentSession: current, available: available, publish: function () { return true; } });
    function notice() {
      if (typeof root.showToast === 'function') root.showToast('Shadow changes are not confirmed. Wait for synchronization before trying again.');
    }
    function disarm() {
      if (native() && root.state) root.state.shadowMode = false;
      var cb = root.document && root.document.getElementById('p-shadowmode'); if (cb && native()) cb.checked = false;
    }
    function off() {
      // An explicit later user-off revokes only arming, never an in-flight native edit.
      armIntentGeneration++; disarm();
    }
    function repaint() { if (root.SMShadowBrush && typeof root.SMShadowBrush.refresh === 'function') root.SMShadowBrush.refresh(); }
    function cancel() {
      disarm();
      if (root.SMShadowBrush && typeof root.SMShadowBrush.invalidate === 'function') root.SMShadowBrush.invalidate();
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
    function busy() { return retired || pending || !!loading; }
    function shown(origin) { return !busy() && !!origin && model.read() === origin; }
    function settled() {
      busyStamp = null;
      var refresh = refreshNeeded; refreshNeeded = false; repaint();
      if (refresh && !retired) presented();
    }
    async function edit(action, origin) {
      if (!shown(origin)) { disarm(); notice(); return { status: 'unavailable' }; }
      var armIntent = ++armIntentGeneration;
      busyStamp = identity(); pending = true; disarm(); repaint();
      try {
        var result = await model.edit(action, origin);
        if (['applied-presented', 'unchanged'].includes(result.status) && model.read() === result.projection) {
          // Arming is transient. Only the confirmed native metadata command selects a swatch.
          if (armIntent === armIntentGeneration) {
            root.state.shadowMode = true;
            var cb = root.document.getElementById('p-shadowmode'); if (cb) cb.checked = true;
          } else disarm();
        } else { disarm(); notice(); }
        return result;
      } catch (error) {
        disarm(); notice(); return { status: 'unconfirmed', code: error && error.code || 'unavailable' };
      } finally { pending = false; settled(); }
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
      // Busy admission leaves both the sealed model and prior attempt unchanged.
      if (!stamp) return Promise.resolve(null);
      var value = model.read(); if (value) return Promise.resolve(value);
      if (failedRead && sameReadOwner(failedRead) && failedRead.presentation === stamp.presentation) return Promise.resolve(null);
      failedRead = null; busyStamp = stamp;
      loading = model.refresh().then(function (value) {
        if (sameReadOwner(stamp)) failedRead = null;
        return value;
      }).catch(function () {
        try { if (sameReadOwner(stamp)) failedRead = stamp; } catch (_) {}
        disarm(); return null;
      })
        .finally(function () { loading = null; settled(); });
      repaint(); return loading;
    }
    var checkbox = root.document && root.document.getElementById('p-shadowmode');
    function checkboxChange(event) {
      if (!native()) return;
      event.stopImmediatePropagation();
      var requested = checkbox.checked, origin = model.read(); disarm();
      if (!requested) { off(); return; }
      if (!shown(origin) || !root.SMShadowBrush) { notice(); return; }
      var list = root.SMShadowBrush.palette(), selected = list.find(function (s) {
        return s.id === origin.shadow.settings.shadowActiveId;
      }) || list[0];
      if (!selected) { notice(); return; }
      edit({ kind: 'select', ordinal: selected.ordinal }, origin);
    }
    if (checkbox && typeof checkbox.addEventListener === 'function') checkbox.addEventListener('change', checkboxChange, true);
    function dispose() {
      if (retired) return;
      retired = true; armIntentGeneration++; refreshNeeded = false; failedRead = null; cancel(); model.dispose();
      if (checkbox && typeof checkbox.removeEventListener === 'function') checkbox.removeEventListener('change', checkboxChange, true);
      if (typeof root.removeEventListener === 'function') root.removeEventListener('pagehide', dispose);
      repaint();
    }
    var controller = Object.freeze({ read: model.read, edit: edit, off: off, presented: presented,
      isPending: function () { return pending || !!loading; },
      willProject: function () { epoch++; failedRead = null; cancel(); if (!pending) model.invalidate(); repaint(); }, dispose: dispose });
    installed.set(root, controller); root.NemoNativeShadowController = controller;
    if (typeof root.addEventListener === 'function') root.addEventListener('pagehide', dispose, { once: true });
    return controller;
  }
  return Object.freeze({ install: install });
}));
