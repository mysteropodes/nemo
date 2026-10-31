'use strict';
// One normal packaged runtime, with the existing owner/window/input protocols.
const fs = require('node:fs');
const path = require('node:path');
const { spawn, execFileSync } = require('node:child_process');
const harness = require('./native-harness.cjs');
const { requireCheck: check, hash } = harness;
const { cleanupHelper } = require('./p38-helper-protocol.cjs');
const { OperatorInputError } = require('./cs7-runner.cjs');
const ROOT = path.resolve(__dirname, '../..');

function createInput({ executable, env, timeout = 10_000 }) {
  let child, pinned, ordinal = 0, pending, buffer = '', failed = false, starting = false, lastCommand = null;
  function fail() {
    failed = true; buffer = '';
    if (pending) { const reject = pending.reject; clearTimeout(pending.timer); pending = null;
      reject(new OperatorInputError('input-refused', { command: lastCommand, ordinal })); }
  }
  function next(deadline = timeout) {
    check(!failed && !pending, 'input is failed or busy');
    return new Promise((resolve, reject) => {
      pending = { resolve, reject, timer: setTimeout(fail, deadline) };
    });
  }
  async function open(pid) {
    check(!starting, 'input helper already started'); starting = true;
    child = spawn(executable, ['--ui32-persistent', String(pid)], {
      detached: true, stdio: ['pipe', 'pipe', 'pipe'], env,
    });
    child.p38DetachedGroup = true;
    child.p38GroupMayContainDescendants = true; // The finite capture command spawns screencapture.
    child.on('error', fail); child.stdin.on('error', fail); child.stderr.resume();
    child.once('close', () => { if (pending) fail(); });
    child.stdout.setEncoding('utf8');
    child.stdout.on('data', chunk => {
      if (failed) return;
      buffer += chunk;
      if (Buffer.byteLength(buffer) > 1024 * 1024) { fail(); return; }
      let index;
      while ((index = buffer.indexOf('\n')) >= 0) {
        const line = buffer.slice(0, index); buffer = buffer.slice(index + 1);
        if (!pending || failed) { fail(); return; }
        let value;
        try { value = JSON.parse(line); } catch { fail(); return; }
        if (!value || value.status === 'refused') { fail(); return; }
        const resolve = pending.resolve; clearTimeout(pending.timer); pending = null; resolve(value);
      }
    });
    pinned = await next(Math.max(timeout, 5000));
    check(pinned.status === 'pinned' && pinned.purpose === 'UI32' && pinned.pid === pid &&
      pinned.matchedAX === true && Number.isInteger(pinned.windowId) && pinned.windowId > 0 &&
      Number.isSafeInteger(pinned.processStartSeconds) && Number.isSafeInteger(pinned.processStartMicroseconds), 'input window identity refused');
    return { ...pinned };
  }
  async function command(command, option, value) {
    check(pinned && !failed && !pending && ordinal < 255, 'input helper unavailable');
    check(/^[a-z][a-z0-9.-]*$/.test(command), 'input command invalid');
    const request = { purpose: 'UI32', pid: pinned.pid, windowId: pinned.windowId, command,
      ...(option === undefined ? {} : { option }), ...(value === undefined ? {} : { value }) };
    const line = JSON.stringify(request);
    check(Buffer.byteLength(line) <= 2048, 'input request too large');
    const reply = next(); ordinal += 1; lastCommand = command;
    child.stdin.write(line + '\n');
    try {
      const result = await reply;
      check(result.status === 'ok' && result.purpose === 'UI32' && result.pid === pinned.pid &&
        result.windowId === pinned.windowId && result.ordinal === ordinal && result.command === command, 'input response identity refused');
      return result;
    } catch { fail(); throw new OperatorInputError('input-refused', { command: lastCommand, ordinal }); }
  }
  async function close() {
    if (pending) fail();
    if (child) await cleanupHelper(child);
  }
  return { open, command, close };
}

