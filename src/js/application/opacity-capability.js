// The opacity feature's own capability registration (P06/#1008).
//
// This is the feature side of the join P05 built: opacity declares itself and
// hands over its own handler, and the registry becomes the single place the
// application entry point looks the handler up.
//
// The runtime descriptor below is a SUBSET of
// engineering/application/capabilities/opacity.json — exactly the fields
// registration inspects (schemaVersion, id, handlerKey, availability, effects,
// and the presence of input/output). The JSON file stays the source of truth
// for the full contract; shipping it into src/ just to re-read it at runtime
// would put the same document in two places with no check that they agree.
// Instead tests/application-opacity-capability.test.cjs pins these fields
// against that file, so drift fails a test rather than going unnoticed.
//
// `effects.kind: "mutation"` / `scope: "document"` classify the capability AS A
// WHOLE — the union across its lifecycle — which is why they read "mutation"
// even though `property.get` in that same lifecycle is a read. See the note in
// capability-registry.js; D02's per-stage JobLifecycle is the per-stage view.
var NemoOpacityCapability = (function () {
  'use strict';

  var DESCRIPTOR = {
    schemaVersion: 1,
    id: 'opacity',
    version: 1,
    handlerKey: 'application.opacity.property',
    input: {}, output: {},
    units: { value: 'percent' },
    effects: {
      kind: 'mutation',
      scope: 'document',
      lifecycle: ['property.get', 'property.set', 'property.key.set', 'property.key.remove', 'property.animation.set'],
    },
    availability: { state: 'available', reason: null },
  };

  // Registering asserts the availability claim: `available` is refused unless
  // this handler is really bound (capability-registry.js). Opacity is the one
  // capability that can honestly claim it today, and this is where it proves
  // it rather than asserting it in a JSON file nobody checks.
  function register(registry, handler) {
    return registry.register(DESCRIPTOR, handler);
  }

  return { DESCRIPTOR: DESCRIPTOR, register: register };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoOpacityCapability;
