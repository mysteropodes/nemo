/* Offline P38S2A contract only. A future owned initialization-script closure
 * consumes this expression; it installs no global API or product include. */
(function () {
  'use strict';
  const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
  const MAX = 15;
  function createStartupProbe(config, ports) {
    let identity, win, doc;
    try {
      if (!config) throw Error();
      const enabled = config.enabled, runId = config.runId, loadGeneration = config.loadGeneration;
      if (enabled !== true || typeof runId !== 'string' || !UUID.test(runId)
        || !Number.isSafeInteger(loadGeneration) || loadGeneration < 1
        || loadGeneration > 2147483647) throw Error();
      identity = Object.freeze({ runId, loadGeneration });
      win = ports.window; doc = ports.document;
      if (win.top !== win || win.document !== doc || typeof ports.emit !== 'function'
        || typeof ports.requestAnimationFrame !== 'function' || typeof ports.cancelAnimationFrame !== 'function'
        || typeof ports.getComputedStyle !== 'function') throw Error();
    } catch (_) { throw new TypeError('startup probe unavailable'); }
    let generated = 0, sent = 0, failed = 0, dropped = 0, late = 0;
    let refused = false, disposed = false, contentReady = false, domEvent = false, loadEvent = false;
    let sampled = false, rafAcks = 0, queue = Promise.resolve();
    const listeners = [], frames = new Set();
    function state() {
      return Object.freeze({ generated, sent, failed, dropped, late, refused, disposed,
        contentReady, domEvent, loadEvent, rafAcks, visiblePaint: 'not-observed' });
    }
    function record(stage, fields) {
      if (disposed) { late++; return; }
      if (generated >= MAX) { dropped++; refused = true; return; }
      const event = Object.freeze(Object.assign({ schema: 'nemo.startup-js/1',
        runId: identity.runId, loadGeneration: identity.loadGeneration,
        sequence: ++generated, stage }, fields));
      if (JSON.stringify(event).length > 1024) { dropped++; refused = true; return; }
      queue = queue.then(async () => {
        if (disposed) { late++; return; }
        try { if (await ports.emit(event) !== true) { failed++; refused = true; }
          else sent++; } catch (_) { failed++; refused = true; }
      });
    }
    function reject(reason) { if (!refused) record('probe-refused', { reason }); refused = true; }
    function readyState() {
      const value = doc.readyState;
      if (!['loading', 'interactive', 'complete'].includes(value)) throw Error();
      return value;
    }
    function control() {
      try {
        const node = doc.getElementById('start-open');
        if (!node) return 'missing';
        let current = node, depth = 0;
        while (current && depth++ < 8) {
          const style = ports.getComputedStyle(current);
          const opacity = Number(style.opacity);
          if (typeof style.display !== 'string' || !['visible', 'hidden', 'collapse'].includes(style.visibility)
            || !Number.isFinite(opacity) || opacity < 0 || opacity > 1) return 'unknown';
          if (style.display === 'none' || ['hidden', 'collapse'].includes(style.visibility)
            || opacity === 0 || current.hidden === true) return 'hidden';
          current = current.parentElement;
        }
        if (current) return 'unknown';
        const box = node.getBoundingClientRect();
        if (![box.width, box.height].every(n => typeof n === 'number' && Number.isFinite(n)
          && n >= 0 && n <= 20000)) return 'unknown';
        return box.width > 0 && box.height > 0 ? 'visible' : 'hidden';
      } catch (_) { return 'unknown'; }
    }
    function scheduleFrame() {
      let id;
      try {
        id = ports.requestAnimationFrame(() => {
          if (disposed) { late++; return; }
          if (!frames.delete(id)) { reject('stage-order'); return; }
          if (refused) return;
          record('raf-ack', { ordinal: ++rafAcks });
          if (rafAcks === 1) scheduleFrame();
        });
        frames.add(id);
      } catch (_) { reject('unsupported-port'); }
    }
    function sample() {
      if (sampled || refused || disposed) return;
      sampled = true;
      const visibility = control();
      record('startup-control', { control: 'start-open', visibility });
      if (visibility === 'unknown') reject('unsupported-document');
      else scheduleFrame();
    }
    function trusted(event) {
      if (disposed) { late++; return false; }
      if (refused) return false;
      try { if (!event || event.isTrusted !== true) { reject('synthetic-event'); return false; } }
      catch (_) { reject('unsupported-document'); return false; }
      return true;
    }
    function dom(event) {
      if (!trusted(event)) return;
      try {
        if (domEvent || readyState() === 'loading') { reject('stage-order'); return; }
        domEvent = true; contentReady = true;
        record('dom-ready', {}); sample();
      } catch (_) { reject('unsupported-document'); }
    }
    function load(event) {
      if (!trusted(event)) return;
      try {
        if (!contentReady || loadEvent || readyState() !== 'complete') { reject('stage-order'); return; }
        loadEvent = true; record('window-load', {}); sample();
      } catch (_) { reject('unsupported-document'); }
    }
    function error(event, rejection) {
      if (!trusted(event)) return;
      let category = 'unknown';
      try {
        const value = rejection ? event.reason : event.error;
        const name = value && value.name;
        if (['SyntaxError', 'ReferenceError', 'TypeError', 'RangeError'].includes(name)) category = name;
      } catch (_) { /* Private or throwing values are never reproduced. */ }
      record(rejection ? 'unhandled-rejection' : 'js-error', { category });
    }
    function listen(target, name, handler) {
      target.addEventListener(name, handler); listeners.push([target, name, handler]);
    }
    function dispose() {
      if (disposed) return state();
      disposed = true;
      for (const [target, name, handler] of listeners) {
        try { target.removeEventListener(name, handler); } catch (_) { failed++; }
      }
      for (const id of frames) {
        try { ports.cancelAnimationFrame(id); } catch (_) { failed++; }
      }
      frames.clear(); return state();
    }
    record('early-script', {});
    try {
      listen(doc, 'DOMContentLoaded', dom); listen(win, 'load', load);
      listen(win, 'error', event => error(event, false));
      listen(win, 'unhandledrejection', event => error(event, true));
      const initial = readyState();
      if (initial !== 'loading') {
        contentReady = true; record('ready-state-observed', { readyState: initial }); sample();
      }
    } catch (_) { reject('unsupported-document'); }
    return Object.freeze({ state, dispose, drained: () => queue });
  }
  const api = Object.freeze({ createStartupProbe });
  if (typeof module === 'object' && module.exports) module.exports = api;
  return api;
}());
