'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { after, test } = require('node:test');

const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-build-runtime-test-'));
process.env.NEMO_ISOLATION_ROOT = path.join(scratch, 'runtime');
const isolation = require('./lib/isolation.cjs');
const runtime = require('./lib/build-runtime.cjs');
const cli = path.join(__dirname, 'build.cjs');
const repoRoot = path.resolve(__dirname, '..', '..');

const stub = path.join(scratch, 'build-stub');
fs.writeFileSync(stub, `#!/usr/bin/env node\n` + String.raw`
const fs = require('node:fs');
const [capture, delay, exitCode] = process.argv.slice(2);
fs.writeFileSync(capture, JSON.stringify({
  pid: process.pid,
  target: process.env.CARGO_TARGET_DIR,
  temp: process.env.TMPDIR,
  cache: process.env.XDG_CACHE_HOME,
  reports: process.env.NEMO_REPORT_DIR,
  task: process.env.NEMO_TASK_ID,
}));
setTimeout(() => process.exit(Number(exitCode || 0)), Number(delay || 0));
`, { mode: 0o700 });

const stubbornStub = path.join(scratch, 'stubborn-build-stub');
fs.writeFileSync(stubbornStub, `#!/usr/bin/env node\n` + String.raw`
const fs = require('node:fs');
fs.writeFileSync(process.argv[2], String(process.pid));
process.on('SIGTERM', () => {});
setInterval(() => {}, 1000);
`, { mode: 0o700 });

const orphaningStub = path.join(scratch, 'orphaning-build-stub');
fs.writeFileSync(orphaningStub, `#!/usr/bin/env node\n` + String.raw`
const fs = require('node:fs');
const { spawn } = require('node:child_process');
const descendant = spawn(process.execPath, ['-e', "process.on('SIGTERM',()=>{});process.send('ready');setInterval(()=>{},1000)"], {
  stdio: ['ignore', 'ignore', 'ignore', 'ipc'],
});
descendant.once('message', () => {
  fs.writeFileSync(process.argv[2], String(descendant.pid));
  descendant.unref();
  process.exit(0);
});
`, { mode: 0o700 });

after(() => fs.rmSync(scratch, { recursive: true, force: true }));

function exited(child) {
  if (child.exitCode != null || child.signalCode != null) return Promise.resolve({ code: child.exitCode, signal: child.signalCode });
  return new Promise((resolve) => child.once('exit', (code, signal) => resolve({ code, signal })));
}

function firstLine(child) {
  return new Promise((resolve, reject) => {
    let data = ''; const timer = setTimeout(() => done(new Error('launcher readiness timed out')), 10_000);
    function done(error, value) { clearTimeout(timer); child.stdout.off('data', read); child.off('exit', early); error ? reject(error) : resolve(value); }
    function read(chunk) { data += chunk; const newline = data.indexOf('\n'); if (newline !== -1) done(null, data.slice(0, newline)); }
    function early(code) { done(new Error(`launcher exited before readiness (${code}): ${data}`)); }
    child.stdout.on('data', read); child.once('exit', early);
  });
}

