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
  // timeRemap is keyable exactly like the transform properties — it just
  // drives WHICH internal frame a component instance shows instead of where
  // it sits. Only meaningful on a component layer, checked at use.
  var PROPS = ['position', 'anchor', 'scale', 'rotation', 'opacity', 'timeRemap'];
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
  // Read through the app's resolvers, never off the raw field. ld.inPoint is
  // only ONE of the three things that decide a layer's range — an explicit
  // value, a time link (Parent in Time), or auto-detection from its own
  // blank keyframes. Reading the field directly reported the stale explicit
  // value and made a linked layer look like it wasn't following; caught by
  // running an ordinary script against this API.
  function edgeSetter(which) {
    return function (f) {
      var ld = this._ld();
      f = f | 0;
      if (ld.timeLink) {
        // The link wins over ld.inPoint, so writing the field would be
        // silently discarded. Move the OFFSET instead: the layer keeps
        // following its source, at the distance just asked for. Same
        // contract as dragging a linked bar in the timeline.
        var src = null;
        state.layers.forEach(function (o) { if (o !== ld && o.layerUid === ld.timeLink.uid) src = o; });
        if (src && window.SMMotion) {
          var mode = ld.timeLink.mode || 'both';
          // Offsets are Motion properties (timeLinkInOffset/Out) — write
          // through the same public setter any other property uses.
          if (which === 'in' && mode !== 'out') SMMotion.setLayerValue(this.index, 'timeLinkInOffset', [f - layerInPoint(src)]);
          if (which === 'out' && mode !== 'in') SMMotion.setLayerValue(this.index, 'timeLinkOutOffset', [f - layerOutPoint(src)]);
          if (window.loadFrame) loadFrame(state.currentFrame);
          if (window.renderTimeline) renderTimeline();
          return;
        }
      }
      if (which === 'in') ld.inPoint = f; else ld.outPoint = f;
      if (window.renderTimeline) renderTimeline();
    };
  }
  Object.defineProperty(Layer.prototype, 'inFrame', {
    get: function () { return window.layerInPoint ? layerInPoint(this._ld()) : (this._ld().inPoint || 0); },
    set: edgeSetter('in')
  });
  Object.defineProperty(Layer.prototype, 'outFrame', {
    get: function () { return window.layerOutPoint ? layerOutPoint(this._ld()) : (this._ld().outPoint != null ? this._ld().outPoint : state.totalFrames - 1); },
    set: edgeSetter('out')
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

  // ---- layer switches -------------------------------------------------
  // Each goes through the app's own toggle rather than flipping the field,
  // because every one of them has a side effect the field alone doesn't
  // carry (a re-render, a toast, a canvas deselect when locking).
  function switchProp(name, getter, toggler) {
    Object.defineProperty(Layer.prototype, name, {
      get: function () { return getter(this._ld()); },
      set: function (v) {
        if (!!v === !!getter(this._ld())) return; // already there — don't toast twice
        toggler(this.index);
      }
    });
  }
  switchProp('solo', function (ld) { return !!ld.solo; }, function (i) { window.SM.toggleLayerSolo(i); });
  switchProp('shy', function (ld) { return !!ld.shy; }, function (i) { window.SM.toggleLayerShy(i); });
  switchProp('motionBlur', function (ld) { return !!ld.motionBlur; }, function (i) { window.SM.toggleLayerMotionBlur(i); });

  Object.defineProperty(Layer.prototype, 'color', {
    get: function () { return this._ld().color || null; },
    set: function (v) { this._ld().color = String(v); if (window.renderLayerList) renderLayerList(); }
  });
  Object.defineProperty(Layer.prototype, 'blendMode', {
    get: function () { return this._ld().blendMode || 'normal'; },
    set: function (v) { this._ld().blendMode = String(v); if (window.SMEngineBridge) SMEngineBridge.renderNow(); }
  });
  Object.defineProperty(Layer.prototype, 'matte', {
    get: function () { return this._ld().matteMode || 'none'; },
    set: function (v) { this._ld().matteMode = String(v); if (window.SMEngineBridge) SMEngineBridge.renderNow(); }
  });
  // Which edge the layer's keys are pinned to: 'in' | 'out' | 'layer' | null.
  Object.defineProperty(Layer.prototype, 'keyLock', {
    get: function () { return this._ld().keyLock || null; },
    set: function (v) {
      if (v && ['in', 'out', 'layer'].indexOf(v) < 0) fail('keyLock attendu : in, out, layer ou null');
      window.SM.setLayerKeyLock(this.index, v || null);
    }
  });

  // ---- time link (Parent in Time) --------------------------------------
  // Reads as a plain object so a script can inspect it; written through
  // linkTime/unlinkTime so the cycle refusal stays in one place.
  Object.defineProperty(Layer.prototype, 'timeLink', {
    get: function () {
      var tl = this._ld().timeLink;
      if (!tl) return null;
      var si = -1;
      state.layers.forEach(function (o, oi) { if (o.layerUid === tl.uid) si = oi; });
      // Offsets are Motion properties now (timeLinkInOffset/Out,
      // 2026-07-30) — read through the same public getter as any other
      // property, resolved (expression included) at the current frame,
      // not the raw legacy field.
      var inOff = window.SMMotion ? (SMMotion.getLayerValue(this.index, 'timeLinkInOffset') || [0])[0] : (tl.inOffset | 0);
      var outOff = window.SMMotion ? (SMMotion.getLayerValue(this.index, 'timeLinkOutOffset') || [0])[0] : (tl.outOffset | 0);
      return { layer: si >= 0 ? new Layer(si) : null, inOffset: inOff | 0, outOffset: outOff | 0, mode: tl.mode || 'both' };
    }
  });
  Layer.prototype.linkTime = function (other, opts) {
    if (!window.SMMotion) fail('Motion indisponible');
    if (!other || other.index === this.index) fail('un calque ne peut pas lier son temps au sien');
    opts = opts || {};
    var ld = this._ld(), src = layerAt(other.index);
    // Seeded from the CURRENT gap by default, so linking never makes the
    // layer jump — same contract as the pickwhip.
    var myIn = window.layerInPoint ? layerInPoint(ld) : 0, myOut = window.layerOutPoint ? layerOutPoint(ld) : state.totalFrames - 1;
    ld.timeLink = { uid: SMMotion.ensureLayerUid(src), mode: opts.mode || 'both' };
    SMMotion.setLayerValue(this.index, 'timeLinkInOffset', [opts.inOffset != null ? opts.inOffset | 0 : myIn - layerInPoint(src)]);
    SMMotion.setLayerValue(this.index, 'timeLinkOutOffset', [opts.outOffset != null ? opts.outOffset | 0 : myOut - layerOutPoint(src)]);
    if (window.loadFrame) loadFrame(state.currentFrame);
    note('linkTime');
    return this;
  };
  Layer.prototype.unlinkTime = function () { delete this._ld().timeLink; return this; };

  // ---- time remap ------------------------------------------------------
  Layer.prototype.enableTimeRemap = function () {
    if (!window.SMMotion) fail('Motion indisponible');
    if (!this._ld().symbolId) fail('le remappage temporel ne s\u2019applique qu\u2019à un calque composant');
    SMMotion.enableTimeRemap(this.index);
    return this;
  };
  Layer.prototype.disableTimeRemap = function () {
    if (window.SMMotion) SMMotion.disableTimeRemap(this.index);
    return this;
  };

  // ---- expressions -----------------------------------------------------
  // An expression is code evaluated per frame for one property. The sandbox
  // gives it time/frame/value/layer/wiggle/loopOut — see motion.js.
  Layer.prototype.expression = function (prop) {
    checkProp(prop);
    var ex = (this._ld().expressions || {})[prop];
    return ex ? { code: ex.code, enabled: !!ex.enabled, error: ex.lastError || null } : null;
  };
  Layer.prototype.setExpression = function (prop, code, enabled) {
    checkProp(prop); note('setExpression ' + prop);
    var ld = this._ld();
    if (!ld.expressions) ld.expressions = {};
    if (code == null || code === '') { delete ld.expressions[prop]; }
    else ld.expressions[prop] = { code: String(code), enabled: enabled !== false, lastError: null };
    // The compile cache is keyed on the code — drop it or the old one stands.
    if (ld._exprCompiled) delete ld._exprCompiled[prop];
    return this;
  };

  // ---- layer markers ---------------------------------------------------
  Layer.prototype.markers = function () {
    return (this._ld().markers || []).map(function (m) { return { frame: m.frame, name: m.name || '', color: m.color || null }; });
  };
  Layer.prototype.addMarker = function (frame, name) {
    if (!window.SMMarkers) fail('repères indisponibles');
    SMMarkers.addLayerMarker(this.index, frame == null ? state.currentFrame : frame | 0, name || '');
    return this;
  };

  // ---- structural ------------------------------------------------------
  // Cuts the layer in two at `frame`; returns the second half.
  Layer.prototype.splitAt = function (frame) {
    var before = state.layers.length;
    if (frame != null) goToFrame(frame | 0);
    window.SM.splitLayerAtPlayhead(this.index);
    if (state.layers.length === before) fail('coupe impossible : la frame est hors du calque');
    return new Layer(this.index + 1);
  };
  // Merges this layer with others into one. Returns the survivor.
  Layer.prototype.mergeWith = function (others) {
    var idx = [this.index].concat((others || []).map(function (l) { return l.index; }));
    if (!window.SM.mergeLayersIntoOne(idx)) fail('fusion impossible');
    return new Layer(Math.min.apply(null, idx));
  };
  // Applies a width profile to this layer's strokes at the current frame:
  // 'taper-both' | 'taper-in' | 'taper-out' | 'bulge' | 'even'.
  Layer.prototype.strokeProfile = function (kind) {
    var prev = state.activeLayerIdx;
    window.SM.setActiveLayer(this.index);
    var L = userLayers[this.index];
    window.selectedPaths = L ? L.children.slice() : [];
    window.SM.applyStrokeProfile(kind);
    window.selectedPaths = [];
    window.SM.setActiveLayer(prev);
    return this;
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
          layer: ['name', 'visible', 'locked', 'solo', 'shy', 'motionBlur', 'color',
                  'blendMode', 'matte', 'keyLock', 'parent', 'inFrame', 'outFrame',
                  'timeLink (lecture)',
                  'get(prop,frame)', 'set(prop,value)', 'key(prop,frame,value,ease)',
                  'hold(prop,frame)', 'keys(prop)', 'clearKeys(prop)',
                  'expression(prop)', 'setExpression(prop,code,enabled)',
                  'linkTime(layer,opts)', 'unlinkTime()',
                  'enableTimeRemap()', 'disableTimeRemap()',
                  'markers()', 'addMarker(frame,name)',
                  'splitAt(frame)', 'mergeWith([layers])', 'strokeProfile(kind)',
                  'strokes(frame)', 'effects()', 'addEffect(type)',
                  'select()', 'duplicate()', 'remove()'],
          effect: ['type', 'enabled', 'params()', 'get(param,frame)', 'set(param,value)',
                   'key(param,frame,value,ease)', 'keys(param)', 'remove()'],
          nemo: ['fps', 'width', 'height', 'frameCount', 'frame', 'layerCount',
                 'layer(nameOrIndex)', 'layers()', 'activeLayer()', 'addLayer(name)',
                 'addPivot(name)', 'goToFrame(f)', 'isKeyframe(li,f)', 'toast(msg)',
                 'undoGroup(fn)', 'ui.panel(opts)'],
          comp: ['fps', 'width', 'height', 'frameCount', 'frame', 'background',
                 'workArea', 'setWorkArea(in,out)', 'trimToWorkArea()',
                 'bpm', 'bpmOffset', 'beat(frame)', 'beats()',
                 'globals', 'mode'],
          markers: ['list()', 'add(frame,name)', 'removeAt(frame)', 'next()', 'prev()'],
          selection: ['layers()', 'setLayers([layers])', 'strokes()', 'clear()'],
          camera: ['keys()', 'enabled  — lecture seule'],
          profils: ['taper-both', 'taper-in', 'taper-out', 'bulge', 'even'],
          units: 'tout est en FRAMES (nemo.fps pour convertir)'
        };
      }
    };
    api.ui = { panel: function (opts) { return window.SMPanelUI.create(opts || {}); } };

    // ---- nemo.comp — the composition itself ----------------------------
    // Named `comp` because that is what this app calls the thing being
    // animated. Sizes and counts are read/write where the app has a real
    // setter, read-only where changing them would need a rebuild a script
    // has no way to trigger safely.
    api.comp = {
      get fps() { return state.fps; },
      get width() { return state.canvasW; },
      get height() { return state.canvasH; },
      get frameCount() { return state.totalFrames; },
      get frame() { return state.currentFrame; },
      set frame(f) { goToFrame(f | 0); },
      get background() { return state.canvasBg; },

      // Work area, in frames like everything else.
      get workArea() {
        return { in: state.waIn || 0, out: state.waOut != null ? state.waOut : state.totalFrames - 1 };
      },
      setWorkArea: function (inF, outF) {
        window.SM.setWorkArea(inF | 0, outF | 0);
        if (window.updateWaBar) updateWaBar();
        return api.comp;
      },
      // Drops everything outside the work area and rebases frame 0 on its
      // start — layers, keys, markers and the camera shift together.
      trimToWorkArea: function () { window.SM.trimToWorkArea(); return api.comp; },

      // Beat grid. `bpm` alone shows it; 0 hides it.
      get bpm() { return state.bpmShow ? state.bpm : 0; },
      set bpm(v) {
        var n = Number(v) || 0;
        state.bpm = n > 0 ? n : (state.bpm || 120);
        state.bpmShow = n > 0;
        if (window.SMBpm) SMBpm.refresh();
      },
      get bpmOffset() { return state.bpmOffset || 0; },
      set bpmOffset(v) { state.bpmOffset = v | 0; if (window.SMBpm) SMBpm.refresh(); },
      // Nearest beat to a frame — for snapping keys to the music.
      beat: function (frame) {
        return window.SMBpm ? SMBpm.snapFrame(frame == null ? state.currentFrame : frame | 0) : frame;
      },
      beats: function () { return window.SMBpm ? SMBpm.beatFrames().map(function (b) { return b.frame; }) : []; },

      // Code run before EVERY expression in the project.
      get globals() { return state.exprGlobals || ''; },
      set globals(code) { if (window.SMMotion) SMMotion.setExprGlobals(String(code || '')); },

      // Which of the app's three views is showing.
      get mode() { return state.appMode; },
      set mode(m) {
        if (['anim2d', 'motion', 'storyboard'].indexOf(m) < 0) fail('mode attendu : anim2d, motion, storyboard');
        if (window.SMMotion) SMMotion.setAppMode(m);
      }
    };

    // ---- nemo.markers — composition markers ----------------------------
    api.markers = {
      list: function () {
        return (state.markers || []).map(function (m) { return { frame: m.frame, name: m.name || '', color: m.color || null }; });
      },
      add: function (frame, name) {
        if (!window.SMMarkers) fail('repères indisponibles');
        var m = SMMarkers.addCompMarker(frame == null ? state.currentFrame : frame | 0, name || '');
        if (!m) fail('un repère existe déjà sur cette frame');
        return api.markers;
      },
      removeAt: function (frame) {
        var list = state.markers || [];
        for (var i = 0; i < list.length; i++) if (list[i].frame === (frame | 0)) { list.splice(i, 1); break; }
        if (window.SMMarkers) SMMarkers.render();
        return api.markers;
      },
      next: function () { if (window.SMMarkers) SMMarkers.gotoAdjacent(1); return api.markers; },
      prev: function () { if (window.SMMarkers) SMMarkers.gotoAdjacent(-1); return api.markers; }
    };

    // ---- nemo.selection ------------------------------------------------
    // Two different things are "selected" in this app and a script needs
    // both: which LAYERS are picked in the timeline, and which STROKES are
    // picked on the canvas. Keeping them apart avoids the ambiguity AE's
    // single `selectedLayers` creates.
    api.selection = {
      layers: function () {
        var sel = (window._layerSel && window._layerSel.length) ? window._layerSel : [state.activeLayerIdx];
        return sel.filter(function (i) { return state.layers[i]; }).map(function (i) { return new Layer(i); });
      },
      setLayers: function (arr) {
        window._layerSel = (arr || []).map(function (l) { return l.index; });
        if (window._layerSel.length) window.SM.setActiveLayer(window._layerSel[0]);
        if (window.renderLayerList) renderLayerList();
        if (window.renderTimeline) renderTimeline();
        return api.selection;
      },
      strokes: function () {
        return (window.selectedPaths || []).map(function (p) {
          return { id: (p.data && p.data.strokeId) || null, points: p.segments ? p.segments.length : 0,
                   closed: !!p.closed, profile: (p.data && p.data.strokeProfile) || null };
        });
      },
      clear: function () {
        if (typeof clearSel === 'function') clearSel();
        window._layerSel = [];
        if (window.renderLayerList) renderLayerList();
        return api.selection;
      }
    };

    // ---- nemo.camera ---------------------------------------------------
    // Read-only for now, deliberately: the camera's keys carry pan/zoom/roll
    // whose interaction with a Component's own camera is subtle (CLAUDE.md
    // §8), and a wrong write there is invisible until export.
    api.camera = {
      keys: function () {
        return (state.cameraKeys || []).map(function (k) { return { frame: k.frame, x: k.x, y: k.y, zoom: k.zoom, rot: k.rot }; });
      },
      get enabled() { return !!state.cameraLayerOn; }
    };

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
    // Two script idioms, both natural, and eval() only accepts one: a script
    // that ENDS in an expression yields it, but a script that says `return`
    // at top level is a syntax error inside eval — "Illegal return
    // statement", which reads like the author's mistake and isn't. Found by
    // writing an ordinary script against this API. So: run it as an
    // expression list, and on exactly that failure re-run it as a function
    // body, where `return` is legal. Neither idiom has to be taught.
    function runSource(src) {
      try { return { v: fn(api, src) }; }
      catch (e) {
        if (e instanceof SyntaxError && /return/i.test(e.message || '')) {
          try { return { v: new Function('nemo', '"use strict";\n' + src)(api) }; }
          catch (e2) { return { err: e2 }; }
        }
        return { err: e };
      }
    }
    var res = runSource(source);
    if (res.err) {
      var isSyntax0 = (res.err instanceof SyntaxError);
      return { ok: false, error: (isSyntax0 ? 'Erreur de syntaxe : ' : '') + res.err.message, log: _log.slice() };
    }
    try {
      var v = res.v;
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
