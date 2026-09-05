'use strict';
// Task-runtime isolation (R06, https://github.com/mysteropodes/nemo/issues/902).
// Gives each task instance its own temp/cache/build/report roots, a real
// OS-level port reservation, and an owner-checked launcher handshake so only
// the process that started a task can stop it.
//
// Reads scripts/nemo/lib/util.cjs and identity.cjs (R02) for source identity
// as READ-ONLY references — nothing here edits those files, and this module
// is not wired into scripts/nemo/lib/jobs.cjs or package.json (out of scope
// for this increment; see engineering/runtime-isolation.md).
const fs = require('node:fs');
const os = require('node:os');
const net = require('node:net');
const path = require('node:path');
const crypto = require('node:crypto');
const { exists, nowIso } = require('./util.cjs');
const identity = require('./identity.cjs'); // property access below (identity.sourceIdentity()),
// deliberately not destructured, so tests can substitute identity.sourceIdentity without editing this file.

// Root that holds all isolated task state. Deliberately OUTSIDE the repo
// (os.tmpdir(), not `reports/`) so this module needs no .gitignore change
// and nothing here survives (or needs to survive) `git clean`. Override with
// NEMO_ISOLATION_ROOT for tests or a shared machine-wide cache location.
const RUNTIME_ROOT = process.env.NEMO_ISOLATION_ROOT || path.join(os.tmpdir(), 'nemo-runtime');
const SLOTS_DIR = path.join(RUNTIME_ROOT, 'slots');

// Scratch band for reserved ports. Distinct from the `npm run serve` default
// (1420) and anything Tauri/vite pick by default; arbitrary otherwise.
const PORT_RANGE = { start: 41000, end: 45000 };

function sanitize(id) {
  return String(id).replace(/[^a-zA-Z0-9._-]+/g, '_').slice(0, 120) || 'task';
}

function safeSourceIdentity() {
  try { return identity.sourceIdentity(); } catch { return {}; }
}

function safeReadJson(file) {
  try { return JSON.parse(fs.readFileSync(file, 'utf8')); } catch { return null; }
}

// ---- task id --------------------------------------------------------------
// Explicit id > NEMO_TASK_ID env > derived from worktree + pid + time + random.
// The derived form is unique per *process invocation*, which is what makes
// "two task instances started without any coordination" not collide by
// default; an explicit id is for a caller (e.g. a wrapper script) that wants
// a stable, human-legible name across a task's lifetime.
function resolveTaskId(explicit) {
  if (explicit) return sanitize(explicit);
  if (process.env.NEMO_TASK_ID) return sanitize(process.env.NEMO_TASK_ID);
  const src = safeSourceIdentity();
  const worktree = src.worktree ? path.basename(src.worktree) : 'worktree';
  const stamp = Date.now().toString(36);
  const rand = crypto.randomBytes(3).toString('hex');
  return sanitize(`${worktree}-${process.pid}-${stamp}-${rand}`);
}

function taskRoot(taskId) {
  return path.join(RUNTIME_ROOT, sanitize(taskId));
}

// Every mutable root a task needs, created up front. `reports` is meant to be
// exported as NEMO_REPORT_DIR (see engineering/runtime-isolation.md) so
// `scripts/nemo`'s existing receipt/build-wasm output (which already nests
// under NEMO_REPORT_DIR — see receipt.cjs reportDir() and jobs.cjs
// jobBuildWasm()) lands isolated with zero changes to those files.
function taskRoots(taskId) {
  const root = taskRoot(taskId);
  const roots = {
    root,
    temp: path.join(root, 'tmp'),
    cache: path.join(root, 'cache'),
    build: path.join(root, 'build'),
    reports: path.join(root, 'reports'),
    browserProfile: path.join(root, 'browser-profile'),
    tauriDataDir: path.join(root, 'tauri-data'),
    ports: path.join(root, 'ports'),
  };
  for (const p of Object.values(roots)) fs.mkdirSync(p, { recursive: true });
  return roots;
}

