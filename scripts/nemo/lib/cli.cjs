'use strict';
// Common runner: build a receipt, execute the requested jobs in order, write
// the receipt under reports/<runId>/ and exit with the overall status code
// (0 pass, 1 fail, 2 blocked). Every entry point goes through here so a
// single job run and a full verify produce the same receipt shape.
const receipt = require('./receipt.cjs');
const jobs = require('./jobs.cjs');

function parseArgs(argv) {
  const out = { _: [], json: false, quiet: false, profile: null, jobs: null };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--json') out.json = true;
    else if (a === '--quiet') out.quiet = true;
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
  if (opts.json) {
    process.stdout.write(require('node:fs').readFileSync(written.jsonPath, 'utf8'));
  } else if (!opts.quiet) {
    receipt.printSummary(r, written);
  }
  return { receipt: r, written };
}

module.exports = { parseArgs, runJobs };
