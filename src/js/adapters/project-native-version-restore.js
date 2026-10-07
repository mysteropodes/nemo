/* Version Restore orchestration; native ports own documents and publication. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoProjectNativeVersionRestore = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  function copyPin(value) {
    if (!value || typeof value.instanceId !== 'string' || !value.instanceId ||
        typeof value.documentId !== 'string' || !value.documentId ||
        !Number.isSafeInteger(value.contentRevision) || value.contentRevision < 0) return null;
    return Object.freeze({ instanceId: value.instanceId, documentId: value.documentId,
      contentRevision: value.contentRevision, path: value.path, name: value.name });
  }
  function same(a, b) {
    return !!a && !!b && a.instanceId === b.instanceId && a.documentId === b.documentId &&
      a.contentRevision === b.contentRevision && a.path === b.path && a.name === b.name;
  }
  function create(ports) {
    var busy = false;
    async function restore(path) {
      if (busy) return false;
      busy = true;
      try {
        var before = copyPin(ports.pin());
        if (!before) throw new Error('Native version Restore is unavailable');
        var json = await ports.read(path);
        ports.validate(json);
        if (!same(before, copyPin(ports.pin()))) throw new Error('Native document changed during version read');
        // Resolve the existing history namespace before replacement. No file
        // is written here; a later path change cannot redirect A's backup.
        var target = await ports.backupTarget();
        if (!same(before, copyPin(ports.pin()))) throw new Error('Native document changed during backup preparation');
        var previous = ports.serialize();
        if (typeof previous !== 'string' || !previous || !same(before, copyPin(ports.pin())))
          throw new Error('Native previous-version persistence is unavailable or stale');
        var first = await ports.importNative(json);
        if (!first || ports.ready(first) !== true) throw new Error('Native version admission did not complete');
        // The existing native port verifies the final presented frame and
        // rolls back UI projection/fences an indeterminate replacement. This
        // adapter cannot roll back Rust or treat an initial receipt as success.
        if (await ports.publish(first) !== true || ports.ready(first) !== true)
          throw new Error('Native version publication did not complete');
        try { await ports.backup(previous, target); }
        catch (error) { ports.backupFailed(error); } // Existing best-effort disk-history policy.
        ports.restored();
        return true;
      } catch (error) {
        ports.unavailable(error);
        return false;
      } finally { busy = false; }
    }
    return Object.freeze({ restore: restore });
  }
  function bind(root, ui) {
    return create({
      pin: function () {
        var native = root.NemoNativeOpacityCutover;
        if (!native || native.blocksLegacy() !== true || native.isActive() !== true) return null;
        var pin = native.identity(), metadata = ui.metadata();
        return pin && { instanceId: pin.instanceId, documentId: pin.documentId,
          contentRevision: pin.contentRevision, path: metadata.path, name: metadata.name };
      },
      read: function (path) { return root.__TAURI__.fs.readTextFile(path); },
      validate: function (json) { root.SMProjectDocument.prepareNativeOpacity(json); },
      serialize: function () {
        var native = root.NemoNativeOpacityCutover;
        if (!native || native.blocksLegacy() !== true || native.isActive() !== true)
          throw new Error('Native version persistence is unavailable');
        return native.persistenceJSON();
      },
      backupTarget: ui.backupTarget,
      importNative: function (json) {
        var project = root.NemoNativeOpacityProject;
        if (!project || typeof project.importJSON !== 'function') throw new Error('Native version import is unavailable');
        return project.importJSON(json, true, true);
      },
      ready: function (receipt) { return root.NemoNativeOpacityProjectEntry.ready(root, receipt); },
      publish: async function (receipt) {
        var modal = ui.modal(), display = modal && modal.style.display;
        if (modal) modal.style.display = 'none';
        try {
          var result = await ui.reveal(receipt);
          if (result !== true && modal) modal.style.display = display;
          return result;
        }
        catch (error) { if (modal) modal.style.display = display; throw error; }
      },
      backup: ui.backup, backupFailed: ui.backupFailed,
      restored: ui.restored, unavailable: ui.unavailable,
    });
  }
  async function writeSnapshot(json, dir, fs, maximum, now) {
    await fs.mkdir(dir, { recursive: true });
    await fs.writeTextFile(dir + '/' + now() + '.json', json);
    var entries = await fs.readDir(dir);
    var names = entries.filter(function (entry) { return /\.json$/.test(entry.name); })
      .map(function (entry) { return entry.name; }).sort();
    while (names.length > maximum) {
      var victim = names.shift();
      try { await fs.remove(dir + '/' + victim); } catch (error) {}
    }
  }
  return Object.freeze({ create: create, bind: bind, writeSnapshot: writeSnapshot });
}));
