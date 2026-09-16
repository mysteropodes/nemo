// The Labs timelapse feature's own capability registration (P31/#1033).
//
// Bounded to a descriptor and its registration contract, not a bootstrap
// integration: src/js/labs/timelapse.js already implements
// SMLabs.timelapseStart/timelapseStop/register('timelapse',...), and nothing
// here changes that module's behavior. Wiring this into the live
// NemoApplication.handle dispatch (as opacity and export-job are) is
// bootstrap-layer work outside this leaf's bounded source ownership.
//
// This synchronous classic-script projection is kept byte-for-value
// equivalent to engineering/application/capabilities/timelapse.json by the
// focused contract test, same convention as opacity-capability.js.
var NemoTimelapseCapability = (function () {
  'use strict';

  var MIME_CANDIDATES = ['video/webm;codecs=vp9', 'video/webm;codecs=vp8', 'video/webm'];

  function loaded(win) {
    return !!(win && win.SMLabs && typeof win.SMLabs.timelapseStart === 'function'
      && typeof win.SMLabs.timelapseStop === 'function' && typeof win.SMLabs.isOn === 'function');
  }

  function mediaRecorderSupported(win) {
    var mr = win && win.MediaRecorder;
    return !!(mr && MIME_CANDIDATES.some(function (m) { return mr.isTypeSupported && mr.isTypeSupported(m); }));
  }

  // Live environment check for a caller to pass into register()'s optional
  // third argument at actual boot time. NOT re-evaluated after registration:
  // the registry (P05/#1007) has no path to update a descriptor once
  // registered, and adding one is outside this leaf's bounded scope. A labs
  // flag toggled by the user after boot leaves a registered claim stale
  // until reload -- a documented limitation, not a silently assumed one.
  function computeAvailability(win) {
    if (!loaded(win)) return { state: 'unavailable', reason: 'missing-dependency' };
    if (!mediaRecorderSupported(win)) return { state: 'unavailable', reason: 'platform-unsupported' };
    if (!win.SMLabs.isOn('timelapse')) return { state: 'unavailable', reason: 'feature-flag-off' };
    return { state: 'available', reason: null };
  }

  var DESCRIPTOR = {
    schemaVersion: 1,
    id: 'timelapse',
    version: 1,
    handlerKey: 'labs.timelapse.recording',
    input: {
      type: 'object',
      additionalProperties: false,
      properties: {
        fps: { type: 'integer', minimum: 1, maximum: 60,
          description: 'Capture rate for the "start" stage; ignored by "stop". Clamped 1-60 by timelapseStart; defaults to 30 when omitted.' },
      },
    },
    output: {
      type: 'object',
      additionalProperties: false,
      properties: {
        started: { type: 'boolean', description: 'Present on a successful "start".' },
        seconds: { type: 'number', minimum: 0, description: 'Present on a successful "stop": recording duration.' },
        bytes: { type: 'number', minimum: 0, description: 'Present on a successful "stop": recorded blob size.' },
      },
    },
    units: { fps: 'fps', seconds: 'seconds', bytes: 'bytes' },
    effects: { kind: 'job', scope: 'none', lifecycle: ['start', 'stop'] },
    // Static baseline: what a plain require() with no window/SMLabs computes.
    // A live caller passes computeAvailability(window) into register()'s
    // third argument instead of trusting this fixed claim.
    availability: { state: 'unavailable', reason: 'missing-dependency' },
    fixture: {
      label: 'start a timelapse recording at the default rate',
      input: {},
      output: { started: true },
    },
    examples: [
      { label: 'start a timelapse recording at the default rate', input: {}, output: { started: true } },
      { label: 'start at a custom capture rate', input: { fps: 15 }, output: { started: true } },
      { label: 'stop and receive the recorded duration and size', input: {}, output: { seconds: 12, bytes: 483920 } },
    ],
  };

  // Minimal request/response shape ({operation,payload} -> {ok,result} /
  // {ok:false,error:{code,message}}), matching export-job.js's convention --
  // the closest already-merged sibling (job kind, scope:none) -- rather than
  // opacity's document-revision envelope, which does not apply here (no
  // document, no instance/revision identity). Never reaches into rec/chunks/
  // comp directly: those stay private to timelapse.js's own closure, exactly
  // as before this file existed.
  function handlerFor(win) {
    return function (request) {
      var operation = request && request.operation;
      var payload = (request && request.payload) || {};
      if (operation === 'start') {
        var started = win.SMLabs.timelapseStart(payload.fps);
        if (!started) {
          return { ok: false, error: { code: 'start-refused',
            message: 'timelapseStart returned false (already recording, no capturable canvas, or capture unsupported here); see console.warn for the specific reason.' } };
        }
        return { ok: true, result: { started: true } };
      }
      if (operation === 'stop') {
        var pending = win.SMLabs.timelapseStop();
        if (pending === null) return { ok: false, error: { code: 'not-recording', message: 'timelapseStop: nothing to stop.' } };
        return pending.then(function (r) { return { ok: true, result: { seconds: r.seconds, bytes: r.bytes } }; });
      }
      return { ok: false, error: { code: 'unknown_operation', message: 'Unsupported operation: ' + operation } };
    };
  }

  // availability defaults to the static baseline; a live caller passes
  // computeAvailability(window) explicitly. registry.register (P05) throws
  // on a duplicate id or an "available" claim with no bound handler -- both
  // real guarantees this file leans on rather than re-checking.
  function register(registry, handler, availability) {
    var descriptor = Object.assign({}, DESCRIPTOR, { availability: availability || DESCRIPTOR.availability });
    return registry.register(descriptor, handler);
  }

  return { DESCRIPTOR: DESCRIPTOR, computeAvailability: computeAvailability, handlerFor: handlerFor, register: register };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoTimelapseCapability;
