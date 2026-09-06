'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { test } = require('node:test');
const { packagedApp, createController, alive, until, disjoint, validateSnapshot } = require('../native-harness.cjs');

// A Node-only transport fixture, never a .app or a native-runtime launcher.
// Its processes, task files and fake ownership tokens stay in one scratch root.
const fixture = String.raw`
const fs = require('node:fs');
const path = require('node:path');
const args = process.argv.slice(2);
const get = key => args[args.indexOf(key) + 1];
const task = get('--task');
const file = path.join(process.env.FIXTURE_ROOT, task + '.json');
const emit = value => process.stdout.write(JSON.stringify(value) + '\n');
const helpers = path.join(process.env.FIXTURE_ROOT, 'helpers.json');
const mode = process.env.FIXTURE_MODE;
const resistant = mode === args[0] + '-resistant' ||
  (mode === args[0] + '-resistant-once' && !fs.existsSync(helpers));
if (resistant && !fs.existsSync(path.join(process.env.FIXTURE_ROOT, 'allow-helpers'))) {
  process.on('SIGTERM', () => fs.writeFileSync(path.join(process.env.FIXTURE_ROOT, process.pid + '.sigterm'), 'received'));
  const pids = fs.existsSync(helpers) ? JSON.parse(fs.readFileSync(helpers)) : [];
  fs.writeFileSync(helpers, JSON.stringify([...pids, process.pid]));
  setInterval(() => {}, 1000);
} else if (args[0] === 'start') {
  const value = { started: true, taskId: task, pid: process.pid, ownerToken: 'private-fixture-token' };
  fs.writeFileSync(file, JSON.stringify(value));
  process.stderr.write('/private/fixture/path private-fixture-token\n');
  emit(value);
  if (process.env.FIXTURE_MODE === 'malformed') process.stdout.write('bad private-fixture-token /private/fixture/path\n');
  else if (process.env.FIXTURE_MODE === 'invalid-manifest') emit({ appRuntime: null, appRuntimeError: '/private/fixture/path private-fixture-token' });
  else if (process.env.FIXTURE_MODE !== 'silent') emit({ appRuntime: { isolated: true } });
  process.on('SIGTERM', () => process.exit(0));
  setInterval(() => {}, 1000);
} else {
  const value = JSON.parse(fs.readFileSync(file));
  if (get('--owner') !== value.ownerToken) { emit({ ok: false, stopped: false }); process.exit(1); }
  if (args[0] === 'status') { emit({ ok: true }); process.exit(0); }
  process.kill(value.pid, 'SIGTERM');
  const timer = setInterval(() => {
    try { process.kill(value.pid, 0); }
    catch { clearInterval(timer); emit({ stopped: true }); }
  }, 10);
}
`;

function setup(t, mode = 'ready') {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-harness-unit-'));
  const cli = path.join(root, 'fixture.cjs');
  fs.writeFileSync(cli, fixture);
  const controller = createController({ root, cli, app: 'unused', timeout: 1500,
    env: { ...process.env, FIXTURE_ROOT: root, FIXTURE_MODE: mode } });
  const helperPids = () => {
    const file = path.join(root, 'helpers.json');
    return fs.existsSync(file) ? JSON.parse(fs.readFileSync(file)) : [];
  };
  t.after(async () => {
    try {
      // Fail-safe teardown also runs against the unfixed harness after an assertion fails.
      fs.writeFileSync(path.join(root, 'allow-helpers'), '');
      const pids = helperPids();
      for (const pid of pids) if (alive(pid)) process.kill(pid, 'SIGKILL');
      await until(() => pids.every(pid => !alive(pid)), 'fixture helper teardown incomplete', 1500);
      await controller.cleanup();
      assert.equal(controller.instances.some(instance => alive(instance.process.child.pid)), false);
    }
    finally { fs.rmSync(root, { recursive: true, force: true }); }
  });
  return { controller, root, helperPids };
}

test('missing bundle and unsupported platform fail rather than skip', () => {
  assert.throws(() => packagedApp(undefined, {}, 'darwin'), /set NEMO_DESKTOP_APP/);
  assert.throws(() => packagedApp('anything.app', {}, 'linux'), /requires macOS/);
  assert.throws(() => packagedApp('anything.app', { bundleExecutable() { throw new Error('/private/fixture/path'); } }, 'darwin'), error => {
    assert.doesNotMatch(error.message, /private|fixture/);
    return /Mach-O/.test(error.message);
  });
});

test('packaged gate rejects executable scripts disguised as an app binary', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-not-a-native-app-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const executable = path.join(root, 'stub');
  fs.writeFileSync(executable, '#!/usr/bin/env node\n', { mode: 0o700 });
  assert.throws(() => packagedApp('fixture.app', { bundleExecutable: () => executable }, 'darwin'), /Mach-O/);
});

test('transport keeps owner output private and stops only its owned fixture', async t => {
  const { controller } = setup(t);
  const a = await controller.start('a');
  const b = await controller.start('b');
  assert.equal((await controller.status(a)).value.ok, true);
  assert.equal((await controller.status(a, 'wrong-owner')).code, 1);
  await controller.stop(a);
  assert.equal(alive(a.info.pid), false);
  assert.equal(alive(b.info.pid), true);
  await controller.stop(b);
  assert.equal(alive(b.info.pid), false);
});

for (const mode of ['malformed', 'invalid-manifest', 'silent']) {
  test(`a ${mode} app response fails and cleans a partial start`, async t => {
    const { controller } = setup(t, mode);
    await assert.rejects(controller.start('partial'), error => {
      assert.doesNotMatch(error.message, /private-fixture-token|\/private\/fixture/);
      return /native/.test(error.message);
    });
    const pid = controller.instances[0].info.pid;
    await controller.cleanup();
    assert.equal(alive(pid), false);
  });
}

