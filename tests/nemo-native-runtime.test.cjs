'use strict';

// Keep this process-isolated: the suite sets NEMO_ISOLATION_ROOT before the
// isolation module captures it at import time.
require('../scripts/nemo/native-runtime.test.cjs');

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawn, execFileSync } = require('node:child_process');
const { test } = require('node:test');
const isolation = require('../scripts/nemo/lib/isolation.cjs');
const runtime = require('../scripts/nemo/lib/native-runtime.cjs');
const buildRuntime = require('../scripts/nemo/lib/build-runtime.cjs');

async function waitFor(check, description, timeoutMs = 5000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const value = check();
    if (value) return value;
    await new Promise(resolve => setTimeout(resolve, 20));
  }
  assert.fail(`timed out waiting for ${description}`);
}

// The helper inherits BOTH app pipes and deliberately ignores SIGTERM. This
// keeps 'close' pending after the leader exits and makes the held-reservation
// window observable before process-group cleanup escalates to SIGKILL.
const helperProgram = String.raw`
const fs = require('node:fs');
process.on('SIGTERM', () => {
  fs.writeFileSync(process.argv[1], 'SIGTERM');
  process.stdout.write('helper cleanup stdout\n');
  process.stderr.write('helper cleanup stderr\n');
});
process.send('ready');
setInterval(() => {}, 1000);
`;
const leaderProgram = String.raw`
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');
const [capture, exitGate, termCapture, mode, helperProgram] = process.argv.slice(1);
const helper = spawn(process.execPath, ['-e', helperProgram, termCapture], {
  stdio: ['ignore', 'inherit', 'inherit', 'ipc'],
});
helper.once('message', () => {
  if (mode === 'manifest') {
    fs.writeFileSync(path.join(process.env.NEMO_TAURI_DATA_DIR, 'native-runtime.json'), JSON.stringify({
      schema: 'nemo.native-runtime/1', taskId: process.env.NEMO_TASK_ID,
      isolated: true, state: 'active', pid: process.pid,
    }));
  }
  fs.writeFileSync(capture + '.tmp', JSON.stringify({ leader: process.pid, helper: helper.pid }));
  fs.renameSync(capture + '.tmp', capture);
  setInterval(() => {
    if (fs.existsSync(exitGate)) process.exit(mode === 'manifest' ? 0 : 17);
  }, 20);
});
`;

for (const mode of ['manifest', 'no-manifest']) {
  test(`leader exit cleans inherited-pipe helper and releases native reservation (${mode})`, {
    skip: process.platform === 'win32' ? 'POSIX process-group termination coverage' : false,
  }, async () => {
    const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-native-leader-test-'));
    const task = `native-leader-${mode}-${process.pid}`;
    const slot = `${task}-slot`;
    const capture = path.join(scratch, 'processes.json');
    const exitGate = path.join(scratch, 'exit');
    const termCapture = path.join(scratch, 'helper-signaled');
    const cli = path.resolve(__dirname, '../scripts/nemo/native.cjs');
    const launcher = spawn(process.execPath, [cli, 'start', '--task', task,
      '--executable', process.execPath, '--reserve', slot, '--manifest-timeout-ms', '15000',
      '--', '-e', leaderProgram, capture, exitGate, termCapture, mode, helperProgram], {
      env: { ...process.env }, stdio: ['ignore', 'pipe', 'pipe'],
    });
    launcher.stdout.resume();
    let stderr = '';
    launcher.stderr.on('data', chunk => { stderr += chunk; });
    let pids;
    try {
      pids = await waitFor(() => fs.existsSync(capture) && JSON.parse(fs.readFileSync(capture, 'utf8')), 'helper readiness');
      assert.equal(runtime.readNativeStatus(task).childPid, pids.leader);
      const group = Number(execFileSync('ps', ['-p', String(pids.helper), '-o', 'pgid='], { encoding: 'utf8' }).trim());
      assert.equal(group, pids.leader, 'helper belongs to the launcher-owned app process group');
      const slotFile = path.join(isolation.RUNTIME_ROOT, 'slots', isolation.idKey(slot) + '.lock');
      assert.equal(fs.existsSync(slotFile), true, 'running app holds the reservation');
      if (mode === 'manifest') {
        await waitFor(() => runtime.readNativeStatus(task).appRuntime, 'accepted app manifest');
      }

      fs.writeFileSync(exitGate, 'exit');
      await waitFor(() => !isolation.pidAlive(pids.leader), 'app leader exit');
      assert.equal(isolation.pidAlive(pids.helper), true, 'inherited pipes outlive the app leader');
      await waitFor(() => fs.existsSync(termCapture), 'automatic helper cleanup after leader exit');
      assert.equal(isolation.pidAlive(pids.helper), true, 'stubborn helper remains during the cleanup grace period');
      assert.equal(fs.existsSync(slotFile), true, 'reservation stays held until the owned group exits');
      const contender = isolation.acquireExclusiveSlot(slot, `${task}-contender`);
      try { assert.equal(contender.acquired, false, 'another owner cannot acquire the slot during cleanup'); }
      finally { if (contender.acquired) contender.release(); }

      await waitFor(() => launcher.exitCode !== null || launcher.signalCode !== null, 'launcher exit after helper cleanup');
      assert.equal(launcher.exitCode, 0, stderr);
      assert.equal(isolation.pidAlive(pids.helper), false);
      assert.equal(buildRuntime.buildProcessTreeAlive({ pid: pids.leader }), false);
      const status = runtime.readNativeStatus(task);
      assert.equal(status.state, mode === 'manifest' ? 'exited' : 'failed');
      assert.equal(status.exitCode, mode === 'manifest' ? 0 : 17);
      assert.equal(status.processTree.stopped, true);
      assert.equal(status.processTree.forced, true);
      assert.deepEqual(status.slotRelease, [{ slot, released: true }]);
      assert.equal(fs.existsSync(slotFile), false, 'launcher releases the slot without dead-holder reclamation');
      assert.match(fs.readFileSync(status.logs.stdout, 'utf8'), /helper cleanup stdout/);
      assert.match(fs.readFileSync(status.logs.stderr, 'utf8'), /helper cleanup stderr/);
      if (mode === 'no-manifest') assert.match(status.appRuntimeError, /did not write its runtime manifest/);
      const next = isolation.acquireExclusiveSlot(slot, `${task}-next`);
      try { assert.equal(next.acquired, true, 'reservation is available after automatic cleanup'); }
      finally { if (next.acquired) next.release(); }
    } finally {
      // Only these fixture-created processes may be signaled, including on
      // the expected pre-fix failure. Never leave the regression's orphan alive.
      const leader = pids ? pids.leader : runtime.readNativeStatus(task)?.childPid;
      if (leader && buildRuntime.buildProcessTreeAlive({ pid: leader })) {
        try { process.kill(-leader, 'SIGKILL'); } catch (err) { if (err.code !== 'ESRCH') throw err; }
      }
      if (launcher.exitCode === null && launcher.signalCode === null) launcher.kill('SIGKILL');
      await waitFor(() => launcher.exitCode !== null || launcher.signalCode !== null, 'fixture launcher cleanup');
      await waitFor(() => !buildRuntime.buildProcessTreeAlive({ pid: leader }), 'fixture process-group cleanup');
      const record = isolation.readLauncher(task);
      if (record) isolation.releaseTask(task, record.ownerToken);
      fs.rmSync(scratch, { recursive: true, force: true });
    }
  });
}
