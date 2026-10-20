/* Ordinary guide management consumes native readback; it owns only DOM and gestures. */
(function (root, factory) {
  var common = typeof module === 'object' && module.exports;
  var api = factory(common ? require('./native-guide-desktop.js') : root.NemoNativeGuideDesktop);
  if (common) module.exports = api;
  else { root.NemoNativeGuideUI = api; api.install(root); }
}(typeof globalThis === 'object' ? globalThis : this, function (Desktop) {
  'use strict';
  var installed = new WeakMap();
  function install(root) {
    if (installed.has(root)) return installed.get(root);
    var retired = false, loading = null, pending = false, gesture = null;
    var projectionEpoch = 0, refreshNeeded = false, busyStamp = null;
    var identityKeys = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
    var document = root.document;
    function native() { return !!(root.__TAURI__ && root.__TAURI__.core); }
    function current() { return root.NemoNativeObjectProject && root.NemoNativeObjectProject.desktop(root); }
    function available(session, info) {
      return native() && !retired && session === current() && !!root.NemoNativeObjectProjectFile &&
        root.NemoNativeObjectProjectFile.projectionReadyForUI(root, info) === true;
    }
    var model = Desktop.create({ currentSession: current, available: available, publish: function () { return true; } });
    function notice(message) {
      if (typeof root.showToast === 'function') root.showToast(message || 'Guide changes are not confirmed. Wait for synchronization before trying again.');
    }
    function busy() { return retired || pending || !!loading; }
    function identity() {
      try {
        var session = current(), info = session && session.info(), pin = {};
        if (!session || !info || !session.isActive()) return null;
        identityKeys.forEach(function (key) { pin[key] = info[key]; });
        return { session: session, info: info, pin: pin, epoch: projectionEpoch, ticket: session.readSubscriptionGeneration() };
      } catch (_) { return null; }
    }
    function changed(stamp, next) {
      return !stamp || stamp.session !== next.session || stamp.epoch !== next.epoch || stamp.ticket !== next.ticket ||
        identityKeys.some(function (key) { return stamp.pin[key] !== next.pin[key]; });
    }
    function settled() {
      busyStamp = null;
      var refresh = refreshNeeded; refreshNeeded = false; repaint();
      if (refresh && !retired && !model.read()) presented();
    }
    function shown(origin) { return !busy() && !!origin && model.read() === origin; }
    function repaint() { if (root.SMRulers) root.SMRulers.render(); }
    function cancelGesture() { if (gesture) { var old = gesture; gesture = null; old.cancel(); } }
    async function edit(action, origin) {
      if (!shown(origin)) { notice(); return { status: 'unavailable' }; }
      cancelGesture(); busyStamp = identity(); pending = true; repaint();
      try {
        var result = await model.edit(action, origin);
        if (!['applied-presented', 'unchanged'].includes(result.status)) notice();
        return result;
      } catch (error) { notice(); return { status: 'unconfirmed', code: error && error.code || 'unavailable' }; }
      finally { pending = false; settled(); }
    }
    function beginDrag(event, axis, ordinal, container) {
      var origin = model.read(), area = document.getElementById('canvas-area');
      if (!shown(origin) || gesture || !area || !container || !['h', 'v'].includes(axis) ||
          ordinal !== null && (!Number.isSafeInteger(ordinal) || ordinal < 1 || ordinal > origin.guides[axis].length)) {
        notice(); return false;
      }
      var pointer = event.pointerId, ghost = document.createElement('div');
      ghost.className = 'guide-line ' + axis; ghost.style.opacity = '.6'; ghost.style.pointerEvents = 'none'; container.appendChild(ghost);
      function cleanup() {
        document.removeEventListener('pointermove', move, true); document.removeEventListener('pointerup', up, true);
        document.removeEventListener('pointercancel', cancel, true); document.removeEventListener('keydown', key, true); ghost.remove();
      }
      var own = { cancel: cleanup }; gesture = own;
      function cancel(e) {
        if (e && typeof e.pointerId === 'number' && e.pointerId !== pointer) return;
        if (gesture === own) gesture = null; cleanup();
      }
      function key(e) { if (e.key === 'Escape') cancel(); }
      function place(e) {
        var rect = area.getBoundingClientRect();
        ghost.style[axis === 'h' ? 'top' : 'left'] = Math.max(18, axis === 'h' ? e.clientY - rect.top : e.clientX - rect.left) + 'px';
      }
      function move(e) {
        if (e.pointerId !== pointer) return;
        if (!shown(origin) || root.state.guidesLocked || !root.state.rulersOn) { cancel(); return; }
        e.stopImmediatePropagation(); place(e);
      }
      function up(e) {
        if (e.pointerId !== pointer) return;
        e.stopImmediatePropagation(); cancel();
        if (!shown(origin) || root.state.guidesLocked || !root.state.rulersOn) { notice(); return; }
        var rect = area.getBoundingClientRect(), x = e.clientX - rect.left, y = e.clientY - rect.top;
        var back = (axis === 'h' ? y : x) < 18;
        if (back) { if (ordinal !== null) edit({ kind: 'remove', axis: axis, ordinal: ordinal }, origin); return; }
        var point = root.view.viewToProject(new root.Point(axis === 'h' ? 0 : x, axis === 'h' ? y : 0));
        var position = axis === 'h' ? point.y : point.x;
        var action = ordinal === null ? { kind: 'add', axis: axis, position: position } :
          { kind: 'move', axis: axis, ordinal: ordinal, position: position };
        edit(action, origin);
      }
      document.addEventListener('pointermove', move, true); document.addEventListener('pointerup', up, true);
      document.addEventListener('pointercancel', cancel, true); document.addEventListener('keydown', key, true); place(event); return true;
    }
    function presented() {
      if (!native() || retired) return Promise.resolve(null);
      if (pending || loading) {
        var next = identity();
        if (next && changed(busyStamp, next) && available(next.session, next.info)) refreshNeeded = true;
        return loading || Promise.resolve(null);
      }
      if (model.read()) return Promise.resolve(model.read());
      busyStamp = identity(); loading = model.refresh().catch(function () { return null; })
        .finally(function () { loading = null; settled(); }); repaint(); return loading;
    }
    var controller = Object.freeze({ read: model.read, edit: edit, beginDrag: beginDrag, presented: presented,
      isPending: function () { return pending || !!loading; },
      willProject: function () { projectionEpoch++; cancelGesture(); if (!pending) model.invalidate(); repaint(); },
      dispose: function () { retired = true; refreshNeeded = false; cancelGesture(); model.dispose(); repaint(); }
    });
    installed.set(root, controller); root.NemoNativeGuideController = controller;
    if (typeof root.addEventListener === 'function') {
      root.addEventListener('pagehide', controller.dispose, { once: true }); root.addEventListener('blur', cancelGesture);
    }
    return controller;
  }
  return Object.freeze({ install: install });
}));
