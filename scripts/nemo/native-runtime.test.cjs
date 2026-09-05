'use strict';
// Focused regressions for the isolated native launcher (R06, #902).
//
// These never start Nemo, a webview or a GPU process: the stub below stands
// in for the app and records the isolation environment it was handed. What
// they do prove is the launcher half — disjoint roots per task instance, the
// exact environment contract src-tauri/src/task_runtime.rs consumes,
// owner-only status/stop/cleanup, and the exclusive desktop input slot.
//
// The app half (that a real Nemo resolves those roots, and that two WKWebView
// website data stores are genuinely separate) is proved by the Rust suite in
// src-tauri/src/task_runtime.rs plus a paired two-instance desktop run, which
// needs the exclusive input slot. See engineering/runtime/native-isolation.md.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { after, test } = require('node:test');

const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-native-runtime-test-'));
process.env.NEMO_ISOLATION_ROOT = path.join(scratch, 'runtime');
const isolation = require('./lib/isolation.cjs');
const runtime = require('./native-runtime.cjs');
const cli = path.join(__dirname, 'native-runtime.cjs');
const repoRoot = path.resolve(__dirname, '..', '..');

// Stands in for the desktop app: writes the instance record the real
// task_runtime.rs writes, into the root it was told to use, then stays alive.
//
// A `#!${process.execPath}` shebang is not usable here: a shebang line takes
// the interpreter path literally, and Node can legitimately live under a path
// containing a space (`~/Library/Application Support/...` on this machine),
// which makes the kernel report the stub as ENOENT. The /bin/sh trampoline
// quotes it properly.
const appScript = path.join(scratch, 'app-stub.js');
const appStub = path.join(scratch, 'app-stub');
fs.writeFileSync(appStub, `#!/bin/sh\nexec ${JSON.stringify(process.execPath)} ${JSON.stringify(appScript)} "$@"\n`, { mode: 0o700 });
fs.writeFileSync(appScript, String.raw`
const fs = require('node:fs');
const path = require('node:path');
const root = process.env.NEMO_TASK_DATA_ROOT;
if (!root) { process.stderr.write('no NEMO_TASK_DATA_ROOT\n'); process.exit(78); }
fs.mkdirSync(root, { recursive: true });
fs.writeFileSync(path.join(root, 'instance.json'), JSON.stringify({
  schema: 'nemo.native-runtime/1',
  taskId: process.env.NEMO_TASK_ID,
  pid: process.pid,
  webDataUuid: process.env.NEMO_TASK_WEB_DATA_UUID,
  declaredSource: JSON.parse(process.env.NEMO_TASK_SOURCE_IDENTITY || 'null'),
}, null, 2));
fs.writeFileSync(process.argv[2], JSON.stringify({ ...process.env }));
setInterval(() => {}, 1000);
`);

after(() => fs.rmSync(scratch, { recursive: true, force: true }));

function firstLine(child) {
  return new Promise((resolve, reject) => {
    let data = ''; const timer = setTimeout(() => done(new Error('launcher readiness timed out')), 10_000);
    function done(error, value) { clearTimeout(timer); child.stdout.off('data', read); child.off('exit', early); error ? reject(error) : resolve(value); }
    function read(chunk) { data += chunk; const nl = data.indexOf('\n'); if (nl !== -1) done(null, data.slice(0, nl)); }
    function early(code) { done(new Error(`launcher exited before readiness (${code}): ${data}`)); }
    child.stdout.on('data', read); child.once('exit', early);
  });
}

async function launch(task) {
  const capture = path.join(scratch, `${task}.env.json`);
  const child = spawn(process.execPath, [cli, 'start', '--task', task, '--command', appStub, '--', capture], {
    cwd: repoRoot, env: { ...process.env }, stdio: ['ignore', 'pipe', 'pipe'],
  });
  return { child, capture, info: JSON.parse(await firstLine(child)) };
}

