/* Lifecycle-scoped native preview admission and presentation. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeOpacityPreviewWorkflow = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  function create(scope) {
    function present(frame) {
      if (scope.phase() !== 'native' || scope.replacementRequested()) {
        return Promise.reject(new Error('native opacity authority is not active'));
      }
      var target = scope.cycle();
      var superseded = new Error('native opacity preview lifecycle changed or synchronization is pending');
      superseded.code = 'native_preview_superseded';
      function verify() {
        scope.requireAdmitted(target);
        if (scope.phase() !== 'native') throw new Error('native opacity authority is not active');
        if (target.synchronizing) throw superseded;
      }
      return scope.enqueue(async function () {
        try {
          verify();
          var current = scope.identity(), prepared = scope.prepared(), serialized = scope.serialized();
          return await target.preview.present(current.instanceId, scope.generation(), {
            documentSnapshotId: serialized.result.documentSnapshotId, documentId: current.documentId,
            contentRevision: current.contentRevision, contextId: 'scene-root', frame: frame,
            quality: 'final', outputSpec: scope.output(), geometryHandle: prepared.frames[frame].geometryHandle,
          }, scope.host(), verify);
        } catch (error) {
          // A prior-frame preview may complete while an external native revision
          // is synchronizing. Its receipt is stale, but the revision must still
          // be allowed to refresh caches and acknowledge the committed write.
          if (error === superseded) throw error;
          if (scope.phase() === 'release-requested' && target === scope.cycle()) throw error;
          throw scope.fail(target, error);
        }
      });
    }
    function render(frame) {
      if (scope.phase() === 'legacy') return false;
      if (scope.phase() !== 'native') return true;
      try { scope.requireAdmission(); } catch (_) { return true; }
      if (scope.busy()) return true;
      present(frame).catch(function () {});
      return true;
    }
    return Object.freeze({ present: present, render: render });
  }

  return Object.freeze({ create: create });
}));
