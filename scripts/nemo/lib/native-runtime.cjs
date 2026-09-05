'use strict';
// Native (Tauri) task-instance launcher — R06, https://github.com/mysteropodes/nemo/issues/902.
//
// The isolation library already allocates a `tauri-data` root per task, and
// src-tauri/src/task_runtime.rs makes the app resolve its identifier, app
// directories and WebKit data store from it. This module is what actually puts
// the two together: it starts a built Nemo with that task's environment, owns
// the resulting process group, and reports the identity the running instance
// disclosed so a second instance can be proved to share nothing with it.
//
// Ownership, stop semantics and the reconciliation rules are the same ones the
// build launcher already documents; the process-group primitives are imported
// from it rather than written a second time.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const { finished } = require('node:stream/promises');
const { isDeepStrictEqual } = require('node:util');
const isolation = require('./isolation.cjs');
const identity = require('./identity.cjs');
const buildRuntime = require('./build-runtime.cjs');
const { ROOT, exists, nowIso } = require('./util.cjs');

const SCHEMA = 'nemo.native-launcher/1';
const STATUS_FILE = 'native-launcher.json';
// Written by the app itself (src-tauri/src/task_runtime.rs) into the task's
// tauri-data root. This file, not our own bookkeeping, is what proves which
// identifier and directories the running instance actually resolved.
const APP_MANIFEST_FILE = 'native-runtime.json';
const APP_MANIFEST_SCHEMA = 'nemo.native-runtime/1';

function assertNativePlatform(platform = process.platform) {
  if (platform !== 'darwin') {
    throw new Error('native app launcher currently supports macOS only; bundle layout and process-tree ownership are not validated elsewhere');
  }
}

// A .app bundle names its own executable; do not guess it from the product
// name (Tauri's productName, the crate binary name and the bundle display
// name are three different strings in this repository).
function bundleExecutable(bundle) {
  const plist = path.join(bundle, 'Contents', 'Info.plist');
  if (!exists(plist)) throw new Error(`not an app bundle (no Contents/Info.plist): ${bundle}`);
  const text = fs.readFileSync(plist, 'utf8');
  const match = /<key>CFBundleExecutable<\/key>\s*<string>([^<]+)<\/string>/.exec(text);
  if (!match) throw new Error(`Info.plist has no CFBundleExecutable: ${plist}`);
  const executable = path.join(bundle, 'Contents', 'MacOS', match[1]);
  if (!exists(executable)) throw new Error(`bundle executable missing: ${executable}`);
  return executable;
}

function firstAppBundle(dir) {
  if (!exists(dir)) return null;
  const entry = fs.readdirSync(dir).filter((name) => name.endsWith('.app')).sort()[0];
  return entry ? path.join(dir, entry) : null;
}

// Search order: an explicit path, then this task's own isolated build output
// (where scripts/nemo/build.cjs puts it), then the worktree's default target.
// Never another task's build directory: a stale binary from a different source
// state is exactly what the source/build handshake exists to catch.
function resolveApp(taskId, options = {}) {
  const triple = options.hostTriple || identity.hostTriple();
  const candidates = [];
  if (options.app) candidates.push({ from: 'explicit', target: path.resolve(options.app) });
  else {
    // taskRoot(), not taskRoots(): merely looking for a binary must not
    // create this task's roots, so a refused start leaves nothing behind.
    const taskTarget = path.join(isolation.taskRoot(taskId), 'build', 'tauri-target');
    for (const [from, base] of [['task-build', taskTarget], ['worktree-build', path.join(ROOT, 'src-tauri', 'target')]]) {
      for (const profile of ['release', 'debug']) {
        candidates.push({ from, target: path.join(base, triple, profile, 'bundle', 'macos') });
      }
    }
  }
  for (const candidate of candidates) {
    let bundle = null;
    if (candidate.from === 'explicit') {
      if (!exists(candidate.target)) continue;
      bundle = candidate.target.endsWith('.app') ? candidate.target : null;
      if (!bundle) {
        fs.accessSync(candidate.target, fs.constants.X_OK);
        return { from: candidate.from, bundle: null, executable: candidate.target };
      }
    } else {
      bundle = firstAppBundle(candidate.target);
    }
    if (!bundle) continue;
    return { from: candidate.from, bundle, executable: bundleExecutable(bundle) };
  }
  const tried = candidates.map((c) => c.target).join(', ');
  const error = new Error(`no built Nemo app found; run npm run build:desktop (or scripts/nemo/build.cjs) first. Looked in: ${tried}`);
  error.code = 'ENOAPP';
  throw error;
}