function cliRun(args) {
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [cli, ...args], { cwd: repoRoot, env: { ...process.env } });
    let out = ''; let err = '';
    child.stdout.on('data', (c) => { out += c; });
    child.stderr.on('data', (c) => { err += c; });
    child.once('exit', (code) => resolve({ code, out, err, json: (() => { try { return JSON.parse(out); } catch { return null; } })() }));
  });
}

async function waitFor(predicate, timeoutMs = 10_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return true;
    await new Promise((r) => setTimeout(r, 20));
  }
  return false;
}

// ---- the environment contract with src-tauri/src/task_runtime.rs --------

// Mirrors the app-side rules in task_runtime.rs (validate_data_root,
// validate_task_id, parse_web_data_uuid). Those rules are proved by the Rust
// suite; this asserts the launcher only ever emits values that satisfy them,
// so a refusal on the app side cannot come from the launcher's own output.
function assertSatisfiesAppContract(env) {
  const root = env.NEMO_TASK_DATA_ROOT;
  assert.ok(root, 'NEMO_TASK_DATA_ROOT is the only activation trigger and must be set');
  assert.ok(path.isAbsolute(root), `data root must be absolute: ${root}`);
  const segments = root.split(path.sep).filter(Boolean);
  assert.ok(!segments.includes('..') && !segments.includes('.'), `data root must not contain traversal: ${root}`);
  assert.ok(segments.length >= 2, `data root must be at least two levels deep: ${root}`);
  assert.match(env.NEMO_TASK_ID, /^[a-zA-Z0-9][a-zA-Z0-9._-]{0,119}$/);
  assert.match(env.NEMO_TASK_WEB_DATA_UUID, /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
  assert.notEqual(env.NEMO_TASK_WEB_DATA_UUID, '00000000-0000-0000-0000-000000000000');
}

test('two task instances get disjoint data roots and distinct website-data identities', () => {
  const a = runtime.launchConfig('native-a', { command: appStub });
  const b = runtime.launchConfig('native-b', { command: appStub });
  assert.notEqual(a.dataRoot, b.dataRoot);
  assert.ok(!a.dataRoot.startsWith(b.dataRoot) && !b.dataRoot.startsWith(a.dataRoot));
  assert.notEqual(a.webDataUuid, b.webDataUuid);
  assert.notEqual(a.statusFile, b.statusFile);
  assertSatisfiesAppContract(a.env);
  assertSatisfiesAppContract(b.env);
});

test('the isolated data root is not the shared install app-data directory', () => {
  const config = runtime.launchConfig('native-not-shared', { command: appStub });
  const shared = path.join(os.homedir(), 'Library', 'Application Support', 'com.strokemotion.app');
  assert.notEqual(config.dataRoot, shared);
  assert.ok(!config.dataRoot.startsWith(shared + path.sep), `${config.dataRoot} must not sit inside the shared install`);
});

test('generated website-data UUIDs are valid v4 and never the nil UUID', () => {
  const seen = new Set();
  for (let i = 0; i < 200; i++) {
    const uuid = runtime.webDataUuid();
    assert.match(uuid, /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    assert.notEqual(uuid, '00000000-0000-0000-0000-000000000000');
    seen.add(uuid);
  }
  assert.equal(seen.size, 200);
  // The nil input is the one value WebKit rejects; the version/variant bits
  // must lift even an all-zero buffer out of it.
  assert.notEqual(runtime.webDataUuid(Buffer.alloc(16)), '00000000-0000-0000-0000-000000000000');
});

// ---- app resolution -----------------------------------------------------

test('a .app bundle resolves to its inner executable and bad targets are named', () => {
  const bundle = path.join(scratch, 'Fixture.app');
  const macos = path.join(bundle, 'Contents', 'MacOS');
  fs.mkdirSync(macos, { recursive: true });
  const binary = path.join(macos, 'Fixture');
  fs.writeFileSync(binary, '#!/bin/sh\nexit 0\n', { mode: 0o700 });
  assert.equal(runtime.resolveExecutable(bundle), binary);

  const empty = path.join(scratch, 'Empty.app');
  fs.mkdirSync(path.join(empty, 'Contents', 'MacOS'), { recursive: true });
  assert.throws(() => runtime.resolveExecutable(empty), /no executable inside/);
  assert.throws(() => runtime.resolveExecutable(path.join(scratch, 'Absent.app')), /app not found/);
});

test('a missing build names every path it looked in instead of failing vaguely', () => {
  const bare = fs.mkdtempSync(path.join(scratch, 'bare-'));
  assert.throws(
    () => runtime.defaultAppPath(bare, 'aarch64-apple-darwin'),
    (err) => /no built Nemo\.app found/.test(err.message) && err.message.includes(bare),
  );
});

// ---- two live instances -------------------------------------------------

test('two concurrent instances keep separate roots, records and stop authority', async (t) => {
  const a = await launch('native-live-a');
  const b = await launch('native-live-b');
  t.after(async () => {
    for (const inst of [a, b]) {
      await cliRun(['stop', '--task', inst.info.taskId, '--owner', inst.info.ownerToken]);
      if (inst.child.exitCode == null) inst.child.kill('SIGKILL');
    }
  });

  assert.notEqual(a.info.dataRoot, b.info.dataRoot);
  assert.notEqual(a.info.webDataUuid, b.info.webDataUuid);
  assert.notEqual(a.info.ownerToken, b.info.ownerToken);

  assert.ok(await waitFor(() => fs.existsSync(a.capture) && fs.existsSync(b.capture)), 'both stubs should record their environment');
  const envA = JSON.parse(fs.readFileSync(a.capture, 'utf8'));
  const envB = JSON.parse(fs.readFileSync(b.capture, 'utf8'));
  assertSatisfiesAppContract(envA);
  assertSatisfiesAppContract(envB);
  assert.equal(envA.NEMO_TASK_DATA_ROOT, a.info.dataRoot);
  assert.equal(envB.NEMO_TASK_DATA_ROOT, b.info.dataRoot);
  assert.notEqual(envA.NEMO_TASK_WEB_DATA_UUID, envB.NEMO_TASK_WEB_DATA_UUID);
  // Each instance also gets its own cache and report roots, not just data.
  assert.notEqual(envA.XDG_CACHE_HOME, envB.XDG_CACHE_HOME);
  assert.notEqual(envA.NEMO_REPORT_DIR, envB.NEMO_REPORT_DIR);

  // Each instance record lands inside its own root and nowhere else.
  assert.ok(await waitFor(() => !!runtime.readInstanceRecord(a.info.taskId) && !!runtime.readInstanceRecord(b.info.taskId)));
  const recordA = runtime.readInstanceRecord(a.info.taskId);
  const recordB = runtime.readInstanceRecord(b.info.taskId);
  assert.equal(recordA.taskId, 'native-live-a');
  assert.equal(recordB.taskId, 'native-live-b');
  assert.equal(recordA.webDataUuid, a.info.webDataUuid);
  assert.equal(fs.readdirSync(path.dirname(a.info.instanceFile)).length, 1);

  // The handshake only reports ok once the app itself confirmed the root.
  const statusA = await cliRun(['status', '--task', 'native-live-a', '--owner', a.info.ownerToken]);
  assert.equal(statusA.code, 0, statusA.out + statusA.err);
  assert.equal(statusA.json.ok, true);
  assert.equal(statusA.json.instance.taskId, 'native-live-a');
  assert.equal(statusA.json.dataRoot, a.info.dataRoot);

  // Stopping one instance leaves the other running and intact.
  const stopA = await cliRun(['stop', '--task', 'native-live-a', '--owner', a.info.ownerToken]);
  assert.equal(stopA.code, 0, stopA.out + stopA.err);
  assert.equal(stopA.json.stopped, true);
  assert.equal(stopA.json.released.released, true);
  assert.ok(!fs.existsSync(a.info.dataRoot), 'the released task root is gone');
  assert.equal(b.child.exitCode, null, 'the other instance keeps running');
  assert.ok(fs.existsSync(b.info.dataRoot), 'the other task root survives');
  assert.ok(!!runtime.readInstanceRecord('native-live-b'));
});

test('status and stop refuse a caller that is not the task owner', async (t) => {
  const inst = await launch('native-owner');
  t.after(async () => {
    await cliRun(['stop', '--task', inst.info.taskId, '--owner', inst.info.ownerToken]);
    if (inst.child.exitCode == null) inst.child.kill('SIGKILL');
  });
  assert.ok(await waitFor(() => !!runtime.readInstanceRecord('native-owner')));

  const badStatus = await cliRun(['status', '--task', 'native-owner', '--owner', 'not-the-token']);
  assert.equal(badStatus.code, 1);
  assert.equal(badStatus.json.ok, false);
  assert.match(badStatus.json.reason, /owner token mismatch/);

  const badStop = await cliRun(['stop', '--task', 'native-owner', '--owner', 'not-the-token']);
  assert.equal(badStop.code, 1);
  assert.equal(badStop.json.stopped, false);
  assert.equal(badStop.json.released, null);
  // A refused stop must change nothing: the app keeps running and its
  // isolated state is still there.
  assert.equal(inst.child.exitCode, null);
  assert.ok(fs.existsSync(inst.info.instanceFile));
});

test('the handshake reports unconfirmed isolation when the app wrote no record', () => {
  const taskId = 'native-no-record';
  const roots = isolation.taskRoots(taskId);
  fs.writeFileSync(path.join(roots.reports, runtime.STATUS_FILE), JSON.stringify({
    schema: runtime.SCHEMA, taskId, dataRoot: roots.tauriDataDir, webDataUuid: runtime.webDataUuid(),
    build: { startup: null },
  }));
  const result = runtime.nativeHandshake(taskId, 'whatever');
  assert.equal(result.ok, false);
  assert.equal(result.instance, null);
});

// ---- exclusive desktop input -------------------------------------------

test('the desktop input slot admits one task at a time and releases by owner', async () => {
  const first = isolation.acquireExclusiveSlot(runtime.INPUT_SLOT, 'input-a', { pid: process.pid });
  assert.equal(first.acquired, true);
  try {
    const second = isolation.acquireExclusiveSlot(runtime.INPUT_SLOT, 'input-b', { pid: process.pid });
    assert.equal(second.acquired, false);
    assert.match(second.reason, /held by another task/);
    // Two instances may run at once; only human input is serialized.
    assert.equal(isolation.releaseExclusiveSlot(runtime.INPUT_SLOT, 'wrong-token').released, false);
  } finally {
    assert.equal(isolation.releaseExclusiveSlot(runtime.INPUT_SLOT, first.ownerToken).released, true);
  }
  const again = isolation.acquireExclusiveSlot(runtime.INPUT_SLOT, 'input-b', { pid: process.pid });
  assert.equal(again.acquired, true);
  isolation.releaseExclusiveSlot(runtime.INPUT_SLOT, again.ownerToken);
});

test('the CLI input slot commands round-trip and refuse a second holder', async () => {
  const acquired = await cliRun(['input-acquire', '--task', 'input-cli-a', '--pid', String(process.pid)]);
  assert.equal(acquired.code, 0, acquired.out + acquired.err);
  assert.equal(acquired.json.acquired, true);
  try {
    const denied = await cliRun(['input-acquire', '--task', 'input-cli-b', '--pid', String(process.pid)]);
    assert.equal(denied.code, 1);
    assert.equal(denied.json.acquired, false);
  } finally {
    const released = await cliRun(['input-release', '--owner', acquired.json.ownerToken]);
    assert.equal(released.code, 0, released.out + released.err);
    assert.equal(released.json.released, true);
  }
});

test('non-macOS launches are refused before any task state is created', () => {
  assert.throws(() => runtime.assertNativePlatform('linux'), /macOS only/);
  assert.throws(() => runtime.assertNativePlatform('win32'), /macOS only/);
  assert.doesNotThrow(() => runtime.assertNativePlatform('darwin'));
});
