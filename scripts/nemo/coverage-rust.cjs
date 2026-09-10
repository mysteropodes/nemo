#!/usr/bin/env node
'use strict';
// CLI for the T04 Rust coverage ratchet (nemo-mcp).
//
//   node scripts/nemo/coverage-rust.cjs            # check against the baseline
//   node scripts/nemo/coverage-rust.cjs --update   # re-record the baseline
//   node scripts/nemo/coverage-rust.cjs --json     # machine-readable verdict
//
// Exit codes: 0 = every reviewed floor held, 1 = ratchet violation or failing
// suite, 2 = bad usage or a missing tool/baseline (blocked). Blocked is never
// reported as success.
//
// `--update` rewrites the committed baseline from a fresh measurement. It is a
// reviewed action, not routine maintenance: raising a floor is how coverage
// improvements are locked in, and LOWERING one is a deliberate admission that
// needs its reason in the pull request.

const fs = require('node:fs');
const path = require('node:path');
const { ROOT, run, exists, readJson } = require('./lib/util.cjs');
const { evaluateRustCoverage, buildBaseline } = require('./lib/coverage-rust.cjs');
const { produceReports, listSourceFiles, SOURCE_ROOT, MANIFEST, BASELINE } = require('./lib/coverage-rust-job.cjs');

function parseArgs(argv) {
  const out = { update: false, json: false, outDir: null };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--update') out.update = true;
    else if (a === '--check') out.update = false;
    else if (a === '--json') out.json = true;
    else if (a === '--out') {
      if (!argv[i + 1] || argv[i + 1].startsWith('--')) throw new Error('--out requires a directory');
      out.outDir = argv[++i];
    } else throw new Error(`unknown option: ${a}`);
  }
  return out;
}

function gitValue(args, fallback) {
  const r = run('git', args, { timeout: 20000 });
  return r.status === 0 ? r.stdout.trim() : fallback;
}

function toolVersion(line) {
  const m = /(\d+\.\d+\.\d+)/.exec(line || '');
  return m ? m[1] : null;
}

