/* Native script undo groups (CS8-SDK #2354, I33).
 * Owner: Ilya/Claude clean-slate session. Fixture: tests/script-native-groups.test.cjs.
 * One successful script run is ONE undo group. The native owner records one history entry per changed command, so a run that
 * drained N commands leaves N entries; the baseline wrapped them in one undo snapshot. The owner already moves N entries as one
 * restore (`object.undo` / `object.redo` with `steps`), so the group is a count, kept here and applied by the ordinary UI undo.
 *   record(identity, { start, end, steps })   a drain moved the document from revision `start` to `end` by `steps` history entries
 *   plan(action, pin)  -> steps               what the UI undo/redo at `pin` should move: a recorded group's size, else 1
 *   moved(action, before, after, steps)       the UI move succeeded: advance the chain or drop it
 * A chain is the groups that follow each other with nothing between them (each group starts at the revision the one before ended
 * on), and the revision the head is at. A group is applied ONLY when the pin is exactly that revision: every other write, undo or
 * redo moves the revision, so a stale chain is dropped and the UI falls back to one entry per undo. The failure direction is
 * therefore always the ungrouped one, never an undo of entries the run did not make. Nothing here touches the document.
 */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoScriptNativeGroups = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var KEYS = ['instanceId', 'documentId', 'lifecycleGeneration'];
  function count(n) { return Number.isSafeInteger(n) && n >= 1; }
  function revision(n) { return Number.isSafeInteger(n) && n >= 0; }
  function same(a, b) { return !!a && !!b && KEYS.every(function (k) { return a[k] === b[k]; }); }
  function create() {
    var chain = null; // { id, groups: [{ start, end, steps }], pos, at }: `pos` groups are applied, the head sits at revision `at`
    function mine(pin) { return chain && same(chain.id, pin) && pin.contentRevision === chain.at ? chain : null; }
    // The group the move would cross, or null (then the move is one ordinary entry).
    function next(action, pin) {
      var c = mine(pin);
      if (!c) { if (chain && same(chain.id, pin)) chain = null; return null; }
      if (action === 'undo') return c.pos >= 1 ? c.groups[c.pos - 1] : null;
      return c.pos < c.groups.length ? c.groups[c.pos] : null;
    }
    return Object.freeze({
      record: function (identity, run) {
        if (!identity || !run || !revision(run.start) || !revision(run.end) || !count(run.steps)) return false;
        var id = {}; KEYS.forEach(function (k) { id[k] = identity[k]; });
        var group = { start: run.start, end: run.end, steps: run.steps };
        if (chain && same(chain.id, id) && chain.at === group.start) {
          chain.groups.length = chain.pos; // a new write clears what was undone, as the owner clears its redo stack
          chain.groups.push(group); chain.pos = chain.groups.length; chain.at = group.end;
        } else chain = { id: id, groups: [group], pos: 1, at: group.end };
        return true;
      },
      plan: function (action, pin) {
        var group = pin && (action === 'undo' || action === 'redo') ? next(action, pin) : null;
        return group ? group.steps : 1;
      },
      moved: function (action, before, after, steps) {
        var group = before && after && (action === 'undo' || action === 'redo') ? next(action, before) : null;
        if (!group || group.steps !== steps || !same(before, after) || !revision(after.contentRevision)) { chain = null; return false; }
        chain.pos += action === 'undo' ? -1 : 1; chain.at = after.contentRevision;
        return true;
      },
      forget: function () { chain = null; },
      depth: function () { return chain ? chain.groups.length : 0; },
    });
  }
  return Object.freeze({ create: create, shared: create() });
}));
