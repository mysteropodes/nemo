'use strict';

const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const { isDeepStrictEqual } = require('node:util');
const isolation = require('./isolation.cjs');
const identity = require('./identity.cjs');
const { ROOT, exists, nowIso } = require('./util.cjs');

const SCHEMA = 'nemo.build-runtime/1';
const STATUS_FILE = 'build-runtime.json';

function worktreeBuildSlot(worktree = ROOT) {
  const resolved = fs.realpathSync(worktree);
  const key = crypto.createHash('sha256').update(resolved).digest('hex').slice(0, 24);
  return `desktop-build-${key}`;
}

function localTauriExecutable(root = ROOT) {
  const name = process.platform === 'win32' ? 'tauri.cmd' : 'tauri';
  const executable = path.join(root, 'node_modules', '.bin', name);
  if (!exists(executable)) throw new Error('installed Tauri CLI not found at node_modules/.bin/tauri; run npm ci in this worktree');
  if (process.platform !== 'win32') fs.accessSync(executable, fs.constants.X_OK);
  return executable;
}

function tauriBuildArgs(hostTriple) {
  if (!hostTriple) throw new Error('rustc host triple unavailable; cannot form the Tauri build command');
  return ['build', '--target', hostTriple, '-b', 'app', '--no-sign'];
}

function buildLaunchConfig(taskId, options = {}) {
  const executable = options.command ? path.resolve(options.command) : localTauriExecutable();
  if (!exists(executable)) throw new Error(`build command not found: ${executable}`);
  if (process.platform !== 'win32') fs.accessSync(executable, fs.constants.X_OK);
  const triple = options.hostTriple || identity.hostTriple();
  const args = options.command
    ? (Array.isArray(options.args) ? options.args.slice() : [])
    : tauriBuildArgs(triple);
  const roots = isolation.taskRoots(taskId);
  const targetDir = path.join(roots.build, 'tauri-target');
  return {
    executable,
    args,
    cwd: ROOT,
    slot: worktreeBuildSlot(ROOT),
    roots,
    targetDir,
    stdoutLog: path.join(roots.reports, 'desktop-build.stdout.log'),
    stderrLog: path.join(roots.reports, 'desktop-build.stderr.log'),
    statusFile: path.join(roots.reports, STATUS_FILE),
    env: {
      ...process.env,
      CARGO_TARGET_DIR: targetDir,
      NEMO_TASK_ID: taskId,
      NEMO_REPORT_DIR: roots.reports,
      XDG_CACHE_HOME: roots.cache,
      TMPDIR: roots.temp,
      TMP: roots.temp,
      TEMP: roots.temp,
    },
  };
}

function publicSource(source) {
  if (!source) return source;
  const { originUrl: _originUrl, ...safe } = source;
  return safe;
}

function publicRoots(roots) {
  return {
    root: roots.root,
    temp: roots.temp,
    cache: roots.cache,
    build: roots.build,
    reports: roots.reports,
  };
}

function atomicWriteJson(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true, mode: 0o700 });
  const temp = `${file}.${process.pid}.tmp`;
  fs.writeFileSync(temp, JSON.stringify(value, null, 2) + '\n', { mode: 0o600 });
  fs.renameSync(temp, file);
}

function readBuildStatus(taskId) {
  const file = path.join(isolation.taskRoots(taskId).reports, STATUS_FILE);
  try { return JSON.parse(fs.readFileSync(file, 'utf8')); }
  catch { return null; }
}

function waitForExit(child) {
  if (!child || child.exitCode != null || child.signalCode != null) {
    return Promise.resolve({ code: child && child.exitCode, signal: child && child.signalCode });
  }
  return new Promise((resolve) => child.once('exit', (code, signal) => resolve({ code, signal })));
}

function signalBuild(child, signal) {
  if (!child || child.exitCode != null || child.signalCode != null) return;
  if (process.platform !== 'win32') {
    try { process.kill(-child.pid, signal); return; } catch (err) { if (err.code !== 'ESRCH') throw err; }
  }
  try { child.kill(signal); } catch (err) { if (err.code !== 'ESRCH') throw err; }
}

async function stopBuild(child, graceMs = 1500) {
  if (!child || child.exitCode != null || child.signalCode != null) return;
  const exited = waitForExit(child);
  signalBuild(child, 'SIGTERM');
  let timer;
  await Promise.race([exited, new Promise((resolve) => { timer = setTimeout(resolve, graceMs); })]);
  clearTimeout(timer);
  if (child.exitCode == null && child.signalCode == null) {
    signalBuild(child, 'SIGKILL');
    await exited;
  }
}

function spawnBuild(config) {
  fs.mkdirSync(config.targetDir, { recursive: true, mode: 0o700 });
  fs.mkdirSync(config.roots.reports, { recursive: true, mode: 0o700 });
  const stdout = fs.createWriteStream(config.stdoutLog, { flags: 'a', mode: 0o600 });
  const stderr = fs.createWriteStream(config.stderrLog, { flags: 'a', mode: 0o600 });
  const child = spawn(config.executable, config.args, {
    cwd: config.cwd,
    env: config.env,
    detached: process.platform !== 'win32',
    stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
  });
  child.stdout.pipe(stdout);
  child.stderr.pipe(stderr);
  return { child, stdout, stderr };
}