function main() {
  let args;
  try { args = parseArgs(process.argv.slice(2)); }
  catch (e) { console.error(`usage error: ${e.message}`); return 2; }

  const outDir = args.outDir
    ? path.resolve(args.outDir)
    : path.join(ROOT, 'reports', 'coverage-rust-cli');

  const produced = produceReports(outDir);
  if (produced.status === 'blocked') {
    console.error(`blocked: ${produced.reason}`);
    return 2;
  }
  if (produced.status === 'fail') {
    console.error(`fail: ${produced.reason}`);
    return 1;
  }

  const report = readJson(produced.jsonPath);
  const sourceFiles = listSourceFiles(ROOT, SOURCE_ROOT);
  const baselinePath = path.join(ROOT, ...BASELINE.split('/'));

  if (args.update) {
    const prior = exists(baselinePath) ? readJson(baselinePath) : null;
    const next = buildBaseline({
      report,
      root: ROOT,
      sourceRoot: SOURCE_ROOT,
      sourceFiles,
      meta: {
        crate: 'nemo-mcp',
        manifestPath: MANIFEST,
        recorded: {
          sha: gitValue(['rev-parse', 'HEAD'], null),
          // Whole-tree dirt is expected while this tooling is itself in
          // flight. What decides whether these numbers are reproducible is
          // whether the measured CRATE was clean, so record that separately.
          treeDirty: gitValue(['status', '--porcelain'], '') !== '',
          crateDirty: gitValue(['status', '--porcelain', '--', 'nemo-mcp'], '') !== '',
          date: new Date().toISOString().slice(0, 10),
          issue: 'T04/#1053',
        },
        tool: {
          name: 'cargo-llvm-cov',
          version: toolVersion(produced.tool.version),
          versionLine: produced.tool.version,
          install: 'cargo install cargo-llvm-cov --version 0.9.1 --locked',
          rustupComponent: 'llvm-tools-preview',
          cargo: toolVersion(gitOrTool(['cargo', '--version'])),
          rustc: toolVersion(gitOrTool(['rustc', '--version'])),
          hostTriple: hostTriple(),
        },
        tests: produced.counts,
        exclusions: (prior && prior.exclusions) || [],
        priorFiles: (prior && prior.files) || {},
      },
    });
    fs.mkdirSync(path.dirname(baselinePath), { recursive: true });
    fs.writeFileSync(baselinePath, JSON.stringify(next, null, 2) + '\n');
    console.log(`wrote ${BASELINE}: ${Object.keys(next.files).length} files, `
      + `totals lines ${next.totals.lines}% / regions ${next.totals.regions}% / functions ${next.totals.functions}%`);
    if (prior) reportBaselineDelta(prior, next);
    return 0;
  }

  if (!exists(baselinePath)) {
    console.error(`blocked: ${BASELINE} missing; run with --update and have the result reviewed`);
    return 2;
  }

  let verdict;
  try {
    verdict = evaluateRustCoverage({ report, baseline: readJson(baselinePath), sourceFiles, root: ROOT });
  } catch (e) {
    console.error(`fail: coverage ratchet could not evaluate: ${e.message}`);
    return 1;
  }

  if (args.json) {
    console.log(JSON.stringify(verdict, null, 2));
    return verdict.ok ? 0 : 1;
  }

  for (const row of verdict.perFile) {
    if (row.noRegions) { console.log(`  ${row.path.padEnd(28)} no executable regions`); continue; }
    console.log(`  ${row.path.padEnd(28)} lines ${pct(row.percent.lines)} (floor ${pct(row.floor.lines)})  `
      + `regions ${pct(row.percent.regions)}  functions ${pct(row.percent.functions)}`);
  }
  console.log(`  ${'TOTAL'.padEnd(28)} lines ${pct(verdict.totals.lines)}  regions ${pct(verdict.totals.regions)}  functions ${pct(verdict.totals.functions)}`);
  for (const item of verdict.improvements) {
    console.log(`improved: ${item.file} ${item.metric} ${pct(item.candidate)} > floor ${pct(item.floor)} — re-run --update to lock it in`);
  }
  for (const item of verdict.removals) console.log(`removed: ${item.file} (${item.reason})`);
  for (const v of verdict.violations) console.error(`violation [${v.rule}] ${v.message}`);

  console.log(verdict.ok
    ? `ok: ${verdict.measuredFileCount} measured file(s), every reviewed floor held`
    : `FAILED: ${verdict.violations.length} violation(s)`);
  return verdict.ok ? 0 : 1;
}

function gitOrTool(argv) {
  const r = run(argv[0], argv.slice(1), { timeout: 20000 });
  return r.status === 0 ? r.stdout.trim() : null;
}

function hostTriple() {
  const r = run('rustc', ['-vV'], { timeout: 20000 });
  if (r.status !== 0) return null;
  const m = /^host:\s*(.+)$/m.exec(r.stdout);
  return m ? m[1].trim() : null;
}

function reportBaselineDelta(prior, next) {
  for (const [file, entry] of Object.entries(next.files)) {
    const before = prior.files ? prior.files[file] : null;
    if (!before) { console.log(`  + ${file} newly baselined`); continue; }
    for (const metric of ['lines', 'regions', 'functions']) {
      if (typeof before[metric] !== 'number' || typeof entry[metric] !== 'number') continue;
      if (entry[metric] < before[metric]) console.log(`  ! ${file} ${metric} floor LOWERED ${before[metric]} -> ${entry[metric]} (justify this in the PR)`);
      else if (entry[metric] > before[metric]) console.log(`  ^ ${file} ${metric} floor raised ${before[metric]} -> ${entry[metric]}`);
    }
  }
  for (const file of Object.keys(prior.files || {})) {
    if (!Object.hasOwn(next.files, file)) console.log(`  - ${file} dropped from the baseline`);
  }
}

function pct(value) {
  return typeof value === 'number' ? `${value.toFixed(2)}%`.padStart(8) : '     n/a';
}

process.exit(main());
