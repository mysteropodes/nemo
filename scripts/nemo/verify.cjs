#!/usr/bin/env node
'use strict';
// npm run verify [-- --profile quick|full | --jobs a,b,c] [--no-baseline] —
// the merge profile: runs the selected jobs and emits ONE machine-readable
// receipt, plus reports/<runId>/comparison.json classifying the same jobs
// against the adopted baseline (T02). That comparison is a separate result:
// raw job statuses and this exit code are unchanged by it.
// Exit: 0 pass, 1 any job failed, 2 a required job is blocked.
const { parseArgs, runJobs } = require('./lib/cli.cjs');
const { JOBS, PROFILES } = require('./lib/jobs.cjs');
const args = parseArgs(process.argv.slice(2));
let names;
if (args.jobs) names = args.jobs;
else names = PROFILES[args.profile || 'quick'];
if (!names) { console.error(`unknown profile "${args.profile}". profiles: ${Object.keys(PROFILES).join(', ')}`); process.exit(64); }
for (const n of names) if (!JOBS[n]) { console.error(`unknown job "${n}". jobs: ${Object.keys(JOBS).join(', ')}`); process.exit(64); }
const { receipt } = runJobs('verify', names, { profile: args.jobs ? 'custom' : (args.profile || 'quick'), json: args.json, quiet: args.quiet, baseline: args.baseline });
process.exit(receipt.summary.exitCode);
