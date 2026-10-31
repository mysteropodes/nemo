/* Native script API (CS8-SDK #2354, first increment).
 * Owner: Ilya/Claude clean-slate session. Fixture: tests/script-native-api.test.cjs.
 * The public `nemo` vocabulary over a native document: every baseline sdk: row has one
 * route (ROUTES). Reads are synchronous over the run's pinned read model (detached
 * values, layers by layerUid); document writes are queued during the run and drained
 * afterwards, in order, through the ordinary UI's native entry points, so a script
 * reaches the same handler as the UI and never writes state.layers. A row that is not
 * bound here throws {code:'unavailable', row, reason} before any write is sent.
 * Structural and property writes (addLayer, Layer.remove/duplicate/splitAt/name/set/key/hold/clearKeys, markers.add/removeAt)
 * join the same queue. A handle on a layer the run has not created yet carries a provisional id; the drain learns the id the
 * owner minted from the read model and resolves later steps through it. Every step re-checks the pin, calls one native entry
 * (script-native-writes.js) and counts only what the read model then shows; the first refusal stops the drain.
 * Injected ports: info(), write.{visible,locked,shy}(index) -> Promise<boolean>, ops.{add,remove,duplicate,split,rename,
 * property,track,marker} -> Promise<{ok,applied,layerUid}> (a missing one makes its rows unavailable at the call),
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
  put(S, ['Layer.name'], op(Q, O + 'layer.rename', true));
  put(S, ['isKeyframe'], op(Q + '.layer.timing'));
  put(S, ['addLayer'], op(null, O + 'layer.create', true));
  put(S, ['Layer.remove'], op(null, O + 'layer.delete', true));
  put(S, ['Layer.duplicate'], op(null, O + 'layer.duplicate', true));
  put(S, ['Layer.splitAt'], op(null, O + 'layer.cut', true));
  put(S, ['Layer.strokes'], op(Q));
  put(S, ['Layer.get', 'Layer.keys'], op(Q + '.property.read'));
  // key sets the key (property.edit setKey), then shapes its curve through the track route when an ease is asked for.
  put(S, ['Layer.set', 'Layer.key', 'Layer.clearKeys'], op(null, O + 'property.edit', true));
  put(S, ['Layer.hold'], op(null, O + 'property.track.edit', true));
  put(S, ['comp.beat', 'comp.beats', 'markers.list', 'markers.next', 'markers.prev'], op(Q + '.project.composition.read'));
  put(S, ['markers.add', 'markers.removeAt'], op(null, O + 'project.composition.edit', true));
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

  // A bound write row still needs its native entry (ports.ops[NEEDS]) to be present when the script calls it.
  var NEEDS = { addLayer: 'add', 'Layer.remove': 'remove', 'Layer.duplicate': 'duplicate', 'Layer.splitAt': 'split', 'Layer.name': 'rename',
    'Layer.set': 'property', 'Layer.key': 'property', 'Layer.clearKeys': 'property', 'Layer.hold': 'track', 'markers.add': 'marker', 'markers.removeAt': 'marker' };
  var PROPS = { position: 2, anchor: 2, scale: 2, rotation: 1, opacity: 1 };
  // The baseline ease names over the native curve presets; 'strong' has no native preset and is refused rather than approximated.
  var EASES = { linear: 'linear', smooth: 'ease', strong: null, easeIn: 'easeIn', easeOut: 'easeOut' };
  function unavailable(row, why) {
    var route = byRow[row], reason = why || (route && route.kind === 'unavailable' ? route.reason : 'not-yet-bound');
    var error = new Error('nemo: ' + row.replace(/^sdk:nemo-(script|plugin)\./, '') + ' is unavailable on a native document (' +
      (why || (route ? route.bound ? 'bound' : route.kind === 'unavailable' ? route.reason : 'not-yet-bound' : 'undeclared')) + ')');
    error.code = 'unavailable'; error.row = row; error.reason = reason;
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

  // One run: a pinned read model, a local model of the queued writes, and the queue. A structural write (add, duplicate,
  // split) hands the script a handle on a provisional layerUid; the drain maps it to the id the owner minted.
  function createRun(ports) {
    var start = ports.info();
    if (!start || !Array.isArray(start.layers)) throw fail('no native document is open');
    var pin = {}; PIN.forEach(function (k) { pin[k] = start[k]; });
    var layers = detached(start.layers), overlay = Object.create(null), queue = [], log = [], ops = ports.ops || {};
    var seen = Object.create(null), made = 0;
    layers.forEach(function (l) { if (seen[l.layerUid]) throw fail('duplicate layer id ' + l.layerUid); seen[l.layerUid] = true; });
    function find(uid) {
      for (var i = 0; i < layers.length; i++) if (layers[i].layerUid === uid) return { layer: layers[i], index: i };
      throw fail('layer ' + uid + ' is not in this document');
    }
    function provisional() { do { made += 1; } while (seen['pending-' + made]); seen['pending-' + made] = true; return 'pending-' + made; }
    // The native entry a write row needs, or the typed refusal - before anything is queued.
    function entry(row, name) {
      need(row);
      if (typeof ops[name] !== 'function') throw unavailable(row, 'native-entry-absent');
      return ops[name];
    }
    function here() { return typeof ports.frame === 'function' ? ports.frame() : 0; }
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
      if (last && last.kind === 'flag' && last.uid === uid && last.key === key) queue.pop(); // A toggle back cancels the pending one.
      else queue.push({ kind: 'flag', uid: uid, key: key, value: next });
      return next;
    }
    function prop(row, p) {
      if (p === 'timeRemap') throw unavailable(row, 'no-native-time-remap');
      if (!Object.prototype.hasOwnProperty.call(PROPS, p)) throw fail('unknown property ' + JSON.stringify(p) + ' (expected: ' + Object.keys(PROPS).join(', ') + ')');
      return p;
    }
    function vector(p, value) {
      var v = Array.isArray(value) ? value.slice() : [value];
      if (v.length !== PROPS[p] || !v.every(function (n) { return typeof n === 'number' && isFinite(n); })) throw fail(p + ' takes ' + PROPS[p] + ' finite number(s)');
      return v;
    }
    function inside(frame) {
      var f = frame | 0;
      if (f < 0 || f >= start.totalFrames) throw fail('frame ' + frame + ' is outside the composition');
      return f;
    }
    function insertAfter(uid, name, extra) {
      var at = find(uid), copy = detached(at.layer), id = provisional();
      copy.layerUid = id; copy.name = name; copy.pending = true;
      ['visible', 'locked', 'shy'].forEach(function (k) { copy[k] = flag(uid, k); });
      layers.splice(at.index + 1, 0, Object.assign(copy, extra));
      return id;
    }
    function Layer(uid) { this.layerUid = uid; return guard(this, 'Layer.'); }
    Object.defineProperty(Layer.prototype, 'index', { get: function () { return find(this.layerUid).index; } });
    Object.defineProperty(Layer.prototype, 'name', {
      get: function () { need(S + 'layers'); var l = find(this.layerUid).layer; return l.name === undefined ? 'Layer ' + (find(this.layerUid).index + 1) : l.name; },
      set: function (v) {
        entry(S + 'Layer.name', 'rename');
        var uid = this.layerUid, l = find(uid).layer, next = String(v);
        if (l.name === next) return;
        l.name = next;
        var last = queue.length && queue[queue.length - 1];
        if (last && last.kind === 'rename' && last.uid === uid) last.name = next; else queue.push({ kind: 'rename', uid: uid, name: next });
      },
    });
    ['visible', 'locked', 'shy'].forEach(function (key) {
      Object.defineProperty(Layer.prototype, key, {
        get: function () { need(S + 'Layer.' + key); return flag(this.layerUid, key); },
        set: function (v) { need(S + 'Layer.' + key); flag(this.layerUid, key, v); },
      });
    });
    Layer.prototype.remove = function () {
      entry(S + 'Layer.remove', 'remove');
      var uid = this.layerUid, at = find(uid);
      if (layers.length <= 1) throw fail('a document keeps at least one layer');
      layers.splice(at.index, 1); delete overlay[uid]; queue.push({ kind: 'remove', uid: uid });
    };
    Layer.prototype.duplicate = function () {
      entry(S + 'Layer.duplicate', 'duplicate');
      var uid = this.layerUid, id = insertAfter(uid, this.name + ' copy');
      queue.push({ kind: 'duplicate', uid: uid, made: id }); return new Layer(id);
    };
    // Returns the second half, as the baseline did; a missing frame cuts at the playhead.
    Layer.prototype.splitAt = function (frame) {
      entry(S + 'Layer.splitAt', 'split');
      var uid = this.layerUid, f = frame === undefined || frame === null ? null : inside(frame), id = insertAfter(uid, this.name + ' (2)');
      queue.push({ kind: 'split', uid: uid, made: id, frame: f }); return new Layer(id);
    };
    Layer.prototype.set = function (p, value) {
      entry(S + 'Layer.set', 'property'); prop(S + 'Layer.set', p);
      queue.push({ kind: 'prop', uid: find(this.layerUid).layer.layerUid, property: p, frame: here(), action: { kind: 'setValue', values: vector(p, value) } });
      return this;
    };
    Layer.prototype.key = function (p, frame, value, ease) {
      entry(S + 'Layer.key', 'property'); prop(S + 'Layer.key', p);
      var name = ease === undefined || ease === null ? 'smooth' : ease;
      if (!Object.prototype.hasOwnProperty.call(EASES, name)) throw fail('unknown ease ' + JSON.stringify(ease) + ' (expected: ' + Object.keys(EASES).join(', ') + ')');
      if (EASES[name] === null) throw unavailable(S + 'Layer.key', 'no-native-ease-' + name);
      entry(S + 'Layer.key', 'track');
      var uid = find(this.layerUid).layer.layerUid, f = frame | 0;
      queue.push({ kind: 'prop', uid: uid, property: p, frame: f, action: { kind: 'setKey', values: vector(p, value) } });
      queue.push({ kind: 'track', uid: uid, property: p, frame: f, preset: EASES[name] });
      return this;
    };
    Layer.prototype.hold = function (p, frame) {
      entry(S + 'Layer.hold', 'track'); prop(S + 'Layer.hold', p);
      queue.push({ kind: 'track', uid: find(this.layerUid).layer.layerUid, property: p, frame: frame | 0, preset: 'hold' });
      return this;
    };
    // The owner keeps the value the property had at the playhead as its static value (the baseline restored the older static one).
    Layer.prototype.clearKeys = function (p) {
      entry(S + 'Layer.clearKeys', 'property'); prop(S + 'Layer.clearKeys', p);
      queue.push({ kind: 'prop', uid: find(this.layerUid).layer.layerUid, property: p, frame: here(), action: { kind: 'setAnimated', animated: false } });
      return this;
    };
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
    function addLayer(name) {
      entry(S + 'addLayer', 'add');
      var named = name !== undefined && name !== null && name !== '';
      if (named) entry(S + 'Layer.name', 'rename');
      var used = {}, n = 1; layers.forEach(function (l) { used[l.name] = true; });
      while (used['Layer ' + n]) n += 1;
      var id = provisional(); layers.push({ layerUid: id, name: 'Layer ' + n, visible: true, pending: true });
      queue.push({ kind: 'add', uid: id });
      var created = new Layer(id); if (named) created.name = String(name);
      return created;
    }
    function marker(row, action, frame, name) {
      entry(row, 'marker');
      var f = frame === undefined || frame === null ? here() : frame | 0;
      if (f < 0) throw fail('marker frame ' + frame + ' is before the composition');
      queue.push({ kind: 'marker', action: action, frame: f, name: name ? String(name) : '' });
      return nemo.markers;
    }
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
      get frame() { return here(); },
      set frame(f) { need(S + 'goToFrame'); ports.goToFrame(f | 0); },
      goToFrame: function (f) { need(S + 'goToFrame'); ports.goToFrame(f | 0); return nemo; },
      toast: function (m) { need(S + 'toast'); ports.toast(String(m)); log.push('toast'); return nemo; },
      log: function () { need(S + 'log'); return log.slice(); },
      help: function () {
        need(S + 'help');
        // Layer.name reads without an entry; every other write row is listed only while its native entry is present.
        return ROUTES.filter(function (r) { var n = r.row.slice(S.length); return r.bound && /^sdk:nemo-script\./.test(r.row) && (n === 'Layer.name' || !NEEDS[n] || typeof ops[NEEDS[n]] === 'function'); })
          .map(function (r) { return r.row.replace(/^sdk:nemo-script\./, ''); });
      },
      addLayer: addLayer,
      selection: guard(Object.assign(group(S + 'selection.', ['clear', 'setLayers', 'strokes']),
        { layers: function () { need(S + 'selection.layers'); return ports.selection().map(function (i) { return handle(i); }); } }), 'selection.'),
      comp: guard(Object.assign(group(S + 'comp.', ['beat', 'beats', 'setWorkArea', 'trimToWorkArea']), {
        get fps() { return start.fps; }, get frameCount() { return start.totalFrames; } }), 'comp.'),
      markers: guard(Object.assign(group(S + 'markers.', ['list', 'next', 'prev']), {
        add: function (frame, name) { return marker(S + 'markers.add', 'add', frame, name); },
        removeAt: function (frame) { return marker(S + 'markers.removeAt', 'remove', frame); } }), 'markers.'),
      camera: guard(group(S + 'camera.', ['keys']), 'camera.'),
    };
    ['addPivot', 'isKeyframe', 'undoGroup'].forEach(function (n) { nemo[n] = stub(S + n); });
    nemo = guard(nemo, '');
    // Drains the queue in order. Each step re-reads the pin, resolves its layer by id (through the ids the drain has learned),
    // calls the same native entry the UI uses, and counts only what the native read model then shows. It stops at the first
    // refusal, and refuses everything when the document was replaced or closed after the run read its pin.
    function commit() {
      var applied = 0, items = queue.slice(), alias = Object.create(null), SKIP = {}; queue = [];
      function real(uid) { return alias[uid] || uid; }
      function samePin(info) { return !!info && PIN.every(function (k) { return info[k] === pin[k]; }); }
      function locate(info, uid) {
        for (var j = 0; j < info.layers.length; j++) if (info.layers[j].layerUid === real(uid)) return { row: info.layers[j], index: j };
        return null;
      }
      function flagOf(row, key) { return key === 'visible' ? row.visible !== false : row[key] === true; }
      // The one new layer a structural write added, or null when the model does not show exactly one.
      function added(before, after) {
        if (!samePin(after) || after.layers.length !== before.layers.length + 1) return null;
        var known = {}; before.layers.forEach(function (l) { known[l.layerUid] = true; });
        var fresh = after.layers.filter(function (l) { return !known[l.layerUid]; });
        return fresh.length === 1 ? fresh[0].layerUid : null;
      }
      // Calls the entry; a refusal or rejection is 'write-refused', and the entry's own answer is never proof: verify(after, answer) decides.
      function perform(call, verify) {
        return Promise.resolve().then(call).then(function (r) {
          if (r === false || (r && r.ok === false)) return 'write-refused';
          var after = ports.info();
          return after ? verify(after, r || {}) : 'write-unconfirmed';
        }, function () { return 'write-refused'; });
      }
      function minted(it, now, r, after, index) {
        var uid = added(now, after);
        if (!uid || (r.layerUid && r.layerUid !== uid) || (index !== undefined && after.layers[index].layerUid !== uid)) return 'write-unconfirmed';
        alias[it.made || it.uid] = uid;
      }
      // An edit that leaves no trace in the layer rows is confirmed by the revision the owner then reports.
      function revised(now) {
        return function (after, r) {
          if (!samePin(after)) return 'write-unconfirmed';
          if (r.applied !== true) return after.contentRevision === now.contentRevision ? SKIP : 'write-unconfirmed';
          return after.contentRevision > now.contentRevision ? undefined : 'write-unconfirmed';
        };
      }
      var handlers = {
        flag: function (it, now) {
          var at = locate(now, it.uid);
          if (!at) return 'layer-gone';
          if (flagOf(at.row, it.key) === it.value) return SKIP;
          return perform(function () { return ports.write[it.key](at.index); }, function (after) {
            var r = locate(after, it.uid);
            return samePin(after) && r && flagOf(r.row, it.key) === it.value ? undefined : 'write-unconfirmed';
          });
        },
        add: function (it, now) { return perform(function () { return ops.add(); }, function (after, r) { return minted({ uid: it.uid }, now, r, after); }); },
        remove: function (it, now) {
          var at = locate(now, it.uid);
          if (!at) return 'layer-gone';
          return perform(function () { return ops.remove(at.row.layerUid); }, function (after) {
            return samePin(after) && after.layers.length === now.layers.length - 1 && !locate(after, it.uid) ? undefined : 'write-unconfirmed';
          });
        },
        duplicate: function (it, now) {
          var at = locate(now, it.uid);
          if (!at) return 'layer-gone';
          return perform(function () { return ops.duplicate(at.row.layerUid); }, function (after, r) { return minted(it, now, r, after); });
        },
        split: function (it, now) {
          var at = locate(now, it.uid);
          if (!at) return 'layer-gone';
          return perform(function () { return ops.split(at.index, it.frame); }, function (after, r) { return minted(it, now, r, after, at.index + 1); });
        },
        rename: function (it, now) {
          var at = locate(now, it.uid);
          if (!at) return 'layer-gone';
          if (at.row.name === it.name) return SKIP;
          return perform(function () { return ops.rename(at.row.layerUid, it.name); }, function (after) {
            var r = locate(after, it.uid);
            return samePin(after) && r && r.row.name === it.name ? undefined : 'write-unconfirmed';
          });
        },
        prop: function (it, now) {
          var at = locate(now, it.uid);
          if (!at) return 'layer-gone';
          return perform(function () { return ops.property({ layerUid: at.row.layerUid, property: it.property, frame: it.frame, action: it.action }); }, revised(now));
        },
        track: function (it, now) {
          var at = locate(now, it.uid);
          if (!at) return 'layer-gone';
          return perform(function () { return ops.track({ layerUid: at.row.layerUid, property: it.property, frame: it.frame, preset: it.preset }); }, revised(now));
        },
        marker: function (it, now) {
          return perform(function () { return ops.marker({ action: it.action, frame: it.frame, name: it.name }); }, revised(now));
        },
      };
      function stop(i, error) { return { ok: false, applied: applied, pending: items.length - i, error: error }; }
      function step(i) {
        if (i >= items.length) return Promise.resolve({ ok: true, applied: applied, pending: 0 });
        var now = ports.info();
        if (!now || PIN.some(function (k) { return now[k] !== pin[k]; })) return Promise.resolve(stop(i, 'document-replaced'));
        return Promise.resolve(handlers[items[i].kind](items[i], now)).then(function (result) {
          if (result === SKIP) return step(i + 1);
          if (typeof result === 'string') return stop(i, result);
          applied += 1; return step(i + 1);
        });
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
        ops: root.NemoScriptNativeWrites ? root.NemoScriptNativeWrites.create(root, session) : {},
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
