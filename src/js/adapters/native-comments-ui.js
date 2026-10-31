/* Confirmed project-comment readers and existing popover controls; no canvas input route. */
(function (root, factory) {
  var common = typeof module === 'object' && module.exports;
  var api = factory(common ? require('./native-comments-desktop.js') : root.NemoNativeCommentsDesktop,
    (common ? require('./native-viewport-underlay.js') : root.NemoNativeViewportUnderlay).canvasMapping);
  if (common) module.exports = api;
  else { root.NemoNativeCommentsUI = api; root.document.addEventListener('DOMContentLoaded', function () { api.install(root); }, { once: true }); }
}(typeof globalThis === 'object' ? globalThis : this, function (Desktop, Mapping) {
  'use strict';
  var installed = new WeakMap();
  function native(root) { return !!(root.__TAURI__ && root.__TAURI__.core); }
  function install(root) {
    if (installed.has(root)) return installed.get(root);
    if (!native(root) || !Desktop || !Mapping) return null;
    var retired = false, loading = null, pending = false, refreshNeeded = false, origins = new WeakMap(), popovers = new WeakSet(), gestureEpoch = 0;
    function current() { return root.NemoNativeObjectProject && root.NemoNativeObjectProject.desktop(root); }
    function available(session, info) {
      return !retired && session === current() && root.NemoNativeObjectProjectFile &&
        root.NemoNativeObjectProjectFile.projectionReadyForUI(root, info) === true;
    }
    var model = Desktop.create({ currentSession: current, available: available, publish: function () { return true; } });
    function notice() { if (root.showToast) root.showToast('Project comments are unavailable or unconfirmed. Wait for synchronization.'); }
    function repaint() {
      if (typeof root.refreshFbAvatars === 'function') root.refreshFbAvatars();
      var pop = root.document.getElementById('fb-avatars-pop');
      if (pop && pop.classList.contains('open') && typeof root.renderFbAvatarPopover === 'function') root.renderFbAvatarPopover();
    }
    function cancel(record) { gestureEpoch++; pointerIntent = null; if (record) origins.delete(record); else origins = new WeakMap(); }
    // UI ownership survives consumption of the one-shot command proof. It never authorizes an edit.
    function ownsPopover(record) { return !!record && popovers.has(record); }
    function close(record) {
      cancel(record);
      if (root._activeComment === record && !root._recording && typeof root.closeCommentPopover === 'function') {
        popovers.delete(record); root.closeCommentPopover();
      } else if (root._activeComment !== record) popovers.delete(record);
    }
    var pointerIntent = null;
    var pinKeys = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
    function same(a, b) { return !!a && !!b && pinKeys.every(function (key) { return a[key] === b[key]; }); }
    function context(lease, proof) {
      var session = current(), info = session && session.info(), state = root.state;
      var projection = proof || !pending && !loading && model.read();
      if (!projection || !session || !session.isActive() || !same(info, projection.info) || !state || state.playing ||
          state.spaceDown || state.isPanning || state.activeSymbolId || state.tool !== 'comment') return null;
      if (lease && typeof lease.readPresentedObjectSession !== 'function') return null;
      var frame = state.currentFrame, shown = lease ? lease.readPresentedObjectSession() : session.readPresentedObjectSession();
      if (!Number.isSafeInteger(frame) || frame < 0 || frame >= info.totalFrames || !same(shown, info) ||
          !shown.presentation || shown.presentation.owner !== 'native' || shown.presentation.status !== 'presented' || shown.presentation.frame !== frame || (lease ? (shown.presentation.view ? shown.presentation.view.mode : 'canvas') !== (state.cameraView === true ? 'stored-camera' : 'canvas') : !Mapping.matchesView(root, shown.presentation))) return null;
      var rect = Mapping.bounds(root, info); if (!rect) return null;
      var edges = [rect.left, rect.top, rect.left + rect.width, rect.top + rect.height];
      return { session: session, projection: projection, presentation: shown.presentation, ticket: session.readSubscriptionGeneration(), frame: frame,
        bounds: { left: rect.left, top: rect.top, width: rect.width, height: rect.height },
        canvasW: info.canvasW, canvasH: info.canvasH, key: JSON.stringify([edges, frame, state.tool, state.appMode]) };
    }
    function owns(origin, lease) {
      var next = context(lease, lease ? origin.projection : null);
      return origin.epoch === gestureEpoch && !!next && next.session === origin.context.session && next.projection === origin.projection &&
        next.presentation === origin.context.presentation && next.ticket === origin.context.ticket && next.key === origin.context.key;
    }
    // #2372: a presentation hidden only by the native queue (queue-pending) is "cannot prove it right now", not "changed". Opening the popover on an origin whose
    // session, subscription and gesture are untouched is allowed through that window; save / delete re-prove the whole origin (route) before any write.
    function hiddenByQueue() {
      var Wait = root.NemoNativePresentationWait;
      return !!Wait && Wait.busy(function (f) { var session = current(); return session ? session.readPresentedObjectSession(f) : null; });
    }
    function unproven(origin) {
      var session = current();
      return !retired && !pending && !loading && origin.epoch === gestureEpoch && !!session && session === origin.context.session &&
        session.readSubscriptionGeneration() === origin.context.ticket && hiddenByQueue();
    }
    function begin(existing) {
      var origin = existing && origins.get(existing);
      // Only the genuine existing pointer port can issue this transient identity.
      if (!origin || !(owns(origin) || unproven(origin))) { notice(); return false; }
      return existing;
    }
    function screen(record) {
      var origin = record && origins.get(record); if (!origin || !owns(origin)) return null;
      var c = origin.context;
      return Mapping.screen(origin.point, c);
    }
    function handleRaw(method, event) {
      var active = ownsPopover(root._activeComment);
      if (!root.state || root.state.tool !== 'comment') {
        if (active) close(root._activeComment); pointerIntent = null; return false;
      }
      var cutover = root.NemoNativeOpacityCutover;
      if (!cutover || typeof cutover.blocksLegacy !== 'function' || cutover.blocksLegacy() === false) return false;
      if (event.stopImmediatePropagation) event.stopImmediatePropagation(); if (event.preventDefault) event.preventDefault();
      if (method === 'cancelEvent') {
        if (pointerIntent && event.pointerId !== undefined && event.pointerId !== pointerIntent.pointer) return true;
        pointerIntent = null; if (active) close(root._activeComment); return true;
      }
      if (pending) return true; // Ordinary motion cannot dismiss or re-authorize a pending command.
      var c = context();
      if (!c) { pointerIntent = null; if (active) close(root._activeComment); if (method === 'down') notice(); return true; }
      if (active && (!origins.has(root._activeComment) || !owns(origins.get(root._activeComment)))) close(root._activeComment);
      if (method !== 'down') {
        if (pointerIntent && (!owns(pointerIntent) || event.pointerId !== pointerIntent.pointer)) pointerIntent = null;
        if (method === 'up') pointerIntent = null;
        return true;
      }
      var doc = root.document, stage = event.target === doc.getElementById('drawing-canvas') || event.target === doc.getElementById('canvas-area');
      if (!stage || event.button !== 0 || event.isPrimary !== true || !Number.isSafeInteger(event.pointerId) ||
          !Number.isFinite(event.clientX) || !Number.isFinite(event.clientY)) { pointerIntent = null; return true; }
      if (active) close(root._activeComment);
      var point = Object.freeze(Mapping.point(event, c));
      var hit = c.projection.comments.items.find(function (item) {
        var cm = item.comment;
        if (cm.frame !== c.frame || !Number.isFinite(cm.x) || !Number.isFinite(cm.y)) return false;
        var at = Mapping.screen(cm, c);
        // The original 14/view.zoom world radius is exactly 14 CSS pixels;
        // both axes use the admitted native mapping when the surface stretches.
        return Math.hypot(at.x - event.clientX, at.y - event.clientY) < 14;
      });
      var record;
      if (hit) record = Object.assign({}, hit.comment);
      else {
        var now = Date.now(), profile = root.state.userProfile;
        record = { id: 'c_' + now.toString(36) + '_' + Math.floor(Math.random() * 1e6),
          x: point.x, y: point.y, frame: c.frame, text: '', createdAt: now, resolved: false };
        [['authorId', 'id'], ['authorName', 'name'], ['authorColor', 'color']].forEach(function (pair) {
          if (profile && profile[pair[1]] !== undefined) record[pair[0]] = profile[pair[1]];
        });
      }
      record = Object.freeze(record);
      var origin = { epoch: gestureEpoch, projection: c.projection, ordinal: hit ? hit.ordinal : null, context: c, point: point, pointer: event.pointerId };
      pointerIntent = origin; origins.set(record, origin); popovers.add(record);
      if (typeof root.openCommentPopover !== 'function') { origins.delete(record); popovers.delete(record); notice(); return true; }
      root.openCommentPopover(point, record); return true;
    }
    var waiting = new WeakSet();
    function refuse(record, origin) { if ((origin || ownsPopover(record)) && !pending) close(record); notice(); return true; }
    function route(record, text, resolved, remove) {
      var origin = record && origins.get(record);
      if (retired || pending || loading || !origin) return refuse(record, origin);
      if (!owns(origin)) {
        // #2372: an origin that is not provable only because a pending native command hides the presentation is waited out (bounded, native-presentation-wait.js);
        // every strict check then runs again against what is readable, and a real change (cancel, pin, view) still refuses.
        if (waiting.has(record)) return true;
        if (!hiddenByQueue()) return refuse(record, origin);
        waiting.add(record);
        root.NemoNativePresentationWait.until(root, { readPresentedObjectSession: function (f) { var session = current(); return session ? session.readPresentedObjectSession(f) : null; } },
          function () { return !retired && owns(origin); }, function () { return !retired && origins.get(record) === origin && origin.epoch === gestureEpoch; })
          .then(function (ok) { waiting.delete(record); if (ok && !retired && origins.get(record) === origin && !pending && !loading && owns(origin)) submit(record, origin, text, resolved, remove); else refuse(record, origins.get(record)); },
            function () { waiting.delete(record); refuse(record, origins.get(record)); });
        return true;
      }
      return submit(record, origin, text, resolved, remove);
    }
    function submit(record, origin, text, resolved, remove) {
      if (origin.ordinal === null && (remove || !text.trim())) { close(record); return true; }
      var intent;
      if (origin.ordinal === null) intent = { kind: 'append', comment: Object.assign({}, record, { text: text, resolved: resolved }) };
      else intent = remove ? { kind: 'delete', ordinal: origin.ordinal } :
        { kind: 'update', ordinal: origin.ordinal, text: text, resolved: resolved };
      pending = true; origins.delete(record);
      Promise.resolve().then(function () { return model.edit(intent, origin.projection, function (lease) { return owns(origin, lease); }); }).then(function (result) {
        if (result.status === 'applied-presented' || result.status === 'unchanged') close(record);
        else { close(record); notice(); }
      }).catch(function () { close(record); notice(); }).finally(function () {
        pending = false; repaint(); var again = refreshNeeded; refreshNeeded = false;
        if (again && !model.read()) presented();
      });
      return true;
    }
    function presented() {
      if (retired) return Promise.resolve(null);
      if (pending || loading) { refreshNeeded = true; return loading || Promise.resolve(null); }
      var value = model.read();
      if (value) { repaint(); return Promise.resolve(value); }
      loading = model.refresh().then(function (value) { repaint(); return value; }).catch(function () { repaint(); return null; })
        .finally(function () { loading = null; var again = refreshNeeded; refreshNeeded = false; if (again && !model.read()) presented(); });
      return loading;
    }
    function retireGesture() { if (gateOf) gateOf.cancel(); pointerIntent = null; var record = root._activeComment; if (ownsPopover(record)) close(record); cancel(); }
    function key(event) { if (event.key === 'Escape' || root.state.tool !== 'comment') retireGesture(); }
    function focus(event) { var pop = root.document.getElementById('comment-popover'); if (!pop || !pop.contains(event.target)) retireGesture(); }
    root.document.addEventListener('focusin', focus);
    root.addEventListener('blur', retireGesture); root.addEventListener('resize', retireGesture); root.addEventListener('keydown', key);
    // #2372: a press that finds the presentation hidden by the queue is held with the move / release that follow it and replayed once it is readable; a real change
    // (tool, frame, session, subscription, gesture) drops it. Only a press can start waiting, so the other events skip the gate unless one is held.
    var gateOf = null;
    function gate() {
      var Wait = root.NemoNativePresentationWait;
      if (!gateOf && Wait) gateOf = Wait.gate(root, {
        handle: handleRaw, ends: ['cancelEvent'],
        begins: function (method, event) { var cutover = root.NemoNativeOpacityCutover; return method === 'down' && !pending && !loading && root.state && root.state.tool === 'comment' && event.button === 0 &&
          !!cutover && typeof cutover.blocksLegacy === 'function' && cutover.blocksLegacy() !== false; },
        read: function (f) { var session = current(); return session ? session.readPresentedObjectSession(f) : null; },
        snapshot: function () { var session = current(); return { tool: root.state.tool, frame: root.state.currentFrame, epoch: gestureEpoch, session: session, ticket: session && session.readSubscriptionGeneration(), info: session && session.info() }; },
        holds: function (t) { var session = current();
          return !retired && root.state.tool === 'comment' && root.state.currentFrame === t.frame && gestureEpoch === t.epoch && session === t.session && !!session &&
            session.readSubscriptionGeneration() === t.ticket && same(session.info(), t.info); },
        notice: 'Project comments are unavailable or unconfirmed. Wait for synchronization.' });
      return gateOf;
    }
    function handle(method, event) { var g = gate(); return g && (method === 'down' || g.waiting()) ? g.handle(method, event) : handleRaw(method, event); }
    var controller = Object.freeze({ read: model.read, presented: presented, begin: begin, handle: handle, screen: screen,
      existing: function (record) { var origin = record && origins.get(record); return !!origin && origin.ordinal !== null; },
      save: function (record, text, resolved) { return route(record, text, resolved, false); },
      remove: function (record) { return route(record, null, null, true); },
      cancel: function (record) { cancel(record); if (record) popovers.delete(record); },
      willProject: function () {
        var record = root._activeComment, owned = ownsPopover(record);
        cancel(); if (!pending) model.invalidate();
        if (owned) close(record);
        repaint();
      },
      dispose: function () {
        if (retired) return; retireGesture(); retired = true; if (gateOf) gateOf.cancel(); model.dispose();
        root.removeEventListener('pagehide', controller.dispose); root.removeEventListener('blur', retireGesture);
        root.removeEventListener('resize', retireGesture); root.removeEventListener('keydown', key);
        root.document.removeEventListener('focusin', focus); repaint();
      }
    });
    installed.set(root, controller); root.NemoNativeCommentsController = controller;
    root.addEventListener('pagehide', controller.dispose, { once: true }); presented(); return controller;
  }
  function controller(root) { return installed.get(root) || install(root); }
  function begin(root, existing) {
    if (!native(root)) return null;
    var control = controller(root); return control ? control.begin(existing) : false;
  }
  function save(root, record, text, resolved) {
    if (!native(root)) return false;
    var control = controller(root); if (control) return control.save(record, text, resolved);
    if (root.showToast) root.showToast('Native project comment editing is unavailable.'); return true;
  }
  function remove(root, record) {
    if (!native(root)) return false;
    var control = controller(root); if (control) return control.remove(record);
    if (root.showToast) root.showToast('Native project comment editing is unavailable.'); return true;
  }
  function cancel(root, record) { var control = installed.get(root); if (control) control.cancel(record); }
  function read(root) { var control = installed.get(root); return control && control.read(); }
  function handle(root, method, event, cancelOtherIntent) {
    if (!native(root)) return false;
    var control = controller(root), handled = !!control && control.handle(method, event);
    if (handled && typeof cancelOtherIntent === 'function') cancelOtherIntent(); return handled;
  }
  function screen(root, record) { var control = installed.get(root); return control && control.screen(record); }
  function existing(root, record) { var control = installed.get(root); return !!control && control.existing(record); }
  return Object.freeze({ native: native, install: install, begin: begin, save: save, remove: remove, cancel: cancel, read: read,
    handle: handle, screen: screen, existing: existing });
}));
