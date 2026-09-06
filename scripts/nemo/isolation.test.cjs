'use strict';
// Behavioral tests for R06 task-runtime isolation (scripts/nemo/lib/isolation.cjs).
// Included in `npm test` / `verify` through tests/nemo-isolation.test.cjs.
// Run this suite directly:
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
  identity.sourceIdentity = () => ({ ...launcher.source, head: 'deadbeefdeadbeefdeadbeefdeadbeefdeadbeef' });
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

    const refused = await iso.requestStop(taskId, 'wrong-token');
    assert.equal(refused.stopped, false);
    assert.match(refused.reason, /not the task owner/);
    assert.equal(iso.pidAlive(child.pid), true, 'child must survive a non-owner stop attempt');

    const stopped = await iso.requestStop(taskId, launcher.ownerToken);
    assert.equal(stopped.stopped, true);
    await waitForExit(child, 5000);
    assert.equal(iso.pidAlive(child.pid), false, 'child must be gone after an owner stop');

    assert.equal(iso.readLauncher(taskId), null, 'launcher record is cleared once stopped');
  } finally {
    try { process.kill(child.pid, 'SIGKILL'); } catch { /* already gone */ }
  }
});

test('requestStop on a task with no launcher record names the reason', async () => {
  const result = await iso.requestStop('never-registered-' + process.pid, 'whatever');
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
      const crossStop = await iso.requestStop(taskA, launcherB.ownerToken);
      assert.equal(crossStop.stopped, false);
      assert.equal(iso.pidAlive(childA.pid), true);

      // Stopping B must not disturb A at all.
      const stopB = await iso.requestStop(taskB, launcherB.ownerToken);
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

test('task IDs cannot traverse, normalize aliases, truncate entropy, or collide by filename case', () => {
  for (const id of ['.', '..', '', 'task/a', 'task?a', 'a'.repeat(121)]) {
    assert.throws(() => iso.taskRoot(id), /invalid task\/slot id/);
  }
  const upper = iso.taskRoots('CaseTask');
  const lower = iso.taskRoots('casetask');
  fs.writeFileSync(path.join(upper.temp, 'owner'), 'upper');
  assert.equal(fs.existsSync(path.join(lower.temp, 'owner')), false);
  assert.notEqual(iso.taskRoot('slots'), path.join(scratchRoot, 'slots'));
  const real = identity.sourceIdentity;
  identity.sourceIdentity = () => ({ worktree: '/tmp/' + 'w'.repeat(240) });
  try { assert.notEqual(iso.resolveTaskId(), iso.resolveTaskId()); }
  finally { identity.sourceIdentity = real; }
});

test('a second launcher cannot replace the live owner or release its roots', async () => {
  const child = spawnDummyProcess();
  const task = 'duplicate-owner';
  try {
    const first = iso.registerLauncher(task, { pid: child.pid });
    assert.throws(() => iso.registerLauncher(task), /already registered/);
    assert.equal(iso.readLauncher(task).ownerToken, first.ownerToken);
    assert.equal(iso.releaseTask(task, 'different-owner').released, false);
    assert.equal(iso.releaseTask(task, first.ownerToken).released, false);
    assert.equal((await iso.requestStop(task, first.ownerToken)).stopped, true);
    assert.equal(iso.releaseTask(task, first.ownerToken).released, true);
  } finally { child.kill('SIGKILL'); await waitForExit(child, 5000); }
});

test('full source comparison rejects same-HEAD dirty/worktree changes and unavailable identity', () => {
  const task = 'full-source';
  const launcher = iso.registerLauncher(task);
  const real = identity.sourceIdentity;
  try {
    for (const change of [
      { worktree: launcher.source.worktree + '-other' },
      { dirty: true, dirtyDigest: 'different-bytes' },
      { branch: launcher.source.branch + '-other' },
    ]) {
      identity.sourceIdentity = () => ({ ...launcher.source, ...change });
      assert.equal(iso.verifyHandshake(task, { ownerToken: launcher.ownerToken, checkSource: true }).ok, false);
    }
    identity.sourceIdentity = () => { throw new Error('git unavailable'); };
    assert.equal(iso.verifyHandshake(task, { ownerToken: launcher.ownerToken, checkSource: true }).ok, false);
    assert.throws(() => iso.registerLauncher('no-source'), /source identity unavailable/);
    assert.equal(iso.readLauncher('no-source'), null);
  } finally { identity.sourceIdentity = real; }
  assert.equal(iso.verifyHandshake(task, { checkSource: true }).ok, false, 'owner token is required');
});

test('ignored stop retains owner authority for a later confirmed termination', async () => {
  const child = spawn(process.execPath, ['-e', "process.on('SIGTERM', () => {}); process.send('ready'); setInterval(() => {}, 1000)"], { stdio: ['ignore', 'ignore', 'inherit', 'ipc'] });
  try {
    await message(child);
    const task = 'ignored-stop';
    const launcher = iso.registerLauncher(task, { pid: child.pid });
    const result = await iso.requestStop(task, launcher.ownerToken, { timeoutMs: 50 });
    assert.equal(result.stopped, false);
    assert.match(result.reason, /owner record retained/);
    assert.equal(iso.pidAlive(child.pid), true);
    assert.equal(iso.readLauncher(task).ownerToken, launcher.ownerToken);
    assert.equal(iso.releaseTask(task, launcher.ownerToken).released, false);
    assert.equal((await iso.requestStop(task, launcher.ownerToken, { signal: 'SIGKILL' })).stopped, true);
    assert.equal(iso.readLauncher(task), null);
  } finally { child.kill('SIGKILL'); await waitForExit(child, 5000); }
});

function message(child) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { cleanup(); reject(new Error('worker message timed out')); }, 5000);
    function cleanup() { clearTimeout(timer); child.off('message', receive); child.off('exit', exited); }
    function receive(value) { cleanup(); resolve(value); }
    function exited(code) { cleanup(); reject(new Error('worker exited before result: ' + code)); }
    child.once('message', receive);
    child.once('exit', exited);
  });
}

