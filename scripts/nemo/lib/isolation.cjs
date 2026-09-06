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
const { isDeepStrictEqual } = require('node:util');
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

function validId(id) {
  if (typeof id !== 'string' || !/^[a-zA-Z0-9][a-zA-Z0-9._-]{0,119}$/.test(id)) {
    throw new Error('invalid task/slot id: use 1-120 ASCII letters, digits, dot, underscore or hyphen, starting with a letter or digit');
  }
  return id;
}

// Preserve exact spelling on case-insensitive filesystems; never truncate entropy
// or normalize two user IDs into the same name. Resource namespaces stay separate.
function idKey(id) {
  validId(id);
  return crypto.createHash('sha256').update(id).digest('hex');
}

function safeSourceIdentity() {
  try { return identity.sourceIdentity(); } catch { return null; }
}

function completeSource(src) {
  return src && typeof src.head === 'string' && !!src.head &&
    typeof src.worktree === 'string' && !!src.worktree &&
    typeof src.branch === 'string' && typeof src.dirty === 'boolean' &&
    Object.hasOwn(src, 'dirtyDigest') && (!src.dirty || typeof src.dirtyDigest === 'string');
}

// Serialize all mutations of one resource, including stale-holder reclamation.
// Guards are NEVER stolen automatically: a crash during mutation requires explicit
// reconciliation. That fail-closed choice avoids deleting a successor's live lock.
function mutate(kind, id, action) {
  const dir = path.join(RUNTIME_ROOT, 'mutations');
  fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
  const guard = path.join(dir, `${kind}-${idKey(id)}`);
  try { fs.mkdirSync(guard, { mode: 0o700 }); }
  catch (err) {
    if (err.code !== 'EEXIST') throw err;
    const busy = new Error('resource mutation busy or interrupted; retry later or reconcile the guard');
    busy.code = 'EBUSY';
    throw busy;
  }
  try {
    fs.writeFileSync(path.join(guard, 'owner.json'), JSON.stringify({ pid: process.pid, startedAt: nowIso() }), { mode: 0o600 });
    return action();
  } finally { fs.rmSync(guard, { recursive: true, force: true }); }
}

function writeRecord(file, record) {
  const temporary = `${file}.${crypto.randomBytes(16).toString('hex')}.tmp`;
  try {
    fs.writeFileSync(temporary, JSON.stringify(record, null, 2), { flag: 'wx', mode: 0o600 });
    fs.renameSync(temporary, file);
  } finally { fs.rmSync(temporary, { force: true }); }
}

function validPid(pid) {
  if (!Number.isSafeInteger(pid) || pid <= 0) throw new Error('pid must be a positive integer');
  return pid;
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
  if (explicit != null) return validId(explicit);
  if (process.env.NEMO_TASK_ID != null) return validId(process.env.NEMO_TASK_ID);
  const src = safeSourceIdentity();
  const worktree = src && src.worktree ? path.basename(src.worktree).replace(/[^a-zA-Z0-9]/g, '_').slice(0, 40) : 'worktree';
  const stamp = Date.now().toString(36);
  const rand = crypto.randomBytes(3).toString('hex');
  return validId(`task-${worktree}-${process.pid}-${stamp}-${rand}`);
}

