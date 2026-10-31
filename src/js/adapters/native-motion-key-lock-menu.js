/* D37 / #2522: the "Lock keyframes on:" menu of the native Motion presenter, built once for both surfaces so the labels, order and
 * check state can never drift (baseline buildKeyLockMenuItems / buildKeySelectionLockMenuItems, src/js/motion.js @1065e9d8): a disabled
 * header row, then In / Out / Whole layer, a check on the active mode, and picking the active mode again clears it (a radio that can
 * be switched off). The builder holds no state and writes nothing: `pick(next)` is the caller's guarded native edit. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeMotionKeyLockMenu = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const MODES = ['in', 'out', 'layer'];
  const ROWS = [['in', 'ctxKeyLockInPoint', '   • the in point'], ['out', 'ctxKeyLockOutPoint', '   • the out point'], ['layer', 'ctxKeyLockWholeLayer', '   • the whole layer']];
  const HEADER = ['ctxLockKeyframesOnColon', 'Lock keyframes on:'];
  const translate = (host, key, fallback) => { let value; try { value = host && host.SM && typeof host.SM.t === 'function' ? host.SM.t(key) : undefined; } catch (_) { value = undefined; } return typeof value === 'string' && value && value !== key ? value : fallback; };
  // `active(mode)` says whether the mode is the current one; `pick(next)` receives the mode to write, or null to clear.
  function items(host, active, pick) {
    return [{ label: translate(host, HEADER[0], HEADER[1]), disabled: true, action() {} }].concat(ROWS.map((row) => {
      const on = active(row[0]) === true;
      return { label: translate(host, row[1], row[2]) + (on ? '  ✓' : ''), action() { return pick(on ? null : row[0]); } };
    }));
  }
  // The row title and a mode's short name reuse the menu's own translations, without the colon and the bullet.
  const title = (host) => translate(host, HEADER[0], HEADER[1]).replace(/\s*:\s*$/, '');
  const modeName = (host, mode) => { const row = ROWS.find((r) => r[0] === mode); return row ? translate(host, row[1], row[2]).replace(/^\s*•\s*/, '').replace(/^the /, '') : null; };
  return Object.freeze({ MODES, items, title, modeName });
}));
