'use strict';
// Source-only operator. Every native/package/window operation requires a NEW P38 grant.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { execFile } = require('node:child_process');
const readline = require('node:readline');
const ROOT = path.resolve(__dirname, '../..');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function check(value, code) { if (!value) throw Object.assign(new Error(code), { code }); }
function id(value) { return typeof value === 'string' && /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(value); }
function readJSON(file, limit = 1048576) {
  const fd = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
  try { const stat = fs.fstatSync(fd); check(stat.isFile() && stat.size <= limit, 'evidence-file-refused');
    return JSON.parse(fs.readFileSync(fd, 'utf8')); } finally { fs.closeSync(fd); }
}
// confirmed is an OPERATOR ASSERTION, never cryptographic evidence of human permission.
// Invocation still requires the Lead to deliver Ilya's explicit NEW P38 run grant.
function authorize(config) {
  const grant = config && config.grant;
  check(grant && grant.issue === 1645 && grant.confirmed === true && id(grant.id) &&
    grant.id !== '6121922271' && config.workflow === 'P38', 'new-p38-grant-required');
  check(id(config.taskId) && /^[a-f0-9]{40}$/.test(config.sourceHead) &&
    Number.isInteger(config.timeoutMs) && config.timeoutMs >= 1000 && config.timeoutMs <= 600000, 'config-refused');
}
function safeDirectory(base, parts) {
  const physical = fs.realpathSync(base), pins = [];
  let current = physical;
  function pin(file) {
    const stat = fs.lstatSync(file);
    check(stat.isDirectory() && !stat.isSymbolicLink() && fs.realpathSync(file) === file, 'report-directory-refused');
    pins.push({ file, dev: stat.dev, ino: stat.ino });
  }
  pin(current);
  for (const part of parts) {
    check(typeof part === 'string' && /^[A-Za-z0-9._-]+$/.test(part) && part !== '..', 'report-directory-refused');
    current = path.join(current, part);
    try { fs.mkdirSync(current, { mode: 0o700 }); } catch (error) { if (error.code !== 'EEXIST') throw error; }
    pin(current);
  }
  return { directory: current, verify() {
    for (const item of pins) {
      const now = fs.lstatSync(item.file);
      check(now.isDirectory() && !now.isSymbolicLink() && now.dev === item.dev && now.ino === item.ino &&
        fs.realpathSync(item.file) === item.file, 'report-directory-refused');
    }
  } };
}
function atomicJSON(guard, name, value) {
  check(/^[A-Za-z0-9._-]+\.json$/.test(name), 'receipt-file-refused');
  guard.verify();
  const destination = path.join(guard.directory, name);
  try { const old = fs.lstatSync(destination); check(old.isFile() && !old.isSymbolicLink(), 'receipt-file-refused'); }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  const temporary = path.join(guard.directory, name + '.' + crypto.randomUUID() + '.tmp');
  const fd = fs.openSync(temporary, fs.constants.O_CREAT | fs.constants.O_EXCL | fs.constants.O_WRONLY | fs.constants.O_NOFOLLOW, 0o600);
  try { guard.verify(); fs.writeFileSync(fd, JSON.stringify(value, null, 2) + '\n'); fs.fsyncSync(fd); }
  finally { fs.closeSync(fd); }
  guard.verify(); fs.renameSync(temporary, destination); guard.verify();
}

function safeWindowEvidence(result) {
  const roles = new Set(['AXWindow','AXSheet','AXButton','AXTextField','AXStaticText','AXGroup','AXWebArea','AXCheckBox','AXMenu','AXMenuItem','other']);
  const controls = new Set(['Open','Cancel','A','B','Motion','Opacity','R08 rectangle']);
  const alerts = new Set(['switch-unavailable','active-close-unavailable']);
  check(Array.isArray(result.nodes) && result.nodes.length <= 4096, 'ax-evidence-refused');
  return { pid: result.pid, windowId: result.windowId, matchedAX: result.matchedAX,
    nodes: result.nodes.map(node => {
      check(node && typeof node.path === 'string' && /^(?:\d+(?:\.\d+)*)?$/.test(node.path) && node.path.length <= 128, 'ax-evidence-refused');
      const clean = { path: node.path, role: roles.has(node.role) ? node.role : 'other' };
      if (controls.has(node.control)) clean.control = node.control;
      if (alerts.has(node.alert)) clean.alert = node.alert;
      return clean;
    }) };
}

