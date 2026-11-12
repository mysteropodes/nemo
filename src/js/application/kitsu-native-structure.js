/* Kitsu shot open (#2381 I44): the Rough/Clean/Color layer structure on a native object project.
 * The baseline openShot renamed "Layer 1" to Rough and created Clean and Color through state.layers/createUserLayer. Here
 * the same three names are built only through the native entries the ordinary UI uses (NemoScriptNativeWrites: Drawing
 * Add -> layer.create, the lifecycle intent -> layer.rename) and then read back from the native projection. No state.layers
 * write exists on this path. A refused step throws with the step's name; nothing is rolled back and no unrelated project
 * is touched. Fixture: tests/kitsu-native-structure.test.cjs. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoKitsuNativeStructure = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var NAMES = ['Rough', 'Clean', 'Color'], TRIES = 40, WAIT_MS = 60;
  function sleep(root, ms) { return new Promise(function (done) { ((root && root.setTimeout) || setTimeout)(done, ms); }); }
  function fail(step, why) { return new Error('Structure des calques Kitsu : étape « ' + step + ' » ' + (why || 'refusée') + ' (projet natif)'); }
  // Nothing was sent when an entry answers busy or unavailable, so only those two are retried (bounded).
  async function step(root, label, run) {
    var last = null;
    for (var n = 0; n < TRIES; n++) {
      last = await run();
      if (last && last.ok === true) return last;
      if (!last || (last.code !== 'busy' && last.code !== 'unavailable')) break;
      await sleep(root, WAIT_MS);
    }
    throw fail(label, last && last.code ? 'refusée (' + last.code + ')' : 'refusée');
  }
  function layers(session) { var info = session.info(); return info && Array.isArray(info.layers) ? info.layers : []; }

  async function build(root) {
    var project = root.NemoNativeObjectProject, session = project && typeof project.desktop === 'function' ? project.desktop(root) : null;
    var writes = root.NemoScriptNativeWrites;
    if (!session || typeof session.isActive !== 'function' || session.isActive() !== true || !writes) throw fail('session', 'indisponible');
    var ops = writes.create(root, session);
    if (typeof ops.add !== 'function' || typeof ops.rename !== 'function') throw fail('ops', 'indisponible');
    var first = layers(session);
    if (first.length !== 1 || typeof first[0].layerUid !== 'string') throw fail('layer 1', 'introuvable');
    var rough = first[0].layerUid;
    await step(root, 'rename ' + NAMES[0], function () { return ops.rename(rough, NAMES[0]); });
    for (var i = 1; i < NAMES.length; i++) {
      var made = await step(root, 'create ' + NAMES[i], function () { return ops.add(); });
      if (typeof made.layerUid !== 'string') throw fail('create ' + NAMES[i], 'sans identifiant');
      await step(root, 'rename ' + NAMES[i], function () { return ops.rename(made.layerUid, NAMES[i]); });
    }
    // Confirm against the native read model, bounded: the receipt is reconciled before each step returns.
    for (var k = 0; k < TRIES; k++) {
      var rows = layers(session);
      if (rows.length === NAMES.length && NAMES.every(function (name, at) { return rows[at].name === name; })) {
        // The baseline leaves Rough (index 0) active; the selection is presentation only and best effort.
        try { if (typeof project.selectLayerUI === 'function') project.selectLayerUI(root, 0, false, rows[0].layerUid, session.info()); } catch (_) { }
        return { layers: rows.map(function (row) { return { layerUid: row.layerUid, name: row.name }; }) };
      }
      await sleep(root, WAIT_MS);
    }
    throw fail('confirm', 'non confirmée');
  }
  return Object.freeze({ build: build, NAMES: Object.freeze(NAMES.slice()) });
}));
