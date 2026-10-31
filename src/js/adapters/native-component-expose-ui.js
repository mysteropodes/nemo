/* #2256: the canvas context menu's "Expose opacity / visibility as a Component property" entries (baseline select-bridge.js onContext)
 * under the native owner. While a native object session owns the document, `menuItems` answers the two entries for ONE selected shape
 * of an entered component, and an entry sends the existing native expose transaction (NemoNativeObjectComponent.expose, the
 * command.document.object.component.expose request: symbolId = the held component, key ep_<clock36>_<rand> minted by the intent,
 * targetStrokeId = the native item id of the presented selection, field 'opacity' or '__visible'). The toast follows an ok:true
 * answer only; a typed refusal shows its reason and nothing is written. SM.exposeSymbolProperty (the legacy writer, which returns
 * false once a native session blocks it) is never called. For a legacy document `menuItems` answers null and select-bridge keeps
 * its original entries. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeComponentExposeUI = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var SCENE = 'scene-root';
  var PREFIX = "Impossible d'exposer : ";
  var pending = new WeakSet();

  function owned(win) {
    try {
      var P = win.NemoNativeObjectProject, s = P && typeof P.desktop === 'function' ? P.desktop(win) : null;
      return !!s && s.blocksLegacy() === true;
    } catch (_) { return false; }
  }
  function toast(win, text) { try { if (typeof win.showToast === 'function') win.showToast(text); } catch (_) { /* presentation only */ } }
  function held(win) {
    var c = win.NemoNativeComponentContext;
    try { return c && typeof c.contextId === 'function' ? c.contextId(win) : SCENE; } catch (_) { return SCENE; }
  }
  // The default of an opacity property: the item's listed opacity (0..1) as the baseline's whole percent, 100 when it lists none.
  function opacityDefault(item) {
    var style = item.data && item.data.nativeStyle, v = style && typeof style.opacity === 'number' ? style.opacity : item.opacity;
    return Math.round((typeof v === 'number' ? v : 1) * 100);
  }
  function expose(win, symbolId, strokeId, field, ask, fallback, defaultValue) {
    var label = typeof win.prompt === 'function' ? win.prompt('Nom de cette propriété (visible dans le panneau Motion de chaque instance) :', ask) : null;
    if (!label) return Promise.resolve(false);
    var component = null;
    try { component = win.NemoNativeObjectComponent && typeof win.NemoNativeObjectComponent.desktop === 'function' ? win.NemoNativeObjectComponent.desktop(win) : null; } catch (_) { component = null; }
    if (!component) { toast(win, PREFIX + 'unavailable'); return Promise.resolve(false); }
    if (pending.has(win)) return Promise.resolve(false);
    pending.add(win);
    return Promise.resolve().then(function () { return component.expose(symbolId, strokeId, field, label, defaultValue); }).then(function (out) {
      if (out && out.ok === true) { toast(win, 'Propriété "' + label + '" exposée sur ce Component.'); return true; }
      toast(win, PREFIX + (out && (out.message || out.reason || out.code) || fallback));
      return false;
    }, function () { toast(win, PREFIX + fallback); return false; }).then(function (done) { pending.delete(win); return done; });
  }
  // null: a legacy document (the original entries stand). Otherwise the entries to add: none unless a shape of an entered component is the single selection.
  function menuItems(win, selected) {
    if (!win || !owned(win)) return null;
    var state = win.state, symbolId = held(win), item = Array.isArray(selected) && selected.length === 1 ? selected[0] : null;
    if (!state || !state.activeSymbolId || symbolId === SCENE || !item || !item.data || !item.data.strokeId) return [];
    var strokeId = item.data.strokeId;
    return [
      { label: "Exposer l'opacité comme propriété de Component…", action: function () { return expose(win, symbolId, strokeId, 'opacity', 'Opacité', 'unavailable', opacityDefault(item)); } },
      { label: 'Exposer la visibilité comme propriété de Component…', action: function () { return expose(win, symbolId, strokeId, '__visible', 'Visible', 'unavailable', 100); } },
    ];
  }
  return Object.freeze({ menuItems: menuItems });
}));