function verifyIdentity(snapshot, instance, config) {
  const app = snapshot && snapshot.app;
  check(snapshot && snapshot.ok === true && snapshot.taskId === instance.task &&
    snapshot.source && snapshot.source.matches === true && snapshot.source.startup.head === config.sourceHead &&
    snapshot.build && snapshot.build.matches === true, 'native-identity-refused');
  check(app && Number.isSafeInteger(app.pid) && app.pid > 0 && app.pid <= 2147483647 &&
    app.taskId === instance.task && app.isolated === true && app.state === 'active' &&
    snapshot.runtime && snapshot.runtime.childPid === app.pid, 'flat-app-pid-required');
  return app.pid; // Never snapshot.app.manifest.pid.
}
function verifyWindow(info, pid, expected) {
  check(info && info.pid === pid && Number.isSafeInteger(info.windowId) && info.windowId > 0 &&
    info.windowId <= 4294967295 && info.matchedAX === true &&
    (!expected || info.windowId === expected), 'owned-window-refused');
  return info.windowId;
}
async function runOperator(config, services) {
  // Before this gate there is no package access, journal, process, AX or capture.
  authorize(config);
  const receipt = { schema: 'nemo.p38-operator/1', issue: 1645, grant: config.grant.id,
    source: config.sourceHead, task: config.taskId, startAttempts: 0, appPid: null, windowId: null,
    actions: [], status: 'partial', cleanup: 'not-required', helperCleanup: 'not-required', installedAcceptance: 'not-established' };
  let controller, instance, app, timer, expire, failure = null;
  const deadline = new Promise((_, reject) => { expire = () => reject(Object.assign(new Error('watchdog-timeout'), { code: 'watchdog-timeout' })); });
  const bounded = work => Promise.race([work, deadline]);
  const save = () => services.save(receipt);
  try {
    app = await services.package(config);
    receipt.executableSha256 = app.executableSha256; receipt.buildProofSha256 = app.proofHash;
    await services.journal(config); // Exclusive grant journal survives cleanup and prevents reuse.
    save();
    controller = services.controller(app, config);
    timer = services.setTimer(expire, config.timeoutMs); // Armed BEFORE any start attempt.
    receipt.startAttempts = 1; save();
    instance = await bounded(controller.start(config.taskId, ['desktop-input', 'gpu-reference']));
    const first = await bounded(controller.status(instance));
    services.validate(first.value, instance, app, config);
    receipt.appPid = verifyIdentity(first.value, instance, config); save(); // Persist before window/UI.
    receipt.windowId = verifyWindow(await bounded(services.window('discover', receipt.appPid)), receipt.appPid); save();
    for (;;) {
      check(receipt.actions.length < 64, 'action-budget-exhausted');
      const action = await bounded(services.next());
      check(action && ['inspect', 'press', 'set', 'raise', 'capture', 'stop'].includes(action.type), 'action-refused');
      if (action.type === 'stop') break;
      if (['press', 'set'].includes(action.type)) check(typeof action.element === 'string' && /^\d+(?:\.\d+)*$/.test(action.element), 'element-refused');
      if (action.type === 'set') check(typeof action.value === 'string' && action.value.length <= 4096, 'value-refused');
      const status = await bounded(controller.status(instance));
      services.validate(status.value, instance, app, config);
      check(verifyIdentity(status.value, instance, config) === receipt.appPid, 'native-pid-changed');
      verifyWindow(await bounded(services.window('check', receipt.appPid, receipt.windowId)), receipt.appPid, receipt.windowId);
      const result = await bounded(services.window(action.type, receipt.appPid, receipt.windowId, action));
      verifyWindow(result, receipt.appPid, receipt.windowId);
      if (result.captureSha256) check(/^[a-f0-9]{64}$/.test(result.captureSha256), 'capture-proof-refused');
      receipt.actions.push({ type: action.type, ordinal: receipt.actions.length + 1,
        ...(result.captureSha256 ? { captureSha256: result.captureSha256 } : {}) });
      save();
    }
    receipt.status = 'operator-complete'; // No claim that tab/alert/native evidence passes.
  } catch (error) { failure = error; receipt.reason = ['watchdog-timeout','native-identity-refused','flat-app-pid-required','owned-window-refused',
      'native-pid-changed','action-refused','element-refused','value-refused','capture-proof-refused','action-budget-exhausted',
      'source-head-refused','fresh-build-proof-required','window-helper-refused','grant-already-attempted'].includes(error.code) ? error.code : 'operator-failed'; }
  finally {
    if (timer !== undefined) services.clearTimer(timer);
    try { if (services.close) await services.close(); receipt.helperCleanup = 'complete'; }
    catch (_) { failure = true; receipt.helperCleanup = 'incomplete'; receipt.reason = 'helper-cleanup-incomplete'; }
    if (controller) {
      try { await controller.cleanup(); receipt.cleanup = receipt.helperCleanup === 'incomplete' ? 'incomplete' : 'complete'; }
      catch (_) { receipt.cleanup = 'incomplete'; receipt.status = 'partial'; receipt.reason = 'cleanup-incomplete'; }
    }
    if (failure) receipt.status = 'partial';
    save();
  }
  return Object.freeze(receipt);
}
function defaultServices(config) {
  const harness = require('./native-harness.cjs');
  const runtime = require('../../scripts/nemo/lib/native-runtime.cjs');
  const isolation = require('../../scripts/nemo/lib/isolation.cjs');
  const identity = require('../../scripts/nemo/lib/identity.cjs');
  const directory = path.join(ROOT, 'reports', 'p38-installed-operators', hash(config.grant.id));
  let ready = false, ordinal = 0, reportGuard; const helpers = new Set();
  const input = readline.createInterface({ input: process.stdin });
  const iterator = input[Symbol.asyncIterator]();
  async function helper(mode, pid, windowId, action) {
    reportGuard.verify();
    const args = [String(pid), mode === 'discover' ? 'discover' : String(windowId)];
    if (mode !== 'discover') args.push(mode);
    if (['press', 'set'].includes(mode)) args.push(action.element);
    if (mode === 'set') args.push(action.value);
    const output = path.join(directory, 'capture-' + ++ordinal + '.png');
    if (mode === 'capture') args.push(output);
    const raw = await new Promise((resolve, reject) => {
      const child = execFile(config.windowHelper, args, { encoding: 'utf8', timeout: 10000, detached: true,
        env: { ...process.env, NEMO_P38_GRANT: config.grant.id, NEMO_P38_REPORT_ROOT: reportGuard.directory }, maxBuffer: 1024 * 1024 },
        (error, stdout) => error ? reject(error) : resolve(stdout));
      helpers.add(child); child.once('close', () => {
        try { process.kill(-child.pid, 0); }
        catch (error) { if (error.code === 'ESRCH') helpers.delete(child); }
      });
    });
    reportGuard.verify();
    const result = JSON.parse(raw);
    if (mode === 'capture') result.captureSha256 = hash(fs.readFileSync(output));
    if (mode === 'inspect') atomicJSON(reportGuard, 'ax-' + ordinal + '.json', safeWindowEvidence(result));
    return result;
  }
  return {
    async package() {
      const source = identity.sourceIdentity(); check(!source.dirty && source.head === config.sourceHead, 'source-head-refused');
      const app = harness.packagedApp(config.app, runtime);
      const proof = readJSON(config.buildProof);
      check(proof.status === 'pass' && proof.details.source.head === config.sourceHead && !proof.details.source.dirty &&
        proof.details.cleanup.stopped === true && proof.details.cleanup.released === true &&
        proof.artifacts.some(item => item.sha256 === app.executableSha256 && item.path.endsWith('/Contents/MacOS/nemo')), 'fresh-build-proof-required');
      check(typeof config.windowHelper === 'string' && /^[a-f0-9]{64}$/.test(config.helperSha256) &&
        hash(fs.readFileSync(config.windowHelper)) === config.helperSha256, 'window-helper-refused');
      fs.accessSync(config.windowHelper, fs.constants.X_OK);
      const helperProof = readJSON(config.helperProof, 16384);
      check(helperProof.schema === 'nemo.p38-window-helper/1' && helperProof.sourceHead === config.sourceHead &&
        helperProof.sourceSha256 === hash(fs.readFileSync(path.join(ROOT, 'tests/desktop/p38-owned-window.swift'))) &&
        helperProof.executableSha256 === config.helperSha256, 'window-helper-refused');
      return { ...app, source, build: identity.buildIdentity(), proofHash: hash(fs.readFileSync(config.buildProof)) };
    },
    async journal() {
      reportGuard = safeDirectory(ROOT, ['reports', 'p38-installed-operators', hash(config.grant.id)]);
      const journalGuard = safeDirectory(path.dirname(isolation.RUNTIME_ROOT), [path.basename(isolation.RUNTIME_ROOT), 'p38-grants']);
      journalGuard.verify();
      let fd;
      try { fd = fs.openSync(path.join(journalGuard.directory, hash(config.grant.id) + '.json'), fs.constants.O_CREAT | fs.constants.O_EXCL | fs.constants.O_WRONLY | fs.constants.O_NOFOLLOW, 0o600); }
      catch (error) { if (error.code === 'EEXIST') throw Object.assign(new Error('grant-already-attempted'), { code: 'grant-already-attempted' }); throw error; }
      try { journalGuard.verify(); fs.writeFileSync(fd, JSON.stringify({ grant: config.grant.id, task: config.taskId, source: config.sourceHead })); fs.fsyncSync(fd); ready = true; }
      finally { fs.closeSync(fd); }
    },
    save(receipt) { if (ready) atomicJSON(reportGuard, 'receipt.json', receipt); },
    controller(app) { return harness.createController({ root: ROOT, app: app.bundle }); },
    validate(snapshot, instance, app) { harness.validateSnapshot(snapshot, instance, { isolation, runtime, source: app.source, build: app.build, app }); },
    window: helper,
    async next() { const next = await iterator.next(); check(!next.done && next.value.length <= 8192, 'operator-input-ended'); return JSON.parse(next.value); },
    async close() {
      input.close();
      for (const child of [...helpers]) {
        const exited = new Promise(resolve => child.once('close', resolve));
        if (helpers.has(child)) {
          try { process.kill(-child.pid, 'SIGTERM'); } catch (error) { if (error.code !== 'ESRCH') throw error; }
          let timer;
          try { if (child.exitCode === null && child.signalCode === null) await Promise.race([exited, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('helper-cleanup-timeout')), 12000); })]); }
          finally { clearTimeout(timer); }
          const until = Date.now() + 3000;
          for (;;) {
            try { process.kill(-child.pid, 0); }
            catch (error) { if (error.code === 'ESRCH') break; throw error; }
            check(Date.now() < until, 'helper-group-remains');
            await new Promise(resolve => setTimeout(resolve, 50));
          }
        }
      }
    },
    setTimer: setTimeout, clearTimer: clearTimeout,
  };
}
async function main() {
  const config = readJSON(process.argv[2], 16384);
  authorize(config);
  const result = await runOperator(config, defaultServices(config));
  process.stdout.write(JSON.stringify(result) + '\n');
  process.exitCode = result.cleanup === 'incomplete' || result.status === 'partial' ? 1 : 0;
}
if (require.main === module) main().catch(() => { process.stderr.write('P38 operator refused; no installed acceptance.\n'); process.exitCode = 1; });
module.exports = { authorize, verifyIdentity, verifyWindow, runOperator, safeDirectory, atomicJSON, safeWindowEvidence };