function worker(code, args = []) {
  return spawn(process.execPath, ['-e', code, ...args], { env: process.env, stdio: ['ignore', 'ignore', 'inherit', 'ipc'] });
}

// Pause one contender after its stale read. The other process must be refused
// while that mutation is in progress, then refused by the new live owner.
test('two processes cannot both reclaim the same stale exclusive slot', async () => {
  const dead = spawnDummyProcess();
  const held = iso.acquireExclusiveSlot('stale-race', 'dead-holder', { pid: dead.pid });
  dead.kill('SIGKILL');
  await waitForExit(dead, 5000);
  const gate = path.join(scratchRoot, 'resume-slot');
  const code = String.raw`
    const fs = require('node:fs');
    const [role, modulePath, lockFile, gate] = process.argv.slice(1);
    if (role === 'A') {
      const read = fs.readFileSync; let paused = false;
      fs.readFileSync = function(file, ...args) {
        const bytes = read.call(this, file, ...args);
        if (file === lockFile && !paused) {
          paused = true; process.send('paused');
          const deadline = Date.now() + 4000;
          while (!fs.existsSync(gate) && Date.now() < deadline) Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 10);
          if (!fs.existsSync(gate)) throw new Error('gate timed out');
        }
        return bytes;
      };
    }
    const iso = require(modulePath);
    process.send({ acquired: iso.acquireExclusiveSlot('stale-race', role).acquired });
    setInterval(() => {}, 1000);
  `;
  const a = worker(code, ['A', require.resolve('./lib/isolation.cjs'), held.file, gate]);
  let b;
  try {
    assert.equal(await message(a), 'paused');
    b = worker(code, ['B', require.resolve('./lib/isolation.cjs'), held.file, gate]);
    assert.equal((await message(b)).acquired, false);
    const resumed = message(a);
    fs.writeFileSync(gate, 'go');
    assert.equal((await resumed).acquired, true);
    assert.equal(iso.acquireExclusiveSlot('stale-race', 'third').acquired, false);
    assert.equal(JSON.parse(fs.readFileSync(held.file)).taskId, 'A');
  } finally {
    for (const child of [a, b].filter(Boolean)) { child.kill('SIGKILL'); await waitForExit(child, 5000); }
  }
});

