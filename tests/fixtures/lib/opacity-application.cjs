'use strict';
// Synthetic opacity-application fixture (T07): a small, fully deterministic
// isolated document + ports for NemoOpacityApplicationCore, seeded the same
// way tests/fixtures/lib/rng.cjs seeds the R03 corpus -- the same (id, seed)
// always produces the same instance/document ids and the same starting
// layer state, so a fixture is identified by {id, hash of its starting
// state} rather than by anything that could vary between runs or machines.
const crypto = require('node:crypto');
const path = require('node:path');
const core = require(path.join(__dirname, '../../../src/js/application/opacity-application.js'));
const domain = require(path.join(__dirname, '../../../src/js/domain/animation/opacity.js'));
const { mulberry32 } = require('./rng.cjs');

function fixtureHash(startingState) {
  return crypto.createHash('sha256').update(JSON.stringify(startingState)).digest('hex');
}
function deepClone(value) { return JSON.parse(JSON.stringify(value)); }

// id: a short fixture identifier (e.g. 'opacity-repro-a'). seed: a 32-bit
// integer. Returns {id, seed, hash, app, state, undo, redo} -- app is a
// live NemoOpacityApplicationCore bound to this isolated state, never the
// running application's own document.
function build(id, seed) {
  const rng = mulberry32(seed >>> 0);
  let sequence = 0;
  const startingState = {
    currentFrame: 0, totalFrames: 24,
    layers: [
      { layerUid: `${id}-layer-a`, motionStatic: { opacity: [rng.int(0, 100)] } },
      { layerUid: `${id}-layer-b`, motionStatic: { opacity: [rng.int(0, 100)] } },
    ],
  };
  const hash = fixtureHash(startingState);
  const state = deepClone(startingState);
  const undo = [], redo = [];
  function restore(from, to) {
    if (!from.length) return false;
    to.push(deepClone(state.layers)); state.layers = from.pop(); return true;
  }
  const app = core.create({
    newId: () => `${id}-${seed}-${++sequence}`, state: () => state, context: () => 'document',
    canMutate: () => true, snapshot: () => deepClone(state),
    valueAtFrame: layer => layer.motionStatic.opacity,
    write(operation, layer, payload) { domain.setValue(layer, [payload.value], state.currentFrame); },
    history: {
      checkpoint() { undo.push(deepClone(state.layers)); redo.length = 0; },
      undo: () => restore(undo, redo), redo: () => restore(redo, undo),
    },
    afterMutation() {},
  });
  return { id, seed, hash, app, state, undo, redo };
}

module.exports = { build, fixtureHash };
