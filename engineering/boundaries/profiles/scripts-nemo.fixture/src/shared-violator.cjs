'use strict';
// Retained fixture: `tooling-shared` (allowedLayers: []) reaching
// `tooling-adapters` — the actually-forbidden edge under this profile's real
// layerRules (not the "application reaching adapters" edge those rules
// allow).
require('./adapter-target.cjs');
module.exports = { name: 'shared-violator' };
