/* #2256 Q3: the Duplicator column cell and the "Object-mode Duplicator..." layer-menu entry over a NATIVE object project.
 * Baseline (1065e9d8): motion.js drew a .lico.motion-col-duplicator cell per row (click -> toggleLayerDuplicator) and a layer
 * context entry ctxObjectDuplicatorEllipsis (convertSelectionToObjectDuplicator, enabled for a multi-selection). Here the cell
 * reads its on/off state from the native layer row (hasDuplicator, dupSourceMode: a read-model fact, never a legacy layer flag),
 * a click is ONE command.document.object.duplicator.set with a fresh seed, and the entry is ONE
 * command.document.object.duplicator.objects naming the selected layers in document order (both via native-object-duplicator.js).
 * The owner locks a layer when it enables a duplicator and unlocks it when it disables one; this file only repaints from the
 * confirmed revision. A refusal (locked/folder/effect source, stale pin, entered component, ...) toasts the owner's typed
 * reason and writes nothing. toggleLayerDuplicator / convertSelectionToObjectDuplicator are never called. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectDuplicatorUi = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const ICO_DUP = '<svg viewBox="0 0 24 24"><rect x="4" y="4" width="7" height="7" rx="1.2" fill="currentColor"/><rect x="13.5" y="4" width="7" height="7" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.6"/><rect x="4" y="13.5" width="7" height="7" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.6"/><rect x="13.5" y="13.5" width="7" height="7" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.6"/></svg>';
  const text = (v) => typeof v === 'string' && v.length > 0;
  const tr = (host, key, fallback) => {
    const t = host.SM && typeof host.SM.t === 'function' ? host.SM.t(key) : null;
    return typeof t === 'string' && t.length && t !== key ? t : fallback;
  };
  const toast = (host, message) => { if (typeof host.showToast === 'function') host.showToast(message); };
  const busy = new WeakSet();

  function adapterOf(host) {
    const module = host.NemoNativeObjectDuplicator;
    return module && typeof module.desktop === 'function' ? module.desktop(host) : null;
  }
  // The presented native layer rows (the pinned project info), or [] when no native project is presented.
  function rowsOf(host) {
    const project = host.NemoNativeObjectProject, session = project && typeof project.desktop === 'function' ? project.desktop(host) : null;
    const info = session && typeof session.info === 'function' ? session.info() : null;
    return info && Array.isArray(info.layers) ? info.layers : [];
  }
  // The owner's typed message when it sent one (locked/folder/effect source, "Close the component first.", ...), else the local reason.
  function refusalText(outcome) {
    if (outcome && outcome.code === 'stale_selection') return 'The layer is no longer in the project.';
    const detail = outcome && (text(outcome.message) ? outcome.message : text(outcome.reason) ? outcome.reason : outcome.code);
    return 'Duplicator refused' + (text(detail) ? ': ' + detail : '.');
  }
  function repaint(host) {
    ['renderLayerList', 'renderTimeline', 'nativeCompositingRefresh'].forEach((name) => { if (typeof host[name] === 'function') host[name](); });
  }
  function settle(host, outcome, onApplied) {
    if (outcome && outcome.ok === true) { if (onApplied) onApplied(); repaint(host); return true; }
    toast(host, refusalText(outcome));
    return false;
  }
  function run(host, start, onApplied) {
    const adapter = adapterOf(host);
    if (!adapter) { toast(host, 'Native duplicator is unavailable.'); return Promise.resolve(false); }
    if (busy.has(host)) return Promise.resolve(false);
    busy.add(host);
    return Promise.resolve().then(() => start(adapter)).then(
      (outcome) => settle(host, outcome, onApplied),
      (error) => { toast(host, refusalText({ message: String(error && error.message || error) })); return false; },
    ).finally(() => { busy.delete(host); });
  }

  // The baseline toggle: enabling writes the default descriptor with a fresh seed, disabling removes it.
  function toggle(host, layerUid) {
    const row = rowsOf(host).find((layer) => layer.layerUid === layerUid);
    if (!row) { toast(host, refusalText({ code: 'stale_selection' })); return Promise.resolve(false); }
    return run(host, (adapter) => adapter.setEnabled(layerUid, row.hasDuplicator !== true));
  }
  // The selection (layer indices) in document order, de-duplicated: ONE command naming those layers.
  function convertSelection(host, indices) {
    const rows = rowsOf(host);
    const wanted = (Array.isArray(indices) ? indices : []).filter((at, k, all) => Number.isInteger(at) && all.indexOf(at) === k && rows[at])
      .sort((a, b) => a - b).map((at) => rows[at].layerUid);
    const hosts = wanted.filter((uid) => rows.find((layer) => layer.layerUid === uid).hasDuplicator === true).length;
    return run(host, (adapter) => adapter.fromObjects(wanted), () => {
      toast(host, tr(host, 'toastObjectDuplicatorCreated', 'Duplicator created — {n} objects in the pool').replace('{n}', String(wanted.length - hosts)));
    });
  }

  // The layer-row cell (the presenter's duplicatorCell hook): the baseline icon with the off class from the native fact.
  function cell(host, info, layer) {
    const node = host.document.createElement('div'), on = layer && layer.hasDuplicator === true;
    node.className = 'lico motion-col-duplicator' + (on ? '' : ' off');
    node.title = tr(host, 'dupColTitle', 'Duplicator (grid / radial / path)');
    node.innerHTML = ICO_DUP;
    node.dataset.nativeDuplicatorCell = 'true';
    if (on && layer.dupSourceMode === 'objects') node.dataset.dupSourceMode = 'objects';
    node.addEventListener('click', (e) => {
      if (e && typeof e.stopPropagation === 'function') e.stopPropagation();
      return toggle(host, layer.layerUid);
    });
    return node;
  }
  // Layer-menu entries for row `li` (appended to the native row menu): enabled for a multi-selection that includes the row.
  function contextItems(host, li) {
    const selection = Array.isArray(host._layerSel) ? host._layerSel.slice() : [];
    return [{ label: tr(host, 'ctxObjectDuplicatorEllipsis', 'Object-mode Duplicator…'), disabled: !(selection.length >= 2 && selection.indexOf(li) >= 0),
      action: () => convertSelection(host, selection) }];
  }
  return Object.freeze({ cell, contextItems, toggle, convertSelection, refusalText });
}));
