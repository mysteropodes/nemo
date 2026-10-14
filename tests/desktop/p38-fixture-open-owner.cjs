'use strict';
// First-party, read-only owner observer for the fixed P38 fixture-open probe.
// It never launches Nemo and persists only bounded allowlisted diagnostics.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const readline = require('node:readline');
const ROOT = path.resolve(__dirname, '../..');
const PHASES = Object.freeze(['ready', 'a-opened', 'b-opened']);
const PROBE_FAILURE_CODES = Object.freeze({
  ready: Object.freeze(['ready-open-path-unavailable', 'ready-observer-install-failed']),
  a: Object.freeze(['a-open-threw', 'a-open-false', 'a-open-unaccepted', 'a-pre-native-refusal',
    'a-native-status-rejected', 'a-native-bootstrap-unsupported-format', 'a-native-bootstrap-rejected',
    'a-native-bootstrap-resolved-open-refused', 'a-native-status-empty-no-bootstrap',
    'a-native-status-available-no-bootstrap', 'a-native-status-malformed-no-bootstrap',
    'a-observer-late-untrusted', 'a-fs-read-rejected', 'a-fs-read-resolved-open-refused']),
  b: Object.freeze(['b-open-threw', 'b-open-false', 'b-open-unaccepted', 'b-pre-native-refusal',
    'b-native-status-rejected', 'b-native-bootstrap-unsupported-format', 'b-native-bootstrap-rejected',
    'b-native-bootstrap-resolved-open-refused', 'b-native-status-empty-no-bootstrap',
    'b-native-status-available-no-bootstrap', 'b-native-status-malformed-no-bootstrap',
    'b-observer-late-untrusted', 'b-fs-read-rejected', 'b-fs-read-resolved-open-refused']),
});
const PROBE_FAILURE_PHASES = Object.freeze([
  Object.freeze({ checkpoint: 0, codes: PROBE_FAILURE_CODES.ready }),
  Object.freeze({ checkpoint: 1, codes: PROBE_FAILURE_CODES.a }),
  Object.freeze({ checkpoint: 2, codes: PROBE_FAILURE_CODES.b }),
]);
const PROBE_FAILURE_CATEGORIES = PROBE_FAILURE_PHASES.flatMap(({ codes }) =>
  codes.map(code => `probe-manifest-failed-${code}`));