function buildHandshake(taskId, ownerToken) {
  const local = isolation.verifyHandshake(taskId, { ownerToken, checkSource: true });
  const launcher = isolation.readLauncher(taskId);
  const status = readBuildStatus(taskId);
  let currentBuild = null;
  try { currentBuild = identity.buildIdentity(); } catch { /* fail closed below */ }
  const buildMatches = !!status && isDeepStrictEqual(status.build.startup, currentBuild);
  return {
    ok: local.ok && buildMatches,
    reason: !local.ok ? local.reason : (!currentBuild ? 'build identity unavailable' : (buildMatches ? 'task owner, source and build identities match' : 'build identity changed')),
    taskId,
    pid: launcher ? launcher.pid : null,
    source: launcher ? { startup: publicSource(launcher.source), matches: local.ok } : null,
    build: status ? { startup: status.build.startup, current: currentBuild, matches: buildMatches } : null,
    runtime: status,
  };
}

async function runBuildLauncher(taskId, options = {}, emit = () => {}) {
  const config = buildLaunchConfig(taskId, options);
  // Resolve the complete startup identity before acquiring ownership. A failed
  // identity probe must not leave a live launcher that never disclosed its token.
  let startupBuild;
  try { startupBuild = identity.buildIdentity(); }
  catch (err) {
    isolation.releaseTask(taskId);
    throw new Error(`build identity unavailable: ${err.message}`);
  }
  const slot = isolation.acquireExclusiveSlot(config.slot, taskId, { pid: process.pid });
  if (!slot.acquired) {
    isolation.releaseTask(taskId);
    throw new Error(`same-worktree desktop build unavailable: ${slot.reason}`);
  }
  let launcher;
  try {
    launcher = isolation.registerLauncher(taskId, {
      pid: process.pid,
      ownerToken: slot.ownerToken,
      label: 'desktop-build',
    });
  } catch (err) {
    slot.release();
    throw err;
  }
  let status = {
    schema: SCHEMA,
    taskId,
    launcherPid: process.pid,
    childPid: null,
    state: 'starting',
    startedAt: nowIso(),
    finishedAt: null,
    exitCode: null,
    signal: null,
    slot: config.slot,
    targetDir: config.targetDir,
    roots: publicRoots(config.roots),
    logs: { stdout: config.stdoutLog, stderr: config.stderrLog },
    source: publicSource(launcher.source),
    build: { startup: startupBuild },
  };
  const save = (change) => {
    status = { ...status, ...change };
    atomicWriteJson(config.statusFile, status);
  };
  save({});
  emit({
    started: true,
    taskId,
    pid: process.pid,
    ownerToken: launcher.ownerToken,
    slot: config.slot,
    roots: publicRoots(config.roots),
    targetDir: config.targetDir,
    statusFile: config.statusFile,
    source: publicSource(launcher.source),
    build: startupBuild,
  });

  let processInfo = null;
  let slotReleased = false;
  const releaseSlot = () => {
    if (slotReleased) return { released: true, alreadyReleased: true };
    const result = slot.release();
    if (result.released) slotReleased = true;
    return result;
  };
  let closing = false;
  const shutdown = async (signal) => {
    if (closing) return;
    closing = true;
    await stopBuild(processInfo && processInfo.child).catch(() => {});
    const slotRelease = releaseSlot();
    save({ state: 'stopped', finishedAt: nowIso(), signal: signal || 'SIGTERM', slotRelease });
    if (processInfo) {
      processInfo.stdout.end();
      processInfo.stderr.end();
    }
    process.exit(0);
  };
  process.once('SIGTERM', () => shutdown('SIGTERM'));
  process.once('SIGINT', () => shutdown('SIGINT'));

  try {
    processInfo = spawnBuild(config);
    await new Promise((resolve, reject) => {
      const failed = (err) => { processInfo.child.off('spawn', ready); reject(err); };
      const ready = () => { processInfo.child.off('error', failed); resolve(); };
      processInfo.child.once('error', failed);
      processInfo.child.once('spawn', ready);
    });
    save({ state: 'active', childPid: processInfo.child.pid });
    const result = await waitForExit(processInfo.child);
    processInfo.stdout.end();
    processInfo.stderr.end();
    const slotRelease = releaseSlot();
    save({
      state: result.code === 0 ? 'completed' : 'failed',
      finishedAt: nowIso(),
      exitCode: result.code,
      signal: result.signal,
      slotRelease,
    });
  } catch (err) {
    if (processInfo) {
      processInfo.stdout.end();
      processInfo.stderr.end();
    }
    const slotRelease = releaseSlot();
    save({ state: 'failed', finishedAt: nowIso(), error: err.message, slotRelease });
  }

  // Keep the ownership record live so status remains trustworthy and build
  // artifacts stay available. The owner copies artifacts, then calls stop.
  setInterval(() => {}, 60_000);
}

module.exports = {
  SCHEMA,
  STATUS_FILE,
  worktreeBuildSlot,
  localTauriExecutable,
  tauriBuildArgs,
  buildLaunchConfig,
  readBuildStatus,
  buildHandshake,
  stopBuild,
  runBuildLauncher,
};
