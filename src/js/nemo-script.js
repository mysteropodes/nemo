// ---- NEMO SCRIPTING API ----
//
// Nemo's own extensibility: a `nemo` object exposing this app's model in this
// app's vocabulary, plus a panel builder so a script can put a real interface
// on screen.
//
// WHY THIS EXISTS RATHER THAN THE OTHER THING. An earlier experiment ran
// After Effects scripts by emulating Adobe's DOM. It worked, but a legal audit
// put the exposure squarely on ADOBE'S VOCABULARY — the object graph, the SDK
// constants, claiming to be "AEFT" — and not on the capability itself. So the
// capability stays and the vocabulary goes: everything here is named after
// Nemo's own concepts (layers, frames, keys, strokes), and nothing in this
// file or its plugin loader refers to another company's API.
//
// The design still borrows the two SHAPES that make script tooling pleasant,
// because those are generic and not anyone's property: a script can open a
// floating panel of controls, and a plugin can be an ordinary HTML page. The
// second is barely a design decision at all — we run in a browser.
//
// UNITS. Everything is in FRAMES, because that is what this app is. No
// seconds-to-frames translation layer, no rounding surprises. `nemo.fps` is
// there for a script that wants to compute one.
(function () {
  'use strict';

  var _log = [];
  function note(w) { _log.push(w); }
  function fail(msg) { throw new Error('nemo: ' + msg); }

  // ---- helpers over the app's own state ----
  function layerAt(i) {
    var ld = state.layers[i];
    if (!ld) fail('aucun calque à l\'index ' + i);
    return ld;
  }
  function indexOfName(name) {
    for (var i = 0; i < state.layers.length; i++) if (state.layers[i].name === name) return i;
    return -1;
  }
  var PROPS = ['position', 'anchor', 'scale', 'rotation', 'opacity'];
  function checkProp(p) {
    if (PROPS.indexOf(p) < 0) fail('propriété inconnue « ' + p + ' » (attendu : ' + PROPS.join(', ') + ')');
    return p;
  }

  // ---- Layer ----
  function Layer(i) { this.index = i; }
  Layer.prototype._ld = function () { return layerAt(this.index); };
  Object.defineProperty(Layer.prototype, 'name', {
    get: function () { return this._ld().name; },
    set: function (v) { this._ld().name = String(v); if (window.renderLayerList) renderLayerList(); }
  });
  Object.defineProperty(Layer.prototype, 'visible', {
    get: function () { return this._ld().visible !== false; },
    set: function (v) { this._ld().visible = !!v; if (window.renderLayerList) renderLayerList(); }
  });
  Object.defineProperty(Layer.prototype, 'locked', {
    get: function () { return !!this._ld().locked; },
    set: function (v) { this._ld().locked = !!v; if (window.renderLayerList) renderLayerList(); }
  });
  Object.defineProperty(Layer.prototype, 'parent', {
    get: function () {
      var uid = this._ld().parentLayerUid;
      var pi = uid && window.SMMotion ? SMMotion.findLayerIndexByUid(uid) : -1;
      return pi >= 0 ? new Layer(pi) : null;
    },
    set: function (l) {
      if (!window.SMMotion) return;
      SMMotion.setLayerParent(this.index, l ? SMMotion.ensureLayerUid(layerAt(l.index)) : null);
      if (window.renderLayerList) renderLayerList();
    }
  });
  // Frame range the layer is present for. Named in/out like the timeline UI
  // calls them, and in frames like everything else.
  Object.defineProperty(Layer.prototype, 'inFrame', {
    get: function () { var v = this._ld().inPoint; return v == null ? 0 : v; },
    set: function (f) { this._ld().inPoint = f | 0; if (window.renderTimeline) renderTimeline(); }
  });
  Object.defineProperty(Layer.prototype, 'outFrame', {
    get: function () { var v = this._ld().outPoint; return v == null ? state.totalFrames - 1 : v; },
    set: function (f) { this._ld().outPoint = f | 0; if (window.renderTimeline) renderTimeline(); }
  });

  // Static (unkeyed) transform value. Reading falls back to the property's
  // neutral value so a script never has to special-case "never touched".
  Layer.prototype.get = function (prop, frame) {
    checkProp(prop);
    var ld = this._ld();
    var f = frame == null ? state.currentFrame : frame;
    var v = window.SMMotion ? SMMotion.valueAtFrame(ld, prop, f) : null;
    if (v == null) v = (ld.motionStatic && ld.motionStatic[prop]) ||
      (prop === 'scale' ? [100, 100] : prop === 'opacity' ? [100] : [0, 0]);
    return Array.isArray(v) ? v.slice() : [v];
  };
  Layer.prototype.set = function (prop, value) {
    checkProp(prop); note('set ' + prop);
    var ld = this._ld();
    if (!ld.motionStatic) ld.motionStatic = {};
    ld.motionStatic[prop] = Array.isArray(value) ? value.slice() : [value];
    return this;
  };
  // Keyframing. `ease` is one of the names the app's own UI uses, so a script
  // and the graph editor describe the same thing.
  var EASES = {
    linear: [{ x: 0, y: 0 }, { x: 1, y: 1 }],
    smooth: [{ x: 0, y: 0 }, { x: 0.25, y: 0.156 }, { x: 0.5, y: 0.5 }, { x: 0.75, y: 0.844 }, { x: 1, y: 1 }],
    strong: [{ x: 0, y: 0 }, { x: 0.25, y: 0.09 }, { x: 0.5, y: 0.5 }, { x: 0.75, y: 0.91 }, { x: 1, y: 1 }],
    easeIn: [{ x: 0, y: 0 }, { x: 0.25, y: 0.25 }, { x: 0.5, y: 0.5 }, { x: 0.75, y: 0.91 }, { x: 1, y: 1 }],
    easeOut: [{ x: 0, y: 0 }, { x: 0.25, y: 0.09 }, { x: 0.5, y: 0.5 }, { x: 0.75, y: 0.75 }, { x: 1, y: 1 }]
  };
  Layer.prototype.key = function (prop, frame, value, ease) {
    checkProp(prop); note('key ' + prop + ' @' + frame);
    var ld = this._ld();
    if (!ld.motion) ld.motion = {};
    if (!ld.motion[prop]) ld.motion[prop] = { keys: [] };
    var trk = ld.motion[prop];
    var arr = Array.isArray(value) ? value.slice() : [value];
    var curve = EASES[ease || 'smooth'];
    if (!curve) fail('ease inconnu « ' + ease + ' » (attendu : ' + Object.keys(EASES).join(', ') + ')');
    var ex = trk.keys.filter(function (k) { return k.frame === frame; })[0];
    if (ex) { ex.v = arr; ex.curvePoints = JSON.parse(JSON.stringify(curve)); ex.hold = false; }
    else {
      trk.keys.push({ frame: frame | 0, v: arr, curvePoints: JSON.parse(JSON.stringify(curve)), hOut: [0, 0], hIn: [0, 0] });
      trk.keys.sort(function (a, b) { return a.frame - b.frame; });
    }
    return this;
  };
  Layer.prototype.hold = function (prop, frame) {
    checkProp(prop);
    var trk = (this._ld().motion || {})[prop];
    var k = trk && trk.keys.filter(function (x) { return x.frame === frame; })[0];
    if (k) k.hold = true;
    return this;
  };
  Layer.prototype.keys = function (prop) {
    checkProp(prop);
    var trk = (this._ld().motion || {})[prop];
    return trk ? trk.keys.map(function (k) { return { frame: k.frame, value: k.v.slice(), hold: !!k.hold }; }) : [];
  };
  Layer.prototype.clearKeys = function (prop) {
    checkProp(prop);
    var ld = this._ld();
    if (ld.motion && ld.motion[prop]) delete ld.motion[prop];
    return this;
  };
  // Strokes drawn on this layer at a given frame — read-only introspection, so
  // a script can count, measure or report on artwork.
  Layer.prototype.strokes = function (frame) {
    var f = frame == null ? state.currentFrame : frame;
    var fr = this._ld().frames[f];
    return (fr && fr.strokes ? fr.strokes : []).map(function (s) {
      return { id: s.strokeId || null, points: (s.segments || []).length, closed: !!s.closed,
               strokeColor: s.strokeColor || null, fillColor: s.fillColor || null };
    });
  };
  // ---- effects ----
  // Reads and writes go through SMEffectKeys (effects-panel.js) rather than
  // touching the key objects here: one definition of the shape, so a script
  // and the effects panel can never disagree about what a key looks like.
  function Effect(li, i) { this._li = li; this._i = i; }
  Effect.prototype._e = function () {
    var fx = (state.layers[this._li].effects || [])[this._i];
    if (!fx) fail('aucun effet à l\'index ' + this._i);
    return fx;
  };
  Object.defineProperty(Effect.prototype, 'type', { get: function () { return this._e().type; } });
  Object.defineProperty(Effect.prototype, 'enabled', {
    get: function () { return !!this._e().enabled; },
    set: function (v) { this._e().enabled = !!v; if (window.updateEffectsPanel) updateEffectsPanel(); }
  });
  Effect.prototype.params = function () {
    return window.SMEffectKeys ? SMEffectKeys.paramNames(this._e().type) : [];
  };
  Effect.prototype.get = function (param, frame) {
    var f = frame == null ? state.currentFrame : frame;
    return window.SMEffectKeys ? SMEffectKeys.valueAt(this._e(), param, f) : this._e()[param];
  };
  Effect.prototype.set = function (param, value) {
    note('effect.set ' + param);
    this._e()[param] = value;
    return this;
  };
  Effect.prototype.key = function (param, frame, value, ease) {
    note('effect.key ' + param + ' @' + frame);
    var curve = EASES[ease || 'smooth'];
    if (!curve) fail('ease inconnu « ' + ease + ' » (attendu : ' + Object.keys(EASES).join(', ') + ')');
    if (!window.SMEffectKeys) fail('panneau d\'effets indisponible');
    SMEffectKeys.setKey(this._e(), param, frame, value, JSON.parse(JSON.stringify(curve)));
    return this;
  };
  Effect.prototype.keys = function (param) {
    var trk = (this._e().keys || {})[param];
    return trk ? trk.keys.map(function (k) { return { frame: k.frame, value: k.v[0] }; }) : [];
  };
  Effect.prototype.remove = function () {
    var fx = state.layers[this._li].effects;
    if (fx) fx.splice(this._i, 1);
    if (window.updateEffectsPanel) updateEffectsPanel();
  };
  Layer.prototype.effects = function () {
    var li = this.index;
    return (this._ld().effects || []).map(function (_e, i) { return new Effect(li, i); });
  };
  Layer.prototype.addEffect = function (type) {
    note('addEffect ' + type);
    var ld = this._ld();
    // Validate BEFORE creating: the panel's adder accepts any string, so an
    // unknown type used to produce an effect the renderer cannot draw — silent
    // nonsense instead of an error.
    var valid = window.SMEffectKeys ? SMEffectKeys.types() : null;
    if (valid && valid.indexOf(type) < 0) {
      fail('type d\'effet inconnu « ' + type + ' » (attendu : ' + valid.join(', ') + ')');
    }
    var prev = state.activeLayerIdx;
    // Goes through the panel's own adder so the effect gets its real defaults
    // for its type, rather than a hand-built object that would be missing them.
    window.SM.setActiveLayer(this.index);
    if (!window.addEffectToActiveLayer) fail('panneau d\'effets indisponible');
    addEffectToActiveLayer(type);
    window.SM.setActiveLayer(prev);
    if (!ld.effects || !ld.effects.length) fail('l\'effet « ' + type + ' » n\'a pas pu être créé');
    return new Effect(this.index, ld.effects.length - 1);
  };
  Layer.prototype.select = function () { window.SM.setActiveLayer(this.index); return this; };
  Layer.prototype.remove = function () { window.SM.setActiveLayer(this.index); window.SM.deleteLayer(); };
  Layer.prototype.duplicate = function () {
    var prev = state.activeLayerIdx;
    window.SM.setActiveLayer(this.index); window.SM.duplicateLayer();
    var ni = state.layers.length - 1;
    window.SM.setActiveLayer(prev);
    return new Layer(ni);
  };

  // ---- the nemo object ----
  function buildApi() {
    var api = {
      version: 1,
      get fps() { return state.fps; },
      get width() { return state.canvasW; },
      get height() { return state.canvasH; },
      get frameCount() { return state.totalFrames; },
      get frame() { return state.currentFrame; },
      set frame(f) { goToFrame(f | 0); },
      get layerCount() { return state.layers.length; },

      layer: function (which) {
        if (typeof which === 'string') {
          var i = indexOfName(which);
          if (i < 0) fail('aucun calque nommé « ' + which + ' »');
          return new Layer(i);
        }
        return new Layer(which | 0);
      },
      layers: function () { return state.layers.map(function (_l, i) { return new Layer(i); }); },
      activeLayer: function () { return new Layer(state.activeLayerIdx); },
      addLayer: function (name) {
        window.SM.addLayer();
        var i = state.layers.length - 1;
        if (name) state.layers[i].name = String(name);
        if (window.renderLayerList) renderLayerList();
        return new Layer(i);
      },
      // A layer that never renders, meant to be parented to — the app already
      // has the concept, this just names it for scripts.
      addPivot: function (name) {
        window.SM.addNullLayer();
        var i = state.layers.length - 1;
        if (name) state.layers[i].name = String(name);
        if (window.renderLayerList) renderLayerList();
        return new Layer(i);
      },

      // Frames
      goToFrame: function (f) { goToFrame(f | 0); return api; },
      isKeyframe: function (li, f) {
        var fr = layerAt(li).frames[f == null ? state.currentFrame : f];
        return !!(fr && fr.isKeyframe);
      },

      // Feedback + undo. One undo entry per script run is the sane default, so
      // `undoGroup` wraps a whole routine rather than each edit.
      toast: function (m) { if (window.showToast) showToast(String(m)); note('toast'); return api; },
      undoGroup: function (fn) {
        if (window.pushUndo) pushUndo();
        return fn();
      },
      log: function () { return _log.slice(); },

      // Everything a script can name, so `nemo.help()` answers "what can I do"
      // without leaving the app.
      help: function () {
        return {
          properties: PROPS,
          eases: Object.keys(EASES),
          layer: ['name', 'visible', 'locked', 'parent', 'inFrame', 'outFrame',
                  'get(prop,frame)', 'set(prop,value)', 'key(prop,frame,value,ease)',
                  'hold(prop,frame)', 'keys(prop)', 'clearKeys(prop)', 'strokes(frame)',
                  'effects()', 'addEffect(type)', 'select()', 'duplicate()', 'remove()'],
          effect: ['type', 'enabled', 'params()', 'get(param,frame)', 'set(param,value)',
                   'key(param,frame,value,ease)', 'keys(param)', 'remove()'],
          nemo: ['fps', 'width', 'height', 'frameCount', 'frame', 'layerCount',
                 'layer(nameOrIndex)', 'layers()', 'activeLayer()', 'addLayer(name)',
                 'addPivot(name)', 'goToFrame(f)', 'isKeyframe(li,f)', 'toast(msg)',
                 'undoGroup(fn)', 'ui.panel(opts)'],
          units: 'tout est en FRAMES (nemo.fps pour convertir)'
        };
      }
    };
    api.ui = { panel: function (opts) { return window.SMPanelUI.create(opts || {}); } };
    return api;
  }

  // ---- runner ----
  function run(source, opts) {
    opts = opts || {};
    _log = [];
    var api = buildApi();
    var fn;
    try {
      // The last expression is the script's result, which is what a console-
      // style tool wants; eval's completion value gives that, and "use strict"
      // keeps the script's own vars out of the page.
      fn = new Function('nemo', '__src', '"use strict";\nreturn eval(__src);');
    } catch (e) {
      return { ok: false, error: 'Erreur de syntaxe : ' + e.message, log: [] };
    }
    try {
      var v = fn(api, source);
      if (window.saveActiveLayerFrame) saveActiveLayerFrame();
      if (window.renderLayerList) renderLayerList();
      if (window.renderTimeline) renderTimeline();
      if (window.updateUI) updateUI();
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
      return { ok: true, value: v, log: _log.slice() };
    } catch (e) {
      var isSyntax = (e instanceof SyntaxError);
      return { ok: false, error: (isSyntax ? 'Erreur de syntaxe : ' : '') + e.message, log: _log.slice() };
    }
  }

  function openFile() {
    var inp = document.createElement('input');
    inp.type = 'file'; inp.accept = '.js,.nemo'; inp.style.display = 'none';
    document.body.appendChild(inp);
    inp.addEventListener('change', function () {
      var f = inp.files && inp.files[0];
      if (!f) { inp.remove(); return; }
      var rd = new FileReader();
      rd.onload = function () {
        var r = run(String(rd.result), { name: f.name });
        if (window.showToast) showToast(r.ok ? ('Script « ' + f.name + ' » exécuté') : ('« ' + f.name + ' » : ' + r.error));
        window.__nemoScriptLast = r;
        inp.remove();
      };
      rd.readAsText(f);
    });
    inp.click();
  }

  window.SMScript = { run: run, openFile: openFile, api: buildApi, help: function () { return buildApi().help(); } };
})();
