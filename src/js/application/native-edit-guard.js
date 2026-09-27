/* N19C dormant legacy-edit admission guard. N20 owns production installation. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.SMNativeEditGuard = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  var controller = null;
  // Once native or indeterminate authority has been observed, this dormant
  // compatibility guard can never reopen a legacy writer in this session.
  var denied = false;

  function isExactReleaseReceipt(value) {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
    var keys = Object.keys(value).sort();
    return keys.length === 4
      && keys[0] === 'documentId'
      && keys[1] === 'generation'
      && keys[2] === 'owner'
      && keys[3] === 'status'
      && typeof value.documentId === 'string' && value.documentId.length > 0
      && Number.isSafeInteger(value.generation) && value.generation >= 0
      && value.owner === 'legacy'
      && value.status === 'released';
  }

  function nativeIdentity() {
    if (!controller) return null;
    try {
      var identity = controller.getNativeIdentity();
      if (identity === null) return null;
      if (!identity || typeof identity !== 'object' || Array.isArray(identity)
        || Object.keys(identity).length !== 2) return undefined;
      var documentId = identity.documentId;
      var generation = identity.generation;
      if (typeof documentId !== 'string' || documentId.length === 0
        || !Number.isSafeInteger(generation) || generation < 0) return undefined;
      return { documentId: documentId, generation: generation };
    } catch (_) { return undefined; }
  }

  // This helper is only a synchronous denial gate. It does not request a
  // release or replay an edit; N20 owns any future explicit transition policy.
  function allow(kind) {
    if (denied) return false;
    var identity = nativeIdentity();
    if (identity === null) return true;
    denied = true;
    return false;
  }

  function install(next) {
    if (!next || typeof next.getNativeIdentity !== 'function' || typeof next.requestRelease !== 'function') {
      throw new TypeError('native edit guard controller requires getNativeIdentity and requestRelease');
    }
    if (controller) throw new Error('native edit guard controller is already installed');
    controller = next;
    return api;
  }

  var api = Object.freeze({ allow: allow, install: install, isExactReleaseReceipt: isExactReleaseReceipt });
  return api;
}));
