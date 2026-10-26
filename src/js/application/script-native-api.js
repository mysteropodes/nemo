/* Native script API (CS8-SDK #2354, first increment).
 * Owner: Ilya/Claude clean-slate session. Fixture: tests/script-native-api.test.cjs.
 * The public `nemo` vocabulary over a native document: every baseline sdk: row has one
 * route (ROUTES). Reads are synchronous over the run's pinned read model (detached
 * values, layers by layerUid); document writes are queued during the run and drained
 * afterwards, in order, through the ordinary UI's native entry points, so a script
 * reaches the same handler as the UI and never writes state.layers. A row that is not
 * bound here throws {code:'unavailable', row, reason} before any write is sent.
 * Injected ports: info(), write.{visible,locked,shy}(index) -> Promise<boolean>,
 * goToFrame(frame), frame(), activeIndex(), selection() -> indices, toast(message).
 * Undeclared members throw instead of reading undefined. The script body still runs
 * in the page realm: common globals are shadowed as hygiene, which is not a sandbox
 * (the baseline runner had the same unsandboxed shape, recorded under I30).
 */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoScriptNativeApi = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var O = 'command.document.object.', Q = 'query.document.object';
  function op(read, write, bound) { var r = { kind: 'operation', bound: !!bound }; if (read) r.read = read; if (write) r.write = write; return r; }
  function present(note, bound) { return { kind: 'presentation', note: note, bound: !!bound }; }
  function host(note, bound) { return { kind: 'host', note: note, bound: !!bound }; }
  function gone(reason) { return { kind: 'unavailable', reason: reason, bound: false }; }
  var S = 'sdk:nemo-script.', P = 'sdk:nemo-plugin.';
  var table = {};
  function put(prefix, names, route) { names.forEach(function (n) { table[prefix + n] = route; }); }
  put(S, ['layers', 'layer', 'activeLayer'], op(Q, null, true));
  put(S, ['Layer.visible'], op(Q, O + 'layer.visibility.set', true));
  put(S, ['Layer.locked', 'Layer.shy'], op(Q, O + 'layer.policy.edit', true));
  put(S, ['Layer.name'], Object.assign(op(Q, O + 'layer.rename', true), { writeBound: false }));
  put(S, ['isKeyframe'], op(Q + '.layer.timing'));
  put(S, ['addLayer'], op(null, O + 'layer.create'));
  put(S, ['Layer.remove'], op(null, O + 'layer.delete'));
  put(S, ['Layer.duplicate'], op(null, O + 'layer.duplicate'));
  put(S, ['Layer.splitAt'], op(null, O + 'layer.cut'));
  put(S, ['Layer.strokes'], op(Q));
  put(S, ['Layer.get', 'Layer.keys'], op(Q + '.property.read'));
  put(S, ['Layer.set'], op(null, O + 'property.edit'));
  put(S, ['Layer.key', 'Layer.hold', 'Layer.clearKeys'], op(null, O + 'property.track.edit'));
  put(S, ['comp.beat', 'comp.beats', 'markers.list', 'markers.next', 'markers.prev'], op(Q + '.project.composition.read'));
  put(S, ['markers.add', 'markers.removeAt'], op(null, O + 'project.composition.edit'));
  put(S, ['goToFrame'], present('viewport frame presentation; no document write', true));
  put(S, ['selection.layers'], present('editor selection state', true));
  put(S, ['selection.clear', 'selection.setLayers', 'selection.strokes', 'Layer.select'], present('editor selection state'));
  put(S, ['help', 'log', 'toast'], host('script console and editor toast', true));
  put(P, ['closeAll', 'openCount', 'setTitle'], host('plugin window management; the plugin host is not admitted yet'));
  var reasons = {
    'no-native-effects': ['Effect.enabled', 'Effect.get', 'Effect.key', 'Effect.keys', 'Effect.params', 'Effect.remove', 'Effect.set', 'Effect.type', 'Layer.addEffect', 'Layer.effects'],
    'no-native-layer-markers': ['Layer.addMarker', 'Layer.markers'], 'no-native-blend-matte': ['Layer.blendMode', 'Layer.matte'],
    'no-native-layer-color': ['Layer.color'], 'no-native-time-remap': ['Layer.enableTimeRemap', 'Layer.disableTimeRemap'],
    'no-native-expressions': ['Layer.expression', 'Layer.setExpression'], 'no-native-layer-range': ['Layer.inFrame', 'Layer.outFrame'],
    'no-native-key-lock': ['Layer.keyLock'], 'no-native-time-link': ['Layer.linkTime', 'Layer.timeLink', 'Layer.unlinkTime'],
    'no-native-layer-merge': ['Layer.mergeWith'], 'no-native-motion-blur': ['Layer.motionBlur'], 'no-native-parenting': ['Layer.parent'],
    'no-native-stroke-profile': ['Layer.strokeProfile'], 'no-native-pivot': ['addPivot'], 'no-native-camera': ['camera.keys'],
    'no-native-work-area': ['comp.setWorkArea', 'comp.trimToWorkArea'], 'no-native-solo': ['Layer.solo'], 'script-run-grouping': ['undoGroup'],
  };
  Object.keys(reasons).forEach(function (reason) { put(S, reasons[reason], gone(reason)); });
  var ROUTES = Object.freeze(Object.keys(table).sort().map(function (row) { return Object.freeze(Object.assign({ row: row }, table[row])); }));
  var byRow = Object.create(null); ROUTES.forEach(function (r) { byRow[r.row] = r; });
  var PIN = ['instanceId', 'documentId', 'lifecycleGeneration'];

  function unavailable(row) {
    var route = byRow[row], error = new Error('nemo: ' + row.replace(/^sdk:nemo-(script|plugin)\./, '') + ' is unavailable on a native document (' +
      (route ? route.bound ? 'bound' : route.kind === 'unavailable' ? route.reason : 'not-yet-bound' : 'undeclared') + ')');
    error.code = 'unavailable'; error.row = row; error.reason = route && route.kind === 'unavailable' ? route.reason : 'not-yet-bound';
    return error;
  }
  function need(row) { var route = byRow[row]; if (!route || !route.bound) throw unavailable(row); return route; }
  function fail(message) { var e = new Error('nemo: ' + message); e.code = 'refused'; return e; }
  // Every member a script can reach is declared; anything else throws rather than reading undefined.
  var INERT = { then: 1, toJSON: 1, constructor: 1, valueOf: 1, toString: 1 };
  function undeclared(label, key) {
    var e = new Error('nemo: ' + label + String(key) + ' is not part of the native script API'); e.code = 'unavailable'; e.reason = 'undeclared'; return e;
  }
  function guard(target, label) {
    if (typeof Proxy !== 'function') return target;
    return new Proxy(target, {
      get: function (t, key, receiver) {
        if (typeof key === 'symbol' || key in t || INERT[key]) return Reflect.get(t, key, receiver);
        throw undeclared(label, key);
      },
      set: function (t, key, value, receiver) {
        if (typeof key === 'symbol' || key in t) return Reflect.set(t, key, value, receiver);
        throw undeclared(label, key);
      },
    });
  }
  function detached(value) { return value === undefined ? undefined : JSON.parse(JSON.stringify(value)); }

  // One run: a pinned read model, a local overlay of queued flag writes, and the queue.
  function createRun(ports) {
    var start = ports.info();
    if (!start || !Array.isArray(start.layers)) throw fail('no native document is open');
    var pin = {}; PIN.forEach(function (k) { pin[k] = start[k]; });
    var layers = detached(start.layers), overlay = Object.create(null), queue = [], log = [];
    var seen = Object.create(null);
    layers.forEach(function (l) { if (seen[l.layerUid]) throw fail('duplicate layer id ' + l.layerUid); seen[l.layerUid] = true; });
    function find(uid) {
      for (var i = 0; i < layers.length; i++) if (layers[i].layerUid === uid) return { layer: layers[i], index: i };
      throw fail('layer ' + uid + ' is not in this document');
    }
    function flag(uid, key, value) {
      var own = overlay[uid] && Object.prototype.hasOwnProperty.call(overlay[uid], key);
      if (value === undefined) {
        if (own) return overlay[uid][key];
        var layer = find(uid).layer;
        return key === 'visible' ? layer.visible !== false : layer[key] === true;
      }
      var next = !!value;
      if (flag(uid, key) === next) return next;
      (overlay[uid] = overlay[uid] || {})[key] = next;
      var last = queue.length && queue[queue.length - 1];
      if (last && last.uid === uid && last.key === key) queue.pop(); // A toggle back cancels the pending one.
      else queue.push({ uid: uid, key: key, value: next });
      return next;
    }
    function Layer(uid) { this.layerUid = uid; return guard(this, 'Layer.'); }
    Object.defineProperty(Layer.prototype, 'index', { get: function () { return find(this.layerUid).index; } });
    Object.defineProperty(Layer.prototype, 'name', {
      get: function () { need(S + 'layers'); var l = find(this.layerUid).layer; return l.name === undefined ? 'Layer ' + (find(this.layerUid).index + 1) : l.name; },
      set: function () {
        var e = new Error('nemo: Layer.name cannot be written on a native document (not-yet-bound)');
        e.code = 'unavailable'; e.row = S + 'Layer.name'; e.reason = 'not-yet-bound'; throw e;
      },
    });
    ['visible', 'locked', 'shy'].forEach(function (key) {
      Object.defineProperty(Layer.prototype, key, {
        get: function () { need(S + 'Layer.' + key); return flag(this.layerUid, key); },
        set: function (v) { need(S + 'Layer.' + key); flag(this.layerUid, key, v); },
      });
    });
    ROUTES.forEach(function (r) {
      var m = /^sdk:nemo-script\.Layer\.(\w+)$/.exec(r.row);
      if (m && !Object.prototype.hasOwnProperty.call(Layer.prototype, m[1]))
        Object.defineProperty(Layer.prototype, m[1], { get: function () { throw unavailable(r.row); }, set: function () { throw unavailable(r.row); } });
    });
    function handle(which) {
      if (typeof which === 'number' && Number.isInteger(which) && which >= 0 && which < layers.length) return new Layer(layers[which].layerUid);
      if (typeof which === 'string') for (var i = 0; i < layers.length; i++) if (layers[i].name === which || layers[i].layerUid === which) return new Layer(layers[i].layerUid);
      throw fail('no layer ' + JSON.stringify(which));
    }
    function stub(row) { return function () { throw unavailable(row); }; }
    function group(prefix, names) { var o = {}; names.forEach(function (n) { o[n] = stub(prefix + n); }); return o; }
    var nemo = {
      get fps() { return start.fps; }, get width() { return start.canvasW; }, get height() { return start.canvasH; },
      get frameCount() { return start.totalFrames; }, get layerCount() { return layers.length; },
      layer: function (which) { need(S + 'layer'); return handle(which); },
      layers: function () { need(S + 'layers'); return layers.map(function (l) { return new Layer(l.layerUid); }); },
      activeLayer: function () {
        need(S + 'activeLayer');
        var i = typeof ports.activeIndex === 'function' ? ports.activeIndex() : ports.selection()[0];
        return handle(Number.isInteger(i) && i >= 0 && i < layers.length ? i : 0);
      },
      get frame() { return typeof ports.frame === 'function' ? ports.frame() : 0; },
      set frame(f) { need(S + 'goToFrame'); ports.goToFrame(f | 0); },
      goToFrame: function (f) { need(S + 'goToFrame'); ports.goToFrame(f | 0); return nemo; },
      toast: function (m) { need(S + 'toast'); ports.toast(String(m)); log.push('toast'); return nemo; },
      log: function () { need(S + 'log'); return log.slice(); },
      help: function () { need(S + 'help'); return ROUTES.filter(function (r) { return r.bound && /^sdk:nemo-script\./.test(r.row); }).map(function (r) { return r.row.replace(/^sdk:nemo-script\./, ''); }); },
      selection: guard(Object.assign(group(S + 'selection.', ['clear', 'setLayers', 'strokes']),
        { layers: function () { need(S + 'selection.layers'); return ports.selection().map(function (i) { return handle(i); }); } }), 'selection.'),
      comp: guard(Object.assign(group(S + 'comp.', ['beat', 'beats', 'setWorkArea', 'trimToWorkArea']), {
        get fps() { return start.fps; }, get frameCount() { return start.totalFrames; } }), 'comp.'),
      markers: guard(group(S + 'markers.', ['add', 'list', 'next', 'prev', 'removeAt']), 'markers.'),
      camera: guard(group(S + 'camera.', ['keys']), 'camera.'),
    };
    ['addLayer', 'addPivot', 'isKeyframe', 'undoGroup'].forEach(function (n) { nemo[n] = stub(S + n); });
    nemo = guard(nemo, '');
    // Drains queued writes in order. Stops at the first refusal; refuses everything when
    // the document was replaced or closed after the run read its pin.
    function commit() {
      var applied = 0, items = queue.slice(); queue = [];
      function step(i) {
        if (i >= items.length) return Promise.resolve({ ok: true, applied: applied, pending: 0 });
        var now = ports.info();
        if (!now || PIN.some(function (k) { return now[k] !== pin[k]; }))
          return Promise.resolve({ ok: false, applied: applied, pending: items.length - i, error: 'document-replaced' });
        var index = -1;
        for (var j = 0; j < now.layers.length; j++) if (now.layers[j].layerUid === items[i].uid) { index = j; break; }
        if (index < 0) return Promise.resolve({ ok: false, applied: applied, pending: items.length - i, error: 'layer-gone' });
        var current = now.layers[index], has = items[i].key === 'visible' ? current.visible !== false : current[items[i].key] === true;
        if (has === items[i].value) return step(i + 1);
        var refused = function () { return { ok: false, applied: applied, pending: items.length - i, error: 'write-refused' }; };
        return Promise.resolve().then(function () { return ports.write[items[i].key](index); }).then(function (ok) {
          if (ok === false) return refused();
          // The entry's return value is not proof: the native read model must show the new value.
          var after = ports.info(), row = after && after.layers.filter(function (l) { return l.layerUid === items[i].uid; })[0];
          var value = row && (items[i].key === 'visible' ? row.visible !== false : row[items[i].key] === true);
          if (value !== items[i].value) return { ok: false, applied: applied, pending: items.length - i, error: 'write-unconfirmed' };
          applied += 1; return step(i + 1);
        }, refused);
      }
      return step(0);
    }
    return { nemo: nemo, commit: commit, discard: function () { queue = []; }, log: function () { return log.slice(); } };
  }
  var REFUSED = 'Scripts run on an open native document only.';
  // Shadowing is hygiene for ordinary scripts, not isolation (see the header).
  var SHADOWED = ['state', 'SM', 'SMScript', 'SMMotion', 'NemoNativeObjectProject', 'window', 'globalThis', 'self'];
  function compile() { return Function.apply(null, ['nemo', '__src'].concat(SHADOWED, ['"use strict";\nreturn eval(__src);'])); }
  function compileBody(source) { return Function.apply(null, ['nemo'].concat(SHADOWED, ['"use strict";\n' + source])); }
  // The desktop dispatch predicate shared with nemo-script.js: a Tauri host, or an open native document.
  function owns(root) {
    var p = root.NemoNativeObjectProject, s = p && typeof p.desktop === 'function' && p.desktop(root);
    return !!(root.__TAURI__ && root.__TAURI__.core) || !!(s && s.blocksLegacy() && s.isActive());
  }
  // The desktop SMScript.run: one run over the active native object session, or a refusal.
  function runOnDesktop(root, source) {
    var project = root.NemoNativeObjectProject, session = project && typeof project.desktop === 'function' && project.desktop(root);
    if (!session || !session.blocksLegacy() || !session.isActive()) return { ok: false, error: REFUSED, log: [] };
    var sm = root.SM || {}, entry = function (name) { return function (index) { return typeof sm[name] === 'function' ? sm[name](index) : false; }; };
    var toast = function (m) { if (typeof root.showToast === 'function') root.showToast(m); }, run;
    try {
      run = createRun({ info: function () { return session.isActive() ? session.info() : null; },
        write: { visible: entry('toggleLayerVis'), locked: entry('toggleLayerLock'), shy: entry('toggleLayerShy') },
        goToFrame: function (f) { if (typeof root.goToFrame === 'function') root.goToFrame(f); },
        frame: function () { return root.state && Number.isInteger(root.state.currentFrame) ? root.state.currentFrame : 0; },
        activeIndex: function () { return root.state ? root.state.activeLayerIdx : undefined; },
        selection: function () { return (root._layerSel || []).slice(); }, toast: toast });
    } catch (e) { return { ok: false, error: e.message, log: [] }; }
    var value;
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
    // A failing script sends nothing; a successful one commits its queued writes in order.
    var committed = run.commit().then(function (c) { if (!c.ok) toast('Script writes stopped: ' + c.error); return c; });
    return { ok: true, value: value, log: run.log(), committed: committed };
  }
  return Object.freeze({ ROUTES: ROUTES, createRun: createRun, runOnDesktop: runOnDesktop, owns: owns });
}));
