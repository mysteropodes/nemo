'use strict';

// Private harness transport. Child output contains owner tokens and host paths:
// never attach it (or native snapshots) to assertion errors or test diagnostics.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const { isDeepStrictEqual } = require('node:util');

function requireCheck(condition, message) {
  if (!condition) throw new Error(message);
}

function alive(pid, group = false) {
  if (!Number.isSafeInteger(pid) || pid <= 0) return false;
  try { process.kill(group ? -pid : pid, 0); return true; }
  catch (error) { return error.code !== 'ESRCH'; }
}

async function until(read, message, timeout = 30_000) {
  const deadline = Date.now() + timeout;
  do {
    const value = read();
    if (value) return value;
    await new Promise(resolve => setTimeout(resolve, 50));
  } while (Date.now() < deadline);
  throw new Error(message);
}

function hash(file) {
  return crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
}

function packagedApp(app, runtime, platform = process.platform) {
  requireCheck(platform === 'darwin', 'desktop prerequisite: packaged runtime requires macOS');
  requireCheck(typeof app === 'string' && app.endsWith('.app'), 'desktop prerequisite: set NEMO_DESKTOP_APP to a built .app bundle');
  let executable;
  try {
    executable = runtime.bundleExecutable(path.resolve(app));
    fs.accessSync(executable, fs.constants.X_OK);
    const fd = fs.openSync(executable, 'r');
    const magic = Buffer.alloc(4);
    try { fs.readSync(fd, magic, 0, 4, 0); } finally { fs.closeSync(fd); }
    requireCheck(['feedface', 'cefaedfe', 'feedfacf', 'cffaedfe', 'cafebabe', 'bebafeca', 'cafebabf', 'bfbafeca'].includes(magic.toString('hex')), 'not Mach-O');
  } catch { throw new Error('desktop prerequisite: readable executable Mach-O app bundle required'); }
  return { bundle: fs.realpathSync(app), executable: fs.realpathSync(executable), executableSha256: hash(executable) };
}

function launchProcess(cli, args, options) {
  const child = spawn(process.execPath, [cli, ...args], {
    cwd: options.root, env: options.env, stdio: ['ignore', 'pipe', 'pipe'],
  });
  const state = { child, frames: [], pending: '', finished: false, code: null, malformed: false, overflow: false, spawnError: false };
  let bytes = 0;
  child.stdout.on('data', chunk => {
    bytes += chunk.length;
    if (bytes > 1024 * 1024) { state.overflow = true; return; }
    state.pending += chunk;
    let newline;
    while ((newline = state.pending.indexOf('\n')) >= 0) {
      const line = state.pending.slice(0, newline);
      state.pending = state.pending.slice(newline + 1);
      try { state.frames.push(JSON.parse(line)); } catch { state.malformed = true; }
    }
  });
  child.stderr.resume(); // Consume private native diagnostics without reproducing them.
  child.once('error', () => { state.spawnError = true; state.finished = true; });
  child.once('close', code => { state.code = code; state.finished = true; });
  return state;
}

