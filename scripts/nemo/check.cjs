#!/usr/bin/env node
'use strict';
// npm run check — static integrity: version sync, JSON validity, JS syntax,
// index.html script references, private-labs guard, committed artifacts.
const { parseArgs, runJobs } = require('./lib/cli.cjs');
const args = parseArgs(process.argv.slice(2));
const { receipt } = runJobs('check', ['check'], { json: args.json, quiet: args.quiet });
if (!args.json && !args.quiet) {
  const j = receipt.jobs.find((x) => x.name === 'check');
  console.log('');
  for (const [k, v] of Object.entries(j.details || {})) console.log(`  ${v.status.toUpperCase().padEnd(8)} ${k.padEnd(20)} ${v.reason}`);
  console.log('');
}
process.exit(receipt.summary.exitCode);
