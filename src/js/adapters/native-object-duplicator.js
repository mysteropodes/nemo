/* L04-N duplicator setup intents over the single native application client: enable or disable a duplicator on a
 * layer, turn a layer selection into an object duplicator, and pick the path layer. Each is ONE closed typed request
 * (command.document.object.duplicator.*) pinned to the presented revision. toggleLayerDuplicator,
 * convertSelectionToObjectDuplicator, the duplicator panel writers and applyLayerDuplicator are never called here:
 * placement, count, modes and effectors are evaluated by the native engine, not by this adapter. */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const contract = commonjs ? require('./native-object-component-contract.js') : root.NemoNativeObjectComponentContract;
  const core = commonjs ? require('./native-object-component-intent.js') : root.NemoNativeObjectComponentIntent;
  const api = factory(contract, core);
  if (commonjs) module.exports = api;
  else root.NemoNativeObjectDuplicator = api;
}(typeof globalThis === 'object' ? globalThis : this, function (contract, core) {
  'use strict';

  const OP = (suffix) => contract.PREFIX + suffix;
  const fail = core.failure;
  // Largest seed the baseline draws: Math.floor(Math.random() * 1e6) (toggleLayerDuplicator and
  // convertSelectionToObjectDuplicator, both verbatim). The native owner refuses anything above it when it enables a duplicator.
  const SEED_SPAN = 1e6;
  function drawSeed(random) { return Math.floor(random() * SEED_SPAN); }

  // ports: pin(), declared(operation), submit(request, pin), requestId(label).
  // hooks (optional): random() -> [0, 1) (default Math.random); isDuplicator(layerUid) -> boolean|null, the native state of a
  // layer for toggle() (absent or null: toggle is typed unavailability, a target-state call needs no read).
  function createDuplicatorIntent(ports, hooks) {
    hooks = hooks || {};
    const runner = core.createIntentRunner(ports);
    const random = typeof hooks.random === 'function' ? hooks.random : () => Math.random();

    function currentIds() { try { return core.layerIds(ports.pin()); } catch (_) { return null; } }
    function seedNow(operation) {
      let seed;
      try { seed = drawSeed(random); } catch (error) { return { error: fail(operation, 'invalid_request', { reason: String(error && error.message || error) }) }; }
      return Number.isSafeInteger(seed) && seed >= 0 && seed < SEED_SPAN ? { seed } : { error: fail(operation, 'invalid_request', { reason: 'seed outside 0..999999' }) };
    }

    // The target state of toggleLayerDuplicator: enabling writes the baseline default descriptor with a fresh seed and locks
    // the layer, disabling removes the descriptor and unlocks it. Asking for the state a layer already has changes nothing.
    function setEnabled(layerUid, enabled) {
      return runner.exclusive(async (exec) => {
        const operation = OP('duplicator.set');
        if (typeof layerUid !== 'string' || !layerUid.length || typeof enabled !== 'boolean') {
          return fail(operation, 'invalid_request', { reason: 'layerUid and enabled are required' });
        }
        const ids = currentIds();
        if (ids && core.stale([layerUid], ids).length) return fail(operation, 'stale_selection', { ids: [layerUid] });
        if (!enabled) return exec(operation, { layerUid, enabled: false }, 'duplicator-set');
        const drawn = seedNow(operation);
        if (drawn.error) return drawn.error;
        return exec(operation, { layerUid, enabled: true, seed: drawn.seed }, 'duplicator-set');
      });
    }
    function toggle(layerUid) {
      const operation = OP('duplicator.set');
      let state = null;
      try { state = typeof hooks.isDuplicator === 'function' ? hooks.isDuplicator(layerUid) : null; } catch (_) { state = null; }
      if (typeof state !== 'boolean') return Promise.resolve(fail(operation, 'unavailable', { reason: 'duplicator_state_unavailable' }));
      return setEnabled(layerUid, !state);
    }

    // convertSelectionToObjectDuplicator(indices): the selection is de-duplicated and taken in document order; a pool
    // of fewer than two layers is refused without a request. The owner skips layers that already host a duplicator.
    function fromObjects(layerUids) {
      return runner.exclusive(async (exec) => {
        const operation = OP('duplicator.objects');
        if (!Array.isArray(layerUids) || !layerUids.every((uid) => typeof uid === 'string' && uid.length)) {
          return fail(operation, 'invalid_request', { reason: 'layerUids must name layers' });
        }
        const wanted = layerUids.filter((uid, at) => layerUids.indexOf(uid) === at);
        if (wanted.length < 2) return fail(operation, 'invalid_request', { reason: 'too_few_layers', message: 'Select at least 2 layers' });
        const ids = currentIds();
        if (ids) {
          const gone = core.stale(wanted, ids);
          if (gone.length) return fail(operation, 'stale_selection', { ids: gone });
          wanted.sort((a, b) => ids.indexOf(a) - ids.indexOf(b));
        }
        const drawn = seedNow(operation);
        if (drawn.error) return drawn.error;
        const outcome = await exec(operation, { layerUids: wanted, seed: drawn.seed }, 'duplicator-objects');
        // The host layer the owner minted is the one the caller selects, as the baseline activates it.
        return outcome.ok ? Object.freeze(Object.assign({}, outcome, { hostLayerUid: outcome.layerUid })) : outcome;
      });
    }

    // The path-layer picker: a layer to follow, or null to clear it. A layer that is itself a duplicator, the host itself and
    // an unknown layer are the owner's typed refusals.
    function setPath(layerUid, pathLayerUid) {
      return runner.exclusive(async (exec) => {
        const operation = OP('duplicator.path');
        if (typeof layerUid !== 'string' || !layerUid.length || !(pathLayerUid === null || typeof pathLayerUid === 'string')) {
          return fail(operation, 'invalid_request', { reason: 'layerUid and pathLayerUid (string or null) are required' });
        }
        const ids = currentIds();
        if (ids) {
          const gone = core.stale([layerUid].concat(pathLayerUid === null || pathLayerUid === '' ? [] : [pathLayerUid]), ids);
          if (gone.length) return fail(operation, 'stale_selection', { ids: gone });
        }
        return exec(operation, { layerUid, pathLayerUid: pathLayerUid === '' ? null : pathLayerUid }, 'duplicator-path');
      });
    }
    return Object.freeze({ setEnabled, toggle, fromObjects, setPath });
  }

  const bindings = new WeakMap();
  // The presented desktop Project session -> intents. Null while the native Project session does not own the document.
  function desktop(host, hooks) {
    const project = host && host.NemoNativeObjectProject;
    const session = project && typeof project.desktop === 'function' ? project.desktop(host) : null;
    if (!session || typeof session.blocksLegacy !== 'function' || !session.blocksLegacy()) return null;
    if (!bindings.has(host)) bindings.set(host, createDuplicatorIntent(core.portsFromSession(session), hooks));
    return bindings.get(host);
  }
  return Object.freeze({
    createDuplicatorIntent, desktop, drawSeed,
    OPERATIONS: Object.freeze(['duplicator.set', 'duplicator.objects', 'duplicator.path'].map(OP)),
  });
}));
