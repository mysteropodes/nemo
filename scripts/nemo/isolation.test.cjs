'use strict';
// Behavioral tests for R06 task-runtime isolation (scripts/nemo/lib/isolation.cjs).
// Not wired into `npm test` (tests/*.test.cjs glob in package.json, out of
// scope for this increment — see engineering/runtime-isolation.md). Run
// directly:
//   node --test scripts/nemo/isolation.test.cjs
//
// NEMO_ISOLATION_ROOT is redirected to a throwaway tmp dir *before*
// requiring the library, since the library reads it once at module load.
const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const net = require('node:net');
const { spawn } = require('node:child_process');

const scratchRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-isolation-test-'));
process.env.NEMO_ISOLATION_ROOT = scratchRoot;

const iso = require('./lib/isolation.cjs');
const identity = require('./lib/identity.cjs');
const receipt = require('./lib/receipt.cjs');

test.after(() => {
  fs.rmSync(scratchRoot, { recursive: true, force: true });
});

function connectable(port, host) {
  return new Promise((resolve) => {
    const sock = net.connect({ port, host }, () => { sock.destroy(); resolve(true); });
    sock.once('error', () => resolve(false));
  });
}

// Keep a real, indefinitely-alive child process to stand in for "a task's
// owned process" without touching anything outside this test.
function spawnDummyProcess() {
  return spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { stdio: 'ignore' });
}

function waitForExit(child, timeoutMs) {
  return new Promise((resolve, reject) => {
    if (child.exitCode !== null || child.signalCode !== null) { resolve(); return; }
    const t = setTimeout(() => reject(new Error('process did not exit in time')), timeoutMs);
    child.once('exit', () => { clearTimeout(t); resolve(); });
  });
}

test('resolveTaskId is unique per call without an explicit id', () => {
  const a = iso.resolveTaskId();
  const b = iso.resolveTaskId();
  assert.notEqual(a, b);
});

test('taskRoots: two tasks get non-overlapping, existing directory trees', () => {
  const a = iso.taskRoots('task-a-' + process.pid);
  const b = iso.taskRoots('task-b-' + process.pid);
  const aPaths = Object.values(a);
  const bPaths = Object.values(b);
  for (const p of aPaths) {
    assert.ok(fs.statSync(p).isDirectory(), `${p} should exist`);
    assert.ok(!bPaths.includes(p), `${p} leaked into task b's roots`);
  }
  assert.ok(a.root !== b.root);
  assert.ok(path.relative(a.root, b.root).startsWith('..'), 'task b root must not be nested under task a root');
});

test('reports root threads through R02 receipt.cjs (read-only) without editing it', () => {
  const saved = process.env.NEMO_REPORT_DIR;
  try {
    const a = iso.taskRoots('report-task-a-' + process.pid);
    const b = iso.taskRoots('report-task-b-' + process.pid);

    process.env.NEMO_REPORT_DIR = a.reports;
    const dirA = receipt.reportDir(receipt.create('test'));
    process.env.NEMO_REPORT_DIR = b.reports;
    const dirB = receipt.reportDir(receipt.create('test'));

    assert.ok(dirA.startsWith(a.reports + path.sep) || dirA === a.reports);
    assert.ok(dirB.startsWith(b.reports + path.sep) || dirB === b.reports);
    assert.notEqual(dirA, dirB);
  } finally {
    if (saved === undefined) delete process.env.NEMO_REPORT_DIR;
    else process.env.NEMO_REPORT_DIR = saved;
  }
});

test('reservePort: two different tasks get distinct, simultaneously-bound ports', async () => {
  const a = await iso.reservePort('port-task-a-' + process.pid);
  const b = await iso.reservePort('port-task-b-' + process.pid);
  try {
    assert.notEqual(a.port, b.port);
    assert.equal(await connectable(a.port, a.host), true);
    assert.equal(await connectable(b.port, b.host), true);
  } finally {
    await a.release();
    await b.release();
  }
});

test('reservePort: colliding requests for the same task id self-resolve to different ports', async () => {
  const taskId = 'port-collide-' + process.pid;
  const first = await iso.reservePort(taskId);
  try {
    const second = await iso.reservePort(taskId); // deterministic hash would pick the same starting port
    try {
      assert.notEqual(first.port, second.port, 'second reservation must probe past the already-bound port');
      assert.equal(await connectable(second.port, second.host), true);
    } finally {
      await second.release();
    }
  } finally {
    await first.release();
  }
});

test('reservePort: a released port can be reused (proves it is not held after release)', async () => {
  const taskId = 'port-reuse-' + process.pid;
  const first = await iso.reservePort(taskId);
  const p = first.port;
  await first.release();
  assert.equal(await connectable(p, first.host), false, 'port must actually be free after release');
});

test('owner/source handshake: correct token verifies, wrong token is refused', () => {
  const taskId = 'handshake-' + process.pid;
  const launcher = iso.registerLauncher(taskId, { pid: process.pid, label: 'unit-test' });

  const ok = iso.verifyHandshake(taskId, { ownerToken: launcher.ownerToken });
  assert.equal(ok.ok, true);

  const bad = iso.verifyHandshake(taskId, { ownerToken: 'not-the-real-token' });
  assert.equal(bad.ok, false);
  assert.match(bad.reason, /owner token mismatch/);

  const unknown = iso.verifyHandshake('no-such-task-' + process.pid);
  assert.equal(unknown.ok, false);
  assert.match(unknown.reason, /no launcher record/);
});

