/* U36 (#2362): the Labs generators (speed lines, boil, follow-path) through their native operations. The engine reads the
 * shown cell itself and runs the baseline algorithms, so a request carries parameters only. Holds no document state and
 * has no fallback writer: an absent project or route is a typed unavailability. */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const command = commonjs ? require('./native-object-rig-command.js') : root.NemoNativeObjectRigCommand;
  const contract = commonjs ? require('./native-object-rig-contract.js') : root.NemoNativeObjectRigContract;
  const api = factory(command, contract);
  if (commonjs) module.exports = api;
  else root.NemoNativeObjectLabs = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Command, Contract) {
  'use strict';
  // '#rgb', '#rrggbb' or '#rrggbbaa' as the record's 0..1 components; anything else is opaque black.
  function rgba(hex) {
    const m = /^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.exec(typeof hex === 'string' ? hex : '');
    if (!m) return [0, 0, 0, 1];
    const h = m[1].length === 3 ? m[1].replace(/./g, (c) => c + c) : m[1];
    const byte = (at) => parseInt(h.slice(at, at + 2), 16) / 255;
    return [byte(0), byte(2), byte(4), h.length === 8 ? byte(6) : 1];
  }
  function create(session) {
    const commands = Command.create(session, 'labs');
    const run = (operation, body) => commands.execute(operation, () => Object.assign({ command: Contract.commandOf(operation) }, body));
    return Object.freeze({
      layerUid: (index) => { const info = session.info(), layer = info && info.layers && info.layers[index]; return layer ? layer.layerUid : null; },
      speedLines: (body) => run(Contract.LABS_SPEED_LINES, body),
      boil: (body) => run(Contract.LABS_BOIL, body),
      followPath: (body) => run(Contract.LABS_FOLLOW_PATH, body),
    });
  }
  const bindings = new WeakMap();
  function desktop(host) {
    const module = host.NemoNativeObjectProject, session = module && module.desktop(host);
    if (!session || !session.blocksLegacy()) return null;
    if (!bindings.has(host)) bindings.set(host, create(session));
    return bindings.get(host);
  }
  return Object.freeze({ create, desktop, rgba });
}));
