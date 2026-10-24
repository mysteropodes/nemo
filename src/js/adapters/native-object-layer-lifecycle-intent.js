/* L02-J layer lifecycle, selection-edit and clipboard intents over the single native application client.
 * Each method is one closed typed request pinned to the presented revision. reorderLayersAtGap, duplicateLayer,
 * deleteLayer, splitLayerIntoElements and the Paper clipboard are never called: there is no JS fallback writer.
 * The layer selection is presentation state keyed by stable native layerUid and is rebuilt from the post-revision
 * projection after every applied command, so an id the owner no longer lists never survives. */
(function (root, factory) {
  const core = typeof module === 'object' && module.exports ? require('./native-object-edit-intent-core.js') : root.NemoNativeObjectEditIntent;
  const api = factory(core);
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectLayerLifecycleIntent = api;
}(typeof globalThis === 'object' ? globalThis : this, function (core) {
  'use strict';

  const PREFIX = 'command.document.object.';
  const SHAPE = {
    deleteShapes: ['selection.delete', 'selection-delete'], duplicateShapes: ['selection.duplicate', 'selection-duplicate'],
    splitLayer: ['layer.split', 'layer-split'], pasteShapes: ['clipboard.paste', 'clipboard-paste'],
    copyShapes: ['clipboard.copy', 'clipboard-copy'], cutShapes: ['clipboard.cut', 'clipboard-cut'],
  };
  const has = (value, key) => !!value && typeof value === 'object' && Object.prototype.hasOwnProperty.call(value, key);

  // ports: pin() with layers[{layerUid}], declared(operation), submit(request, pin), requestId(label), optional strokeIds(target).
  function createLayerLifecycleIntent(ports) {
    const runner = core.createIntentRunner(ports);
    const editor = core.createShapeEditor(ports, runner);
    let held = { active: null, selected: [], anchor: null };
    const fail = core.failure;

    function layerIds(pin) {
      return pin && Array.isArray(pin.layers) && pin.layers.every((layer) => layer && typeof layer.layerUid === 'string') ? pin.layers.map((layer) => layer.layerUid) : null;
    }
    function currentIds() { try { return layerIds(ports.pin()); } catch (_) { return null; } }
    function wire(selection) {
      const body = { active: selection.active, selected: selection.selected.slice() };
      if (typeof selection.anchor === 'string') body.anchor = selection.anchor;
      return body;
    }
    function rebuild(previous, beforeIds, afterIds) {
      if (!afterIds) return { active: null, selected: [], anchor: null };
      const selected = core.reconcileIds(previous.selected, afterIds);
      let active = previous.active;
      if (!afterIds.includes(active)) {
        const slot = Math.max(0, beforeIds ? beforeIds.indexOf(previous.active) : 0);
        active = afterIds.length ? afterIds[Math.min(slot, afterIds.length - 1)] : null;
      }
      return { active, selected, anchor: afterIds.includes(previous.anchor) ? previous.anchor : null };
    }
    function select(next) {
      const ids = currentIds();
      if (!ids) return fail('select', 'unavailable', { reason: 'no_layer_projection' });
      const named = [next && next.active].concat(next && Array.isArray(next.selected) ? next.selected : [], next && typeof next.anchor === 'string' ? [next.anchor] : []);
      if (!next || typeof next.active !== 'string' || !Array.isArray(next.selected)) return fail('select', 'invalid_request', { reason: 'selection needs an active layer and a selected list' });
      const gone = core.stale(named, ids);
      if (gone.length) return fail('select', 'stale_selection', { ids: gone });
      held = { active: next.active, selected: core.reconcileIds(next.selected, ids), anchor: typeof next.anchor === 'string' ? next.anchor : null };
      return Object.freeze({ ok: true, operation: 'select', selection: Object.freeze(Object.assign({}, held)) });
    }
    // Runs inside the runner's turn: the stale-id check, the default selection (`held`) and the pin all belong to the
    // state the predecessor left, so an un-awaited second intent is judged against the post-predecessor projection.
    function layerEdit(suffix, label, input, targets, needsSelection) {
      const operation = PREFIX + suffix;
      return runner.exclusive(async (exec) => {
        let declared = false;
        try { declared = ports.declared(operation) === true; } catch (_) { declared = false; }
        const before = declared ? currentIds() : null;
        let body = input && typeof input === 'object' && !Array.isArray(input) ? input : {};
        if (needsSelection) {
          const given = has(body, 'selection') ? body.selection : null;
          const selection = given || (held.active !== null ? wire(held) : null);
          if (!selection) return fail(operation, 'invalid_request', { reason: 'no layer selection to edit' });
          body = Object.assign({}, body, { selection });
        }
        if (before) {
          const named = [].concat(targets(body), body.selection ? [body.selection.active].concat(body.selection.selected || [], typeof body.selection.anchor === 'string' ? [body.selection.anchor] : []) : []);
          const gone = core.stale(named.filter((id) => typeof id === 'string' && id.length), before);
          if (gone.length) return fail(operation, 'stale_selection', { ids: gone });
        }
        const outcome = await exec(operation, body, label);
        if (!outcome.ok || !outcome.applied) return outcome;
        const previous = held, after = outcome.synchronized ? layerIds(outcome.pin) : null;
        held = rebuild(previous, before, after);
        return Object.freeze(Object.assign({}, outcome, { selection: Object.freeze(Object.assign({}, held)), selectionRebuilt: !!after }));
      });
    }
    const api = {
      selectLayers: select,
      layerSelection: () => Object.freeze(Object.assign({}, held)),
      deleteLayers: (input) => layerEdit('layer.delete', 'layer-delete', input || {}, () => [], true),
      duplicateLayer: (input) => layerEdit('layer.duplicate', 'layer-duplicate', input, (b) => [b.layerUid], false),
      renameLayer: (input) => layerEdit('layer.rename', 'layer-rename', input, (b) => [b.layerUid], false),
      reorderLayers: (input) => layerEdit('layer.reorder', 'layer-reorder', input, (b) => (Array.isArray(b.layerUids) ? b.layerUids : []), true),
      selectShapes: editor.select,
      shapeSelection: editor.selection,
    };
    Object.keys(SHAPE).forEach((name) => { api[name] = (input) => editor.edit(PREFIX + SHAPE[name][0], input, SHAPE[name][1]); });
    return Object.freeze(api);
  }
  return Object.freeze({ createLayerLifecycleIntent, OPERATIONS: Object.freeze([
    'layer.delete', 'layer.duplicate', 'layer.rename', 'layer.reorder', 'selection.delete', 'selection.duplicate', 'layer.split', 'clipboard.paste', 'clipboard.copy', 'clipboard.cut',
  ].map((suffix) => PREFIX + suffix)) });
}));
