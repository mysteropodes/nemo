'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { after, test } = require('node:test');

const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-native-orphan-reservation-'));
process.env.NEMO_ISOLATION_ROOT = path.join(scratch, 'runtime');

// Import the shipped launcher state and reservation implementation only after
// selecting this test's isolated runtime root.
const isolation = require('../scripts/nemo/lib/isolation.cjs');
const runtime = require('../scripts/nemo/lib/native-runtime.cjs');
const cli = path.join(__dirname, '..', 'scripts', 'nemo', 'native.cjs');
const repoRoot = path.resolve(__dirname, '..');

// A harmless executable standing in for built Nemo. It writes the native
// runtime manifest the production launcher requires, then remains alive until
// its process group is stopped. No desktop, window or GPU is involved.
const appStub = path.join(scratch, 'app-stub');
fs.writeFileSync(appStub, '#!/usr/bin/env node\n' + String.raw`
const fs = require('node:fs');
const path = require('node:path');
const [capture] = process.argv.slice(2);
const manifest = {
  schema: 'nemo.native-runtime/1',
  dirs: {},
  isolated: true,
  state: 'active',
  taskId: process.env.NEMO_TASK_ID,
  taskKey: process.env.NEMO_TASK_KEY,
  identifier: 'com.strokemotion.app.nemo-task-' + process.env.NEMO_TASK_KEY.slice(0, 16),
  dataStoreIdentifier: process.env.NEMO_TASK_KEY.slice(0, 32),
  dataDir: process.env.NEMO_TAURI_DATA_DIR,
  pid: process.pid,
};
fs.writeFileSync(capture, JSON.stringify({ pid: process.pid, taskId: manifest.taskId }));
fs.mkdirSync(manifest.dataDir, { recursive: true });
fs.writeFileSync(path.join(manifest.dataDir, 'native-runtime.json'), JSON.stringify(manifest, null, 2));
console.log(JSON.stringify(manifest));
process.on('SIGTERM', () => process.exit(0));
setInterval(() => {}, 1000);
`, { mode: 0o700 });

after(() => fs.rmSync(scratch, { recursive: true, force: true }));

function exited(child) {
  if (child.exitCode != null || child.signalCode != null) {
    return Promise.resolve({ code: child.exitCode, signal: child.signalCode });
  }
  return new Promise((resolve) => child.once('exit', (code, signal) => resolve({ code, signal })));
}

function firstLine(child) {
  return new Promise((resolve, reject) => {
    let data = '';
    const timer = setTimeout(() => done(new Error('launcher readiness timed out')), 10_000);
    function done(error, value) {
      clearTimeout(timer);
      child.stdout.off('data', read);
      child.off('exit', early);
      error ? reject(error) : resolve(value);
    }
    function read(chunk) {
      data += chunk;
      const newline = data.indexOf('\n');
      if (newline !== -1) done(null, data.slice(0, newline));
    }
    function early(code) { done(new Error(`launcher exited before readiness (${code}): ${data}`)); }
    child.stdout.on('data', read);
    child.once('exit', early);
  });
}

async function launch(taskId, slot) {
  const child = spawn(process.execPath, [
    cli, 'start', '--task', taskId, '--executable', appStub,
    '--reserve', slot, '--manifest-timeout-ms', '5000', '--', path.join(scratch, 'capture.json'),
  ], {
    cwd: repoRoot,
    env: { ...process.env },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  return { child, info: JSON.parse(await firstLine(child)) };
}

function command(args) {
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [cli, ...args], {
      cwd: repoRoot,
      env: { ...process.env },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => { stdout += chunk; });
    child.stderr.on('data', (chunk) => { stderr += chunk; });
    child.once('exit', (code, signal) => resolve({ code, signal, stdout, stderr }));
  });
}

async function waitFor(check, what, timeoutMs = 5000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const value = check();
    if (value) return value;
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  throw new Error(`timed out waiting for ${what}`);
}

