/* L01-J typed intents of the native vector operations (G14 create/replace/delete, G22 parameter edit, G13 path-effect stack)
 * over the single native application client. Thin: each method maps a baseline stroke dictionary or tool argument to ONE closed
 * request pinned to the presented revision, sends it through the serialized native session and returns a typed status. It
 * never touches Paper, Paper items, the legacy frame store or WASM geometry (shape-bridge, tools.js and path-fx.js are not
 * reachable from here), and a missing, busy or too-old native session is typed unavailability, never a legacy fallback.
 * Statuses: applied | noop | unavailable | unmapped | invalid | too_large | stale | refused | unconfirmed | indeterminate. */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const command = commonjs ? require('./native-object-vector-command.js') : root.NemoNativeObjectVectorCommand;
  const contract = commonjs ? require('./native-object-vector-contract.js') : root.NemoNativeObjectVectorContract;
  const stroke = commonjs ? require('./native-object-vector-stroke.js') : root.NemoNativeObjectVectorStroke;
  const api = factory(command, contract, stroke);
  if (commonjs) module.exports = api;
  else root.NemoNativeObjectVector = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Command, Contract, Stroke) {
  'use strict';

  const PIN_KEYS = ['instanceId', 'documentId', 'contentRevision', 'documentSnapshotId'];
  const fault = Command.fault;
  const unavailable = (reason) => Object.freeze({ status: 'unavailable', reason });
  function stableTarget(layerUid, frame, strokeId) {
    return { contextId: 'scene-root', frameScope: { kind: 'authored', frame }, layerUid, strokeId };
  }
  // Maps a rejected command to a status. The faults are the shared command protocol's; a string code that is none of the local
  // ones is the owner's own refusal (stale_revision, not_found, wrong_document, locked/guarded = unavailable, ...).
  function failed(error) {
    const code = error && error.code, reason = (error && (error.reason || error.message)) || null;
    if (typeof code !== 'string') return unavailable(reason || 'native_session_failed');
    if (code === 'request_too_large') return Object.freeze({ status: 'too_large', bytes: error.bytes, limit: error.limit });
    if (code === 'invalid_request' && error.reason === 'local_validation') return Object.freeze({ status: 'invalid', reason: error.message });
    if (code === 'stale_origin') return Object.freeze({ status: 'stale', reason: error.reason || 'origin_changed' });
    // The same request id and body go back to the same owner: it answers the stored receipt or executes exactly once.
    if (code.endsWith('_command_indeterminate')) {
      return Object.freeze({ status: 'indeterminate', requestId: error.requestId, replay: typeof error.replay === 'function' ? () => error.replay().catch(failed) : undefined });
    }
    if (code === 'uncorrelated') return Object.freeze({ status: 'unconfirmed', requestId: error.requestId });
    if (code === 'unavailable') return unavailable(error.reason || reason);
    return Object.freeze({ status: 'refused', code, reason: error.reason || null, requestId: error.requestId, contentRevision: error.contentRevision });
  }
  function mapped(result) {
    const out = { status: result.code === 'unavailable' ? 'unavailable' : 'unmapped', code: result.code, reason: result.reason };
    if (result.members) out.members = result.members.slice();
    return Object.freeze(out);
  }
  // The origin the caller read: every pin it names must still be the session's, or nothing is sent.
  function checkOrigin(origin, pin) {
    if (origin === undefined) return;
    if (!origin || typeof origin !== 'object' || PIN_KEYS.some((key) => origin[key] !== pin[key])) {
      throw fault('stale_origin', 'The presented native revision is not the origin the edit was prepared against', { reason: 'origin_changed' });
    }
  }

  function create(session) {
    const commands = session ? Command.create(session, 'vector') : null;
    function send(operation, body, options, describe) {
      if (!commands) return Promise.resolve(unavailable('no_native_session'));
      const origin = options && options.origin;
      let pinned = null;
      return commands.execute(operation, (pin) => {
        checkOrigin(origin, pin);
        pinned = Object.freeze(PIN_KEYS.reduce((acc, key) => { acc[key] = pin[key]; return acc; }, {}));
        return body;
      }, (receipt) => Object.freeze(Object.assign({
        status: receipt.applied ? 'applied' : 'noop', operation: body.command, requestId: receipt.requestId,
        revision: receipt.contentRevision, origin: pinned, receipt,
      }, describe ? describe(receipt) : null))).catch(failed);
    }
    const strokeFields = (receipt) => ({ strokeId: receipt.strokeId });

    function createStroke(layerUid, frame, dict, options) {
      const map = Stroke.toRecord(dict, { layerUid, frame, strokeId: '' }, { opacity: options && options.opacity });
      if (!map.ok) return Promise.resolve(mapped(map));
      return send(Contract.VECTOR, { command: 'object.vector.create', record: map.record }, options,
        (receipt) => ({ strokeId: receipt.strokeId, authoredStrokeId: map.authoredStrokeId, unmapped: map.unmapped }));
    }
    function replaceStroke(target, dict, options) {
      if (!target || typeof target.strokeId !== 'string' || !target.strokeId) return Promise.resolve(Object.freeze({ status: 'invalid', reason: 'a replace names the stroke it replaces' }));
      const map = Stroke.toRecord(dict, { layerUid: target.layerUid, frame: target.frame, strokeId: target.strokeId }, { opacity: options && options.opacity });
      if (!map.ok) return Promise.resolve(mapped(map));
      return send(Contract.VECTOR, { command: 'object.vector.replace', record: map.record }, options, (receipt) => ({ strokeId: receipt.strokeId, unmapped: map.unmapped }));
    }
    function deleteStroke(target, options) {
      const at = target || {};
      return send(Contract.VECTOR, { command: 'object.vector.delete', stableTarget: stableTarget(at.layerUid, at.frame, at.strokeId) }, options, strokeFields);
    }
    function parametricSend(command, target, body, options) {
      const at = target || {};
      return send(Contract.PARAMETRIC, Object.assign({ command, stableTarget: stableTarget(at.layerUid, at.frame, at.strokeId) }, body), options);
    }
    // One native call is one committed net change: an adapter commits a drag's NET transform from its pointer-down state.
    const setParametric = (target, paramShape, options) => parametricSend('object.parametric.set', target, { paramShape }, options);
    const translateParametric = (target, delta, options) => parametricSend('object.parametric.translate', target, { delta }, options);
    const scaleParametric = (target, scale, anchor, options) => parametricSend('object.parametric.scale', target, { scale, anchor }, options);
    // `stack` is the layer's COMPLETE new ordered list (the baseline panel's add, toggle, edit and delete are all ld.pathFx writes).
    function setPathEffects(layerUid, stack, options) {
      const map = Stroke.stackToNative(stack);
      if (!map.ok) return Promise.resolve(mapped(map));
      return send(Contract.EFFECTS, { command: 'object.path.effects.set', layerUid, stack: map.stack }, options);
    }
    return Object.freeze({ createStroke, replaceStroke, deleteStroke, setParametric, translateParametric, scaleParametric, setPathEffects, defaultEffect: Stroke.defaultsFor });
  }

  // Read-back validator: the record the owner returns for a target equals what was sent (numbers by value, the allocated
  // strokeId taken from the create receipt) and is itself a valid native record.
  function same(a, b) {
    if (typeof a === 'number' && typeof b === 'number') return Object.is(a, b) || a === b;
    if (Array.isArray(a)) return Array.isArray(b) && a.length === b.length && a.every((item, at) => same(item, b[at]));
    if (a && b && typeof a === 'object' && typeof b === 'object') {
      const keys = Object.keys(a);
      return keys.length === Object.keys(b).length && keys.every((key) => Object.prototype.hasOwnProperty.call(b, key) && same(a[key], b[key]));
    }
    return a === b;
  }
  function verifyReadBack(sent, readBack, receipt) {
    try { Contract.record(readBack, false); } catch (error) { return Object.freeze({ ok: false, reason: 'invalid_record', message: error.message }); }
    const expected = JSON.parse(JSON.stringify(sent));
    if (receipt && typeof receipt.strokeId === 'string' && expected.target.strokeId === '') expected.target.strokeId = receipt.strokeId;
    return same(expected, readBack) ? Object.freeze({ ok: true }) : Object.freeze({ ok: false, reason: 'mismatch' });
  }

  const bindings = new WeakMap();
  // One binding per host window, as the lane-6 adapters: a host without the native-only session has no vector client.
  function desktop(host) {
    const module = host && host.NemoNativeObjectProject, session = module && module.desktop(host);
    if (!session || !session.blocksLegacy()) return null;
    if (!bindings.has(host)) bindings.set(host, create(session));
    return bindings.get(host);
  }
  return Object.freeze({ create, desktop, verifyReadBack, stableTarget, OPERATIONS: Contract.OPERATIONS });
}));
