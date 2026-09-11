'use strict';
// P02/T02 — how one job's result is DESCRIBED (its exact case) and JUDGED
// (its verdict and severity), separated from baseline.cjs so the manifest
// module stays about adoption, references and comparison shape.
//
// Two rules drive every decision here:
//
//   1. `blocked` and `not-run` are never rounded up to `pass`. A native,
//      packaged-desktop or browser check that could not genuinely run keeps
//      its own disposition and its exact reason.
//   2. Severity is a function of the verdict AND of whether the job is
//      required. The same verdict means "nothing to answer for" on an optional
//      job and "no evidence for something that must answer" on a required one.
const { ROOT, sha256Text } = require('./util.cjs');

// Comparison verdicts. The three P02 requires are `new-regression`,
// `unchanged-known-failure` and `environment-mismatch`; the rest exist so that
// none of those three has to absorb a case it does not actually describe.
const VERDICT = {
  PASS: 'pass',
  RESOLVED: 'resolved',
  UNCHANGED_KNOWN_FAILURE: 'unchanged-known-failure',
  CHANGED_KNOWN_FAILURE: 'changed-known-failure',
  NEW_REGRESSION: 'new-regression',
  NEWLY_OBSERVED_FAILURE: 'newly-observed-failure',
  ENVIRONMENT_MISMATCH: 'environment-mismatch',
  UNCHANGED_BLOCKED: 'unchanged-blocked',
  UNCHANGED_NOT_RUN: 'unchanged-not-run',
  UNKNOWN_ENTRY: 'unknown-entry',
  MISSING_ENTRY: 'missing-entry',
};

// Verdicts that must stop an integration. `changed-known-failure` is here on
// purpose: a failure that moved to a different case is a different failure, and
// letting it inherit the old one's acceptance is how a regression hides.
const BLOCKING = new Set([VERDICT.NEW_REGRESSION, VERDICT.CHANGED_KNOWN_FAILURE, VERDICT.NEWLY_OBSERVED_FAILURE]);
// Verdicts that mean "this run cannot answer the question", not "this run is bad".
const INCONCLUSIVE = new Set([VERDICT.ENVIRONMENT_MISMATCH, VERDICT.UNKNOWN_ENTRY, VERDICT.MISSING_ENTRY]);

// T02 — a verdict alone cannot decide severity; whether the job is REQUIRED
// decides it too, and P02 shipped without that half.
//
// A required baseline entry the run did not cover is not "uncomparable", it is
// the run failing to answer the question it exists to answer: an entry silently
// dropped from a profile would otherwise turn a required case into exit 0.
const REQUIRED_BLOCKING = new Set([VERDICT.MISSING_ENTRY]);
// A required job that produced no result at all — blocked on a missing runtime,
// or with nothing defined to run — is unchanged and known, but it is still no
// evidence. `unchanged-blocked` sat outside both sets, so `test:desktop`
// (required, blocked on a missing packaged app since the baseline was adopted)
// reported "nothing to answer for" with exit 0. Missing native/desktop evidence
// must never read as passing; it stays inconclusive until a run produces one.
// The same verdicts on an OPTIONAL job are genuinely nothing to answer for.
const REQUIRED_INCONCLUSIVE = new Set([VERDICT.UNCHANGED_BLOCKED, VERDICT.UNCHANGED_NOT_RUN]);

// Order matters: a required `missing-entry` blocks rather than falling through
// to the inconclusive set it also belongs to.
function severity(verdict, required) {
  if (BLOCKING.has(verdict)) return 'blocking';
  if (required && REQUIRED_BLOCKING.has(verdict)) return 'blocking';
  if (INCONCLUSIVE.has(verdict)) return 'inconclusive';
  if (required && REQUIRED_INCONCLUSIVE.has(verdict)) return 'inconclusive';
  return 'ok';
}

const EXIT = { ok: 0, blocking: 1, inconclusive: 2 };
// Receipts live under the gitignored reports/ directory and may quote absolute
// paths; the manifest is committed source and must not. Checkout and home
// directory are replaced by stable placeholders, which also makes a case
// signature comparable across machines and worktrees.
const HOME = require('node:os').homedir();
function redact(value) {
  if (typeof value === 'string') {
    let s = value.split(ROOT).join('<repo>');
    if (HOME && HOME !== '/') s = s.split(HOME).join('<home>');
    return s;
  }
  if (Array.isArray(value)) return value.map(redact);
  if (value && typeof value === 'object') {
    const out = {};
    for (const k of Object.keys(value)) out[k] = redact(value[k]);
    return out;
  }
  return value;
}

// The exact case a non-pass entry is about. Kept verbatim from the receipt
// (modulo path redaction) so a later run compares against what was actually
// observed, not a paraphrase.
function exactCase(job) {
  return redact({
    reason: job.reason || null,
    exitCode: job.exitCode ?? null,
    limitations: (job.limitations || []).slice(),
    details: job.details === undefined ? null : job.details,
  });
}

// A stable signature for "is this the same failure?". Reason text is the
// receipt's own identification of the case; details are included because two
// runs can share a reason string while failing different specifics.
function caseSignature(entryCase) {
  if (!entryCase) return null;
  return sha256Text(JSON.stringify({ reason: entryCase.reason ?? null, details: entryCase.details ?? null }));
}

function classify(entry, job) {
  if (!entry) return { verdict: VERDICT.UNKNOWN_ENTRY, note: 'No baseline entry for this job; it cannot be judged new or known.' };
  if (!job) return { verdict: VERDICT.MISSING_ENTRY, note: 'Baseline entry not covered by this run.' };

  const b = entry.status, c = job.status;

  if (c === 'pass') {
    return b === 'pass'
      ? { verdict: VERDICT.PASS, note: null }
      : { verdict: VERDICT.RESOLVED, note: `Was ${b} in the baseline.` };
  }

  if (c === 'fail') {
    if (b === 'fail') {
      const same = entry.caseSignature && entry.caseSignature === caseSignature(exactCase(job));
      return same
        ? { verdict: VERDICT.UNCHANGED_KNOWN_FAILURE, note: entry.case.reason || null }
        : { verdict: VERDICT.CHANGED_KNOWN_FAILURE, note: `Still failing, but not the recorded case. Baseline: ${entry.case.reason || 'n/a'} — now: ${job.reason || 'n/a'}` };
    }
    if (b === 'pass') return { verdict: VERDICT.NEW_REGRESSION, note: 'Passed in the baseline.' };
    return { verdict: VERDICT.NEWLY_OBSERVED_FAILURE, note: `Baseline was ${b}, so this failure was not previously observed.` };
  }

  // c is blocked or not-run: no result was produced.
  if (c === b) {
    return {
      verdict: c === 'blocked' ? VERDICT.UNCHANGED_BLOCKED : VERDICT.UNCHANGED_NOT_RUN,
      note: entry.case.reason || null,
    };
  }
  return {
    verdict: VERDICT.ENVIRONMENT_MISMATCH,
    note: `Baseline ${b}, this run ${c} — this run produced no result for it (${job.reason || 'no reason recorded'}).`,
  };
}

module.exports = { VERDICT, BLOCKING, INCONCLUSIVE, REQUIRED_BLOCKING, REQUIRED_INCONCLUSIVE, EXIT, severity, redact, exactCase, caseSignature, classify };
