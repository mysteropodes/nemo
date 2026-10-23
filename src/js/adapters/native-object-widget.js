/* Joystick / slider widgets as native commands (lane 3, F19): create the widget layer with its axis controls, write a drag
 * through the existing control property edits, and edit the presentation settings. Three owner transactions
 * (`command.document.object.widget.create | write | settings`). A widget has no value of its own: its axes name ordinary layer
 * controls, so the drag mapping (local / world with anchor and angle / rest) is the owner's, from the immutable snapshot.
 * Hit-testing and the overlay stay in rig-widget.js (document-capture pointer listener, presentation only); no JS value
 * writer, no `state.layers` mirror and no fallback exists here. */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const command = commonjs ? require('./native-object-rig-command.js') : root.NemoNativeObjectRigCommand;
  const contract = commonjs ? require('./native-object-rig-contract.js') : root.NemoNativeObjectRigContract;
  const api = factory(command, contract);
  if (commonjs) module.exports = api;
  else root.NemoNativeObjectWidget = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Command, Contract) {
  'use strict';
  // rig-widget.js addWidgetLayer at the baseline: any kind but 'slider' (and 'joystick') is a joystick, and the default layer
  // name is the next layer name with its leading "Layer" replaced by the kind.
  function kindOf(kind) { return kind !== 'joystick' && kind !== 'slider' ? 'joystick' : kind; }
  function layerName(kind, nextName) {
    return (typeof nextName === 'string' ? nextName : 'Layer').replace(/^Layer/, kindOf(kind) === 'slider' ? 'Slider' : 'Joystick');
  }
  function refusal(message, reason) { return Promise.reject(Command.fault('invalid_request', message, { reason })); }

  function create(session, ports) {
    ports = ports || {};
    const commands = Command.create(session, 'widget');
    function createWidget(input) {
      const spec = input || {};
      const kind = kindOf(spec.kind);
      let pos = spec.pos;
      if (pos === undefined && typeof ports.center === 'function') pos = ports.center();
      if (pos === undefined) return refusal('A widget needs a position', 'pos_required');
      const name = spec.name === undefined ? layerName(kind, typeof ports.nextName === 'function' ? ports.nextName() : undefined) : spec.name;
      return commands.execute(Contract.WIDGET_CREATE, () => ({ command: 'object.widget.create', kind, name, pos }));
    }
    const write = (layerUid, frame, to) => commands.execute(Contract.WIDGET_WRITE, () => ({ command: 'object.widget.write', layerUid, frame, to }));
    return Object.freeze({
      create: createWidget,
      write,
      writeLocal: (layerUid, frame, point) => write(layerUid, frame, { kind: 'local', point }),
      writeWorld: (layerUid, frame, point, anchor, angleDegrees) => write(layerUid, frame, { kind: 'world', point, anchor, angleDegrees }),
      writeRest: (layerUid, frame) => write(layerUid, frame, { kind: 'rest' }),
      settings: (layerUid, set) => commands.execute(Contract.WIDGET_SETTINGS, () => ({ command: 'object.widget.settings', layerUid, set })),
    });
  }
  const bindings = new WeakMap();
  function desktop(host) {
    const module = host.NemoNativeObjectProject, session = module && module.desktop(host);
    if (!session || !session.blocksLegacy()) return null;
    if (!bindings.has(host)) bindings.set(host, create(session, {}));
    return bindings.get(host);
  }
  return Object.freeze({ create, desktop, kindOf, layerName });
}));
