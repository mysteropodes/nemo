'use strict';
// The receipt: what was sent, what came back (digests), every assertion with observed and expected values, a verdict per row
// and totals per lane. It says in words what it is not.
const { coverageTable } = require('./coverage.cjs');
const SCHEMA = 'nemo.installed-client-evidence/2';
// Exit status: 0 only when EVERY catalog row ran and passed; 1 when any row failed; 2 when the run was refused before any row;
// 4 when no row failed but at least one is unrun; 5 when every selected row passed but the selection left catalog rows out
// (`subset-pass`: a one-row receipt is not a lane receipt). (3 is the CS7 subset status and is deliberately not reused.)
const EXIT = Object.freeze({ pass: 0, fail: 1, refused: 2, unrun: 4, 'subset-pass': 5 });

const STATEMENT = 'Agent evidence produced only through the package\'s bundled nemo-mcp (nemo_discover, nemo_command): no keyboard or mouse input was sent. ' +
  'It shows the native owner\'s contract for the operations listed per row on this candidate. It is not the CS7 batched installed session (#2348) and it does not ' +
  'record the installed/client acceptance disposition of any lane; that disposition belongs to the project owners. Lane totals count the rows that ran: ' +
  '`coverage` lists, per lane, each assigned id as covered (a passing row observes it), attempted-not-passed (only failing rows) or never attempted (no row ran for it), and the behaviours the lane text requires that no row covers; `totals.retries` counts every operation that was refused and repeated; `subset-pass` marks a run that left rows out.';
const RESIDUAL_HANDS_ON = Object.freeze([
  'Drawing-tool strokes, Add-tool shapes and gestures that reach the owner through the UI (not through nemo-mcp)',
  'Keyboard shortcuts for copy, cut and paste, and the group, flatten and layer buttons as an artist presses them',
  'How text looks at the viewport (preview versus export) and font selection through the font picker',
  'The export modal and video export',
  'Every behaviour listed under coverage.notCovered for a lane (no row observes it yet)',
]);

// Retries are reported next to the verdicts, never inside the pass count: a row that passed only after an operation was refused and
// repeated counts as passed AND as retried, and the totals say how many operations were refused and repeated, how many rows it touched,
// and whether any retry ran out of attempts.
const emptyRetries = () => ({ retriedOperations: 0, refusedAttempts: 0, exhausted: 0, rows: 0, byOperation: {} });
function addRetries(t, row) {
  const list = Array.isArray(row.retries) ? row.retries : [];
  if (list.length === 0) return;
  t.retries.rows++;
  for (const r of list) {
    t.retries.retriedOperations++; t.retries.refusedAttempts += r.refused; if (r.accepted !== true) t.retries.exhausted++;
    const o = t.retries.byOperation[r.operation] ||= { retriedOperations: 0, refusedAttempts: 0 };
    o.retriedOperations++; o.refusedAttempts += r.refused;
  }
}
function totals(rows, catalogRows = {}) {
  const lane = {}, all = { rows: 0, pass: 0, fail: 0, unrun: 0, retries: emptyRetries() };
  for (const row of rows) {
    const t = lane[row.lane] ||= { rows: 0, pass: 0, fail: 0, unrun: 0, retries: emptyRetries() };
    t.rows++; t[row.verdict]++; all.rows++; all[row.verdict]++;
    addRetries(t, row); addRetries(all, row);
  }
  for (const key of Object.keys(lane)) { lane[key].catalogRows = Number.isInteger(catalogRows[key]) ? catalogRows[key] : null; lane[key].complete = lane[key].catalogRows === lane[key].rows; }
  return { overall: all, byLane: lane };
}
function verdictOf(rows, cleanupOk, omitted = 0) {
  if (!cleanupOk || rows.some(r => r.verdict === 'fail')) return 'fail';
  if (rows.some(r => r.verdict === 'unrun')) return 'unrun';
  return omitted > 0 ? 'subset-pass' : 'pass';
}
function exitStatus(verdict) { return EXIT[verdict] ?? EXIT.fail; }

function buildReceipt({ candidate, selection, rows, cleanup, now = new Date(), refusal = null }) {
  const cleanupOk = cleanup ? cleanup.ok === true : false;
  const omitted = selection && Array.isArray(selection.omitted) ? selection.omitted.length : 0;
  const verdict = refusal ? 'refused' : verdictOf(rows, cleanupOk, omitted);
  return {
    schema: SCHEMA, generatedAt: now.toISOString(), verdict, statement: STATEMENT,
    candidate: candidate || null, selection, cleanup: cleanup || { ok: false, reason: 'not started' },
    ...(refusal ? { refusal } : {}),
    totals: totals(rows, (selection && selection.catalogRows) || {}), coverage: coverageTable(rows, [...new Set(rows.map(r => String(r.lane)))]),
    rows, residualHandsOn: RESIDUAL_HANDS_ON,
  };
}

module.exports = { SCHEMA, EXIT, STATEMENT, RESIDUAL_HANDS_ON, buildReceipt, totals, verdictOf, exitStatus };
