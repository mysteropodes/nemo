#!/usr/bin/env node
'use strict';
// Standalone CLI for the R05 module-profile/dependency rule checker
// (scripts/nemo/lib/boundaries.cjs). Not wired into `npm run check`/`verify`
// or scripts/nemo/lib/jobs.cjs yet — see engineering/boundaries/README.md for
// why (R01/R03 need to land first) and the integration contract for when
// they do.
//
// Usage:
//   node scripts/nemo/boundaries.cjs <profile.json> [--baseline <prior-profile.json>]
//     [--root <dir>] [--json]
//
// Exit codes: 0 = no violations, 1 = one or more checker/ratchet violations,
// 2 = bad usage or an input that could not be loaded/run.

const fs = require('node:fs');
const path = require('node:path');
const { checkProfile } = require('./lib/boundaries.cjs');
const { compareSizeBaseline } = require('./lib/boundaries-ratchet.cjs');

function parseArgs(argv) {
  const out = { profilePath: null, baselinePath: null, root: process.cwd(), json: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--json') out.json = true;
    else if (a === '--baseline') {
      if (!argv[i + 1] || argv[i + 1].startsWith('--')) throw new Error('--baseline requires a prior profile');
      out.baselinePath = argv[++i];
    }
    else if (a === '--root') {
      if (!argv[i + 1] || argv[i + 1].startsWith('--')) throw new Error('--root requires a directory');
      out.root = argv[++i];
    } else if (a.startsWith('-')) throw new Error(`unknown option: ${a}`);
    else if (!out.profilePath) out.profilePath = a;
    else throw new Error(`unexpected argument: ${a}`);
  }
  return out;
}

function main() {
  let args;
  try {
    args = parseArgs(process.argv.slice(2));
  } catch (err) {
    console.error(`bad usage: ${err.message}`);
    process.exit(2);
  }
  if (!args.profilePath) {
    console.error('usage: node scripts/nemo/boundaries.cjs <profile.json> [--baseline <prior-profile.json>] [--root <dir>] [--json]');
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

  if (args.baselinePath) {
    let baseline;
    try {
      baseline = JSON.parse(fs.readFileSync(path.resolve(args.baselinePath), 'utf8'));
    } catch (err) {
      console.error(`could not read/parse baseline "${args.baselinePath}": ${err.message}`);
      process.exit(2);
    }
    try {
      const ratchet = compareSizeBaseline(baseline, profile, { root: path.resolve(args.root) });
      report = { ...report, ok: report.ok && ratchet.ok, ratchet };
    } catch (err) {
      console.error(`baseline comparison could not run: ${err.message}`);
      process.exit(2);
    }
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
    if (report.ratchet) {
      console.log(`ratchet: ${report.ratchet.violations.length} violation(s), ${report.ratchet.reductions.length} reduction(s), ${report.ratchet.removals.length} removal(s)`);
      for (const v of report.ratchet.violations) {
        console.log(`  FAIL  ${v.rule.padEnd(30)} ${v.file || v.detail?.sizeProfile || 'policy'} — ${v.message}`);
      }
      for (const reduction of report.ratchet.reductions) {
        console.log(`  INFO  ${'size-baseline-reduction'.padEnd(30)} ${reduction.file} — ceiling ${reduction.priorCeiling} -> ${reduction.candidateCeiling}, actual ${reduction.actualLines}`);
      }
      for (const removal of report.ratchet.removals) {
        console.log(`  INFO  ${'size-baseline-removal'.padEnd(30)} ${removal.file} — source absent; prior ceiling ${removal.priorCeiling}`);
      }
    }
  }
  process.exit(report.ok ? 0 : 1);
}

main();
