'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { after, test } = require('node:test');

const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-native-runtime-test-'));
process.env.NEMO_ISOLATION_ROOT = path.join(scratch, 'runtime');
const isolation = require('./lib/isolation.cjs');
const runtime = require('./lib/native-runtime.cjs');
const cli = path.join(__dirname, 'native.cjs');
const repoRoot = path.resolve(__dirname, '..', '..');

// Stands in for a built Nemo. It reproduces only the app's OBSERVABLE contract
// — the environment it is handed and the manifest it writes into that task's
// tauri-data root — so these tests exercise the launcher without a desktop,
// a GPU or a window. The identifier/data-store derivation itself is proved on
// the native side (src-tauri/src/task_runtime.rs unit tests).
const appStub = path.join(scratch, 'app-stub');
fs.writeFileSync(appStub, '#!/usr/bin/env node\n' + String.raw`
const fs = require('node:fs');
const path = require('node:path');
const [capture, mode = 'manifest', appDirBase = ''] = process.argv.slice(2);
const seen = {
  pid: process.pid,
  taskId: process.env.NEMO_TASK_ID,
  taskKey: process.env.NEMO_TASK_KEY,
  dataDir: process.env.NEMO_TAURI_DATA_DIR,
  ownerTokenPresent: !!process.env.NEMO_TASK_OWNER_TOKEN,
  reports: process.env.NEMO_REPORT_DIR,
  temp: process.env.TMPDIR,
  cache: process.env.XDG_CACHE_HOME,
};
fs.writeFileSync(capture, JSON.stringify(seen));
if (mode !== 'no-manifest') {
  const key = String(seen.taskKey || '');
  let dirs = {};
  if (appDirBase) {
    const suffix = '.nemo-task-' + key.slice(0, 16);
    const mine = path.join(appDirBase, 'AppSupport', 'com.strokemotion.app' + suffix);
    const production = path.join(appDirBase, 'AppSupport', 'com.strokemotion.app');
    const other = path.join(appDirBase, 'AppSupport', 'com.strokemotion.app.nemo-task-0123456789abcdef');
    for (const d of [mine, production, other]) {
      fs.mkdirSync(d, { recursive: true });
      fs.writeFileSync(path.join(d, 'history.json'), '{}');
    }
    dirs = { appData: mine, appLocalData: mine, appLog: production, appCache: other, appConfig: 'relative/path' };
  }
  const manifest = {
    schema: 'nemo.native-runtime/1',
    dirs,
    isolated: true,
    state: 'active',
    taskId: mode === 'foreign-manifest' ? 'another-task' : seen.taskId,
    taskKey: key,
    identifier: 'com.strokemotion.app.nemo-task-' + key.slice(0, 16),
    dataStoreIdentifier: key.slice(0, 32),
    dataDir: seen.dataDir,
    pid: process.pid,
  };
  fs.mkdirSync(seen.dataDir, { recursive: true });
  fs.writeFileSync(path.join(seen.dataDir, 'native-runtime.json'), JSON.stringify(manifest, null, 2));
  console.log(JSON.stringify(manifest));
}
process.on('SIGTERM', () => process.exit(0));
setInterval(() => {}, 1000);
`, { mode: 0o700 });

after(() => fs.rmSync(scratch, { recursive: true, force: true }));

function exited(child) {
  if (child.exitCode != null || child.signalCode != null) return Promise.resolve({ code: child.exitCode, signal: child.signalCode });
  return new Promise((resolve) => child.once('exit', (code, signal) => resolve({ code, signal })));
}

function firstLine(child) {
  return new Promise((resolve, reject) => {
    let data = ''; const timer = setTimeout(() => done(new Error('launcher readiness timed out')), 10_000);
    function done(error, value) { clearTimeout(timer); child.stdout.off('data', read); child.off('exit', early); error ? reject(error) : resolve(value); }
    function read(chunk) { data += chunk; const newline = data.indexOf('\n'); if (newline !== -1) done(null, data.slice(0, newline)); }
    function early(code) { done(new Error(`launcher exited before readiness (${code}): ${data}`)); }
    child.stdout.on('data', read); child.once('exit', early);
  });
}

