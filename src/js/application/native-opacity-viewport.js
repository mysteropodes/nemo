/* Session-keyed native viewport resize and presented-frame scheduling. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeOpacityViewport = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  function create(lifecycle, ports, isOpening, isPublished) {
    var resize = null, resizing = null, presenting = null, lastPresentedFrame = null;

    function disposeSession(session) {
      if (resize && resize.session === session) {
        if (resize.timer !== null) ports.surface.cancel(resize.timer);
        resize = null;
      }
      lastPresentedFrame = null;
    }

    function applyResize(job) {
      if (job.work) return job.work;
      if (job.timer !== null) ports.surface.cancel(job.timer);
      job.timer = null;
      var prior = resizing && resizing.session === job.session ? resizing.work : null;
      var priorPresentation = presenting;
      job.work = (async function () {
        if (prior) await prior;
        if (priorPresentation) {
          try { await priorPresentation; }
          catch (error) {
            // A prior frame can be superseded by a native revision while this
            // resize waits. Join that revision before resizing its new identity.
            if (error && error.code === 'native_preview_superseded') await lifecycle.flush();
            else throw error;
          }
        }
        var latest = lifecycle.inspect(), currentToken;
        if (latest.session !== job.session) return false;
        try { currentToken = lifecycle.getNativeIdentity(); } catch (_) { return false; }
        if (!currentToken || currentToken.documentId !== job.token.documentId ||
            currentToken.generation !== job.token.generation) return false;
        await ports.surface.resize(lifecycle.identity());
        return true;
      })().catch(function (error) {
        if (lifecycle.inspect().session === job.session) lifecycle.fence(error);
        throw error;
      }).finally(function () {
        if (resizing === job) resizing = null;
        if (resize === job) resize = null;
      });
      resizing = job;
      return job.work;
    }

    async function settleResize(session) {
      // A receipt cannot precede a scheduled or already-running host resize.
      while ((resizing && resizing.session === session) || (resize && resize.session === session)) {
        if (resizing && resizing.session === session) {
          var active = resizing;
          active.joined = true;
          await active.work;
        } else {
          var pending = resize;
          pending.joined = true;
          await applyResize(pending);
        }
      }
    }

    async function presentPreview(frame, allowOccludedAdmission) {
      if (!lifecycle.isActive()) throw new Error('native opacity authority is not active');
      var observed = lifecycle.inspect(), current = lifecycle.identity();
      var retryable = false;
      try {
        var staleRetries = 0;
        for (;;) {
          await settleResize(observed.session);
          var work = lifecycle.presentPreview(frame);
          presenting = work;
          var presented;
          try { presented = await work; }
          finally { if (presenting === work) presenting = null; }
          var latest = lifecycle.inspect();
          if (!['presented', 'deferred-occluded', 'deferred-timeout', 'stale-discarded'].includes(presented.status) ||
              presented.frame !== frame ||
              presented.lifecycleGeneration !== observed.generation ||
              presented.instanceId !== current.instanceId ||
              presented.documentId !== current.documentId ||
              presented.contentRevision !== current.contentRevision ||
              latest.phase !== 'native' || latest.session !== observed.session ||
              latest.generation !== observed.generation ||
              !latest.identity || latest.identity.instanceId !== current.instanceId ||
              latest.identity.documentId !== current.documentId ||
              latest.identity.contentRevision !== current.contentRevision ||
              typeof lifecycle.persistenceJSON() !== 'string') {
            throw new Error('native opacity frame was not presented at the admitted revision');
          }
          // Deferred native work remains keyed by evaluation until terminal.
          // After a newer view presents, revisiting it retires the stale work;
          // one new request must prove a fresh presentation before publication.
          if (presented.status === 'stale-discarded' &&
              typeof isPublished === 'function' && isPublished(observed.session)) {
            if (staleRetries++ === 0) continue;
            var stale = new Error('native opacity frame presentation remains stale');
            stale.code = 'native_preview_superseded';
            throw stale;
          }
          // A resize can arrive while the host presents. Finish it and retry.
          if (presented.status === 'presented' &&
              ((resize && resize.session === observed.session) ||
               (resizing && resizing.session === observed.session))) continue;
          // The start screen can occlude the native layer before reveal. This
          // receipt admits only the handoff, never an Open success; reveal must
          // obtain a fresh, strictly presented receipt at the same identity.
          if (presented.status === 'deferred-occluded' && allowOccludedAdmission && isOpening()) {
            return Object.freeze(Object.assign({ owner: 'native' }, presented));
          }
          if (presented.status !== 'presented') {
            if (typeof isPublished === 'function' && isPublished(observed.session)) {
              var deferred = new Error('native opacity frame presentation is deferred');
              deferred.code = 'native_preview_deferred';
              deferred.status = presented.status;
              retryable = true;
              throw deferred;
            }
            throw new Error('native opacity frame was not presented at the admitted revision');
          }
          lastPresentedFrame = frame;
          return Object.freeze(Object.assign({ owner: 'native' }, presented));
        }
      } catch (error) {
        var failed = lifecycle.inspect();
        if (!retryable && (!error || error.code !== 'native_preview_superseded') &&
            failed.phase === 'native' && failed.session === observed.session) lifecycle.fence(error);
        throw error;
      }
    }

    function resizeViewport() {
      var observed = lifecycle.inspect(), token;
      try { token = lifecycle.getNativeIdentity(); } catch (_) { return; }
      if (!token || !observed.session) return;
      if (resize && resize.timer !== null) ports.surface.cancel(resize.timer);
      var job = { session: observed.session, token: token, timer: null, work: null, joined: false };
      resize = job;
      job.timer = ports.surface.defer(async function () {
        if (resize !== job || lifecycle.inspect().session !== job.session) return;
        try {
          if (await applyResize(job) && !job.joined &&
              lifecycle.inspect().session === job.session && lastPresentedFrame !== null) {
            await presentPreview(lastPresentedFrame);
          }
        } catch (_) { /* The current session was fenced by the failing operation. */ }
      }, 50);
    }

    function renderPreview(frame) {
      // Admission's afterChange sees the prior document's frame. First-open
      // presentation selects frame zero explicitly after native activation.
      if (isOpening()) return true;
      var handled = lifecycle.renderPreview(frame);
      if (handled === true && lifecycle.isActive()) lastPresentedFrame = frame;
      return handled;
    }

    return Object.freeze({ disposeSession: disposeSession,
      reset: function () { lastPresentedFrame = null; },
      presentPreview: presentPreview, resizeViewport: resizeViewport,
      renderPreview: renderPreview });
  }

  return Object.freeze({ create: create });
}));
