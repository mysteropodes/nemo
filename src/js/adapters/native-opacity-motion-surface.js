/* Stateless Motion/native compatibility; callers retain no mutable authority here. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeOpacityMotionSurface = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var UNHANDLED = Object.freeze({ handled: false });
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
  // Capture ownership while the row is built from a readable revision.
  // A rapid native scrub deliberately opens a cache fence before this
  // same DOM callback returns; re-querying prepared() from that callback
  // would then throw and drop its next no-await value.
  function renderedOpacityRoute(controller, holder, prop, frame) {
    if (prop !== 'opacity') return UNHANDLED;
    var current = preparedFor(controller, holder);
    if (!current) return UNHANDLED;
    var layer = current.projection.layers[0], track = layer.motion && layer.motion.opacity;
    var keys = track && track.keys || [];
    return current ? freeze({ handled: true, layerUid: current.layerUid,
      property: 'opacity', opacityMode: current.opacityMode, animated: keys.length > 0,
      keyAtFrame: keys.some(function (key) { return key.frame === frame; }) }) : UNHANDLED;
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
  function requireAvailable(value) {
    var methods = ['owns', 'read', 'detachedElementView', 'detachedExpressionView', 'expressionSnapshot',
      'positionOverlayPlan', 'nativeKeyInteractionPlan', 'renderedOpacityRoute', 'routeIntent', 'routeDimension', 'publishWriters'];
    if (!value || !Object.isFrozen(value) || methods.some(function (name) { return typeof value[name] !== 'function'; })) {
      throw new Error('native Motion surface is unavailable or malformed');
    }
    return value;
  }
  return Object.freeze({ owns: owns, read: read, detachedElementView: detachedElementView,
    detachedExpressionView: detachedExpressionView, expressionSnapshot: expressionSnapshot,
    positionOverlayPlan: positionOverlayPlan, nativeKeyInteractionPlan: nativeKeyInteractionPlan,
    renderedOpacityRoute: renderedOpacityRoute, routeIntent: routeIntent, routeDimension: routeDimension,
    publishWriters: publishWriters, requireAvailable: requireAvailable });
}));
