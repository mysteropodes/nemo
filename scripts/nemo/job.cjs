#!/usr/bin/env node
'use strict';
// node scripts/nemo/job.cjs <job>[,<job>...] — run named jobs and emit a receipt.
const { parseArgs, runJobs } = require('./lib/cli.cjs');
const { JOBS } = require('./lib/jobs.cjs');
const args = parseArgs(process.argv.slice(2));
const names = args._.join(',').split(',').filter(Boolean);
if (!names.length) { console.error('usage: job.cjs <job>[,<job>...]\njobs: ' + Object.keys(JOBS).join(', ')); process.exit(64); }
for (const n of names) if (!JOBS[n]) { console.error(`unknown job "${n}". jobs: ${Object.keys(JOBS).join(', ')}`); process.exit(64); }
const { receipt } = runJobs(names.join(','), names, { json: args.json, quiet: args.quiet });
process.exit(receipt.summary.exitCode);
