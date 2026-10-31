/* Native script runner (CS8-SDK #2354, read increment).
 * Owner: Ilya/Claude clean-slate session. Fixture: tests/script-native-reads.test.cjs, tests/script-runner-native.test.cjs.
 * The desktop SMScript.run: one run over the active native object session, or a refusal. A script that cannot ask for a
 * track, key or timing read runs once and returns its result synchronously. One that can (script-native-reads.js needs())
 * runs in rounds over a pinned read cache - quiet passes collect the reads, the declared queries fetch them, the last pass
 * is the real one - and returns a Promise of the same result shape. A script therefore runs more than once before it takes
 * effect; its reads are deterministic and its writes and presentation calls happen only in the last pass.
 * make(ports, opts) is script-native-api.js createRun, injected so the runner does not depend on the API module.
 */
(function (root, factory) {
  var api = factory(typeof module === 'object' && module.exports ? require('./script-native-reads.js') : root.NemoScriptNativeReads);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoScriptNativeRun = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Reads) {
  'use strict';
  var REFUSED = 'Scripts run on an open native document only.', PIN = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  // Shadowing is hygiene for ordinary scripts, not isolation (see script-native-api.js).
  var SHADOWED = ['state', 'SM', 'SMScript', 'SMMotion', 'NemoNativeObjectProject', 'window', 'globalThis', 'self'];
  function compile() { return Function.apply(null, ['nemo', '__src'].concat(SHADOWED, ['"use strict";\nreturn eval(__src);'])); }
  function compileBody(source) { return Function.apply(null, ['nemo'].concat(SHADOWED, ['"use strict";\n' + source])); }
  function session(root) {
    var project = root.NemoNativeObjectProject, s = project && typeof project.desktop === 'function' && project.desktop(root);
    return s && s.blocksLegacy() && s.isActive() ? s : null;
  }
  // The desktop dispatch predicate shared with nemo-script.js: a Tauri host, or an open native document.
  function owns(root) { return !!(root.__TAURI__ && root.__TAURI__.core) || !!session(root); }
  function portsFor(root, s) {
    var sm = root.SM || {}, entry = function (name) { return function (index) { return typeof sm[name] === 'function' ? sm[name](index) : false; }; };
    var reads = {};
    if (typeof s.readProperty === 'function') reads.property = function (view) {
      var pin = s.info();
      return s.readProperty({ instanceId: pin.instanceId, documentId: pin.documentId, atRevision: pin.contentRevision, documentSnapshotId: pin.documentSnapshotId, view: view });
    };
    if (typeof s.readLayerCells === 'function') reads.cells = function (uid) { return s.readLayerCells(uid, s.info()); };
    // The composition controller's presented projection, only while it belongs to the pin the run reads.
    reads.composition = function () {
      var c = root.NemoNativeCompositionController, v = c && c.read(), i = s.info();
      return v && v.composition && v.info && PIN.every(function (k) { return v.info[k] === i[k]; }) ? v.composition : null;
    };
    return { info: function () { return s.isActive() ? s.info() : null; }, reads: reads,
      write: { visible: entry('toggleLayerVis'), locked: entry('toggleLayerLock'), shy: entry('toggleLayerShy') },
      ops: root.NemoScriptNativeWrites ? root.NemoScriptNativeWrites.create(root, s) : {},
      goToFrame: function (f) { if (typeof root.goToFrame === 'function') root.goToFrame(f); },
      frame: function () { return root.state && Number.isInteger(root.state.currentFrame) ? root.state.currentFrame : 0; },
      activeIndex: function () { return root.state ? root.state.activeLayerIdx : undefined; },
      selection: function () { return (root._layerSel || []).slice(); },
      toast: function (m) { if (typeof root.showToast === 'function') root.showToast(m); } };
  }
  // One evaluation of the script. A quiet pass sends nothing; the real pass commits its queued writes in order.
  function pass(make, ports, source, cache, quiet) {
    var run, value;
    try { run = make(ports, { cache: cache, quiet: quiet }); } catch (e) { return { ok: false, error: e.message, code: e.code, log: [] }; }
    try { value = compile()(run.nemo, source); }
    catch (e) {
      // A script that says `return` at top level runs as a function body (baseline idiom).
      var retry = e instanceof SyntaxError && /return/i.test(e.message || '');
      try { if (!retry) throw e; value = compileBody(source)(run.nemo); }
      catch (e2) {
        run.discard();
        return { ok: false, error: (e2 instanceof SyntaxError ? 'Erreur de syntaxe : ' : '') + e2.message, code: e2.code, log: run.log() };
      }
    }
    if (quiet) { run.discard(); return { ok: true, value: value, log: run.log() }; }
    // A failing script sends nothing; a successful one commits its queued writes in order.
    var committed = run.commit().then(function (c) { if (!c.ok) ports.toast('Script writes stopped: ' + c.error); return c; });
    return { ok: true, value: value, log: run.log(), committed: committed };
  }
  async function settle(make, ports, source) {
    var cache = Reads.cache(), rounds = 0;
    for (;;) {
      pass(make, ports, source, cache, true);
      if (!Reads.pending(cache)) break;
      if (++rounds > Reads.MAX_ROUNDS || !(await Reads.fill(cache, ports.reads))) return { ok: false, error: 'nemo: the script reads more than one run can hold', code: 'refused', log: [] };
    }
    return pass(make, ports, source, cache, false);
  }
  function execute(make, ports, source) { return Reads.needs(source) ? settle(make, ports, source) : pass(make, ports, source, null, false); }
  function runOnDesktop(root, source, make) {
    var s = session(root);
    return s ? execute(make, portsFor(root, s), source) : { ok: false, error: REFUSED, log: [] };
  }
  return Object.freeze({ owns: owns, runOnDesktop: runOnDesktop, execute: execute });
}));
