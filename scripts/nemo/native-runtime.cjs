#!/usr/bin/env node
'use strict';
// Isolated native desktop launcher (R06, https://github.com/mysteropodes/nemo/issues/902).
//
// `engineering/runtime-isolation.md` records the open gap this closes:
// "Merely creating a `tauri-data` directory does not make the app use it."
// `lib/isolation.cjs` has always created that directory; nothing pointed the
// app at it. `src-tauri/src/task_runtime.rs` is the app-side override, and
// this file is the launcher that configures it — the two halves share one
// environment contract and nothing else.
//
// Both a module and a CLI on purpose: `scripts/nemo/native-runtime.cjs` is
// this work package's declared path, so the launch/handshake logic and its
// entry point stay in it rather than widening ownership into `lib/`.
//
// Read-only consumers of R02: `lib/isolation.cjs` (task roots, ownership,
// slots), `lib/identity.cjs` (source/build identity) and `lib/util.cjs`.
// Nothing here edits them, `lib/jobs.cjs` or `package.json`.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn, spawnSync } = require('node:child_process');
const { isDeepStrictEqual } = require('node:util');
const isolation = require('./lib/isolation.cjs');
const identity = require('./lib/identity.cjs');
const { ROOT, exists, nowIso } = require('./lib/util.cjs');

const SCHEMA = 'nemo.native-runtime/1';
const STATUS_FILE = 'native-runtime.json';
const INSTANCE_FILE = 'instance.json';
// Two isolated instances may run at once — that is the point. Exclusive
// *human input* is a different resource: only one of them can own the
// keyboard and pointer during a paired validation run. It is a separate,
// explicitly acquired slot so launching never serializes on it.
const INPUT_SLOT = 'desktop-input';

function assertNativePlatform(platform = process.platform) {
  if (platform !== 'darwin') {
    throw new Error('isolated native launch currently supports macOS only: the WebKit website-data identity and .app bundle layout below are macOS-specific and unvalidated elsewhere');
  }
}

