/* TEST PACKAGE ONLY: embedded by the cfg-gated isolated Tauri main-window builder.
 * This transports metadata; O2 supplies the real publication observations.
 * No call here drives a frame, Play, retry or document command. The only timer hook is the
 * opt-in P24 fixed clock (config.fixedClock === true, from NEMO_TEST_PLAY_FIXED_CLOCK=1): the
 * product's own playback scheduler asks it for time, and it steps exactly one frame period per
 * accepted tick, never while a presentation is outstanding. Absent, product time is untouched.
 */
(function () {
  'use strict';
  var events = ['ready', 'play-start', 'play-step', 'play-stop', 'play-state', 'navigate-request',
    'navigate-superseded', 'host-request', 'host-receipt', 'receipt-rejected',
    'receipt-deferred', 'receipt-stale', 'receipt-retry', 'presented-return',
    'commit-enter', 'commit-return', 'paint-enter', 'paint-return', 'sync-enter',
    'sync-return', 'navigate-return', 'ui-publication', 'rollback', 'failure'];
  var numbers = ['contentRevision', 'frame', 'lifecycleGeneration', 'viewGeneration',
    'jsLifecycleGeneration', 'hostLifecycleGeneration',
    'sessionOrdinal', 'hostCallOrdinal', 'intent', 'runGeneration', 'hostGeneration', 'currentFrame', 'priorFrame'];
  var statuses = ['presented', 'deferred-occluded', 'deferred-timeout', 'stale-discarded',
    'failed-validation', 'failed-device-lost', 'failed-retry-lost', 'failed-retry-outdated', 'failed-recreate', 'failed-reconfigure'];
  var objectDocument = 'native-object-document-';
  var identifier = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;
  var uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
  function snapshot(event) {
    if (!event || typeof event !== 'object' || Array.isArray(event)) return null;
    var data = {}, descriptors = Object.getOwnPropertyDescriptors(event);
    for (var key of Reflect.ownKeys(descriptors)) {
      var descriptor = descriptors[key];
      if (!Object.hasOwn(descriptor, 'value')) return null;
      var value = descriptor.value, valid = false;
      if (key === 'event') valid = events.includes(value);
      else if (numbers.includes(key)) valid = Number.isSafeInteger(value) && value >= (key === 'hostCallOrdinal' ? 1 : 0);
      else if (key === 'result' || key === 'playing') valid = typeof value === 'boolean';
      else if (key === 'documentSnapshotId' || key === 'contextId') valid = typeof value === 'string' && identifier.test(value) && !value.endsWith('\n');
      else if (key === 'instanceId') valid = typeof value === 'string' && uuid.test(value);
      else if (key === 'documentId' || key === 'workId') {
        // Opacity projects publish 'native-document-N'; native object projects publish 'native-object-document-N'.
        var prefix = key === 'workId' ? 'native-work-' : typeof value === 'string' && value.startsWith(objectDocument) ? objectDocument : 'native-document-';
        valid = typeof value === 'string' && value.startsWith(prefix) &&
          /^[1-9][0-9]{0,19}$/.test(value.slice(prefix.length)) && !value.endsWith('\n') &&
          BigInt(value.slice(prefix.length)) <= 18446744073709551615n;
      } else if (key === 'status') valid = statuses.includes(value);
      else if (key === 'owner') valid = value === 'native';
      else if (key === 'phase') valid = ['native', 'blocked', 'preparing', 'closed', 'legacy', 'installing',
        'release-requested', 'releasing', 'replacing', 'indeterminate'].includes(value);
      else if (key === 'code') valid = ['native_preview_deferred', 'native_preview_superseded', 'observer_refused', 'unknown'].includes(value);
      if (!valid) return null;
      data[key] = value;
    }
    return events.includes(data.event) && new TextEncoder().encode(JSON.stringify(data)).length <= 1024 ? data : null;
  }
  function createObserver(config, ports, limit) {
    if (!config || !uuid.test(config.runId) || !ports || typeof ports.invoke !== 'function' || typeof ports.listen !== 'function') {
      throw new TypeError('test observer unavailable');
    }
    limit = Number.isInteger(limit) && limit > 0 && limit <= 4096 ? limit : 4096;
    var generated = 0, queued = 0, dropped = 0, failed = 0, late = 0, pendingCount = 0, outsideInterval = 0;
    var cut = false, retired = false, listening = false, queue = Promise.resolve();
    var sessions = new WeakMap(), adapters = new WeakMap(), receipts = new WeakMap(), calls = new WeakMap();
    var sessionSequence = 0, callSequence = 0;
    var navigations = new WeakMap(), publications = new WeakMap(), steps = new WeakMap(), publication = null;
    var productPendingNavigation = 0, productPendingPlayback = 0, productPlaying = false;
    var failures = new WeakMap();
    var restorations = new WeakMap();
    var failurePins = ['instanceId','documentId','contentRevision','documentSnapshotId','contextId','frame',
      'jsLifecycleGeneration','hostGeneration','sessionOrdinal','hostCallOrdinal'];
    function rememberFailure(error, captured) {
      if (!error || typeof error !== 'object' || !captured || failurePins.some(function (key) { return captured[key] === undefined; })) return false;
      if (failures.has(error)) {
        var prior = failures.get(error);
        if (!prior) return false;
        if (failurePins.concat(['workId','viewGeneration','status']).some(function (key) { return prior[key] !== captured[key]; })) {
          failures.set(error, null); return false; // The SAME rejected object reused by another call is ambiguous.
        }
        return true;
      }
      failures.set(error, captured); return true;
    }
    function associateFailure(receipt, error) {
      if (!rememberFailure(error, receipts.get(receipt))) dropped++;
    }
    function associateNavigationError(ticket, error, receipt) {
      var nav = navigations.get(ticket);
      if (nav && receipt !== undefined) {
        var current = receipts.get(receipt);
        if (!current || ['instanceId','documentId','contentRevision','frame'].some(function (key) { return nav.pins[key] !== current[key]; })) {
          nav.failureFenced = true; dropped++; return;
        }
        nav.currentReceiptContext = current;
        if (!rememberFailure(error, current)) { nav.failureFenced = true; dropped++; }
        return;
      }
      if (nav && nav.pins.hostCallOrdinal && error && typeof error === 'object') {
        if (!rememberFailure(error, nav.pins)) dropped++;
      }
    }
    function navigationFailure(ticket, error) {
      var nav = navigations.get(ticket); if (!nav || nav.finished) return;
      var captured = error && typeof error === 'object' ? failures.get(error) : null;
      var basis = nav.currentReceiptContext || nav.pins;
      var matching = basis.hostCallOrdinal ? failurePins.concat(['workId','viewGeneration','status']) : ['instanceId','documentId','contentRevision','frame'];
      if (!nav.failureFenced && captured && captured.hostCallOrdinal > nav.afterCallOrdinal && matching.every(function (key) { return basis[key] === captured[key]; })) {
        nav.pins = Object.freeze(Object.assign({}, captured, { intent: nav.pins.intent }));
      } else dropped++;
      record(Object.assign({ event: 'failure', code: 'unknown' }, nav.pins));
    }
    function restoreRequested(ticket, priorFrame, error, currentOwner) {
      var nav = navigations.get(ticket), cause = error && typeof error === 'object' ? failures.get(error) : null;
      var token = Object.freeze({});
      var valid = nav && !nav.finished && !nav.failureFenced && cause && cause.hostCallOrdinal > nav.afterCallOrdinal &&
        restoreWork(cause) &&
        ['instanceId','documentId','contentRevision','frame'].every(function (key) { return nav.pins[key] === cause[key]; }) &&
        Number.isSafeInteger(priorFrame) && priorFrame >= 0;
      if (!valid) dropped++;
      var pins = Object.freeze(Object.assign({}, valid ? cause : nav && nav.pins, { intent: nav && nav.pins.intent, priorFrame: priorFrame }));
      restorations.set(token, { nav:nav, cause:valid ? cause : null, pins:pins, prior:priorFrame, afterCallOrdinal:callSequence, currentOwner:currentOwner, valid:!!valid, painted:false, finished:false });
      record(Object.assign({ event:'receipt-retry', result:false }, pins)); return token;
    }
    function restorePresented(token, receipt) {
      var restore = restorations.get(token); if (!restore || restore.finished) { dropped++; return; }
      var captured = receipts.get(receipt);
      var matching = ['instanceId','documentId','contentRevision','documentSnapshotId','contextId','jsLifecycleGeneration','hostGeneration','sessionOrdinal'];
      if (!restore.valid || !captured || !restoreWork(captured) || captured.status !== 'presented' || captured.frame !== restore.prior ||
          captured.hostCallOrdinal <= restore.afterCallOrdinal ||
          captured.hostCallOrdinal <= restore.cause.hostCallOrdinal || !matching.every(function (key) { return captured[key] === restore.cause[key]; }) ||
          !restoreOwner(restore)) {
        restore.valid=false; dropped++; return;
      }
      restore.pins = Object.freeze(Object.assign({}, captured, { intent:restore.nav.pins.intent, priorFrame:restore.prior }));
      publications.set(token, { pins:restore.pins, previous:publication, input:null, rollback:true, restoration:true }); publication=token;
      record(Object.assign({ event:'receipt-retry', result:true }, restore.pins));
    }
    function restorePainted(token) {
      var restore=restorations.get(token); if (!restore || !restore.valid || !publications.has(token)) { dropped++; return; }
      restore.painted=true;
    }
    function restoreCompleted(token) {
      var restore=restorations.get(token), current=publications.get(token);
      if (!restore || restore.finished) { dropped++; return; } restore.finished=true;
      if (!restore.valid || !restore.painted || !current || current.input !== restore.prior || current.agrees !== true ||
          !restoreOwner(restore)) { dropped++; return; }
      record(Object.assign({}, restore.pins, { event:'rollback', result:true, currentFrame:current.input, frame:current.windowFrame }));
    }
    function restoreRejected(token) {
      var restore=restorations.get(token); if (!restore || restore.finished) return; restore.finished=true; dropped++;
      record(Object.assign({}, restore.pins, { event:'rollback', result:false }));
    }
    function restoreOwner(restore) { try { return typeof restore.currentOwner === 'function' && restore.currentOwner() === true; }
      catch (_) { return false; } }
    function restoreWork(pins) { return typeof pins.workId === 'string' && /^native-work-[1-9][0-9]{0,19}$/.test(pins.workId) &&
      BigInt(pins.workId.slice(12)) <= 18446744073709551615n && Number.isSafeInteger(pins.viewGeneration) && pins.viewGeneration > 0; }
    function navigationRequest(intent, frame, identity) {
      var ticket = Object.freeze({}), pins = Object.freeze(Object.assign({ intent: intent, frame: frame }, identity));
      navigations.set(ticket, { pins: pins, finished: false, afterCallOrdinal: callSequence }); productPendingNavigation++;
      record(Object.assign({ event: 'navigate-request' }, pins)); return ticket;
    }
    function navigationSuperseded(ticket) {
      var nav = navigations.get(ticket); if (nav) { nav.superseded = true; record(Object.assign({ event: 'navigate-superseded' }, nav.pins)); }
    }
    function navigationReturn(ticket, result) {
      var nav = navigations.get(ticket); if (!nav || nav.finished) return;
      nav.finished = true; productPendingNavigation--;
      if (!nav.superseded && !nav.pins.hostCallOrdinal) dropped++;
      record(Object.assign({ event: 'navigate-return', result: result === true }, nav.pins));
    }
    function beginPublication(ticket, receipt) {
      var nav = navigations.get(ticket), captured = receipts.get(receipt);
      if (!nav || !captured) { dropped++; return; }
      if (['instanceId','documentId','contentRevision','frame'].some(function (key) { return nav.pins[key] !== captured[key]; })) dropped++;
      nav.pins = Object.freeze(Object.assign({}, captured, { intent: nav.pins.intent }));
      var next = Object.freeze({}); publications.set(next, { pins: nav.pins, previous: publication, input: null, rollback: false });
      publication = next; return next;
    }
    function beginRollback(ticket) {
      var nav = navigations.get(ticket); if (!nav) return;
      var next = Object.freeze({}); publications.set(next, { pins: nav.pins, previous: publication, rollback: true });
      publication = next; return next;
    }
    function endPublication(ticket) {
      var current = publications.get(ticket); if (current && publication === ticket) publication = current.previous;
    }
    function uiPublication(actual, windowFrame) {
      var current = publications.get(publication); if (!current) return;
      if (!current.rollback && (actual !== current.pins.frame || windowFrame !== actual)) dropped++;
      record(Object.assign({}, current.pins, { event: current.rollback ? 'rollback' : 'ui-publication', currentFrame: actual, frame: windowFrame }));
    }
    function inputPublication(value, actual, windowFrame) {
      var current = publications.get(publication); if (!current || current.rollback && !current.restoration) return;
      if (typeof value !== 'number' && typeof value !== 'string') { dropped++; return; }
      var displayed = Number(value); if (!Number.isSafeInteger(displayed) || displayed < 1) { dropped++; return; }
      current.input = displayed - 1; current.windowFrame = windowFrame;
      current.agrees = actual === current.input && windowFrame === current.input;
      if (current.input !== current.pins.frame || !current.agrees) dropped++;
    }
    function publicationStage(ticket, event) {
      var current = publications.get(ticket); if (!current) return;
      var observed = Object.assign({ event: event }, current.pins);
      if (event === 'sync-return') {
        if (current.input === null) dropped++;
        else { observed.currentFrame = current.input; observed.frame = current.windowFrame; observed.result = current.agrees; }
      }
      record(observed);
    }
    function schedulerRequest(run, frame, identity) {
      var ticket = Object.freeze({}), pins = Object.freeze(Object.assign({ runGeneration: run, frame: frame }, identity));
      steps.set(ticket, { pins: pins, finished: false }); productPendingPlayback++;
      record(Object.assign({ event: 'play-step' }, pins)); return ticket;
    }
    function schedulerReturn(ticket, result) {
      var step = steps.get(ticket); if (!step || step.finished) return;
      step.finished = true; productPendingPlayback--; if (typeof result !== 'boolean') dropped++;
      record(Object.assign({ event: 'play-step', result: result === true }, step.pins));
    }
    function schedulerState(event, run, identity) { if (identity) record(Object.assign({ event: event, runGeneration: run }, identity)); }
    function playState(playing, frame) {
      productPlaying = playing === true; record({ event: 'play-state', playing: playing, currentFrame: frame });
    }
    function scalarReceipt(raw) {
      var result = {};
      if (!raw || typeof raw !== 'object') return result;
      ['workId', 'viewGeneration', 'status'].forEach(function (key) {
        var property = Object.getOwnPropertyDescriptor(raw, key);
        if (property && Object.hasOwn(property, 'value') && snapshot(Object.assign({ event: 'host-receipt' }, { [key]: property.value }))) result[key] = property.value;
        else dropped++; // Never retain an object/array/private string in a raw scalar slot.
      });
      return result;
    }
    function bindPreview(raw, session, hostGeneration) {
      try {
        if (!sessions.has(session)) sessions.set(session, ++sessionSequence);
        adapters.set(raw, { session: session, hostGeneration: hostGeneration });
      } catch (_) { dropped++; }
    }
    function beginHost(raw, context, meta) {
      try {
        var binding = adapters.get(raw), host = binding && binding.hostGeneration();
        var captured = { instanceId: context.instanceId, documentId: meta.documentId,
          contentRevision: meta.contentRevision, documentSnapshotId: meta.documentSnapshotId,
          contextId: meta.contextId, frame: meta.frame, jsLifecycleGeneration: context.lifecycleGeneration,
          hostCallOrdinal: ++callSequence };
        if (binding && Number.isSafeInteger(host) && host > 0) {
          captured.sessionOrdinal = sessions.get(binding.session); captured.hostGeneration = host;
        } else dropped++; // Missing native/session attribution makes the whole interval partial.
        var ticket = Object.freeze({}); calls.set(ticket, { captured: Object.freeze(captured), raw: null });
        record(Object.assign({ event: 'host-request' }, captured)); return ticket;
      } catch (_) { dropped++; }
    }
    function rawReceipt(ticket, raw) {
      try { var call = calls.get(ticket); if (!call) { dropped++; return; }
        call.raw = Object.freeze(scalarReceipt(raw));
        record(Object.assign({ event: 'host-receipt' }, call.captured, call.raw));
      } catch (_) { dropped++; }
    }
    function associateReceipt(ticket, receipt) {
      try { var call = calls.get(ticket); if (call) receipts.set(receipt,
        Object.freeze(Object.assign({}, call.captured, scalarReceipt(receipt)))); else dropped++;
      } catch (_) { dropped++; }
    }
    function rejectReceipt(ticket, error) {
      try { var call = calls.get(ticket); if (call) {
        if (!rememberFailure(error, Object.freeze(Object.assign({}, call.captured, call.raw)))) dropped++;
        record(Object.assign({ event: 'receipt-rejected', code: 'unknown' }, call.captured));
      } else dropped++; }
      catch (_) { dropped++; }
    }
    function presentedReturn(source, result) {
      try { var captured = receipts.get(source); if (!captured) { dropped++; return; }
        receipts.set(result, captured); record(Object.assign({ event: 'presented-return' }, captured));
      } catch (_) { dropped++; }
    }
    function record(event) {
      try {
        // The final fence ends this bounded capture; these attempts are outside it.
        if (retired) { outsideInterval++; return; }
        if (cut) {
          // Count the unrecorded attempt synchronously, even if IPC is lost.
          generated++; dropped++;
          if (late++ === 0) {
            pendingCount++;
            queue = queue.then(async function () {
              try { await ports.invoke('late', { runId: config.runId }); }
              catch (_) { failed++; }
              finally { pendingCount--; }
            });
          }
          return;
        }
        generated++;
        if (!listening || queued >= limit) { dropped++; return; }
        var data = snapshot(event);
        if (!data) { dropped++; return; }
        var sequence = generated;
        queued++; pendingCount++;
        // The caller never awaits this chain. It serializes diagnostic deliveries only.
        queue = queue.then(async function () {
          try {
            var ack = await ports.invoke('record', { runId: config.runId, sequence: sequence, event: data });
            if (!ack || ack.sequence !== sequence) failed++;
          } catch (_) { failed++; }
          finally { pendingCount--; }
        });
      } catch (_) { dropped++; }
    }
    async function drain() {
      // Post-cut diagnostics can extend the chain while an earlier ACK is pending.
      var pending;
      do { pending = queue; await pending; } while (pending !== queue);
    }
    async function seal(event) {
      if (cut || !event || !event.payload || event.payload.runId !== config.runId) return;
      if (productPlaying || productPendingNavigation || productPendingPlayback) dropped++;
      cut = true;
      await drain();
      var first = { runId: config.runId, generated: generated, dropped: dropped, failed: failed }, acknowledged;
      try {
        acknowledged = await ports.invoke('seal', first);
        if (!acknowledged || acknowledged.kind !== 'seal' || acknowledged.status !== 'provisional' ||
            acknowledged.runId !== config.runId || !uuid.test(acknowledged.cutId) ||
            acknowledged.generated !== first.generated || acknowledged.dropped !== first.dropped ||
            acknowledged.failed !== first.failed) throw new Error('test observer cut acknowledgement refused');
      } catch (_) { failed++; return; }
      await drain();
      // Synchronous retirement freezes final counters before sending the host fence.
      // The driver must have finished all relevant product work before asking to seal.
      retired = true;
      try {
        await ports.invoke('finish', { runId: config.runId, cutId: acknowledged.cutId,
          generated: generated, dropped: dropped, failed: failed, pending: pendingCount });
      } catch (_) { failed++; }
    }
    // Virtual time for the product scheduler (P24). The 1e-6 ms bias keeps floor((now-clock)/frameMs)
    // at exactly 1 despite float rounding; ticks wait while a scheduler step is still presenting.
    var fixedClock = config.fixedClock !== true ? null : (function () {
      var t = 0, ids = 0, timers = new Map();
      function request(step, frameMs) {
        var id = ++ids;
        function fire() {
          timers.delete(id);
          if (productPendingPlayback > 0) { timers.set(id, setTimeout(fire, 1)); return; }
          t += (frameMs > 0 ? frameMs : 1000 / 24) + 1e-6; step(t);
        }
        timers.set(id, setTimeout(fire, 0)); return id;
      }
      function cancel(id) { clearTimeout(timers.get(id)); timers.delete(id); }
      return Object.freeze({ now: function () { return t; }, request: request, cancel: cancel });
    })();
    var ready = Promise.resolve().then(function () { return ports.listen(seal); }).then(async function () {
      listening = true;
      record({ event: 'ready' });
      await queue;
    }).catch(function () { failed++; });
    return Object.freeze({ fixedClock: fixedClock, record: record, ready: ready, bindPreview: bindPreview, beginHost: beginHost,
      rawReceipt: rawReceipt, associateReceipt: associateReceipt, rejectReceipt: rejectReceipt,
      presentedReturn: presentedReturn, receiptContext: function (receipt) { return receipts.get(receipt); },
      navigationRequest: navigationRequest, navigationSuperseded: navigationSuperseded, navigationReturn: navigationReturn,
      associateFailure: associateFailure, associateNavigationError: associateNavigationError, navigationFailure: navigationFailure,
      restoreRequested:restoreRequested, restorePresented:restorePresented, restorePainted:restorePainted, restoreCompleted:restoreCompleted, restoreRejected:restoreRejected,
      errorContext: function (error) { return error && typeof error === 'object' ? failures.get(error) : undefined; },
      beginPublication: beginPublication, beginRollback: beginRollback, endPublication: endPublication,
      uiPublication: uiPublication, inputPublication: inputPublication, publicationStage: publicationStage,
      schedulerRequest: schedulerRequest, schedulerReturn: schedulerReturn, schedulerState: schedulerState, playState: playState,
      inspect: function () { return { generated: generated, queued: queued, dropped: dropped, failed: failed, late: late,
        pending: pendingCount, outsideInterval: outsideInterval, productPendingNavigation: productPendingNavigation,
        productPendingPlayback: productPendingPlayback, productPlaying: productPlaying,
        phase: retired ? 'retired' : cut ? 'sealed' : 'collecting' }; } });
  }
  if (typeof module === 'object' && module.exports) module.exports = { createObserver: createObserver };
  if (typeof window === 'undefined') return;
  var config = window.__NEMO_PLAY_OBSERVER_CONFIG__;
  delete window.__NEMO_PLAY_OBSERVER_CONFIG__;
  try {
    var url = new URL(window.location.href), tauri = window.__TAURI__;
    if (window.top !== window || url.protocol !== 'tauri:' || url.hostname !== 'localhost' ||
        url.port !== '' || url.username !== '' || url.password !== '' ||
        !['', '/', '/index.html'].includes(url.pathname) || !config || !tauri || !tauri.core || !tauri.event) return;
    window.NemoTestPlayObserver = createObserver(config, {
      invoke: function (action, payload) { return tauri.core.invoke('nemo_test_play_observer', { action: action, payload: payload }); },
      listen: function (callback) { return tauri.event.listen('nemo-test-play-seal', callback); }
    });
  } catch (_) { /* No available collector is an unready test, never product failure. */ }
})();
