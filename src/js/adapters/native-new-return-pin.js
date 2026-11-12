/* New-project return pin under the native queue (#2372). The pin is read from the presented object session, which the native queue hides while ANY command is
 * pending (queue-pending): "cannot prove it right now", not "changed". soon() waits for it with the shared bounded wait and only in that window; any other
 * reason, and a context that moved on (holds() false), are not waited out. Returns null when the presentation is not hidden by the queue (the caller refuses as
 * before), else true and later calls ok(presentedSession) once, or fail(status) with status 'changed' | 'refused' | 'busy'. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeNewReturnPin = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  function reader(root) { return function (f) { return root.NemoNativeObjectProject.desktop(root).readPresentedObjectSession(f); }; }
  function soon(root, holds, ok, fail) {
    var Wait = root && root.NemoNativePresentationWait, read = reader(root);
    try { if (!Wait || !Wait.busy(read)) return null; } catch (_) { return null; }
    Wait.await(root, { read: read, holds: holds }).then(function (r) { if (r.ready) ok(r.value); else fail(r.status); },
      function () { fail('refused'); });
    return true;
  }
  return Object.freeze({ soon: soon });
}));