// RFC 4122 v4. The app refuses the nil UUID, so a fresh random one per task
// instance is what actually keeps two WKWebView website data stores apart.
function webDataUuid(bytes = crypto.randomBytes(16)) {
  const b = Buffer.from(bytes);
  b[6] = (b[6] & 0x0f) | 0x40;
  b[8] = (b[8] & 0x3f) | 0x80;
  const hex = b.toString('hex');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

// A website-data identity belongs to the TASK, not to one launch of it.
// `localStorage` is where the app keeps `nemo-auto`, recents, the sync-folder
// setting and the feedback fallback; a fresh UUID per launch would give the
// same task a brand-new empty WKWebView store every time it is relaunched, so
// its own autosave would be invisible to it while its on-disk history was
// still there. Allocated once, next to the state it identifies (the app-side
// `webkit/` root exists for exactly this), and removed with the task roots on
// release — a task that has been released must never inherit the previous
// tenant's website data.
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const NIL_UUID = '00000000-0000-0000-0000-000000000000';
const WEB_DATA_IDENTITY_FILE = path.join('webkit', 'identity.json');

function validWebDataUuid(value) {
  return typeof value === 'string' && UUID_RE.test(value) && value !== NIL_UUID;
}

function taskWebDataUuid(dataRoot, generate = webDataUuid) {
  const file = path.join(dataRoot, WEB_DATA_IDENTITY_FILE);
  const existing = readJsonOrNull(file);
  if (existing && validWebDataUuid(existing.webDataUuid)) return existing.webDataUuid;
  const uuid = generate();
  if (!validWebDataUuid(uuid)) throw new Error(`refusing an invalid website-data identity: ${uuid}`);
  atomicWriteJson(file, { schema: SCHEMA, webDataUuid: uuid, createdAt: nowIso() });
  return uuid;
}

// The OS-side half of a released task. `releaseTask` removes the task roots;
// WebKit's own store for this identity lives in the user profile and would
// otherwise outlive every task that ever ran, holding that task's
// localStorage and IndexedDB. Only a directory whose FINAL component is
// exactly this UUID, directly under a known per-identifier website-data
// parent, is ever removed.
function webDataStoreDirs(identifier, uuid, home = process.env.HOME) {
  if (!home || typeof identifier !== 'string' || !identifier || !validWebDataUuid(uuid)) return [];
  return ['WebsiteDataStore', 'CustomWebsiteData']
    .map((kind) => path.join(home, 'Library', 'WebKit', identifier, kind, uuid))
    .filter((dir) => path.basename(dir) === uuid && exists(dir));
}

function removeWebDataStore(identifier, uuid, home = process.env.HOME) {
  if (!validWebDataUuid(uuid)) return { removed: [], reason: 'no valid website-data identity recorded for this task' };
  if (!identifier) return { removed: [], reason: 'the app wrote no bundle identifier; its website-data store was left in place' };
  const dirs = webDataStoreDirs(identifier, uuid, home);
  const removed = [];
  for (const dir of dirs) {
    try { fs.rmSync(dir, { recursive: true, force: true }); removed.push(dir); } catch { /* reported as not removed */ }
  }
  return {
    removed,
    reason: removed.length ? 'website-data store removed with the task roots'
      : 'no website-data directory carrying this identity was found under this HOME',
  };
}

// The bundle's OWN answer to "which of these is the app". Nemo.app ships the
// ffmpeg sidecar next to the app binary in Contents/MacOS, and readdir returns
// `ffmpeg` first: taking the first executable entry launched the sidecar,
// which exits 1 on its usage message while every launcher record still said
// "isolated instance started". Info.plist is the only authority here, and
// plutil reads both the XML and the binary form.
function bundleExecutableName(appPath) {
  const plist = path.join(appPath, 'Contents', 'Info.plist');
  if (!exists(plist)) return null;
  const r = spawnSync('/usr/bin/plutil', ['-extract', 'CFBundleExecutable', 'raw', '-o', '-', plist], { encoding: 'utf8' });
  if (r.status !== 0) return null;
  const name = String(r.stdout || '').trim();
  // A name with a separator would escape Contents/MacOS entirely.
  return name && !name.includes('/') && name !== '.' && name !== '..' ? name : null;
}

// Accepts a bundle or a bare executable. A `.app` is a directory; spawning it
// directly fails with EACCES, which reads as a permissions problem rather
// than "you passed a bundle", so resolve it here.
function resolveExecutable(target) {
  const resolved = path.resolve(target);
  if (!exists(resolved)) throw new Error(`app not found: ${resolved}`);
  if (resolved.endsWith('.app')) {
    const macos = path.join(resolved, 'Contents', 'MacOS');
    const declared = bundleExecutableName(resolved);
    if (!declared) {
      throw new Error(`${resolved} declares no CFBundleExecutable; refusing to guess which binary in ${macos} is the app (it also ships sidecars)`);
    }
    const binary = path.join(macos, declared);
    if (!exists(binary)) throw new Error(`${resolved} declares CFBundleExecutable "${declared}", which is missing from ${macos}; rebuild the app`);
    fs.accessSync(binary, fs.constants.X_OK);
    return binary;
  }
  fs.accessSync(resolved, fs.constants.X_OK);
  return resolved;
}

function defaultAppPath(root = ROOT, triple = null) {
  const host = triple || identity.hostTriple();
  const candidates = [
    host && path.join(root, 'src-tauri', 'target', host, 'release', 'bundle', 'macos', 'Nemo.app'),
    path.join(root, 'src-tauri', 'target', 'release', 'bundle', 'macos', 'Nemo.app'),
    path.join(root, 'src-tauri', 'target', 'debug', 'bundle', 'macos', 'Nemo.app'),
  ].filter(Boolean);
  const found = candidates.find(exists);
  if (!found) {
    throw new Error(`no built Nemo.app found; looked in ${candidates.join(', ')} — run the desktop build first (scripts/nemo/build.cjs)`);
  }
  return found;
}

// The complete environment contract with src-tauri/src/task_runtime.rs.
// NEMO_TASK_DATA_ROOT is the only activation trigger on the app side, so an
// unrelated process inheriting NEMO_TASK_ID never flips into isolation.
function launchConfig(taskId, options = {}) {
  if (!options.command) assertNativePlatform();
  const executable = options.command
    ? resolveExecutable(options.command)
    : resolveExecutable(options.app || defaultAppPath(ROOT, options.hostTriple));
  const roots = isolation.taskRoots(taskId);
  // Reuse the `tauri-data` root lib/isolation.cjs already creates and already
  // reaps in releaseTask(); this adds no second lifecycle to reconcile.
  const dataRoot = roots.tauriDataDir;
  const uuid = options.webDataUuid || taskWebDataUuid(dataRoot);
  let source = null;
  try { source = identity.sourceIdentity(); } catch { source = null; }
  const declaredSource = source ? { ...source, originUrl: undefined } : null;
  return {
    executable,
    args: Array.isArray(options.args) ? options.args.slice() : [],
    cwd: ROOT,
    roots,
    dataRoot,
    webDataUuid: uuid,
    instanceFile: path.join(dataRoot, INSTANCE_FILE),
    statusFile: path.join(roots.reports, STATUS_FILE),
    stdoutLog: path.join(roots.reports, 'desktop-app.stdout.log'),
    stderrLog: path.join(roots.reports, 'desktop-app.stderr.log'),
    env: {
      ...process.env,
      NEMO_TASK_ID: taskId,
      NEMO_TASK_DATA_ROOT: dataRoot,
      NEMO_TASK_OWNER_TOKEN: options.ownerToken || '',
      NEMO_TASK_WEB_DATA_UUID: uuid,
      NEMO_TASK_SOURCE_IDENTITY: JSON.stringify(declaredSource),
      NEMO_REPORT_DIR: roots.reports,
      XDG_CACHE_HOME: roots.cache,
    },
  };
}

function atomicWriteJson(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true, mode: 0o700 });
  const temp = `${file}.${process.pid}.tmp`;
  fs.writeFileSync(temp, JSON.stringify(value, null, 2) + '\n', { mode: 0o600 });
  fs.renameSync(temp, file);
}

