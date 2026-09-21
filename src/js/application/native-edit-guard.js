/* N19C dormant legacy-edit admission guard. N20 owns production installation. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.SMNativeEditGuard = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  var controller = null;
  var cycle = null;

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
    var identity;
    try { identity = controller.getNativeIdentity(); }
    catch (_) { return undefined; }
    if (identity === null) return null;
    if (!identity || typeof identity !== 'object' || Array.isArray(identity)
      || Object.keys(identity).length !== 2
      || typeof identity.documentId !== 'string' || identity.documentId.length === 0
      || !Number.isSafeInteger(identity.generation) || identity.generation < 0) return undefined;
    return { documentId: identity.documentId, generation: identity.generation };
  }

  function sameIdentity(left, right) {
    return left.documentId === right.documentId && left.generation === right.generation;
  }

  function receiveRelease(value, identity) {
    if (cycle && sameIdentity(cycle.identity, identity) && isExactReleaseReceipt(value)
      && sameIdentity(value, identity)) cycle.receipt = value;
  }

  function requestRelease(kind, identity) {
    if (cycle.requested) return;
    cycle.requested = true;
    var result;
    try { result = controller.requestRelease({ kind: kind, documentId: identity.documentId, generation: identity.generation }); }
    catch (_) { return; }
    if (result && typeof result.then === 'function') {
      result.then(function (value) { receiveRelease(value, identity); }, function () {});
      return;
    }
    receiveRelease(result, identity);
  }

  // A release never admits its own current stack. Native ownership always denies;
  // only a later call after matching receipt + reported legacy ownership may enter.
  function allow(kind) {
    var identity = nativeIdentity();
    if (identity === null) {
      if (!cycle) return true;
      if (!cycle.receipt) return false;
      cycle = null;
      return true;
    }
    if (identity === undefined) return false;
    if (!cycle || !sameIdentity(cycle.identity, identity)) cycle = { identity: identity, requested: false, receipt: null };
    requestRelease(kind, identity);
    return false;
  }

  function install(next) {
    if (!next || typeof next.getNativeIdentity !== 'function' || typeof next.requestRelease !== 'function') {
      throw new TypeError('native edit guard controller requires getNativeIdentity and requestRelease');
    }
    if (controller || cycle) throw new Error('native edit guard controller is already installed');
    controller = next;
    return api;
  }

  var api = Object.freeze({ allow: allow, install: install, isExactReleaseReceipt: isExactReleaseReceipt });
  return api;
}));