// ---- ports ------------------------------------------------------------
// Reserves a port by actually binding it. Two callers can never end up
// holding the same port: the kernel refuses the second bind (EADDRINUSE),
// which is stronger than any lock-file convention we could write by hand.
// The lock file alongside it is just a readable record of who holds what,
// not the enforcement mechanism.
function reservePort(taskId, opts = {}) {
  const host = opts.host || '127.0.0.1';
  const start = opts.start || PORT_RANGE.start;
  const end = opts.end || PORT_RANGE.end;
  const span = end - start + 1;
  // Deterministic starting offset from the task id: repeated runs of the
  // *same* task id tend to land on the same port instead of drifting, while
  // a real collision (two different tasks hashing near each other, or the
  // same task id run twice concurrently) falls through to a linear probe.
  const h = crypto.createHash('sha256').update(String(taskId)).digest();
  const startPort = start + (h.readUInt32BE(0) % span);
  const { ports } = taskRoots(taskId);

  return new Promise((resolve, reject) => {
    let attemptsLeft = span;
    const tryPort = (p) => {
      if (attemptsLeft <= 0) { reject(new Error(`no free port for task ${taskId} in [${start},${end}]`)); return; }
      attemptsLeft -= 1;
      const srv = net.createServer();
      srv.once('error', (err) => {
        if (err.code === 'EADDRINUSE' || err.code === 'EACCES') {
          tryPort(p + 1 > end ? start : p + 1);
        } else reject(err);
      });
      srv.listen(p, host, () => {
        const lockFile = path.join(ports, `${p}.json`);
        fs.writeFileSync(lockFile, JSON.stringify({ taskId, pid: process.pid, port: p, host, createdAt: nowIso() }, null, 2));
        resolve({
          port: p,
          host,
          server: srv,
          lockFile,
          release: () => new Promise((res) => {
            try { fs.unlinkSync(lockFile); } catch { /* already gone */ }
            srv.close(() => res());
          }),
        });
      });
    };
    tryPort(startPort);
  });
}

// ---- owner/source handshake --------------------------------------------
// registerLauncher() is called by the process that "owns" a task instance
// (a dev server, a spawned test runner, a sidecar). It records who it is
// (pid + an unguessable owner token) and what it is (source identity from
// R02's identity.cjs, read-only), so a caller can check both "is this the
// process I started" and "is this the code I think it is" before trusting
// or stopping it.
function launcherFile(root) { return path.join(root, 'launcher.json'); }

function registerLauncher(taskId, opts = {}) {
  const { root } = taskRoots(taskId);
  const ownerToken = opts.ownerToken || crypto.randomBytes(16).toString('hex');
  const pid = opts.pid != null ? opts.pid : process.pid;
  const src = safeSourceIdentity();
  const record = {
    taskId,
    pid,
    ownerToken,
    source: { head: src.head || null, branch: src.branch || null, dirty: !!src.dirty, dirtyDigest: src.dirtyDigest || null },
    label: opts.label || null,
    startedAt: nowIso(),
  };
  fs.writeFileSync(launcherFile(root), JSON.stringify(record, null, 2));
  return record;
}

function readLauncher(taskId) {
  const file = launcherFile(taskRoot(taskId));
  return exists(file) ? safeReadJson(file) : null;
}

function pidAlive(pid) {
  try { process.kill(pid, 0); return true; } catch (err) { return err.code === 'EPERM'; }
}

// ok only if: a launcher record exists, its pid is still running, the
// caller's owner token matches (when supplied), and — with checkSource —
// the source identity observed right now still matches what the launcher
// recorded at start, catching a task id reused against a moved checkout.
function verifyHandshake(taskId, opts = {}) {
  const rec = readLauncher(taskId);
  if (!rec) return { ok: false, reason: 'no launcher record for task' };
  if (!pidAlive(rec.pid)) return { ok: false, reason: `launcher pid ${rec.pid} is not running` };
  if (opts.ownerToken != null && opts.ownerToken !== rec.ownerToken) return { ok: false, reason: 'owner token mismatch' };
  if (opts.checkSource) {
    const now = safeSourceIdentity();
    if (rec.source.head && now.head && rec.source.head !== now.head) {
      return { ok: false, reason: `source moved: launcher started at ${rec.source.head}, now at ${now.head}` };
    }
  }
  return { ok: true, reason: 'launcher alive and matches', launcher: rec };
}