function createController({ root, app, env = process.env, timeout = 30_000, cli = path.join(root, 'scripts/nemo/native.cjs') }) {
  const instances = [];
  const commands = new Set();
  const options = { root, env };
  async function command(args) {
    const state = launchProcess(cli, args, options);
    commands.add(state); // A timeout signals the helper; only close proves it exited.
    state.child.once('close', () => commands.delete(state));
    try { await until(() => state.finished, 'native command timed out', timeout); }
    catch (error) { state.child.kill('SIGTERM'); throw error; }
    requireCheck(!state.spawnError && !state.malformed && !state.overflow, 'native command returned an invalid response');
    return { code: state.code, value: state.frames[0] };
  }
  async function start(task, reserve = []) {
    const args = ['start', '--task', task, '--app', app, '--manifest-timeout-ms', String(timeout - 1000)];
    if (reserve.length) args.push('--reserve', reserve.join(','));
    const instance = { task, process: launchProcess(cli, args, options), info: null, stopped: false };
    instances.push(instance); // Track before readiness so partial starts also get cleanup.
    await until(() => {
      const state = instance.process;
      instance.info = state.frames.find(frame => frame.started === true) || instance.info;
      requireCheck(!state.finished && !state.spawnError && !state.malformed && !state.overflow, 'native launcher failed before readiness');
      return instance.info;
    }, 'native launcher readiness timed out', timeout);
    requireCheck(instance.info.taskId === task && typeof instance.info.ownerToken === 'string' && instance.info.ownerToken.length > 0, 'native launcher returned invalid ownership');
    await until(() => {
      const state = instance.process;
      requireCheck(!state.finished && !state.malformed && !state.overflow, 'native launcher exited before app manifest');
      const frame = state.frames.find(value => Object.hasOwn(value, 'appRuntime'));
      if (frame) requireCheck(frame.appRuntime && !frame.appRuntimeError, 'native app did not disclose a valid runtime manifest');
      return frame;
    }, 'native app manifest timed out', timeout);
    return instance;
  }
  async function status(instance, owner = instance.info.ownerToken) {
    return command(['status', '--task', instance.task, '--owner', owner]);
  }
  async function stop(instance, retain = false) {
    requireCheck(instance.info && instance.info.ownerToken, 'native cleanup lacks a verified launcher token; reconcile owned run');
    const args = ['stop', '--task', instance.task, '--owner', instance.info.ownerToken, '--timeout-ms', '10000'];
    if (retain) args.push('--retain-data');
    const result = await command(args);
    requireCheck(result.code === 0 && result.value && result.value.stopped === true, 'native owner stop was refused or incomplete; reconcile owned run');
    await until(() => instance.process.finished, 'native launcher did not exit after stop', 12_000);
    requireCheck(!alive(instance.info.pid), 'native launcher remains alive after stop');
    if (instance.snapshot) requireCheck(!alive(instance.snapshot.app.pid) && !alive(instance.snapshot.app.pid, true), 'native app process group remains alive after stop');
    instance.stopped = true;
    instance.retained = retain;
    if (!retain) for (const previous of instances) if (previous.task === instance.task) previous.retained = false;
    return result.value;
  }
  async function cleanupCommands() {
    let failed = false;
    for (const state of [...commands]) {
      try {
        const exited = () => state.finished && !alive(state.child.pid);
        if (exited()) continue;
        // Escalate only this exact spawned helper, never a launcher or app group.
        state.child.kill('SIGTERM');
        try { await until(exited, 'native command helper did not exit', 1000); }
        catch {
          state.child.kill('SIGKILL');
          await until(exited, 'native command helper did not exit', timeout);
        }
      } catch { failed = true; }
    }
    return failed;
  }
  async function cleanup() {
    let failed = await cleanupCommands();
    for (const instance of [...instances].reverse()) {
      if (instance.stopped) continue;
      instance.info ||= instance.process.frames.find(frame => frame.started === true);
      try {
        if (instance.info && instance.info.ownerToken) await stop(instance);
        else {
          // This is our exact spawned launcher, never a discovered task or pid.
          instance.process.child.kill('SIGTERM');
          await until(() => instance.process.finished, 'partial launcher did not exit', 12_000);
          if (instance.process.frames.some(frame => frame.started)) failed = true;
        }
      } catch { failed = true; }
    }
    // Owner-stop attempts above can themselves time out and leave helpers running.
    if (await cleanupCommands()) failed = true;
    requireCheck(!failed && commands.size === 0 && !instances.some(instance => instance.retained), 'native harness cleanup incomplete; reconcile this run before retrying');
  }
  return { instances, command, start, status, stop, cleanup };
}

