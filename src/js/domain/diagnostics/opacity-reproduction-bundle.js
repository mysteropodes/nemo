// Bounded reproduction-bundle codec (T07): packages a recorded opacity
// command sequence -- the same shape NemoOpacityDiagnostics.entries() (T05)
// returns -- into a portable, versioned bundle that can later be replayed
// against an ISOLATED document to reproduce the exact sequence. Never the
// live/active document: this module only builds and validates bundle data,
// it never touches application state or dispatches anything itself. A
// caller replays a parsed bundle's commands through its own isolated
// NemoOpacityApplicationCore instance, exactly the same "shape here,
// dispatch there" split T05's prepareReplay already uses.
var NemoOpacityReproductionBundle = (function () {
  'use strict';
  var FORMAT_VERSION = 1;
  var MAX_COMMANDS = 32;
  var MAX_SERIALIZED_CHARS = 65536;
  var MAX_IDENTIFIER_CHARS = 128;
  var PROPERTY_WRITES = ['property.set', 'property.key.set', 'property.key.remove', 'property.animation.set'];
  function clone(value) { return JSON.parse(JSON.stringify(value)); }
  function object(value) { return !!value && typeof value === 'object' && !Array.isArray(value); }
  function identifier(value) {
    return typeof value === 'string' && value.length > 0 && value.length <= MAX_IDENTIFIER_CHARS
      && /^[A-Za-z0-9_.:-]+$/.test(value);
  }
  function boundedClone(value) {
    var encoded = JSON.stringify(value);
    if (!encoded || encoded.length > MAX_SERIALIZED_CHARS) throw new Error('Bundle exceeds its size limit.');
    return JSON.parse(encoded);
  }
  function fixtureFor(value, strict) {
    if (!object(value) || !identifier(value.id) || !identifier(value.hash)
        || strict && Object.keys(value).some(function (key) { return key !== 'id' && key !== 'hash'; })) {
      throw new Error('fixture must be {id, hash}');
    }
    return { id: value.id, hash: value.hash };
  }
  function commandFor(operation, rawPayload, strict) {
    if (!PROPERTY_WRITES.includes(operation) || !object(rawPayload)) throw new Error('every entry must carry a well-formed request');
    var source = boundedClone(rawPayload);
    if (!object(source)) throw new Error('every entry must carry a well-formed request');
    var allowed = ['layerId', 'property'];
    var payload = { layerId: source.layerId, property: source.property };
    if (!identifier(source.layerId)
        || source.property !== 'opacity') throw new Error('every entry must carry a well-formed request');
    if (operation === 'property.set' || operation === 'property.key.set') {
      allowed.push('value'); payload.value = source.value;
      if (!Number.isFinite(source.value) || source.value < 0 || source.value > 100) {
        throw new Error('every entry must carry a well-formed request');
      }
    }
    if (operation === 'property.key.set' || operation === 'property.key.remove') {
      allowed.push('frame'); payload.frame = source.frame;
      if (!Number.isSafeInteger(source.frame) || source.frame < 0) throw new Error('every entry must carry a well-formed request');
    } else if (source.frame != null) {
      allowed.push('frame'); payload.frame = source.frame;
      if (!Number.isFinite(source.frame) || source.frame < 0) throw new Error('every entry must carry a well-formed request');
    }
    if (operation === 'property.animation.set') {
      allowed.push('animated'); payload.animated = source.animated;
      if (typeof source.animated !== 'boolean') throw new Error('every entry must carry a well-formed request');
    }
    if (strict && Object.keys(source).some(function (key) { return !allowed.includes(key); })) {
      throw new Error('every entry must carry a well-formed request');
    }
    return { operation: operation, payload: payload };
  }

  // fixture: {id, hash} identifying the starting document a bundle's
  // commands assume. entries: a recorded command trace, e.g.
  // NemoOpacityDiagnostics.entries(). meta: optional {clock, seed} for a
  // bundle whose command sequence actually depended on either -- absent
  // (null) for the plain deterministic property writes this covers today;
  // carried so a future non-deterministic diagnostics source has somewhere
  // to record what made it reproducible, without this codec inventing
  // clock/seed values that do not exist.
  function buildBundle(fixture, entries, meta) {
    var selectedFixture = fixtureFor(fixture, false);
    if (!Array.isArray(entries) || !entries.length || entries.length > MAX_COMMANDS) {
      throw new Error('entries must contain 1..32 commands');
    }
    var commands = entries.map(function (entry) {
      var request = entry && entry.request;
      if (!entry || entry.ok !== true || !object(request)) throw new Error('every entry must carry a successful property write');
      return commandFor(request.operation, request.payload, false);
    });
    return boundedClone({
      formatVersion: FORMAT_VERSION,
      fixture: selectedFixture,
      commands: commands,
      clock: (meta && meta.clock) != null ? clone(meta.clock) : null,
      seed: (meta && meta.seed) != null ? clone(meta.seed) : null,
    });
  }

  // Validates a bundle's shape and version WITHOUT touching any document.
  // Returns {fixture, commands, clock, seed} on success or {error} on
  // rejection -- never throws on malformed input, so a caller can safely try
  // an untrusted bundle before deciding whether to replay it at all.
  // clock/seed are returned rather than dropped: buildBundle records them so
  // a non-deterministic source can say what made its sequence reproducible,
  // and a replayer that never receives them would silently diverge from the
  // recording instead of failing. They stay opaque here -- this codec does
  // not interpret them, it only refuses to lose them.
  function parseBundle(bundle) {
    try {
      if (!object(bundle)) return { error: 'Bundle must be an object.' };
      if (bundle.formatVersion !== FORMAT_VERSION) return { error: 'Unsupported or missing bundle format version.' };
      if (!Array.isArray(bundle.commands) || !bundle.commands.length || bundle.commands.length > MAX_COMMANDS) {
        return { error: 'Bundle must contain 1..32 commands.' };
      }
      var detached = boundedClone(bundle);
      if (!object(detached) || Object.keys(detached).some(function (key) {
        return !['formatVersion', 'fixture', 'commands', 'clock', 'seed'].includes(key);
      })) return { error: 'Bundle contains unexpected fields.' };
      var fixture = fixtureFor(detached.fixture, true);
      if (!Array.isArray(detached.commands) || !detached.commands.length || detached.commands.length > MAX_COMMANDS) {
        return { error: 'Bundle must contain 1..32 commands.' };
      }
      var commands = detached.commands.map(function (command) {
        if (!object(command) || Object.keys(command).some(function (key) { return key !== 'operation' && key !== 'payload'; })) {
          throw new Error('Malformed command');
        }
        return commandFor(command.operation, command.payload, true);
      });
      return {
        fixture: fixture,
        commands: commands,
        clock: detached.clock != null ? clone(detached.clock) : null,
        seed: detached.seed != null ? clone(detached.seed) : null,
      };
    } catch (_) {
      return { error: 'Bundle contains malformed or non-serializable data.' };
    }
  }

  return { FORMAT_VERSION: FORMAT_VERSION, buildBundle: buildBundle, parseBundle: parseBundle };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoOpacityReproductionBundle;
