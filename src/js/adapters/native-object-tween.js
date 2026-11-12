/* Tween commands (D20-D22, #2358): generate, regenerate, split and reassign through their native operations
 * (`command.document.object.tween.*`, one sealed owner transaction and one history entry each). The engine reads the layer cells
 * itself, so a request carries the layer, the scope (start keys or selected frames), the panel options and, for reassign, the
 * span. A thin client of the native project session: no tween mirror, no Paper writer and no fallback; the legacy
 * generateTweens stays behind n20AllowLegacyWrite and is never called. An absent route, project or declaration is a typed
 * unavailability. The caller owns the panel semantics (native-object-tween-ui.js). */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const command = commonjs ? require('./native-object-rig-command.js') : root.NemoNativeObjectRigCommand;
  const contract = commonjs ? require('./native-object-rig-contract.js') : root.NemoNativeObjectRigContract;
  const api = factory(command, contract);
  if (commonjs) module.exports = api;
  else root.NemoNativeObjectTween = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Command, Contract) {
  'use strict';
  // The declared request budget of native.object.tween (capabilities-v2/native-object-tween.json): easing curves and
  // override lists do not fit the 4096 bytes of the small lane-3 commands.
  const LIMIT = 65536;
  const LABEL = 'tween';
  // `held()` (optional) answers the entered component, `{contextId, rows}`, or null at the scene root (#2358 B-comp-edit-rest): baseline
  // `enterSymbol` swapped `state.layers` for the component's, so Tween works on its layers. The request then names that context and rides the
  // component command route (one revision and one history entry, like the scene's).
  function create(session, held) {
    const commands = Command.create(session, LABEL, Contract, LIMIT);
    let inside = null;
    const componentCommands = () => {
      if (!inside && typeof session.runComponentCommand === 'function') {
        inside = Command.create(Object.freeze({ info: session.info, isActive: session.isActive, requestId: session.requestId, runResourceCommand: session.runComponentCommand.bind(session) }), LABEL, Contract, LIMIT);
      }
      return inside;
    };
    const here = () => { const h = typeof held === 'function' ? held() : null; return h && typeof h.contextId === 'string' ? h : null; };
    // Members the caller leaves undefined are absent from the wire: absent and explicit values stay distinct.
    const body = (name, layerUid, members, contextId) => {
      const payload = { command: 'object.tween.' + name, layerUid };
      if (contextId) payload.contextId = contextId;
      Object.keys(members || {}).forEach((key) => { if (members[key] !== undefined) payload[key] = members[key]; });
      return payload;
    };
    const send = (operation, name, layerUid, members) => {
      const at = here();
      if (!at) return commands.execute(operation, () => body(name, layerUid, members));
      const route = componentCommands();
      if (!route) return Promise.reject(Object.assign(new Error('Native component route is unavailable'), { code: 'unavailable', reason: 'component_route_unavailable' }));
      return route.execute(operation, () => {
        const now = here();
        if (!now || now.contextId !== at.contextId) throw Object.assign(new Error('The entered component changed'), { code: 'stale_origin', reason: 'origin_changed' });
        return body(name, layerUid, members, at.contextId);
      });
    };
    // scope: {} (every span), {starts: [...]} or {selection: [...]}; options: all four members or absent.
    const scoped = (operation, name) => (layerUid, scope, options) =>
      send(operation, name, layerUid, Object.assign({}, scope && { starts: scope.starts, selection: scope.selection }, { options }));
    return Object.freeze({
      layerUid: (index) => {
        const at = here();
        if (at) { const row = at.rows[index]; return row ? row.layerUid : null; }
        const info = session.info(), layer = info && info.layers && info.layers[index]; return layer ? layer.layerUid : null;
      },
      declared: (operation) => Contract.admitted(operation) && typeof (here() ? session.runComponentCommand : session.runResourceCommand) === 'function',
      generate: scoped(Contract.TWEEN_GENERATE, 'generate'),
      regenerate: scoped(Contract.TWEEN_REGENERATE, 'regenerate'),
      split: (layerUid, frame, options) => send(Contract.TWEEN_SPLIT, 'split', layerUid, { frame, options }),
      reassign: (layerUid, span, options) => send(Contract.TWEEN_REASSIGN, 'reassign', layerUid, { span, options }),
    });
  }
  // The entered component the workspace controller holds, with its layer rows; null at the scene root.
  function heldComponent(host) {
    try {
      const context = host.NemoNativeComponentContext;
      const id = context && typeof context.contextId === 'function' ? context.contextId(host) : 'scene-root';
      const rows = id !== 'scene-root' && typeof context.layers === 'function' ? context.layers(host) : null;
      return rows ? { contextId: id, rows } : null;
    } catch (_) { return null; }
  }
  const bindings = new WeakMap();
  function desktop(host) {
    const module = host.NemoNativeObjectProject, session = module && module.desktop(host);
    if (!session || !session.blocksLegacy()) return null;
    if (!bindings.has(host)) bindings.set(host, create(session, () => heldComponent(host)));
    return bindings.get(host);
  }
  return Object.freeze({ create, desktop, LIMIT });
}));