async function launch(task, capture, { mode = 'manifest', reserve = null, appDirBase = '' } = {}) {
  const args = [cli, 'start', '--task', task, '--executable', appStub];
  if (reserve) args.push('--reserve', reserve);
  args.push('--manifest-timeout-ms', '5000', '--', capture, mode, appDirBase);
  const child = spawn(process.execPath, args, {
    cwd: repoRoot,
    env: { ...process.env },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  return { child, info: JSON.parse(await firstLine(child)) };
}

function command(args) {
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [cli, ...args], { cwd: repoRoot, env: { ...process.env }, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = ''; let stderr = '';
    child.stdout.on('data', (chunk) => { stdout += chunk; });
    child.stderr.on('data', (chunk) => { stderr += chunk; });
    child.once('exit', (code, signal) => resolve({ code, signal, stdout, stderr }));
  });
}

async function waitFor(check, what, timeoutMs = 5000) {
  const deadline = Date.now() + timeoutMs;
  let value = null;
  while (Date.now() < deadline) {
    value = check();
    if (value) return value;
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  throw new Error(`timed out waiting for ${what}`);
}

async function stopOwned(instance) {
  if (!instance || !isolation.pidAlive(instance.child.pid)) return;
  await command(['stop', '--task', instance.info.taskId, '--owner', instance.info.ownerToken]);
  await exited(instance.child);
}

test('the launcher and the native module name the same environment contract', () => {
  // The activation switch and its three companions are read in Rust and
  // written here. A rename on one side only would silently return the app to
  // the shared production identifier, which is the failure R06 exists to stop.
  const native = fs.readFileSync(path.join(repoRoot, 'src-tauri', 'src', 'task_runtime.rs'), 'utf8');
  const config = runtime.nativeLaunchConfig('env-contract', { executable: appStub });
  for (const name of ['NEMO_TAURI_DATA_DIR', 'NEMO_TASK_ID', 'NEMO_TASK_KEY', 'NEMO_TASK_OWNER_TOKEN']) {
    assert.match(native, new RegExp(`"${name}"`), `${name} must be read by src-tauri/src/task_runtime.rs`);
    assert.ok(config.env[name], `${name} must be exported to the app`);
  }
  assert.match(native, new RegExp(`"${runtime.APP_MANIFEST_SCHEMA}"`));
  assert.match(native, new RegExp(`"${runtime.APP_MANIFEST_FILE}"`));
  assert.equal(config.env.NEMO_TASK_KEY, isolation.idKey('env-contract'));
  assert.equal(config.env.NEMO_TAURI_DATA_DIR, config.roots.tauriDataDir);
  assert.equal(config.env.NEMO_TASK_OWNER_TOKEN, config.ownerToken);
  assert.throws(() => runtime.assertNativePlatform('win32'), /supports macOS only/);
});

test('two instances run concurrently on disjoint native state', async () => {
  const captureA = path.join(scratch, 'capture-a.json');
  const captureB = path.join(scratch, 'capture-b.json');
  let a; let b;
  try {
    a = await launch(`native-a-${process.pid}`, captureA);
    b = await launch(`native-b-${process.pid}`, captureB);
    const seenA = JSON.parse(await waitFor(() => fs.existsSync(captureA) && fs.readFileSync(captureA, 'utf8'), 'instance A environment'));
    const seenB = JSON.parse(await waitFor(() => fs.existsSync(captureB) && fs.readFileSync(captureB, 'utf8'), 'instance B environment'));

    assert.ok(isolation.pidAlive(a.child.pid) && isolation.pidAlive(b.child.pid), 'both launchers stay live');
    const identities = [a, b].map((instance) => runtime.readNativeStatus(instance.info.taskId).launcherIdentity);
    for (const identity of identities) assert.match(identity, /nemo-native-[0-9a-f]{32}$/);
    assert.notEqual(identities[0], identities[1], 'same-command launchers have distinct OS process identities');
    for (const field of ['taskId', 'taskKey', 'dataDir', 'reports', 'temp', 'cache']) {
      assert.notEqual(seenA[field], seenB[field], `${field} must differ between instances`);
    }
    assert.equal(seenA.taskKey, isolation.idKey(seenA.taskId));
    assert.equal(seenB.taskKey, isolation.idKey(seenB.taskId));
    assert.ok(seenA.ownerTokenPresent && seenB.ownerTokenPresent);
    // Distinct keys are what the identifier and WebKit data store are derived
    // from on the native side, so distinct keys are the isolation claim here.
    assert.notEqual(seenA.taskKey.slice(0, 16), seenB.taskKey.slice(0, 16));

    for (const instance of [a, b]) {
      await waitFor(() => runtime.readNativeStatus(instance.info.taskId).appRuntime, 'claimed app manifest');
    }
    const handshakeA = runtime.nativeHandshake(a.info.taskId, a.info.ownerToken);
    assert.equal(handshakeA.ok, true, handshakeA.reason);
    assert.equal(handshakeA.app.taskId, a.info.taskId);
    assert.notEqual(handshakeA.app.identifier, runtime.nativeHandshake(b.info.taskId, b.info.ownerToken).app.identifier);
  } finally {
    await stopOwned(a);
    await stopOwned(b);
  }
});

test('an instance is only accepted when the app itself disclosed this task', async () => {
  const silent = path.join(scratch, 'capture-silent.json');
  const foreign = path.join(scratch, 'capture-foreign.json');
  let a; let b;
  try {
    a = await launch(`native-silent-${process.pid}`, silent, { mode: 'no-manifest' });
    const quiet = runtime.nativeHandshake(a.info.taskId, a.info.ownerToken);
    assert.equal(quiet.ok, false);
    assert.match(quiet.reason, /runtime manifest/);

    b = await launch(`native-foreign-${process.pid}`, foreign, { mode: 'foreign-manifest' });
    await waitFor(() => runtime.readAppManifest(b.info.taskId), 'foreign app manifest');
    const mismatched = runtime.nativeHandshake(b.info.taskId, b.info.ownerToken);
    assert.equal(mismatched.ok, false);
    assert.match(mismatched.reason, /belongs to task another-task/);

    const wrongOwner = runtime.nativeHandshake(b.info.taskId, 'not-the-owner');
    assert.equal(wrongOwner.ok, false);
    assert.match(wrongOwner.reason, /owner token mismatch/);
  } finally {
    await stopOwned(a);
    await stopOwned(b);
  }
});

test('only the owner stops an instance, and stopping removes its native state', async () => {
  const capture = path.join(scratch, 'capture-stop.json');
  const instance = await launch(`native-stop-${process.pid}`, capture);
  try {
    const dataDir = isolation.taskRoots(instance.info.taskId).tauriDataDir;
    await waitFor(() => fs.existsSync(path.join(dataDir, runtime.APP_MANIFEST_FILE)), 'app manifest');

    const refused = await command(['stop', '--task', instance.info.taskId, '--owner', 'not-the-owner']);
    assert.equal(refused.code, 1);
    assert.match(refused.stdout, /not the task owner/);
    assert.ok(isolation.pidAlive(instance.child.pid), 'a refused stop leaves the instance running');

    const stopped = await command(['stop', '--task', instance.info.taskId, '--owner', instance.info.ownerToken]);
    assert.equal(stopped.code, 0, stopped.stderr || stopped.stdout);
    await exited(instance.child);
    assert.equal(fs.existsSync(dataDir), false, 'the task tauri-data root is removed on release');
  } finally {
    await stopOwned(instance);
  }
});

test('a shared desktop resource is reserved explicitly and refused to a second holder', async () => {
  const capture = path.join(scratch, 'capture-reserve.json');
  const slot = `desktop-input-test-${process.pid}`;
  const instance = await launch(`native-reserve-a-${process.pid}`, capture, { reserve: slot });
  try {
    const held = await waitFor(() => runtime.readNativeStatus(instance.info.taskId), 'launcher status');
    assert.deepEqual(held.reservations, [slot]);

    const refused = await command([
      'start', '--task', `native-reserve-b-${process.pid}`, '--executable', appStub,
      '--reserve', slot, '--', path.join(scratch, 'capture-reserve-b.json'), 'manifest',
    ]);
    assert.equal(refused.code, 1);
    assert.match(refused.stderr, /exclusive resource "[^"]+" unavailable/);

    // Concurrency itself is not what the slot restricts: a second instance
    // that does not ask for the shared resource still starts.
    const free = await launch(`native-reserve-c-${process.pid}`, path.join(scratch, 'capture-reserve-c.json'));
    await stopOwned(free);
  } finally {
    await stopOwned(instance);
  }
  const after = isolation.acquireExclusiveSlot(slot, `native-reserve-after-${process.pid}`);
  assert.equal(after.acquired, true, `slot must be free after the owner stopped: ${after.reason}`);
  isolation.releaseExclusiveSlot(slot, after.ownerToken);
});

test('a missing build is a named blocker, and a bundle names its own executable', () => {
  const missing = path.join(scratch, 'no-such-target');
  assert.throws(
    () => runtime.resolveApp(`native-missing-${process.pid}`, { hostTriple: 'aarch64-test-host' }),
    (err) => err.code === 'ENOAPP' && /Looked in: .*aarch64-test-host/.test(err.message),
  );
  assert.equal(fs.existsSync(missing), false);

  const bundle = path.join(scratch, 'Fake.app');
  fs.mkdirSync(path.join(bundle, 'Contents', 'MacOS'), { recursive: true });
  fs.writeFileSync(path.join(bundle, 'Contents', 'Info.plist'), '<plist><dict><key>CFBundleExecutable</key><string>renamed-binary</string></dict></plist>');
  assert.throws(() => runtime.bundleExecutable(bundle), /bundle executable missing/);
  fs.writeFileSync(path.join(bundle, 'Contents', 'MacOS', 'renamed-binary'), '#!/bin/sh\n', { mode: 0o700 });
  assert.equal(runtime.bundleExecutable(bundle), path.join(bundle, 'Contents', 'MacOS', 'renamed-binary'));
  // The real R04 package ships the ffmpeg sidecar next to the app binary, and
  // readdir returns it FIRST there (observed: ['ffmpeg', 'nemo'] — and 'ffmpeg'
  // also sorts before 'nemo', so sorting the scan would not save it either).
  // Picking any executable found in Contents/MacOS therefore launches ffmpeg,
  // not the app; only the declared CFBundleExecutable is right.
  fs.writeFileSync(path.join(bundle, 'Contents', 'MacOS', 'ffmpeg'), '#!/bin/sh\n', { mode: 0o700 });
  assert.equal(runtime.bundleExecutable(bundle), path.join(bundle, 'Contents', 'MacOS', 'renamed-binary'));
  assert.throws(() => runtime.bundleExecutable(path.join(scratch, 'Nope.app')), /no Contents\/Info\.plist/);
});

test('stopping removes the isolated app directories and refuses everything else', async () => {
  // The isolated app directories are `<platform dir>/<identifier>`, so they sit
  // OUTSIDE the task root and isolation.releaseTask never sees them. Found by
  // running two real instances: both left their Application Support, Caches and
  // Logs directories behind after a clean stop.
  const base = fs.mkdtempSync(path.join(scratch, 'appdirs-'));
  const task = `native-appstate-${process.pid}`;
  const instance = await launch(task, path.join(scratch, 'capture-appstate.json'), { appDirBase: base });
  const suffix = runtime.taskIdentifierSuffix(task);
  const mine = path.join(base, 'AppSupport', `com.strokemotion.app${suffix}`);
  const production = path.join(base, 'AppSupport', 'com.strokemotion.app');
  const other = path.join(base, 'AppSupport', 'com.strokemotion.app.nemo-task-0123456789abcdef');
  await waitFor(() => fs.existsSync(mine), 'app directories');

  const stopped = await command(['stop', '--task', task, '--owner', instance.info.ownerToken]);
  assert.equal(stopped.code, 0, stopped.stderr || stopped.stdout);
  await exited(instance.child);
  const result = JSON.parse(stopped.stdout.trim().split('\n').pop());

  assert.equal(fs.existsSync(mine), false, 'this task\'s own directory is removed');
  assert.equal(fs.existsSync(production), true, 'the shared production directory is never touched');
  assert.equal(fs.existsSync(other), true, "another task's directory is never touched");
  const refusedNames = result.appState.refused.map((entry) => entry.name).sort();
  assert.deepEqual(refusedNames, ['appCache', 'appConfig', 'appLog']);
  assert.ok(result.appState.removed.some((entry) => entry.dir === mine && entry.removed === true));
});

test('cleanup eligibility is bound to the task derivation, not to what a manifest claims', () => {
  const task = `native-targets-${process.pid}`;
  const suffix = runtime.taskIdentifierSuffix(task);
  const { targets, refused } = runtime.nativeStateTargets(task, {
    dirs: {
      appData: `/tmp/Application Support/com.strokemotion.app${suffix}`,
      appLocalData: `/tmp/Application Support/com.strokemotion.app${suffix}`,
      appLog: '/tmp/Application Support/com.strokemotion.app',
      appCache: '/tmp/Application Support/com.strokemotion.app.nemo-task-0123456789abcdef',
      appConfig: 'not/absolute',
      appTray: 42,
    },
  }, null);
  assert.deepEqual(targets.map((t) => t.name), ['appData']);
  assert.deepEqual(refused.map((r) => r.name).sort(), ['appCache', 'appConfig', 'appLog', 'appTray']);
  assert.match(runtime.dataStoreUuid(task), /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);
});

test('a refused start leaves an existing task instance state intact', async () => {
  // Task ids are reused across restarts — that is what makes an instance's
  // state survive a stop. A start that never took ownership must not release
  // the roots of the run before it; releasing here destroyed a stopped
  // instance's records during the live paired run.
  const slot = `desktop-input-retain-${process.pid}`;
  const task = `native-retain-${process.pid}`;
  const roots = isolation.taskRoots(task);
  const keepsake = path.join(roots.tauriDataDir, 'retained.json');
  fs.writeFileSync(keepsake, '{"kept":true}');
  const holder = isolation.acquireExclusiveSlot(slot, `native-retain-holder-${process.pid}`);
  assert.equal(holder.acquired, true, holder.reason);
  try {
    const refused = await command([
      'start', '--task', task, '--executable', appStub, '--reserve', slot,
      '--', path.join(scratch, 'capture-retain.json'), 'manifest', '',
    ]);
    assert.equal(refused.code, 1);
    assert.match(refused.stderr, /exclusive resource/);
    assert.equal(fs.existsSync(keepsake), true, 'the earlier instance state survives a refused start');
  } finally {
    isolation.releaseExclusiveSlot(slot, holder.ownerToken);
    fs.rmSync(roots.root, { recursive: true, force: true });
  }
});

test('a killed launcher cannot delete a live app state; later release reconciles its dead slot', async () => {
  const task = `native-orphan-${process.pid}`;
  const slot = `native-orphan-slot-${process.pid}`;
  const instance = await launch(task, path.join(scratch, 'capture-orphan.json'), {
    reserve: slot, appDirBase: path.join(scratch, 'orphan-appdirs'),
  });
  let appPid;
  try {
    const app = await waitFor(() => runtime.readAppManifest(task), 'orphan manifest');
    appPid = app.manifest.pid;
    const appDir = app.manifest.dirs.appData;
    const slotFile = path.join(isolation.RUNTIME_ROOT, 'slots', isolation.idKey(slot) + '.lock');
    instance.child.kill('SIGKILL'); await exited(instance.child);
    assert.ok(isolation.pidAlive(appPid), 'the detached app survives its launcher crash');

    const refused = await command(['stop', '--task', task, '--owner', instance.info.ownerToken]);
    assert.equal(refused.code, 1, refused.stdout || refused.stderr);
    assert.match(refused.stdout, /app process group is still live/);
    assert.ok(isolation.pidAlive(appPid), 'stop never signals an orphan by its stale PID');
    assert.ok(fs.existsSync(appDir) && fs.existsSync(slotFile), 'live app state and slot evidence remain');
    assert.equal(isolation.readLauncher(task).ownerToken, instance.info.ownerToken);

    // Explicit test-owned reconciliation, after proving this is our stub.
    process.kill(-appPid, 'SIGKILL');
    await waitFor(() => !isolation.pidAlive(appPid), 'orphan app exit');
    const statusFile = path.join(isolation.taskRoots(task).reports, runtime.STATUS_FILE);
    const statusBytes = fs.readFileSync(statusFile);
    fs.writeFileSync(statusFile, JSON.stringify({ ...JSON.parse(statusBytes), childPid: null }));
    const unknown = await command(['stop', '--task', task, '--owner', instance.info.ownerToken]);
    assert.equal(unknown.code, 1);
    assert.match(unknown.stdout, /app process group is unknown/);
    assert.ok(fs.existsSync(appDir) && fs.existsSync(slotFile), 'uncertain process evidence keeps data and reservations');
    fs.writeFileSync(statusFile, statusBytes);
    const stopped = await command(['stop', '--task', task, '--owner', instance.info.ownerToken]);
    assert.equal(stopped.code, 0, stopped.stdout || stopped.stderr);
    const result = JSON.parse(stopped.stdout.trim());
    assert.deepEqual(result.released.slots.reconciled, [slot]);
    assert.equal(fs.existsSync(slotFile), false);
    assert.equal(fs.existsSync(appDir), false);
    assert.equal(fs.existsSync(isolation.taskRoot(task)), false);
  } finally {
    if (appPid && isolation.pidAlive(appPid)) process.kill(-appPid, 'SIGKILL');
    await stopOwned(instance);
  }
});

test('retained stop releases ownership and reservations while same-task relaunch keeps saved state', async () => {
  const task = `native-restart-${process.pid}`;
  const slot = `native-restart-slot-${process.pid}`;
  const base = path.join(scratch, 'restart-appdirs');
  let instance = await launch(task, path.join(scratch, 'capture-restart.json'), { reserve: slot, appDirBase: base });
  try {
    const app = await waitFor(() => runtime.readAppManifest(task), 'restart manifest');
    const roots = isolation.taskRoots(task);
    const dirs = [roots.temp, roots.cache, roots.reports, roots.tauriDataDir, app.manifest.dirs.appData];
    for (const dir of dirs) fs.writeFileSync(path.join(dir, 'persisted-sentinel.json'), '{"saved":42}');
    const stopped = await command(['stop', '--task', task, '--owner', instance.info.ownerToken, '--retain-data']);
    assert.equal(stopped.code, 0, stopped.stdout || stopped.stderr);
    await exited(instance.child);
    const result = JSON.parse(stopped.stdout.trim());
    assert.equal(result.stopped, true);
    assert.equal(result.retainedData, true);
    assert.equal(result.released.retainedData, true);
    assert.equal(result.appState, null);
    assert.equal(isolation.pidAlive(app.manifest.pid), false);
    assert.equal(isolation.readLauncher(task), null, 'same-task registration is available again');
    const reservation = isolation.acquireExclusiveSlot(slot, 'restart-contender');
    assert.equal(reservation.acquired, true); reservation.release();

    const originalPid = app.manifest.pid;
    instance = await launch(task, path.join(scratch, 'capture-restarted.json'), { reserve: slot, appDirBase: base });
    await waitFor(() => {
      const current = runtime.readAppManifest(task);
      return current && current.valid && current.manifest.pid !== originalPid;
    }, 'fresh restarted app manifest');
    const handshake = runtime.nativeHandshake(task, instance.info.ownerToken);
    assert.equal(handshake.ok, true, handshake.reason);
    for (const dir of dirs) assert.equal(fs.readFileSync(path.join(dir, 'persisted-sentinel.json'), 'utf8'), '{"saved":42}');
  } finally { await stopOwned(instance); }
});

test('retained handshake cannot satisfy a new silent app instance', async () => {
  const task = `native-stale-handshake-${process.pid}`;
  let instance = await launch(task, path.join(scratch, 'capture-handshake-old.json'));
  try {
    await waitFor(() => runtime.readAppManifest(task), 'old manifest');
    const stopped = await command(['stop', '--task', task, '--owner', instance.info.ownerToken, '--retain-data']);
    assert.equal(stopped.code, 0, stopped.stdout || stopped.stderr);
    await exited(instance.child);
    instance = await launch(task, path.join(scratch, 'capture-handshake-new.json'), { mode: 'no-manifest' });
    await waitFor(() => fs.existsSync(path.join(scratch, 'capture-handshake-new.json')), 'silent app startup');
    assert.equal(runtime.nativeHandshake(task, instance.info.ownerToken).ok, false);
    assert.equal(runtime.readAppManifest(task), null);
  } finally { await stopOwned(instance); }
});

test('a recorded PID with changed process identity is never signaled', async () => {
  const task = `native-reused-pid-${process.pid}`;
  const unrelated = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { stdio: 'ignore' });
  const record = isolation.registerLauncher(task, { pid: unrelated.pid });
  const roots = isolation.taskRoots(task);
  fs.writeFileSync(path.join(roots.reports, runtime.STATUS_FILE), JSON.stringify({
    schema: runtime.SCHEMA, taskId: task, launcherPid: unrelated.pid,
    launcherIdentity: 'a previous process birth and command', childPid: null,
  }));
  try {
    const stopped = await command(['stop', '--task', task, '--owner', record.ownerToken]);
    assert.equal(stopped.code, 1);
    assert.match(stopped.stdout, /launcher process identity changed or unavailable/);
    assert.ok(isolation.pidAlive(unrelated.pid), 'the unrelated live process is untouched');
    assert.equal(isolation.readLauncher(task).ownerToken, record.ownerToken);
  } finally {
    unrelated.kill('SIGKILL'); await exited(unrelated);
    isolation.releaseTask(task, record.ownerToken);
  }
});

test('failed retained-manifest invalidation never exposes readiness or an accepted handshake', async () => {
  const task = `native-unlink-failed-${process.pid}`;
  const slot = `native-unlink-slot-${process.pid}`;
  const capture = path.join(scratch, 'capture-unlink-new.json');
  const instance = await launch(task, path.join(scratch, 'capture-unlink-old.json'));
  const roots = isolation.taskRoots(task);
  const manifestFile = path.join(roots.tauriDataDir, runtime.APP_MANIFEST_FILE);
  const sentinel = path.join(roots.tauriDataDir, 'saved-project.json');
  let child;
  try {
    await waitFor(() => runtime.readAppManifest(task), 'retained app manifest');
    fs.writeFileSync(sentinel, '{"saved":42}');
    const stopped = await command(['stop', '--task', task, '--owner', instance.info.ownerToken, '--retain-data']);
    assert.equal(stopped.code, 0, stopped.stdout || stopped.stderr);
    await exited(instance.child);
    const retained = fs.readFileSync(manifestFile, 'utf8');
    fs.chmodSync(roots.tauriDataDir, 0o500);
    assert.throws(() => fs.unlinkSync(manifestFile), /EACCES|EPERM/, 'fixture must actually refuse unlink');
    child = spawn(process.execPath, [cli, 'start', '--task', task, '--executable', appStub,
      '--reserve', slot, '--', capture, 'no-manifest'],
    { cwd: repoRoot, env: { ...process.env }, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = ''; let stderr = '';
    child.stdout.on('data', chunk => { stdout += chunk; });
    child.stderr.on('data', chunk => { stderr += chunk; });
    const failed = await waitFor(() => {
      const status = runtime.readNativeStatus(task);
      return status && status.state === 'failed' && status;
    }, 'failed manifest invalidation');
    const owner = isolation.readLauncher(task).ownerToken;
    assert.equal(failed.childPid, null);
    assert.match(failed.error, /EACCES|EPERM/);
    assert.equal(fs.existsSync(capture), false, 'no app was spawned');
    assert.equal(fs.readFileSync(sentinel, 'utf8'), '{"saved":42}');
    assert.equal(fs.readFileSync(manifestFile, 'utf8'), retained);
    const contender = isolation.acquireExclusiveSlot(slot, 'unlink-contender');
    assert.equal(contender.acquired, true, 'failed start releases its reservation');
    contender.release();
    assert.deepEqual({ ready: stdout.includes('"started":true'), handshakeOk: runtime.nativeHandshake(task, owner).ok },
      { ready: false, handshakeOk: false });
    await exited(child);
    assert.equal(child.exitCode, 1, stderr);
    assert.equal(stdout, '');
    const released = await command(['stop', '--task', task, '--owner', owner, '--retain-data']);
    assert.equal(released.code, 0, released.stdout || released.stderr);
    assert.equal(isolation.readLauncher(task), null, 'owner can reconcile the failed start');
    assert.equal(fs.readFileSync(sentinel, 'utf8'), '{"saved":42}');
    assert.equal(fs.readFileSync(manifestFile, 'utf8'), retained);
  } finally {
    fs.chmodSync(roots.tauriDataDir, 0o700);
    const record = isolation.readLauncher(task);
    if (record) {
      await command(['stop', '--task', task, '--owner', record.ownerToken, '--retain-data']);
    }
    if (child && isolation.pidAlive(child.pid)) { child.kill('SIGKILL'); await exited(child); }
    await stopOwned(instance);
  }
});

test('handshake requires current launcher and app agreement while preserving identity guards', async () => {
  const task = `native-current-handshake-${process.pid}`;
  const instance = await launch(task, path.join(scratch, 'capture-current.json'));
  const roots = isolation.taskRoots(task);
  const statusFile = path.join(roots.reports, runtime.STATUS_FILE);
  const manifestFile = path.join(roots.tauriDataDir, runtime.APP_MANIFEST_FILE);
  const launcherFile = path.join(roots.root, 'launcher.json');
  let status; let manifest; let launcher;
  try {
    await waitFor(() => runtime.readNativeStatus(task).appRuntime, 'claimed app manifest');
    status = fs.readFileSync(statusFile, 'utf8');
    manifest = fs.readFileSync(manifestFile, 'utf8');
    launcher = fs.readFileSync(launcherFile, 'utf8');
    const check = () => runtime.nativeHandshake(task, instance.info.ownerToken);
    assert.equal(check().ok, true);
    for (const change of [
      { state: 'failed' }, { state: 'starting' }, { childPid: null },
      { launcherPid: process.pid }, { launcherIdentity: 'stale launcher identity' },
      { taskId: 'another-task' }, { schema: 'unknown' },
    ]) {
      fs.writeFileSync(statusFile, JSON.stringify({ ...JSON.parse(status), ...change }));
      assert.equal(check().ok, false, JSON.stringify(change));
    }
    fs.writeFileSync(statusFile, status);
    fs.writeFileSync(manifestFile, JSON.stringify({ ...JSON.parse(manifest), pid: process.pid }));
    assert.equal(check().ok, false, 'another live process cannot stand in for the app');
    fs.writeFileSync(manifestFile, manifest);
    assert.match(runtime.nativeHandshake(task, 'wrong-owner').reason, /owner token mismatch/);
    const changedBuild = JSON.parse(status);
    changedBuild.build.startup.packageVersion = 'different-build';
    fs.writeFileSync(statusFile, JSON.stringify(changedBuild));
    assert.match(check().reason, /build identity changed/);
    fs.writeFileSync(statusFile, status);
    const changedSource = JSON.parse(launcher);
    changedSource.source.head = '0'.repeat(40);
    fs.writeFileSync(launcherFile, JSON.stringify(changedSource));
    assert.match(check().reason, /source moved/);
    fs.writeFileSync(launcherFile, launcher);
    assert.equal(check().ok, true, 'restoring current identities restores acceptance');
  } finally {
    if (status) fs.writeFileSync(statusFile, status);
    if (manifest) fs.writeFileSync(manifestFile, manifest);
    if (launcher) fs.writeFileSync(launcherFile, launcher);
    await stopOwned(instance);
  }
});

test('natural app exit releases its managed reservation before task cleanup', async () => {
  const task = `native-natural-exit-${process.pid}`;
  const slot = `native-natural-exit-slot-${process.pid}`;
  let instance;
  try {
    instance = await launch(task, path.join(scratch, 'natural-exit.json'), { reserve: slot });
    const status = await waitFor(() => {
      const value = runtime.readNativeStatus(task);
      return value && value.state === 'active' && value.childPid ? value : null;
    }, 'native app active');
    process.kill(status.childPid, 'SIGTERM');
    await waitFor(() => !isolation.pidAlive(instance.child.pid), 'launcher natural exit');
    const successor = isolation.acquireExclusiveSlot(slot, 'natural-exit-successor');
    assert.equal(successor.acquired, true, 'confirmed app-group exit releases the slot without a later stop command');
    successor.release();
  } finally {
    if (instance) {
      await stopOwned(instance);
      await command(['stop', '--task', task, '--owner', instance.info.ownerToken]);
    }
  }
});
