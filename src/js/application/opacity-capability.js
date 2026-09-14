// The opacity feature's own capability registration (P06/#1008).
//
// This is the feature side of the join P05 built: opacity declares itself and
// hands over its own handler, and the registry becomes the single place the
// application entry point looks the handler up.
//
// This synchronous classic-script projection is kept byte-for-value equivalent
// to engineering/application/capabilities/opacity.json by the focused contract
// test. The JSON file is canonical; the runtime copy exists because application
// boot cannot fetch a descriptor asynchronously before native MCP discovery.
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
    input: {
      type: 'object',
      additionalProperties: false,
      properties: {
        layerId: { type: 'string', minLength: 1, description: 'layers[].layerUid from the latest snapshot.' },
        value: { type: 'number', minimum: 0, maximum: 100 },
        frame: { type: 'integer', minimum: 0 },
        animated: { type: 'boolean' },
      },
      required: ['layerId'],
    },
    output: {
      type: 'object',
      additionalProperties: false,
      properties: {
        layerId: { type: 'string' },
        property: { const: 'opacity' },
        value: { type: 'number', minimum: 0, maximum: 100 },
      },
      required: ['layerId', 'property', 'value'],
    },
    units: { value: 'percent' },
    effects: {
      kind: 'mutation',
      scope: 'document',
      lifecycle: ['property.get', 'property.set', 'property.key.set', 'property.key.remove', 'property.animation.set'],
    },
    availability: { state: 'available', reason: null },
    fixture: {
      label: 'set opacity to 40 on one layer',
      input: { layerId: 'layer-1', value: 40 },
      output: { layerId: 'layer-1', property: 'opacity', value: 40 },
    },
    examples: [
      {
        label: 'set opacity to 40 on one layer',
        input: { layerId: 'layer-1', value: 40 },
        output: { layerId: 'layer-1', property: 'opacity', value: 40 },
      },
      {
        label: 'key opacity at frame 12',
        input: { layerId: 'layer-1', value: 40, frame: 12 },
        output: { layerId: 'layer-1', property: 'opacity', value: 40 },
      },
      {
        label: 'enable animation on the property',
        input: { layerId: 'layer-1', animated: true },
        output: { layerId: 'layer-1', property: 'opacity', value: 40 },
      },
    ],
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