for (const action of ['command', 'status', 'stop']) {
  test(`cleanup reaps a timed-out SIGTERM-resistant ${action} helper`, async t => {
    const mode = action === 'stop' ? 'stop-resistant-once' : action + '-resistant';
    const { controller, root, helperPids } = setup(t, mode);
    const instance = action === 'command' ? null : await controller.start('owned');
    const operation = action === 'command'
      ? controller.command(['command', '--task', 'unused']) : controller[action](instance);
    await assert.rejects(operation, { message: 'native command timed out' });
    const pids = helperPids();
    assert.equal(pids.length, 1);
    await until(() => fs.existsSync(path.join(root, pids[0] + '.sigterm')), 'fixture did not receive SIGTERM', 1500);
    assert.equal(alive(pids[0]), true, 'fixture must survive the timeout SIGTERM');
    await controller.cleanup();
    assert.equal(pids.some(pid => alive(pid)), false, 'successful cleanup must prove helper exit');
    if (instance) assert.equal(alive(instance.info.pid), false);
  });
}

test('cleanup reaps its own timed-out stop helper but rejects an incomplete owner stop', async t => {
  const { controller, helperPids } = setup(t, 'stop-resistant');
  const instance = await controller.start('owned');
  await assert.rejects(controller.stop(instance), { message: 'native command timed out' });
  await assert.rejects(controller.cleanup(), { message: 'native harness cleanup incomplete; reconcile this run before retrying' });
  const pids = helperPids();
  assert.equal(pids.length, 2, 'include the stop helper spawned during cleanup');
  assert.equal(pids.some(pid => alive(pid)), false, 'failed cleanup must still reap its command helpers');
  assert.equal(instance.stopped, false);
  assert.equal(alive(instance.info.pid), true, 'a failed owner stop must not force-kill the launcher');
});

test('source or task mismatch cannot pass merely because ok was supplied', () => {
  const context = { source: { head: 'expected' } };
  assert.throws(() => validateSnapshot({ ok: true, taskId: 'foreign' }, { task: 'mine' }, context), /handshake/);
  assert.throws(() => validateSnapshot({ ok: true, taskId: 'mine', source: { matches: true, startup: { head: 'wrong' } }, build: { matches: true } }, { task: 'mine' }, context), /source identity differs/);
});

test('actual process, environment, build and manifest evidence is required', async t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-snapshot-unit-'));
  const child = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { detached: true, stdio: 'ignore' });
  await new Promise((resolve, reject) => { child.once('spawn', resolve); child.once('error', reject); });
  t.after(async () => {
    child.kill('SIGTERM');
    await new Promise(resolve => child.once('close', resolve));
    fs.rmSync(root, { recursive: true, force: true });
  });
  const roots = { root };
  for (const key of ['temp', 'cache', 'reports', 'tauriDataDir']) {
    roots[key] = path.join(root, key); fs.mkdirSync(roots[key]);
  }
  const source = { head: 'candidate', dirty: false };
  const build = { identifier: 'fixture', tauriVersion: '1.2.3' };
  const runtime = { SCHEMA: 'launcher/1', APP_MANIFEST_SCHEMA: 'manifest/1', APP_MANIFEST_FILE: 'manifest.json',
    taskIdentifierSuffix: () => '.task', nativeStateTargets: () => ({ refused: [], targets: [{ name: 'webkitStore', dir: root }] }) };
  const context = { source, build, runtime, app: { executable: fs.realpathSync(process.execPath), bundle: 'unused' },
    isolation: { taskRoots: () => roots, idKey: () => 'abcd' } };
  const good = {
    ok: true, taskId: 'mine', pid: process.pid,
    source: { matches: true, startup: source }, build: { matches: true, startup: build, current: build },
    runtime: { schema: 'launcher/1', state: 'active', roots, childPid: child.pid, launcherPid: process.pid, app: { executable: process.execPath } },
    app: { schema: 'manifest/1', state: 'active', isolated: true, taskId: 'mine', taskKey: 'abcd',
      identifier: 'fixture.task', dataStoreIdentifier: 'abcd', ownerTokenConfigured: true, pid: child.pid,
      executable: process.execPath, appVersion: '1.2.3', dataDir: roots.tauriDataDir,
      manifestFile: path.join(roots.tauriDataDir, 'manifest.json'),
      dirs: { appData: root, appLocalData: root, appConfig: root, appCache: root, appLog: root },
      processEnvironment: { tempDir: roots.temp, TMPDIR: roots.temp, TMP: roots.temp, TEMP: roots.temp, XDG_CACHE_HOME: roots.cache, NEMO_REPORT_DIR: roots.reports } },
  };
  const instance = { task: 'mine', info: { pid: process.pid } };
  validateSnapshot(good, instance, context);
  for (const mutate of [
    value => { value.app.processEnvironment.TMP = roots.cache; },
    value => { delete value.app.processEnvironment; },
    value => { value.app.pid = process.pid; },
    value => { value.app.dataStoreIdentifier = 'foreign'; },
    value => { value.app.ownerToken = 'secret'; },
    value => { value.build.current.tauriVersion = 'stale'; },
    value => { value.app.appVersion = 'stale'; },
    value => { value.runtime.roots.reports = roots.cache; },
  ]) {
    const value = structuredClone(good); mutate(value);
    assert.throws(() => validateSnapshot(value, instance, context));
  }
});

test('normalized shared paths are rejected without exposing their values', () => {
  assert.throws(() => disjoint(['/private/fixture/cache'], ['/private/fixture/tmp/../cache'], 'shared storage'), { message: 'shared storage' });
  disjoint(['/private/fixture/a'], ['/private/fixture/b'], 'shared storage');
});