function createInstalledSession(config) {
  const identity = require('../../scripts/nemo/lib/identity.cjs');
  const runtime = require('../../scripts/nemo/lib/native-runtime.cjs');
  const isolation = require('../../scripts/nemo/lib/isolation.cjs');
  const source = identity.sourceIdentity(), build = identity.buildIdentity();
  check(!source.dirty && source.head === config.sourceSha, 'clean frozen source required');
  check(/^1062\/[0-9]+$/.test(config.grant || ''), 'recorded local slot required');
  const app = harness.packagedApp(config.app, runtime);
  check(app.executableSha256 === config.executableSha256, 'packaged executable differs');
  const proof = JSON.parse(fs.readFileSync(config.buildProof, 'utf8'));
  check(proof.status === 'pass' && proof.details?.source?.head === source.head && proof.details.source.dirty === false &&
    proof.details.cleanup?.stopped === true && proof.details.cleanup.released === true &&
    proof.artifacts?.some(a => a.sha256 === app.executableSha256 && a.path.endsWith('/Contents/MacOS/nemo')), 'standard package proof required');
  const helperProof = JSON.parse(fs.readFileSync(config.helperProof, 'utf8'));
  const helper = require('./ui32-owned-window-build.cjs').manifest();
  check(helperProof.schema === 'nemo.ui32-window-helper/1' && helperProof.sourceHead === source.head &&
    helperProof.sourceSha256 === helper.sourceSha256 && helperProof.executableSha256 === hash(config.helper), 'frozen helper proof required');
  const report = fs.realpathSync(config.reportDirectory), stat = fs.lstatSync(config.reportDirectory);
  check(report === config.reportDirectory && stat.isDirectory() && !stat.isSymbolicLink(), 'owned report directory required');
  const environment = { ...process.env };
  // Never inherit another historical operator's admission or fixture mode.
  for (const key of Object.keys(environment)) if (/^NEMO_(UI32|P38|REFERENCE|PNG_UI)_/.test(key)) delete environment[key];
  Object.assign(environment, { NEMO_UI32_GRANT: config.grant, NEMO_UI32_REPORT_ROOT: report });
  if (config.fixtureKind) environment.NEMO_UI32_FIXTURE_KIND = config.fixtureKind;
  const input = createInput({ executable: config.helper, env: environment });
  const operatorDirectory = path.join(report, 'operator');
  check(!fs.existsSync(operatorDirectory), 'fresh operator handoff directory required');
  fs.mkdirSync(operatorDirectory, { mode: 0o700 });
  const operator = require('./cs7-scenarios/operator.cjs').createOperator({
    directory: operatorDirectory,
    announce: request => process.stdout.write(JSON.stringify({ operatorRequest: request }) + '\n'),
  });
  const unsigned = config.playObserver === true ? require('./cs7-play-evidence.cjs').qualifyUnsigned(app.bundle) : false;
  const appEnvironment = { ...process.env };
  delete appEnvironment.NEMO_TEST_PLAY_OBSERVER;
  delete appEnvironment.NEMO_NATIVE_OCCLUSION_DIAGNOSTICS;
  delete appEnvironment.NEMO_TEST_PLAY_FIXED_CLOCK;
  if (config.playObserver === true) Object.assign(appEnvironment, {
    NEMO_TEST_PLAY_OBSERVER: '1', NEMO_NATIVE_OCCLUSION_DIAGNOSTICS: '1',
  });
  // P24: opt-in virtual playback time, only with the observer package (see play-publication-observer-bootstrap.js).
  if (config.playObserver === true && config.play?.fixedClock === true) appEnvironment.NEMO_TEST_PLAY_FIXED_CLOCK = '1';
  const controller = harness.createController({ root: ROOT, app: app.bundle, env: appEnvironment });
  const tree = execFileSync('git', ['rev-parse', 'HEAD^{tree}'], { cwd: ROOT, encoding: 'utf8' }).trim();
  let instance, started = false, pinned, latestSnapshot, cleaned = false, lastIdentity;
  async function currentIdentity() {
    check(instance, 'installed session not started');
    const current = identity.sourceIdentity();
    check(!current.dirty && current.head === source.head && hash(app.executable) === app.executableSha256 &&
      hash(config.helper) === helperProof.executableSha256, 'installed source or executable changed');
    const status = await controller.status(instance);
    check(status.code === 0, 'installed status refused');
    harness.validateSnapshot(status.value, instance, { isolation, runtime, source, build, app });
    latestSnapshot = status.value;
    const pid = status.value.app.pid;
    if (pinned) check(pid === pinned.pid, 'installed PID changed');
    return lastIdentity = { sourceSha: source.head, tree, driverSha: source.head, executableSha256: app.executableSha256,
      sessionId: config.taskId, pid, dirty: false };
  }
  return {
    async start() {
      check(!started, 'installed session cannot restart'); started = true;
      instance = await controller.start(config.taskId, ['desktop-input', 'gpu-reference']);
      const current = await currentIdentity(); pinned = await input.open(current.pid); return current;
    },
    identity: currentIdentity,
    async input(command, option, value) {
      await currentIdentity();
      return input.command(command, option, value);
    },
    async runtimeContext() {
      const current = await currentIdentity();
      return { identity: current, snapshot: structuredClone(latestSnapshot), window: { ...pinned } };
    },
    async playContext() {
      const current = await currentIdentity(), observer = latestSnapshot.app.playObserver;
      check(config.playObserver === true && observer?.schema === 'nemo.play-observer/1' &&
        /^[a-f0-9-]{36}$/.test(observer.runId), 'owned opt-in Play observer unavailable');
      return { identity: current, runId: observer.runId, windowId: pinned.windowId };
    },
    retainedPlayEvidence() {
      check(config.playObserver === true, 'Play observer was not admitted');
      return require('./cs7-play-evidence.cjs').retained({ instance, identity: lastIdentity,
        windowId: pinned.windowId, cleaned, unsigned,
        directory: path.join(ROOT, 'reports', 'play-observer-' + config.taskId) });
    },
    async requestEvidence(request) {
      const current = await currentIdentity();
      const evidence = await operator.request({ ...request, identity: current });
      check(require('node:util').isDeepStrictEqual(await currentIdentity(), current), 'operator candidate changed');
      return evidence;
    },
    async cleanup() {
      let failed = false;
      operator.close();
      try { await input.close(); } catch { failed = true; }
      try { await controller.cleanup(); } catch { failed = true; }
      check(!failed, 'installed session cleanup incomplete');
      cleaned = true;
    },
  };
}

module.exports = { createInput, createInstalledSession };
