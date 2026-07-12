// ---- LABS CORE — registry for feature-scouting prototypes ----
// (docs/feature-scouting.md). Each prototype lives in its own file under
// js/labs/, registers itself here, and is gated by its own localStorage
// flag (all off by default). The ONLY touch to core files is the single
// guarded `window.SMLabs.onStrokeCommitted` hook in draw-bridge.js — this
// file fans that one hook out to every registered prototype, so adding a
// new prototype never means touching draw-bridge.js again.
//
// Console API (no Réglages UI on purpose — these are prototypes to try,
// not shipped features; a real toggle comes only if one is picked):
//   SMLabs.list()            — every prototype + on/off state
//   SMLabs.enable('name')    — turn one on (persisted)
//   SMLabs.disable('name')   — turn one off
//   SMLabs.toggle('name')
(function () {
  var protos = {}; // name -> {flag, onStroke?, onEnable?, onDisable?, describe}

  function isOn(name) {
    var p = protos[name];
    return !!p && localStorage.getItem(p.flag) === '1';
  }
  function setOn(name, on) {
    var p = protos[name];
    if (!p) { console.warn('[labs] unknown prototype:', name, '— SMLabs.list()'); return false; }
    localStorage.setItem(p.flag, on ? '1' : '0');
    if (on && p.onEnable) p.onEnable();
    if (!on && p.onDisable) p.onDisable();
    if (typeof showToast === 'function') showToast('Labs — ' + name + ' : ' + (on ? 'activé' : 'désactivé'));
    return on;
  }

  window.SMLabs = window.SMLabs || {};
  var L = window.SMLabs;

  L.register = function (name, def) { protos[name] = def; };
  L.isOn = isOn;
  L.enable = function (n) { return setOn(n, true); };
  L.disable = function (n) { return setOn(n, false); };
  L.toggle = function (n) { return setOn(n, !isOn(n)); };
  L.list = function () {
    return Object.keys(protos).map(function (n) {
      return { name: n, on: isOn(n), what: protos[n].describe || '' };
    });
  };

  // Fan-out for draw-bridge.js's commit hook. Order = registration order
  // (index.html script order), which is deliberate: stroke-MUTATING
  // prototypes (predictive-stroke) must load before stroke-COPYING ones
  // (symmetry), so copies mirror the corrected shape, not the raw one.
  L.onStrokeCommitted = function (path, layer) {
    Object.keys(protos).forEach(function (n) {
      var p = protos[n];
      if (!p.onStroke || !isOn(n)) return;
      try { p.onStroke(path, layer); } catch (e) { console.warn('[labs] ' + n + ' failed:', e); }
    });
  };
})();
