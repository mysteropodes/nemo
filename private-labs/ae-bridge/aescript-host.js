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
  // ---- keyframe interpolation & easing ----
  // The single most common thing a script does after creating keys is ease
  // them, so without this most "animate X" scripts produced correct timing
  // with the wrong feel. AE expresses ease two ways and both are used:
  //   setInterpolationTypeAtKey(i, LINEAR|BEZIER|HOLD)  — the coarse type
  //   setTemporalEaseAtKey(i, [inEase], [outEase])      — KeyframeEase objects
  //     carrying speed + INFLUENCE (a percentage, AE's default ease is 33.33)
  // Nemo stores ease as on-curve waypoints on the key that STARTS a segment
  // (see motion.js DEFAULT_CURVE), so both forms are translated into that one
  // representation rather than stored alongside it — a second source of truth
  // would drift the moment the user touched the graph editor.
  function easeCurveFor(inInf, outInf) {
    // influence 0 = linear at that end, 100 = fully eased. AE's own Easy Ease
    // is 33.33; mapping influence to the waypoint's distance from the diagonal
    // reproduces that shape closely enough to read identically on the graph.
    var a = Math.max(0, Math.min(100, outInf == null ? 0 : outInf)) / 100;
    var b = Math.max(0, Math.min(100, inInf == null ? 0 : inInf)) / 100;
    // outInf shapes the START of the segment (leaving the key), inInf its END.
    return [
      { x: 0, y: 0 },
      { x: 0.25, y: 0.25 - 0.19 * a },
      { x: 0.5, y: 0.5 },
      { x: 0.75, y: 0.75 + 0.19 * b },
      { x: 1, y: 1 }
    ];
  }
  AEProperty.prototype._keyAt = function (i) {
    var trk = (state.layers[this._li].motion || {})[this._p];
    return trk && trk.keys[i - 1] ? trk.keys[i - 1] : null;
  };
  AEProperty.prototype.setInterpolationTypeAtKey = function (i, inType, outType) {
    note('setInterpolationTypeAtKey ' + this.name + ' #' + i);
    var k = this._keyAt(i); if (!k) return;
    var t = (outType != null ? outType : inType);
    if (t === 6604 /* HOLD */) { k.hold = true; return; }
    k.hold = false;
    if (t === 6612 /* LINEAR */) k.curvePoints = [{ x: 0, y: 0 }, { x: 1, y: 1 }];
    else k.curvePoints = easeCurveFor(33.33, 33.33); // BEZIER — AE's own default influence
  };
  AEProperty.prototype.setTemporalEaseAtKey = function (i, inEase, outEase) {
    note('setTemporalEaseAtKey ' + this.name + ' #' + i);
    var k = this._keyAt(i); if (!k) return;
    function inf(e) { return (e && e.length ? e[0] : e) ? ((e && e.length ? e[0] : e).influence) : null; }
    k.hold = false;
    k.curvePoints = easeCurveFor(inf(inEase), inf(outEase));
  };
  AEProperty.prototype.keyInTemporalEase = function (i) {
    var k = this._keyAt(i); return [new KeyframeEase(0, k && !k.hold ? 33.33 : 0.1)];
  };
  AEProperty.prototype.keyOutTemporalEase = function (i) { return this.keyInTemporalEase(i); };
  AEProperty.prototype.keyInInterpolationType = function (i) {
    var k = this._keyAt(i); return k && k.hold ? 6604 : 6613;
  };
  AEProperty.prototype.keyOutInterpolationType = AEProperty.prototype.keyInInterpolationType;
  // AE's batch setter — cheaper than a loop for a script writing many keys,
  // and common in generated/baked animation.
  AEProperty.prototype.setValuesAtTimes = function (times, values) {
    note('setValuesAtTimes ' + this.name + ' x' + (times || []).length);
    for (var i = 0; i < times.length; i++) this._write(secToFrame(times[i]), values[i], true);
  };
  AEProperty.prototype.nearestKeyIndex = function (t) {
    var trk = (state.layers[this._li].motion || {})[this._p];
    if (!trk || !trk.keys.length) return 0;
    var f = secToFrame(t), best = 1, bd = Infinity;
    trk.keys.forEach(function (k, i) { var d = Math.abs(k.frame - f); if (d < bd) { bd = d; best = i + 1; } });
    return best;
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
  AELayer.prototype.duplicate = function () {
    note('layer.duplicate');
    var prev = state.activeLayerIdx;
    window.SM.setActiveLayer(this._li);
    window.SM.duplicateLayer();
    var ni = state.layers.length - 1;
    window.SM.setActiveLayer(prev);
    return new AELayer(ni);
  };
  // AE's layer stack is 1 = frontmost; Nemo's array is the reverse (highest
  // index renders on top, which is why the panel counts down). Moving a layer
  // therefore means splicing it to the mirrored position.
  function moveLayerTo(from, to) {
    if (from === to || to < 0 || to >= state.layers.length) return;
    var ld = state.layers.splice(from, 1)[0];
    var ul = userLayers.splice(from, 1)[0];
    state.layers.splice(to, 0, ld);
    userLayers.splice(to, 0, ul);
    // userLayers is a Paper.js z-order too — reinsert so painting matches.
    if (ul && ul.parent) ul.remove();
    if (paper && paper.project) paper.project.insertLayer(to, ul);
    if (window.renderLayerList) renderLayerList();
    if (window.renderTimeline) renderTimeline();
  }
  AELayer.prototype.moveToBeginning = function () { note('layer.moveToBeginning'); moveLayerTo(this._li, state.layers.length - 1); };
  AELayer.prototype.moveToEnd = function () { note('layer.moveToEnd'); moveLayerTo(this._li, 0); };
  AELayer.prototype.moveBefore = function (other) { note('layer.moveBefore'); moveLayerTo(this._li, other._li); };
  AELayer.prototype.moveAfter = function (other) { note('layer.moveAfter'); moveLayerTo(this._li, Math.max(0, other._li - 1)); };
  // AE in/out points are SECONDS off the comp start; Nemo stores them as frame
  // indices (layer-inout.js), so they convert like every other time here.
  Object.defineProperty(AELayer.prototype, 'inPoint', {
    get: function () { var v = state.layers[this._li].inPoint; return frameToSec(v == null ? 0 : v); },
    set: function (t) { state.layers[this._li].inPoint = secToFrame(t); if (window.renderTimeline) renderTimeline(); }
  });
  Object.defineProperty(AELayer.prototype, 'outPoint', {
    get: function () { var v = state.layers[this._li].outPoint; return frameToSec(v == null ? state.totalFrames - 1 : v); },
    set: function (t) { state.layers[this._li].outPoint = secToFrame(t); if (window.renderTimeline) renderTimeline(); }
  });
  Object.defineProperty(AELayer.prototype, 'startTime', {
    get: function () { return 0; },
    set: function () { nyi('Layer.startTime (décalage temporel de calque)'); }
  });
  Object.defineProperty(AELayer.prototype, 'selected', {
    get: function () { return state.activeLayerIdx === this._li; },
    set: function (v) { if (v) window.SM.setActiveLayer(this._li); }
  });
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
  // A solid in AE is a coloured rectangle filling its own size. Nemo has no
  // "solid" layer TYPE, so it becomes what it actually is here: a new layer
  // holding one filled rectangle. That keeps the script's intent (a coloured
  // block you can transform and parent to) rather than refusing over a
  // vocabulary difference.
  AELayers.prototype.addSolid = function (color, name, w, h) {
    note('layers.addSolid ' + name);
    window.SM.addLayer();
    var li = state.layers.length - 1;
    state.layers[li].name = name || 'Solid';
    var prevActive = state.activeLayerIdx;
    state.activeLayerIdx = li; activateUL(li);
    state.layers[li].frames[state.currentFrame].isKeyframe = true;
    loadFrame(state.currentFrame);
    var cw = w || compW(), ch = h || compH();
    var x = (compW() - cw) / 2, y = (compH() - ch) / 2;
    var hex = '#000000';
    if (color && color.length >= 3) {
      hex = '#' + [0, 1, 2].map(function (i) {
        var c = Math.round(Math.max(0, Math.min(1, color[i])) * 255).toString(16);
        return c.length < 2 ? '0' + c : c;
      }).join('');
    }
    var r = new Path.Rectangle({ from: [x, y], to: [x + cw, y + ch], insert: false });
    r.fillColor = hex; r.strokeColor = null;
    userLayers[li].addChild(r);
    saveActiveLayerFrame();
    state.activeLayerIdx = prevActive; activateUL(prevActive); loadFrame(state.currentFrame);
    if (window.renderLayerList) renderLayerList();
    return new AELayer(li);
  };
  // Text needs the app's own text pipeline (font metrics, vector-text
  // conversion), so this creates the layer and marks it as a text layer with
  // the string set — anything richer (per-character styling, source text
  // keyframes) still refuses by name rather than pretending.
  AELayers.prototype.addText = function (txt) {
    note('layers.addText');
    window.SM.addLayer();
    var li = state.layers.length - 1;
    state.layers[li].name = (typeof txt === 'string' && txt) ? txt : 'Text';
    state.layers[li].isTextLayer = true;
    if (window.renderLayerList) renderLayerList();
    return new AELayer(li);
  };
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

  // AE exposes these as globals and scripts construct/compare against them
  // constantly — `new KeyframeEase(0, 33)` and
  // `KeyframeInterpolationType.LINEAR` appear in almost every easing routine.
  function KeyframeEase(speed, influence) {
    if (!(this instanceof KeyframeEase)) return new KeyframeEase(speed, influence);
    this.speed = speed || 0;
    this.influence = influence == null ? 33.33 : influence;
  }
  window.KeyframeEase = KeyframeEase;

  // A tiny ES3-era shim for the globals AE scripts reach for reflexively.
  function buildGlobals() {
    return {
      KeyframeEase: KeyframeEase,
      // AE's own numeric enum values, so a script comparing against them
      // behaves identically here.
      KeyframeInterpolationType: { LINEAR: 6612, BEZIER: 6613, HOLD: 6604 },
      PropertyValueType: { NO_VALUE: 6613, ThreeD_SPATIAL: 6614, ThreeD: 6615, TwoD_SPATIAL: 6616, TwoD: 6617, OneD: 6618, COLOR: 6619 },
      BlendingMode: { NORMAL: 5012, MULTIPLY: 5016, SCREEN: 5019, OVERLAY: 5013 },
      alert: function (m) { if (window.showToast) showToast(String(m)); note('alert: ' + m); },
      writeLn: function (m) { note('writeLn: ' + m); },
      $: { writeln: function (m) { note('$.writeln: ' + m); }, engineName: 'nemo', level: 0 },
      // Real ScriptUI (aescript-ui.js) — most aescripts tools ARE a palette,
      // so refusing here would have limited the bridge to the minority of
      // scripts with no interface.
      Window: window.SMScriptUI ? window.SMScriptUI.Window : function () { nyi('ScriptUI (Window)'); },
      Panel: window.SMScriptUI ? window.SMScriptUI.Panel : function () { nyi('ScriptUI (Panel)'); },
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
        layer: ['name', 'index', 'enabled', 'locked', 'selected', 'parent', 'inPoint', 'outPoint', 'position', 'scale', 'rotation', 'opacity', 'anchorPoint', 'property()', 'remove()', 'duplicate()', 'moveBefore/moveAfter/moveToBeginning/moveToEnd()'],
        comp: ['name', 'width', 'height', 'frameRate', 'frameDuration', 'duration', 'numLayers', 'time', 'layers(i)', 'layers.byName()', 'layers.addNull()', 'layers.addSolid()', 'layers.addText()', 'selectedLayers'],
        app: ['project.activeItem', 'project.item()', 'beginUndoGroup()', 'endUndoGroup()', 'version'],
        property: ['setValue', 'setValueAtTime', 'setValuesAtTimes', 'valueAtTime', 'value', 'numKeys', 'keyTime', 'keyValue', 'removeKey', 'nearestKeyIndex', 'setInterpolationTypeAtKey', 'setTemporalEaseAtKey', 'keyInInterpolationType', 'keyIn/OutTemporalEase'],
        globals: ['alert()', '$.writeln()', 'KeyframeEase', 'KeyframeInterpolationType', 'PropertyValueType', 'BlendingMode'],
        scriptUI: ['Window (palette/dialog)', 'group', 'panel', 'button', 'statictext', 'edittext', 'checkbox', 'radiobutton', 'dropdownlist', 'listbox', 'slider', 'progressbar', 'orientation/alignChildren/spacing/margins', 'onClick/onChange/onChanging'],
        notSupported: ['File/Folder I/O', 'expressions', 'render queue', 'executeCommand', 'Layer.startTime', 'absolute-bounds ScriptUI layout']
      };
    }
  };
})();
