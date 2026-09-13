// @ts-check
// Engine image-store budget (P26): the JS-side bookkeeping and least-
// recently-used eviction policy for images uploaded to the render engine.
// Moved from engine-bridge.js (its `_imgBytes` / `_imgLastUsed` /
// `_imgUsedThisBuild` block) with the policy unchanged:
//
//   * eviction is driven from THIS side, not the engine, because this side
//     knows what the scene being built actually references and can always
//     re-upload (CLAUDE.md §5quinquies);
//   * least-recently-USED — "used" meaning last emitted into a scene — never
//     touching anything the CURRENT build references, down to a byte budget;
//   * a freshly uploaded image counts as used, not merely registered,
//     otherwise it is an eviction candidate the instant it arrives.
//
// Lifecycle, driven by the scene builder: `beginBuild()` opens the
// current-build set, `touch(id)` / `noteRegistered(id, w, h)` record every
// use while the build is in flight, `endBuild(retire)` decides eviction with
// the complete set and closes it. `retire(ids)` is the caller's port to the
// engine; it returns true on success. On failure (or an exception) nothing
// is forgotten here, so the caller's own upload gate stays untouched too.
// On success `endBuild` returns the dropped ids so the caller can clear its
// gate and the next use re-uploads.
//
// This module owns no GPU state and reads no globals; bytes are the decoded
// RGBA size (w*h*4), exactly what the engine holds.
var NemoImageBudget = (function () {
  'use strict';

  function create(opts) {
    var bytes = new Map();      // id -> decoded RGBA bytes held on the GPU side
    var lastUsed = new Map();   // id -> build tick when last emitted
    var used = null;            // Set, non-null only during a scene build
    var tick = 0;
    var budget = setBudgetBytes(opts && opts.budgetBytes != null ? opts.budgetBytes : 384 * 1024 * 1024);
    var evictions = 0;

    // Math.floor, NOT `| 0`: bitwise coercion wraps at 2^31, so any budget
    // above ~2.1GB silently became a tiny number (a 4GB budget landed on 1
    // byte and evicted the whole store on the first frame).
    function setBudgetBytes(n) { budget = Math.max(1, Math.floor(n)); return budget; }

    function noteRegistered(id, w, h) {
      bytes.set(id, w * h * 4);
      touch(id);
    }
    function touch(id) {
      lastUsed.set(id, ++tick);
      if (used) used.add(id);
    }
    function totalBytes() {
      var t = 0;
      bytes.forEach(function (b) { t += b; });
      return t;
    }
    function beginBuild() { used = new Set(); }

    // Candidates are everything not drawn by the build that just ended,
    // oldest use first, dropped until the total fits the budget.
    function endBuild(retire) {
      var dropped = [];
      try {
        var total = totalBytes();
        if (total <= budget || typeof retire !== 'function') return dropped;
        var cands = [];
        bytes.forEach(function (b, id) {
          if (used && used.has(id)) return;   // on screen now
          cands.push({ id: id, t: lastUsed.get(id) || 0, b: b });
        });
        cands.sort(function (a, b) { return a.t - b.t; });
        var drop = [];
        for (var i = 0; i < cands.length && total > budget; i++) { drop.push(cands[i].id); total -= cands[i].b; }
        if (!drop.length) return dropped;
        var ok;
        try { ok = retire(drop.slice()) !== false; } catch (e) { ok = false; }
        if (!ok) return dropped;
        for (var k = 0; k < drop.length; k++) { bytes.delete(drop[k]); lastUsed.delete(drop[k]); }
        evictions += drop.length;
        dropped = drop;
        return dropped;
      } finally {
        used = null;
      }
    }

    function stats() { return { jsBytes: totalBytes(), budgetBytes: budget, evictions: evictions, count: bytes.size }; }

    return { noteRegistered: noteRegistered, touch: touch, totalBytes: totalBytes, beginBuild: beginBuild,
      endBuild: endBuild, stats: stats, setBudgetBytes: setBudgetBytes,
      has: function (id) { return bytes.has(id); }, building: function () { return used !== null; } };
  }

  return { create: create };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoImageBudget;
