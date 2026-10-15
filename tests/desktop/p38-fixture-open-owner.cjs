'use strict';
// First-party, read-only owner observer for the fixed P38 fixture-open probe.
// It never launches Nemo and persists only bounded allowlisted diagnostics.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const readline = require('node:readline');
const { safeWindowEvidence, verifyWindow, cleanupHelper } = require('./p38-installed-driver.cjs');
const { INSPECTION_FAILURE_CODES, inspectProjectTabs, failureCompleteness, failureBudget } = require('./p38-inspection-diagnostics.cjs');
const ROOT = path.resolve(__dirname, '../..');
const PHASES = Object.freeze(['ready', 'a-opened', 'b-opened', 'b-closed']);
const OPEN_STAGES=['receipt','installed-status','metadata','first-preview','first-owner','reveal','second-preview','second-owner','ui-commit','publication','subscriber'].concat(
  ['first-preview','second-preview'].flatMap(prefix=>['frame-range','mapping','viewport','owner','receipt','status','work-id','view-generation','receipt-access',
    'status-stale-discarded','status-deferred-timeout','status-deferred-occluded','status-failed-validation','status-failed-device-lost',
    'status-failed-retry-lost','status-failed-retry-outdated','status-failed-recreate','status-failed-reconfigure'].map(detail=>prefix+'-'+detail)));
const OPEN_FAILURES=OPEN_STAGES.flatMap(stage=>['invoke-rejected','local-refusal'].flatMap(kind=>
  ['invalid_request','unavailable','unsupported_format','replacement_refused','stale_revision','stale_owner','cancelled','wrong_instance','wrong_document','cancelled_before_dispatch','unknown'].flatMap(code=>
    ['notDispatched','other'].map(disposition=>`post-bootstrap-${stage}:${kind}:${code}:${disposition}`))));
const PROBE_FAILURE_CODES = Object.freeze({
  ready: Object.freeze(['ready-open-path-unavailable', 'ready-observer-install-failed']),
  a: Object.freeze(['a-open-threw', 'a-open-false', 'a-open-unaccepted', 'a-pre-native-refusal',
    'a-native-status-rejected', 'a-native-bootstrap-unsupported-format', 'a-native-bootstrap-rejected',
    'a-native-bootstrap-resolved-open-refused', 'a-native-status-empty-no-bootstrap',
    'a-native-status-available-no-bootstrap', 'a-native-status-malformed-no-bootstrap',
    'a-observer-late-untrusted', 'a-fs-read-rejected', 'a-fs-read-resolved-open-refused',...OPEN_FAILURES.map(code=>'a-'+code)]),
  b: Object.freeze(['b-open-threw', 'b-open-false', 'b-open-unaccepted', 'b-pre-native-refusal',
    'b-native-status-rejected', 'b-native-bootstrap-unsupported-format', 'b-native-bootstrap-rejected',
    'b-native-bootstrap-resolved-open-refused', 'b-native-status-empty-no-bootstrap',
    'b-native-status-available-no-bootstrap', 'b-native-status-malformed-no-bootstrap',
    'b-observer-late-untrusted', 'b-fs-read-rejected', 'b-fs-read-resolved-open-refused',
    'b-switch-unaccepted', 'b-a-state-changed',...OPEN_FAILURES.map(code=>'b-'+code)]),
  closed: Object.freeze(['b-close-precondition-refused', 'b-close-threw', 'b-close-unaccepted', 'b-close-a-state-changed']),
});
const PROBE_FAILURE_PHASES = Object.freeze([
  Object.freeze({ checkpoint: 0, codes: PROBE_FAILURE_CODES.ready }),
  Object.freeze({ checkpoint: 1, codes: PROBE_FAILURE_CODES.a }),
  Object.freeze({ checkpoint: 2, codes: PROBE_FAILURE_CODES.b }),
  Object.freeze({ checkpoint: 3, codes: PROBE_FAILURE_CODES.closed }),
]);
const PROBE_FAILURE_CATEGORIES = PROBE_FAILURE_PHASES.flatMap(({ codes }) =>
  codes.map(code => `probe-manifest-failed-${code}`));