// Everything the isolated instance needs, and nothing that would let it fall
// back to the shared production state. NEMO_TAURI_DATA_DIR is the activation
// switch the native side keys on; the other three are refused unless valid.
function nativeLaunchConfig(taskId, options = {}) {
  if (!options.executable) assertNativePlatform();
  const app = options.executable
    ? { from: 'explicit', bundle: null, executable: path.resolve(options.executable) }
    : resolveApp(taskId, options);
  if (!exists(app.executable)) throw new Error(`app executable not found: ${app.executable}`);
  fs.accessSync(app.executable, fs.constants.X_OK);
  const roots = isolation.taskRoots(taskId);
  const ownerToken = options.ownerToken || crypto.randomBytes(16).toString('hex');
  return {
    app,
    ownerToken,
    args: Array.isArray(options.args) ? options.args.slice() : [],
    cwd: ROOT,
    roots,
    reserve: Array.isArray(options.reserve) ? options.reserve.slice() : [],
    manifestFile: path.join(roots.tauriDataDir, APP_MANIFEST_FILE),
    manifestTimeoutMs: Number.isFinite(options.manifestTimeoutMs) ? options.manifestTimeoutMs : 15_000,
    stdoutLog: path.join(roots.reports, 'native-app.stdout.log'),
    stderrLog: path.join(roots.reports, 'native-app.stderr.log'),
    statusFile: path.join(roots.reports, STATUS_FILE),
    env: {
      ...process.env,
      NEMO_TASK_ID: taskId,
      NEMO_TASK_KEY: isolation.idKey(taskId),
      NEMO_TAURI_DATA_DIR: roots.tauriDataDir,
      NEMO_TASK_OWNER_TOKEN: ownerToken,
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
    reports: roots.reports,
    tauriDataDir: roots.tauriDataDir,
  };
}

function atomicWriteJson(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true, mode: 0o700 });
  const temp = `${file}.${process.pid}.tmp`;
  fs.writeFileSync(temp, JSON.stringify(value, null, 2) + '\n', { mode: 0o600 });
  fs.renameSync(temp, file);
}

function readNativeStatus(taskId) {
  const file = path.join(isolation.taskRoots(taskId).reports, STATUS_FILE);
  try { return JSON.parse(fs.readFileSync(file, 'utf8')); } catch { return null; }
}

// Read what the app disclosed about itself. A file that exists but is not this
// schema, or names another task, is reported as such rather than accepted:
// leftovers from an earlier run must not be read as this instance's identity.
function readAppManifest(taskId) {
  const file = path.join(isolation.taskRoots(taskId).tauriDataDir, APP_MANIFEST_FILE);
  let manifest;
  try { manifest = JSON.parse(fs.readFileSync(file, 'utf8')); }
  catch { return null; }
  if (!manifest || manifest.schema !== APP_MANIFEST_SCHEMA) {
    return { valid: false, reason: 'app manifest is not a nemo.native-runtime/1 document', file };
  }
  if (manifest.taskId !== taskId) {
    return { valid: false, reason: `app manifest belongs to task ${manifest.taskId}`, file, manifest };
  }
  return { valid: true, file, manifest };
}

