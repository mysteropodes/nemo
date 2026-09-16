'use strict';
// `test:coverage` (T01): per-file JavaScript coverage thresholds with named,
// reviewed exceptions.
//
// Split out of jobs.cjs by #1320: that file measured 398 non-blank lines
// against a hardMax of 400, so the next job it gained had to come with room.
// Same shape as its sibling ./coverage-rust-job.cjs, which the registry also
// requires lazily — every CLI entry point loads the registry, and neither
// coverage module should be a dependency of `npm run doctor`.
const path = require('node:path');
const { ROOT, run, exists, readJson, fileInfo } = require('./util.cjs');
const { STATUS } = require('./receipt.cjs');
const caps = require('./capabilities.cjs');

const pass = (reason, extra) => Object.assign({ status: STATUS.PASS, reason }, extra);
const fail = (reason, extra) => Object.assign({ status: STATUS.FAIL, reason }, extra);
const blocked = (reason, extra) => Object.assign({ status: STATUS.BLOCKED, reason }, extra);
const logOf = (r) => `$ ${r.cmd}\n(exit ${r.status}${r.signal ? ' signal ' + r.signal : ''}${r.error ? ' error ' + r.error : ''}, ${r.durationMs} ms)\n--- stdout ---\n${r.stdout}\n--- stderr ---\n${r.stderr}\n`;

// ---- coverage:js (T01) --------------------------------------------------
// c8 has no per-file threshold with named exceptions built in — --check-coverage
// is all-or-nothing across --include. This reads c8's own coverage-summary.json
// (istanbul's computed per-file percentages, not reimplemented here) and applies
// lines/branches thresholds file by file, skipping paths in .c8rc.json's
// "exceptions" (each with a reviewed reason) while still reporting them.
function evaluateCoverageSummary(summary, config, root) {
  const exceptions = new Map((config.exceptions || []).map((e) => [e.path, e.reason]));
  const perFile = [];
  const violations = [];
  for (const relPath of config.include || []) {
    const absPath = path.resolve(root, relPath);
    const entry = summary[absPath];
    const exceptionReason = exceptions.get(relPath) || null;
    if (!entry) {
      if (exceptionReason) { perFile.push({ path: relPath, linesPct: null, branchesPct: null, exception: exceptionReason }); continue; }
      violations.push(`${relPath}: missing from coverage summary`);
      continue;
    }
    const linesPct = entry.lines.pct;
    const branchesPct = entry.branches.pct;
    perFile.push({ path: relPath, linesPct, branchesPct, exception: exceptionReason });
    if (exceptionReason) continue;
    if (linesPct < config.lines) violations.push(`${relPath}: lines ${linesPct}% < ${config.lines}%`);
    if (branchesPct < config.branches) violations.push(`${relPath}: branches ${branchesPct}% < ${config.branches}%`);
  }
  return { ok: violations.length === 0, perFile, violations };
}

function jobTestCoverageJs(ctx) {
  const c8Bin = caps.localBin('c8');
  if (!c8Bin) return blocked('c8 is not installed (not in devDependencies); run npm ci');
  const configPath = path.join(ROOT, '.c8rc.json');
  if (!exists(configPath)) return blocked('.c8rc.json is missing');
  const config = readJson(configPath);
  const reportDir = path.join(ctx.reportDir, 'coverage-js');
  const r = run(c8Bin,
    ['--config', configPath, '--report-dir', reportDir, process.execPath, '--test', 'tests/*.test.cjs', 'tests/animation/*.test.cjs'],
    { timeout: 10 * 60 * 1000 });
  if (r.status !== 0) return fail(`node --test under c8 exit ${r.status}`, { exitCode: r.status, log: logOf(r) });
  const summaryPath = path.join(reportDir, 'coverage-summary.json');
  if (!exists(summaryPath)) return fail('c8 did not produce coverage-summary.json', { log: logOf(r) });
  const verdict = evaluateCoverageSummary(readJson(summaryPath), config, ROOT);
  const artifacts = [
    fileInfo(path.join(reportDir, 'index.html')),
    fileInfo(path.join(reportDir, 'lcov.info')),
    fileInfo(path.join(reportDir, 'coverage-final.json')),
  ].filter((a) => a.present);
  const exceptionCount = (config.exceptions || []).length;
  const label = `${verdict.perFile.length} migrated module(s), lines>=${config.lines}%/branches>=${config.branches}%, ${exceptionCount} reviewed exception(s)`;
  if (!verdict.ok) return fail(`${label} — violations: ${verdict.violations.join('; ')}`, { details: { perFile: verdict.perFile }, artifacts });
  return pass(label, { details: { perFile: verdict.perFile }, artifacts });
}

module.exports = { evaluateCoverageSummary, jobTestCoverageJs };
