// @ts-check
// Folder-metadata codec (P20): the document and undo-snapshot representations
// of `state.layerFolders` (folder name + collapsed flag, keyed by folder id)
// and each layer's `folderId` membership tag.
//
// The copy semantics here are deliberately NOT uniform. Each function keeps
// exactly the semantics its original call site had:
//
//   * document export/import share the map BY REFERENCE — the exported object
//     is serialized immediately, and an imported document has just been
//     parsed, so neither aliases anything the editor keeps mutating.
//   * undo snapshot/restore DEEP COPY — undo state must never alias live
//     state. That aliasing was a real bug for these maps (an undo brought
//     layers back out of a folder but left the now-empty entry behind, and a
//     renamed folder came back with its new name); see the 2026-09 QA sweep
//     comments in tweens.js `layersSnapshotNow`.
//
// Collapsing those two into one shared copy policy would be a behaviour
// change, not a cleanup. This module owns no state and reads no globals:
// callers pass values in and assign the results.
var NemoFolderCodec = (function () {
  'use strict';

  function deepCopy(value) {
    return JSON.parse(JSON.stringify(value));
  }

  // --- project document (timeline.js SM.exportJSON / SM.importJSON) --------

  // By reference, and undefined-preserving: an absent map must stay absent in
  // the document rather than becoming `{}`, so that importing an older file
  // still takes the import-side default below.
  function exportFolderMap(map) {
    return map;
  }

  // Missing or falsy map in the document -> empty map, never undefined.
  function importFolderMap(value) {
    return value || {};
  }

  // Per-layer membership as written to the document. May be undefined; the
  // document keeps the key so the shape stays stable across versions.
  function exportFolderId(layer) {
    return layer.folderId;
  }

  // Per-layer membership adopted from a parsed document. A falsy id is
  // SKIPPED rather than assigned, leaving whatever the target already had —
  // this is what keeps a legacy file without `folderId` from stamping
  // `undefined` over a freshly created layer.
  function applyFolderId(target, source) {
    if (source.folderId) target.folderId = source.folderId;
    return target;
  }

  // --- undo (tweens.js layersSnapshotNow / restoreLayersSnapshot) ----------

  // Deep copy into an undo snapshot; a missing live map snapshots as `{}`.
  function snapshotFolderMap(map) {
    return deepCopy(map || {});
  }

  // Whether a snapshot carries folder metadata at all. Snapshots taken before
  // these maps were included are `undefined` here, and the caller must then
  // leave the live map untouched rather than clearing it.
  function hasFolderMap(snapshot) {
    return !!snapshot;
  }

  // Deep copy back out of an undo snapshot. Only valid when hasFolderMap().
  function restoreFolderMap(snapshot) {
    return deepCopy(snapshot);
  }

  return {
    exportFolderMap: exportFolderMap,
    importFolderMap: importFolderMap,
    exportFolderId: exportFolderId,
    applyFolderId: applyFolderId,
    snapshotFolderMap: snapshotFolderMap,
    hasFolderMap: hasFolderMap,
    restoreFolderMap: restoreFolderMap,
  };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoFolderCodec;
