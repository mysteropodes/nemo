// ---- LABS PROTOTYPE — Auto Actions / macros (Clip Studio Paint) ----
// CSP's signature time-saver: record a sequence of commands once, replay
// it in one call. Here the recordable surface is a curated set of the
// same top-level functions every toolbar button already goes through —
// tool switches, frame ops, layer ops, playback toggles — instrumented
// with call-through wrappers ONLY while the flag is on (installed on
// enable, removed on disable, same runtime-wrapper pattern as
// out-of-pegs).
//
//   SMLabs.macroStart()          — begin recording
//   SMLabs.macroStop('nom')      — save the recording under a name
//   SMLabs.macroPlay('nom')      — replay
//   SMLabs.macroList()           — names + step counts
//   SMLabs.macroDelete('nom')
//
// Macros persist in localStorage (Labs-local, not the project file, same
// precedent as timeline-markers). Drawing strokes are NOT recorded — a
// macro replays COMMANDS, not ink; recording gestures would be a
// different (heavier) feature.
(function () {
  // NOT the same key as the prototype's own enable flag ('nemo-labs-macros')
  // — found live: enable() wrote '1' into the store, JSON.parse gave a
  // number, and every saved macro silently vanished into a primitive.
  var STORE = 'nemo-labs-macros-store';
  // name -> path to the real function, resolved at call time so wrappers
  // stack correctly even if another Labs prototype wraps the same target.
  var SURFACE = {
    'setTool': function () { return [window.SM, 'setTool']; },
    'goToFrame': function () { return [window, 'goToFrame']; },
    'insertKeyframe': function () { return [window, 'insertKeyframe']; },
    'insertBlankKeyframe': function () { return [window, 'insertBlankKeyframe']; },
    'insertFrame': function () { return [window, 'insertFrame']; },
    'removeFrame': function () { return [window, 'removeFrame']; },
    'addLayer': function () { return [window.SM, 'addLayer']; },
    'duplicateLayer': function () { return [window.SM, 'duplicateLayer']; },
    'toggleOnion': function () { return [window.SM, 'toggleOnion']; },
    'toggleLoopPlayback': function () { return [window.SM, 'toggleLoopPlayback']; },
  };
  var installed = {}, recording = null;

  function load() { try { return JSON.parse(localStorage.getItem(STORE) || '{}'); } catch (e) { return {}; } }
  function save(m) { localStorage.setItem(STORE, JSON.stringify(m)); }

  function install() {
    Object.keys(SURFACE).forEach(function (name) {
      var loc = SURFACE[name]();
      if (!loc || !loc[0] || typeof loc[0][loc[1]] !== 'function' || installed[name]) return;
      var target = loc[0], key = loc[1], orig = target[key];
      installed[name] = { target: target, key: key, orig: orig };
      target[key] = function () {
        if (recording) recording.push({ cmd: name, args: Array.prototype.slice.call(arguments) });
        return orig.apply(this, arguments);
      };
    });
  }
  function uninstall() {
    Object.keys(installed).forEach(function (name) {
      var w = installed[name];
      w.target[w.key] = w.orig;
    });
    installed = {};
    recording = null;
  }

  window.SMLabs.macroStart = function () {
    if (!window.SMLabs.isOn('auto-actions')) { console.warn('[labs] enable(\'auto-actions\') d\'abord'); return; }
    recording = [];
    if (typeof showToast === 'function') showToast('Macro : enregistrement… (macroStop(\'nom\') pour sauver)');
  };
  window.SMLabs.macroStop = function (name) {
    if (!recording) { console.warn('[labs] pas d\'enregistrement en cours'); return; }
    var steps = recording; recording = null;
    if (!name) { console.warn('[labs] macroStop(nom) — enregistrement jeté'); return; }
    var m = load(); m[name] = steps; save(m);
    if (typeof showToast === 'function') showToast('Macro « ' + name + ' » : ' + steps.length + ' étape(s)');
    return steps.length;
  };
  window.SMLabs.macroPlay = function (name) {
    var m = load(), steps = m[name];
    if (!steps) { console.warn('[labs] macro inconnue:', name, Object.keys(m)); return; }
    // Replay through the CURRENT function bindings (wrapped or not) with a
    // beat between steps so each one's UI settles like a human sequence.
    var i = 0;
    (function next() {
      if (i >= steps.length) { if (typeof showToast === 'function') showToast('Macro « ' + name + ' » rejouée'); return; }
      var s = steps[i++];
      var loc = SURFACE[s.cmd] && SURFACE[s.cmd]();
      if (loc && loc[0] && typeof loc[0][loc[1]] === 'function') loc[0][loc[1]].apply(loc[0], s.args);
      setTimeout(next, 40);
    })();
  };
  window.SMLabs.macroList = function () {
    var m = load();
    return Object.keys(m).map(function (n) { return { name: n, steps: m[n].length }; });
  };
  window.SMLabs.macroDelete = function (name) { var m = load(); delete m[name]; save(m); };

  window.SMLabs.register('auto-actions', {
    flag: 'nemo-labs-macros',
    describe: 'Auto Actions (Clip Studio) : macroStart() → actions → macroStop(\'nom\') → macroPlay(\'nom\') — enregistre les COMMANDES (outils/frames/calques), pas les traits',
    onEnable: install,
    onDisable: uninstall,
  });
  window.addEventListener('load', function () { if (window.SMLabs.isOn('auto-actions')) install(); });
})();
