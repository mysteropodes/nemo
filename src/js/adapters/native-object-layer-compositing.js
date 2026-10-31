/* #2372: layer blend, track matte and effect stack over the native document. Every control of the baseline panels
 * (src/js/timeline.js blend and matte pickers, src/js/effects-panel.js stack @1065e9d8) becomes ONE native edit
 * (command.document.object.layer.compositing.edit): a blend or matte pick is one write, and an effect add / remove /
 * reorder / toggle sends the layer's COMPLETE resulting stack, computed at execution time from the pinned layer row so a
 * queued call never edits a stale stack. The adapter holds no document state, reads current values from the pinned
 * project info (blendMode, matteMode, matteSourceLayerUid, mattesMore, effects) and has no fallback writer: an absent
 * project, route or declaration is a typed unavailability. Hover previews are the caller's own UI state and never call this. */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const command = commonjs ? require('./native-object-rig-command.js') : root.NemoNativeObjectRigCommand;
  const contract = commonjs ? require('./native-object-rig-contract.js') : root.NemoNativeObjectRigContract;
  const api = factory(command, contract);
  if (commonjs) module.exports = api;
  else root.NemoNativeObjectLayerCompositing = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Command, Contract) {
  'use strict';
  const LIMIT = 4096, MAX_EFFECTS = 32;
  const BLEND = Object.freeze(['normal', 'multiply', 'screen', 'overlay', 'darken', 'lighten', 'colorDodge', 'colorBurn', 'hardLight', 'softLight', 'difference', 'exclusion', 'hue', 'saturation', 'color', 'luminosity']);
  const MATTE = Object.freeze(['none', 'alpha', 'alphaInverted', 'luma', 'lumaInverted']);
  const fault = (code, message, extra) => Command.fault(code, message, extra);
  const copy = (v) => JSON.parse(JSON.stringify(v));
  const text = (v) => typeof v === 'string' && v.length > 0;

  function create(session) {
    const commands = Command.create(session, 'compositing', Contract, LIMIT);
    const rows = (info) => (info && Array.isArray(info.layers) ? info.layers : []);
    const rowAt = (index) => rows(session.info())[index] || null;
    const uidOf = (index) => { const row = rowAt(index); return row && text(row.layerUid) ? row.layerUid : null; };
    const gone = () => fault('not_found', 'The layer is not in the native project', { reason: 'layer_missing' });
    const rowByUid = (pin, uid) => rows(pin).find((row) => row.layerUid === uid) || null;

    // One native edit of one layer; `members` is computed from the pinned row when the request is built.
    function edit(index, kind, members, finish) {
      const uid = uidOf(index);
      if (!uid) return Promise.reject(gone());
      return commands.execute(Contract.COMPOSITING_EDIT, (pin) => {
        const row = rowByUid(pin, uid);
        if (!row) throw gone();
        return Object.assign({ command: 'object.layer.compositing.edit', layerUid: uid, edit: kind },
          typeof members === 'function' ? members(row, rows(pin), pin) : members);
      }, finish);
    }
    // The stored stack of a layer row: [] when none, null when the row names it oversize (never guess at a stack).
    function stackOfRow(row) {
      if (!row) return null;
      if (row.effectsOversize === true) return null;
      return Array.isArray(row.effects) ? copy(row.effects) : [];
    }
    function stackEdit(index, mutate) {
      return edit(index, 'effects', (row) => {
        const stack = stackOfRow(row);
        if (!stack) throw fault('unavailable', 'This effect stack is too large to edit natively', { reason: 'effects_oversize' });
        const next = mutate(stack);
        if (!Array.isArray(next) || next.length > MAX_EFFECTS) throw fault('invalid_request', 'The effect stack is outside the native bound', { reason: 'effects_bound' });
        return { effects: next };
      });
    }
    return Object.freeze({
      layerUid: uidOf,
      // Current values as the pickers show them: an absent member is Normal / no matte.
      blendOf(index) { const row = rowAt(index); return row && text(row.blendMode) ? row.blendMode : 'normal'; },
      matteOf(index) { const row = rowAt(index); return row && text(row.matteMode) ? row.matteMode : 'none'; },
      sourceOf(index) { const row = rowAt(index); return row && text(row.matteSourceLayerUid) ? row.matteSourceLayerUid : null; },
      layerCount() { return rows(session.info()).length; },
      stackOf(index) { return stackOfRow(rowAt(index)); },
      stackAvailable(index) { return stackOfRow(rowAt(index)) !== null; },
      setBlend(index, mode) {
        if (!BLEND.includes(mode)) return Promise.reject(fault('invalid_request', 'Unknown blend mode', { reason: 'local_validation' }));
        return edit(index, 'blend', { mode });
      },
      // None removes the matte (and its additional mattes with it); any other pick sets the mode and, like the baseline
      // menu, freezes the adjacent layer as the source when none is stored yet, in the same write.
      setMatteMode(index, mode) {
        if (!MATTE.includes(mode)) return Promise.reject(fault('invalid_request', 'Unknown matte mode', { reason: 'local_validation' }));
        if (mode === 'none') return edit(index, 'matteClear', {});
        return edit(index, 'matteMode', (row, all) => {
          const members = { mode };
          const below = all[all.indexOf(row) + 1];
          if (!text(row.matteSourceLayerUid) && below && text(below.layerUid) && below.layerUid !== row.layerUid) members.source = below.layerUid;
          return members;
        });
      },
      setMatteSource(index, source) { return edit(index, 'matteSource', { source }); },
      addMatteMore(index, source, mode) { return edit(index, 'matteMoreAdd', mode ? { source, mode } : { source }); },
      setMatteMore(index, at, members) { return edit(index, 'matteMoreSet', Object.assign({ index: at }, members)); },
      removeMatteMore(index, at) { return edit(index, 'matteMoreRemove', { index: at }); },
      borrowEffects(index, source) { return edit(index, 'effectsFromSet', { source }); },
      clearBorrowedEffects(index) { return edit(index, 'effectsFromClear', {}); },
      setFolderCollapsed(index, collapsed) { return edit(index, 'folderCollapsed', { collapsed: collapsed === true }); },
      addEffect(index, effect) { return stackEdit(index, (stack) => stack.concat([copy(effect)])); },
      removeEffect(index, at) { return stackEdit(index, (stack) => { if (!(at >= 0 && at < stack.length)) throw fault('not_found', 'No such effect', { reason: 'effect_missing' }); stack.splice(at, 1); return stack; }); },
      toggleEffect(index, at) { return stackEdit(index, (stack) => { if (!stack[at]) throw fault('not_found', 'No such effect', { reason: 'effect_missing' }); stack[at].enabled = stack[at].enabled !== true; return stack; }); },
      moveEffect(index, from, to) {
        return stackEdit(index, (stack) => {
          if (!stack[from] || !(to >= 0 && to < stack.length)) throw fault('not_found', 'No such effect', { reason: 'effect_missing' });
          stack.splice(to, 0, stack.splice(from, 1)[0]);
          return stack;
        });
      },
      // A static parameter value (p1..p8) of one effect; keyed parameters have their own tracks and are not written here.
      setEffectParam(index, at, key, value) {
        if (!/^p[1-8]$/.test(key) || typeof value !== 'number' || !Number.isFinite(value)) return Promise.reject(fault('invalid_request', 'Invalid effect parameter', { reason: 'local_validation' }));
        return stackEdit(index, (stack) => { if (!stack[at]) throw fault('not_found', 'No such effect', { reason: 'effect_missing' }); stack[at][key] = value; return stack; });
      },
      setEffects(index, stack) { return stackEdit(index, () => copy(stack)); },
    });
  }
  const bindings = new WeakMap();
  function desktop(host) {
    const module = host.NemoNativeObjectProject, session = module && module.desktop(host);
    if (!session || !session.blocksLegacy()) return null;
    if (!bindings.has(host)) bindings.set(host, create(session));
    return bindings.get(host);
  }
  return Object.freeze({ create, desktop, BLEND, MATTE });
}));