const FAILURE_CODES = Object.freeze({
  config: ['observer-config-refused'],
  'driver-receipt': ['driver-receipt-refused', 'driver-first-failure', 'driver-receipt-timeout'],
  'task-owner': ['task-owner-unavailable'],
  'probe-manifest': ['probe-temp-identity-refused', 'probe-temp-read-refused',
    'probe-manifest-validation-refused', 'probe-manifest-failure-unknown',
    ...PROBE_FAILURE_CATEGORIES, 'probe-manifest-failed-post-wait-validation-refused',
    'probe-ready-timeout', 'probe-manifest-timeout'],
  'helper-start': ['helper-start-failed', 'helper-executable-refused'],
  'helper-pin': ['helper-window-identity-refused'],
  checkpoint: ['driver-first-failure', 'checkpoint-refused', 'checkpoint-timeout'],
  inspection: INSPECTION_FAILURE_CODES,
  acknowledgment: ['owner-ack-refused'],
  capture: ['capture-refused'],
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
function checkpointFailureCategory(root, manifest, expected, phase, index) {
  if (manifest.failed !== true || manifest.waiting !== false || manifest.result !== 'failed')
    return 'probe-manifest-failure-unknown';
  try {
    validateManifestIdentity(root, manifest, expected);
  } catch {
    return 'probe-manifest-failure-unknown';
  }
  if (manifest.checkpoint === index) return phase === PHASES[index] ? failedManifestCategory(manifest) : 'probe-manifest-failure-unknown';
  if (phase === PHASES[index] && index > 0 && manifest.checkpoint === index - 1 &&
      manifest.failureCode === 'post-wait-validation-refused')
    return 'probe-manifest-failed-post-wait-validation-refused';
  return 'probe-manifest-failure-unknown';
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
  return JSON.parse(readBytesNoFollow(file,limit).toString('utf8'));
}
function readBytesNoFollow(file, limit, beforeRead = () => {}) {
  const named = fs.lstatSync(file);
  check(named.isFile() && !named.isSymbolicLink() && named.size <= limit, 'driver-receipt', 'driver-receipt-refused');
  const fd = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
  try {
    const opened = fs.fstatSync(fd);
    check(opened.isFile() && opened.dev === named.dev && opened.ino === named.ino && opened.size <= limit,
      'driver-receipt', 'driver-receipt-refused');
    beforeRead(fd);
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
    return bytes;
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
function pngChunksComplete(bytes) {
  let offset=8,data=false,first=true;
  while(offset+12<=bytes.length) {
    const size=bytes.readUInt32BE(offset),type=bytes.toString('ascii',offset+4,offset+8);
    if(size>bytes.length-offset-12||!/^[A-Za-z]{4}$/.test(type)||first&&(type!=='IHDR'||size!==13)||!first&&type==='IHDR')return false;
    let crc=0xffffffff;
    for(let i=offset+4;i<offset+8+size;i++) {crc^=bytes[i];for(let bit=0;bit<8;bit++)crc=(crc>>>1)^((crc&1)?0xedb88320:0);}
    if(((crc^0xffffffff)>>>0)!==bytes.readUInt32BE(offset+8+size))return false;
    if(type==='IDAT'&&size>0)data=true;
    if(type==='IEND')return size===0&&data&&offset+12===bytes.length;
    first=false;offset+=size+12;
  }
  return false;
}
function captureEvidence(directory,index,identity,reply) {
  try {
    const capture='capture-'+(index+1)+'.png';
    check(Number.isInteger(index)&&index>=1&&index<=3&&Number.isSafeInteger(identity.pid)&&identity.pid>0&&identity.pid<=2147483647&&
      Number.isSafeInteger(identity.windowId)&&identity.windowId>0&&identity.windowId<=4294967295&&reply?.status==='ok'&&reply.type==='capture'&&
      reply.matchedAX===true&&reply.pid===identity.pid&&reply.windowId===identity.windowId&&reply.capture===capture,
      'capture','capture-refused');
    const guard=pinDirectory(directory,'capture','capture-refused');
    const bytes=readBytesNoFollow(path.join(directory,capture),64*1024*1024,fd=>fs.fchmodSync(fd,0o600));guard.verify();
    check(bytes.length>=45&&bytes.subarray(0,8).equals(Buffer.from('89504e470d0a1a0a','hex'))&&
      bytes.readUInt32BE(8)===13&&bytes.toString('ascii',12,16)==='IHDR'&&
      bytes.subarray(-12).equals(Buffer.from('0000000049454e44ae426082','hex'))&&pngChunksComplete(bytes),'capture','capture-refused');
    const width=bytes.readUInt32BE(16),height=bytes.readUInt32BE(20);
    check(width>0&&height>0&&width<=16384&&height<=16384,'capture','capture-refused');
    return {phase:PHASES[index],pid:identity.pid,windowId:identity.windowId,capture,sha256:hash(bytes),width,height};
  } catch {fail('capture','capture-refused');}
}
function outcomeBase(config) {
  return { schema: 'nemo.p38-owner-observation/2', issue: 1645, grant: config.grantId,
    task: config.taskId, source: config.sourceHead, status: 'partial', observations: [],
    firstFailure: null, captures:[], cleanup: 'not-required', installedAcceptance: 'not-established' };
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
      ? 'e1bc63b0e284f521e170d8236296392bdcf19d4b1f13b4e04683727844fa71a0'
      : 'a438bda3b5c25ec3e8797d1c95fb8ade1da7b0bda675c2279499c659ece46bf6';
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
const READY_REFUSALS = Object.freeze({
  'probe-visible-state-refused':'ready-visible-state-refused',
  'probe-window-pin-refused':'ready-window-pin-refused', 'owned-window-refused':'ready-window-pin-refused',
  'ax-evidence-refused':'ready-ax-projection-refused', 'probe-phase-refused':'ready-phase-refused',
});
function readyVisibility(snapshot, expected, decision, errorCode) {
  let projection = null;
  try {
    const safe = safeWindowEvidence(snapshot);
    verifyWindow(safe, expected.pid, expected.windowId);
    if (safe.nodes.length > 128) throw new Error('projection-bound');
    projection = safe;
  } catch { /* Never persist the raw snapshot or exception, even for diagnostics. */ }
  const category = decision === 'accepted' ? 'ready-visible-accepted' :
    typeof errorCode === 'string' && Object.hasOwn(READY_REFUSALS,errorCode) ?
      READY_REFUSALS[errorCode] : 'ready-visible-unclassified';
  return { phase:'ready', decision, category, projectionStatus:projection ? 'retained' : 'refused', projection };
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
    let regionToken;
    for (let index = 0; index < PHASES.length; index++) {
      const phase = PHASES[index];
      currentStage = 'checkpoint';
      const manifest = await services.waitForCheckpoint(config, expected, probe, phase, index, deadline);
      validateManifest(probe.root, manifest, expected, services.owner);
      currentStage = 'inspection';
      const snapshot = await helper.inspect(window);
      let decision = 'accepted', visibilityCode;
      try { services.owner.visibleEvidence(snapshot, { ...manifest, ...expected, phase, regionToken }); }
      catch (error) { decision = 'refused'; visibilityCode = error?.code; }
      if (phase === 'ready') {
        outcome.readyVisibility = readyVisibility(snapshot,expected,decision,visibilityCode);
        if (decision === 'refused') {
          firstFailure = { stage:'inspection',code:outcome.readyVisibility.category,elapsedMs:elapsed() };
          outcome.firstFailure = firstFailure;
        }
        currentStage = 'receipt';
        services.save(outcome); // Durable safe evidence precedes the unchanged ACK decision.
      }
      if(decision==='accepted'&&index>0) {
        currentStage='capture';
        try {
          const capture=await helper.capture(window,index);
          check(capture?.phase===phase&&capture.pid===window.pid&&capture.windowId===window.windowId&&
            capture.capture==='capture-'+(index+1)+'.png'&&/^[a-f0-9]{64}$/.test(capture.sha256)&&
            ['width','height'].every(k=>Number.isInteger(capture[k])&&capture[k]>0&&capture[k]<=16384),'capture','capture-refused');
          outcome.captures.push(Object.fromEntries(['phase','pid','windowId','capture','sha256','width','height'].map(k=>[k,capture[k]])));
          currentStage='receipt';services.save(outcome);
        } catch {
          decision='refused';firstFailure={stage:currentStage,code:currentStage==='receipt'?'receipt-write-failed':'capture-refused',elapsedMs:elapsed()};
          outcome.firstFailure=firstFailure;
        }
      }
      if(decision==='refused') {
        if(!firstFailure) {firstFailure={stage:'inspection',code:'inspection-refused',elapsedMs:elapsed()};outcome.firstFailure=firstFailure;}
        currentStage='receipt';services.save(outcome);
      }
      currentStage = 'acknowledgment';
      try {
        services.owner.acknowledge(probe.root, { ...manifest, ...expected, phase, regionToken }, ownerToken, decision,
          decision === 'accepted' ? snapshot : undefined);
      } catch { fail('acknowledgment', 'owner-ack-refused'); }
      outcome.observations.push({ phase, decision, pid: window.pid, windowId: window.windowId, checkpoint: index });
      if(decision==='accepted')regionToken=snapshot.projectTabs?.token;
      if (decision === 'refused') break;
    }
    if (outcome.observations.length === PHASES.length && outcome.observations.every(item => item.decision === 'accepted')) {
      currentStage = 'inspection';
      await helper.quit(window);
      outcome.status = 'observed';
    }
  } catch (error) {
    const stage = FAILURE_CODES[error?.observerStage] ? error.observerStage : currentStage;
    if (!firstFailure) firstFailure = { stage, code: safeCode(stage, error?.observerCode || error?.code),
      elapsedMs: elapsed(), ...(stage === 'inspection' ? { ...failureCompleteness(error), ...failureBudget(error) } : {}) };
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
  const reportDir = testServices.reportDir || path.join(ROOT, 'reports', 'p38-installed-operators', hash(config.grantId));
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
  async function waitForCheckpoint(_config, expected, probe, phase, index, deadline) {
    while (Date.now() < deadline) {
      let driver;
      try { driver = readJSONNoFollow(receiptFile); }
      catch { fail('checkpoint', 'driver-first-failure'); }
      if (driver?.firstFailure) fail('checkpoint', 'driver-first-failure');
      const manifest = readManifest(probe.root);
      if (manifest && (manifest.failed !== false || ['failed','refused'].includes(manifest.result))) {
        const code = checkpointFailureCategory(probe.root, manifest, expected, phase, index);
        fail('probe-manifest', code);
      }
      if (manifest?.waiting === true && manifest?.failed === false && manifest?.checkpoint === index) return manifest;
      await new Promise(resolve => setTimeout(resolve, 75));
    }
    fail('checkpoint', 'checkpoint-timeout');
  }
  async function startHelper(_config, window) {
    const pinned = verifyHelperExecutable(config.helperPath, config.helperSha256);
    const child = spawn(pinned.path, ['--persistent', String(window.pid)], { detached: true,
      stdio: ['pipe', 'pipe', 'ignore'], env: helperEnvironment(config.grantId, reportDir) });
    child.p38DetachedGroup=true;child.p38GroupMayContainDescendants=false;
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
    let closed = false,capturePending=false;
    const regionState={actions:0};
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
        return await inspectProjectTabs(async request=>{send(request);return await waitLine(lines);},identity,regionState,owner);
      },
      async capture(identity,index) {
        check(Number.isInteger(index)&&index>=1&&index<=3&&regionState.actions++<63,'capture','capture-refused');
        child.p38GroupMayContainDescendants=true;capturePending=true;
        send({type:'capture',target:'main',pid:identity.pid,windowId:identity.windowId,capture:'capture-'+(index+1)+'.png'});
        const reply=await waitLine(lines);
        check(reply?.status==='ok'&&reply.type==='capture'&&reply.matchedAX===true&&reply.pid===identity.pid&&
          reply.windowId===identity.windowId&&reply.capture==='capture-'+(index+1)+'.png','capture','capture-refused');
        capturePending=false; // This matching helper reply follows screencapture waitUntilExit + window revalidation.
        return captureEvidence(reportDir,index,identity,reply);
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
        try {await cleanupHelper(child);}catch {fail('cleanup','helper-cleanup-incomplete');}
        if(capturePending)fail('cleanup','helper-cleanup-incomplete'); // Helper/group exit alone cannot prove an unanswered capture finished.
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
module.exports = { PHASES, FAILURE_CODES, PROBE_FAILURE_CODES, captureEvidence, validateConfig, validateDriverReceipt, validateOwner,
  validateManifest, validateManifestIdentity, verifyHelperExecutable, helperEnvironment, stopSpawnedHelper, runObserver, defaultServices, writeOutcome, safeCode, fail, inspectProjectTabs };