function readJsonOrNull(file) {
  try { return JSON.parse(fs.readFileSync(file, 'utf8')); } catch { return null; }
}

function readNativeStatus(taskId) {
  return readJsonOrNull(path.join(isolation.taskRoots(taskId).reports, STATUS_FILE));
}

/// The app writes this itself, inside the root it resolved. It is the one
/// piece of evidence that proves the override took effect rather than that
/// the launcher merely asked for it.
function readInstanceRecord(taskId) {
  const status = readNativeStatus(taskId);
  const dataRoot = status && status.dataRoot ? status.dataRoot : isolation.taskRoots(taskId).tauriDataDir;
  return readJsonOrNull(path.join(dataRoot, INSTANCE_FILE));
}

function nativeHandshake(taskId, ownerToken) {
  const local = isolation.verifyHandshake(taskId, { ownerToken, checkSource: true });
  const launcher = isolation.readLauncher(taskId);
  const status = readNativeStatus(taskId);
  const instance = readInstanceRecord(taskId);
  let currentBuild = null;
  try { currentBuild = identity.buildIdentity(); } catch { /* fail closed below */ }
  const buildMatches = !!status && isDeepStrictEqual(status.build.startup, currentBuild);
  // A launcher record proves the launcher; the instance record proves the
  // app. Both are required before calling an instance isolated.
  const isolationObserved = !!instance
    && !!status
    && instance.taskId === taskId
    && instance.webDataUuid === status.webDataUuid;
  return {
    schema: SCHEMA,
    ok: local.ok && buildMatches && isolationObserved,
    reason: !local.ok ? local.reason
      : !currentBuild ? 'build identity unavailable'
        : !buildMatches ? 'build identity changed'
          : !instance ? 'app has not written its instance record; isolated roots unconfirmed'
            : !isolationObserved ? 'instance record does not match this task/website-data identity'
              : 'task owner, source and build identities match and the app confirmed its isolated roots',
    taskId,
    pid: launcher ? launcher.pid : null,
    dataRoot: status ? status.dataRoot : null,
    webDataUuid: status ? status.webDataUuid : null,
    build: status ? { startup: status.build.startup, current: currentBuild, matches: buildMatches } : null,
    instance,
    runtime: status,
  };
}

function processAlive(child) {
  return !!child && child.exitCode == null && child.signalCode == null;
}

