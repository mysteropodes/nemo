/* TEST PACKAGE ONLY: embedded by the cfg-gated isolated Tauri main-window builder.
 * This transports metadata; O2 supplies the real publication observations.
 * No call here drives a frame, Play, retry, document command or product timer.
 */
(function () {
  'use strict';
  var events = ['ready', 'play-start', 'play-step', 'play-stop', 'navigate-request',
    'navigate-superseded', 'host-request', 'host-receipt', 'receipt-rejected',
    'receipt-deferred', 'receipt-stale', 'receipt-retry', 'presented-return',
    'commit-enter', 'commit-return', 'paint-enter', 'paint-return', 'sync-enter',
    'sync-return', 'navigate-return', 'ui-publication', 'rollback', 'failure'];
  var numbers = ['contentRevision', 'frame', 'lifecycleGeneration', 'viewGeneration',
    'jsLifecycleGeneration', 'hostLifecycleGeneration',
    'sessionOrdinal', 'intent', 'runGeneration', 'hostGeneration', 'currentFrame', 'priorFrame'];
  var uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
  function snapshot(event) {
    if (!event || typeof event !== 'object' || Array.isArray(event)) return null;
    var data = {}, descriptors = Object.getOwnPropertyDescriptors(event);
    for (var key of Reflect.ownKeys(descriptors)) {
      var descriptor = descriptors[key];
      if (!Object.hasOwn(descriptor, 'value')) return null;
      var value = descriptor.value, valid = false;
      if (key === 'event') valid = events.includes(value);
      else if (numbers.includes(key)) valid = Number.isSafeInteger(value) && value >= 0;
      else if (key === 'result') valid = typeof value === 'boolean';
      else if (key === 'instanceId') valid = typeof value === 'string' && uuid.test(value);
      else if (key === 'documentId' || key === 'workId') {
        var prefix = key === 'documentId' ? 'native-document-' : 'native-work-';
        valid = typeof value === 'string' && value.startsWith(prefix) &&
          /^[1-9][0-9]{0,19}$/.test(value.slice(prefix.length));
      } else if (key === 'status') valid = ['presented', 'deferred-occluded', 'deferred-timeout', 'stale-discarded'].includes(value);
      else if (key === 'owner') valid = value === 'native';
      else if (key === 'phase') valid = ['native', 'blocked', 'preparing', 'closed', 'legacy'].includes(value);
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
    var generated = 0, queued = 0, dropped = 0, failed = 0, late = 0;
    var cut = false, listening = false, queue = Promise.resolve();
    function record(event) {
      try {
        if (cut) {
          if (late++ === 0) Promise.resolve().then(function () {
            return ports.invoke('late', { runId: config.runId });
          }).catch(function () { failed++; });
          return;
        }
        generated++;
        if (!listening || queued >= limit) { dropped++; return; }
        var data = snapshot(event);
        if (!data) { dropped++; return; }
        var sequence = generated;
        queued++;
        // The caller never awaits this chain. It serializes diagnostic deliveries only.
        queue = queue.then(async function () {
          try {
            var ack = await ports.invoke('record', { runId: config.runId, sequence: sequence, event: data });
            if (!ack || ack.sequence !== sequence) failed++;
          } catch (_) { failed++; }
        });
      } catch (_) { dropped++; }
    }
    async function seal(event) {
      if (cut || !event || !event.payload || event.payload.runId !== config.runId) return;
      cut = true;
      await queue;
      try {
        await ports.invoke('seal', { runId: config.runId, generated: generated, dropped: dropped, failed: failed });
      } catch (_) { failed++; }
    }
    var ready = Promise.resolve().then(function () { return ports.listen(seal); }).then(async function () {
      listening = true;
      record({ event: 'ready' });
      await queue;
    }).catch(function () { failed++; });
    return Object.freeze({ record: record, ready: ready,
      inspect: function () { return { generated: generated, queued: queued, dropped: dropped, failed: failed, late: late }; } });
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
