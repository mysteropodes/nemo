'use strict';
// One-shot owner of the long-lived isolated desktop builder. Never publishes
// the launcher's control line/token; receipts contain only status and artifacts.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const isolation = require('./isolation.cjs');
const build = require('./build-runtime.cjs');
const { bundleExecutable } = require('./native-runtime.cjs');
const { ROOT, run, exists, fileInfo } = require('./util.cjs');

const SCHEMA = 'nemo.build-job/1';
const delay = (ms) => new Promise(resolve => setTimeout(resolve, ms));
const result = (status, reason) => ({ schema: SCHEMA, status, reason,
  exitCode: status === 'pass' ? 0 : status === 'blocked' ? 2 : 1,
  artifacts: [], details: {}, limitations: ['Unsigned local verification package; not release acceptance.'] });

function launch(taskId, options) {
  const args = [path.join(ROOT, 'scripts/nemo/build.cjs'), 'start', '--task', taskId];
  if (options.hostTriple) args.push('--host-triple', options.hostTriple);
  if (options.command) args.push('--command', options.command, '--', ...(options.args || []));
  const child = spawn(process.execPath, args, { cwd: ROOT, env: process.env, stdio: ['ignore', 'pipe', 'pipe'] });
  let stderr = ''; let stdout = '';
  child.stderr.on('data', chunk => { stderr = (stderr + chunk).slice(-32_768); });
  const ready = new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('build launcher readiness timed out')), options.readyTimeoutMs || 30_000);
    const fail = error => { clearTimeout(timer); reject(error); };
    child.once('error', fail);
    child.once('exit', code => fail(new Error(`build launcher refused startup (${code}): ${stderr.trim()}`)));
    child.stdout.on('data', chunk => {
      stdout += chunk;
      const newline = stdout.indexOf('\n');
      if (newline < 0) return;
      clearTimeout(timer);
      try {
        const control = JSON.parse(stdout.slice(0, newline));
        if (!control.started || control.taskId !== taskId || control.pid !== child.pid || typeof control.ownerToken !== 'string') {
          throw new Error('build launcher returned invalid ownership evidence');
        }
        resolve(control);
      } catch { reject(new Error('build launcher returned invalid ownership evidence')); }
      stdout = '';
    });
  });
  return { child, ready };
}

function helperExited(child) {
  return !Number.isSafeInteger(child.pid) || child.exitCode !== null || child.signalCode !== null;
}

function lateControl(taskId, child) {
  const record = isolation.readLauncher(taskId);
  return record && record.taskId === taskId && Number.isSafeInteger(child.pid) && record.pid === child.pid
    && typeof record.ownerToken === 'string' && record.ownerToken ? record : null;
}

async function reconcileStartup(taskId, child, graceMs) {
  const wasLive = !helperExited(child);
  // A rejected control line does not prove no ownership was acquired. Recover
  // only this invocation's task record and exact spawned helper PID; otherwise
  // signal that ChildProcess handle alone, never a record's PID or app group.
  for (const signal of ['SIGTERM', 'SIGKILL']) {
    const control = lateControl(taskId, child);
    if (control) return { control };
    if (helperExited(child)) break;
    try { child.kill(signal); } catch { /* report an unconfirmed exit below */ }
    const deadline = Date.now() + graceMs;
    while (!helperExited(child) && Date.now() < deadline) {
      const acquired = lateControl(taskId, child);
      if (acquired) return { control: acquired };
      await delay(20);
    }
  }
  const control = lateControl(taskId, child);
  if (control) return { control };
  const stopped = helperExited(child);
  const retained = exists(isolation.taskRoot(taskId));
  return { failed: wasLive || retained || !stopped,
    cleanup: { stopped: stopped && !retained, helperStopped: stopped, released: stopped && !retained, taskId },
    reason: !stopped ? 'spawned helper exit unconfirmed; task retained for reconciliation'
      : retained ? 'task ownership could not be proved; existing data retained for reconciliation'
        : 'unregistered spawned helper exited; no task runtime remains' };
}

