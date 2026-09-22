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
  function clone(value) { return JSON.parse(JSON.stringify(value)); }
  function object(value) { return !!value && typeof value === 'object' && !Array.isArray(value); }

  // fixture: {id, hash} identifying the starting document a bundle's
  // commands assume. entries: a recorded command trace, e.g.
  // NemoOpacityDiagnostics.entries(). meta: optional {clock, seed} for a
  // bundle whose command sequence actually depended on either -- absent
  // (null) for the plain deterministic property writes this covers today;
  // carried so a future non-deterministic diagnostics source has somewhere
  // to record what made it reproducible, without this codec inventing
  // clock/seed values that do not exist.
  function buildBundle(fixture, entries, meta) {
    if (!object(fixture) || typeof fixture.id !== 'string' || !fixture.id
        || typeof fixture.hash !== 'string' || !fixture.hash) {
      throw new Error('fixture must be {id, hash}');
    }
    if (!Array.isArray(entries) || !entries.length) throw new Error('entries must be a nonempty array');
    var commands = entries.map(function (entry) {
      var request = entry && entry.request;
      if (!object(request) || typeof request.operation !== 'string' || !object(request.payload)) {
        throw new Error('every entry must carry a well-formed request');
      }
      return { operation: request.operation, payload: clone(request.payload) };
    });
    return {
      formatVersion: FORMAT_VERSION,
      fixture: { id: fixture.id, hash: fixture.hash },
      commands: commands,
      clock: (meta && meta.clock) != null ? meta.clock : null,
      seed: (meta && meta.seed) != null ? meta.seed : null,
    };
  }

  // Validates a bundle's shape and version WITHOUT touching any document.
  // Returns {fixture, commands} on success or {error} on rejection -- never
  // throws on malformed input, so a caller can safely try an untrusted
  // bundle before deciding whether to replay it at all.
  function parseBundle(bundle) {
    if (!object(bundle)) return { error: 'Bundle must be an object.' };
    if (bundle.formatVersion !== FORMAT_VERSION) return { error: 'Unsupported or missing bundle format version.' };
    if (!object(bundle.fixture) || typeof bundle.fixture.id !== 'string' || !bundle.fixture.id
        || typeof bundle.fixture.hash !== 'string' || !bundle.fixture.hash) {
      return { error: 'Bundle fixture is missing or malformed.' };
    }
    if (!Array.isArray(bundle.commands) || !bundle.commands.length) return { error: 'Bundle must contain at least one command.' };
    for (var i = 0; i < bundle.commands.length; i++) {
      var c = bundle.commands[i];
      if (!object(c) || typeof c.operation !== 'string' || !c.operation || !object(c.payload)) {
        return { error: 'Bundle command ' + i + ' is malformed.' };
      }
    }
    return { fixture: clone(bundle.fixture), commands: clone(bundle.commands) };
  }

  return { FORMAT_VERSION: FORMAT_VERSION, buildBundle: buildBundle, parseBundle: parseBundle };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoOpacityReproductionBundle;