function validateSnapshot(snapshot, instance, context) {
  const { isolation, runtime, source, build, app } = context;
  requireCheck(snapshot && snapshot.ok === true && snapshot.taskId === instance.task, 'native owner/source/build handshake failed');
  requireCheck(snapshot.source.matches && snapshot.build.matches, 'native source/build identities differ');
  const { originUrl: _privateOrigin, ...expectedSource } = source;
  requireCheck(isDeepStrictEqual(snapshot.source.startup, expectedSource), 'native source identity differs from tested checkout');
  requireCheck(isDeepStrictEqual(snapshot.build.startup, build) && isDeepStrictEqual(snapshot.build.current, build), 'native build identity differs from tested checkout');
  const manifest = snapshot.app;
  const roots = snapshot.runtime.roots;
  const expectedRoots = isolation.taskRoots(instance.task);
  requireCheck(manifest && manifest.schema === runtime.APP_MANIFEST_SCHEMA && manifest.state === 'active' && manifest.isolated === true, 'app runtime manifest is not active and isolated');
  requireCheck(snapshot.runtime.schema === runtime.SCHEMA && snapshot.runtime.state === 'active', 'launcher runtime is not active');
  requireCheck(manifest.taskId === instance.task && manifest.taskKey === isolation.idKey(instance.task), 'app task identity differs');
  requireCheck(manifest.identifier === build.identifier + runtime.taskIdentifierSuffix(instance.task), 'app identifier is not task scoped');
  requireCheck(manifest.dataStoreIdentifier === isolation.idKey(instance.task).slice(0, 32), 'WebKit data store identity differs');
  requireCheck(manifest.ownerTokenConfigured === true, 'app did not configure owner authorization');
  requireCheck(!Object.hasOwn(manifest, 'ownerToken'), 'app manifest exposed an owner token');
  requireCheck(manifest.pid === snapshot.runtime.childPid && snapshot.pid === instance.info.pid && snapshot.runtime.launcherPid === instance.info.pid, 'native process identities differ');
  requireCheck(manifest.pid !== snapshot.pid && alive(manifest.pid) && alive(manifest.pid, true) && alive(snapshot.pid), 'native app and launcher process groups are not live');
  requireCheck(fs.realpathSync(manifest.executable) === app.executable && fs.realpathSync(snapshot.runtime.app.executable) === app.executable, 'native executable differs from supplied bundle');
  requireCheck(manifest.appVersion === build.tauriVersion, 'native app version differs from checkout');
  requireCheck(manifest.dataDir === expectedRoots.tauriDataDir && manifest.manifestFile === path.join(expectedRoots.tauriDataDir, runtime.APP_MANIFEST_FILE), 'native manifest storage differs from task root');
  for (const key of ['root', 'temp', 'cache', 'reports', 'tauriDataDir']) {
    requireCheck(roots[key] === expectedRoots[key] && fs.statSync(roots[key]).isDirectory(), 'native mutable root differs from task allocation');
  }
  const observed = manifest.processEnvironment;
  requireCheck(observed && typeof observed === 'object', 'app did not report its actual process environment');
  for (const [key, root] of Object.entries({ tempDir: 'temp', TMPDIR: 'temp', TMP: 'temp', TEMP: 'temp', XDG_CACHE_HOME: 'cache', NEMO_REPORT_DIR: 'reports' })) {
    requireCheck(typeof observed[key] === 'string' && fs.realpathSync(observed[key]) === fs.realpathSync(expectedRoots[root]), 'native process environment differs from isolated roots');
  }
  const keys = ['appData', 'appLocalData', 'appConfig', 'appCache', 'appLog'];
  for (const key of keys) requireCheck(typeof manifest.dirs[key] === 'string' && path.isAbsolute(manifest.dirs[key]), 'app did not resolve every application directory');
  const state = runtime.nativeStateTargets(instance.task, manifest, app.bundle);
  requireCheck(state.refused.length === 0 && state.targets.some(target => target.name === 'webkitStore'), 'app directory or WebKit cleanup derivation is invalid');
  instance.snapshot = snapshot;
  return state.targets;
}

function disjoint(a, b, message) {
  const left = new Set(a.map(value => path.resolve(value)));
  requireCheck(b.every(value => !left.has(path.resolve(value))), message);
}

module.exports = { requireCheck, alive, until, hash, packagedApp, createController, validateSnapshot, disjoint };
