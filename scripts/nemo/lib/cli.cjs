'use strict';
// Common runner: build a receipt, execute the requested jobs in order, write
// the receipt under reports/<runId>/ and exit with the overall status code
// (0 pass, 1 fail, 2 blocked). Every entry point goes through here so a
// single job run and a full verify produce the same receipt shape.
//
// T02 — when a baseline manifest is adopted, the run also writes
// reports/<runId>/comparison.json: the same jobs classified against that
// baseline (new regression vs unchanged known failure vs no evidence). It is a
// SEPARATE result and never touches the run's own exit code or job statuses:
// a known failure still fails the run, it is only labelled as known. Opt out
// with --no-baseline.
const path = require('node:path');
const fs = require('node:fs');
const receipt = require('./receipt.cjs');
const jobs = require('./jobs.cjs');
// `./baseline.cjs` is required lazily, inside compareToBaseline. It is an
// optional add-on to a run and must never be able to stop one: the desktop
// harness fixtures copy a SUBSET of scripts/nemo/ into a temp tree, so a
// top-level require here made `job.cjs test:desktop` die with MODULE_NOT_FOUND
// in that fixture and report exit 1 instead of the blocked 2 it had measured.

function parseArgs(argv) {
  const out = { _: [], json: false, quiet: false, profile: null, jobs: null, baseline: true };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--json') out.json = true;
    else if (a === '--quiet') out.quiet = true;
    else if (a === '--no-baseline') out.baseline = false;
    else if (a === '--profile') out.profile = argv[++i];
    else if (a.startsWith('--profile=')) out.profile = a.slice(10);
    else if (a === '--jobs') out.jobs = argv[++i].split(',');
    else if (a.startsWith('--jobs=')) out.jobs = a.slice(7).split(',');
    else out._.push(a);
  }
  return out;
}

function runJobs(command, names, opts = {}) {
  const r = receipt.create(command, { profile: opts.profile || null });
  const ctx = { receipt: r, reportDir: receipt.reportDir(r) };
  // doctor first when present so later jobs can read capabilities
  const ordered = names.includes('doctor') ? ['doctor'].concat(names.filter((n) => n !== 'doctor')) : names;
  for (const n of ordered) {
    if (!opts.quiet && !opts.json) process.stdout.write(`… ${n}\n`);
    jobs.execute(n, ctx);
  }
  receipt.finalize(r);
  const written = receipt.write(r);
  const comparison = opts.baseline === false ? null : compareToBaseline(r, ctx.reportDir, written.jsonPath, ordered);
  if (opts.json) {
    // --json owns stdout: the receipt is the payload. The comparison is still
    // written to its file, so nothing is lost by not printing it here.
    process.stdout.write(fs.readFileSync(written.jsonPath, 'utf8'));
  } else if (!opts.quiet) {
    receipt.printSummary(r, written);
    printComparison(comparison);
  }
  return { receipt: r, written, comparison };
}

// Classify this run against the adopted baseline and write the result next to
// the receipt. Returns { path, comparison } | { error } | null.
//
// Failures here are reported, never swallowed and never fatal: a comparator
// problem must not fail a run whose jobs actually passed, and must not be
// mistaken for "compared clean" either.
function compareToBaseline(r, reportDir, receiptPath, expect) {
  let baseline, manifest;
  try {
    baseline = require('./baseline.cjs');
    manifest = baseline.loadManifest();
  } catch (e) {
    return { error: e.message, adopted: false };
  }
  try {
    // `expect` is what this run was asked to run, so a targeted job run is not
    // judged for the baseline entries it never claimed to cover.
    const cmp = baseline.compare(manifest, r, { receiptPath, expect });
    const out = path.join(reportDir, 'comparison.json');
    fs.mkdirSync(reportDir, { recursive: true });
    fs.writeFileSync(out, JSON.stringify(cmp, null, 2) + '\n');
    return { path: out, comparison: cmp, adopted: true };
  } catch (e) {
    return { error: e.message, adopted: true };
  }
}

function printComparison(res) {
  if (!res) return;
  if (res.error) {
    console.log(res.adopted
      ? `baseline: comparison failed — ${res.error}`
      : `baseline: none adopted — ${res.error}`);
    return;
  }
  const s = res.comparison.summary;
  console.log(`baseline ${s.overall.toUpperCase()} (informational, exit unchanged)  `
    + Object.entries(s.counts).map(([k, v]) => `${k} ${v}`).join(', '));
  for (const b of s.blocking) console.log(`  blocking      ${b.job}${b.required ? ' (required)' : ''}: ${b.verdict}${b.note ? ' — ' + b.note : ''}`);
  for (const n of s.noEvidence || []) console.log(`  no evidence   ${n.job} (required): ${n.status}${n.reason ? ' — ' + n.reason : ''}`);
  // Without this an INCONCLUSIVE line has no visible cause: a moved tree is
  // the most common reason and says nothing about the jobs themselves.
  if (s.staleReferences && s.staleReferences.length) {
    console.log(`  stale         baseline adopted against different bytes: ${s.staleReferences.join(', ')}`);
  }
  console.log(`  ${path.relative(process.cwd(), res.path)}`);
}

module.exports = { parseArgs, runJobs, compareToBaseline };
