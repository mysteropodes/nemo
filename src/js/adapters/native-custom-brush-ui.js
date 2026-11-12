/* Catalog presentation and original callbacks only. Native Drawing's custom brush provider remains unavailable. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else {
    root.NemoNativeCustomBrushUI = api;
    root.document.addEventListener('DOMContentLoaded', function () { api.install(root); }, { once: true });
  }
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var installed = new WeakMap(), selectionIntents = new WeakMap();
  var pinKeys = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  function has(value, key) { return Object.prototype.hasOwnProperty.call(value, key); }
  function native(root) { return !!(root.__TAURI__ && root.__TAURI__.core); }
  function builtin(root, key) {
    return root.BRUSH_PRESETS && has(root.BRUSH_PRESETS, key) ? root.BRUSH_PRESETS[key] : null;
  }
  function controller(root) { return installed.get(root) || null; }
  function projection(root) { var c = controller(root); return c && c.read(); }
  function exists(root, id, origin) {
    var p = origin || projection(root);
    return p ? p.catalog.items.some(function (row) { return row.id === id; }) : null;
  }
  function record(root, id, origin) {
    var p = origin || projection(root);
    if (!p || p !== projection(root)) return null;
    var row = p.catalog.records.find(function (row) { return row.id === id; });
    return row && !row.unavailable ? row.preset : null;
  }
  function resolve(root, key) {
    if (key === undefined || key === null || key === 'none') return null;
    return builtin(root, key) || record(root, key);
  }
  function label(root, key) {
    if (key === 'none') return 'None (solid)';
    var p = projection(root), row = p && p.catalog.items.find(function (row) { return row.id === key; });
    if (row && row.label) return row.label;
    return String(key).split('-').map(function (word) {
      return word ? word[0].toUpperCase() + word.slice(1) : '';
    }).join(' - ');
  }
  function notice(root, message) {
    if (root.showToast) root.showToast(message || 'Custom brush catalog is unavailable or unconfirmed. Wait for synchronization.');
  }
  function selectionIntent(root) { return selectionIntents.get(root) || 0; }
  function select(root, key) {
    var p = projection(root);
    var built = key === 'none' || key === undefined || key === null || builtin(root, key);
    if (key === '' || !built && (!p || !record(root, key, p))) {
      notice(root);
      return false;
    }
    var custom = p && record(root, key, p);
    selectionIntents.set(root, selectionIntent(root) + 1);
    root.state.brushPreset = key === undefined || key === null ? 'none' : key;
    if (custom && typeof custom.savedBrushSize === 'number') {
      root.state.brushSize = custom.savedBrushSize;
      var input = root.document.getElementById('p-sw');
      if (input) input.value = Math.round(root.state.brushSize);
    }
    return true; // Transient tool selection only; no legacy selection, history or texture mutation.
  }
  function unavailablePreview(root, canvas, key) {
    if (!native(root) || key === 'none' || builtin(root, key) || record(root, key)) return false;
    var ctx = canvas.getContext('2d');
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.fillStyle = '#ffffff';
    ctx.fillText('Unavailable', 4, Math.max(12, canvas.height / 2));
    canvas.title = 'Native custom brush record unavailable';
    return true;
  }
  function install(root) {
    if (installed.has(root)) return installed.get(root);
    if (!native(root) || !root.NemoNativeCustomBrushDesktop) return null;
    var retired = false, pending = false, loading = null, busyStamp = null, refreshNeeded = false;
    function current() { return root.NemoNativeObjectProject && root.NemoNativeObjectProject.desktop(root); }
    function available(session, info) {
      return !retired && session === current() && root.NemoNativeObjectProjectFile &&
        root.NemoNativeObjectProjectFile.projectionReadyForUI(root, info) === true;
    }
    var model = root.NemoNativeCustomBrushDesktop.create({
      currentSession: current, available: available, publish: function () { return true; }
    });
    function stamp() {
      try {
        var s = current();
        return s && s.isActive() ? { session: s, info: Object.assign({}, s.info()), ticket: s.readSubscriptionGeneration() } : null;
      } catch (_) { return null; }
    }
    function changed(a, b) {
      return !!b && (!a || a.session !== b.session || a.ticket !== b.ticket ||
        pinKeys.some(function (key) { return a.info[key] !== b.info[key]; }));
    }
    function sameLifetime(a, b) {
      return !!a && !!b && a.session === b.session && ['instanceId', 'documentId', 'lifecycleGeneration'].every(function (key) {
        return a.info[key] === b.info[key];
      });
    }
    function closeViews() {
      [root.BrushEditor, root.BrushPresetPicker, root.BrushMenu].forEach(function (view) {
        if (view && typeof view.close === 'function') view.close();
      });
    }
    function refreshViews() {
      if (retired) return;
      if (root.BrushPresetPicker && root.BrushPresetPicker.refresh) root.BrushPresetPicker.refresh();
      if (root.BrushMenu && root.BrushMenu.refresh) root.BrushMenu.refresh();
    }
    function read() { return model.read(); }
    function ready(origin) { return !retired && !pending && !loading && !!origin && model.read() === origin; }
    function confirmed(result) {
      return !retired && result && ['applied-presented', 'unchanged'].includes(result.status) && result.projection === model.read();
    }
    // #2372: an origin that is not ready only because a pending native command hides the presentation is waited out (model.readSettled), not refused.
    async function readySoon(origin) { return ready(origin) || (!retired && !pending && !loading && !!origin && await model.readSettled() === origin); }
    async function edit(action, origin) {
      if (!(await readySoon(origin))) { notice(root); return { status: 'unavailable' }; }
      pending = true;
      busyStamp = stamp();
      try {
        var result = await model.edit(action, origin);
        if (!confirmed(result)) notice(root);
        return result;
      } catch (_) {
        if (!retired) notice(root);
        return { status: 'unconfirmed' };
      } finally {
        pending = false;
        busyStamp = null;
        if (refreshNeeded) { refreshNeeded = false; presented(); }
      }
    }
    function presented() {
      if (retired) return Promise.resolve(null);
      if (pending || loading) {
        if (changed(busyStamp, stamp())) refreshNeeded = true;
        return loading || Promise.resolve(null);
      }
      var value = model.read();
      if (value) return Promise.resolve(value);
      closeViews();
      busyStamp = stamp();
      loading = model.refresh().then(function (value) {
        closeViews();
        refreshViews();
        return value;
      }).catch(function () { return null; }).finally(function () {
        loading = null;
        busyStamp = null;
        var again = refreshNeeded;
        refreshNeeded = false;
        if (again && !model.read()) presented();
      });
      return loading;
    }
    var api = Object.freeze({
      read: read, ready: ready, confirmed: confirmed, edit: edit, presented: presented,
      isPending: function () { return pending || !!loading; },
      willProject: function () {
        var next = stamp();
        if (!pending || !sameLifetime(busyStamp, next)) closeViews();
        if (!pending) model.invalidate();
      },
      dispose: function () {
        if (retired) return;
        closeViews();
        retired = true;
        refreshNeeded = false;
        model.dispose();
        root.removeEventListener('pagehide', api.dispose);
      }
    });
    installed.set(root, api);
    root.NemoNativeCustomBrushController = api;
    root.addEventListener('pagehide', api.dispose, { once: true });
    presented();
    return api;
  }
  return Object.freeze({
    native: native, controller: controller, projection: projection, exists: exists, record: record, resolve: resolve,
    label: label, select: select, selectionIntent: selectionIntent, unavailablePreview: unavailablePreview,
    notice: notice, install: install
  });
}));