// Only the holder of the correct owner token may stop a task's launcher.
// Everyone else gets a named refusal, never a silent no-op.
function requestStop(taskId, ownerToken, opts = {}) {
  const rec = readLauncher(taskId);
  if (!rec) return { stopped: false, reason: 'no launcher record for task' };
  if (ownerToken !== rec.ownerToken) return { stopped: false, reason: 'refused: caller is not the task owner (owner token mismatch)' };
  if (!pidAlive(rec.pid)) {
    cleanupLauncher(taskId);
    return { stopped: true, reason: 'already exited', pid: rec.pid };
  }
  try {
    process.kill(rec.pid, opts.signal || 'SIGTERM');
  } catch (err) {
    return { stopped: false, reason: `kill failed: ${err.code || err.message}` };
  }
  cleanupLauncher(taskId);
  return { stopped: true, reason: 'signaled', pid: rec.pid };
}

function cleanupLauncher(taskId) {
  try { fs.unlinkSync(launcherFile(taskRoot(taskId))); } catch { /* already gone */ }
}

function releaseTask(taskId) {
  fs.rmSync(taskRoot(taskId), { recursive: true, force: true });
}

// ---- exclusive slots ----------------------------------------------------
// Generic mutual-exclusion primitive for a resource that isn't per-task by
// nature — a shared physical desktop input, a GPU reference-benchmark slot.
// Acquisition uses O_EXCL ('wx'): the filesystem itself refuses a second
// create while the lock file exists, so this doesn't depend on us getting a
// check-then-write race right. A lock left behind by a dead process is
// reclaimed automatically (never by a live one, even if it's a different
// task) so a crashed holder can't wedge the slot forever.
function slotFile(slot) { return path.join(SLOTS_DIR, `${sanitize(slot)}.lock`); }

function acquireExclusiveSlot(slot, taskId, opts = {}) {
  fs.mkdirSync(SLOTS_DIR, { recursive: true });
  const file = slotFile(slot);
  const ownerToken = opts.ownerToken || crypto.randomBytes(16).toString('hex');
  const pid = opts.pid != null ? opts.pid : process.pid;
  const record = { slot, taskId, pid, ownerToken, acquiredAt: nowIso() };
  let fd;
  try {
    fd = fs.openSync(file, 'wx');
  } catch (err) {
    if (err.code !== 'EEXIST') throw err;
    const holder = safeReadJson(file);
    if (holder && !pidAlive(holder.pid)) {
      fs.rmSync(file, { force: true }); // stale lock from a dead process
      return acquireExclusiveSlot(slot, taskId, opts);
    }
    return { acquired: false, reason: 'slot held by another task', holder };
  }
  fs.writeSync(fd, JSON.stringify(record, null, 2));
  fs.closeSync(fd);
  return { acquired: true, ownerToken, taskId, slot, file, release: () => releaseExclusiveSlot(slot, ownerToken) };
}

function releaseExclusiveSlot(slot, ownerToken) {
  const file = slotFile(slot);
  const holder = safeReadJson(file);
  if (!holder) return { released: false, reason: 'slot not held' };
  if (holder.ownerToken !== ownerToken) return { released: false, reason: 'refused: caller is not the slot owner' };
  fs.rmSync(file, { force: true });
  return { released: true };
}

module.exports = {
  RUNTIME_ROOT,
  PORT_RANGE,
  resolveTaskId,
  taskRoot,
  taskRoots,
  reservePort,
  registerLauncher,
  readLauncher,
  verifyHandshake,
  requestStop,
  releaseTask,
  pidAlive,
  acquireExclusiveSlot,
  releaseExclusiveSlot,
};
