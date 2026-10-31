/* Native script write drain (CS8-SDK #2354, second increment).
 * Owner: Ilya/Claude clean-slate session. Fixture: tests/script-native-api.test.cjs.
 * Drains the queue of one script run, in order, after the script succeeded. run(items, { ports, pin, ops, keys }) resolves
 * { ok, applied, pending[, error] }. Each step: re-read the pin (a replaced or closed document stops the drain), resolve its layer by
 * id (through the ids the drain has learned for layers the run created), call ONE native entry, then count the step only when the
 * native read model shows its effect. An entry's own answer is never proof. The first refusal stops the drain; nothing is retried.
 * Errors: document-replaced, layer-gone, write-refused, write-unconfirmed.
 */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoScriptNativeDrain = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  function run(items, ctx) {
    var ports = ctx.ports, pin = ctx.pin, ops = ctx.ops, PIN = ctx.keys, applied = 0, alias = Object.create(null), SKIP = {};
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
  return Object.freeze({ run: run });
}));
