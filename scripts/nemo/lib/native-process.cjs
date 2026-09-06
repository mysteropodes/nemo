'use strict';
// Native launcher PID identity and non-signaling process-group exit proof.
const { spawnSync } = require('node:child_process');
const SCHEMA = 'nemo.native-launcher/1';

// Compare a fresh OS start/command fingerprint before signaling a recorded PID.
// Missing evidence is a refusal; an orphan is never signaled by a stale app PID.
function launcherProcessIdentity(pid) {
  if (!Number.isSafeInteger(pid) || pid <= 0) return null;
  const result = spawnSync('ps', ['-p', String(pid), '-o', 'lstart=', '-o', 'comm='], {
    encoding: 'utf8', timeout: 2000, env: { ...process.env, LC_ALL: 'C' },
  });
  return result.status === 0 && result.stdout.trim() ? result.stdout.trim() : null;
}

function nativeProcessTreeStopped(taskId, record, status) {
  if (!record || !status || status.schema !== SCHEMA || status.taskId !== taskId || status.launcherPid !== record.pid) {
    return { stopped: false, reason: 'native process ownership evidence unavailable; explicit reconciliation required' };
  }
  if (!Number.isSafeInteger(status.childPid) || status.childPid <= 0) {
    return status.childPid === null && status.processTree && status.processTree.stopped
      ? { stopped: true, reason: 'launcher confirmed no app process remained' }
      : { stopped: false, reason: 'app process group is unknown; explicit reconciliation required' };
  }
  try { process.kill(-status.childPid, 0); }
  catch (err) {
    if (err.code === 'ESRCH') return { stopped: true, reason: 'app process group exit confirmed' };
    return { stopped: false, reason: `app process group exit unconfirmed (${err.code}); explicit reconciliation required` };
  }
  return { stopped: false, reason: 'app process group is still live; explicit reconciliation required' };
}

module.exports = { SCHEMA, launcherProcessIdentity, nativeProcessTreeStopped };
