/* UI save-status metadata only; document bytes remain owned by the serializer. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoProjectDirtyBaseline = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  function create() {
    var savedBytes = null;

    function markSaved(bytes) { savedBytes = bytes; }
    function isDirtyBytes(bytes) { return savedBytes !== null && bytes !== savedBytes; }
    function isDirty(readJSON) {
      // Preserve the clean, uninitialized baseline without reading a document.
      try { return savedBytes !== null && isDirtyBytes(readJSON()); }
      catch (error) { return true; }
    }
    function restoreTab(dirty, readJSON) {
      // Imports may normalize bytes. Retain the dirty flag captured before
      // replacement; only clean tabs use the newly normalized serializer bytes.
      // Evaluate the read before assigning so failure preserves the old baseline.
      savedBytes = dirty ? '' : readJSON();
    }

    return Object.freeze({ markSaved: markSaved, isDirty: isDirty,
      isDirtyBytes: isDirtyBytes, restoreTab: restoreTab });
  }

  return Object.freeze({ create: create });
}));
