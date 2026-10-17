/* L02-J group and boolean intents over the single native application client. Thin: each method is one closed
 * typed request (command.document.object.group.* / boolean.*) pinned to the presented revision. No JS fallback
 * writer exists here: group-bridge.js, foldBooleanOp and the Paper/WASM boolean paths are never called. */
(function (root, factory) {
  const core = typeof module === 'object' && module.exports ? require('./native-object-edit-intent-core.js') : root.NemoNativeObjectEditIntent;
  const api = factory(core);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectGroupIntent = api;
}(typeof globalThis === 'object' ? globalThis : this, function (core) {
  'use strict';

  const PREFIX = 'command.document.object.';
  // method -> [operation suffix, request id label]
  const METHODS = {
    group: ['group.create', 'group-create'], ungroup: ['group.ungroup', 'group-ungroup'],
    combine: ['group.combine', 'group-combine'], setMode: ['group.mode.set', 'group-mode'],
    rename: ['group.rename', 'group-rename'], removeMember: ['group.member.remove', 'group-exit'],
    booleanApply: ['boolean.apply', 'boolean-apply'], booleanErase: ['boolean.erase', 'boolean-erase'],
    flatten: ['boolean.flatten', 'boolean-flatten'],
  };

  // ports: pin(), declared(operation), submit(request, pin), requestId(label), optional strokeIds(target).
  function createGroupIntent(ports) {
    const runner = core.createIntentRunner(ports);
    const editor = core.createShapeEditor(ports, runner);
    const api = {};
    Object.keys(METHODS).forEach((name) => {
      const [suffix, label] = METHODS[name];
      api[name] = (input) => editor.edit(PREFIX + suffix, input, label);
    });
    api.select = editor.select;
    api.selection = editor.selection;
    return Object.freeze(api);
  }
  return Object.freeze({ createGroupIntent, OPERATIONS: Object.freeze(Object.values(METHODS).map((entry) => PREFIX + entry[0])) });
}));