const FAILURE_CODES = Object.freeze({
  config: ['observer-config-refused'],
  'driver-receipt': ['driver-receipt-refused', 'driver-first-failure', 'driver-receipt-timeout'],
  'task-owner': ['task-owner-unavailable'],
  'probe-manifest': ['probe-temp-identity-refused', 'probe-temp-read-refused',
    'probe-manifest-validation-refused', 'probe-manifest-failure-unknown',
    ...PROBE_FAILURE_CATEGORIES, 'probe-ready-timeout', 'probe-manifest-timeout'],
  'helper-start': ['helper-start-failed', 'helper-executable-refused'],
  'helper-pin': ['helper-window-identity-refused'],
  checkpoint: ['driver-first-failure', 'checkpoint-refused', 'checkpoint-timeout'],
  inspection: ['helper-input-ended', 'helper-malformed', 'inspection-refused'],
  acknowledgment: ['owner-ack-refused'],
  cleanup: ['helper-cleanup-incomplete'],
  receipt: ['receipt-write-failed'],
});
const hash = value => crypto.createHash('sha256').update(value).digest('hex');
function fail(stage, code) { throw Object.assign(new Error('observer-failed'), { observerStage: stage, observerCode: code }); }
function check(ok, stage, code) { if (!ok) fail(stage, code); }
function safeCode(stage, code) { return FAILURE_CODES[stage]?.includes(code) ? code : FAILURE_CODES[stage]?.[0] || 'observer-config-refused'; }
function failedManifestCategory(manifest) {
  if (manifest.waiting !== false || manifest.result !== 'failed') return 'probe-manifest-failure-unknown';
  const phase = PROBE_FAILURE_PHASES.find(item => item.checkpoint === manifest.checkpoint &&
    item.codes.includes(manifest.failureCode));
  return phase ? `probe-manifest-failed-${manifest.failureCode}` : 'probe-manifest-failure-unknown';
}
function validId(value) { return typeof value === 'string' && /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(value); }
function validateConfig(config) {
  check(config && typeof config === 'object' && validId(config.taskId) && validId(config.grantId) &&
    /^[a-f0-9]{40}$/.test(config.sourceHead) && typeof config.helperPath === 'string' &&
    path.isAbsolute(config.helperPath) && path.resolve(config.helperPath) === config.helperPath &&
    /^[a-f0-9]{64}$/.test(config.helperSha256) && Number.isInteger(config.timeoutMs) && config.timeoutMs >= 1000 &&
    config.timeoutMs <= 540000, 'config', 'observer-config-refused');
}
function readJSONNoFollow(file, limit = 16384) {
  const named = fs.lstatSync(file);
  check(named.isFile() && !named.isSymbolicLink() && named.size <= limit, 'driver-receipt', 'driver-receipt-refused');
  const fd = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
  try {
    const opened = fs.fstatSync(fd);
    check(opened.isFile() && opened.dev === named.dev && opened.ino === named.ino && opened.size <= limit,
      'driver-receipt', 'driver-receipt-refused');
    const chunks = [], buffer = Buffer.alloc(Math.min(65536, limit + 1));
    let total = 0;
    for (;;) {
      const count = fs.readSync(fd, buffer, 0, Math.min(buffer.length, limit + 1 - total), null);
      if (!count) break;
      total += count;
      check(total <= limit, 'driver-receipt', 'driver-receipt-refused');
      chunks.push(Buffer.from(buffer.subarray(0, count)));
    }
    check(fs.fstatSync(fd).size <= limit, 'driver-receipt', 'driver-receipt-refused');
    const bytes = Buffer.concat(chunks, total);
    const after = fs.lstatSync(file);
    check(after.isFile() && !after.isSymbolicLink() && after.dev === opened.dev && after.ino === opened.ino,
      'driver-receipt', 'driver-receipt-refused');
    return JSON.parse(bytes.toString('utf8'));
  } finally { fs.closeSync(fd); }
}
function pinDirectory(directory, stage, code) {
  const stat = fs.lstatSync(directory);
  check(stat.isDirectory() && !stat.isSymbolicLink() && fs.realpathSync(directory) === directory, stage, code);
  return { dev: stat.dev, ino: stat.ino, verify() {
    const now = fs.lstatSync(directory);
    check(now.isDirectory() && !now.isSymbolicLink() && now.dev === stat.dev && now.ino === stat.ino &&
      fs.realpathSync(directory) === directory, stage, code);
  } };
}
function writeOutcome(directory, outcome) {
  const guard = pinDirectory(directory, 'receipt', 'receipt-write-failed');
  const file = path.join(directory, 'owner-observation.json');
  const temporary = path.join(directory, '.owner-observation-' + crypto.randomUUID() + '.tmp');
  const fd = fs.openSync(temporary, fs.constants.O_CREAT | fs.constants.O_EXCL | fs.constants.O_WRONLY | fs.constants.O_NOFOLLOW, 0o600);
  try { guard.verify(); fs.writeFileSync(fd, JSON.stringify(outcome, null, 2) + '\n'); fs.fsyncSync(fd); }
  finally { fs.closeSync(fd); }
  guard.verify(); fs.renameSync(temporary, file); guard.verify();
}
function outcomeBase(config) {
  return { schema: 'nemo.p38-owner-observation/2', issue: 1645, grant: config.grantId,
    task: config.taskId, source: config.sourceHead, status: 'partial', observations: [],
    firstFailure: null, cleanup: 'not-required', installedAcceptance: 'not-established' };
}
function validateDriverReceipt(receipt, config) {
  if (receipt?.firstFailure) fail('driver-receipt', 'driver-first-failure');
  check(receipt && receipt.schema === 'nemo.p38-operator/1' && receipt.grant === config.grantId &&
    receipt.task === config.taskId && receipt.source === config.sourceHead &&
    Number.isSafeInteger(receipt.appPid) && receipt.appPid > 0 && receipt.appPid <= 2147483647 &&
    Number.isSafeInteger(receipt.windowId) && receipt.windowId > 0 && receipt.windowId <= 4294967295,
    'driver-receipt', 'driver-receipt-refused');
  return { pid: receipt.appPid, windowId: receipt.windowId };
}
function validateOwner(record, taskId) {
  check(record && record.taskId === taskId && typeof record.ownerToken === 'string' &&
    /^[a-f0-9]{32,128}$/.test(record.ownerToken), 'task-owner', 'task-owner-unavailable');
  return record.ownerToken;
}
function validateManifestIdentity(root, manifest, expected) {
  check(manifest && manifest.schema === 'nemo.p38-fixture-open/1' &&
    /^[a-f0-9-]{36}$/.test(manifest.runId) && manifest.taskKey === expected.taskKey &&
    manifest.sourceSha === expected.source && manifest.buildSha === expected.source &&
    manifest.pid === expected.pid && manifest.window === 'main' &&
    manifest.origin === 'tauri://localhost/index.html' && Array.isArray(manifest.fixtures) &&
    manifest.fixtures.length === 2 && Number.isSafeInteger(manifest.checkpoint) &&
    manifest.checkpoint >= 0 && manifest.checkpoint < PHASES.length &&
    typeof manifest.waiting === 'boolean' && typeof manifest.failed === 'boolean' &&
    ['prepared', 'waiting', 'failed', 'accepted', 'refused'].includes(manifest.result),
  'probe-manifest', 'probe-manifest-validation-refused');
  for (const [index, id] of ['A', 'B'].entries()) {
    const fixture = manifest.fixtures[index];
    const expectedHash = id === 'A'
      ? 'c5a3ec61decf27755b3a66a4ef47844660ac465196d7c3aa81856472bbaabbf1'
      : '75ab937f96de3d1865690e2b475d3968ed2eabfdfc57b8d73c7732aeffd3813b';
    check(fixture && fixture.id === id && fixture.file === path.join(root, `${id}.json`) &&
      fixture.sha256 === expectedHash && Number.isSafeInteger(fixture.device) && Number.isSafeInteger(fixture.inode),
    'probe-manifest', 'probe-manifest-validation-refused');
  }
  const pins = { schema: manifest.schema, runId: manifest.runId, taskKey: manifest.taskKey,
    sourceSha: manifest.sourceSha, buildSha: manifest.buildSha, pid: manifest.pid,
    window: manifest.window, origin: manifest.origin, fixtures: manifest.fixtures };
  check(/^[a-f0-9]{64}$/.test(manifest.manifestSha256) &&
    hash(JSON.stringify(pins)) === manifest.manifestSha256,
  'probe-manifest', 'probe-manifest-validation-refused');
}
function validateManifest(root, manifest, expected, owner) {
  check(manifest && manifest.schema === 'nemo.p38-fixture-open/1' &&
    /^[a-f0-9-]{36}$/.test(manifest.runId) && manifest.taskKey === expected.taskKey &&
    manifest.sourceSha === expected.source && manifest.buildSha === expected.source &&
    manifest.pid === expected.pid && manifest.window === 'main' &&
    manifest.origin === 'tauri://localhost/index.html' && Array.isArray(manifest.fixtures) &&
    manifest.fixtures.length === 2, 'probe-manifest', 'probe-manifest-validation-refused');
  const verified = owner.receipt(root, { runId: manifest.runId, taskKey: expected.taskKey,
    sourceSha: expected.source, buildSha: expected.source, pid: expected.pid,
    phase: PHASES[manifest.checkpoint] });
  check(verified.value.manifestSha256 === manifest.manifestSha256, 'probe-manifest', 'probe-manifest-validation-refused');
  return verified.value;
}
function helperWindowIsPinned(value, pid, windowId) {
  return value && value.status === 'pinned' && value.pid === pid && value.windowId === windowId && value.matchedAX === true;
}
function verifyHelperExecutable(file, expectedSha256) {
  try {
    const named = fs.lstatSync(file);
    check(path.isAbsolute(file) && path.resolve(file) === file && fs.realpathSync(file) === file &&
      named.isFile() && !named.isSymbolicLink() && named.size > 0 && named.size <= 64 * 1024 * 1024 &&
      (named.mode & 0o111) !== 0, 'helper-start', 'helper-executable-refused');
    const fd = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
    try {
      const opened = fs.fstatSync(fd);
      check(opened.isFile() && opened.dev === named.dev && opened.ino === named.ino &&
        opened.size === named.size && (opened.mode & 0o111) !== 0, 'helper-start', 'helper-executable-refused');
      const digest = crypto.createHash('sha256'), buffer = Buffer.alloc(65536);
      let total = 0;
      for (;;) {
        const count = fs.readSync(fd, buffer, 0, buffer.length, null);
        if (!count) break;
        total += count;
        check(total <= 64 * 1024 * 1024, 'helper-start', 'helper-executable-refused');
        digest.update(buffer.subarray(0, count));
      }
      const after = fs.fstatSync(fd), namedAfter = fs.lstatSync(file);
      check(after.dev === opened.dev && after.ino === opened.ino && after.size === opened.size &&
        namedAfter.isFile() && !namedAfter.isSymbolicLink() && namedAfter.dev === opened.dev &&
        namedAfter.ino === opened.ino && digest.digest('hex') === expectedSha256,
        'helper-start', 'helper-executable-refused');
      return { path:file, dev:opened.dev, ino:opened.ino, size:opened.size, sha256:expectedSha256 };
    } finally { fs.closeSync(fd); }
  } catch (error) {
    if (error?.observerStage === 'helper-start') throw error;
    fail('helper-start', 'helper-executable-refused');
  }
}
function helperEnvironment(grantId, reportRoot) {
  return { NEMO_P38_GRANT: grantId, NEMO_P38_REPORT_ROOT: reportRoot };
}
async function stopSpawnedHelper(child) {
  const waitForClose = async timeoutMs => {
    if (child.exitCode !== null || child.signalCode !== null) return true;
    return await Promise.race([
      new Promise(resolve => child.once('close', () => resolve(true))),
      new Promise(resolve => setTimeout(() => resolve(false), timeoutMs)),
    ]);
  };
  try { child.kill('SIGTERM'); } catch { fail('helper-start', 'helper-cleanup-incomplete'); }
  if (await waitForClose(500)) return;
  try { child.kill('SIGKILL'); } catch { fail('helper-start', 'helper-cleanup-incomplete'); }
  if (!await waitForClose(500)) fail('helper-start', 'helper-cleanup-incomplete');
}
async function runObserver(config, services) {
  const outcome = outcomeBase(config);
  let helper = null, firstFailure = null, currentStage = 'config';
  const started = services.now();
  const elapsed = () => Math.max(0, Math.min(540000, Math.round(services.now() - started)));
  try {
    validateConfig(config);
    const deadline = started + config.timeoutMs;
    currentStage = 'driver-receipt';
    const driver = await services.waitForDriver(config, deadline);
    const window = validateDriverReceipt(driver, config);
    outcome.pid = window.pid; outcome.windowId = window.windowId;
    currentStage = 'task-owner';
    const ownerToken = validateOwner(await services.readOwner(config.taskId), config.taskId);
    const expected = { taskId: config.taskId, taskKey: services.taskKey(config.taskId), source: config.sourceHead,
      pid: window.pid, windowId: window.windowId };
    currentStage = 'probe-manifest';
    const probe = await services.waitForProbe(config, expected, deadline);
    check(probe && typeof probe.root === 'string' && path.isAbsolute(probe.root) && probe.manifest,
      'probe-manifest', 'probe-manifest-validation-refused');
    const initial = validateManifest(probe.root, probe.manifest, expected, services.owner);
    outcome.runId = initial.runId; outcome.build = initial.buildSha;
    currentStage = 'helper-start';
    helper = await services.startHelper(config, window, probe);
    currentStage = 'helper-pin';
    const pinned = await helper.next();
    check(helperWindowIsPinned(pinned, window.pid, window.windowId), 'helper-pin', 'helper-window-identity-refused');
    for (let index = 0; index < PHASES.length; index++) {
      const phase = PHASES[index];
      currentStage = 'checkpoint';
      const manifest = await services.waitForCheckpoint(config, expected, probe, phase, index, deadline);
      validateManifest(probe.root, manifest, expected, services.owner);
      currentStage = 'inspection';
      const snapshot = await helper.inspect(window);
      let decision = 'accepted';
      try { services.owner.visibleEvidence(snapshot, { ...manifest, ...expected, phase }); }
      catch { decision = 'refused'; }
      currentStage = 'acknowledgment';
      try {
        services.owner.acknowledge(probe.root, { ...manifest, ...expected, phase }, ownerToken, decision,
          decision === 'accepted' ? snapshot : undefined);
      } catch { fail('acknowledgment', 'owner-ack-refused'); }
      outcome.observations.push({ phase, decision, pid: window.pid, windowId: window.windowId, checkpoint: index });
      if (decision === 'refused') break;
    }
    if (outcome.observations.length === PHASES.length && outcome.observations.every(item => item.decision === 'accepted')) {
      currentStage = 'inspection';
      await helper.quit(window);
      outcome.status = 'observed';
    }
  } catch (error) {
    const stage = FAILURE_CODES[error?.observerStage] ? error.observerStage : currentStage;
    firstFailure = { stage, code: safeCode(stage, error?.observerCode || error?.code), elapsedMs: elapsed() };
    outcome.firstFailure = firstFailure;
    if (error?.helperCleanupIncomplete) outcome.cleanup = 'incomplete';
    try { services.save(outcome); } catch { /* Keep the primary failure; no raw error is recorded. */ }
  } finally {
    if (helper) {
      currentStage = 'cleanup';
      try { await helper.close(); outcome.cleanup = 'complete'; }
      catch {
        outcome.cleanup = 'incomplete';
        outcome.status = 'partial';
        if (!firstFailure) {
          firstFailure = { stage: 'cleanup', code: 'helper-cleanup-incomplete', elapsedMs: elapsed() };
          outcome.firstFailure = firstFailure;
        }
      }
    }
    try { services.save(outcome); }
    catch {
      outcome.status = 'partial';
      if (!firstFailure) outcome.firstFailure = { stage: 'receipt', code: 'receipt-write-failed', elapsedMs: elapsed() };
    }
  }
  return Object.freeze(outcome);
}
function waitLine(lines, timeoutMs = 10000) {
  let timer;
  return Promise.race([lines.next().then(line => {
    if (line.done || typeof line.value !== 'string' || line.value.length > 1048576)
      fail('inspection', 'helper-input-ended');
    try { return JSON.parse(line.value); } catch { fail('inspection', 'helper-malformed'); }
  }), new Promise((_, reject) => {
    timer = setTimeout(() => reject(Object.assign(new Error('timeout'), {
      observerStage: 'inspection', observerCode: 'helper-input-ended' })), timeoutMs);
  })]).finally(() => clearTimeout(timer));
}
function defaultServices(config, testServices = {}) {
  const isolation = testServices.isolation || require(path.join(ROOT, 'scripts/nemo/lib/isolation.cjs'));
  const owner = testServices.owner || require('./p38-fixture-open-installed.cjs');
  const reportDir = path.join(ROOT, 'reports', 'p38-installed-operators', hash(config.grantId));
  const receiptFile = path.join(reportDir, 'receipt.json');
  // This observer runs only while the exact driver task is active.
  const roots = testServices.roots || isolation.taskRoots(config.taskId);
  const now = testServices.now || Date.now;
  const pause = testServices.pause || (ms => new Promise(resolve => setTimeout(resolve, ms)));
  const save = value => writeOutcome(reportDir, value);
  async function waitForDriver(_config, deadline) {
    for (;;) {
      let receipt;
      try { receipt = readJSONNoFollow(receiptFile); }
      catch (error) { if (error.code !== 'ENOENT') fail('driver-receipt', 'driver-receipt-refused'); }
      if (receipt?.firstFailure) fail('driver-receipt', 'driver-first-failure');
      if (receipt?.appPid && receipt?.windowId) return receipt;
      if (Date.now() >= deadline) fail('driver-receipt', 'driver-receipt-timeout');
      await new Promise(resolve => setTimeout(resolve, 100));
    }
  }
  function readManifest(root) {
    try { return readJSONNoFollow(path.join(root, 'manifest.json'), 2048); }
    catch (error) {
      if (error.code === 'ENOENT') return null;
      fail('probe-manifest', 'probe-manifest-validation-refused');
    }
  }
  async function waitForProbe(_config, expected, deadline) {
    const tempGuard = pinDirectory(roots.temp, 'probe-manifest', 'probe-temp-identity-refused');
    let sawPrepared = false;
    while (now() < deadline) {
      try { tempGuard.verify(); }
      catch { fail('probe-manifest', 'probe-temp-identity-refused'); }
      let entries;
      try { entries = fs.readdirSync(roots.temp); }
      catch { fail('probe-manifest', 'probe-temp-read-refused'); }
      for (const name of entries.slice(0, 128)) {
        if (!/^p38-fixture-open-[a-f0-9-]{36}$/.test(name)) continue;
        const root = path.join(roots.temp, name);
        let meta;
        try { meta = fs.lstatSync(root); }
        catch (error) { if (error.code === 'ENOENT') continue; fail('probe-manifest', 'probe-temp-identity-refused'); }
        let real;
        try { real = fs.realpathSync(root); }
        catch { fail('probe-manifest', 'probe-temp-identity-refused'); }
        if (!meta.isDirectory() || meta.isSymbolicLink() || real !== root || path.dirname(root) !== roots.temp)
          fail('probe-manifest', 'probe-temp-identity-refused');
        const manifest = readManifest(root);
        if (!manifest) continue;
        try { validateManifestIdentity(root, manifest, expected); }
        catch { fail('probe-manifest', 'probe-manifest-validation-refused'); }
        if (manifest.failed === true) {
          fail('probe-manifest', failedManifestCategory(manifest));
        }
        // The native probe first publishes a prepared manifest, then waits at
        // checkpoint zero. Discovery must tolerate that startup interval.
        if (manifest.waiting === false && manifest.checkpoint === 0) { sawPrepared = true; continue; }
        if (manifest.waiting !== true || manifest.checkpoint !== 0)
          fail('probe-manifest', 'probe-manifest-validation-refused');
        try { validateManifest(root, manifest, expected, owner); return { root, manifest }; }
        catch { fail('probe-manifest', 'probe-manifest-validation-refused'); }
      }
      await pause(100);
    }
    if (sawPrepared) fail('probe-manifest', 'probe-ready-timeout');
    fail('probe-manifest', 'probe-manifest-timeout');
  }
  async function waitForCheckpoint(_config, _expected, probe, _phase, index, deadline) {
    while (Date.now() < deadline) {
      let driver;
      try { driver = readJSONNoFollow(receiptFile); }
      catch { fail('checkpoint', 'driver-first-failure'); }
      if (driver?.firstFailure) fail('checkpoint', 'driver-first-failure');
      const manifest = readManifest(probe.root);
      if (manifest?.failed) fail('checkpoint', 'checkpoint-refused');
      if (manifest?.waiting === true && manifest?.checkpoint === index) return manifest;
      await new Promise(resolve => setTimeout(resolve, 75));
    }
    fail('checkpoint', 'checkpoint-timeout');
  }
  async function startHelper(_config, window) {
    const pinned = verifyHelperExecutable(config.helperPath, config.helperSha256);
    const child = spawn(pinned.path, ['--persistent', String(window.pid)], { detached: true,
      stdio: ['pipe', 'pipe', 'ignore'], env: helperEnvironment(config.grantId, reportDir) });
    child.spawnFailed = false;
    await new Promise((resolve, reject) => {
      child.once('spawn', resolve);
      child.once('error', () => { child.spawnFailed = true; reject(new Error('helper-spawn-failed')); });
    }).catch(() => fail('helper-start', 'helper-start-failed'));
    try {
      const afterSpawn = verifyHelperExecutable(config.helperPath, config.helperSha256);
      check(afterSpawn.dev === pinned.dev && afterSpawn.ino === pinned.ino && afterSpawn.size === pinned.size &&
        afterSpawn.sha256 === pinned.sha256, 'helper-start', 'helper-executable-refused');
    } catch (error) {
      let cleanupIncomplete = false;
      try { await stopSpawnedHelper(child); } catch { cleanupIncomplete = true; }
      const refusal = new Error('helper-start-failed');
      refusal.observerStage = 'helper-start';
      refusal.observerCode = error?.observerCode || 'helper-executable-refused';
      refusal.helperCleanupIncomplete = cleanupIncomplete;
      throw refusal;
    }
    const input = readline.createInterface({ input: child.stdout, terminal: false });
    const lines = input[Symbol.asyncIterator]();
    let closed = false;
    const send = value => {
      if (!child.stdin.write(JSON.stringify(value) + '\n')) fail('inspection', 'inspection-refused');
    };
    return {
      async next() {
        try { return await waitLine(lines); }
        catch (error) {
          if (child.spawnFailed) fail('helper-start', 'helper-start-failed');
          fail('helper-pin', 'helper-window-identity-refused');
        }
      },
      async inspect(identity) {
        try {
          send({ type: 'inspect', target: 'main', pid: identity.pid, windowId: identity.windowId, depth: 5, maxNodes: 128 });
          const value = await waitLine(lines);
          check(value?.status === 'ok' && value.type === 'inspect' && value.pid === identity.pid &&
            value.windowId === identity.windowId, 'inspection', 'inspection-refused');
          return value;
        } catch (error) {
          if (error?.observerStage === 'inspection') throw error;
          fail('inspection', 'inspection-refused');
        }
      },
      async quit(identity) {
        send({ type: 'quit', pid: identity.pid, windowId: identity.windowId });
        const value = await waitLine(lines);
        check(value?.status === 'quit' && value.pid === identity.pid && value.windowId === identity.windowId,
          'inspection', 'inspection-refused');
      },
      async close() {
        if (closed) return;
        closed = true; input.close();
        if (child.spawnFailed && !Number.isSafeInteger(child.pid)) return;
        if (child.exitCode !== null || child.signalCode !== null) return;
        try { child.kill('SIGTERM'); } catch { fail('cleanup', 'helper-cleanup-incomplete'); }
        const ended = await Promise.race([
          new Promise(resolve => child.once('close', () => resolve(true))),
          new Promise(resolve => setTimeout(() => resolve(false), 500)),
        ]);
        if (!ended) {
          try { child.kill('SIGKILL'); } catch { fail('cleanup', 'helper-cleanup-incomplete'); }
          const killed = await Promise.race([
            new Promise(resolve => child.once('close', () => resolve(true))),
            new Promise(resolve => setTimeout(() => resolve(false), 500)),
          ]);
          if (!killed) fail('cleanup', 'helper-cleanup-incomplete');
        }
      },
    };
  }
  return { now: Date.now, taskKey: isolation.idKey, owner, save,
    waitForDriver, readOwner: isolation.readLauncher, waitForProbe, startHelper, waitForCheckpoint };
}
async function main() {
  const [taskId, grantId, helperPath, sourceHead, helperSha256, timeoutRaw] = process.argv.slice(2);
  const config = { taskId, grantId, helperPath, sourceHead, helperSha256,
    timeoutMs: timeoutRaw ? Number(timeoutRaw) : 540000 };
  try {
    validateConfig(config);
    const outcome = await runObserver(config, defaultServices(config));
    process.stdout.write(JSON.stringify({ status: outcome.status, firstFailure: outcome.firstFailure,
      cleanup: outcome.cleanup, installedAcceptance: outcome.installedAcceptance }) + '\n');
    process.exitCode = outcome.firstFailure || outcome.status !== 'observed' ? 1 : 0;
  } catch (error) {
    const stage = FAILURE_CODES[error?.observerStage] ? error.observerStage : 'config';
    process.stderr.write(JSON.stringify({ stage, code: safeCode(stage, error?.observerCode) }) + '\n');
    process.exitCode = 1;
  }
}
if (require.main === module) main();
module.exports = { PHASES, FAILURE_CODES, PROBE_FAILURE_CODES, validateConfig, validateDriverReceipt, validateOwner,
  validateManifest, validateManifestIdentity, verifyHelperExecutable, helperEnvironment, stopSpawnedHelper, runObserver, defaultServices, writeOutcome, safeCode, fail };
