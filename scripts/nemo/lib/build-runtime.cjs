'use strict';

const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const { finished } = require('node:stream/promises');
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

function assertNativePlatform(platform = process.platform) {
  if (platform !== 'darwin') {
    throw new Error('native desktop build launcher currently supports macOS only; platform bundle arguments and process-tree ownership are not validated here');
  }
}

function buildLaunchConfig(taskId, options = {}) {
  if (!options.command) assertNativePlatform();
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

function buildProcessTreeAlive(child) {
  if (!child || !Number.isSafeInteger(child.pid) || child.pid <= 0) return false;
  if (process.platform !== 'win32') {
    try { process.kill(-child.pid, 0); return true; }
    catch (err) { if (err.code === 'ESRCH') return false; return err.code === 'EPERM'; }
  }
  return child.exitCode == null && child.signalCode == null;
}

function signalBuild(child, signal) {
  if (!child || !Number.isSafeInteger(child.pid) || child.pid <= 0) return;
  if (process.platform !== 'win32') {
    try { process.kill(-child.pid, signal); return; } catch (err) { if (err.code !== 'ESRCH') throw err; return; }
  }
  if (child.exitCode == null && child.signalCode == null) {
    try { child.kill(signal); } catch (err) { if (err.code !== 'ESRCH') throw err; }
  }
}

async function waitForBuildProcessTree(child, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (buildProcessTreeAlive(child) && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  return !buildProcessTreeAlive(child);
}

async function stopBuild(child, graceMs = 1500) {
  if (!buildProcessTreeAlive(child)) return { stopped: true, forced: false, reason: 'owned build process tree already exited' };
  signalBuild(child, 'SIGTERM');
  if (await waitForBuildProcessTree(child, graceMs)) {
    return { stopped: true, forced: false, reason: 'owned build process tree exited after SIGTERM' };
  }
  signalBuild(child, 'SIGKILL');
  if (await waitForBuildProcessTree(child, 2000)) {
    return { stopped: true, forced: true, reason: 'owned build process tree required SIGKILL' };
  }
  return { stopped: false, forced: true, reason: 'owned build process tree still exists after SIGKILL; reconciliation required' };
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
  const closed = new Promise((resolve) => child.once('close', (code, signal) => resolve({ code, signal })));
  const logsDone = Promise.all([finished(stdout), finished(stderr)]);
  const drained = Promise.all([closed, logsDone]);
  // A later await handles the result; attach now so an early stream error is
  // never reported as an unhandled rejection while the leader is still live.
  drained.catch(() => {});
  return { child, stdout, stderr, closed, logsDone, drained };
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
    let tree;
    try { tree = await stopBuild(processInfo && processInfo.child); }
    catch (err) { tree = { stopped: false, forced: false, reason: err.message }; }
    if (!tree.stopped) {
      save({ state: 'reconciliation-required', finishedAt: nowIso(), signal: signal || 'SIGTERM', processTree: tree });
      closing = false;
      return;
    }
    if (processInfo) {
      await processInfo.closed.catch(() => {});
      await processInfo.logsDone.catch(() => {});
    }
    const slotRelease = releaseSlot();
    save({ state: 'stopped', finishedAt: nowIso(), signal: signal || 'SIGTERM', processTree: tree, slotRelease });
    process.exit(0);
  };
  // Retain cooperative handlers after an unproved cleanup so the owner can
  // retry instead of turning the next stop request into an unhandled exit.
  process.on('SIGTERM', () => shutdown('SIGTERM'));
  process.on('SIGINT', () => shutdown('SIGINT'));
  // Ownership must remain live on every post-registration return path. In
  // particular, reconciliation-required must not degrade into a reclaimable
  // dead-holder slot merely because the direct child and its pipes exited.
  setInterval(() => {}, 60_000);

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
    // Let normal leader teardown and pipe closure settle before probing its
    // process group. If descendants inherited the pipes this stays pending,
    // and the group probe below remains authoritative.
    await Promise.race([
      processInfo.drained.catch(() => {}),
      new Promise((resolve) => setTimeout(resolve, 100)),
    ]);
    let tree;
    try { tree = await stopBuild(processInfo.child); }
    catch (err) { tree = { stopped: false, forced: false, reason: err.message }; }
    if (closing) return;
    if (!tree.stopped) {
      save({
        state: 'reconciliation-required',
        finishedAt: nowIso(),
        exitCode: result.code,
        signal: result.signal,
        processTree: tree,
      });
      return;
    }
    await processInfo.closed;
    await processInfo.logsDone;
    const slotRelease = releaseSlot();
    save({
      state: result.code === 0 && !tree.forced ? 'completed' : 'failed',
      finishedAt: nowIso(),
      exitCode: result.code,
      signal: result.signal,
      error: tree.forced ? 'build leader exited while descendants remained; owned process group required forced cleanup' : undefined,
      processTree: tree,
      slotRelease,
    });
  } catch (err) {
    let tree = { stopped: true, forced: false, reason: 'build process was not spawned' };
    if (processInfo) {
      try { tree = await stopBuild(processInfo.child); }
      catch (stopErr) { tree = { stopped: false, forced: false, reason: stopErr.message }; }
    }
    if (!tree.stopped) {
      save({ state: 'reconciliation-required', finishedAt: nowIso(), error: err.message, processTree: tree });
      return;
    }
    if (processInfo) await processInfo.logsDone.catch(() => {});
    const slotRelease = releaseSlot();
    save({ state: 'failed', finishedAt: nowIso(), error: err.message, processTree: tree, slotRelease });
  }
}

module.exports = {
  SCHEMA,
  STATUS_FILE,
  worktreeBuildSlot,
  localTauriExecutable,
  tauriBuildArgs,
  assertNativePlatform,
  buildLaunchConfig,
  readBuildStatus,
  buildHandshake,
  buildProcessTreeAlive,
  stopBuild,
  runBuildLauncher,
};
