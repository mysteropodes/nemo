/* Deliberate unavailable state for inspector controls that have no native owner yet (#2372, UNAVAIL-NOTICES). On a native object project the engine owns the
 * document, and these controls of the baseline (1065e9d8) still drive a Paper writer or a JS-only store: "Propagate to all frames" (fill-bridge.js),
 * "Select all keyframes' content" (timeline.js selectGhostAll), the stroke-gradient-along-path row (timeline.js applyStrokeGradAlongToSelection) and the
 * custom shader library (effects-panel.js). None of them is an admitted native feature, so each is shown as unavailable (aria-disabled, a title naming
 * why) and, if still activated, says so with ONE stable sentence - before any undo entry, frame save or Paper write, never a dead click.
 * The TABLE is the single declaration, keyed by element id (the custom shader entry is a menu row, keyed by its menu id): the native owner of a capability
 * deletes its row(s) and the control is the baseline one again. #btn-grad-add-stop is deliberately NOT here - it already toasts its own refusal.
 * Legacy (non-native) projects are untouched: sync removes every mark it set. This module holds no document state. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeInspectorUnavailable = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  function row(reason, what, kind) { return Object.freeze({ reason: reason, kind: kind || 'control', notice: what + ' not available on this project yet.' }); }
  var PROPAGATE = row('propagate-across-frames', 'Propagating a fill across frames is');
  var GHOST = row('multi-frame-ghost-selection', 'Selecting the content of several keyframes at once is');
  var STROKE_GRADIENT = row('stroke-gradient-along-path', 'A gradient along a stroke is');
  var TABLE = Object.freeze({
    'btn-fill-propagate': PROPAGATE,
    'btn-ghost-select': GHOST,
    'p-strokegrad-along': STROKE_GRADIENT,
    'p-strokegrad-from': STROKE_GRADIENT,
    'p-strokegrad-from-c': STROKE_GRADIENT,
    'p-strokegrad-to': STROKE_GRADIENT,
    'p-strokegrad-to-c': STROKE_GRADIENT,
    'new-custom-shader': row('custom-shader-library', 'The custom shader library is', 'menu')
  });
  var installed = new WeakMap();
  // True on a native object project (the same route the Select style panel uses to decide that the engine owns the edit).
  function active(root) {
    var S = root && root.NemoNativeSelectionStyle;
    try { return !!(S && typeof S.native === 'function' && S.native(root)); } catch (_) { return false; }
  }
  function noticeFor(id) { return TABLE[id] ? TABLE[id].notice : null; }
  function toast(root, text) { if (root && typeof root.showToast === 'function') try { root.showToast(text); } catch (_) {} }
  // Activation guard: on a native project, says the stable sentence and returns true (the caller returns before any writer); otherwise false.
  function refuse(root, id) {
    if (!TABLE[id] || !active(root)) return false;
    toast(root, TABLE[id].notice);
    return true;
  }
  function mark(el, text) {
    if (!el) return;
    var d = el.dataset || (el.dataset = {});
    if (d.nativeUnavailable !== '1') { d.nativeUnavailable = '1'; d.nativeBaseTitle = el.title || ''; }
    if (el.title !== text) el.title = text;
    if (typeof el.setAttribute === 'function') el.setAttribute('aria-disabled', 'true');
  }
  function unmark(el) {
    var d = el && el.dataset;
    if (!d || d.nativeUnavailable !== '1') return;
    el.title = d.nativeBaseTitle || '';
    if (!el.title && typeof el.removeAttribute === 'function') el.removeAttribute('title');
    delete d.nativeUnavailable; delete d.nativeBaseTitle;
    if (typeof el.removeAttribute === 'function') el.removeAttribute('aria-disabled');
  }
  function ownerOf(root, target) {
    for (var el = target; el; el = el.parentElement || el.parentNode) {
      if (el.dataset && el.dataset.nativeUnavailable === '1' && el.id && TABLE[el.id]) return el.id;
    }
    return null;
  }
  // One capture listener on the document: an activation of a marked control says why and goes no further (a checkbox keeps its state, a colour picker
  // does not open, the control's own handler never runs).
  function intercept(root, event) {
    var id = ownerOf(root, event && event.target);
    if (!id || !refuse(root, id)) return;
    if (typeof event.preventDefault === 'function') event.preventDefault();
    if (typeof event.stopImmediatePropagation === 'function') event.stopImmediatePropagation();
  }
  // Brings the marks in line with the project kind (cheap and idempotent: called from every UI repaint) and installs the listener once.
  function sync(root) {
    var doc = root && root.document;
    if (!doc || typeof doc.getElementById !== 'function') return false;
    if (!installed.has(root) && typeof doc.addEventListener === 'function') {
      var handler = function (event) { intercept(root, event); };
      doc.addEventListener('click', handler, true);
      installed.set(root, handler);
    }
    var native = active(root);
    Object.keys(TABLE).forEach(function (id) {
      if (TABLE[id].kind !== 'control') return;
      var el = doc.getElementById(id);
      if (native) mark(el, TABLE[id].notice); else unmark(el);
    });
    return native;
  }
  return Object.freeze({ TABLE: TABLE, active: active, noticeFor: noticeFor, refuse: refuse, sync: sync });
}));
