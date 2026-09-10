'use strict';
// `test:coverage-rust` (T04): run the nemo-mcp Cargo suites under
// cargo-llvm-cov, emit LCOV/HTML/JSON for production source, and compare the
// result with the committed reviewed ratchet baseline.
//
// The status contract is the repo's existing vocabulary, and it is the point
// of this job rather than a detail of it:
//   blocked — the coverage tool or the llvm-tools component is absent, or the
//             baseline is missing. Named exactly. Never downgraded to a skip
//             and never reported as pass.
//   fail    — a Cargo test failed under instrumentation, a report could not be
//             produced, or coverage fell below a reviewed floor.
//   pass    — the suites ran, reports exist, and every floor held.

const fs = require('node:fs');
const path = require('node:path');
const { ROOT, run, which, exists, readJson, fileInfo } = require('./util.cjs');
const { evaluateRustCoverage, SCHEMA } = require('./coverage-rust.cjs');

const CRATE_DIR = 'nemo-mcp';
const MANIFEST = 'nemo-mcp/Cargo.toml';
const SOURCE_ROOT = 'nemo-mcp/src';
const BASELINE = 'engineering/coverage/rust-mcp.baseline.json';
const TIMEOUT_MS = 60 * 60 * 1000;

function listSourceFiles(root, sourceRoot) {
  const base = path.resolve(root, ...sourceRoot.split('/'));
  const out = [];
  (function walk(dir) {
    let entries;
    try { entries = fs.readdirSync(dir, { withFileTypes: true }); } catch { return; }
    for (const entry of entries.sort((a, b) => a.name.localeCompare(b.name))) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (entry.name.endsWith('.rs')) out.push(path.relative(root, full).split(path.sep).join('/'));
    }
  })(base);
  return out;
}

// cargo-llvm-cov is a cargo subcommand: `cargo-llvm-cov --version` is an
// error, the real probe is the `llvm-cov` subcommand.
function probeCoverageTool() {
  const bin = which('cargo-llvm-cov');
  if (!bin) return { present: false, version: null };
  const r = run(bin, ['llvm-cov', '--version'], { timeout: 60000 });
  const line = (r.stdout + '\n' + r.stderr).split(/\r?\n/).find((l) => l.trim()) || '';
  return { present: r.status === 0, version: r.status === 0 ? line.trim() : null, probeExit: r.status };
}

// llvm-tools ships the profdata/cov binaries cargo-llvm-cov drives. Without
// rustup there is no read-only way to enumerate components, so absence is
// only asserted when rustup can actually answer.
function llvmToolsMissing() {
  if (!which('rustup')) return false;
  const r = run('rustup', ['component', 'list', '--installed'], { timeout: 60000 });
  if (r.status !== 0) return false;
  return !/^llvm-tools/m.test(r.stdout);
}

function logOf(r) {
  return `$ ${r.cmd}\n(exit ${r.status}${r.signal ? ' signal ' + r.signal : ''}${r.error ? ' error ' + r.error : ''}, ${r.durationMs} ms)\n--- stdout ---\n${r.stdout}\n--- stderr ---\n${r.stderr}\n`;
}

function countTests(stdout) {
  const results = [...stdout.matchAll(/^test result: \w+\. (\d+) passed; (\d+) failed/gm)];
  return {
    binaries: results.length,
    passed: results.reduce((a, m) => a + Number(m[1]), 0),
    failed: results.reduce((a, m) => a + Number(m[2]), 0),
  };
}

/**
 * Run the crate's suites under instrumentation and render LCOV/HTML/JSON.
 *
 * Returns `{ status: 'blocked'|'fail'|'ok', ... }` so the job and the
 * baseline-update CLI share exactly one cargo invocation path — a second copy
 * would be free to drift into measuring something the gate never checks.
 */
