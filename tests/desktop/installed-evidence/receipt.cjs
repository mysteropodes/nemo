'use strict';
// The receipt: what was sent, what came back (digests), every assertion with observed and expected values, a verdict per row
// and totals per lane. It says in words what it is not.
const SCHEMA = 'nemo.installed-client-evidence/1';
// Exit status: 0 only when every selected row passed; 1 when any row failed; 2 when the run was refused before any row;
// 4 when no row failed but at least one is unrun. (3 is the CS7 subset status and is deliberately not reused.)
const EXIT = Object.freeze({ pass: 0, fail: 1, refused: 2, unrun: 4 });

const STATEMENT = 'Agent evidence produced only through the package\'s bundled nemo-mcp (nemo_discover, nemo_command): no keyboard or mouse input was sent. ' +
  'It shows the native owner\'s contract for the operations listed per row on this candidate. It is not the CS7 batched installed session (#2348) and it does not ' +
  'record the installed/client acceptance disposition of any lane; that disposition belongs to the project owners.';
const RESIDUAL_HANDS_ON = Object.freeze([
  'Drawing-tool strokes, Add-tool shapes and gestures that reach the owner through the UI (not through nemo-mcp)',
  'Keyboard shortcuts for copy, cut and paste, and the group, flatten and layer buttons as an artist presses them',
  'How text looks at the viewport (preview versus export) and font selection through the font picker',
  'The export modal and video export',
]);

function totals(rows) {
  const lane = {}, all = { rows: 0, pass: 0, fail: 0, unrun: 0 };
  for (const row of rows) {
    const t = lane[row.lane] ||= { rows: 0, pass: 0, fail: 0, unrun: 0 };
    t.rows++; t[row.verdict]++; all.rows++; all[row.verdict]++;
  }
  return { overall: all, byLane: lane };
}
function verdictOf(rows, cleanupOk) {
  if (!cleanupOk || rows.some(r => r.verdict === 'fail')) return 'fail';
  return rows.some(r => r.verdict === 'unrun') ? 'unrun' : 'pass';
}
function exitStatus(verdict) { return EXIT[verdict] ?? EXIT.fail; }

function buildReceipt({ candidate, selection, rows, cleanup, now = new Date(), refusal = null }) {
  const cleanupOk = cleanup ? cleanup.ok === true : false;
  const verdict = refusal ? 'refused' : verdictOf(rows, cleanupOk);
  return {
    schema: SCHEMA, generatedAt: now.toISOString(), verdict, statement: STATEMENT,
    candidate: candidate || null, selection, cleanup: cleanup || { ok: false, reason: 'not started' },
    ...(refusal ? { refusal } : {}),
    totals: totals(rows), rows, residualHandsOn: RESIDUAL_HANDS_ON,
  };
}

module.exports = { SCHEMA, EXIT, STATEMENT, RESIDUAL_HANDS_ON, buildReceipt, totals, verdictOf, exitStatus };
