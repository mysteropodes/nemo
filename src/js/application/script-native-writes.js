/* Native script writes (CS8-SDK #2354, second increment).
 * Owner: Ilya/Claude clean-slate session. Fixture: tests/script-native-writes.test.cjs.
 * The desktop side of the script write queue: one function per queued write kind, each reaching the SAME native entry
 * the ordinary UI uses and nothing else. create(root, session) returns only the entries that are present right now,
 * so a script that needs an absent one is refused at the call, before any write is sent.
 *   add        NemoNativeDrawingAddController.add()           Drawing Add (the timeline "+" button)
 *   split      SM.splitLayerAtPlayhead(index)                 the playhead cut, after the playhead reaches the frame
 *   remove     layer lifecycle intent .deleteLayers           command.document.object.layer.delete
 *   duplicate  layer lifecycle intent .duplicateLayer         command.document.object.layer.duplicate
 *   rename     layer lifecycle intent .renameLayer            command.document.object.layer.rename
 *   property   session.editProperty                           command.document.object.property.edit
 *   track      session.editPropertyTrack                      command.document.object.property.track.edit
 *   marker     NemoNativeCompositionController.edit           command.document.object.project.composition.edit
 * The lifecycle intents submit through the session's resource command queue (dispatch, then reconcile), the route the
 * playhead cut and the composition controller already use. Nothing here reads or writes state.layers.
 * Every entry resolves { ok, applied?, layerUid? }; a refusal, a stale target or a rejected transport is ok:false.
 */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoScriptNativeWrites = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var CONTEXT = 'scene-root', TRIES = 16, WAIT_MS = 60, PIN = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  function same(a, b) { return !!a && !!b && PIN.every(function (k) { return a[k] === b[k]; }); }
  function sleep(root, ms) { return new Promise(function (done) { ((root && root.setTimeout) || setTimeout)(done, ms); }); }
  function refused(extra) { return Object.assign({ ok: false }, extra); }
  function guarded(fn) { return Promise.resolve().then(fn).catch(function (e) { return refused({ message: String(e && e.message || e) }); }); }
  function outcome(r) { return r && r.ok === true ? { ok: true, applied: r.applied === true, layerUid: r.layerUid } : refused({ code: r && r.code }); }

  // The layer lifecycle intents over the session's pinned resource command route.
  function lifecycle(root, session) {
    var Intent = root.NemoNativeObjectLayerLifecycleIntent, Edit = root.NemoNativeObjectEditContract;
    if (!Intent || !Edit || typeof session.runResourceCommand !== 'function' || typeof session.requestId !== 'function') return null;
    return Intent.createLayerLifecycleIntent({
      pin: function () { return session.isActive() ? session.info() : null; },
      declared: function (operation) { return Edit.admitted(operation) && session.isActive() === true; },
      requestId: function (label) { return session.requestId(label); },
      submit: function (request) {
        return session.runResourceCommand(async function (ctx) {
          var response = await ctx.dispatch(request);
          if (response && response.ok === true) await ctx.reconcile(response.contentRevision);
          return response;
        });
      },
    });
  }
  function target(layerUid) { return { contextId: CONTEXT, layerUid: layerUid }; }

  function create(root, session) {
    var ops = {}, add = root.NemoNativeDrawingAddController, sm = root.SM, comp = root.NemoNativeCompositionController;
    var intent = session ? lifecycle(root, session) : null;
    if (add && typeof add.add === 'function') {
      ops.add = function () {
        return guarded(function () {
          return add.add().then(function (r) { return r && /^created/.test(String(r.status)) && typeof r.layerUid === 'string' ? { ok: true, applied: true, layerUid: r.layerUid } : refused({ code: r && r.status }); });
        });
      };
    }
    if (intent) {
      ops.remove = function (layerUid) { return guarded(function () { return intent.deleteLayers({ selection: { active: layerUid, selected: [layerUid] } }).then(outcome); }); };
      ops.duplicate = function (layerUid) { return guarded(function () { return intent.duplicateLayer({ layerUid: layerUid }).then(outcome); }); };
      ops.rename = function (layerUid, name) { return guarded(function () { return intent.renameLayer({ layerUid: layerUid, name: name }).then(outcome); }); };
    }
    if (sm && typeof sm.splitLayerAtPlayhead === 'function' && typeof root.goToFrame === 'function' && root.state) {
      // The cut reads the playhead, so the playhead must be at the frame AND presented there before the cut is asked.
      ops.split = function (index, frame) {
        return guarded(async function () {
          if (frame !== null && frame !== undefined && root.state.currentFrame !== frame) {
            root.goToFrame(frame);
            for (var n = 0; n < TRIES; n++) {
              var shown = session && typeof session.readPresentedObjectSession === 'function' && session.readPresentedObjectSession();
              if (root.state.currentFrame === frame && shown && shown.presentation && shown.presentation.frame === frame) break;
              if (n === TRIES - 1) return refused({ code: 'playhead-not-presented' });
              await sleep(root, WAIT_MS);
            }
          }
          return (await sm.splitLayerAtPlayhead(index)) === true ? { ok: true, applied: true } : refused({ code: 'cut-refused' });
        });
      };
    }
    function pinned(write) {
      return session && typeof write === 'function' ? function (payload) {
        return guarded(function () {
          var pin = session.info();
          return write.call(session, payload, pin, function () { return session.isActive() && same(session.info(), pin); })
            .then(function (r) { return r && typeof r.applied === 'boolean' ? { ok: true, applied: r.applied } : refused({ code: 'malformed-receipt' }); });
        });
      } : null;
    }
    var property = pinned(session && session.editProperty), track = pinned(session && session.editPropertyTrack);
    // spec: { layerUid, property, frame, action:{kind, ...} }
    if (property) ops.property = function (spec) {
      return property({ command: 'object.property.edit', target: target(spec.layerUid), property: spec.property, action: Object.assign({ frame: spec.frame }, spec.action) });
    };
    // spec: { layerUid, property, frame, preset }
    if (track) ops.track = function (spec) {
      return track({ command: 'object.property.track.edit', selections: [{ target: target(spec.layerUid), property: spec.property, frame: spec.frame }], action: { kind: 'preset', preset: spec.preset } });
    };
    if (comp && typeof comp.readWhenPresented === 'function' && typeof comp.edit === 'function') {
      // spec: { action:'add'|'remove', frame, name } - removal resolves the marker's ordinal from the presented projection.
      ops.marker = function (spec) {
        return guarded(async function () {
          var shown = await comp.readWhenPresented(), action;
          if (!shown || !shown.composition) return refused({ code: 'composition-unavailable' });
          if (spec.action === 'add') { action = { kind: 'addMarker', frame: spec.frame }; if (spec.name) action.name = spec.name; }
          else {
            var found = (shown.composition.markers || []).filter(function (m) { return m.frame === spec.frame; })[0];
            if (!found) return { ok: true, applied: false };
            action = { kind: 'removeMarker', ordinal: found.ordinal };
          }
          var r = await comp.edit(action, shown);
          return r && (r.status === 'applied-presented' || r.status === 'unchanged') ? { ok: true, applied: r.applied === true } : refused({ code: r && r.status });
        });
      };
    }
    return ops;
  }
  return Object.freeze({ create: create });
}));