function produceReports(outDir) {
  if (!which('cargo')) return { status: 'blocked', reason: 'cargo not found' };
  const manifest = path.join(ROOT, ...MANIFEST.split('/'));
  if (!exists(manifest)) return { status: 'blocked', reason: `${MANIFEST} missing` };

  const tool = probeCoverageTool();
  if (!tool.present) {
    return { status: 'blocked', reason: 'cargo-llvm-cov not found; install the pinned tool with '
      + '`cargo install cargo-llvm-cov --version 0.9.1 --locked` and '
      + '`rustup component add llvm-tools-preview`' };
  }
  if (llvmToolsMissing()) {
    return { status: 'blocked', tool, reason: 'the llvm-tools rustup component is not installed; run `rustup component add llvm-tools-preview`' };
  }

  fs.mkdirSync(outDir, { recursive: true });
  const jsonPath = path.join(outDir, 'coverage.json');
  const lcovPath = path.join(outDir, 'lcov.info');
  const htmlDir = path.join(outDir, 'html');
  const logs = [];

  const clean = run('cargo', ['llvm-cov', 'clean', '--workspace', '--manifest-path', MANIFEST], { timeout: TIMEOUT_MS });
  logs.push(logOf(clean));

  // Run the suites once, then render three views of the same profile data.
  const tests = run('cargo', ['llvm-cov', '--no-report', '--manifest-path', MANIFEST], { timeout: TIMEOUT_MS });
  logs.push(logOf(tests));
  const counts = countTests(tests.stdout);
  if (tests.status !== 0) {
    // A failing suite is a failing suite. Coverage is not reported as if the
    // crate were healthy, and this is never softened to blocked.
    const combined = tests.stdout + tests.stderr;
    if (/llvm-tools|llvm-profdata/i.test(combined)) {
      return { status: 'blocked', tool, counts, logs, exitCode: tests.status,
        reason: 'cargo-llvm-cov could not find its llvm-tools binaries; run `rustup component add llvm-tools-preview`' };
    }
    return { status: 'fail', tool, counts, logs, exitCode: tests.status,
      reason: `cargo llvm-cov: ${counts.binaries} binaries, ${counts.passed} passed, ${counts.failed} failed (exit ${tests.status})` };
  }

  for (const args of [['--json', '--output-path', jsonPath], ['--lcov', '--output-path', lcovPath], ['--html', '--output-dir', htmlDir]]) {
    const r = run('cargo', ['llvm-cov', 'report', '--manifest-path', MANIFEST].concat(args), { timeout: TIMEOUT_MS });
    logs.push(logOf(r));
    if (r.status !== 0) {
      return { status: 'fail', tool, counts, logs, exitCode: r.status, reason: `cargo llvm-cov report ${args[0]} exit ${r.status}` };
    }
  }
  if (!exists(jsonPath)) return { status: 'fail', tool, counts, logs, exitCode: 1, reason: 'cargo llvm-cov produced no JSON export' };

  return { status: 'ok', tool, counts, logs, jsonPath, lcovPath, htmlIndex: path.join(htmlDir, 'html', 'index.html') };
}

function jobCoverageRust(ctx) {
  const pass = (reason, extra) => Object.assign({ status: 'pass', reason }, extra);
  const fail = (reason, extra) => Object.assign({ status: 'fail', reason }, extra);
  const blocked = (reason, extra) => Object.assign({ status: 'blocked', reason }, extra);

  const baselinePath = path.join(ROOT, ...BASELINE.split('/'));
  if (!exists(baselinePath)) {
    return blocked(`${BASELINE} missing; regenerate it with \`node scripts/nemo/coverage-rust.cjs --update\` and have it reviewed`);
  }
  const baseline = readJson(baselinePath);

  const produced = produceReports(path.join(ctx.reportDir, 'coverage-rust'));
  const log = (produced.logs || []).join('\n') || null;
  const tool = produced.tool || { version: null };
  if (produced.status === 'blocked') return blocked(produced.reason, { log, details: { coverageTool: tool.version } });
  if (produced.status === 'fail') {
    return fail(produced.reason, { exitCode: produced.exitCode, log, details: { tests: produced.counts, coverageTool: tool.version } });
  }

  const { jsonPath, lcovPath, htmlIndex, counts } = produced;
  const artifacts = [fileInfo(lcovPath), fileInfo(jsonPath), fileInfo(htmlIndex)].filter((a) => a.present);

  let verdict;
  try {
    verdict = evaluateRustCoverage({
      report: readJson(jsonPath),
      baseline,
      sourceFiles: listSourceFiles(ROOT, SOURCE_ROOT),
      root: ROOT,
    });
  } catch (e) {
    return fail(`coverage ratchet could not evaluate: ${e.message}`, { log: log, artifacts });
  }

  const limitations = [baseline.branchesExcluded].filter(Boolean);
  const pinned = baseline.tool && baseline.tool.version;
  if (pinned && tool.version && !tool.version.includes(pinned)) {
    limitations.push(`coverage tool is "${tool.version}" but the baseline floors were recorded with cargo-llvm-cov ${pinned}; percentages are not strictly comparable across tool versions`);
  }
  limitations.push('doc-tests are excluded from coverage on the stable toolchain (cargo-llvm-cov reports them only under nightly)');

  const details = {
    tests: counts,
    coverageTool: tool.version,
    totals: verdict.totals,
    perFile: verdict.perFile,
    improvements: verdict.improvements,
    removals: verdict.removals,
  };
  const label = `${counts.passed} tests, ${verdict.measuredFileCount}/${verdict.baselineFileCount} baselined files measured, `
    + `lines ${verdict.totals.lines.toFixed(2)}% / regions ${verdict.totals.regions.toFixed(2)}% / functions ${verdict.totals.functions.toFixed(2)}%`;

  if (!verdict.ok) {
    return fail(`${label} — ${verdict.violations.length} ratchet violation(s): `
      + verdict.violations.map((v) => v.message).join('; '),
      { exitCode: 1, log: log, artifacts, limitations, details: Object.assign({ violations: verdict.violations }, details) });
  }
  return pass(`${label}; every reviewed floor held`, { exitCode: 0, log: log, artifacts, limitations, details });
}

module.exports = {
  jobCoverageRust, produceReports, listSourceFiles, probeCoverageTool, llvmToolsMissing,
  CRATE_DIR, MANIFEST, SOURCE_ROOT, BASELINE, SCHEMA,
};
