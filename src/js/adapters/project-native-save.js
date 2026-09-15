// @ts-check
// Native project-write adapter (P29): the filesystem block that
// `writeProjectTo` in project.js used to inline.
//
// This module owns no state and reads no globals. Everything it needs comes in
// through explicit ports, so the write sequence — including its failure
// ordering — is exercisable headlessly against injected failures:
//
//   ports.writeTextFile(path, text) — Tauri fs.writeTextFile
//   ports.rename(from, to)          — Tauri fs.rename (needs fs:allow-rename)
//   ports.remove(path)              — Tauri fs.remove
//
// The caller keeps everything that is not the write itself: serializing the
// document, and the project state/recents/dirty/autosave updates that follow a
// successful save.
//
// ⚠️ The fallback is preserved verbatim, not endorsed. Writing to a temp
// sibling and renaming over the target is atomic at the OS level, so the
// project file is always either the old complete version or the new complete
// one. The catch branch gives that up: a direct write to PATH can leave it
// torn if the process dies mid-write. It exists because rename needs
// `fs:allow-rename` (added to capabilities 2026-07-13) and an app built before
// that — or an exotic filesystem refusing the rename — would otherwise fail
// the save outright, and a maybe-torn write still beats guaranteed data loss
// from refusing to save at all. Moving this code does NOT make the save
// atomic; that pre-existing crash-safety limitation travels with it.
//
// Two orderings in the catch branch are load-bearing and must not be "tidied":
// the temp file is removed BEFORE the direct write, and a failing remove is
// swallowed (the temp may never have been created), while a failing direct
// write propagates to the caller so the save is not silently reported as done.
var NemoProjectNativeSave = (function () {
  'use strict';

  // The in-progress sibling. Same directory as the target on purpose: rename
  // is only atomic within one filesystem.
  var TEMP_SUFFIX = '.saving';

  function tempPathFor(path) {
    return path + TEMP_SUFFIX;
  }

  // Resolves only once the bytes are in place under `path`. Rejects only if
  // the fallback direct write also fails.
  async function writeProjectFile(path, json, ports) {
    var tmp = tempPathFor(path);
    try {
      await ports.writeTextFile(tmp, json);
      await ports.rename(tmp, path);
    } catch (e) {
      try { await ports.remove(tmp); } catch (_e) {}
      await ports.writeTextFile(path, json);
    }
  }

  return {
    TEMP_SUFFIX: TEMP_SUFFIX,
    tempPathFor: tempPathFor,
    writeProjectFile: writeProjectFile,
  };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoProjectNativeSave;
