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
  function setOn(name, on, silent) {
    var p = protos[name];
    if (!p) { console.warn('[labs] unknown prototype:', name, '— SMLabs.list()'); return false; }
    localStorage.setItem(p.flag, on ? '1' : '0');
    if (on && p.onEnable) p.onEnable();
    if (!on && p.onDisable) p.onDisable();
    if (!silent && typeof showToast === 'function') showToast('Labs — ' + name + ' : ' + (on ? 'activé' : 'désactivé'));
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
      try { p.onStroke(path, layer); } catch (e) { console.warn('[labs] ' + n + ' failed:', e); }
    });
  };

  // Fan-out for draw-bridge.js's LIVE preview hook (2026-07 — the mirror
  // must be visible while dragging, not only after commitStroke). Each
  // prototype's onPreview(samples, overlayItemFor) gets the in-progress
  // raw samples plus a helper that reuses draw-bridge.js's own overlayItem
  // shaping logic for an alternate samples array (see overlayItemFor's own
  // comment there) — it returns extra overlay item(s) to render alongside
  // the real in-progress stroke, or null/undefined for nothing.
  L.buildDrawPreviewExtras = function (samples, overlayItemFor) {
    var out = [];
    Object.keys(protos).forEach(function (n) {
      var p = protos[n];
      if (!p.onPreview || !isOn(n)) return;
      try {
        var r = p.onPreview(samples, overlayItemFor);
        if (r) out = out.concat(r);
      } catch (e) { console.warn('[labs] ' + n + ' preview failed:', e); }
    });
    return out;
  };
})();
