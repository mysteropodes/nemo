/* G39 (#2376): Motion presets over native tracks. Capture reads the selected layer's transform properties through the
 * ordinary native property read (stored facts, authored keys, raw sample), in the baseline preset shape
 * ({motion:{prop:{keys}}, motionStatic:{prop:[v]}} of motion-preset-picker.js @1065e9d8); apply writes every property through ONE
 * native command, hence one history entry that undo restores exactly. Holds no document state, reads nothing from the legacy
 * layer model and has no fallback writer. Every layer kind (null, folder, effect, 3D, image, text, component) owns all five
 * transform properties, so none is skipped per kind; only names outside PROPS are skipped, and the toast lists them: an absent project or route is a typed unavailability. */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const command = commonjs ? require('./native-object-rig-command.js') : root.NemoNativeObjectRigCommand;
  const contract = commonjs ? require('./native-object-rig-contract.js') : root.NemoNativeObjectRigContract;
  const api = factory(command, contract);
  if (commonjs) module.exports = api;
  else root.NemoNativeObjectMotionPreset = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Command, Contract) {
  'use strict';
  const PROPS = Object.freeze(['position', 'anchor', 'rotation', 'scale', 'opacity']);
  const LIMIT = 65536, ROOT_CONTEXT = 'scene-root', PAGE = 8, MAX_PAGES = 64;
  const KEY_FIELDS = ['frame', 'v', 'hold', 'lockTo', 'curvePoints', 'hIn', 'hOut'];
  const fault = (code, message, extra) => Command.fault(code, message, extra);
  const plain = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
  const copy = (v) => JSON.parse(JSON.stringify(v));

  // A key as the native contract admits it: the known members only, null members dropped.
  function cleanKey(key) {
    const out = {};
    KEY_FIELDS.forEach((field) => { if (plain(key) && key[field] !== undefined && key[field] !== null) out[field] = copy(key[field]); });
    return out;
  }
  // Properties of a stored preset the native transform does not own (order, 3D rows, expression controls): named, never written.
  function skipped(preset) {
    const names = new Set();
    [preset && preset.motion, preset && preset.motionStatic].forEach((table) => {
      if (plain(table)) Object.keys(table).forEach((name) => { if (!PROPS.includes(name)) names.add(name); });
    });
    return Array.from(names).sort();
  }
  // The apply payload rows: every native transform property is listed, so one the preset does not hold is cleared like the
  // baseline's whole-object replacement dropped it.
  function rows(preset) {
    const motion = plain(preset && preset.motion) ? preset.motion : {}, fixed = plain(preset && preset.motionStatic) ? preset.motionStatic : {};
    return PROPS.map((property) => {
      const track = motion[property], keys = plain(track) && Array.isArray(track.keys) ? track.keys.map(cleanKey) : [];
      const row = { property, keys };
      const value = fixed[property];
      if (Array.isArray(value) && value.length) row.static = copy(value);
      else if (typeof value === 'number') row.static = [value];
      return row;
    });
  }

  function create(session) {
    const commands = Command.create(session, 'preset', Contract, LIMIT);
    const layerUid = (index) => { const info = session.info(), layer = info && info.layers && info.layers[index]; return layer ? layer.layerUid : null; };
    const layerName = (index) => { const info = session.info(), layer = info && info.layers && info.layers[index]; return layer && typeof layer.name === 'string' ? layer.name : null; };
    function read(uid, view) {
      const pin = session.info();
      if (!pin || typeof session.readProperty !== 'function') throw fault('unavailable', 'Native property read is unavailable', { reason: 'no_native_project' });
      return session.readProperty({ instanceId: pin.instanceId, documentId: pin.documentId, atRevision: pin.contentRevision,
        documentSnapshotId: pin.documentSnapshotId, view: Object.assign({ target: { contextId: ROOT_CONTEXT, layerUid: uid } }, view) });
    }
    async function keysOf(uid, property) {
      const keys = [];
      for (let page = 0; page < MAX_PAGES; page += 1) {
        const result = await read(uid, { kind: 'authoredKeys', property, offset: keys.length, limit: PAGE });
        result.items.forEach((key) => keys.push(cleanKey(key)));
        if (result.nextOffset === null || !result.items.length) return keys;
      }
      throw fault('unavailable', 'Native key track is too long to capture', { reason: 'track_too_long' });
    }
    async function captureOnce(index, frame) {
      const uid = layerUid(index);
      if (!uid) throw fault('not_found', 'The layer is not in the native project', { reason: 'layer_missing' });
      const motion = {}, motionStatic = {};
      for (const property of PROPS) {
        const stored = await read(uid, { kind: 'storedProperty', property });
        const facts = stored.items[0] && stored.items[0].authored;
        if (!facts || facts.availability !== 'available') continue;
        if (facts.keyCount > 0) motion[property] = { keys: await keysOf(uid, property) };
        else if (facts.hasStaticOverride) motionStatic[property] = copy((await read(uid, { kind: 'rawSample', property, frame })).value);
      }
      return { motion: Object.keys(motion).length ? motion : null, motionStatic: Object.keys(motionStatic).length ? motionStatic : null };
    }
    return Object.freeze({
      layerUid, layerName,
      // A revision published between two reads makes the second refuse; the whole capture is read again once, never mixed.
      async capture(index, frame) {
        try { return await captureOnce(index, frame); } catch (first) {
          if (first && typeof first.code === 'string' && first.code !== 'unavailable') throw first;
          return captureOnce(index, frame);
        }
      },
      apply(index, preset) {
        const uid = layerUid(index);
        if (!uid) return Promise.reject(fault('not_found', 'The layer is not in the native project', { reason: 'layer_missing' }));
        const left = skipped(preset);
        return commands.execute(Contract.PRESET_APPLY, () => ({ command: 'object.property.preset.apply',
          target: { contextId: ROOT_CONTEXT, layerUid: uid }, properties: rows(preset) }), (receipt) => Object.freeze(Object.assign({ skipped: left }, receipt)));
      },
      skipped,
    });
  }
  const bindings = new WeakMap();
  function desktop(host) {
    const module = host.NemoNativeObjectProject, session = module && module.desktop(host);
    if (!session || !session.blocksLegacy()) return null;
    if (!bindings.has(host)) bindings.set(host, create(session));
    return bindings.get(host);
  }
  return Object.freeze({ create, desktop, rows, skipped, PROPS });
}));