test('simultaneous launcher registrations have exactly one persistent owner', async () => {
  const code = String.raw`
    const iso = require(process.argv[1]);
    process.send('ready');
    process.once('message', () => {
      try { const rec = iso.registerLauncher('registration-race'); process.send({ registered: true, token: rec.ownerToken }); }
      catch (err) { process.send({ registered: false, reason: err.message }); }
    });
    setInterval(() => {}, 1000);
  `;
  const children = [worker(code, [require.resolve('./lib/isolation.cjs')]), worker(code, [require.resolve('./lib/isolation.cjs')])];
  try {
    await Promise.all(children.map(message));
    const replies = children.map(message);
    children.forEach(child => child.send('go'));
    const results = await Promise.all(replies);
    const winners = results.filter(result => result.registered);
    assert.equal(winners.length, 1);
    assert.equal(iso.readLauncher('registration-race').ownerToken, winners[0].token);
  } finally {
    for (const child of children) { child.kill('SIGKILL'); await waitForExit(child, 5000); }
  }
});

// ---- dead-task slot residue (slot records live outside every task root) ----

// SIGKILL and wait until the pid is really gone, so a record naming it is
// unambiguously dead rather than merely unresponsive.
async function reap(child) {
  child.kill('SIGKILL');
  await waitForExit(child, 5000);
  const deadline = Date.now() + 2000;
  while (iso.pidAlive(child.pid) && Date.now() < deadline) await new Promise((resolve) => setTimeout(resolve, 20));
  assert.equal(iso.pidAlive(child.pid), false);
}

// Take the slot legitimately (acquire refuses a dead pid, by design), then kill
// the holder: the residue an interrupted run actually leaves behind.
async function slotHeldByDeadPid(slot, task) {
  const child = spawnDummyProcess();
  const held = iso.acquireExclusiveSlot(slot, task, { pid: child.pid });
  assert.equal(held.acquired, true);
  await reap(child);
  return held;
}

// A launcher registered against a process that is then killed: the "killed
// launcher" state an authorized release has to clean up after.
async function killedLauncher(task) {
  const child = spawnDummyProcess();
  const launcher = iso.registerLauncher(task, { pid: child.pid, label: 'killed-launcher' });
  await reap(child);
  return launcher;
}

test('killed launcher: the rightful owner reclaims its own dead slots and no one else\'s', async () => {
  const task = 'slot-residue-own';
  const launcher = await killedLauncher(task);
  const mine = await slotHeldByDeadPid('residue-mine', task);
  const theirs = await slotHeldByDeadPid('residue-theirs', 'slot-residue-other');

  const released = iso.releaseTask(task, launcher.ownerToken);
  assert.equal(released.released, true);
  assert.deepEqual(released.slots.reconciled, ['residue-mine']);
  assert.deepEqual(released.slots.retained, []);
  assert.ok(released.slots.otherTasks >= 1, 'the other task\'s record must be counted, never inspected further');
  assert.equal(fs.existsSync(mine.file), false, 'the dead task\'s own slot record must be gone');
  assert.equal(fs.existsSync(theirs.file), true, 'another task\'s record must survive untouched');
  assert.equal(JSON.parse(fs.readFileSync(theirs.file, 'utf8')).taskId, 'slot-residue-other');
});