async function completed(taskId, control, child, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const owner = isolation.readLauncher(taskId);
    if (!owner || owner.pid !== child.pid || owner.ownerToken !== control.ownerToken) throw new Error('build launcher ownership changed; reconcile before release');
    const status = build.readBuildStatus(taskId);
    if (status && ['completed', 'failed', 'reconciliation-required'].includes(status.state)) return status;
    if (child.exitCode !== null || child.signalCode !== null) throw new Error('build launcher exited before a terminal build result');
    if (Date.now() >= deadline) throw new Error('isolated desktop build timed out');
    await delay(100);
  }
}

function preservePackage(status, directory, hostTriple) {
  // No resolveApp fallback: only this successful build's exact task target is
  // eligible. An older shared-target bundle cannot turn a missing output green.
  const parent = path.join(status.targetDir, hostTriple || status.build.startup.hostTriple, 'release/bundle/macos');
  const names = exists(parent) ? fs.readdirSync(parent).filter(name => name.endsWith('.app')) : [];
  if (names.length !== 1) throw new Error('isolated build did not produce exactly one app bundle');
  const source = path.join(parent, names[0]);
  if (!fs.lstatSync(source).isDirectory()) throw new Error('isolated app output is not a real directory');
  const executable = bundleExecutable(source);
  const destination = path.join(directory, names[0]);
  if (exists(destination)) throw new Error('preserved app destination already exists');
  if (process.platform === 'darwin') {
    const copied = run('/usr/bin/ditto', [source, destination]);
    if (copied.status !== 0) throw new Error(`app preservation failed (${copied.status})`);
  } else fs.cpSync(source, destination, { recursive: true, force: false, errorOnExist: true, preserveTimestamps: true, verbatimSymlinks: true });
  if (fileInfo(executable).sha256 !== fileInfo(bundleExecutable(destination)).sha256) throw new Error('preserved executable differs from the isolated build');
  return destination;
}

function finalizePackage(app) {
  return run('python3', ['scripts/bundle-ffmpeg-dylibs.py', app], { timeout: 10 * 60 * 1000 });
}

function packageArtifacts(app) {
  const executable = fileInfo(bundleExecutable(app));
  const sidecar = fileInfo(path.join(app, 'Contents/MacOS/ffmpeg'));
  if (!sidecar.present) throw new Error('preserved package is missing its ffmpeg sidecar');
  return [{ path: path.relative(ROOT, app) }, executable,
    fileInfo(path.join(app, 'Contents/Info.plist')), sidecar];
}