function taskRoot(taskId) {
  return path.join(RUNTIME_ROOT, 'tasks', idKey(taskId));
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

// ---- local launcher ownership/source record -------------------------------
// This is cooperative local process bookkeeping, not a challenge/response from
// a running HTTP server. A launcher integration must still verify the served URL.
function launcherFile(root) { return path.join(root, 'launcher.json'); }

function registerLauncher(taskId, opts = {}) {
  validId(taskId);
  const pid = validPid(opts.pid != null ? opts.pid : process.pid);
  if (!pidAlive(pid)) throw new Error('cannot register a launcher that is not running');
  const src = safeSourceIdentity();
  if (!completeSource(src)) throw new Error('source identity unavailable; launcher not registered');
  return mutate('task', taskId, () => {
    const { root } = taskRoots(taskId);
    if (exists(launcherFile(root))) throw new Error('task already registered; stop/release its existing owner before reuse');
    const record = {
      taskId, pid, ownerToken: opts.ownerToken || crypto.randomBytes(16).toString('hex'),
      source: src, label: opts.label || null, startedAt: nowIso(),
    };
    writeRecord(launcherFile(root), record);
    return record;
  });
}

function readLauncher(taskId) {
  return safeReadJson(launcherFile(taskRoot(taskId)));
}

function pidAlive(pid) {
  if (!Number.isSafeInteger(pid) || pid <= 0) return false;
  try { process.kill(pid, 0); return true; } catch (err) { return err.code === 'EPERM'; }
}

function verifyHandshake(taskId, opts = {}) {
  const rec = readLauncher(taskId);
  if (!rec) return { ok: false, reason: 'no launcher record for task' };
  if (!pidAlive(rec.pid)) return { ok: false, reason: `launcher pid ${rec.pid} is not running` };
  if (!opts.ownerToken || opts.ownerToken !== rec.ownerToken) return { ok: false, reason: 'owner token mismatch' };
  if (opts.checkSource) {
    const now = safeSourceIdentity();
    if (!completeSource(rec.source) || !completeSource(now)) return { ok: false, reason: 'source identity unavailable' };
    if (!isDeepStrictEqual(rec.source, now)) return { ok: false, reason: 'source moved: recorded source identity differs from current checkout' };
  }
  return { ok: true, reason: 'local launcher record matches; served endpoint not verified', launcher: rec };
}

// Await actual exit. A refused/ignored signal retains the record and owner token
// so the owner can retry. Never equate sending a signal with completing stop.
async function requestStop(taskId, ownerToken, opts = {}) {
  const signal = opts.signal || 'SIGTERM';
  const timeoutMs = opts.timeoutMs ?? 3000;
  if (!['SIGTERM', 'SIGINT', 'SIGKILL'].includes(signal) || !Number.isFinite(timeoutMs) || timeoutMs < 0 || timeoutMs > 60000) {
    return { stopped: false, reason: 'invalid termination signal or timeout' };
  }
  let rec;
  try {
    const refused = mutate('task', taskId, () => {
      rec = readLauncher(taskId);
      if (!rec) return { stopped: false, reason: 'no launcher record for task' };
      if (!ownerToken || ownerToken !== rec.ownerToken) return { stopped: false, reason: 'refused: caller is not the task owner (owner token mismatch)' };
      validPid(rec.pid);
      if (pidAlive(rec.pid)) process.kill(rec.pid, signal);
      return null;
    });
    if (refused) return refused;
  } catch (err) { return { stopped: false, reason: err.message }; }
  const deadline = Date.now() + timeoutMs;
  while (pidAlive(rec.pid) && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 20));
  if (pidAlive(rec.pid)) return { stopped: false, signaled: true, reason: 'termination unconfirmed; owner record retained', pid: rec.pid };
  try {
    mutate('task', taskId, () => {
      const current = readLauncher(taskId);
      if (current && current.ownerToken === rec.ownerToken && current.pid === rec.pid) {
        fs.unlinkSync(launcherFile(taskRoot(taskId)));
      }
    });
  } catch (err) { return { stopped: true, reason: 'exited; owner record cleanup pending: ' + err.message, pid: rec.pid }; }
  return { stopped: true, reason: 'exit confirmed', pid: rec.pid };
}

function releaseTask(taskId, ownerToken) {
  return mutate('task', taskId, () => {
    const file = launcherFile(taskRoot(taskId));
    const rec = readLauncher(taskId);
    if (exists(file) && !rec) return { released: false, reason: 'unreadable launcher record; reconcile before release' };
    if (rec && (!ownerToken || ownerToken !== rec.ownerToken)) return { released: false, reason: 'refused: caller is not the task owner' };
    if (rec && pidAlive(rec.pid)) return { released: false, reason: 'launcher still running; stop it before release' };
    fs.rmSync(taskRoot(taskId), { recursive: true, force: true });
    return { released: true, taskId };
  });
}

// ---- exclusive slots ----------------------------------------------------
function slotFile(slot) { return path.join(SLOTS_DIR, `${idKey(slot)}.lock`); }

function acquireExclusiveSlot(slot, taskId, opts = {}) {
  validId(slot); validId(taskId);
  const pid = validPid(opts.pid != null ? opts.pid : process.pid);
  if (!pidAlive(pid)) throw new Error('cannot acquire a slot for a process that is not running');
  try {
    return mutate('slot', slot, () => {
      fs.mkdirSync(SLOTS_DIR, { recursive: true, mode: 0o700 });
      const file = slotFile(slot);
      const holder = safeReadJson(file);
      if (exists(file) && (!holder || !Number.isSafeInteger(holder.pid) || holder.pid <= 0)) {
        return { acquired: false, reason: 'unreadable slot record; reconcile before reuse' };
      }
      if (holder && pidAlive(holder.pid)) return { acquired: false, reason: 'slot held by another task', holder };
      // Replacement is safe only inside the same guard used by every acquire/release.
      const ownerToken = opts.ownerToken || crypto.randomBytes(16).toString('hex');
      writeRecord(file, { slot, taskId, pid, ownerToken, acquiredAt: nowIso() });
      return { acquired: true, ownerToken, taskId, slot, file, release: () => releaseExclusiveSlot(slot, ownerToken) };
    });
  } catch (err) {
    if (err.code === 'EBUSY') return { acquired: false, reason: err.message };
    throw err;
  }
}

function releaseExclusiveSlot(slot, ownerToken) {
  try {
    return mutate('slot', slot, () => {
      const file = slotFile(slot);
      const holder = safeReadJson(file);
      if (!holder) return { released: false, reason: 'slot not held or record unreadable' };
      if (!ownerToken || holder.ownerToken !== ownerToken) return { released: false, reason: 'refused: caller is not the slot owner' };
      fs.unlinkSync(file);
      return { released: true };
    });
  } catch (err) {
    if (err.code === 'EBUSY') return { released: false, reason: err.message };
    throw err;
  }
}

module.exports = {
  RUNTIME_ROOT,
  PORT_RANGE,
  // Exported so a consumer that must name this task's resources OUTSIDE these
  // roots (the native app launcher derives its bundle identifier and WebKit
  // data store from it) uses this exact derivation instead of writing a second
  // one that has to stay identical by hand.
  idKey,
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
