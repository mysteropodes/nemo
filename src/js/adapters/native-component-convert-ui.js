/* #2256 leaf 1: the ordinary "Convert to Component" entries (layer menu, context menu, keyboard menu, Motion row menu and the
 * Component panel button, all of which call SM.convertActiveLayerToComponent) under the native owner. On a native object project the
 * entry sends the existing native convert transaction (NemoNativeObjectComponent.convert, the #2244 request: one layer converts in
 * place, several merge at the first slot) as ONE history entry and selects the resulting layer; a refusal shows a notice and writes
 * nothing. There is no JS fallback writer: convertLayerToComponent / convertLayersToComponent are never called while a native
 * session owns the document, and for a legacy document the original entry is called unchanged.
 * D-comp-break-apart: the inverse entry SM.convertComponentToLayer (#btn-comp-detach, the layer context menu and the Motion row menu) is
 * bound the same way to the native object.component.breakApart request: one history entry, the layer stays selected, a refusal says why. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeComponentConvertUI = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var ENTRY = 'convertActiveLayerToComponent';
  var BREAK = 'convertComponentToLayer';
  var bindings = new WeakMap();
  var PREFIX = 'Impossible de convertir : '; // the baseline notice of app.js convertLayerToComponent

  function bind(root, ports) {
    if (bindings.has(root)) return bindings.get(root);
    var target = root.SM;
    if (!target || typeof target[ENTRY] !== 'function') return null;
    var original = target[ENTRY], retired = false, pending = false;

    function t(key) { return root.SM && typeof root.SM.t === 'function' ? root.SM.t(key) : key; }
    function toast(text) { try { if (typeof root.showToast === 'function') root.showToast(text); } catch (_) { /* presentation only */ } }
    function refuse(text) { toast(text); return Promise.resolve(false); }

    // The layers the entry acts on, as baseline timeline.js: the multi-selection, else the active layer.
    function selected(rows) {
      var sel = Array.isArray(root._layerSel) && root._layerSel.length > 1 ? root._layerSel.slice() : [root.state.activeLayerIdx];
      var seen = {};
      sel = sel.filter(function (i) { if (seen[i]) return false; seen[i] = true; return true; });
      return sel.every(function (i) { return Number.isInteger(i) && i >= 0 && i < rows.length && !!rows[i]; })
        ? sel.sort(function (a, b) { return a - b; }) : null;
    }
    function convert() {
      if (retired) return Promise.resolve(false);
      var session = ports.currentSession();
      if (!session) return original.apply(this, arguments); // a legacy document keeps its (guarded) entry
      if (pending) return Promise.resolve(false);
      var state = root.state, rows = state && state.layers, info, component;
      try {
        info = session.isActive() === true ? session.info() : null;
        component = root.NemoNativeObjectComponent && typeof root.NemoNativeObjectComponent.desktop === 'function'
          ? root.NemoNativeObjectComponent.desktop(root) : null;
      } catch (_) { info = null; }
      if (!info || !component || !Array.isArray(rows) || !Array.isArray(info.layers) || ports.projectionReady(info) !== true) {
        return refuse('Native component conversion is unavailable.');
      }
      // The baseline refusals, before anything is sent.
      if (state.activeSymbolId) return refuse(t('toastCloseComponentFirst'));
      var indices = selected(rows);
      if (!indices || !indices.length) return refuse(t('labsToastLayerInvalid'));
      var uids = indices.map(function (i) { return rows[i].layerUid; });
      if (uids.some(function (uid, k) { return typeof uid !== 'string' || !uid || !info.layers[indices[k]] || info.layers[indices[k]].layerUid !== uid; })) {
        return refuse('Native component conversion is unavailable.');
      }
      var reason = null;
      if (typeof root.badComponentSourceReason === 'function') {
        indices.some(function (i) { reason = root.badComponentSourceReason(rows[i]); return !!reason; });
      }
      if (reason) return refuse(indices.length > 1 ? t('toastCannotConvertSelectionContains') + reason : PREFIX + reason);
      var name = rows[indices[0]].name, many = indices.length > 1;
      pending = true;
      return Promise.resolve(component.convert(uids)).then(function (outcome) {
        if (retired || !outcome || outcome.ok !== true) {
          if (!retired) toast(PREFIX + (outcome && (outcome.message || outcome.reason || outcome.code) || 'unavailable'));
          return false;
        }
        // The result layer: the converted layer itself (one) or the merged instance (several), as the baseline selected it.
        var next = session.info(), uid = outcome.instanceLayerUid;
        var at = next && Array.isArray(next.layers) ? next.layers.findIndex(function (layer) { return layer && layer.layerUid === uid; }) : -1;
        if (at >= 0 && ports.projectionReady(next) === true) ports.selectLayer(at, false, uid, next);
        toast(many ? t('toastComponentCreatedWith') + uids.length + ' calques' : t('toastComponentCreated') + name + ' (Comp)');
        return true;
      }).catch(function () { toast('Native component conversion was not confirmed.'); return false; })
        .then(function (done) { pending = false; return done; });
    }
    // Break apart (convertComponentToLayer): the three surfaces (#btn-comp-detach, the layer and Motion context menus) all end in
    // SM.convertComponentToLayer. Natively it is ONE object.component.breakApart request for the active layer's instance; the native
    // owner evaluates every frame and bakes it, so the baseline refusals are the only checks made here. No JS fallback writer.
    var originalBreak = typeof target[BREAK] === 'function' ? target[BREAK] : null;
    function breakApart() {
      if (retired) return Promise.resolve(false);
      var session = ports.currentSession();
      if (!session) return originalBreak ? originalBreak.apply(this, arguments) : Promise.resolve(false);
      if (pending) return Promise.resolve(false);
      var state = root.state, rows = state && state.layers, info, component;
      try {
        info = session.isActive() === true ? session.info() : null;
        component = root.NemoNativeObjectComponent && typeof root.NemoNativeObjectComponent.desktop === 'function'
          ? root.NemoNativeObjectComponent.desktop(root) : null;
      } catch (_) { info = null; }
      if (!info || !component || typeof component.breakApart !== 'function' || !Array.isArray(rows) || !Array.isArray(info.layers) ||
        ports.projectionReady(info) !== true) {
        return refuse('Native component break apart is unavailable.');
      }
      if (state.activeSymbolId) return refuse(t('toastCloseComponentFirst'));
      var at = state.activeLayerIdx, row = rows[at], found = Number.isInteger(at) ? info.layers[at] : null;
      if (!row || !found || found.layerUid !== row.layerUid || found.kind !== 'component') return refuse(t('toastNotAComponent'));
      var uid = found.layerUid;
      pending = true;
      return Promise.resolve(component.breakApart(uid)).then(function (outcome) {
        if (retired || !outcome || outcome.ok !== true) {
          if (!retired) toast('Component could not be broken apart: ' + (outcome && (outcome.message || outcome.reason || outcome.code) || 'unavailable'));
          return false;
        }
        var next = session.info();
        var to = next && Array.isArray(next.layers) ? next.layers.findIndex(function (layer) { return layer && layer.layerUid === uid; }) : -1;
        if (to >= 0 && ports.projectionReady(next) === true) ports.selectLayer(to, false, uid, next);
        toast(t('toastComponentBrokenToLayer'));
        return true;
      }).catch(function () { toast('Native component break apart was not confirmed.'); return false; })
        .then(function (done) { pending = false; return done; });
    }
    target[ENTRY] = convert;
    if (originalBreak) target[BREAK] = breakApart;
    var binding = Object.freeze({ convert: convert, breakApart: breakApart, dispose: function () {
      retired = true; pending = false;
      // An active or indeterminate native owner keeps the refusing entry.
      var session = ports.currentSession();
      if (session && session.blocksLegacy() === true) return false;
      if (root.SM === target && target[ENTRY] === convert) target[ENTRY] = original;
      if (root.SM === target && originalBreak && target[BREAK] === breakApart) target[BREAK] = originalBreak;
      bindings.delete(root); return true;
    } });
    bindings.set(root, binding); return binding;
  }
  function createPorts(root, base) {
    var entry = null;
    return Object.freeze({
      bindConvertEntries: function () { if (entry) return true; entry = bind(root, base); return !!entry; },
      disposeConvertEntries: function () { if (entry && entry.dispose() === true) entry = null; }
    });
  }
  function compose(root, base, policy) {
    var own = createPorts(root, base);
    return Object.freeze(Object.assign({}, policy, {
      bindPolicyEntries: function () { var ready = policy.bindPolicyEntries(); return own.bindConvertEntries() && ready; },
      disposePolicyEntries: function () { own.disposeConvertEntries(); policy.disposePolicyEntries(); }
    }));
  }
  return Object.freeze({ bind: bind, createPorts: createPorts, compose: compose });
}));
