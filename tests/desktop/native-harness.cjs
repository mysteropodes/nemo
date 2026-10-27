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

function createController({ root, app, env = process.env, timeout = 30_000, cli = path.join(root, 'scripts/nemo/native.cjs'), startupDiagnostics }) {
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
    try {
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
    } catch (error) {
      if (typeof startupDiagnostics === 'function') {
        try { error.startupDiagnostics = await startupDiagnostics(task); }
        catch { error.startupDiagnostics = { classification: 'diagnostic-capture-failed' }; }
      }
      throw error;
    }
    return instance;
  }
  async function status(instance, owner = instance.info.ownerToken) {
    return command(['status', '--task', instance.task, '--owner', owner]);
  }
  async function stop(instance, retain = false) {
    requireCheck(instance.info && instance.info.ownerToken, 'native cleanup lacks a verified launcher token; reconcile owned run');
    if (instance.snapshot?.app?.playObserver && !instance.playObserverCollection) {
      try {
        await collectPlayObserver(instance, { directory: path.join(root, 'reports', 'play-observer-' + instance.task), timeout });
      } catch {
        // A failed diagnostic must not strand the owned app or disposable data.
        instance.playObserverCollection = { status: 'partial', reason: 'collection-refused' };
      }
    }
    const args = ['stop', '--task', instance.task, '--owner', instance.info.ownerToken, '--timeout-ms', '10000'];
    if (retain) args.push('--retain-data');
    let result;
    try {
      requireCheck(!(retain && instance.snapshot?.app?.playObserver), 'test observer requires normal owner cleanup');
      result = await command(args);
      requireCheck(result.code === 0 && result.value && result.value.stopped === true, 'native owner stop was refused or incomplete; reconcile owned run');
      await until(() => instance.process.finished, 'native launcher did not exit after stop', 12_000);
      requireCheck(!alive(instance.info.pid), 'native launcher remains alive after stop');
      if (instance.snapshot) requireCheck(!alive(instance.snapshot.app.pid) && !alive(instance.snapshot.app.pid, true), 'native app process group remains alive after stop');
      instance.stopReceipt = structuredClone(result.value);
      instance.stopped = true;
    } finally { if (instance.playObserverHandle) finalizePlayObserver(instance); }
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
  instance.validated = true;
  instance.executableSha256 = app.executableSha256;
  return state.targets;
}

function disjoint(a, b, message) {
  const left = new Set(a.map(value => path.resolve(value)));
  requireCheck(b.every(value => !left.has(path.resolve(value))), message);
}

function readPlayCollection(bytes, expected) {
  const records = [], events = [];
  let header, cut, footer, invalid = false;
  for (const line of bytes.toString('utf8').split('\n')) {
    if (!line.startsWith('NEMO_PLAY_OBSERVER ')) continue;
    try { records.push(JSON.parse(line.slice(19))); } catch { invalid = true; }
  }
  for (const value of records) {
    if (!value || value.runId !== expected.runId || footer) { invalid = true; continue; }
    if (value.kind === 'header' && !header && !events.length) header = value;
    else if (value.kind === 'event' && header && !cut && events.length < 4096) {
      if (value.sequence !== events.length + 1 || !value.event || typeof value.event !== 'object' ||
          (events.length === 0) !== (value.event.event === 'ready')) invalid = true;
      events.push(value);
    } else if (value.kind === 'seal' && header && !cut && value.status === 'provisional' &&
        /^[0-9a-f-]{36}$/.test(value.cutId)) cut = value;
    else if (value.kind === 'footer' && header) footer = value;
    else invalid = true;
  }
  const complete = !invalid && bytes.at(-1) === 10 && header?.schema === 'nemo.play-observer/1' &&
    header.taskId === expected.taskId && header.pid === expected.pid && header.window === 'main' &&
    cut && cut.accepted === events.length && cut.generated === events.length && cut.dropped === 0 && cut.failed === 0 &&
    cut.rejected === 0 && footer?.cutId === cut.cutId && footer.terminalFence === true && footer.pending === 0 &&
    footer.status === 'complete' && events.length > 0 && footer.accepted === events.length &&
    footer.generated === events.length && footer.dropped === 0 && footer.failed === 0 && footer.rejected === 0;
  return { status: complete ? 'complete' : 'partial', footer: footer || null, events: events.length };
}

