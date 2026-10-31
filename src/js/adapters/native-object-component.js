/* L04-N component intents over the single native application client: convert, place and expose are each ONE closed
 * typed request (command.document.object.component.*) pinned to the presented revision; the read model, the
 * StoryBoard chain and entering a component go through injected native ports and the real workspace tab. There is no
 * JS fallback writer: convertLayerToComponent, convertLayersToComponent, placeComponentInstance,
 * exposeSymbolProperty, enterSymbol, exitToScene and the storyboard.js state are never called or read here, and
 * animating a flat layer never converts it (conversion is only ever an explicit request). */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const contract = commonjs ? require('./native-object-component-contract.js') : root.NemoNativeObjectComponentContract;
  const core = commonjs ? require('./native-object-component-intent.js') : root.NemoNativeObjectComponentIntent;
  const reads = commonjs ? require('./native-object-component-read-contract.js') : root.NemoNativeObjectComponentReadContract;
  const workspaceContract = commonjs ? require('./native-object-workspace-context-contract.js') : root.NemoNativeObjectWorkspaceContextContract;
  const api = factory(contract, core, reads, workspaceContract);
  if (commonjs) module.exports = api;
  else root.NemoNativeObjectComponent = api;
}(typeof globalThis === 'object' ? globalThis : this, function (contract, core, reads, workspaceContract) {
  'use strict';

  const SCENE = 'scene-root';
  const LIST_ATTEMPTS = 3; // restarts of a context listing the project moved under
  const OP = (suffix) => contract.PREFIX + suffix;
  const fail = core.failure;
  const has = (value, key) => !!value && typeof value === 'object' && Object.prototype.hasOwnProperty.call(value, key);

  // `expose()` of src/js/domain/component/exposed-properties.js at the baseline, key line verbatim:
  // 'ep_' + clock().toString(36) + '_' + random().toString(36).slice(2, 6). The clock and the random source are injected.
  function exposedKey(clock, random) {
    return 'ep_' + clock().toString(36) + '_' + random().toString(36).slice(2, 6);
  }
  // A detached, frozen copy: a caller can never mutate what a native reader returned.
  function detach(value) {
    const copy = JSON.parse(JSON.stringify(value));
    (function freeze(node) {
      if (node && typeof node === 'object' && !Object.isFrozen(node)) { Object.freeze(node); Object.values(node).forEach(freeze); }
    }(copy));
    return copy;
  }

  // ports: pin(), declared(operation), submit(request, pin), requestId(label).
  // hooks (all optional, each absent one is typed unavailability, never a legacy path):
  //   clock() -> ms, random() -> [0, 1)      key material of a new exposed property (default Date.now / Math.random)
  //   activeContext() -> context id          the context the user is working in (default 'scene-root', the only context presented today)
  //   model.read() -> promise of the native component read model ({contexts: [{contextId}], storyboard?})
  //   tabs {active(), enter(contextId), exit()}   the real workspace tab strip
  //   splitPlan(contextId) -> promise of an array, one entry per layer the entry would split (empty = already idempotent)
  function createComponentIntent(ports, hooks) {
    hooks = hooks || {};
    const runner = core.createIntentRunner(ports);
    const clock = typeof hooks.clock === 'function' ? hooks.clock : () => Date.now();
    const random = typeof hooks.random === 'function' ? hooks.random : () => Math.random();

    function currentIds() { try { return core.layerIds(ports.pin()); } catch (_) { return null; } }
    function contextNow() {
      let value = SCENE;
      try { if (typeof hooks.activeContext === 'function') value = hooks.activeContext(); } catch (_) { value = SCENE; }
      return typeof value === 'string' ? value : SCENE;
    }

    // convertLayerToComponent (one layer, in place) / convertLayersToComponent (several, merged at the first slot).
    // The native plan sorts and de-duplicates by document order; the adapter sends the document order itself.
    function convert(layerUids) {
      return runner.exclusive(async (exec) => {
        const operation = OP('component.convert');
        if (!Array.isArray(layerUids) || !layerUids.length || !layerUids.every((uid) => typeof uid === 'string' && uid.length)) {
          return fail(operation, 'invalid_request', { reason: 'layerUids must name at least one layer' });
        }
        const wanted = layerUids.filter((uid, at) => layerUids.indexOf(uid) === at);
        const ids = currentIds();
        if (ids) {
          const gone = core.stale(wanted, ids);
          if (gone.length) return fail(operation, 'stale_selection', { ids: gone });
          wanted.sort((a, b) => ids.indexOf(a) - ids.indexOf(b));
        }
        const outcome = await exec(operation, { contextId: contextNow(), layerUids: wanted }, 'component-convert');
        // A merge reports the instance it minted; a single layer becomes the instance in place.
        return outcome.ok ? Object.freeze(Object.assign({}, outcome, { instanceLayerUid: outcome.layerUid || wanted[0], merged: wanted.length > 1 })) : outcome;
      });
    }

    // placeComponentInstance(symId, layerName): a new locked layer that instantiates an existing component.
    function place(symbolId, name) {
      return runner.run(OP('component.place'), { symbolId, name }, 'component-place');
    }

    // exposeSymbolProperty(symId, targetStrokeId, targetField, label, defaultVal). `defaultValue` undefined stays absent.
    function expose(symbolId, targetStrokeId, targetField, label, defaultValue) {
      let key;
      try { key = exposedKey(clock, random); } catch (error) {
        return Promise.resolve(fail(OP('component.expose'), 'invalid_request', { reason: String(error && error.message || error) }));
      }
      const input = { symbolId, key, targetStrokeId, targetField, label };
      if (defaultValue !== undefined) input.default = defaultValue;
      return runner.run(OP('component.expose'), input, 'component-expose');
    }

    // #2256: the read-only native component model (contexts, one context's summary, its layers, the scene layers that instance it),
    // pinned to the presented revision and snapshot. Reads nothing from the legacy symbols table and opens no context. A response
    // is judged by the read contract at this edge, and an answer is discarded when the project moved on while it was read.
    // `want` ({contentRevision, documentSnapshotId}, optional) is the pin a LATER page of one listing must still be at: when the
    // project has moved off it the read is refused as stale_revision without being sent, so pages never mix two revisions.
    function read(selector, want, via) { return pinned(reads, 'component.read', 'component-read', () => ({ selector: JSON.parse(JSON.stringify(selector)) }), want, via); }
    // One pinned, single-route read of a pure native query (no expectedRevision, no reconcile) judged by its contract.
    // `via` ({dispatch(request)}, optional) is the lease of a revision synchronization or a native mutation that is already running
    // inside the project's queue: the read rides it directly instead of waiting for that queue (it would wait for itself) and
    // takes no intent turn. Everything else about the read (pin, correlation, staleness) is unchanged.
    function pinned(mod, operation, label, extra, want, via) {
      const turn = via ? (body) => body() : (body) => runner.exclusive(body);
      return turn(async () => {
        const unavailable = (reason, more) => fail(operation, 'unavailable', Object.assign({ reason }, more));
        if (!mod || typeof ports.read !== 'function' || typeof ports.readDeclared !== 'function') return unavailable('component_read_unavailable');
        let declared = false;
        try { declared = ports.readDeclared(mod.OP) === true; } catch (_) { declared = false; }
        if (!declared) return unavailable('native_declaration_absent');
        let pin = null;
        try { pin = ports.pin(); } catch (_) { pin = null; }
        if (!pin || typeof pin.instanceId !== 'string' || typeof pin.documentId !== 'string' || !Number.isSafeInteger(pin.contentRevision) || typeof pin.documentSnapshotId !== 'string') return unavailable('no_presented_native_session');
        if (want && (pin.contentRevision !== want.contentRevision || pin.documentSnapshotId !== want.documentSnapshotId)) return fail(operation, 'stale_revision', { stale: true, reason: 'listing_pin_moved' });
        let request;
        try {
          const body = Object.assign({ atRevision: pin.contentRevision, documentSnapshotId: pin.documentSnapshotId }, extra());
          mod.payload(mod.OP, body);
          const requestId = ports.requestId(label);
          if (typeof requestId !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(requestId)) throw new TypeError('request id is not a bounded identifier');
          request = { apiVersion: 2, requestId, instanceId: pin.instanceId, documentId: pin.documentId, operation: mod.OP, payload: body };
        } catch (error) { return fail(operation, 'invalid_request', { reason: String(error && error.message || error) }); }
        let response;
        try { response = await (via ? via.dispatch(request) : ports.read(request)); } catch (error) {
          if (error && error.notSent === true) return unavailable(error.reason || 'native_project_busy', { requestId: request.requestId });
          return fail(operation, 'transport_failed', { requestId: request.requestId, message: String(error && error.message || error) });
        }
        if (!response || response.requestId !== request.requestId || typeof response.ok !== 'boolean') return fail(operation, 'transport_failed', { requestId: request.requestId, message: 'uncorrelated native response' });
        if (!response.ok) {
          const code = response.error && response.error.code || 'unavailable';
          return fail(operation, code, { requestId: request.requestId, stale: code === 'stale_revision', message: response.error && response.error.message });
        }
        try { mod.correlate(request, response); } catch (bad) { return fail(operation, 'transport_failed', { requestId: request.requestId, message: 'uncorrelated native read: ' + bad.message }); }
        let after = null;
        try { after = ports.pin(); } catch (_) { after = null; }
        if (!after || after.contentRevision !== pin.contentRevision || after.documentSnapshotId !== pin.documentSnapshotId) return fail(operation, 'stale_revision', { requestId: request.requestId, stale: true });
        return Object.freeze({ ok: true, operation, requestId: request.requestId, revision: response.contentRevision, instanceId: pin.instanceId, documentId: pin.documentId, result: detach(response.result) });
      });
    }
    // #2256: the enter/exit workspace. The session workspace is view state held HERE (like the viewport navigation), never in the
    // document: each transition sends the held workspace and one action, the native owner applies the baseline rules and answers
    // the new workspace, which is kept only when the answer is correlated and current. A different project resets it to the scene.
    let held = null;
    const sceneWorkspace = (view) => ({ activeContextId: null, openTabs: [], view: Object.assign({}, view), sceneView: null });
    function workspaceHeld(pin) {
      return held && pin && held.instanceId === pin.instanceId && held.documentId === pin.documentId ? held.workspace : null;
    }
    // The workspace the current project holds (a frozen copy), or null when none was ever opened for it.
    function workspaceNow() {
      let pin = null;
      try { pin = ports.pin(); } catch (_) { pin = null; }
      const workspace = workspaceHeld(pin);
      return workspace ? detach(workspace) : null;
    }
    // `view` is the live {frame, workIn, workOut} of the presented context (the caller owns the playhead).
    async function workspaceTransition(action, view, via) {
      const operation = 'workspace.' + (action && action.kind);
      let pin = null;
      try { pin = ports.pin(); } catch (_) { pin = null; }
      if (!pin) return fail(operation, 'unavailable', { reason: 'no_presented_native_session' });
      const base = Object.assign({}, workspaceHeld(pin) || sceneWorkspace(view), { view: Object.assign({}, view) });
      const answer = await pinned(workspaceContract, operation, 'workspace-context', () => ({ workspace: JSON.parse(JSON.stringify(base)), action }), undefined, via);
      if (!answer.ok) return answer;
      const next = answer.result;
      held = { instanceId: answer.instanceId, documentId: answer.documentId, workspace: next.workspace };
      return Object.freeze({ ok: true, operation, outcome: next.outcome, contextId: next.contextId, totalFrames: next.totalFrames, fps: next.fps, workspace: next.workspace, revision: answer.revision });
    }
    // Every item of one paged component listing in order, following nextCursor; bounded so a malformed owner cannot loop.
    // One listing is ONE revision: the first page fixes the pin and every later page must still be at it. When the project moves
    // between pages (a convert or delete would skip or duplicate an item) the listing restarts from cursor 0, a bounded number
    // of times, then answers a typed stale refusal; a mixed list is never returned.
    async function listPagesAt(operation, base, via) {
      const items = [];
      let cursor = 0, want = null;
      for (let guard = 0; guard < 64; guard += 1) {
        const page = await read(Object.assign({}, base, { cursor, limit: reads ? reads.MAX_PAGE : 16 }), want, via);
        if (!page.ok) return fail(operation, page.code, { reason: page.reason, message: page.message, stale: page.stale });
        if (!want) want = { contentRevision: page.result.atRevision, documentSnapshotId: page.result.documentSnapshotId };
        items.push(...page.result.items);
        if (page.result.nextCursor === null) return Object.freeze({ ok: true, operation, revision: want.contentRevision, items: Object.freeze(items) });
        cursor = page.result.nextCursor;
      }
      return fail(operation, 'unavailable', { reason: 'component_pages_unbounded' });
    }
    async function listPages(operation, base, via) {
      let last = null;
      for (let attempt = 0; attempt < LIST_ATTEMPTS; attempt += 1) {
        last = await listPagesAt(operation, base, via);
        if (last.ok || last.stale !== true) return last;
      }
      return fail(operation, 'stale_revision', { stale: true, reason: 'component_list_unstable', message: last && last.message });
    }
    // Every context of the document in order (scene-root first); a name the owner had to cut carries nameTruncated: true.
    async function listContexts(via) {
      const out = await listPages('component.contexts', { kind: 'contexts' }, via);
      return out.ok ? Object.freeze({ ok: true, operation: out.operation, revision: out.revision, contexts: out.items }) : out;
    }
    // All layers of one context / all scene layers instancing one component, from one revision.
    function listLayers(contextId, via) { return listPages('component.layers', { kind: 'layers', contextId }, via); }
    function listInstances(contextId) { return listPages('component.instances', { kind: 'instances', contextId }); }

    // The same typed refusal, reported under the operation the caller asked for.
    function refail(operation, failed) { return fail(operation, failed.code, { reason: failed.reason, message: failed.message }); }
    async function readModel() {
      const operation = 'component.model';
      if (!hooks.model || typeof hooks.model.read !== 'function') return fail(operation, 'unavailable', { reason: 'component_read_unavailable' });
      let model;
      try { model = await hooks.model.read(); } catch (error) { return fail(operation, 'unavailable', { reason: 'component_read_failed', message: String(error && error.message || error) }); }
      if (!model || typeof model !== 'object' || Array.isArray(model)) return fail(operation, 'unavailable', { reason: 'component_read_invalid' });
      return Object.freeze({ ok: true, operation, model: detach(model) });
    }
    // StoryBoard thumbnails and montage read the native chain mapping, never the storyboard.js state.
    async function storyboard() {
      const read = await readModel();
      if (!read.ok) return refail('component.storyboard', read);
      if (!has(read.model, 'storyboard') || read.model.storyboard === null || typeof read.model.storyboard !== 'object') {
        return fail('component.storyboard', 'unavailable', { reason: 'storyboard_read_unavailable' });
      }
      return Object.freeze({ ok: true, operation: 'component.storyboard', storyboard: read.model.storyboard });
    }

    function tabsOk() {
      const tabs = hooks.tabs;
      return !!tabs && typeof tabs.active === 'function' && typeof tabs.enter === 'function' && typeof tabs.exit === 'function';
    }
    // The idempotent element split of the entry. A plan with no entry changes nothing; applying a non-empty plan needs a
    // component-scope split transaction that does not exist yet, so it is reported, never faked.
    async function splitOnEntry(contextId) {
      if (typeof hooks.splitPlan !== 'function') return Object.freeze({ state: 'unavailable', reason: 'split_plan_unavailable' });
      let plan;
      try { plan = await hooks.splitPlan(contextId); } catch (_) { return Object.freeze({ state: 'unavailable', reason: 'split_plan_failed' }); }
      if (!Array.isArray(plan)) return Object.freeze({ state: 'unavailable', reason: 'split_plan_invalid' });
      if (!plan.length) return Object.freeze({ state: 'none', planned: 0 });
      return Object.freeze({ state: 'unavailable', reason: 'component_split_transaction_absent', planned: plan.length });
    }
    // enterComponentLayer / enterSymbol: the component opens as a real context in the real tab strip of the single document.
    async function enter(contextId) {
      const operation = 'workspace.enter';
      if (typeof contextId !== 'string' || !contextId.length) return fail(operation, 'invalid_request', { reason: 'contextId must be a non-empty string' });
      if (!tabsOk()) return fail(operation, 'unavailable', { reason: 'workspace_tab_unavailable' });
      const read = await readModel();
      if (!read.ok) return refail(operation, read);
      const contexts = read.model.contexts;
      if (!Array.isArray(contexts)) return fail(operation, 'unavailable', { reason: 'component_contexts_unavailable' });
      if (contextId === SCENE || !contexts.some((entry) => entry && entry.contextId === contextId)) return fail(operation, 'not_found', { reason: 'unknown_context' });
      let active;
      try { active = hooks.tabs.active(); } catch (_) { return fail(operation, 'unavailable', { reason: 'workspace_tab_unavailable' }); }
      if (active === contextId) return Object.freeze({ ok: true, operation, entered: false, reason: 'already_active', contextId });
      // Nested components are refused, as the baseline (and the native owner) refuse them: close the open one first.
      if (active !== SCENE) return fail(operation, 'unavailable', { reason: 'nested_context', message: 'Close the component first.' });
      try { await hooks.tabs.enter(contextId); } catch (error) { return fail(operation, 'workspace_failed', { message: String(error && error.message || error) }); }
      return Object.freeze({ ok: true, operation, entered: true, contextId, split: await splitOnEntry(contextId) });
    }
    // exitToScene: back to the scene root through the same tab strip.
    async function exit() {
      const operation = 'workspace.exit';
      if (!tabsOk()) return fail(operation, 'unavailable', { reason: 'workspace_tab_unavailable' });
      let active;
      try { active = hooks.tabs.active(); } catch (_) { return fail(operation, 'unavailable', { reason: 'workspace_tab_unavailable' }); }
      if (active === SCENE) return Object.freeze({ ok: true, operation, exited: false, reason: 'already_scene' });
      try { await hooks.tabs.exit(); } catch (error) { return fail(operation, 'workspace_failed', { message: String(error && error.message || error) }); }
      return Object.freeze({ ok: true, operation, exited: true, contextId: SCENE });
    }
    return Object.freeze({
      convert, convertLayer: (layerUid) => convert([layerUid]), place, expose,
      readModel, storyboard, enter, exit, read, listContexts, listLayers, listInstances, workspaceTransition, workspaceNow,
    });
  }

  const bindings = new WeakMap();
  // The presented desktop Project session -> intents. Null while the native Project session does not own the document.
  function desktop(host, hooks) {
    const project = host && host.NemoNativeObjectProject;
    const session = project && typeof project.desktop === 'function' ? project.desktop(host) : null;
    if (!session || typeof session.blocksLegacy !== 'function' || !session.blocksLegacy()) return null;
    if (!bindings.has(host)) bindings.set(host, createComponentIntent(core.portsFromSession(session), hooks));
    return bindings.get(host);
  }
  return Object.freeze({
    createComponentIntent, desktop, exposedKey,
    OPERATIONS: Object.freeze(['component.convert', 'component.place', 'component.expose'].map(OP)),
  });
}));
