'use strict';
// One original Text20 execution can supply the provider's installed clauses and
// its own later row. Cache per actual batch session, including rejected work:
// never restart/replay after failure and never reuse across candidate lifetimes.
const { isDeepStrictEqual: same } = require('node:util');
const { requireCheck: check } = require('../native-harness.cjs');
const sessions = new WeakMap();
const PHASES = require('./text20-phases.json').map(p => p.name);
const ORIGINAL = { id: 'text20', issue: 2267, checks: [{ id: 'original-text20' }] };
function sharedWorkflow(workflow, qualification) {
  return { async run({ session, row }) {
    check(row.issue === 2267 && same(row.checks.map(c => c.id), ['original-text20']), 'original Text20 row required');
    let entry = sessions.get(session);
    if (!entry) {
      entry = { qualification: structuredClone(qualification) };
      sessions.set(session, entry);
      entry.result = Promise.resolve().then(() => workflow.run({ session, row: structuredClone(ORIGINAL) })).then(result => {
        let terminal;
        return { ...result, ...(result.finalize ? { finalize: () => {
          terminal ||= Promise.resolve().then(() => result.finalize());
          return terminal;
        } } : {}) };
      });
    }
    check(same(entry.qualification, qualification), 'shared Text20 qualification differs');
    return entry.result;
  } };
}
const GROUPS = {
  'text-edit': ['point-create', 'point-edit', 'undo-edit', 'redo-edit', 'edit-reopened', 'undo-reopened', 'redo-reopened'],
  'font-bind': ['font-import', 'font-bind', 'undo-font', 'redo-font'],
  'reopen': ['save-as', 'replace-initial', 'reopen-saved', 'save-after', 'stale-read'],
};
function targetWorkflow(original) {
  return { async run({ session, row }) {
    check(row.issue === 2264 && same(row.checks.map(c => c.id), Object.keys(GROUPS)), 'original Text target clauses changed');
    const result = await original.run({ session, row: structuredClone(ORIGINAL) });
    check(typeof result.finalize === 'function', 'original Text20 cleanup finalizer required');
    const phases = result.phaseResults;
    check(Array.isArray(phases) && same(phases.map(p => p.phase), PHASES) &&
      phases.every(p => ['pass', 'fail', 'unrun'].includes(p.status)), 'original Text20 phase evidence missing');
    const mapped = Object.entries(GROUPS).map(([id, names]) => {
      const observations = names.map(name => phases.find(p => p.phase === name));
      check(observations.every(Boolean), 'original Text target phase missing');
      return { id, status: observations.some(p => p.status === 'fail') ? 'fail' :
        observations.every(p => p.status === 'pass') ? 'pass' : 'unrun' };
    });
    return { checks: mapped.map(c => ({ ...c, status: c.status === 'pass' ? 'unrun' : c.status })),
      safeToContinue: result.safeToContinue,
      sharedResults: [{ rowId: ORIGINAL.id, result }],
      finalize: async () => {
        const terminal = await result.finalize();
        check(terminal.length === 1 && terminal[0].id === 'original-text20' &&
          terminal[0].status === (result.safeToContinue ? 'pass' : 'fail'), 'original Text20 terminal evidence refused');
        return mapped;
      } };
  } };
}
module.exports = { sharedWorkflow, targetWorkflow, GROUPS };
