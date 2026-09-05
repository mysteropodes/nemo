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
const [capture, mode = 'manifest'] = process.argv.slice(2);
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
  const manifest = {
    schema: 'nemo.native-runtime/1',
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

async function launch(task, capture, { mode = 'manifest', reserve = null } = {}) {
  const args = [cli, 'start', '--task', task, '--executable', appStub];
  if (reserve) args.push('--reserve', reserve);
  args.push('--manifest-timeout-ms', '5000', '--', capture, mode);
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
    for (const field of ['taskId', 'taskKey', 'dataDir', 'reports', 'temp', 'cache']) {
      assert.notEqual(seenA[field], seenB[field], `${field} must differ between instances`);
    }
    assert.equal(seenA.taskKey, isolation.idKey(seenA.taskId));
    assert.equal(seenB.taskKey, isolation.idKey(seenB.taskId));
    assert.ok(seenA.ownerTokenPresent && seenB.ownerTokenPresent);
    // Distinct keys are what the identifier and WebKit data store are derived
    // from on the native side, so distinct keys are the isolation claim here.
    assert.notEqual(seenA.taskKey.slice(0, 16), seenB.taskKey.slice(0, 16));

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
  assert.throws(() => runtime.bundleExecutable(path.join(scratch, 'Nope.app')), /no Contents\/Info\.plist/);
});
