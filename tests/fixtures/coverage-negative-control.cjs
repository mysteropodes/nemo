'use strict';
// Deliberately unimported (T01/#1050). No test requires this file, and it is
// not product source (kept out of src/js and scripts/nemo on purpose — both
// have their own pinned boundary/inventory profiles that a new committed
// source file would break; this is a coverage-tooling fixture, not a module
// under architectural governance). It exists only to prove that `c8 --all`
// reports zero coverage for a real gap instead of silently omitting a file
// nothing happened to execute. See .c8rc.json's "exceptions" for how the
// coverage job excludes it from the pass/fail threshold while still
// reporting it. Do not import this from product code or a real test.
function coverageNegativeControlNeverCalled(flag) {
  if (flag) {
    return 'unreachable branch A';
  }
  return 'unreachable branch B';
}

module.exports = { coverageNegativeControlNeverCalled };
