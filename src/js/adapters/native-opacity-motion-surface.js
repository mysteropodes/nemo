/* Stateless Motion/native compatibility; callers retain no mutable authority here. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeOpacityMotionSurface = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var UNHANDLED = Object.freeze({ handled: false });
  // One logical frame step, preserving the original loop, ping-pong and
  // audio boundary behavior. This changes only the UI playback direction;
  // document evaluation and presentation remain the native navigator's job.
  // Ping-pong reverses only beyond the work-area edge, including a one-frame
  // work area. Ordinary looping notifies audio when wrapping to the start.
  function advancePlaybackFrame(state, audio, cur) {
    var next = cur + state.playDir;
    if (next > state.waOut) {
      if (state.loopPlayback && state.pingPongPlayback) {
        state.playDir = -1; next = cur - 1;
        if (next < state.waIn) next = state.waIn;
      } else if (state.loopPlayback) {
        next = state.waIn;
        if (audio) audio.onLoop(next);
      } else return null;
    } else if (next < state.waIn) {
      if (state.loopPlayback && state.pingPongPlayback) {
        state.playDir = 1; next = cur + 1;
        if (next > state.waOut) next = state.waOut;
      } else return null;
    }
    return next;
  }
  function freeze(value) {
    if (value && typeof value === 'object') {
      Object.keys(value).forEach(function (key) { freeze(value[key]); });
      Object.freeze(value);
    }
    return value;
  }
  function detached(value) { return freeze(JSON.parse(JSON.stringify(value))); }
  function preparedFor(controller, holder) {
    if (!controller) return null;
    if (typeof controller.blocksLegacy !== 'function') throw new Error('native Motion controller is malformed');
    var blocked = controller.blocksLegacy();
    if (blocked === false) return null;
    if (blocked !== true) throw new Error('native Motion ownership is malformed');
    var current = controller.prepared(), identity = controller.identity();
    if (!identity || typeof identity.instanceId !== 'string' || !identity.instanceId ||
        typeof identity.documentId !== 'string' || !identity.documentId ||
        !Number.isSafeInteger(identity.contentRevision) || identity.contentRevision < 0 ||
        !current || !holder || current.layerUid !== holder.layerUid) {
      throw new Error('native Motion projection identity is unavailable');
    }
    return current;
  }
  function owns(controller, holder, prop) {
    return (prop === 'opacity' || prop === 'position') && !!preparedFor(controller, holder);
  }
  function positionAtFrame(current, frame) {
    if (!current || !Number.isInteger(frame) || frame < 0 || frame >= current.totalFrames ||
        !Array.isArray(current.frames) || !Array.isArray(current.resources)) {
      throw new Error('native position projection is unavailable');
    }
    var selected = current.frames[frame], handle = selected && selected.geometryHandle;
    if (!selected || selected.sourceFrame !== frame || !handle || typeof handle.resourceId !== 'string' ||
        typeof handle.resourceVersion !== 'string') throw new Error('native position frame identity is unavailable');
    var matches = current.resources.filter(function (resource) {
      return resource && resource.resourceId === handle.resourceId &&
        resource.resourceVersion === handle.resourceVersion;
    });
    if (matches.length !== 1 || !Array.isArray(matches[0].layers) || matches[0].layers.length !== 1) {
      throw new Error('native position resource identity is unavailable');
    }
    var projected = matches[0].layers[0], transform = projected && projected.transform;
    if (!projected || projected.layerUid !== current.layerUid || !Array.isArray(projected.bounds) ||
        projected.bounds.length !== 4 || projected.bounds[0] !== 20 || projected.bounds[1] !== 60 ||
        projected.bounds[2] !== 40 || projected.bounds[3] !== 80 || !Array.isArray(transform) ||
        transform.length !== 6 || transform[0] !== 1 || transform[1] !== 0 || transform[2] !== 0 ||
        transform[3] !== 1 || !Number.isFinite(transform[4]) || !Number.isFinite(transform[5])) {
      throw new Error('native position geometry projection is unavailable');
    }
    return [transform[4], transform[5]];
  }
  function read(controller, holder, prop, frame) {
    if (prop !== 'opacity' && prop !== 'position') return UNHANDLED;
    var current = preparedFor(controller, holder);
    if (!current) return UNHANDLED;
    if (!Number.isInteger(frame) || frame < 0 || frame >= current.totalFrames) throw new Error('native Motion frame is unavailable');
    var value;
    if (prop === 'position') value = positionAtFrame(current, frame);
    else {
      var selected = controller.projectSelection({ activeLayerUid: holder.layerUid,
        selected: [{ layerUid: holder.layerUid, opacityMode: current.opacityMode }] }, frame);
      value = [selected.selected[0].value];
      if (!Number.isFinite(value[0])) throw new Error('native opacity projection is unavailable');
    }
    return freeze({ handled: true, value: value });
  }
  // Rendering/property inspection must not materialize a persisted holder.
  // A detached seeded view preserves the displayed param-shape defaults;
  // the first real edit is replayed after release and calls the writer above.
  function detachedElementView(item, seed) {
    if (typeof seed !== 'function') throw new TypeError('Motion element seed callback is required');
    // Seed a detached holder from this shape's own defaults. Inspecting a row
    // must not add persisted elementMotion; its first write still requires release.
    // Motion owns the canonical defaults; this adapter owns their detached projection.
    // Dynamic shapes phase 2 (2026-08-18) — auto-tag + seed from the live
    // item's OWN current radii, found once here rather than re-derived on
    // every propsFor call: unlike Component exposedProps (one shared
    // default per key across every instance), each rect's un-animated
    // corner value is genuinely its OWN (data.paramShape.tl/tr/br/bl),
    // so PROP_DEFAULT can't carry it — motionStatic must, from the start,
    // or clicking the stopwatch for the first time (toggleAnimated's
    // OFF→ON reads valueAtFrame → staticValue → PROP_DEFAULT[0] when
    // nothing else is seeded) would silently snap a 40px corner back to
    // 0 the instant it's keyed.
    return detached(seed(item));
  }
  // Opening either expression editor is a read. Keep the absent-expression
  // view detached; the first actual commit creates persisted state only
  // after the common legacy-write admission succeeds.
  function detachedExpressionView(holder, prop, read) {
    if (typeof read !== 'function') throw new TypeError('Motion expression read callback is required');
    return detached(read(holder, prop));
  }
  function expressionSnapshot(holder, prop, read, project) {
    if (!holder) return null;
    if (typeof read !== 'function' || typeof project !== 'function') throw new TypeError('Motion expression snapshot callbacks are required');
    return freeze(project(read(holder, prop)));
  }
  // The admitted compatibility shell retains the original Position keys
  // only so release can reconstruct the document. Native ownership may
  // use the immutable per-frame geometry projection above for the box,
  // but must never evaluate or expose the shell's curve/key/handle data.
  // Suppress that editable path overlay until ownership returns to legacy.
  function positionOverlayPlan(controller, holder, outerFrames) {
    var current = preparedFor(controller, holder);
    if (!current) return UNHANDLED;
    return freeze({ handled: true, showLegacyKeys: false, allowKeyHitTest: false,
      samples: outerFrames.map(function (frame) {
        return { outerFrame: frame, position: positionAtFrame(current, frame) };
      }) });
  }
  // Native-owned Position has no editable JS key/handle overlay. Do not
  // even inspect the retained shell track from pointer hit-testing.
  function nativeKeyInteractionPlan(controller, holder) {
    return positionOverlayPlan(controller, holder, []);
  }
  // Persistence is composed only after the pinned native serialize and all
  // evaluations publish. The prepared projection is the opening snapshot.
  function opacityReadModel(controller, holder, frame) {
    var current = preparedFor(controller, holder);
    if (!current) return UNHANDLED;
    if (!Number.isInteger(frame) || frame < 0 || frame >= current.totalFrames) {
      throw new Error('native opacity frame is unavailable');
    }
    if (typeof controller.persistenceJSON !== 'function') throw new Error('native opacity persistence is unavailable');
    var before = controller.identity();
    var json = controller.persistenceJSON();
    var after = controller.identity();
    if (!before || !after || before.instanceId !== after.instanceId ||
        before.documentId !== after.documentId || before.contentRevision !== after.contentRevision ||
        typeof json !== 'string' || !json || json.length > 1048576) {
      throw new Error('native opacity persistence is fenced or stale');
    }
    var document = JSON.parse(json);
    var layers = document && document.layers;
    if (document.version !== 13 || document.totalFrames !== current.totalFrames ||
        !Array.isArray(layers) || layers.length !== 1) throw new Error('native opacity document is unsupported');
    var layer = layers[0];
    if (!layer || layer.layerUid !== current.layerUid ||
        !layer.motionStatic || !Array.isArray(layer.motionStatic.opacity) ||
        layer.motionStatic.opacity.length !== 1 ||
        !Number.isFinite(layer.motionStatic.opacity[0]) || layer.motionStatic.opacity[0] < 0 ||
        layer.motionStatic.opacity[0] > 100) throw new Error('native opacity layer is malformed');
    var track = layer.motion && layer.motion.opacity;
    var keys = [];
    if (current.opacityMode === 'keyed') {
      if (!track || !Array.isArray(track.keys) || track.keys.length !== 2) {
        throw new Error('native opacity keys are unavailable');
      }
      track.keys.forEach(function (key, index) {
        if (!key || key.frame !== (index ? 20 : 0) || !Array.isArray(key.v) ||
            key.v.length !== 1 || key.v[0] !== (index ? 80 : 20) ||
            Object.keys(key).sort().join(',') !== 'curvePoints,frame,hIn,hOut,v' ||
            !Array.isArray(key.hIn) || key.hIn.length !== 2 || key.hIn[0] !== 0 || key.hIn[1] !== 0 ||
            !Array.isArray(key.hOut) || key.hOut.length !== 2 || key.hOut[0] !== 0 || key.hOut[1] !== 0 ||
            !Array.isArray(key.curvePoints) || key.curvePoints.length !== 5 ||
            key.curvePoints.some(function (point, i) {
              var expected = [[0, 0], [0.25, 0.156], [0.5, 0.5], [0.75, 0.844], [1, 1]][i];
              return !point || Object.keys(point).sort().join(',') !== 'x,y' ||
                point.x !== expected[0] || point.y !== expected[1];
            })) {
          throw new Error('native opacity key is malformed');
        }
        keys.push({ frame: key.frame, value: key.v[0] });
      });
    } else if (current.opacityMode !== 'static' || track) {
      throw new Error('native opacity mode is malformed');
    }
    return freeze({ handled: true, instanceId: before.instanceId, documentId: before.documentId,
      contentRevision: before.contentRevision, layerUid: current.layerUid, property: 'opacity',
      opacityMode: current.opacityMode, staticValue: layer.motionStatic.opacity[0], keys: keys,
      animated: keys.length > 0, keyAtFrame: keys.some(function (key) { return key.frame === frame; }) });
  }
  function renderedOpacityRoute(controller, holder, prop, frame) {
    if (prop !== 'opacity') return UNHANDLED;
    return opacityReadModel(controller, holder, frame);
  }
  // Native opacity owns its own history. Static value edits stay native;
  // every key/animation-mode edit requests release through opacityLegacy.
  // In neither case may the generic JS history checkpoint run first.
  // The controller refresh callback renders after authoritative
  // caches publish. Re-entering the panel here would read through
  // the fence raised synchronously by setValue.
  // afterChange owns the post-publication UI/render refresh.
  function routeIntent(plan, application, kind, values, frame) {
    if (!plan || !plan.handled) return false;
    if (!application || typeof application.legacy !== 'function') throw new Error('native opacity intent facade is unavailable');
    application.legacy(kind, plan, values, frame);
    return true;
  }
  function routeDimension(plan, application, values, dimension, value, frame) {
    if (!plan || !plan.handled) return false;
    var next = values.slice(); next[dimension] = value;
    return routeIntent(plan, application, 'set', next, frame);
  }
  // Wrap at publication, before another classic script can retain a raw writer.
  // Each wrapper consults live admission; no lifecycle or wrapper registry lives here.
  function publishWriters(api, allow) {
    [
      ['addExprControl', 'motion-add-expression-control', null],
      ['renameExprControl', 'motion-rename-expression-control', false],
      ['removeExprControl', 'motion-remove-expression-control', false],
      ['moveExprControl', 'motion-move-expression-control', false],
      ['migrateLegacyCurves', 'motion-migrate-legacy-curves', 0],
      ['setKeyAtFrame', 'motion-key-frame', false],
      ['toggleLayer3D', 'motion-toggle-layer-3d', false],
      ['setDuplicatorEditSource', 'motion-duplicator-source', false],
      ['setLayerParent', 'motion-set-parent', false],
      ['setLayerParentB', 'motion-set-parent-b', false],
      ['setLayerFollowPath', 'motion-set-follow-path', false],
      ['addTextAnimator', 'motion-add-text-animator', null],
      ['removeTextAnimator', 'motion-remove-text-animator', false],
      ['upsertBlendKeyAt', 'motion-upsert-blend-key', false],
      ['removeBlendKeyAt', 'motion-remove-blend-key', false],
      ['removeParentKeyAt', 'motion-remove-parent-key', false],
      ['shiftLayerMotionKeys', 'motion-shift-layer-keys', false],
      ['setExprGlobals', 'motion-expression-globals', false],
      ['shiftKeySelection', 'motion-shift-key-selection', false],
      ['onEaseSegChanged', 'motion-ease-segment', false]
    ].forEach(function (entry) {
      var original = api[entry[0]];
      if (typeof original !== 'function') throw new Error('Motion writer is unavailable: ' + entry[0]);
      api[entry[0]] = function () {
        if (allow(entry[1]) !== true) return entry[2];
        return original.apply(this, arguments);
      };
    });
    return api;
  }
  // Native Motion navigation is a UI projection, never a Paper frame edit.
  // Serialize host presentations so an older completion cannot repaint over
  // a newer intent; publish the playhead only after a final identity match.
  function createFrameNavigator(ports) {
    var methods = ['frame', 'total', 'commit', 'paint', 'syncInput', 'fail', 'controller'];
    if (!ports || methods.some(function (name) { return typeof ports[name] !== 'function'; })) {
      throw new TypeError('native frame navigation ports are unavailable');
    }
    var queue = Promise.resolve(), intent = 0;
    function sameIdentity(a, b) {
      return !!a && !!b && a.instanceId === b.instanceId &&
        a.documentId === b.documentId && a.contentRevision === b.contentRevision;
    }
    function navigate(controller, frame) {
      ports.syncInput(); // The focused frame input may already show a tentative request.
      if (!Number.isInteger(frame) || frame < 0 || frame >= ports.total()) return Promise.resolve(false);
      var request = ++intent, requestedIdentity;
      try {
        if (!controller || !controller.isActive()) return Promise.resolve(false);
        requestedIdentity = controller.identity();
      } catch (_) { return Promise.resolve(false); }
      var work = queue.catch(function () {}).then(async function () {
        if (request !== intent || ports.controller() !== controller || !controller.isActive() ||
            !sameIdentity(requestedIdentity, controller.identity())) return false;
        var prior = ports.frame();
        try {
          var receipt = await controller.presentPreview(frame);
          if (request !== intent) return false; // The newer intent will re-present its frame.
          if (ports.controller() !== controller || !controller.isActive() ||
              !sameIdentity(requestedIdentity, controller.identity()) ||
              !receipt || receipt.owner !== 'native' || receipt.status !== 'presented' ||
              receipt.frame !== frame || !sameIdentity(requestedIdentity, receipt) ||
              !Number.isInteger(receipt.lifecycleGeneration) ||
              !Number.isInteger(receipt.viewGeneration) || !receipt.workId) {
            throw new Error('native frame presentation changed identity');
          }
          ports.commit(frame);
          ports.paint();
          ports.syncInput();
        } catch (error) {
          var sameOwner = false;
          try { sameOwner = ports.controller() === controller &&
            sameIdentity(requestedIdentity, controller.identity()); } catch (_) {}
          if (sameOwner) ports.commit(prior);
          ports.syncInput();
          // A failed receipt or UI paint can leave the host on a different
          // frame. Restore the previous one; failed restoration fences native.
          if (request === intent && sameOwner && controller.isActive()) {
            try { await controller.presentPreview(prior); ports.paint(); ports.syncInput(); }
            catch (_) {}
          }
          throw error;
        }
        return true;
      }).catch(function () {
        ports.syncInput();
        try { ports.fail(); } catch (_) {}
        return false;
      });
      queue = work;
      return work;
    }
    return Object.freeze({ navigate: navigate });
  }
  // Playback owns only a clock and a cancellable presentation intent. The
  // navigator above remains the sole route that can publish a UI frame.
  function createPlaybackScheduler(ports) {
    var methods = ['frame', 'fps', 'playing', 'controller', 'advance', 'navigate',
      'stop', 'now', 'request', 'cancel'];
    if (!ports || methods.some(function (name) { return typeof ports[name] !== 'function'; })) {
      throw new TypeError('native playback ports are unavailable');
    }
    var generation = 0, controller = null, identity = null, raf = null;
    function sameIdentity(a, b) {
      return !!a && !!b && a.instanceId === b.instanceId &&
        a.documentId === b.documentId && a.contentRevision === b.contentRevision;
    }
    function current(run) {
      try { return run === generation && ports.playing() &&
        ports.controller() === controller && controller.isActive() &&
        sameIdentity(identity, controller.identity()); }
      catch (_) { return false; }
    }
    function start(owner) {
      if (controller) return false;
      var initial;
      try { initial = owner && owner.isActive() && owner.identity(); }
      catch (_) { return false; }
      if (ports.controller() !== owner || !initial ||
          typeof initial.instanceId !== 'string' || !initial.instanceId ||
          typeof initial.documentId !== 'string' || !initial.documentId ||
          !Number.isSafeInteger(initial.contentRevision) || initial.contentRevision < 0 ||
          !Number.isFinite(ports.fps()) || ports.fps() <= 0) return false;
      controller = owner; identity = initial;
      var run = ++generation, frameMs = 1000 / ports.fps(), clock = ports.now();
      var pending = false;
      function step(now) {
        raf = null;
        if (!current(run)) { if (run === generation && ports.playing()) ports.stop(); return; }
        if (!pending) {
          var steps = Math.floor((now - clock) / frameMs);
          if (steps > 0) {
            if (steps > ports.fps() * 2) { steps = 1; clock = now; }
            else clock += steps * frameMs;
            var next = ports.frame(), ending = false;
            for (var k = 0; k < steps; k++) {
              var advanced = ports.advance(next);
              if (advanced === null) { ending = true; break; }
              next = advanced;
            }
            if (next !== ports.frame()) {
              pending = true;
              try {
                Promise.resolve(ports.navigate(next)).then(function (presented) {
                  pending = false;
                  if (current(run) && (!presented || ending)) ports.stop();
                }, function () { pending = false; if (current(run)) ports.stop(); });
              } catch (_) { pending = false; ports.stop(); return; }
            } else if (ending) { ports.stop(); return; }
          }
        }
        raf = ports.request(step);
      }
      try { raf = ports.request(step); }
      catch (_) { ++generation; controller = null; identity = null; return false; }
      return true;
    }
    function stop() {
      var wasNative = !!controller, restore = false;
      if (wasNative) {
        try { restore = ports.controller() === controller && controller.isActive() &&
          sameIdentity(identity, controller.identity()); } catch (_) {}
      }
      ++generation; controller = null; identity = null;
      if (raf !== null) { ports.cancel(raf); raf = null; }
      return Object.freeze({ wasNative: wasNative, restore: restore });
    }
    return Object.freeze({ start: start, stop: stop });
  }
  function requireAvailable(value) {
    var methods = ['owns', 'read', 'detachedElementView', 'detachedExpressionView', 'expressionSnapshot',
      'positionOverlayPlan', 'nativeKeyInteractionPlan', 'renderedOpacityRoute', 'routeIntent', 'routeDimension', 'publishWriters',
      'createFrameNavigator', 'createPlaybackScheduler', 'advancePlaybackFrame'];
    if (!value || !Object.isFrozen(value) || methods.some(function (name) { return typeof value[name] !== 'function'; })) {
      throw new Error('native Motion surface is unavailable or malformed');
    }
    return value;
  }
  return Object.freeze({ owns: owns, read: read, detachedElementView: detachedElementView,
    detachedExpressionView: detachedExpressionView, expressionSnapshot: expressionSnapshot,
    positionOverlayPlan: positionOverlayPlan, nativeKeyInteractionPlan: nativeKeyInteractionPlan,
    opacityReadModel: opacityReadModel, renderedOpacityRoute: renderedOpacityRoute,
    routeIntent: routeIntent, routeDimension: routeDimension,
    publishWriters: publishWriters, createFrameNavigator: createFrameNavigator,
    createPlaybackScheduler: createPlaybackScheduler,
    advancePlaybackFrame: advancePlaybackFrame,
    requireAvailable: requireAvailable });
}));