// Start collection before the commands; request this ONLY after ordinary final
// Stop and all relevant publication work are quiescent. It never stops playback.
async function collectPlayObserver(instance, { directory, timeout = 5000 }) {
  const snapshot = instance.snapshot, app = snapshot?.app, observer = app?.playObserver;
  requireCheck(instance.validated === true && snapshot.taskId === instance.task && app.taskId === instance.task &&
    app.isolated === true && app.windows?.length === 1 && app.windows[0] === 'main' && alive(app.pid) &&
    observer?.schema === 'nemo.play-observer/1' && /^[0-9a-f-]{36}$/.test(observer.runId) &&
    snapshot.source.matches && snapshot.build.matches, 'test observer lacks validated task identity');
  requireCheck(typeof directory === 'string' && path.isAbsolute(directory), 'test observer destination refused');
  const reports = snapshot.runtime.roots.reports, taskRoot = snapshot.runtime.roots.root;
  const relative = path.relative(taskRoot, directory);
  requireCheck(relative.startsWith('..' + path.sep) || path.isAbsolute(relative), 'test observer copy must outlive task cleanup');
  for (const folder of [taskRoot, reports]) {
    const meta = fs.lstatSync(folder);
    requireCheck(meta.isDirectory() && !meta.isSymbolicLink(), 'test observer report root refused');
  }
  requireCheck(path.dirname(reports) === taskRoot, 'test observer report root differs');
  directory = retentionDirectory(directory, taskRoot);
  const marker = path.join(reports, 'play-observer-seal.json'), log = path.join(reports, 'native-app.stderr.log');
  try { fs.writeFileSync(marker, JSON.stringify({ runId: observer.runId }), { flag: 'wx', mode: 0o600 }); }
  catch { throw new Error('test observer seal marker refused'); }
  const expected = { runId: observer.runId, taskId: instance.task, pid: app.pid };
  const deadline = Date.now() + Math.min(Math.max(timeout, 1), 10_000);
  let bytes = Buffer.alloc(0), result = { status: 'partial', footer: null, events: 0 }, fd;
  do {
    try {
      fd ??= fs.openSync(log, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
      bytes = readPlayTail(fd);
      result = readPlayCollection(bytes, expected);
      if (result.footer) break;
    } catch { /* Copy a partial receipt even if no readable log arrived. */ }
    await new Promise(resolve => setTimeout(resolve, 25));
  } while (Date.now() < deadline);
  const copied = path.join(directory, 'native-app.stderr.log');
  const receipt = { schema: 'nemo.play-observer-copy/1', ...expected, ...result, status: 'partial', phase: 'before-owner-stop',
    interval: 'observer-ready-to-host-terminal-fence',
    source: snapshot.source.startup?.head || null, executableSha256: instance.executableSha256 || null,
    stderrSha256: crypto.createHash('sha256').update(bytes).digest('hex') };
  let identities;
  const manifestBytes = Buffer.from(JSON.stringify(receipt, null, 2) + '\n');
  try {
    identities = [newPlayCopy(copied, bytes), newPlayCopy(path.join(directory, 'play-observer.json'), manifestBytes)];
  } catch (error) { if (fd !== undefined) fs.closeSync(fd); throw error; }
  instance.playObserverHandle = { fd, directory, expected, identities,
    realTaskRoot: fs.realpathSync(taskRoot),
    manifestSha256: crypto.createHash('sha256').update(manifestBytes).digest('hex') };
  instance.playObserverCollection = receipt;
  return receipt;
}

function readPlayTail(fd) {
  const meta = fs.fstatSync(fd);
  requireCheck(meta.isFile() && meta.size <= 64 * 1024 * 1024, 'test observer log refused');
  const bytes = Buffer.alloc(meta.size);
  let offset = 0;
  while (offset < bytes.length) {
    const read = fs.readSync(fd, bytes, offset, bytes.length - offset, offset);
    requireCheck(read > 0, 'test observer log changed while copying');
    offset += read;
  }
  return bytes;
}

function retentionDirectory(directory, taskRoot, retainedRealTask, create = true) {
  const requested = path.resolve(directory), realTask = retainedRealTask || fs.realpathSync(taskRoot);
  let existing = requested;
  for (;;) {
    try { fs.lstatSync(existing); break; }
    catch (error) {
      requireCheck(error.code === 'ENOENT' && path.dirname(existing) !== existing, 'test observer destination refused');
      existing = path.dirname(existing);
    }
  }
  // Resolve the existing ancestry once and operate on the physical path below.
  // This permits macOS /var aliases but cannot hide a destination inside task storage.
  const meta = fs.lstatSync(existing);
  requireCheck(!(existing === requested && meta.isSymbolicLink()) &&
    fs.statSync(existing).isDirectory(), 'test observer destination ancestor refused');
  const resolved = path.resolve(fs.realpathSync(existing), path.relative(existing, requested));
  const outside = target => {
    const relative = path.relative(realTask, target);
    return relative.startsWith('..' + path.sep) || path.isAbsolute(relative);
  };
  requireCheck(outside(resolved), 'test observer copy must outlive resolved task cleanup');
  if (create) fs.mkdirSync(resolved, { recursive: true, mode: 0o700 });
  const actual = fs.realpathSync(resolved);
  requireCheck(actual === resolved && outside(actual), 'test observer resolved destination differs');
  return actual;
}

function newPlayCopy(file, bytes) {
  const fd = fs.openSync(file, fs.constants.O_WRONLY | fs.constants.O_CREAT | fs.constants.O_EXCL | fs.constants.O_NOFOLLOW, 0o600);
  try { fs.writeFileSync(fd, bytes); fs.fsyncSync(fd); return fs.fstatSync(fd); }
  finally { fs.closeSync(fd); }
}

function replacePlayCopy(file, bytes, identity, expectedHash) {
  const fd = fs.openSync(file, fs.constants.O_RDWR | fs.constants.O_NOFOLLOW);
  try {
    const current = fs.fstatSync(fd);
    requireCheck(current.isFile() && current.dev === identity.dev && current.ino === identity.ino &&
      crypto.createHash('sha256').update(readPlayTail(fd)).digest('hex') === expectedHash, 'test observer preserved copy changed');
    fs.ftruncateSync(fd, 0);
    fs.writeFileSync(fd, bytes); fs.fsyncSync(fd);
  } finally { fs.closeSync(fd); }
}

// Held read descriptor survives normal owner cleanup unlinking the source log.
// Only the post-exit tail may establish complete collection, never the first footer.
function finalizePlayObserver(instance) {
  const handle = instance.playObserverHandle;
  if (!handle) return instance.playObserverCollection;
  let receipt = { ...instance.playObserverCollection, status: 'partial', phase: 'owner-stop-incomplete' };
  try {
    retentionDirectory(handle.directory, instance.snapshot.runtime.roots.root, handle.realTaskRoot, false);
    const stopped = instance.stopped === true && !alive(instance.snapshot.app.pid) &&
      (!instance.process || (instance.process.finished && !alive(instance.info.pid)));
    if (handle.fd !== undefined) {
      const bytes = readPlayTail(handle.fd), result = readPlayCollection(bytes, handle.expected);
      replacePlayCopy(path.join(handle.directory, 'native-app.stderr.log'), bytes, handle.identities[0], receipt.stderrSha256);
      receipt = { ...receipt, ...result, status: stopped ? result.status : 'partial', phase: stopped ? 'after-owner-stop' : 'owner-stop-incomplete',
        stderrSha256: crypto.createHash('sha256').update(bytes).digest('hex') };
    }
    replacePlayCopy(path.join(handle.directory, 'play-observer.json'), Buffer.from(JSON.stringify(receipt, null, 2) + '\n'),
      handle.identities[1], handle.manifestSha256);
  } catch { receipt = { ...receipt, status: 'partial', phase: 'terminal-copy-refused' }; }
  finally {
    if (handle.fd !== undefined) { try { fs.closeSync(handle.fd); } catch { receipt.status = 'partial'; } }
    delete instance.playObserverHandle;
  }
  instance.playObserverCollection = receipt;
  return receipt;
}

module.exports = { requireCheck, alive, until, hash, packagedApp, createController, validateSnapshot, disjoint,
  collectPlayObserver, finalizePlayObserver };
