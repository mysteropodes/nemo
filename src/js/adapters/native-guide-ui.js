/* Ordinary guide management consumes native readback; it owns only DOM and gestures. */
(function (root, factory) {
  var common = typeof module === 'object' && module.exports;
  var api = factory(common ? require('./native-guide-desktop.js') : root.NemoNativeGuideDesktop,
    ((common ? require('./native-viewport-underlay.js') : root.NemoNativeViewportUnderlay) || {}).canvasMapping);
  if (common) module.exports = api;
  else { root.NemoNativeGuideUI = api; api.install(root); }
}(typeof globalThis === 'object' ? globalThis : this, function (Desktop, Mapping) {
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
    function coordinateState() {
      try {
        var stamp = identity(), projection = model.read(), state = root.state;
        if (busy() || !stamp || !projection || !state || !available(stamp.session, stamp.info) ||
            !identityKeys.every(function (key) { return projection.info[key] === stamp.pin[key]; })) return null;
        var shown = stamp.session.readPresentedObjectSession(), p = shown && shown.presentation, frame = state.currentFrame;
        if (!shown || !p || p.owner !== 'native' || p.status !== 'presented' || !p.view ||
            !identityKeys.every(function (key) { return shown[key] === stamp.pin[key] && p[key] === stamp.pin[key]; }) ||
            p.contextId !== 'scene-root' || typeof p.workId !== 'string' || !p.workId || !Number.isSafeInteger(p.viewGeneration) || p.viewGeneration < 1 ||
            p.frame !== frame || !Number.isSafeInteger(frame) || frame < 0 || frame >= stamp.info.totalFrames ||
            !Mapping.matchesView(root, p)) return null;
        var bounds = Mapping.bounds(root, stamp.info), element = document.getElementById('canvas-area');
        var area = element && element.getBoundingClientRect();
        if (!bounds || !area || ![area.left, area.top, area.width, area.height].every(Number.isFinite) || area.width <= 0 || area.height <= 0) return null;
        var context = { bounds: Object.freeze(Object.assign({}, bounds)), canvasW: stamp.info.canvasW, canvasH: stamp.info.canvasH,
          presentation: JSON.parse(JSON.stringify(p)) };
        var forward = Mapping.transform(context, false), inverse = Mapping.transform(context, true);
        if (![forward, inverse].every(function (m) { var d = m[0] * m[3] - m[1] * m[2]; return Number.isFinite(d) && d !== 0; })) return null;
        area = Object.freeze({ left: area.left, top: area.top, width: area.width, height: area.height });
        var signature = JSON.stringify([identityKeys.map(function (key) { return stamp.pin[key]; }), stamp.ticket, stamp.epoch,
          frame, p.workId, p.viewGeneration, p.view, context.canvasW, context.canvasH, context.bounds, area]);
        return { stamp: stamp, projection: projection, presentation: p, context: context, area: area, signature: signature };
      } catch (_) { return null; }
    }
    // A transient conversion proof, never another writable viewport or guide owner.
    function coordinates() {
      var captured = coordinateState(); if (!captured) return null;
      function currentCoordinates() {
        var next = coordinateState();
        return !!next && !changed(captured.stamp, next.stamp) && next.projection === captured.projection &&
          next.presentation === captured.presentation && next.signature === captured.signature;
      }
      function finite(point) { return point && Number.isFinite(point.x) && Number.isFinite(point.y); }
      function project(point) {
        if (!currentCoordinates() || !finite(point)) return null;
        var result = Mapping.screen(point, captured.context);
        return finite(result) ? Object.freeze({ x: result.x - captured.area.left, y: result.y - captured.area.top }) : null;
      }
      function unproject(event) {
        if (!currentCoordinates() || !event || !Number.isFinite(event.clientX) || !Number.isFinite(event.clientY)) return null;
        var result = Mapping.point(event, captured.context);
        return finite(result) ? Object.freeze(result) : null;
      }
      function segment(axis, position) {
        if (!['h', 'v'].includes(axis) || !Number.isFinite(position)) return null;
        var a = project(axis === 'h' ? { x: 0, y: position } : { x: position, y: 0 });
        var b = project(axis === 'h' ? { x: 1, y: position } : { x: position, y: 1 });
        if (!a || !b) return null;
        var lo = -Infinity, hi = Infinity, delta = { x: b.x - a.x, y: b.y - a.y };
        var inside = ['x', 'y'].every(function (key) {
          var limit = key === 'x' ? captured.area.width : captured.area.height;
          if (delta[key] === 0) return a[key] >= 0 && a[key] <= limit;
          var first = -a[key] / delta[key], last = (limit - a[key]) / delta[key];
          lo = Math.max(lo, Math.min(first, last)); hi = Math.min(hi, Math.max(first, last)); return lo <= hi;
        });
        if (!inside || !Number.isFinite(lo) || !Number.isFinite(hi)) return null;
        var from = { x: a.x + lo * delta.x, y: a.y + lo * delta.y }, to = { x: a.x + hi * delta.x, y: a.y + hi * delta.y };
        return finite(from) && finite(to) ? Object.freeze({ from: Object.freeze(from), to: Object.freeze(to) }) : null;
      }
      return Object.freeze({ signature: captured.signature, area: captured.area, bounds: captured.context.bounds,
        project: project, unproject: unproject, segment: segment, current: currentCoordinates });
    }
    function placeGhost(ghost, segment) {
      if (!segment) { ghost.style.display = 'none'; return; }
      var a = segment.from, dx = segment.to.x - a.x, dy = segment.to.y - a.y;
      Object.assign(ghost.style, { display: '', left: a.x + 'px', top: a.y + 'px', right: 'auto', bottom: 'auto', height: '0px',
        width: Math.hypot(dx, dy) + 'px', borderLeft: '0', borderTop: '1px dashed var(--accent)', transformOrigin: '0 0',
        transform: 'rotate(' + Math.atan2(dy, dx) + 'rad)' });
    }
    function beginDrag(event, axis, ordinal, container) {
      var origin = model.read(), area = document.getElementById('canvas-area'), proof = coordinates();
      if (!shown(origin) || !proof || gesture || !area || !container || !['h', 'v'].includes(axis) ||
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
        var point = proof.unproject(e);
        if (!point) { cancel(); return; }
        placeGhost(ghost, proof.segment(axis, axis === 'h' ? point.y : point.x));
      }
      function move(e) {
        if (e.pointerId !== pointer) return;
        if (!shown(origin) || !proof.current() || root.state.guidesLocked || !root.state.rulersOn) { cancel(); return; }
        e.stopImmediatePropagation(); place(e);
      }
      function up(e) {
        if (e.pointerId !== pointer) return;
        e.stopImmediatePropagation(); cancel();
        if (!shown(origin) || !proof.current() || root.state.guidesLocked || !root.state.rulersOn) { notice(); return; }
        var x = e.clientX - proof.area.left, y = e.clientY - proof.area.top;
        var back = (axis === 'h' ? y : x) < 18;
        if (back) { if (ordinal !== null) edit({ kind: 'remove', axis: axis, ordinal: ordinal }, origin); return; }
        var point = proof.unproject(e);
        if (!point) { notice(); return; }
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
      var value = model.read();
      if (value) { repaint(); return Promise.resolve(value); }
      busyStamp = identity(); loading = model.refresh().catch(function () { return null; })
        .finally(function () { loading = null; settled(); }); repaint(); return loading;
    }
    var controller = Object.freeze({ read: model.read, edit: edit, beginDrag: beginDrag, presented: presented, coordinates: coordinates,
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