async function stopApp(child, graceMs = 3000) {
  if (!processAlive(child)) return { stopped: true, forced: false, reason: 'app process already exited' };
  try { child.kill('SIGTERM'); } catch (err) { if (err.code !== 'ESRCH') throw err; }
  const deadline = Date.now() + graceMs;
  while (processAlive(child) && Date.now() < deadline) await new Promise((r) => setTimeout(r, 20));
  if (!processAlive(child)) return { stopped: true, forced: false, reason: 'app exited after SIGTERM' };
  try { child.kill('SIGKILL'); } catch (err) { if (err.code !== 'ESRCH') throw err; }
  const hard = Date.now() + 2000;
  while (processAlive(child) && Date.now() < hard) await new Promise((r) => setTimeout(r, 20));
  return processAlive(child)
    ? { stopped: false, forced: true, reason: 'app still running after SIGKILL; reconciliation required' }
    : { stopped: true, forced: true, reason: 'app required SIGKILL' };
}

async function runNativeLauncher(taskId, options = {}, emit = () => {}) {
  let startupBuild;
  try { startupBuild = identity.buildIdentity(); }
  catch (err) {
    isolation.releaseTask(taskId);
    throw new Error(`build identity unavailable: ${err.message}`);
  }
  const ownerToken = crypto.randomBytes(16).toString('hex');
  const config = launchConfig(taskId, { ...options, ownerToken });
  const launcher = isolation.registerLauncher(taskId, { pid: process.pid, ownerToken, label: 'desktop-app' });
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
    executable: config.executable,
    dataRoot: config.dataRoot,
    webDataUuid: config.webDataUuid,
    roots: { root: config.roots.root, reports: config.roots.reports, cache: config.roots.cache },
    logs: { stdout: config.stdoutLog, stderr: config.stderrLog },
    source: launcher.source ? { ...launcher.source, originUrl: undefined } : null,
    build: { startup: startupBuild },
  };
  const save = (change) => { status = { ...status, ...change }; atomicWriteJson(config.statusFile, status); };
  fs.mkdirSync(config.dataRoot, { recursive: true, mode: 0o700 });
  fs.mkdirSync(config.roots.reports, { recursive: true, mode: 0o700 });
  save({});
  emit({
    started: true, taskId, pid: process.pid, ownerToken,
    executable: config.executable, dataRoot: config.dataRoot, webDataUuid: config.webDataUuid,
    statusFile: config.statusFile, instanceFile: config.instanceFile,
    source: status.source, build: startupBuild,
  });

  const stdout = fs.createWriteStream(config.stdoutLog, { flags: 'a', mode: 0o600 });
  const stderr = fs.createWriteStream(config.stderrLog, { flags: 'a', mode: 0o600 });
  const child = spawn(config.executable, config.args, {
    cwd: config.cwd, env: config.env, stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true,
  });
  child.stdout.pipe(stdout);
  child.stderr.pipe(stderr);
  const exited = new Promise((resolve) => child.once('exit', (code, signal) => resolve({ code, signal })));

  let closing = false;
  const shutdown = async () => {
    if (closing) return;
    closing = true;
    const tree = await stopApp(child);
    if (!tree.stopped) { save({ state: 'reconciliation-required', finishedAt: nowIso(), app: tree }); closing = false; return; }
    save({ state: 'stopped', finishedAt: nowIso(), app: tree });
    process.exit(0);
  };
  process.on('SIGTERM', shutdown);
  process.on('SIGINT', shutdown);
  // Keep ownership addressable after the app exits, exactly like the build
  // launcher: a dead holder would otherwise look reclaimable.
  setInterval(() => {}, 60_000);

  try {
    await new Promise((resolve, reject) => {
      const failed = (err) => { child.off('spawn', ready); reject(err); };
      const ready = () => { child.off('error', failed); resolve(); };
      child.once('error', failed);
      child.once('spawn', ready);
    });
    save({ state: 'active', childPid: child.pid });
    const result = await exited;
    if (closing) return;
    save({
      state: result.code === 0 ? 'completed' : 'failed',
      finishedAt: nowIso(), exitCode: result.code, signal: result.signal,
      instance: readJsonOrNull(config.instanceFile),
    });
  } catch (err) {
    const tree = await stopApp(child);
    save({ state: tree.stopped ? 'failed' : 'reconciliation-required', finishedAt: nowIso(), error: err.message, app: tree });
  }
}

// ---- CLI ----------------------------------------------------------------

