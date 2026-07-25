// ---- EXTENDSCRIPT (.jsx) HOST ----
//
// Runs an UNMODIFIED After Effects script against Nemo's document. Not a
// converter: nothing rewrites the .jsx. The script runs as-is and this file
// answers the calls it makes, by presenting Nemo's model through the shape of
// the AE DOM (app.project, CompItem, Layer, Property, setValueAtTime...).
//
// WHY THIS WORKS AT ALL. ExtendScript is ES3 with extras; every ES3 program is
// valid modern JS, so the LANGUAGE needs no translation — only the HOST
// OBJECTS do. An .jsx that never touches AE's DOM (pure math, string work)
// already runs untouched. What breaks is `app.project.activeItem` and friends,
// and those are exactly what this file supplies.
//
// WHAT IT DELIBERATELY DOES NOT DO:
//   - no ScriptUI (Window/Panel/button). A script that builds a palette gets a
//     clear refusal naming ScriptUI, not a broken half-run. Wiring ScriptUI to
//     real DOM is a separate project.
//   - no expressions engine, no render queue, no file I/O, no effects beyond
//     the ones Nemo actually has.
//   - no silent approximation. Anything unimplemented THROWS with the AE name
//     it was asked for, so a script fails loudly at the exact call rather than
//     appearing to work while doing nothing. `SMAEScript.lastReport()` lists
//     every call made, so an unsupported script tells you precisely what it
//     needed.
//
// UNITS. AE time is SECONDS, Nemo is FRAMES — every time value crossing this
// boundary goes through secToFrame/frameToSec against the project fps.
// AE opacity is 0-100 and so is Nemo's. AE scale is [x,y] percent, same.
// AE position is [x,y] in comp pixels; Nemo's layer Position is an offset from
// the layer's own resting place, so an absolute AE position is stored as the
// delta from the comp centre — documented on posToNemo below, and the one
// place where "the same script" can look different between the two apps.
(function () {
  'use strict';

  var _log = [];
  function note(what) { _log.push(what); }
  function nyi(name) {
    // Loud, named failure. See the header: never approximate.
    throw new Error('Non supporté par le pont AE : ' + name);
  }

  function fps() { return Math.max(1, state.fps || 24); }
  function secToFrame(t) { return Math.round((t || 0) * fps()); }
  function frameToSec(f) { return (f || 0) / fps(); }
  function compW() { return state.canvasW || 1920; }
  function compH() { return state.canvasH || 1080; }

  // AE gives absolute comp coordinates; Nemo's layer Position is an offset
  // from where the layer already sits. Anchoring on the comp centre is what
  // makes `layer.position = [width/2, height/2]` — the single most common line
  // in AE scripts — mean "centre it" here too.
  function posToNemo(v) { return [(v[0] || 0) - compW() / 2, (v[1] || 0) - compH() / 2]; }
  function posToAE(v) { return [(v[0] || 0) + compW() / 2, (v[1] || 0) + compH() / 2]; }

  var PROP_MAP = {
    'Position': 'position', 'position': 'position',
    'Anchor Point': 'anchor', 'anchorPoint': 'anchor',
    'Scale': 'scale', 'scale': 'scale',
    'Rotation': 'rotation', 'rotation': 'rotation', 'Z Rotation': 'rotation',
    'Opacity': 'opacity', 'opacity': 'opacity'
  };

  // ---- Property ----
  function AEProperty(li, aeName) {
    var nemo = PROP_MAP[aeName];
    if (!nemo) nyi('property "' + aeName + '"');
    this._li = li; this._p = nemo; this.name = aeName;
    this.matchName = 'ADBE ' + aeName;
  }
  AEProperty.prototype._dims = function () {
    return (this._p === 'rotation' || this._p === 'opacity') ? 1 : 2;
  };
  AEProperty.prototype._read = function (frame) {
    var ld = state.layers[this._li];
    var v = window.SMMotion ? SMMotion.valueAtFrame(ld, this._p, frame) : null;
    if (v == null) v = (ld.motionStatic && ld.motionStatic[this._p]) || (this._p === 'scale' ? [100, 100] : this._p === 'opacity' ? [100] : [0, 0]);
    v = Array.isArray(v) ? v.slice() : [v];
    if (this._p === 'position') v = posToAE(v);
    return this._dims() === 1 ? v[0] : v;
  };
  AEProperty.prototype._write = function (frame, value, keyed) {
    var ld = state.layers[this._li];
    var arr = Array.isArray(value) ? value.slice() : [value];
    if (this._p === 'position') arr = posToNemo(arr);
    if (this._dims() === 2 && arr.length < 2) arr[1] = arr[0];
    if (!keyed) {
      if (!ld.motionStatic) ld.motionStatic = {};
      ld.motionStatic[this._p] = arr;
      return;
    }
    // Keyframing goes through SMMotion so the key carries the same default
    // ease, hOut/hIn and curvePoints shape the app's own UI produces — a key
    // built by hand here would render but not be editable like the others.
    if (!ld.motion) ld.motion = {};
    if (!ld.motion[this._p]) ld.motion[this._p] = { keys: [] };
    var trk = ld.motion[this._p];
    var ex = trk.keys.filter(function (k) { return k.frame === frame; })[0];
    if (ex) { ex.v = arr; return; }
    var proto = trk.keys[0];
    trk.keys.push({
      frame: frame, v: arr,
      curvePoints: proto && proto.curvePoints ? JSON.parse(JSON.stringify(proto.curvePoints))
        : [{ x: 0, y: 0 }, { x: 0.25, y: 0.156 }, { x: 0.5, y: 0.5 }, { x: 0.75, y: 0.844 }, { x: 1, y: 1 }],
      hOut: [0, 0], hIn: [0, 0]
    });
    trk.keys.sort(function (a, b) { return a.frame - b.frame; });
  };
  AEProperty.prototype.setValue = function (v) { note('setValue ' + this.name); this._write(0, v, false); };
  AEProperty.prototype.setValueAtTime = function (t, v) {
    note('setValueAtTime ' + this.name + ' @' + t + 's');
    this._write(secToFrame(t), v, true);
  };
  AEProperty.prototype.valueAtTime = function (t) { return this._read(secToFrame(t)); };
  AEProperty.prototype.removeKey = function (i) {
    var trk = (state.layers[this._li].motion || {})[this._p];
    if (trk && trk.keys[i - 1]) trk.keys.splice(i - 1, 1);
  };
  AEProperty.prototype.keyTime = function (i) {
    var trk = (state.layers[this._li].motion || {})[this._p];
    return trk && trk.keys[i - 1] ? frameToSec(trk.keys[i - 1].frame) : 0;
  };
  AEProperty.prototype.keyValue = function (i) {
    var trk = (state.layers[this._li].motion || {})[this._p];
    return trk && trk.keys[i - 1] ? this._read(trk.keys[i - 1].frame) : null;
  };
  Object.defineProperty(AEProperty.prototype, 'value', {
    get: function () { return this._read(state.currentFrame); },
    set: function (v) { this.setValue(v); }
  });
  Object.defineProperty(AEProperty.prototype, 'numKeys', {
    get: function () { var t = (state.layers[this._li].motion || {})[this._p]; return t ? t.keys.length : 0; }
  });

  // ---- Layer ----
  function AELayer(li) { this._li = li; }
  AELayer.prototype.property = function (n) { note('property ' + n); return new AEProperty(this._li, n); };
  AELayer.prototype.remove = function () { note('layer.remove'); window.SM.setActiveLayer(this._li); window.SM.deleteLayer(); };
  AELayer.prototype.moveToBeginning = function () { nyi('Layer.moveToBeginning'); };
  Object.defineProperty(AELayer.prototype, 'name', {
    get: function () { return state.layers[this._li].name; },
    set: function (v) { state.layers[this._li].name = String(v); if (window.renderLayerList) renderLayerList(); }
  });
  Object.defineProperty(AELayer.prototype, 'index', { get: function () { return this._li + 1; } });
  Object.defineProperty(AELayer.prototype, 'enabled', {
    get: function () { return state.layers[this._li].visible !== false; },
    set: function (v) { state.layers[this._li].visible = !!v; if (window.renderLayerList) renderLayerList(); }
  });
  Object.defineProperty(AELayer.prototype, 'locked', {
    get: function () { return !!state.layers[this._li].locked; },
    set: function (v) { state.layers[this._li].locked = !!v; if (window.renderLayerList) renderLayerList(); }
  });
  // AE's parenting maps 1:1 onto Nemo's own parentLayerUid chain.
  Object.defineProperty(AELayer.prototype, 'parent', {
    get: function () {
      var uid = state.layers[this._li].parentLayerUid;
      var pi = uid && window.SMMotion ? SMMotion.findLayerIndexByUid(uid) : -1;
      return pi >= 0 ? new AELayer(pi) : null;
    },
    set: function (l) {
      note('layer.parent');
      if (!window.SMMotion) return;
      SMMotion.setLayerParent(this._li, l ? SMMotion.ensureLayerUid(state.layers[l._li]) : null);
      if (window.renderLayerList) renderLayerList();
    }
  });
  ['position', 'scale', 'rotation', 'opacity', 'anchorPoint'].forEach(function (short) {
    Object.defineProperty(AELayer.prototype, short, {
      get: function () { return new AEProperty(this._li, short); },
      set: function (v) { new AEProperty(this._li, short).setValue(v); }
    });
  });

  // ---- LayerCollection ----
  function AELayers() {}
  AELayers.prototype.__getByIndex = function (i) {
    // AE collections are 1-based.
    if (i < 1 || i > state.layers.length) return null;
    return new AELayer(i - 1);
  };
  AELayers.prototype.addNull = function () {
    note('layers.addNull');
    window.SM.addNullLayer();
    return new AELayer(state.layers.length - 1);
  };
  AELayers.prototype.addSolid = function () { nyi('LayerCollection.addSolid'); };
  AELayers.prototype.addText = function () { nyi('LayerCollection.addText'); };
  AELayers.prototype.byName = function (n) {
    for (var i = 0; i < state.layers.length; i++) if (state.layers[i].name === n) return new AELayer(i);
    return null;
  };
  Object.defineProperty(AELayers.prototype, 'length', { get: function () { return state.layers.length; } });

  // ---- CompItem ----
  function AEComp() { this.layers = new AELayers(); this.typeName = 'Composition'; }
  Object.defineProperty(AEComp.prototype, 'name', {
    get: function () { return (window.SMProject && SMProject.getProjectName) ? SMProject.getProjectName() : 'Scene'; }
  });
  Object.defineProperty(AEComp.prototype, 'width', { get: function () { return compW(); } });
  Object.defineProperty(AEComp.prototype, 'height', { get: function () { return compH(); } });
  Object.defineProperty(AEComp.prototype, 'frameRate', { get: function () { return fps(); } });
  Object.defineProperty(AEComp.prototype, 'frameDuration', { get: function () { return 1 / fps(); } });
  Object.defineProperty(AEComp.prototype, 'duration', { get: function () { return frameToSec(state.totalFrames); } });
  Object.defineProperty(AEComp.prototype, 'numLayers', { get: function () { return state.layers.length; } });
  Object.defineProperty(AEComp.prototype, 'time', {
    get: function () { return frameToSec(state.currentFrame); },
    set: function (t) { goToFrame(secToFrame(t)); }
  });
  Object.defineProperty(AEComp.prototype, 'selectedLayers', {
    get: function () { return [new AELayer(state.activeLayerIdx)]; }
  });

  // ---- app / project ----
  function AEProject() { this.rootFolder = { items: [] }; }
  Object.defineProperty(AEProject.prototype, 'activeItem', { get: function () { return new AEComp(); } });
  Object.defineProperty(AEProject.prototype, 'numItems', { get: function () { return 1; } });
  AEProject.prototype.item = function (i) { return i === 1 ? new AEComp() : null; };

  function buildApp() {
    var app = {
      project: new AEProject(),
      version: '24.0 (Nemo AE bridge)',
      // Undo grouping is the one AE idiom worth honouring rather than
      // stubbing: a script that wraps its work in begin/endUndoGroup expects
      // ONE undo entry, and Nemo's pushUndo gives exactly that if we push at
      // the start and let everything after ride on it.
      beginUndoGroup: function (n) { note('beginUndoGroup ' + n); if (window.pushUndo) pushUndo(); },
      endUndoGroup: function () { note('endUndoGroup'); },
      purge: function () {},
      executeCommand: function () { nyi('app.executeCommand'); },
      findMenuCommandId: function () { nyi('app.findMenuCommandId'); },
      get activeViewer() { return nyi('app.activeViewer'); }
    };
    return app;
  }

  // A tiny ES3-era shim for the globals AE scripts reach for reflexively.
  function buildGlobals() {
    return {
      alert: function (m) { if (window.showToast) showToast(String(m)); note('alert: ' + m); },
      writeLn: function (m) { note('writeLn: ' + m); },
      $: { writeln: function (m) { note('$.writeln: ' + m); }, engineName: 'nemo', level: 0 },
      Window: function () { nyi('ScriptUI (Window)'); },
      Panel: function () { nyi('ScriptUI (Panel)'); },
      File: function () { nyi('File I/O'); },
      Folder: function () { nyi('Folder I/O'); }
    };
  }

  // Collections in AE are called with (), e.g. comp.layers(1). A JS object
  // can't be both callable and property-bearing unless it IS a function, so
  // the collection is handed to the script as a function carrying the same
  // methods — which is what makes `comp.layers(1)` AND `comp.layers.addNull()`
  // both work from the same object.
  function callableCollection(coll) {
    var f = function (i) {
      if (typeof i === 'string') return coll.byName(i);
      return coll.__getByIndex(i);
    };
    for (var k in coll) if (typeof coll[k] === 'function') f[k] = coll[k].bind(coll);
    f.addNull = coll.addNull.bind(coll);
    f.addSolid = coll.addSolid.bind(coll);
    f.addText = coll.addText.bind(coll);
    f.byName = coll.byName.bind(coll);
    Object.defineProperty(f, 'length', { get: function () { return coll.length; } });
    return f;
  }
  // Same trick for layer.property(...) vs layer.position — property() is
  // already a method, so only the collection needed wrapping.
  var _origLayers = Object.getOwnPropertyDescriptor(AEComp.prototype, 'layers');
  function wrapComp(c) { c.layers = callableCollection(c.layers); return c; }

  function run(source, opts) {
    opts = opts || {};
    _log = [];
    var app = buildApp();
    // Hand the script a comp whose .layers is callable, matching AE.
    var realActive = Object.getOwnPropertyDescriptor(AEProject.prototype, 'activeItem').get;
    Object.defineProperty(app.project, 'activeItem', { get: function () { return wrapComp(realActive.call(this)); }, configurable: true });
    var g = buildGlobals();
    var names = ['app'].concat(Object.keys(g));
    var vals = [app].concat(names.slice(1).map(function (n) { return g[n]; }));
    // ExtendScript returns the value of the script's LAST EXPRESSION — a bare
    // `myResult;` on the final line is idiomatic and common. new Function only
    // returns on an explicit `return`, so the source goes through eval, whose
    // completion value has exactly ExtendScript's semantics. Direct eval keeps
    // the host objects (app, alert, $) in scope since they are this function's
    // own parameters, and under "use strict" the script's `var`s stay inside
    // the eval instead of leaking into the page.
    var fn;
    try {
      fn = new Function(names.concat(['__aeSrc']).join(','), '"use strict";\nreturn eval(__aeSrc);');
    } catch (e) {
      return { ok: false, error: 'Erreur de syntaxe dans le pont : ' + e.message, log: _log.slice() };
    }
    try {
      var ret = fn.apply(null, vals.concat([source]));
      if (window.saveActiveLayerFrame) saveActiveLayerFrame();
      if (window.renderLayerList) renderLayerList();
      if (window.renderTimeline) renderTimeline();
      if (window.updateUI) updateUI();
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
      return { ok: true, value: ret, log: _log.slice() };
    } catch (e) {
      // A syntax error now surfaces from eval rather than from Function
      // construction, so it has to be recognised here instead.
      var isSyntax = (e instanceof SyntaxError);
      return { ok: false, error: (isSyntax ? 'Erreur de syntaxe dans le script : ' : '') + e.message, log: _log.slice() };
    }
  }

  // Opening a .jsx is a plain file read — this is what makes the feature
  // "ouvrir le script dans l'app" rather than "paste some code somewhere".
  // The result is reported through the app's own toast channel, and a refusal
  // names the AE feature that was missing so it's obvious WHY a given script
  // from aescripts didn't run.
  function openFile() {
    var inp = document.createElement('input');
    inp.type = 'file';
    inp.accept = '.jsx,.jsxinc,.js';
    inp.style.display = 'none';
    document.body.appendChild(inp);
    inp.addEventListener('change', function () {
      var f = inp.files && inp.files[0];
      if (!f) { inp.remove(); return; }
      var rd = new FileReader();
      rd.onload = function () {
        var res = run(String(rd.result), { name: f.name });
        if (window.showToast) {
          showToast(res.ok ? ('Script « ' + f.name + ' » exécuté — ' + res.log.length + ' appels')
                           : ('« ' + f.name + ' » : ' + res.error));
        }
        window.__aeLastResult = res;
        inp.remove();
      };
      rd.readAsText(f);
    });
    inp.click();
  }

  window.SMAEScript = {
    run: run,
    openFile: openFile,
    lastReport: function () { return _log.slice(); },
    supported: function () {
      return {
        properties: Object.keys(PROP_MAP),
        layer: ['name', 'index', 'enabled', 'locked', 'parent', 'position', 'scale', 'rotation', 'opacity', 'anchorPoint', 'property()', 'remove()'],
        comp: ['name', 'width', 'height', 'frameRate', 'frameDuration', 'duration', 'numLayers', 'time', 'layers(i)', 'layers.byName()', 'layers.addNull()', 'selectedLayers'],
        app: ['project.activeItem', 'project.item()', 'beginUndoGroup()', 'endUndoGroup()', 'version'],
        globals: ['alert()', '$.writeln()'],
        notSupported: ['ScriptUI (Window/Panel)', 'File/Folder I/O', 'expressions', 'render queue', 'addSolid/addText', 'executeCommand']
      };
    }
  };
})();
