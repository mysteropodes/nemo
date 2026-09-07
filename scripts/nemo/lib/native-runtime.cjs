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
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const { once } = require('node:events');
const { finished } = require('node:stream/promises');
const { isDeepStrictEqual } = require('node:util');
const isolation = require('./isolation.cjs');
const identity = require('./identity.cjs');
const buildRuntime = require('./build-runtime.cjs');
const { ROOT, exists, nowIso } = require('./util.cjs');

const { SCHEMA, launcherProcessIdentity, nativeProcessTreeStopped } = require('./native-process.cjs');
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

// The WebKit store lives under the BUNDLE identifier from Info.plist, not the
// per-task runtime identifier: macOS keys the container by bundle and the store
// by its UUID inside it. Observed on macOS 26.6 as
// ~/Library/WebKit/<bundle id>/WebsiteDataStore/<uuid>.
function bundleIdentifier(bundle) {
  const plist = path.join(bundle, 'Contents', 'Info.plist');
  if (!exists(plist)) return null;
  const match = /<key>CFBundleIdentifier<\/key>\s*<string>([^<]+)<\/string>/.exec(fs.readFileSync(plist, 'utf8'));
  return match ? match[1] : null;
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
  const exited = new Promise((resolve) => child.once('exit', (code, signal) => resolve({ code, signal })));
  const closed = new Promise((resolve) => child.once('close', (code, signal) => resolve({ code, signal })));
  const logsDone = Promise.all([finished(stdout), finished(stderr)]);
  const drained = Promise.all([closed, logsDone]);
  drained.catch(() => {});
  return { child, exited, closed, logsDone, drained };
}

// ---- releasing the state that lives OUTSIDE the task root ------------------
// The isolated app directories are `<platform dir>/<identifier>`, so they are
// NOT under the task root and `isolation.releaseTask` never sees them. Found by
// running it: two stopped instances left their Application Support, Caches and
// Logs directories behind for good.
//
// Cleanup is bound to THIS task's own derivation, never to whatever a manifest
// claims: only a directory whose last component carries this task's identifier
// suffix is eligible. A corrupted or hostile manifest naming the production
// `com.strokemotion.app` directory is refused by name instead of deleted.
function taskIdentifierSuffix(taskId) {
  return `.nemo-task-${isolation.idKey(taskId).slice(0, 16)}`;
}

// 32 hex characters as WebKit writes them: 8-4-4-4-12.
function dataStoreUuid(taskId) {
  const key = isolation.idKey(taskId).slice(0, 32);
  return [key.slice(0, 8), key.slice(8, 12), key.slice(12, 16), key.slice(16, 20), key.slice(20, 32)].join('-');
}

function webkitStoreDir(bundle, taskId) {
  const identifier = bundle ? bundleIdentifier(bundle) : null;
  if (!identifier) return null;
  return path.join(os.homedir(), 'Library', 'WebKit', identifier, 'WebsiteDataStore', dataStoreUuid(taskId));
}

function nativeStateTargets(taskId, manifest, bundle) {
  const suffix = taskIdentifierSuffix(taskId);
  const uuid = dataStoreUuid(taskId);
  const dirs = manifest && manifest.dirs && typeof manifest.dirs === 'object' ? manifest.dirs : {};
  const seen = new Set();
  const targets = [];
  const refused = [];
  for (const [name, value] of Object.entries(dirs)) {
    if (typeof value !== 'string' || !path.isAbsolute(value)) {
      refused.push({ name, value, reason: 'not an absolute path' });
      continue;
    }
    const dir = path.resolve(value);
    if (!path.basename(dir).endsWith(suffix)) {
      refused.push({ name, value: dir, reason: `does not carry this task's identifier suffix ${suffix}` });
      continue;
    }
    if (seen.has(dir)) continue;
    seen.add(dir);
    targets.push({ name, dir });
  }
  const store = webkitStoreDir(bundle, taskId);
  // Exact name match, so this can only ever address this task's own store.
  if (store && path.basename(store) === uuid && !seen.has(store)) targets.push({ name: 'webkitStore', dir: store });
  return { targets, refused };
}

