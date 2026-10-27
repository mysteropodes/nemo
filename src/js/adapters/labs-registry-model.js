/* Existing Labs behavior behind explicit caller-owned ports. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoLabsRegistryModel=api;}(typeof globalThis==='object'?globalThis:this,function(){
  function create(ports){
  var protos = {}; // name -> {flag, onStroke?, onEnable?, onDisable?, describe}

  function isOn(name) {
    var p = protos[name];
    return !!p && ports.getItem(p.flag) === '1';
  }
  function setOn(name, on, silent) {
    var p = protos[name];
    if (!p) { ports.warn('[labs] unknown prototype:', name, '— SMLabs.list()'); return false; }
    ports.setItem(p.flag, on ? '1' : '0');
    if (on && p.onEnable) p.onEnable();
    if (!on && p.onDisable) p.onDisable();
    if (!silent) ports.toast(name,on);
    return on;
  }

  var L={};
  L.register = function (name, def) { protos[name] = def; };
  L.isOn = isOn;
  L.enable = function (n) { return setOn(n, true); };
  L.disable = function (n) { return setOn(n, false); };
  L.toggle = function (n) { return setOn(n, !isOn(n)); };
  L.list = function () {
    return Object.keys(protos).map(function (n) {
      // `describe` is stored as an i18n KEY (not literal text) so the
      // Réglages > Labs panel (labs-panel.js's describeOf) always shows it
      // in the app's current language — resolved here, at read time, since
      // this file (and every labs/*.js registration) runs BEFORE i18n.js in
      // index.html's script order, so SM.t isn't available yet at the
      // moment SMLabs.register() itself is called (feedback #105: Labs
      // stayed French under an English app language because these strings
      // were hardcoded literals instead of going through SM.t).
      var key = protos[n].describe || '';
      var what = ports.describe(key);
      return { name: n, on: isOn(n), what: what };
    });
  };
  // Flags are plain origin-scoped localStorage, with no project identity
  // attached at all (2026-07-30 fix, QA sweep: "un Labs enabled dans un
  // projet carrie silencieusement dans tous les projets suivants, avec pour
  // seul indicateur un toast one-shot au moment du dessin"). Making this
  // properly per-project (stored inside the project JSON) would put
  // prototype-testing scaffolding into real project files — exactly what
  // "no Réglages UI on purpose... not shipped features" already rejects.
  // Simpler and true to that intent: OFF is the only state a project can
  // ever load into. Called from project.js's newProject() and timeline.js's
  // SM.importJSON() (non-silent — an explicit Open Project, not the silent
  // nemo-auto boot-time resume of the SAME session, which should still let
  // an in-progress Labs test survive a plain page refresh).
  L.resetAll = function () {
    Object.keys(protos).forEach(function (n) { if (isOn(n)) setOn(n, false, true); });
  };

  // Fan-out for draw-bridge.js's commit hook. Order = registration order
  // (index.html script order), which is deliberate: stroke-MUTATING
  // prototypes (predictive-stroke) must load before stroke-COPYING ones
  // (symmetry), so copies mirror the corrected shape, not the raw one.
  L.onStrokeCommitted = function (path, layer) {
    Object.keys(protos).forEach(function (n) {
      var p = protos[n];
      if (!p.onStroke || !isOn(n)) return;
      try { p.onStroke(path, layer); } catch (e) { ports.warn('[labs] ' + n + ' failed:', e); }
    });
  };

  // Fan-out for draw-bridge.js's LIVE preview hook (2026-07 — the mirror
  // must be visible while dragging, not only after commitStroke). Each
  // prototype's onPreview(samples, overlayItemFor) gets the in-progress
  // raw samples plus a helper that reuses draw-bridge.js's own overlayItem
  // shaping logic for an alternate samples array (see overlayItemFor's own
  // comment there) — it returns extra overlay item(s) to render alongside
  // the real in-progress stroke, or null/undefined for nothing.
  // The enabled prototypes whose draw hooks (onStroke/onPreview) have no native route yet: native Drawing refuses while
  // any is on rather than silently dropping its effect. A prototype that registers `nativeDraw: true` applies its hook
  // inside the native commit itself (U33, #2362: predictive-stroke).
  L.unmigratedDrawHooks = function () {
    return Object.keys(protos).filter(function (n) {
      var p = protos[n];
      return (p.onStroke || p.onPreview) && !p.nativeDraw && isOn(n);
    });
  };
  L.buildDrawPreviewExtras = function (samples, overlayItemFor) {
    var out = [];
    Object.keys(protos).forEach(function (n) {
      var p = protos[n];
      if (!p.onPreview || !isOn(n)) return;
      try {
        var r = p.onPreview(samples, overlayItemFor);
        if (r) out = out.concat(r);
      } catch (e) { ports.warn('[labs] ' + n + ' preview failed:', e); }
    });
    return out;
  };
  return L;
  }
  return Object.freeze({create:create});
}));
