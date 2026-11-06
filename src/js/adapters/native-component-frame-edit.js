/* #2256 A-comp-edit-timeline: the dense editor's frame and key actions INSIDE an entered native component.
 * Baseline: enterSymbol swaps state.layers for the component's layers, so insertFrame / insertKeyframe / convertToKeyframes /
 * insertBlankKeyframe / duplicateKeyframe / extendExposure / clearKeyframe / deleteSelectedFrames / removeTweenSpan / removeFrame(Span)
 * all edit the component unchanged (1065e9d8:src/js/app.js:4130 enterSymbol; the editors read state.layers / state.totalFrames).
 * Native: the intent planner of native-frame-edit-intent.js (the same action grammar, selection rules and payloads as the scene) is run
 * against the component's own layer rows and frame count, the held context id is stamped on the payload, and ONE closed request goes
 * through the component command route (session.runComponentCommand) to the declared native operation: one revision and one history
 * entry per gesture. No document, no Paper object, no fallback writer. Actions with no component route yet (move, repeat, clipboard,
 * layer-bar drag) answer a typed unavailable code and never reach a scene editor. */
(function (root, factory) {
  var commonjs = typeof module === 'object' && module.exports;
  var api = factory(commonjs ? require('./native-frame-edit-intent.js') : root.NemoNativeFrameEditIntent,
    commonjs ? require('./native-frame-edit-contract.js') : root.NemoNativeFrameEditContract,
    commonjs ? require('./native-object-vector-command.js') : root.NemoNativeObjectVectorCommand);
  if (commonjs) module.exports = api;
  else root.NemoNativeComponentFrameEdit = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Intent, Contract, Command) {
  'use strict';
  var LABEL = 'component-frame';
  // The kinds that have a component route: the scene kinds whose native operation accepts a contextId.
  var ROUTED = Object.freeze(['insertFrame', 'insertKey', 'insertKeyAt', 'convertToKeyframes', 'insertBlankKey', 'duplicateKey', 'extendExposure',
    'clearKey', 'deleteFrames', 'removeTween', 'removeFrame', 'removeFrameSpan']);
  var CODES = Object.freeze({ unavailable: 'frame_edit_component_unavailable', session: Intent.CODES.session, invalid: Intent.CODES.invalid, stale: Intent.CODES.stale });
  var P = 'command.document.object.';
  var viewContract = null;

  function unavailable(code, reason) { return Object.freeze({ status: 'unavailable', code: code, reason: reason || null }); }
  function context(host) { var c = host && host.NemoNativeComponentContext; return c && typeof c.presented === 'function' ? c : null; }
  // The component the workspace holds, or null at the scene root.
  function held(host) { var c = context(host); return c ? c.presented(host) : null; }
  function session(host) { try { var project = host.NemoNativeObjectProject; return project ? project.desktop(host) : null; } catch (_) { return null; } }
  // The shared command round trip over the component route, with the frame-edit contract (its byte bound is the 4096 of the public transport).
  function contract() {
    if (!viewContract) {
      viewContract = Object.freeze({
        payload: Contract.payload, result: Contract.result, correlate: Contract.correlate,
        bytes: function (text) { return typeof TextEncoder === 'function' ? new TextEncoder().encode(text).length : text.length; },
        requestByteLimit: Contract.requestByteLimit,
      });
    }
    return viewContract;
  }
  function commands(s) {
    if (typeof s.runComponentCommand !== 'function') return null;
    return Command.create(Object.freeze({ info: s.info, isActive: s.isActive, requestId: s.requestId, runResourceCommand: s.runComponentCommand.bind(s) }), LABEL, contract());
  }
  function failed(error) {
    var code = error && error.code, reason = (error && (error.reason || error.message)) || null;
    if (typeof code !== 'string') return unavailable(CODES.session, reason);
    if (code === 'stale_origin') return unavailable(CODES.stale, reason);
    if (code === 'invalid_request') return unavailable(Intent.CODES.request, reason);
    if (code === 'unavailable' || code === 'request_too_large') return unavailable(CODES.session, reason);
    if (code === 'uncorrelated' || code.endsWith('_command_indeterminate')) return Object.freeze({ status: 'unconfirmed', requestId: error.requestId });
    return Object.freeze({ status: 'refused', code: code, reason: error.reason || null });
  }
  // The editor state, read against the component: rows are its layers, the length is its own frame count.
  function readState(host, rows, total) {
    var s = host.state || {};
    return Intent.readState({
      frame: function () { return s.currentFrame; }, activeLayer: function () { return s.activeLayerIdx; },
      layerSelection: function () { return host._layerSel; }, cellSelection: function () { return host._sel && host._sel.frames ? host._sel.frames : []; },
    }, { layers: rows, totalFrames: total });
  }
  // Where the baseline left the playhead: F5 and Duplicate Key step to the new frame; a removal only clamps (the context refresh already did).
  function hint(operation, frame) { return operation === P + 'frames.insert' || operation === P + 'frames.duplicate' ? frame + 1 : null; }

  async function run(host, value) {
    var c = context(host), here = c ? c.presented(host) : null;
    if (!here) return null; // not inside a component: the scene route decides
    var intent = Intent.action(value);
    if (!intent) return unavailable(CODES.invalid);
    if (ROUTED.indexOf(intent.kind) < 0) return unavailable(CODES.unavailable, intent.kind);
    var s = session(host), rows = c.layers(host), send = s && commands(s);
    if (!s || !send || !rows) return unavailable(CODES.session);
    var state = readState(host, rows, here.totalFrames);
    if (!state) return unavailable(CODES.invalid);
    var step = Intent.plan(intent, state, { layers: rows, totalFrames: here.totalFrames }, null);
    if (step.code) return unavailable(step.code);
    var payload = Object.assign({}, step.payload, { contextId: here.contextId });
    if (!Contract.valid(step.op, payload)) return unavailable(Intent.CODES.request);
    var origin = s.info(), frame = state.frame;
    if (!origin) return unavailable(CODES.session);
    try {
      var receipt = await send.execute(step.op, function (pin) {
        // The component, its revision and its rows are the ones the intent was planned against, or nothing is sent.
        if (pin.contentRevision !== origin.contentRevision || c.contextId(host) !== here.contextId || c.projectionReady(host, pin) !== true) {
          throw Command.fault('stale_origin', 'The presented component is not the one the edit was prepared against', { reason: 'origin_changed' });
        }
        return payload;
      });
      var out = { status: receipt.applied ? 'applied-presented' : 'unchanged', applied: receipt.applied === true };
      var next = receipt.applied ? hint(step.op, frame) : null, after = held(host);
      if (next !== null && after && next < after.totalFrames && typeof host.goToFrame === 'function') {
        try { await host.goToFrame(next); out.playhead = next; } catch (_) { /* the edit is published; only the playhead move failed */ }
      }
      return Object.freeze(out);
    } catch (error) { return failed(error); }
  }
  return Object.freeze({ run: run, held: held, ROUTED: ROUTED, CODES: CODES });
}));