async function launch(task, capture, delay = 200, exitCode = 0, executable = stub) {
  const args = executable === stub ? [capture, String(delay), String(exitCode)] : [capture];
  const child = spawn(process.execPath, [cli, 'start', '--task', task, '--command', executable, '--', ...args], {
    cwd: repoRoot,
    env: { ...process.env },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  return { child, info: JSON.parse(await firstLine(child)) };
}

function command(args) {
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [cli, ...args], { cwd: repoRoot, env: { ...process.env }, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = ''; let stderr = '';
    child.stdout.on('data', (chunk) => { stdout += chunk; });
    child.stderr.on('data', (chunk) => { stderr += chunk; });
    child.once('exit', (code, signal) => resolve({ code, signal, stdout, stderr }));
  });
}

async function waitForState(task, state, timeoutMs = 5000) {
  return waitForStates(task, [state], timeoutMs);
}

async function waitForStates(task, states, timeoutMs = 5000) {
  const deadline = Date.now() + timeoutMs;
  let value = null;
  while (Date.now() < deadline) {
    value = runtime.readBuildStatus(task);
    if (value && states.includes(value.state)) return value;
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  throw new Error(`timed out waiting for ${task} state ${states.join(' or ')}; last status ${JSON.stringify(value)}`);
}

async function stopOwned(instance) {
  if (!instance || !isolation.pidAlive(instance.child.pid)) return;
  const result = await command(['stop', '--task', instance.info.taskId, '--owner', instance.info.ownerToken]);
  assert.equal(result.code, 0, result.stderr || result.stdout);
  await exited(instance.child);
}

test('launch plans isolate mutable build paths and serialize only the same worktree', () => {
  const a = runtime.buildLaunchConfig('build-plan-a', { command: stub, args: [] });
  const b = runtime.buildLaunchConfig('build-plan-b', { command: stub, args: [] });
  assert.notEqual(a.targetDir, b.targetDir);
  assert.notEqual(a.roots.reports, b.roots.reports);
  assert.notEqual(a.roots.temp, b.roots.temp);
  assert.notEqual(a.roots.cache, b.roots.cache);
  assert.equal(a.slot, b.slot);
  assert.notEqual(runtime.worktreeBuildSlot(repoRoot), runtime.worktreeBuildSlot(scratch));
  assert.equal(a.env.CARGO_TARGET_DIR, a.targetDir);
  assert.equal(a.env.NEMO_REPORT_DIR, a.roots.reports);
  assert.deepEqual(runtime.tauriBuildArgs('aarch64-test-host'), [
    'build', '--target', 'aarch64-test-host', '-b', 'app', '--no-sign',
  ]);
  assert.throws(() => runtime.tauriBuildArgs(null), /host triple unavailable/);
  assert.throws(() => runtime.assertNativePlatform('win32'), /supports macOS only/);
  assert.throws(() => runtime.assertNativePlatform('linux'), /supports macOS only/);
});

test('active build holds the worktree slot, completion releases it, and artifacts remain owner-addressable', async () => {
  const captureA = path.join(scratch, 'capture-a.json');
  const captureB = path.join(scratch, 'capture-b.json');
  const a = await launch(`build-active-a-${process.pid}`, captureA, 350);
  let b;
  try {
    await waitForState(a.info.taskId, 'active');
    const refused = await command(['start', '--task', `build-active-b-${process.pid}`, '--command', stub, '--', captureB, '1', '0']);
    assert.equal(refused.code, 1);
    assert.match(refused.stderr, /same-worktree desktop build unavailable/);
    assert.equal(fs.existsSync(captureB), false);
    await waitForState(a.info.taskId, 'completed');
    assert.equal(runtime.readBuildStatus(a.info.taskId).slotRelease.released, true);
    assert.equal(isolation.pidAlive(a.child.pid), true, 'completed launcher retains artifacts and owner status');
    b = await launch(`build-after-a-${process.pid}`, captureB, 10);
    await waitForState(b.info.taskId, 'completed');
    const envA = JSON.parse(fs.readFileSync(captureA));
    const envB = JSON.parse(fs.readFileSync(captureB));
    assert.equal(envA.target, a.info.targetDir);
    assert.equal(envA.reports, a.info.roots.reports);
    assert.notEqual(envA.target, envB.target);
    assert.notEqual(envA.temp, envB.temp);
  } finally {
    await stopOwned(a);
    await stopOwned(b);
  }
});

test('status and stop require the owner and preserve peer launchers', async () => {
  const capture = path.join(scratch, 'capture-owner.json');
  const instance = await launch(`build-owner-${process.pid}`, capture, 20);
  try {
    await waitForState(instance.info.taskId, 'completed');
    const badStatus = await command(['status', '--task', instance.info.taskId, '--owner', 'wrong-token']);
    assert.equal(badStatus.code, 1);
    assert.match(JSON.parse(badStatus.stdout).reason, /owner token mismatch/);
    const badStop = await command(['stop', '--task', instance.info.taskId, '--owner', 'wrong-token']);
    assert.equal(badStop.code, 1);
    assert.equal(isolation.pidAlive(instance.child.pid), true);
    const goodStatus = await command(['status', '--task', instance.info.taskId, '--owner', instance.info.ownerToken]);
    const body = JSON.parse(goodStatus.stdout);
    assert.equal(goodStatus.code, 0);
    assert.equal(body.ok, true);
    assert.equal(body.runtime.state, 'completed');
    await stopOwned(instance);
    assert.equal(fs.existsSync(instance.info.roots.root), false);
  } finally {
    if (isolation.pidAlive(instance.child.pid)) await stopOwned(instance);
  }
});

test('owner stop reaps an active stubborn build group before releasing roots', {
  skip: process.platform === 'win32' ? 'POSIX process-group termination coverage' : false,
}, async () => {
  const capture = path.join(scratch, 'capture-stubborn.pid');
  const child = spawn(process.execPath, [cli, 'start', '--task', `build-stubborn-${process.pid}`, '--command', stubbornStub, '--', capture], {
    cwd: repoRoot,
    env: { ...process.env },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  const instance = { child, info: JSON.parse(await firstLine(child)) };
  try {
    await waitForState(instance.info.taskId, 'active');
    for (let i = 0; i < 100 && !fs.existsSync(capture); i++) await new Promise((resolve) => setTimeout(resolve, 10));
    const buildPid = Number(fs.readFileSync(capture, 'utf8'));
    assert.equal(isolation.pidAlive(buildPid), true);
    const stopped = await command(['stop', '--task', instance.info.taskId, '--owner', instance.info.ownerToken]);
    assert.equal(stopped.code, 0, stopped.stderr || stopped.stdout);
    assert.equal(JSON.parse(stopped.stdout).runtime.state, 'stopped');
    await exited(child);
    assert.equal(isolation.pidAlive(buildPid), false);
    assert.equal(fs.existsSync(instance.info.roots.root), false);
  } finally {
    if (isolation.pidAlive(child.pid)) await stopOwned(instance);
  }
});

test('leader exit cannot release the slot while its stubborn descendant remains', {
  skip: process.platform === 'win32' ? 'POSIX process-group termination coverage' : false,
}, async () => {
  const capture = path.join(scratch, 'capture-descendant.pid');
  const orphaning = await launch(`build-orphan-${process.pid}`, capture, 0, 0, orphaningStub);
  try {
    for (let i = 0; i < 100 && !fs.existsSync(capture); i++) await new Promise((resolve) => setTimeout(resolve, 10));
    const descendantPid = Number(fs.readFileSync(capture, 'utf8'));
    const status = await waitForStates(orphaning.info.taskId, ['failed', 'reconciliation-required'], 8000);
    assert.equal(status.exitCode, 0);
    assert.equal(status.processTree.forced, true);
    if (status.state === 'failed') {
      assert.equal(status.slotRelease.released, true);
      assert.equal(isolation.pidAlive(descendantPid), false);
    } else {
      assert.equal(status.slotRelease, undefined);
      const refused = await command(['start', '--task', `build-orphan-peer-${process.pid}`, '--command', stub, '--', path.join(scratch, 'capture-orphan-peer'), '1', '0']);
      assert.equal(refused.code, 1);
      assert.match(refused.stderr, /same-worktree desktop build unavailable/);
      const deadline = Date.now() + 5000;
      while (runtime.buildProcessTreeAlive({ pid: status.childPid }) && Date.now() < deadline) {
        await new Promise((resolve) => setTimeout(resolve, 20));
      }
      assert.equal(runtime.buildProcessTreeAlive({ pid: status.childPid }), false);
    }
  } finally { await stopOwned(orphaning); }
});

test('nonzero child result remains inspectable until owner cleanup', async () => {
  const capture = path.join(scratch, 'capture-failure.json');
  const instance = await launch(`build-failure-${process.pid}`, capture, 10, 17);
  try {
    const status = await waitForState(instance.info.taskId, 'failed');
    assert.equal(status.exitCode, 17);
    assert.equal(isolation.pidAlive(instance.child.pid), true);
    assert.equal(fs.existsSync(status.logs.stdout), true);
    assert.equal(fs.existsSync(status.logs.stderr), true);
  } finally { await stopOwned(instance); }
});
