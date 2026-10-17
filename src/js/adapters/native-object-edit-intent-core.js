/* L02-J shared intent runner for the native object edit adapters. It owns no document, no history and no
 * fallback writer: every intent becomes ONE closed typed request pinned to the presented native revision and
 * is submitted through an injected port; refusal, staleness and absence are typed results, never retries. */
(function (root, factory) {
  const contract = typeof module === 'object' && module.exports ? require('./native-object-edit-contract.js') : root.NemoNativeObjectEditContract;
  const api = factory(contract);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectEditIntent = api;
}(typeof globalThis === 'object' ? globalThis : this, function (contract) {
  'use strict';

  const MAX_REQUEST_BYTES = 4096;
  const identifier = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;
  const refused = /^Native command refused: ([a-z_]+)/;
  const PORTS = ['pin', 'declared', 'submit', 'requestId'];

  function failure(operation, code, extra) { return Object.freeze(Object.assign({ ok: false, operation, code }, extra)); }
  function safeRevision(value) { return Number.isSafeInteger(value) && value >= 0; }
  function pinOk(pin) {
    return !!pin && typeof pin.instanceId === 'string' && typeof pin.documentId === 'string' && safeRevision(pin.contentRevision);
  }
  // Presentation ids that still name a record of the current native projection, in projection order.
  function reconcileIds(previous, current) {
    const wanted = new Set(Array.isArray(previous) ? previous : []);
    const seen = new Set();
    return (Array.isArray(current) ? current : []).filter((id) => wanted.has(id) && !seen.has(id) && seen.add(id));
  }
  function stale(previous, current) {
    const live = new Set(Array.isArray(current) ? current : []);
    return (Array.isArray(previous) ? previous : []).filter((id) => !live.has(id));
  }

  // The caller supplies exactly the declared members; `command` is added here and nothing else is forwarded.
  function build(operation, input) {
    const keys = contract.members(operation);
    if (!input || typeof input !== 'object' || Array.isArray(input)) throw new TypeError('intent input must be a plain object');
    const own = Object.keys(input);
    if (own.some((key) => !keys.includes(key)) || keys.some((key) => !own.includes(key))) throw new TypeError('intent input has missing or unknown fields');
    const payload = { command: contract.commandOf(operation) };
    keys.forEach((key) => { payload[key] = input[key]; });
    return payload;
  }

  function createIntentRunner(ports) {
    if (!ports || PORTS.some((name) => typeof ports[name] !== 'function')) throw new TypeError('intent runner requires pin, declared, submit and requestId ports');
    let tail = Promise.resolve();

    async function execute(operation, input, label) {
      if (!contract.admitted(operation)) return failure(operation, 'unavailable', { reason: 'operation_not_declared' });
      let declared = false;
      try { declared = ports.declared(operation) === true; } catch (_) { declared = false; }
      if (!declared) return failure(operation, 'unavailable', { reason: 'native_declaration_absent' });
      let pin = null;
      try { pin = ports.pin(); } catch (_) { pin = null; }
      if (!pinOk(pin)) return failure(operation, 'unavailable', { reason: 'no_presented_native_session' });
      let request;
      try {
        const body = build(operation, input);
        contract.payload(operation, body);
        const requestId = ports.requestId(label);
        if (typeof requestId !== 'string' || !identifier.test(requestId)) throw new TypeError('request id is not a bounded identifier');
        request = {
          apiVersion: 2, requestId, instanceId: pin.instanceId, documentId: pin.documentId,
          operation, expectedRevision: pin.contentRevision, payload: JSON.parse(JSON.stringify(body)),
        };
      } catch (error) { return failure(operation, 'invalid_request', { reason: String(error && error.message || error) }); }
      if (contract.bytes(JSON.stringify(request)) > MAX_REQUEST_BYTES) return failure(operation, 'request_too_large', { limit: MAX_REQUEST_BYTES });
      let response;
      try { response = await ports.submit(request, pin); } catch (error) {
        const match = refused.exec(String(error && error.message || ''));
        // A refused command was classified by the owner; any other rejection cannot say whether it committed.
        if (match) return failure(operation, match[1], { requestId: request.requestId, stale: match[1] === 'stale_revision' });
        return failure(operation, 'transport_failed', { requestId: request.requestId, indeterminate: true, message: String(error && error.message || error) });
      }
      if (!response || response.requestId !== request.requestId || typeof response.ok !== 'boolean') {
        return failure(operation, 'transport_failed', { requestId: request.requestId, indeterminate: true, message: 'uncorrelated native response' });
      }
      if (!response.ok) {
        const code = response.error && response.error.code || 'unavailable';
        return failure(operation, code, { requestId: request.requestId, revision: response.contentRevision, stale: code === 'stale_revision', message: response.error && response.error.message });
      }
      if (!response.result || typeof response.result !== 'object' || Array.isArray(response.result)) {
        return failure(operation, 'transport_failed', { requestId: request.requestId, indeterminate: true, message: 'native response carries no result' });
      }
      let after = null;
      try { after = ports.pin(); } catch (_) { after = null; }
      const outcome = {
        ok: true, operation, requestId: request.requestId, applied: response.result.applied === true,
        revision: response.contentRevision, pin: pinOk(after) ? after : null,
        synchronized: pinOk(after) && after.contentRevision === response.contentRevision,
      };
      if (typeof response.result.layerUid === 'string') outcome.layerUid = response.result.layerUid;
      return Object.freeze(outcome);
    }
    // One pinned mutation at a time. `exclusive(body)` runs `body(exec)` in the runner's turn, so the caller's own reads
    // (stale-id check, default selection) and the pin read by `exec` all see what the predecessor left behind.
    function exclusive(body) {
      const next = tail.then(() => body((operation, input, label) => execute(operation, input, label || 'object-edit')));
      tail = next.then(() => undefined, () => undefined);
      return next;
    }
    const run = (operation, input, label) => exclusive((exec) => exec(operation, input, label));
    return Object.freeze({ run, exclusive });
  }

  // Shape selection by stable native stroke id. `ports.strokeIds({layerUid, contextId, frame})` returns the ids of the
  // CURRENT projection (or null): after every applied revision the selection is rebuilt from it, so an id the owner
  // no longer lists is dropped and reported, and a request naming such an id is refused before it is sent.
  function createShapeEditor(ports, runner) {
    let selected = [];
    const read = (target) => {
      if (typeof ports.strokeIds !== 'function') return null;
      try { const ids = ports.strokeIds(target); return Array.isArray(ids) ? ids.slice() : null; } catch (_) { return null; }
    };
    const targetOf = (input) => (input && typeof input === 'object' ? { layerUid: input.layerUid, contextId: input.contextId, frame: input.frame } : {});
    // Empty or non-string members are malformed input, not stale ids: the contract names them.
    const named = (input) => (input && Array.isArray(input.strokeIds) ? input.strokeIds : input && typeof input.strokeId === 'string' ? [input.strokeId] : []).filter((id) => typeof id === 'string' && id.length);
    // The stale-id check and the selection rebuild run inside the runner's turn: input is judged against the projection
    // the pinned request will actually be applied to.
    function edit(operation, input, label) {
      return runner.exclusive(async (exec) => {
        let declared = false;
        try { declared = ports.declared(operation) === true; } catch (_) { declared = false; }
        const target = targetOf(input), before = declared ? read(target) : null;
        const gone = before ? stale(named(input), before) : [];
        if (gone.length) return failure(operation, 'stale_selection', { ids: gone });
        const outcome = await exec(operation, input, label);
        if (!outcome.ok || !outcome.applied) return outcome;
        const after = read(target), previous = selected;
        selected = after ? reconcileIds(previous, after) : [];
        const created = after && before ? after.filter((id) => !before.includes(id)) : [];
        return Object.freeze(Object.assign({}, outcome, {
          selection: selected.slice(), rejected: after ? stale(previous, after) : previous.slice(), created, selectionRebuilt: !!after,
        }));
      });
    }
    function select(target, ids) {
      const current = read(target);
      if (!current) return failure('select', 'unavailable', { reason: 'no_stroke_projection' });
      const gone = stale(ids, current);
      if (gone.length) return failure('select', 'stale_selection', { ids: gone });
      selected = reconcileIds(ids, current);
      return Object.freeze({ ok: true, operation: 'select', selection: selected.slice() });
    }
    return Object.freeze({ edit, select, selection: () => selected.slice() });
  }
  // Maps the presented native Project session to the intent ports. `session.editObject(request, pin)` is the session's pinned
  // mutation entry; while the session does not offer it every intent is typed unavailability. `hooks.strokeIds` reads a layer cell.
  function portsFromSession(session, hooks) {
    const present = () => !!session && typeof session.isActive === 'function' && session.isActive() === true;
    return {
      pin: () => (present() ? session.info() : null),
      declared: (operation) => contract.admitted(operation) && present() && typeof session.editObject === 'function',
      submit: (request, pin) => session.editObject(request, pin),
      requestId: (label) => session.requestId(label),
      strokeIds: hooks && typeof hooks.strokeIds === 'function' ? hooks.strokeIds : undefined,
    };
  }
  return Object.freeze({ createIntentRunner, createShapeEditor, portsFromSession, reconcileIds, stale, failure, MAX_REQUEST_BYTES });
}));