function releaseNativeState(taskId, manifest, bundle) {
  const { targets, refused } = nativeStateTargets(taskId, manifest, bundle);
  const removed = [];
  for (const target of targets) {
    if (!exists(target.dir)) { removed.push({ ...target, removed: false, reason: 'already absent' }); continue; }
    try { fs.rmSync(target.dir, { recursive: true, force: true }); removed.push({ ...target, removed: true }); }
    catch (err) { removed.push({ ...target, removed: false, reason: err.message }); }
  }
  return { removed, refused };
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
  const buildMatches = !!(status && status.build) && isDeepStrictEqual(status.build.startup, currentBuild);
  const launcherOk = !!(launcher && status && status.schema === SCHEMA && status.taskId === taskId
    && status.launcherPid === launcher.pid && status.launcherIdentity
    && status.launcherIdentity === launcherProcessIdentity(launcher.pid) && status.state === 'active');
  const appMatches = !!(app && app.valid && status && Number.isSafeInteger(status.childPid)
    && status.childPid > 0 && app.manifest.pid === status.childPid && isolation.pidAlive(status.childPid));
  const appOk = !!(app && app.valid && app.manifest.isolated && app.manifest.state === 'active');
  let reason = 'task owner, source and build identities match, and the app disclosed this task\'s isolated runtime';
  if (!local.ok) reason = local.reason;
  else if (!currentBuild) reason = 'build identity unavailable';
  else if (!buildMatches) reason = 'build identity changed';
  else if (!app) reason = 'app has not written its runtime manifest';
  else if (!app.valid) reason = app.reason;
  else if (!app.manifest.isolated) reason = 'app is running on the shared application state, not this task\'s roots';
  else if (app.manifest.state !== 'active') reason = `app runtime state is ${app.manifest.state}`;
  else if (!launcherOk) reason = 'current native launcher identity or active state does not match';
  else if (!appMatches) reason = 'app manifest does not name the current live launcher child';
  return {
    ok: local.ok && buildMatches && appOk && launcherOk && appMatches,
    reason,
    taskId,
    pid: launcher ? launcher.pid : null,
    source: launcher ? { startup: publicSource(launcher.source), matches: local.ok } : null,
    build: status && status.build ? { startup: status.build.startup, current: currentBuild, matches: buildMatches } : null,
    app: app && app.valid ? app.manifest : app,
    runtime: status,
  };
}