test('a live native orphan retains its exclusive reservation after launcher death', async () => {
  const taskId = `native-orphan-reservation-${process.pid}`;
  const slot = `gpu-reference-orphan-${process.pid}`;
  let instance;
  let appPid;
  let contender;

  try {
    instance = await launch(taskId, slot);
    const active = await waitFor(() => {
      const status = runtime.readNativeStatus(taskId);
      return status && status.state === 'active' && status.childPid && status;
    }, 'active native app');
    appPid = active.childPid;
    const app = await waitFor(() => {
      const read = runtime.readAppManifest(taskId);
      return read && read.valid && read;
    }, 'app runtime manifest');
    assert.equal(app.manifest.pid, appPid, 'the manifest must identify the actual live app');
    assert.equal(isolation.pidAlive(appPid), true, 'the launched app must be alive');

    const beforeCrash = isolation.acquireExclusiveSlot(slot, `${taskId}-before-crash`);
    assert.equal(beforeCrash.acquired, false, 'the live launcher must exclude a contender');
    assert.equal(beforeCrash.holder.taskId, taskId);
    assert.equal(beforeCrash.holder.pid, instance.child.pid);

    instance.child.kill('SIGKILL');
    await exited(instance.child);
    assert.equal(isolation.pidAlive(instance.child.pid), false, 'the launcher must be dead');
    assert.equal(isolation.pidAlive(appPid), true, 'the detached app must survive launcher death');

    const ownerStop = await command(['stop', '--task', taskId, '--owner', instance.info.ownerToken]);
    assert.equal(ownerStop.code, 1, ownerStop.stdout || ownerStop.stderr);
    assert.match(ownerStop.stdout, /app process group is still live/);
    assert.equal(isolation.pidAlive(appPid), true, 'refused owner stop must leave the orphan untouched');

    contender = isolation.acquireExclusiveSlot(slot, `${taskId}-other-task`);
    console.log(JSON.stringify({
      case: 'live-native-orphan-reservation',
      beforeCrashRefused: !beforeCrash.acquired,
      launcherDead: !isolation.pidAlive(instance.child.pid),
      appAlive: isolation.pidAlive(appPid),
      ownerStopExit: ownerStop.code,
      contenderAcquired: contender.acquired,
    }));
    assert.equal(
      contender.acquired,
      false,
      'a live app must keep its exclusive resource despite launcher death',
    );
  } finally {
    if (contender && contender.acquired) contender.release();
    if (appPid && isolation.pidAlive(appPid)) {
      try { process.kill(-appPid, 'SIGKILL'); } catch (err) { if (err.code !== 'ESRCH') throw err; }
      await waitFor(() => !isolation.pidAlive(appPid), 'owned orphan app exit');
    }
    if (instance && isolation.pidAlive(instance.child.pid)) {
      await command(['stop', '--task', taskId, '--owner', instance.info.ownerToken]);
      await exited(instance.child);
    } else if (instance) {
      await command(['stop', '--task', taskId, '--owner', instance.info.ownerToken]);
    }
    const cleaned = {
      launcherAlive: !!(instance && isolation.pidAlive(instance.child.pid)),
      appAlive: !!(appPid && isolation.pidAlive(appPid)),
      taskRootExists: fs.existsSync(isolation.taskRoot(taskId)),
    };
    console.log(JSON.stringify({ case: 'owned-cleanup', ...cleaned }));
    assert.deepEqual(cleaned, { launcherAlive: false, appAlive: false, taskRootExists: false });
  }
});

test('unknown native process evidence retains the orphan reservation for reconciliation', async () => {
  const taskId = `native-unknown-reservation-${process.pid}`;
  const slot = `gpu-reference-unknown-${process.pid}`;
  let instance;
  let appPid;
  let contender;
  let statusFile;
  let statusBytes;

  try {
    instance = await launch(taskId, slot);
    const active = await waitFor(() => {
      const status = runtime.readNativeStatus(taskId);
      return status && status.state === 'active' && status.childPid && status;
    }, 'active native app');
    appPid = active.childPid;
    await waitFor(() => {
      const read = runtime.readAppManifest(taskId);
      return read && read.valid && read.manifest.pid === appPid;
    }, 'app runtime manifest');

    const beforeCrash = isolation.acquireExclusiveSlot(slot, `${taskId}-before-crash`);
    assert.equal(beforeCrash.acquired, false, 'the live launcher must exclude a contender');

    instance.child.kill('SIGKILL');
    await exited(instance.child);
    assert.equal(isolation.pidAlive(appPid), true, 'the detached app must survive launcher death');

    statusFile = path.join(isolation.taskRoots(taskId).reports, runtime.STATUS_FILE);
    statusBytes = fs.readFileSync(statusFile);
    fs.writeFileSync(statusFile, JSON.stringify({ ...JSON.parse(statusBytes), childPid: null }));
    const ownerStop = await command(['stop', '--task', taskId, '--owner', instance.info.ownerToken]);
    assert.equal(ownerStop.code, 1, ownerStop.stdout || ownerStop.stderr);
    assert.match(ownerStop.stdout, /app process group is unknown/);
    assert.equal(isolation.pidAlive(appPid), true, 'unknown evidence must not signal the actual app');

    contender = isolation.acquireExclusiveSlot(slot, `${taskId}-other-task`);
    console.log(JSON.stringify({
      case: 'unknown-native-orphan-reservation',
      beforeCrashRefused: !beforeCrash.acquired,
      launcherDead: !isolation.pidAlive(instance.child.pid),
      appAlive: isolation.pidAlive(appPid),
      ownerStopExit: ownerStop.code,
      contenderAcquired: contender.acquired,
    }));
    assert.equal(
      contender.acquired,
      false,
      'unknown process evidence must keep the slot unavailable for explicit reconciliation',
    );
  } finally {
    if (contender && contender.acquired) contender.release();
    if (statusBytes && statusFile && fs.existsSync(path.dirname(statusFile))) {
      fs.writeFileSync(statusFile, statusBytes);
    }
    if (appPid && isolation.pidAlive(appPid)) {
      try { process.kill(-appPid, 'SIGKILL'); } catch (err) { if (err.code !== 'ESRCH') throw err; }
      await waitFor(() => !isolation.pidAlive(appPid), 'owned unknown-evidence app exit');
    }
    if (instance && isolation.pidAlive(instance.child.pid)) {
      await command(['stop', '--task', taskId, '--owner', instance.info.ownerToken]);
      await exited(instance.child);
    } else if (instance) {
      await command(['stop', '--task', taskId, '--owner', instance.info.ownerToken]);
    }
    const cleaned = {
      launcherAlive: !!(instance && isolation.pidAlive(instance.child.pid)),
      appAlive: !!(appPid && isolation.pidAlive(appPid)),
      taskRootExists: fs.existsSync(isolation.taskRoot(taskId)),
    };
    console.log(JSON.stringify({ case: 'owned-unknown-cleanup', ...cleaned }));
    assert.deepEqual(cleaned, { launcherAlive: false, appAlive: false, taskRootExists: false });
  }
});
