'use strict';

// Rust test jobs are loaded only when selected so minimal scripts/nemo harnesses
// can use the shared registry without copying this job-specific module.
const fs = require('node:fs');
const path = require('node:path');
const { ROOT, run, which, exists } = require('./util.cjs');
const { STATUS } = require('./receipt.cjs');
const caps = require('./capabilities.cjs');

function pass(reason, extra) { return Object.assign({ status: STATUS.PASS, reason }, extra); }
function fail(reason, extra) { return Object.assign({ status: STATUS.FAIL, reason }, extra); }
function blocked(reason, extra) { return Object.assign({ status: STATUS.BLOCKED, reason }, extra); }

function logOf(r) { return `$ ${r.cmd}\n(exit ${r.status}${r.signal ? ' signal ' + r.signal : ''}${r.error ? ' error ' + r.error : ''}, ${r.durationMs} ms)\n--- stdout ---\n${r.stdout}\n--- stderr ---\n${r.stderr}\n`; }

function mcpSidecarTarget() {
  let target = process.env.TAURI_ENV_TARGET_TRIPLE || process.env.NEMO_MCP_TARGET;
  if (!target) {
    const host = run('rustc', ['-vV'], { timeout: 15000 });
    target = host.status === 0 ? /^host: (\S+)$/m.exec(host.stdout)?.[1] : null;
  }
  if (!target || !/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(target)) return null;
  return path.join(ROOT, 'src-tauri', 'binaries', `nemo-mcp-${target}${target.includes('windows') ? '.exe' : ''}`);
}

function mcpSidecarPresent(sidecar) {
  try { return fs.lstatSync(sidecar); }
  catch (error) {
    if (error.code === 'ENOENT') return null;
    throw error;
  }
}

function jobTestTauri(ctx, manifest, label) {
  const selected = mcpSidecarTarget();
  if (!selected) return fail('MCP sidecar target is unavailable');
  const existing = mcpSidecarPresent(selected);
  let staged = false;
  let result;
  let primaryError;
  try {
    if (!existing) {
      staged = true;
      const builder = run(process.execPath, ['scripts/build-mcp-sidecar.cjs'], { timeout: 10 * 60 * 1000 });
      if (builder.status !== 0) {
        result = fail(`MCP sidecar builder failed (${builder.status})`, { exitCode: builder.status, log: logOf(builder) });
        return result;
      }
      const artifact = mcpSidecarPresent(selected);
      if (!artifact || !artifact.isFile()) {
        result = fail(`MCP sidecar builder produced no regular expected artifact: ${path.relative(ROOT, selected)}`, { log: logOf(builder) });
        return result;
      }
    }
    // --no-fail-fast: cargo stops at the first failing test BINARY, so the
    // summary below would sum one binary's result and under-report the crate.
    const args = ['test', '--no-fail-fast', '--release', '--manifest-path', manifest, '--', '--test-threads=1'];
    const r = run('cargo', args, { timeout: 60 * 60 * 1000 });
    const results = [...r.stdout.matchAll(/^test result: (\w+)\. (\d+) passed; (\d+) failed/gm)];
    const passed = results.reduce((a, m) => a + Number(m[2]), 0), failed = results.reduce((a, m) => a + Number(m[3]), 0);
    const summary = results.length ? `${results.length} binaries, ${passed} passed, ${failed} failed` : `exit ${r.status}`;
    result = (r.status === 0 ? pass : fail)(`cargo test ${label}: ${summary}`, { exitCode: r.status, log: logOf(r) });
    return result;
  } catch (error) {
    primaryError = error;
    throw error;
  } finally {
    // Only a target absent before this job can be builder-owned. Do not touch
    // a pre-existing sidecar, including its bytes and mode, on any Cargo path.
    if (staged) {
      try { fs.unlinkSync(selected); }
      catch (error) {
        if (error.code !== 'ENOENT' && result) {
          const cleanup = `MCP sidecar cleanup failed: ${error.message || error.code}`;
          if (result.status === STATUS.PASS) {
            result.status = STATUS.FAIL;
            result.reason = cleanup;
            result.exitCode = 1;
          } else {
            result.limitations = (result.limitations || []).concat(cleanup);
          }
        } else if (error.code !== 'ENOENT' && primaryError && typeof primaryError === 'object') {
          primaryError.message = `${primaryError.message || primaryError}; MCP sidecar cleanup failed: ${error.message || error.code}`;
        }
      }
    }
  }
}