function parse(argv) {
  const out = { _: [], passthrough: [] };
  let passthrough = false;
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (passthrough) { out.passthrough.push(arg); continue; }
    if (arg === '--') { passthrough = true; continue; }
    if (!arg.startsWith('--')) { out._.push(arg); continue; }
    const key = arg.slice(2); const next = argv[i + 1];
    if (next == null || next.startsWith('--')) out[key] = true;
    else { out[key] = next; i += 1; }
  }
  return out;
}

function output(value) { process.stdout.write(JSON.stringify(value) + '\n'); }

async function main() {
  const args = parse(process.argv.slice(2));
  const command = args._[0] || 'start';
  if (command === 'start') {
    const taskId = isolation.resolveTaskId(args.task);
    return runNativeLauncher(taskId, { app: args.app, command: args.command, args: args.passthrough, hostTriple: args['host-triple'] }, output);
  }
  if (command === 'paths') {
    const taskId = isolation.resolveTaskId(args.task);
    const config = launchConfig(taskId, { command: args.command, app: args.app });
    return output({ schema: SCHEMA, taskId, dataRoot: config.dataRoot, webDataUuid: config.webDataUuid, executable: config.executable, statusFile: config.statusFile });
  }
  if (command === 'status') {
    if (!args.task || !args.owner) throw new Error('status requires --task ID and --owner TOKEN');
    const result = nativeHandshake(args.task, args.owner);
    output(result);
    return process.exit(result.ok ? 0 : 1);
  }
  if (command === 'stop') {
    if (!args.task || !args.owner) throw new Error('stop requires --task ID and --owner TOKEN');
    const stopped = await isolation.requestStop(args.task, args.owner, { timeoutMs: args['timeout-ms'] == null ? 10_000 : Number(args['timeout-ms']) });
    const before = readNativeStatus(args.task);
    const instance = readInstanceRecord(args.task);
    const released = stopped.stopped ? isolation.releaseTask(args.task, args.owner) : null;
    // The task roots are gone; WebKit's own store for this instance lives in
    // the user profile and has to be removed by identity or a released task
    // leaves its localStorage and IndexedDB behind. `instance` was read above,
    // before the roots were removed — it is the only record of the bundle
    // identifier the app actually ran under.
    const webData = released && released.released
      ? removeWebDataStore(instance && instance.identifier, before && before.webDataUuid)
      : { removed: [], reason: 'task not released; its website-data store was left in place' };
    output({ ...stopped, runtime: before, instance, released, webData });
    return process.exit(stopped.stopped && released && released.released ? 0 : 1);
  }
  // Exclusive human input, for a paired two-instance validation run.
  if (command === 'input-acquire') {
    if (!args.task) throw new Error('input-acquire requires --task ID');
    const result = isolation.acquireExclusiveSlot(INPUT_SLOT, args.task, { pid: args.pid == null ? process.ppid : Number(args.pid) });
    output({ slot: INPUT_SLOT, acquired: result.acquired, reason: result.reason || null, ownerToken: result.ownerToken || null, holder: result.holder || null });
    return process.exit(result.acquired ? 0 : 1);
  }
  if (command === 'input-release') {
    if (!args.owner) throw new Error('input-release requires --owner TOKEN');
    const result = isolation.releaseExclusiveSlot(INPUT_SLOT, args.owner);
    output({ slot: INPUT_SLOT, ...result });
    return process.exit(result.released ? 0 : 1);
  }
  throw new Error('usage: native-runtime.cjs start [--task ID] [--app /abs/Nemo.app | --command /abs/stub -- args...] | paths --task ID | status --task ID --owner TOKEN | stop --task ID --owner TOKEN | input-acquire --task ID [--pid N] | input-release --owner TOKEN');
}

module.exports = {
  SCHEMA, STATUS_FILE, INSTANCE_FILE, INPUT_SLOT,
  assertNativePlatform, webDataUuid, taskWebDataUuid, validWebDataUuid, bundleExecutableName,
  webDataStoreDirs, removeWebDataStore, resolveExecutable, defaultAppPath,
  launchConfig, readNativeStatus, readInstanceRecord, nativeHandshake,
  stopApp, runNativeLauncher,
};

if (require.main === module) {
  main().catch((err) => { process.stderr.write((err.stack || String(err)) + '\n'); process.exit(1); });
}