async function waitForAppManifest(taskId, timeoutMs, alive) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const read = readAppManifest(taskId);
    if (read && read.valid) return read;
    if (Date.now() >= deadline || (alive && !alive())) {
      return read || { valid: false, reason: 'app did not write its runtime manifest', file: path.join(isolation.taskRoots(taskId).tauriDataDir, APP_MANIFEST_FILE) };
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

function spawnApp(config) {
  fs.mkdirSync(config.roots.reports, { recursive: true, mode: 0o700 });
  fs.mkdirSync(config.roots.tauriDataDir, { recursive: true, mode: 0o700 });
  const stdout = fs.createWriteStream(config.stdoutLog, { flags: 'a', mode: 0o600 });
  const stderr = fs.createWriteStream(config.stderrLog, { flags: 'a', mode: 0o600 });
  const child = spawn(config.app.executable, config.args, {
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
  drained.catch(() => {});
  return { child, closed, logsDone, drained };
}

// Source and build identity of the checkout, plus what the app itself
// disclosed. `ok` requires all three: owner, unchanged source/build, and a
// live app manifest that names this task.
function nativeHandshake(taskId, ownerToken) {
  const local = isolation.verifyHandshake(taskId, { ownerToken, checkSource: true });
  const launcher = isolation.readLauncher(taskId);
  const status = readNativeStatus(taskId);
  const app = readAppManifest(taskId);
  let currentBuild = null;
  try { currentBuild = identity.buildIdentity(); } catch { /* fail closed below */ }
  const buildMatches = !!status && isDeepStrictEqual(status.build.startup, currentBuild);
  const appOk = !!(app && app.valid && app.manifest.isolated && app.manifest.state === 'active');
  let reason = 'task owner, source and build identities match, and the app disclosed this task\'s isolated runtime';
  if (!local.ok) reason = local.reason;
  else if (!currentBuild) reason = 'build identity unavailable';
  else if (!buildMatches) reason = 'build identity changed';
  else if (!app) reason = 'app has not written its runtime manifest';
  else if (!app.valid) reason = app.reason;
  else if (!app.manifest.isolated) reason = 'app is running on the shared application state, not this task\'s roots';
  else if (app.manifest.state !== 'active') reason = `app runtime state is ${app.manifest.state}`;
  return {
    ok: local.ok && buildMatches && appOk,
    reason,
    taskId,
    pid: launcher ? launcher.pid : null,
    source: launcher ? { startup: publicSource(launcher.source), matches: local.ok } : null,
    build: status ? { startup: status.build.startup, current: currentBuild, matches: buildMatches } : null,
    app: app && app.valid ? app.manifest : app,
    runtime: status,
  };
}

async function runNativeLauncher(taskId, options = {}, emit = () => {}) {
  const config = nativeLaunchConfig(taskId, options);
  let startupBuild;
  try { startupBuild = identity.buildIdentity(); }
  catch (err) {
    isolation.releaseTask(taskId);
    throw new Error(`build identity unavailable: ${err.message}`);
  }
  // Shared exclusive resources are reserved BEFORE the app starts and only when
  // asked for. Two isolated instances are meant to run at the same time; it is
  // driving the one keyboard/mouse, or measuring the one GPU, that has to be
  // serialized (R06 acceptance: "explicit exclusive reservations").
  const reservations = [];
  for (const slot of config.reserve) {
    const acquired = isolation.acquireExclusiveSlot(slot, taskId, { pid: process.pid });
    if (!acquired.acquired) {
      for (const held of reservations) held.release();
      isolation.releaseTask(taskId);
      throw new Error(`exclusive resource "${slot}" unavailable: ${acquired.reason}`);
    }
    reservations.push(acquired);
  }
  let launcher;
  try {
    launcher = isolation.registerLauncher(taskId, {
      pid: process.pid,
      ownerToken: config.ownerToken,
      label: 'native-app',
    });
  } catch (err) {
    for (const held of reservations) held.release();
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
    app: { from: config.app.from, bundle: config.app.bundle, executable: config.app.executable },
    reservations: config.reserve,
    roots: publicRoots(config.roots),
    manifestFile: config.manifestFile,
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
    app: status.app,
    reservations: config.reserve,
    roots: publicRoots(config.roots),
    manifestFile: config.manifestFile,
    statusFile: config.statusFile,
    source: publicSource(launcher.source),
    build: startupBuild,
  });

  let processInfo = null;
  let released = false;
  const releaseReservations = () => {
    if (released) return [];
    released = true;
    return reservations.map((held) => ({ slot: held.slot, ...held.release() }));
  };
  let closing = false;
  const shutdown = async (signal) => {
    if (closing) return;
    closing = true;
    let tree;
    try { tree = await buildRuntime.stopBuild(processInfo && processInfo.child); }
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
    save({ state: 'stopped', finishedAt: nowIso(), signal: signal || 'SIGTERM', processTree: tree, slotRelease: releaseReservations() });
    process.exit(0);
  };
  process.on('SIGTERM', () => shutdown('SIGTERM'));
  process.on('SIGINT', () => shutdown('SIGINT'));
  // Keep the owning process alive for as long as the instance is addressable,
  // exactly as the build launcher does.
  setInterval(() => {}, 60_000);

  try {
    processInfo = spawnApp(config);
    await new Promise((resolve, reject) => {
      const failed = (err) => { processInfo.child.off('spawn', ready); reject(err); };
      const ready = () => { processInfo.child.off('error', failed); resolve(); };
      processInfo.child.once('error', failed);
      processInfo.child.once('spawn', ready);
    });
    save({ state: 'active', childPid: processInfo.child.pid });
    const manifest = await waitForAppManifest(
      taskId,
      config.manifestTimeoutMs,
      () => buildRuntime.buildProcessTreeAlive(processInfo.child),
    );
    save(manifest.valid
      ? { appRuntime: manifest.manifest }
      : { appRuntime: null, appRuntimeError: manifest.reason });
    emit({ appRuntime: manifest.valid ? manifest.manifest : null, appRuntimeError: manifest.valid ? null : manifest.reason });

    const result = await processInfo.closed;
    if (closing) return;
    let tree;
    try { tree = await buildRuntime.stopBuild(processInfo.child); }
    catch (err) { tree = { stopped: false, forced: false, reason: err.message }; }
    if (!tree.stopped) {
      save({ state: 'reconciliation-required', finishedAt: nowIso(), exitCode: result.code, signal: result.signal, processTree: tree });
      return;
    }
    await processInfo.logsDone.catch(() => {});
    save({
      state: result.code === 0 ? 'exited' : 'failed',
      finishedAt: nowIso(),
      exitCode: result.code,
      signal: result.signal,
      processTree: tree,
      slotRelease: releaseReservations(),
    });
  } catch (err) {
    let tree = { stopped: true, forced: false, reason: 'app process was not spawned' };
    if (processInfo) {
      try { tree = await buildRuntime.stopBuild(processInfo.child); }
      catch (stopErr) { tree = { stopped: false, forced: false, reason: stopErr.message }; }
    }
    if (!tree.stopped) {
      save({ state: 'reconciliation-required', finishedAt: nowIso(), error: err.message, processTree: tree });
      return;
    }
    if (processInfo) await processInfo.logsDone.catch(() => {});
    save({ state: 'failed', finishedAt: nowIso(), error: err.message, processTree: tree, slotRelease: releaseReservations() });
  }
}

module.exports = {
  SCHEMA,
  STATUS_FILE,
  APP_MANIFEST_FILE,
  APP_MANIFEST_SCHEMA,
  assertNativePlatform,
  bundleExecutable,
  resolveApp,
  nativeLaunchConfig,
  readNativeStatus,
  readAppManifest,
  nativeHandshake,
  runNativeLauncher,
};
