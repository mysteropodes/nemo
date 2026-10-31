/* #2256 leaf 3: the ordinary enter / leave entries of a Component (SM.enterSymbol, SM.exitToScene, SM.closeSymbolTab: the Scene and
 * component tabs, the layer-row double-click, the Component panel) under the native owner. While a native object session owns the
 * document each entry asks NemoNativeComponentContext for ONE native workspace transition; for a legacy document the original (guarded)
 * entries are called unchanged. There is no JS fallback: the legacy enterSymbol / exitToScene / closeSymbolTab never run on a native
 * session. `openLayer` is the row double-click port of the native layer list. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeComponentEnterUI = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var NAMES = ['enterSymbol', 'exitToScene', 'closeSymbolTab'];
  var bindings = new WeakMap();

  function bind(root, ports) {
    if (bindings.has(root)) return bindings.get(root);
    var target = root.SM;
    if (!target || !NAMES.every(function (name) { return typeof target[name] === 'function'; })) return null;
    var original = {}, installed = {}, retired = false;
    NAMES.forEach(function (name) { original[name] = target[name]; });
    function owned() { var session = ports.currentSession(); return !!session && session.blocksLegacy() === true; }
    function route(name, nativeCall) {
      var entry = function () {
        if (retired || !owned()) return original[name].apply(this, arguments); // a legacy document keeps its (guarded) entry
        var context = root.NemoNativeComponentContext;
        return context ? nativeCall(context, arguments) : Promise.resolve(false);
      };
      installed[name] = entry; target[name] = entry;
    }
    route('enterSymbol', function (context, args) { return context.enter(root, args[0], null); });
    route('exitToScene', function (context) { return context.exit(root); });
    route('closeSymbolTab', function (context, args) { return context.close(root, args[0]); });
    var binding = Object.freeze({ dispose: function () {
      retired = true;
      // An active or indeterminate native owner keeps the refusing entries.
      if (owned()) return false;
      NAMES.forEach(function (name) { if (root.SM === target && target[name] === installed[name]) target[name] = original[name]; });
      bindings.delete(root); return true;
    } });
    bindings.set(root, binding); return binding;
  }
  function createPorts(root, base) {
    var entry = null;
    return Object.freeze({
      bindEnterEntries: function () { if (entry) return true; entry = bind(root, base); return !!entry; },
      disposeEnterEntries: function () { if (entry && entry.dispose() === true) entry = null; },
      openLayer: function (layerUid) {
        var context = root.NemoNativeComponentContext;
        return !!context && typeof layerUid === 'string' ? context.openLayer(root, layerUid) : false;
      }
    });
  }
  function compose(root, base, policy) {
    var own = createPorts(root, base);
    return Object.freeze(Object.assign({}, policy, {
      openLayer: own.openLayer,
      bindPolicyEntries: function () { var ready = policy.bindPolicyEntries(); return own.bindEnterEntries() && ready; },
      disposePolicyEntries: function () { own.disposeEnterEntries(); policy.disposePolicyEntries(); }
    }));
  }
  return Object.freeze({ bind: bind, createPorts: createPorts, compose: compose });
}));
