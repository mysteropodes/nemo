#!/usr/bin/env node
'use strict';
// Standalone CLI for the R05 module-profile/dependency rule checker
// (scripts/nemo/lib/boundaries.cjs). Not wired into `npm run check`/`verify`
// or scripts/nemo/lib/jobs.cjs yet — see engineering/boundaries/README.md for
// why (R01/R03 need to land first) and the integration contract for when
// they do.
//
// Usage:
//   node scripts/nemo/boundaries.cjs <profile.json> [--root <dir>] [--json]
//
// Exit codes: 0 = no violations, 1 = one or more violations, 2 = bad usage or
// a profile that could not be loaded/run (unknown sizeProfile, missing file).

const fs = require('node:fs');
const path = require('node:path');
const { checkProfile } = require('./lib/boundaries.cjs');

function parseArgs(argv) {
  const out = { profilePath: null, root: process.cwd(), json: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--json') out.json = true;
    else if (a === '--root') out.root = argv[++i];
    else if (!out.profilePath) out.profilePath = a;
  }
  return out;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (!args.profilePath) {
    console.error('usage: node scripts/nemo/boundaries.cjs <profile.json> [--root <dir>] [--json]');
    process.exit(2);
  }

  let profile;
  try {
    profile = JSON.parse(fs.readFileSync(path.resolve(args.profilePath), 'utf8'));
  } catch (err) {
    console.error(`could not read/parse profile "${args.profilePath}": ${err.message}`);
    process.exit(2);
  }

  let report;
  try {
    report = checkProfile(profile, { root: path.resolve(args.root) });
  } catch (err) {
    console.error(`boundaries check could not run: ${err.message}`);
    process.exit(2);
  }

  if (args.json) {
    console.log(JSON.stringify(report, null, 2));
  } else {
    console.log(`boundaries: ${report.moduleCount} module(s), ${report.violations.length} violation(s), ${report.warnings.length} warning(s)`);
    for (const v of report.violations) {
      console.log(`  FAIL  ${v.rule.padEnd(18)} ${v.file || v.module}${v.line ? `:${v.line}` : ''} — ${v.message}`);
    }
    for (const w of report.warnings) {
      console.log(`  WARN  ${w.rule.padEnd(18)} ${w.file || w.module}${w.line ? `:${w.line}` : ''} — ${w.message}`);
    }
  }
  process.exit(report.ok ? 0 : 1);
}

main();
