/* Native plugin host bridge (CS8-SDK #2354, I34).
 * Owner: Ilya/Claude clean-slate session. Fixture: tests/script-native-plugin.test.cjs.
 * A plugin panel (nemo-plugin.js) used to receive the legacy builder (`parent.SMScript.api()`), which writes state.layers. On a native host
 * it receives this facade instead: the same `nemo` vocabulary and the same native entries a script run uses, never a second implementation.
 * A plugin is event driven, so the unit is one TICK, not one script: the first member a handler touches opens a run over the active native
 * session (script-native-api.js createRun, ports from script-native-run.js portsOf), every access in that synchronous stretch shares it, and
 * a microtask later the queued writes drain in order through the owner's entries with the same pin, read-back and undo-group rules a script
 * run has (one tick = one undo group). A layer handle is a forwarding proxy keyed by layerUid, so it keeps working across ticks and across
 * reorders; a handle on a layer the plugin created resolves through the id the owner minted. Nothing here reads or writes state.layers.
 *  - a closed plugin refuses every member, and a tick still open when the plugin closes is discarded: nothing is sent;
 *  - a document that is replaced or closed between the tick and its drain gets no write (the drain's pin check);
 *  - reads that need a prefetched round (Layer.get/keys, isKeyframe, markers, beats) throw their typed refusal (not-prefetched): a tick cannot
 *    wait for the owner. Reads of layer rows, flags, names and the composition values still work.
 * The plugin runs in a same-origin iframe with the page's own access: this is NOT a sandbox, and no sandbox acceptance is claimed (the
 * baseline had the same shape; recorded under I30/I34).
 */
(function (root, factory) {
  var cjs = typeof module === 'object' && module.exports;
  var api = factory(cjs ? require('./script-native-run.js') : root.NemoScriptNativeRun, cjs ? require('./script-native-api.js') : root.NemoScriptNativeApi);
  if (cjs) module.exports = api; else root.NemoScriptNativePlugin = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Run, Api) {
  'use strict';
  var GROUPS = ['comp', 'markers', 'selection', 'camera'], INERT = { then: 1, toJSON: 1, constructor: 1, valueOf: 1, toString: 1 };
  function refused(message) { var e = new Error('nemo: ' + message); e.code = 'refused'; return e; }
  function create(root, host) {
    var ticks = Object.create(null); // plugin id -> { run, ports } of the tick in flight
    function isOpen(id) { return !host || typeof host.isOpen !== 'function' || host.isOpen(id) === true; }

    function facade(id, control) {
      var proxies = new WeakMap(), base = Object.assign({}, control || {});
      function tick() {
        if (!isOpen(id)) throw refused('plugin ' + JSON.stringify(id) + ' is closed');
        if (ticks[id]) return ticks[id];
        var ports = Run.portsOf(root);
        if (!ports) throw refused('plugins run on an open native document only');
        var t = ticks[id] = { run: Api.createRun(ports, {}), ports: ports };
        Promise.resolve().then(function () { flush(t); });
        return t;
      }
      function flush(t) {
        if (ticks[id] === t) delete ticks[id];
        if (!isOpen(id)) { t.run.discard(); return Promise.resolve(); }
        return t.run.commit().then(function (c) { if (!c.ok && typeof t.ports.toast === 'function') t.ports.toast('Plugin writes stopped: ' + c.error); },
          function () { if (typeof t.ports.toast === 'function') t.ports.toast('Plugin writes could not be confirmed.'); });
      }
      // What a value the run returns becomes for the plugin: a layer handle that outlives the tick, a group that re-binds each tick, a list of them.
      function wrap(value, t) {
        if (Array.isArray(value)) return value.map(function (v) { return wrap(v, t); });
        if (!value || typeof value !== 'object') return value;
        if (value === t.run.nemo) return top;
        for (var i = 0; i < GROUPS.length; i++) if (value === t.run.nemo[GROUPS[i]]) return group([GROUPS[i]]);
        return typeof value.layerUid === 'string' ? handle(value.layerUid, t.run) : value;
      }
      function unwrap(args) { return Array.prototype.map.call(args, function (a) { var u = a && typeof a === 'object' && proxies.get(a); return u ? resolveUid(u) : a; }); }
      function resolveUid(u) { return u.run.resolve(u.uid); }
      // By layerUid only: nemo.layer(string) would also match a name, and a layer may be named like another one's id.
      function layerOf(t, u) {
        var uid = resolveUid(u), found = t.run.nemo.layers().filter(function (l) { return l.layerUid === uid; })[0];
        if (!found) throw refused('layer ' + JSON.stringify(uid) + ' is not in this document');
        return found;
      }
      function handle(uid, run) {
        var u = { uid: uid, run: run };
        var p = new Proxy({}, {
          get: function (_, key) {
            if (typeof key === 'symbol' || INERT[key]) return undefined;
            if (key === 'layerUid') return resolveUid(u);
            var t = tick(), layer = layerOf(t, u), v = layer[key];
            return typeof v === 'function' ? function () { return wrap(v.apply(layer, unwrap(arguments)), t); } : wrap(v, t);
          },
          set: function (_, key, value) { layerOf(tick(), u)[key] = value; return true; },
        });
        proxies.set(p, u);
        return p;
      }
      function walk(path, t) { return path.reduce(function (o, k) { return o[k]; }, t.run.nemo); }
      var kept = Object.create(null);
      function group(path) { var k = path.join('.'); return kept[k] || (kept[k] = forward(path)); }
      function forward(path) {
        return new Proxy({}, {
          get: function (_, key) {
            if (typeof key === 'symbol' || INERT[key]) return undefined;
            var t = tick(), target = walk(path, t), v = target[key];
            return typeof v === 'function' ? function () { return wrap(v.apply(target, unwrap(arguments)), t); } : wrap(v, t);
          },
          set: function (_, key, value) { walk(path, tick())[key] = value; return true; },
        });
      }
      var top = new Proxy(base, {
        get: function (target, key, receiver) {
          if (typeof key === 'symbol' || INERT[key]) return undefined;
          if (Object.prototype.hasOwnProperty.call(target, key)) return Reflect.get(target, key, receiver);
          var t = tick(), v = t.run.nemo[key];
          return typeof v === 'function' ? function () { return wrap(v.apply(t.run.nemo, unwrap(arguments)), t); } : wrap(v, t);
        },
        set: function (target, key, value) {
          if (Object.prototype.hasOwnProperty.call(target, key)) { target[key] = value; return true; }
          tick().run.nemo[key] = value; return true;
        },
      });
      return top;
    }
    return Object.freeze({ api: facade, isOpen: isOpen, pending: function (id) { return !!ticks[id]; } });
  }
  return Object.freeze({ create: create });
}));