test('release leaves live, malformed and self-inconsistent slot records exactly as found', async () => {
  const task = 'slot-residue-retain';
  const launcher = await killedLauncher(task);
  const live = spawnDummyProcess();
  try {
    const held = iso.acquireExclusiveSlot('retain-live', task, { pid: live.pid });
    const bad = await slotHeldByDeadPid('retain-malformed', task);
    const wrongName = await slotHeldByDeadPid('retain-mismatch', task);
    fs.writeFileSync(bad.file, JSON.stringify({ ...JSON.parse(fs.readFileSync(bad.file, 'utf8')), pid: 'not-a-pid' }));
    fs.writeFileSync(wrongName.file, JSON.stringify({ ...JSON.parse(fs.readFileSync(wrongName.file, 'utf8')), slot: 'some-other-slot' }));

    const released = iso.releaseTask(task, launcher.ownerToken);
    assert.equal(released.released, true);
    assert.deepEqual(released.slots.reconciled, []);
    assert.equal(released.slots.retained.length, 3);
    assert.match(released.slots.retained.map((r) => r.reason).join('|'), /holder still running/);
    assert.match(released.slots.retained.map((r) => r.reason).join('|'), /malformed holder pid/);
    assert.match(released.slots.retained.map((r) => r.reason).join('|'), /does not match its own filename/);
    for (const file of [held.file, bad.file, wrongName.file]) assert.equal(fs.existsSync(file), true);
    assert.equal(iso.acquireExclusiveSlot('retain-live', 'someone-else').acquired, false, 'a live holder keeps its slot');
  } finally { live.kill('SIGKILL'); await waitForExit(live, 5000); }
});

test('a refused release reconciles nothing', async () => {
  const task = 'slot-residue-refused';
  const launcher = await killedLauncher(task);
  const slot = await slotHeldByDeadPid('refused-slot', task);

  const nonOwner = iso.releaseTask(task, 'not-the-owner-token');
  assert.equal(nonOwner.released, false);
  assert.equal(nonOwner.slots, undefined);
  assert.equal(fs.existsSync(slot.file), true, 'a non-owner must not reclaim the task\'s slots');

  assert.deepEqual(iso.releaseTask(task, launcher.ownerToken).slots.reconciled, ['refused-slot']);
});

test('reconciliation never bypasses an interrupted slot mutation guard', async () => {
  const task = 'slot-residue-guarded';
  const launcher = await killedLauncher(task);
  const slot = await slotHeldByDeadPid('guarded-slot', task);

  // Leave that one slot's guard behind exactly as a crashed mutation would.
  const code = String.raw`
    const fs = require('node:fs'); const write = fs.writeFileSync;
    fs.writeFileSync = function(file, ...args) {
      const result = write.call(this, file, ...args);
      if (String(file).endsWith('owner.json')) { process.send('guard-held'); Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 4000); }
      return result;
    };
    require(process.argv[1]).acquireExclusiveSlot('guarded-slot', 'contender');
  `;
  const child = worker(code, [require.resolve('./lib/isolation.cjs')]);
  try { assert.equal(await message(child), 'guard-held'); }
  finally { child.kill('SIGKILL'); await waitForExit(child, 5000); }

  const released = iso.releaseTask(task, launcher.ownerToken);
  assert.equal(released.released, true, 'a wedged slot guard must not block releasing the task itself');
  assert.deepEqual(released.slots.reconciled, []);
  assert.deepEqual(released.slots.retained, [{ slot: 'guarded-slot', reason: 'slot mutation busy or interrupted; left for explicit reconciliation' }]);
  assert.equal(fs.existsSync(slot.file), true);
});

test('an interrupted mutation stays closed until explicit reconciliation', async () => {
  const code = String.raw`
    const fs = require('node:fs'); const write = fs.writeFileSync;
    fs.writeFileSync = function(file, ...args) {
      const result = write.call(this, file, ...args);
      if (String(file).endsWith('owner.json')) { process.send('guard-held'); Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 4000); }
      return result;
    };
    require(process.argv[1]).acquireExclusiveSlot('interrupted-slot', 'crashed');
  `;
  const child = worker(code, [require.resolve('./lib/isolation.cjs')]);
  try { assert.equal(await message(child), 'guard-held'); }
  finally { child.kill('SIGKILL'); await waitForExit(child, 5000); }
  const attempt = iso.acquireExclusiveSlot('interrupted-slot', 'next');
  assert.equal(attempt.acquired, false);
  assert.match(attempt.reason, /busy or interrupted/);
});