test('owner/source handshake: checkSource flags a launcher recorded against a different commit', () => {
  const taskId = 'handshake-source-' + process.pid;
  const launcher = iso.registerLauncher(taskId, { pid: process.pid });
  assert.ok(launcher.source.head, 'this repo is a git checkout; head should resolve');

  const real = identity.sourceIdentity;
  identity.sourceIdentity = () => ({ head: 'deadbeefdeadbeefdeadbeefdeadbeefdeadbeef' });
  try {
    const moved = iso.verifyHandshake(taskId, { ownerToken: launcher.ownerToken, checkSource: true });
    assert.equal(moved.ok, false);
    assert.match(moved.reason, /source moved/);
  } finally {
    identity.sourceIdentity = real;
  }

  const same = iso.verifyHandshake(taskId, { ownerToken: launcher.ownerToken, checkSource: true });
  assert.equal(same.ok, true);
});

test('non-owner stop is refused; owner stop actually terminates the process', async () => {
  const taskId = 'stop-' + process.pid;
  const child = spawnDummyProcess();
  try {
    const launcher = iso.registerLauncher(taskId, { pid: child.pid, label: 'dummy-owned-process' });

    const refused = iso.requestStop(taskId, 'wrong-token');
    assert.equal(refused.stopped, false);
    assert.match(refused.reason, /not the task owner/);
    assert.equal(iso.pidAlive(child.pid), true, 'child must survive a non-owner stop attempt');

    const stopped = iso.requestStop(taskId, launcher.ownerToken);
    assert.equal(stopped.stopped, true);
    await waitForExit(child, 5000);
    assert.equal(iso.pidAlive(child.pid), false, 'child must be gone after an owner stop');

    assert.equal(iso.readLauncher(taskId), null, 'launcher record is cleared once stopped');
  } finally {
    try { process.kill(child.pid, 'SIGKILL'); } catch { /* already gone */ }
  }
});

test('requestStop on a task with no launcher record names the reason', () => {
  const result = iso.requestStop('never-registered-' + process.pid, 'whatever');
  assert.equal(result.stopped, false);
  assert.match(result.reason, /no launcher record/);
});

test('exclusive slots: second acquire is refused until the owner releases; non-owner release is refused', () => {
  const slot = 'desktop-input-' + process.pid;
  const first = iso.acquireExclusiveSlot(slot, 'task-a-' + process.pid);
  assert.equal(first.acquired, true);

  const second = iso.acquireExclusiveSlot(slot, 'task-b-' + process.pid);
  assert.equal(second.acquired, false);
  assert.equal(second.holder.taskId, 'task-a-' + process.pid);

  const badRelease = iso.releaseExclusiveSlot(slot, 'wrong-token');
  assert.equal(badRelease.released, false);

  const goodRelease = iso.releaseExclusiveSlot(slot, first.ownerToken);
  assert.equal(goodRelease.released, true);

  const third = iso.acquireExclusiveSlot(slot, 'task-b-' + process.pid);
  assert.equal(third.acquired, true, 'slot must be acquirable again once released');
  iso.releaseExclusiveSlot(slot, third.ownerToken);
});

test('exclusive slots: a lock left by a dead process is reclaimed automatically', async () => {
  const slot = 'stale-' + process.pid;
  const dead = spawnDummyProcess();
  const held = iso.acquireExclusiveSlot(slot, 'task-dead-' + process.pid, { pid: dead.pid });
  assert.equal(held.acquired, true);
  process.kill(dead.pid, 'SIGKILL');
  await waitForExit(dead, 5000);

  // A busy (non-yielding) wait here would starve libuv's SIGCHLD handling and
  // the pid would never actually get reaped, so poll with real setTimeouts.
  const deadline = Date.now() + 2000;
  while (iso.pidAlive(dead.pid) && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 20));
  }

  const reclaimed = iso.acquireExclusiveSlot(slot, 'task-live-' + process.pid);
  assert.equal(reclaimed.acquired, true, 'a stale lock from a dead pid must not wedge the slot');
  iso.releaseExclusiveSlot(slot, reclaimed.ownerToken);
});

test('two full task instances run concurrently with zero shared state and independent stop authority', async () => {
  const taskA = 'full-a-' + process.pid;
  const taskB = 'full-b-' + process.pid;
  const childA = spawnDummyProcess();
  const childB = spawnDummyProcess();
  try {
    const rootsA = iso.taskRoots(taskA);
    const rootsB = iso.taskRoots(taskB);
    const portA = await iso.reservePort(taskA);
    const portB = await iso.reservePort(taskB);
    const launcherA = iso.registerLauncher(taskA, { pid: childA.pid });
    const launcherB = iso.registerLauncher(taskB, { pid: childB.pid });

    try {
      assert.notEqual(portA.port, portB.port);
      for (const key of Object.keys(rootsA)) assert.notEqual(rootsA[key], rootsB[key]);
      assert.notEqual(launcherA.ownerToken, launcherB.ownerToken);

      // B's owner token must not authorize stopping A.
      const crossStop = iso.requestStop(taskA, launcherB.ownerToken);
      assert.equal(crossStop.stopped, false);
      assert.equal(iso.pidAlive(childA.pid), true);

      // Stopping B must not disturb A at all.
      const stopB = iso.requestStop(taskB, launcherB.ownerToken);
      assert.equal(stopB.stopped, true);
      await waitForExit(childB, 5000);
      assert.equal(iso.verifyHandshake(taskA, { ownerToken: launcherA.ownerToken }).ok, true);
    } finally {
      await portA.release();
      await portB.release();
    }
  } finally {
    for (const c of [childA, childB]) { try { process.kill(c.pid, 'SIGKILL'); } catch { /* already gone */ } }
  }
});