async function runBuildJob(options = {}) {
  const taskId = options.taskId || `desktop-build-${crypto.randomBytes(16).toString('hex')}`;
  const root = isolation.taskRoot(taskId);
  const reportDir = path.resolve(options.reportDir || path.join(ROOT, 'reports'));
  const relative = path.relative(root, reportDir);
  if (!relative || (!relative.startsWith('..' + path.sep) && relative !== '..' && !path.isAbsolute(relative))) {
    return result('blocked', 'build reports must be outside the disposable build task root');
  }
  if (exists(root)) return result('blocked', 'build task runtime already exists; reconcile its owner before reuse');
  if (!options.command) {
    try { build.assertNativePlatform(); build.localTauriExecutable(); }
    catch (error) { return result('blocked', error.message); }
  }
  const directory = path.join(reportDir, taskId);
  fs.mkdirSync(directory, { recursive: true, mode: 0o700 });
  let processInfo; let control; let status; let outcome; let packagePreserved = false; let recoveredStartup = false;
  try {
    processInfo = launch(taskId, options);
    control = await processInfo.ready;
    status = await completed(taskId, control, processInfo.child, options.timeoutMs || 2 * 60 * 60 * 1000);
    const handshake = build.buildHandshake(taskId, control.ownerToken);
    outcome = result(status.state === 'completed' && status.exitCode === 0 && status.processTree && status.processTree.stopped && handshake.ok ? 'pass' : 'fail',
      status.state !== 'completed' ? `isolated desktop build ${status.state} (exit ${status.exitCode})` : !handshake.ok ? handshake.reason : 'isolated desktop build completed');
    outcome.details = { taskId, source: status.source, build: status.build, state: status.state,
      buildExitCode: status.exitCode, processTree: status.processTree, handshake: { ok: handshake.ok, reason: handshake.reason } };
    if (status.state === 'completed' && status.exitCode === 0) {
      const app = preservePackage(status, directory, options.hostTriple);
      packagePreserved = true;
      // Preserve the existing standard command's dylib finalization on the
      // copied package, so postprocessing cannot escape into a shared target.
      const finalized = (options.finalizePackage || finalizePackage)(app);
      outcome.log = (finalized.stdout || '') + (finalized.stderr || '');
      outcome.artifacts = packageArtifacts(app);
      if (finalized.status !== 0) throw new Error(`bundle-ffmpeg-dylibs.py failed (${finalized.status})`);
      const finalHandshake = build.buildHandshake(taskId, control.ownerToken);
      if (!finalHandshake.ok) throw new Error(finalHandshake.reason);
    }
  } catch (error) {
    outcome = Object.assign(outcome || result(control ? 'fail' : 'blocked', error.message),
      { status: control ? 'fail' : 'blocked', reason: error.message, exitCode: control ? 1 : 2 });
  }
  outcome.details.taskId = taskId;
  if (!control && processInfo) {
    const recovered = await reconcileStartup(taskId, processInfo.child, options.startupStopTimeoutMs || 1000);
    if (recovered.control) {
      control = recovered.control; recoveredStartup = true;
      outcome.status = 'fail'; outcome.exitCode = 1;
      outcome.details.recoveredStartup = true;
    } else {
      outcome.details.cleanup = recovered.cleanup;
      if (recovered.failed) { outcome.status = 'fail'; outcome.exitCode = 1; }
      if (!recovered.cleanup.released) outcome.reason += `; cleanup incomplete: ${recovered.reason}`;
    }
  }
  if (control) {
    let launcherStopped = false;
    try {
      const stopped = await isolation.requestStop(taskId, control.ownerToken, {
        timeoutMs: 10_000, retainRecord: true,
        verifyProcess: record => record.pid === processInfo.child.pid && processInfo.child.exitCode === null && processInfo.child.signalCode === null,
      });
      if (!stopped.stopped) throw new Error(stopped.reason);
      launcherStopped = true;
      const finalStatus = build.readBuildStatus(taskId);
      const reports = path.join(directory, 'build-logs');
      fs.cpSync(path.join(root, 'reports'), reports, { recursive: true, errorOnExist: true, force: false });
      outcome.details.finalState = finalStatus && finalStatus.state;
      outcome.artifacts.push({ path: path.relative(ROOT, reports) });
      if (!recoveredStartup && status && status.state === 'completed' && !packagePreserved) throw new Error('completed build output was not preserved; task retained for inspection');
      const released = isolation.releaseTask(taskId, control.ownerToken, { requireOwner: true, retainData: recoveredStartup, beforeRelease(record) {
        if (record.pid !== processInfo.child.pid || !finalStatus || !finalStatus.processTree || !finalStatus.processTree.stopped
          || build.buildProcessTreeAlive({ pid: finalStatus.childPid })) throw new Error('build process group exit unconfirmed; task retained for reconciliation');
      } });
      if (!released.released) throw new Error(released.reason);
      outcome.details.cleanup = { stopped: true, released: true, retainedData: recoveredStartup, slots: released.slots };
    } catch (error) {
      outcome.status = 'fail'; outcome.exitCode = 1;
      outcome.reason += `; cleanup incomplete: ${error.message}`;
      outcome.details.cleanup = { stopped: launcherStopped, released: false, taskId };
    }
  }
  if (processInfo && !helperExited(processInfo.child)) {
    // An unreconciled helper must not keep an embedding job runner hung after
    // its explicit failure receipt. Its identified state remains inspectable.
    processInfo.child.unref(); processInfo.child.stdout.destroy(); processInfo.child.stderr.destroy();
  }
  const proof = path.join(directory, 'build-proof.json');
  fs.writeFileSync(proof, JSON.stringify(outcome, null, 2));
  outcome.artifacts.push(fileInfo(proof));
  return outcome;
}

module.exports = { SCHEMA, runBuildJob };