async function runNativeLauncher(taskId, options = {}, emit = () => {}) {
  const config = nativeLaunchConfig(taskId, options);
  // A fresh public nonce in the OS-visible title distinguishes PID reuse even
  // when an identical Node command starts within the same clock second.
  const launcherTitle = `nemo-native-${crypto.randomBytes(16).toString('hex')}`;
  process.title = launcherTitle;
  const launcherIdentity = launcherProcessIdentity(process.pid);
  if (!launcherIdentity || !launcherIdentity.endsWith(launcherTitle)) throw new Error('launcher process identity unavailable');
  let startupBuild;
  // A start that fails must NOT call isolation.releaseTask: task ids are reused
  // across restarts (that is what makes an instance's state survive a stop), and
  // releasing here deleted the roots of a previous, legitimately stopped
  // instance that this launch never owned. Found by running it — a refused
  // reservation wiped the earlier run's records.
  try { startupBuild = identity.buildIdentity(); }
  catch (err) { throw new Error(`build identity unavailable: ${err.message}`); }
  // Shared exclusive resources are reserved BEFORE the app starts and only when
  // asked for. Two isolated instances are meant to run at the same time; it is
  // driving the one keyboard/mouse, or measuring the one GPU, that has to be
  // serialized (R06 acceptance: "explicit exclusive reservations").
  const reservations = [];
  for (const slot of config.reserve) {
    const acquired = isolation.acquireExclusiveSlot(slot, taskId, { pid: process.pid, releasePolicy: 'owner-confirmed' });
    if (!acquired.acquired) {
      for (const held of reservations) held.release({ verifyRelease: () => true });
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
    for (const held of reservations) held.release({ verifyRelease: () => true });
    throw err;
  }

  let status = {
    schema: SCHEMA,
    taskId,
    launcherPid: process.pid,
    launcherIdentity,
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
  let released = false;
  const releaseReservations = (tree) => {
    if (released) return [];
    released = true;
    return reservations.map((held) => ({ slot: held.slot, ...held.release({
      verifyRelease: () => !!(tree && tree.stopped) && (status.childPid === null || nativeProcessTreeStopped(taskId, launcher, status).stopped),
    }) }));
  };
  try {
    save({});
    // Invalidate only the prior handshake, after ownership but before readiness.
    // Failure must leave retained user data intact and release our reservations.
    fs.rmSync(config.manifestFile, { force: true });
  } catch (err) {
    const slotRelease = releaseReservations({ stopped: true }); // failure before spawnApp
    save({ state: 'failed', finishedAt: nowIso(), error: err.message,
      processTree: { stopped: true, forced: false, reason: 'app process was not spawned' }, slotRelease });
    throw err;
  }
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
    save({ state: 'stopped', finishedAt: nowIso(), signal: signal || 'SIGTERM', processTree: tree, slotRelease: releaseReservations(tree) });
    process.exit(0);
  };
  process.on('SIGTERM', () => shutdown('SIGTERM'));
  process.on('SIGINT', () => shutdown('SIGINT'));
  // Retain the owner while cleanup is pending or requires reconciliation.
  // Successful cleanup releases this hold so natural app exit ends the launcher.
  const keepAlive = setInterval(() => {}, 60_000);

  try {
    processInfo = spawnApp(config);
    await once(processInfo.child, 'spawn');
    save({ state: 'active', childPid: processInfo.child.pid });
    const manifest = await waitForAppManifest(
      taskId,
      config.manifestTimeoutMs,
      () => processInfo.child.exitCode == null && processInfo.child.signalCode == null,
    );
    save(manifest.valid
      ? { appRuntime: manifest.manifest }
      : { appRuntime: null, appRuntimeError: manifest.reason });
    emit({ appRuntime: manifest.valid ? manifest.manifest : null, appRuntimeError: manifest.valid ? null : manifest.reason });

    // Helpers may retain the app's pipes after its leader exits. Stop the owned
    // group on 'exit' before waiting for 'close' or log drainage.
    const result = await processInfo.exited;
    if (closing) return;
    let tree;
    try { tree = await buildRuntime.stopBuild(processInfo.child); }
    catch (err) { tree = { stopped: false, forced: false, reason: err.message }; }
    if (closing) return;
    if (!tree.stopped) {
      save({ state: 'reconciliation-required', finishedAt: nowIso(), exitCode: result.code, signal: result.signal, processTree: tree });
      return;
    }
    await processInfo.closed;
    await processInfo.logsDone.catch(() => {});
    save({
      state: result.code === 0 ? 'exited' : 'failed',
      finishedAt: nowIso(),
      exitCode: result.code,
      signal: result.signal,
      processTree: tree,
      slotRelease: releaseReservations(tree),
    });
    clearInterval(keepAlive);
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
    save({ state: 'failed', finishedAt: nowIso(), error: err.message, processTree: tree, slotRelease: releaseReservations(tree) });
    clearInterval(keepAlive);
  }
}

module.exports = {
  SCHEMA,
  STATUS_FILE,
  APP_MANIFEST_FILE,
  APP_MANIFEST_SCHEMA,
  assertNativePlatform,
  bundleExecutable,
  bundleIdentifier,
  taskIdentifierSuffix,
  dataStoreUuid,
  nativeStateTargets,
  releaseNativeState,
  resolveApp,
  nativeLaunchConfig,
  readNativeStatus,
  readAppManifest,
  nativeHandshake,
  runNativeLauncher,
};
