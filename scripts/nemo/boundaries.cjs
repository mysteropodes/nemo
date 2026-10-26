#!/usr/bin/env node
'use strict';
// Standalone CLI for the R05 module-profile/dependency rule checker
// Reuses the JavaScript checker or the normal CI adopted Rust crate policies,
// according to the declared source paths. See engineering/boundaries/README.md.
//
// Usage:
//   node scripts/nemo/boundaries.cjs <profile.json> [--baseline <prior-profile.json>]
//     [--root <dir>] [--json]
//   node scripts/nemo/boundaries.cjs --suggest [paths...] [--root <dir>] [--json]
//     Read-only: print the profile membership entry for each path, or for every
//     currently unowned Rust/JS source when no path is given. Never writes files.
//
// Exit codes: 0 = no violations, 1 = one or more checker/ratchet violations,
// 2 = bad usage or an input that could not be loaded/run.

const fs = require('node:fs');
const path = require('node:path');
const { checkProfile, validateProfile } = require('./lib/boundaries.cjs');
const { compareSizeBaseline } = require('./lib/boundaries-ratchet.cjs');
const { checkSourceSizes } = require('./lib/boundaries-size.cjs');
const { rustBoundaries } = require('./ci.cjs');
const { suggest, render } = require('./lib/boundaries-suggest.cjs');

function profileReport(profile, root) {
  validateProfile(profile);
  const files = profile.modules.flatMap((m) => m.files);
  const rustCount = files.filter((file) => /\.rs$/i.test(file)).length;
  if (!rustCount) return checkProfile(profile, { root });
  if (rustCount !== files.length) throw new Error('mixed Rust and non-Rust profiles are unsupported; check separate profiles');
  const size = checkSourceSizes(profile, { root });
  const rust = rustBoundaries(root, profile);
  const explicitlyUnanalyzed = new Set(rust.crates.flatMap((crate) => crate.unanalyzedModules));
  const sizeOnlyModules = profile.modules.filter((m) => explicitlyUnanalyzed.has(m.id)
    || !rust.crates.some((crate) => m.dir === crate.sourceDir || m.dir.startsWith(`${crate.sourceDir}/`)))
    .map((m) => m.id).sort();
  return { ...size, ok: size.ok && rust.ok, language: 'rust', moduleCount: profile.modules.length,
    violations: size.violations.concat(rust.crates.flatMap((crate) =>
      crate.violations.map((violation) => ({ ...violation, policy: crate.policy })))),
    exceptionsApplied: size.exceptionsApplied.concat(rust.crates.flatMap((crate) => crate.exceptionsApplied)),
    rust, scope: { graphPolicies: rust.crates.map((crate) => crate.policy), sizeOnlyModules,
      graphComplete: sizeOnlyModules.length === 0 } };
}

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

function suggestMain(argv) {
  const out = { paths: [], root: process.cwd(), json: false };
  try {
    for (let i = 0; i < argv.length; i++) {
      if (argv[i] === '--suggest') continue;
      if (argv[i] === '--json') out.json = true;
      else if (argv[i] === '--root') {
        if (!argv[i + 1] || argv[i + 1].startsWith('--')) throw new Error('--root requires a directory');
        out.root = argv[++i];
      } else if (argv[i].startsWith('-')) throw new Error(`unknown option: ${argv[i]}`);
      else out.paths.push(argv[i]);
    }
    const result = suggest(path.resolve(out.root), out.paths);
    console.log(out.json ? JSON.stringify(result, null, 2) : render(result));
    return 0;
  } catch (err) {
    console.error(`boundaries --suggest could not run: ${err.message}`);
    return 2;
  }
}

function main() {
  if (process.argv.slice(2).includes('--suggest')) return suggestMain(process.argv.slice(2));
  let args;
  try {
    args = parseArgs(process.argv.slice(2));
  } catch (err) {
    console.error(`bad usage: ${err.message}`);
    return 2;
  }
  if (!args.profilePath) {
    console.error('usage: node scripts/nemo/boundaries.cjs <profile.json> [--baseline <prior-profile.json>] [--root <dir>] [--json]');
    return 2;
  }

  let profile;
  try {
    profile = JSON.parse(fs.readFileSync(path.resolve(args.profilePath), 'utf8'));
  } catch (err) {
    console.error(`could not read/parse profile "${args.profilePath}": ${err.message}`);
    return 2;
  }

  let report;
  try {
    report = profileReport(profile, path.resolve(args.root));
  } catch (err) {
    console.error(`boundaries check could not run: ${err.message}`);
    return 2;
  }

  if (args.baselinePath) {
    let baseline;
    try {
      baseline = JSON.parse(fs.readFileSync(path.resolve(args.baselinePath), 'utf8'));
    } catch (err) {
      console.error(`could not read/parse baseline "${args.baselinePath}": ${err.message}`);
      return 2;
    }
    try {
      const ratchet = compareSizeBaseline(baseline, profile, { root: path.resolve(args.root) });
      report = { ...report, ok: report.ok && ratchet.ok, ratchet };
    } catch (err) {
      console.error(`baseline comparison could not run: ${err.message}`);
      return 2;
    }
  }

  if (args.json) {
    console.log(JSON.stringify(report, null, 2));
  } else {
    console.log(`boundaries: ${report.moduleCount} module(s), ${report.violations.length} violation(s), ${report.warnings.length} warning(s)`);
    if (report.language === 'rust') {
      console.log(`Rust graph scope: ${report.scope.graphPolicies.length} adopted crate policies; size checks cover all ${report.moduleCount} profile modules`);
      console.log(`size-only modules: ${report.scope.sizeOnlyModules.join(', ') || '(none)'}`);
    }
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
  return report.ok ? 0 : 1;
}

// Let pending stdout/stderr writes drain, including when either stream is piped.
process.exitCode = main();
