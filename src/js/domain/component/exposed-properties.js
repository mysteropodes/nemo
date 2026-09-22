/* Component exposed-property mutation, with all dependencies supplied per call. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoComponentExposedProperties = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

// Component exposed properties (2026-08-18) — declares a property on the
// SYMBOL (state.symbols[symId].exposedProps), bound to one stroke inside it
// by its stable strokeId. Every instance of this symbol then gets it as an
// ordinary Motion property (propsFor, motion.js — registers PROP_LABEL/DIM/
// UNIT/DEFAULT there, not here, so a project reload rehydrates it the first
// time propsFor is called rather than needing a separate boot-time pass).
// `targetField` is a real field name on a serialized stroke dict
// ('fillColor','strokeColor','opacity'...) EXCEPT the sentinel '__visible',
// which getEffectiveStrokes reads as "drop this stroke from the instance's
// output entirely" rather than writing it onto the dict.
  function expose(symbols, symId, targetStrokeId, targetField, label, defaultVal, clock, random, metadata) {
    var sym = symbols[symId];
    if (!sym) return null;
    if (!sym.exposedProps) sym.exposedProps = [];
    var key = 'ep_' + clock().toString(36) + '_' + random().toString(36).slice(2, 6);
    var entry = { key: key, label: label, targetStrokeId: targetStrokeId,
      targetField: targetField, default: defaultVal };
    sym.exposedProps.push(entry);
    if (metadata) metadata(key, label, defaultVal);
    return entry;
  }

  // Exposed-property overrides (2026-08-18, see exposeSymbolProperty's own
  // comment) — resolved LAST, after every transform above, since these
  // touch paint/visibility fields a matrix never does. `ld` (the INSTANCE
  // layer, not the symbol) is what carries each property's actual value —
  // SMMotion.valueAtFrame already knows how to fall back through
  // animated→static→PROP_DEFAULT for any prop key, exposed ones included,
  // so this reuses that evaluator directly rather than re-deriving a
  // default here. Re-registers metadata every call (cheap, idempotent) —
  // see propsFor's identical reasoning for why this can't be a one-time
  // registration at exposure time alone.
  function applyOverrides(sym, out, ld, frameIdx, available, metadata, read) {
    if (!sym.exposedProps || !sym.exposedProps.length || !available()) return out;
    sym.exposedProps.forEach(function (ep) { metadata(ep); });
    var epByStroke = {};
    sym.exposedProps.forEach(function (ep) {
      (epByStroke[ep.targetStrokeId] = epByStroke[ep.targetStrokeId] || []).push(ep);
    });
    return out.map(function (sd) {
      if (!sd.strokeId || !epByStroke[sd.strokeId]) return sd;
      var sd2 = null, hide = false;
      epByStroke[sd.strokeId].forEach(function (ep) {
        var val = read(ld, ep.key, frameIdx)[0];
        if (ep.targetField === '__visible') { if (val < 50) hide = true; return; }
        if (!sd2) sd2 = JSON.parse(JSON.stringify(sd));
        if (ep.targetField === 'opacity') sd2.opacity = Math.max(0, Math.min(1, val / 100));
        else sd2[ep.targetField] = val;
      });
      return hide ? null : (sd2 || sd);
    }).filter(function (sd) { return sd; });
  }

  return Object.freeze({ expose: expose, applyOverrides: applyOverrides });
}));