function jobTestRust(ctx, crateDir, label) {
  if (!which('cargo')) return blocked('cargo not found');
  const manifest = path.join(ROOT, crateDir, 'Cargo.toml');
  if (!exists(manifest)) return blocked(`${crateDir}/Cargo.toml missing`);
  if (crateDir === 'src-tauri') {
    const sidecar = caps.nativeFixtureSidecarProbe();
    if (!sidecar.runs) return blocked(`native fixture sidecar unavailable (${sidecar.source}): ${sidecar.path}: ${sidecar.failure}`, {
      details: { nativeFixtureSidecar: sidecar },
    });
    return jobTestTauri(ctx, manifest, label);
  }
  // --no-fail-fast: cargo stops at the first failing test BINARY, so the
  // summary below would sum one binary's result and under-report the crate.
  const args = ['test', '--no-fail-fast'];
  // The Tauri crate includes wall-clock decoder regression tests. Running
  // those through an unoptimized test binary or beside dozens of other
  // FFmpeg processes measures the harness, not production decoder latency.
  // Keep every assertion and threshold, but exercise optimized code serially.
  args.push('--manifest-path', manifest);
  const r = run('cargo', args, { timeout: 60 * 60 * 1000 });
  const results = [...r.stdout.matchAll(/^test result: (\w+)\. (\d+) passed; (\d+) failed/gm)];
  const passed = results.reduce((a, m) => a + Number(m[2]), 0), failed = results.reduce((a, m) => a + Number(m[3]), 0);
  const summary = results.length ? `${results.length} binaries, ${passed} passed, ${failed} failed` : `exit ${r.status}`;
  return (r.status === 0 ? pass : fail)(`cargo test ${label}: ${summary}`, { exitCode: r.status, log: logOf(r) });
}

function jobTestNativeEngine(ctx) {
  if (!which('cargo')) return blocked('cargo not found');
  const manifest = path.join(ROOT, 'native-engine', 'Cargo.toml');
  if (!exists(manifest)) return blocked('native-engine/Cargo.toml missing');
  const targets = ['codec', 'commands', 'history', 'evaluation', 'scheduler', 'compositor', 'viewport', 'export_job', 'application'];
  const targetDir = path.join(ctx.reportDir, 'native-engine-target');
  const results = [], logs = [];
  for (const target of targets) {
    const args = ['test', '--locked', '--manifest-path', manifest, '--no-default-features', '--features', `test-${target}`, '--test', target];
    const r = run('cargo', args, { timeout: 15 * 60 * 1000, env: { CARGO_TARGET_DIR: targetDir } });
    const summaries = [...r.stdout.matchAll(/^test result: (\w+)\. (\d+) passed; (\d+) failed/gm)];
    const passed = summaries.reduce((sum, match) => sum + Number(match[2]), 0);
    const failed = summaries.reduce((sum, match) => sum + Number(match[3]), 0);
    const complete = r.status === 0 && summaries.length > 0 && passed > 0 && failed === 0;
    results.push({ target, feature: `test-${target}`, status: complete ? 'pass' : 'fail', exitCode: r.status, summaries: summaries.length, passed, failed });
    logs.push(logOf(r));
  }
  const bad = results.filter((result) => result.status !== 'pass');
  const total = results.reduce((sum, result) => sum + result.passed, 0);
  const extra = { exitCode: bad.length ? 1 : 0, log: logs.join('\n'), details: { targets: results }, artifacts: [{ path: path.relative(ROOT, targetDir) }] };
  return bad.length
    ? fail(`native-engine focused targets incomplete: ${bad.map((result) => `${result.target} (exit ${result.exitCode}, summaries ${result.summaries}, ${result.passed} passed/${result.failed} failed)`).join('; ')}`, extra)
    : pass(`${targets.length} native-engine targets, ${total} tests passed`, extra);
}

module.exports = { jobTestTauri, jobTestRust, jobTestNativeEngine };
