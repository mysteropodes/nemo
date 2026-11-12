/* Ordinary perspective management consumes native readback; it owns only DOM and gestures. */
(function (root, factory) {
  var common = typeof module === 'object' && module.exports;
  var api = factory(common ? require('./native-perspective-desktop.js') : root.NemoNativePerspectiveDesktop);
  if (common) module.exports = api;
  else { root.NemoNativePerspectiveUI = api; api.install(root); }
}(typeof globalThis === 'object' ? globalThis : this, function (Desktop) {
  'use strict';
  var installed = new WeakMap();
  function install(root) {
    if (installed.has(root)) return installed.get(root);
    var retired = false, loading = null, pending = false;
    var projectionEpoch = 0, refreshNeeded = false, busyStamp = null;
    var identityKeys = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
    var document = root.document, nativeDrag = null, readListeners = new Set(), lastPublishedRead = null;
    function engineOn() { return root.SMEngineBridge && root.SMEngineBridge.isEnabled() && !root.state.playing; }
    function native() { return !!(root.__TAURI__ && root.__TAURI__.core); }
    function current() { return root.NemoNativeObjectProject && root.NemoNativeObjectProject.desktop(root); }
    function available(session, info) {
      return native() && !retired && session === current() && !!root.NemoNativeObjectProjectFile &&
        root.NemoNativeObjectProjectFile.projectionReadyForUI(root, info) === true;
    }
    var model = Desktop.create({ currentSession: current, available: available, publish: function () { return true; } });
    function notice(message) {
      if (typeof root.showToast === 'function') root.showToast(message || 'Perspective changes are not confirmed. Wait for synchronization before trying again.');
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
    function publishRead(value) {
      if (!value || retired || model.read() !== value || value === lastPublishedRead) return;
      lastPublishedRead = value;
      readListeners.forEach(function (listener) { try { listener(value); } catch (_) {} });
    }
    function settled() {
      busyStamp = null;
      var refresh = refreshNeeded; refreshNeeded = false; repaint(); publishRead(model.read());
      if (refresh && !retired && !model.read()) presented();
    }
    function shown(origin) { return !busy() && !!origin && model.read() === origin; }
    function repaint() {
      if (typeof root.renderLabsFloatPanel === 'function') root.renderLabsFloatPanel();
      if (!native() && root.SMEngineBridge) root.SMEngineBridge.renderNow();
    }
    function cancelGesture() { if (typeof root.cancelPerspectiveGesture === 'function') root.cancelPerspectiveGesture(); }
    // #2372: an origin that is not shown only because a pending native command hides the presentation is waited out (model.readSettled), not refused.
    async function shownSoon(origin) { return shown(origin) || (!busy() && !!origin && await model.readSettled() === origin); }
    async function edit(action, origin) {
      if (!(await shownSoon(origin))) { notice(); return { status: 'unavailable' }; }
      cancelGesture(); busyStamp = identity(); pending = true; repaint();
      try {
        var result = await model.edit(action, origin);
        if (!['applied-presented', 'unchanged'].includes(result.status)) notice();
        return result;
      } catch (error) { notice(); return { status: 'unconfirmed', code: error && error.code || 'unavailable' }; }
      finally { pending = false; settled(); }
    }
    function presented() {
      if (!native() || retired) return Promise.resolve(null);
      if (pending || loading) {
        var next = identity();
        if (next && changed(busyStamp, next) && available(next.session, next.info)) refreshNeeded = true;
        return loading || Promise.resolve(null);
      }
      var value = model.read();
      if (value) { publishRead(value); return Promise.resolve(value); }
      busyStamp = identity(); loading = Promise.resolve(null); repaint();
      loading = model.refresh().catch(function () { return null; })
        .finally(function () { loading = null; settled(); }); return loading;
    }
    function bounds() {
      var canvas = document.getElementById('drawing-canvas'), rect = canvas && canvas.getBoundingClientRect();
      var html = document.documentElement;
      if (!rect || !html || ![rect.left, rect.top, rect.width, rect.height].every(Number.isFinite) ||
          rect.width <= 0 || rect.height <= 0 || rect.left < 0 || rect.top < 0 ||
          rect.left + rect.width > html.clientWidth || rect.top + rect.height > html.clientHeight ||
          !html.classList.contains('nemo-native-underlay') || typeof html.style.getPropertyValue !== 'function') return null;
      var edges = [rect.left, rect.top, rect.left + rect.width, rect.top + rect.height];
      if (!['left', 'top', 'right', 'bottom'].every(function (key, index) {
        return parseFloat(html.style.getPropertyValue('--nemo-native-' + key)) === edges[index];
      })) return null;
      return rect;
    }
    function nativePoint(event, origin) {
      var rect = bounds();
      if (!origin || !rect || ![event.clientX, event.clientY, rect.left, rect.top, rect.width, rect.height].every(Number.isFinite) ||
          rect.width <= 0 || rect.height <= 0) return null;
      return [(event.clientX - rect.left) * origin.canvas.width / rect.width,
        (event.clientY - rect.top) * origin.canvas.height / rect.height];
    }
    function nativeScale(origin) {
      var rect = bounds();
      return origin && rect && rect.width > 0 && rect.height > 0 ?
        Math.max(origin.canvas.width / rect.width, origin.canvas.height / rect.height) : 1;
    }
    function screenPoint(point, origin, rect) {
      return { x: rect.left + point.x * rect.width / origin.canvas.width,
        y: rect.top + point.y * rect.height / origin.canvas.height };
    }
    function cancelNative() {
      var old = nativeDrag; nativeDrag = null;
      if (old && old.suspended) { old.suspended = false; old.engine.resume(); old.engine.renderNow(); }
    }
    // #2372: a presentation hidden only by the native queue (queue-pending) is "cannot prove it right now", not "changed": a drag in progress survives it, and the
    // release still re-proves the origin strictly (edit -> shownSoon) before anything is written. A real change drops the sealed projection, so this is false then.
    function hiddenByQueue() { var Wait = root.NemoNativePresentationWait; return !!Wait && Wait.busy(model.read); }
    function dragCurrent(drag) {
      var rect = bounds(), c = controller;
      return !retired && c && (c.read() === drag.origin || hiddenByQueue()) && !c.isPending() && engineOn() && root.SMEngineBridge === drag.engine &&
        root.state.tool === drag.tool && root.state.currentFrame === drag.frame && rect &&
        ['left', 'top', 'width', 'height'].every(function (key) { return rect[key] === drag.rect[key]; });
    }
    function nativeDown(event) {
      if (retired || nativeDrag || !(engineOn() && (root.state.tool === 'perspective' || model.read() && model.read().perspective.settings.perspectiveEnabled)) || event.button !== 0) return;
      var origin = model.read(), c = controller, rect = bounds(), w = nativePoint(event, origin);
      if (!origin || !c || c.isPending() || !w || !rect) return;
      var vps = origin.perspective.items, ordinal = null;
      for (var i = 0; i < vps.length; i++) {
        var point = screenPoint(vps[i], origin, rect);
        if (!vps[i].locked && Math.hypot(point.x - event.clientX, point.y - event.clientY) < 12) { ordinal = i + 1; break; }
      }
      if (ordinal === null) {
        if (vps.length < 2) return;
        var a = screenPoint(vps[0], origin, rect), b = screenPoint(vps[1], origin, rect);
        var dx = b.x - a.x, dy = b.y - a.y, length = Math.hypot(dx, dy) || 1;
        var ux = dx / length, uy = dy / length, t = (event.clientX - a.x) * ux + (event.clientY - a.y) * uy;
        if (Math.hypot(event.clientX - a.x - ux * t, event.clientY - a.y - uy * t) >= 10) return;
      }
      event.stopImmediatePropagation(); event.preventDefault();
      var ghost = vps.map(function (vp) { return Object.assign({}, vp); });
      nativeDrag = { origin: origin, ordinal: ordinal, pointer: event.pointerId, tool: root.state.tool, frame: root.state.currentFrame,
        rect: { left: rect.left, top: rect.top, width: rect.width, height: rect.height }, start: w, latest: w,
        ghost: ghost, engine: root.SMEngineBridge, suspended: true };
      nativeDrag.engine.suspend();
    }
    function nativeMove(event) {
      var drag = nativeDrag;
      if (!drag || drag.pointer !== event.pointerId) return;
      if (!dragCurrent(drag)) { cancelNative(); return; }
      event.stopImmediatePropagation(); event.preventDefault();
      var w = nativePoint(event, drag.origin); if (!w) { cancelNative(); return; }
      drag.latest = w;
      drag.ghost = drag.origin.perspective.items.map(function (vp) {
        var next = Object.assign({}, vp);
        if (!vp.locked && (drag.ordinal === null || vp.ordinal === drag.ordinal)) {
          next.x = drag.ordinal === null ? vp.x + w[0] - drag.start[0] : w[0];
          next.y = drag.ordinal === null ? vp.y + w[1] - drag.start[1] : w[1];
        }
        return next;
      });
      drag.engine.renderNow();
    }
    function nativeUp(event) {
      var drag = nativeDrag;
      if (!drag || drag.pointer !== event.pointerId) return;
      event.stopImmediatePropagation(); event.preventDefault();
      if (!dragCurrent(drag)) { cancelNative(); return; }
      var w = nativePoint(event, drag.origin); cancelNative(); if (!w) return;
      var action = drag.ordinal === null ? { kind: 'translate', dx: w[0] - drag.start[0], dy: w[1] - drag.start[1] } :
        { kind: 'move', ordinal: drag.ordinal, x: w[0], y: w[1] };
      edit(action, drag.origin);
    }
    // #2372: a press that finds the presentation hidden by the queue is held with the move / release that follow it and replayed once it is readable
    // (native-presentation-wait.js); a real change meanwhile (tool, frame, pin) drops it. Only a press can start waiting, so move / release skip the gate unless one is held.
    var gateOf = null;
    function gate() {
      var Wait = root.NemoNativePresentationWait;
      if (!gateOf && Wait) gateOf = Wait.gate(root, {
        handle: function (kind, event) {
          if (kind === 'down') nativeDown(event); else if (kind === 'move') nativeMove(event); else if (kind === 'up') nativeUp(event);
          else if (!nativeDrag || event.pointerId === nativeDrag.pointer) cancelNative();
        },
        begins: function (kind, event) { return kind === 'down' && !retired && !nativeDrag && engineOn() && root.state.tool === 'perspective' && event.button === 0; },
        read: model.read, snapshot: function () { return { tool: root.state.tool, frame: root.state.currentFrame, stamp: identity() }; },
        holds: function (token) {
          var next = identity();
          return !retired && root.state.tool === token.tool && root.state.currentFrame === token.frame && !!next && !changed(token.stamp, next);
        }, notice: 'The canvas is still busy - try again.' });
      return gateOf;
    }
    function routed(kind, direct) {
      return function (event) { var g = gate(); return g && (kind === 'down' || g.waiting()) ? g.handle(kind, event) : direct(event); };
    }
    var controller = Object.freeze({ read: model.read, readForCommand: model.readForCommand, edit: edit, presented: presented,
      onRead: function (listener) {
        if (retired || typeof listener !== 'function') throw new TypeError('Perspective read listener unavailable');
        readListeners.add(listener); return function () { readListeners.delete(listener); };
      },
      point: nativePoint, scale: nativeScale, pointerDown: routed('down', nativeDown), pointerMove: routed('move', nativeMove), pointerUp: routed('up', nativeUp),
      pointerCancel: routed('cancel', function (event) { if (!nativeDrag || event.pointerId === nativeDrag.pointer) cancelNative(); }),
      cancelPointer: function () { if (gateOf) gateOf.cancel(); cancelNative(); }, readGhost: function () {
        if (nativeDrag && !dragCurrent(nativeDrag)) cancelNative();
        return nativeDrag ? nativeDrag.ghost : null;
      },
      isPending: function () { return pending || !!loading; },
      willProject: function () { projectionEpoch++; cancelGesture(); if (!pending) model.invalidate(); repaint(); },
      dispose: function () { retired = true; if (gateOf) gateOf.cancel(); refreshNeeded = false; readListeners.clear(); cancelGesture(); model.dispose();
        if (typeof root.removeEventListener === 'function') root.removeEventListener('pagehide', controller.dispose); repaint(); }
    });
    installed.set(root, controller); root.NemoNativePerspectiveController = controller;
    if (typeof root.addEventListener === 'function') {
      root.addEventListener('pagehide', controller.dispose, { once: true });
    }
    return controller;
  }
  return Object.freeze({ install: install });
}));
