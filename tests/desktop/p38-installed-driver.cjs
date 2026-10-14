'use strict';
// Source-only operator. Every native/package/window operation requires a NEW P38 grant.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');
const readline = require('node:readline');
const { performance } = require('node:perf_hooks');
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

function safeWindowEvidence(result, reference = false, completenessDiagnostics = false) {
  check(result && Number.isSafeInteger(result.pid) && result.pid > 0 && result.pid <= 2147483647, 'ax-evidence-refused');
  verifyWindow(result, result.pid);
  const roles = new Set(['AXWindow','AXSheet','AXButton','AXTextField','AXStaticText','AXGroup','AXWebArea','AXCheckBox','AXMenu','AXMenuItem','AXRow','AXOutline','AXScrollArea','other']);
  if(reference)roles.add('AXPopUpButton');
  const controls = new Set(['Open','Open Project…','Cancel','A','B','A.json','B.json','Project tabs','Motion','Opacity','R08 rectangle']);
  if (reference) for (const label of require('./reference-installed-policy.cjs').labels) controls.add(label);
  if (reference) for (const label of ['Go','Go to the folder:','Go to folder:','Go to Folder','Go to the folder']) controls.add(label);
  const alerts = new Set(['switch-unavailable','active-close-unavailable']);
  check(Array.isArray(result.nodes) && result.nodes.length <= 4096, 'ax-evidence-refused');
  const clean = { pid: result.pid, windowId: result.windowId, matchedAX: result.matchedAX,
    nodes: result.nodes.map(node => {
      check(node && typeof node.path === 'string' && /^(?:\d+(?:\.\d+)*)?$/.test(node.path) && node.path.length <= 128, 'ax-evidence-refused');
      const clean = { path: node.path, role: roles.has(node.role) ? node.role : 'other' };
      if (controls.has(node.control)) clean.control = node.control;
      if (alerts.has(node.alert)) clean.alert = node.alert;
      if (['child-owner','children-unavailable'].includes(node.blocked)) clean.blocked = node.blocked;
      for (const key of ['press','confirm','selected','enabled']) if (typeof node[key] === 'boolean') clean[key] = node[key];
      if(node.childCount!==undefined) {
        check(Number.isInteger(node.childCount)&&node.childCount>=0&&node.childCount<4096,'ax-evidence-refused');
        clean.childCount=node.childCount;
      }
      try { Object.assign(clean, require('./p38-inspection-diagnostics.cjs').childrenWitnessFacts(node)); }
      catch { check(false, 'ax-evidence-refused'); }
      if(node.role==='AXTextField' && ['plain','search','other','unreadable'].includes(node.textFieldKind)) clean.textFieldKind=node.textFieldKind;
      return clean;
    }) };
  for(const key of ['truncated','depthClipped','childrenUnreadable','foreignChildren','complete','childrenComplete'])
    if(result[key]!==undefined) { check(typeof result[key]==='boolean','ax-evidence-refused');clean[key]=result[key]; }
  if(result.pageToken!==undefined) {
    check(typeof result.pageToken==='string'&&/^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/.test(result.pageToken),'ax-evidence-refused');
    clean.pageToken=result.pageToken;
    for(const key of ['childOffset','nextChildOffset','childCount']) {
      check(Number.isInteger(result[key])&&result[key]>=0&&result[key]<4096,'ax-evidence-refused');clean[key]=result[key];
    }
    check(clean.childOffset<=clean.nextChildOffset&&clean.nextChildOffset<=clean.childCount&&
      (clean.nextChildOffset>clean.childOffset||clean.childOffset===clean.childCount)&&typeof clean.childrenComplete==='boolean','ax-evidence-refused');
  } else check(['childOffset','nextChildOffset','childCount','childrenComplete'].every(k=>result[k]===undefined),'ax-evidence-refused');
  if(result.projectTabs!==undefined) {
    const region=result.projectTabs;
    check(region&&typeof region.path==='string'&&/^(?:\d+(?:\.\d+)*)?$/.test(region.path)&&region.path.length<=128&&
      typeof region.token==='string'&&/^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/.test(region.token),'ax-evidence-refused');
    clean.projectTabs={path:region.path,token:region.token};
    if(region.unique!==undefined) {check(typeof region.unique==='boolean','ax-evidence-refused');clean.projectTabs.unique=region.unique;}
  }
  if (completenessDiagnostics === true && !reference)
    clean.completenessSummary = require('./p38-inspection-diagnostics.cjs').summarizeCompleteness(result);
  return clean;
}
function safeSelectionDiagnostics(value) {
  check(value && typeof value === 'object' && !Array.isArray(value), 'ax-evidence-refused');
  const stages = new Set(['row-capability','row-write','row-readback']);
  check(stages.has(value.stage), 'ax-evidence-refused');
  const clean = { stage:value.stage };
  for (const key of ['rowQueryError','rowSettableQueryError','rowSetError','rowReadbackError',
    'outlineSelectedRowsQueryError','outlineSelectedRowsSettableQueryError','outlineRowsQueryError']) {
    if (value[key] !== undefined) { check(Number.isInteger(value[key]) && value[key] >= -32768 && value[key] <= 0, 'ax-evidence-refused'); clean[key]=value[key]; }
  }
  for (const key of ['rowSelected','rowSelectedAfter','rowSettable','outlineFound','outlineSelectedRowsSettable',
    'outlineSelectedRowsReadable','outlineSelectedRowMember','outlineSelectedRowsOwned','outlineRowsReadable','outlineRowMember','outlineRowsOwned'])
    if (value[key] !== undefined) { check(typeof value[key] === 'boolean', 'ax-evidence-refused'); clean[key]=value[key]; }
  return clean;
}
function safeInspectionDiagnostics(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return undefined;
  const keys = Object.keys(value).sort();
  if (keys.length !== 2 || keys[0] !== 'queryClass' || keys[1] !== 'stage') return undefined;
  const stages = new Set(['identity-precondition','request-precondition','main-children-query','main-children-value',
    'application-windows-query','application-windows-value','main-sheet-state','root-resolution','projection-budget',
    'ax-owner-query','ax-attribute-query','ax-children-query','ax-projection']);
  const queryClasses = new Set(['identity-check-failed','request-refused','attribute-query-failed','attribute-value-unreadable',
    'retained-sheet-present','attached-sheet-present','route-refused','budget-refused','ownership-refused','topology-refused']);
  const permitted = {
    'identity-precondition':['identity-check-failed'],'request-precondition':['request-refused'],
    'main-children-query':['attribute-query-failed'],'main-children-value':['attribute-value-unreadable'],
    'application-windows-query':['attribute-query-failed'],'application-windows-value':['attribute-value-unreadable'],
    'main-sheet-state':['retained-sheet-present','attached-sheet-present'],'root-resolution':['route-refused'],
    'projection-budget':['budget-refused'],'ax-owner-query':['ownership-refused'],
    'ax-attribute-query':['attribute-query-failed'],'ax-children-query':['attribute-query-failed','attribute-value-unreadable'],
    'ax-projection':['topology-refused']
  };
  if (!stages.has(value.stage) || !queryClasses.has(value.queryClass) || !permitted[value.stage].includes(value.queryClass)) return undefined;
  return {stage:value.stage,queryClass:value.queryClass};
}
const {safeActionDiagnostics,safeCleanupDiagnostics,cleanupFailureDiagnostics,closeOwnedHelpers,cleanupHelper}=require('./p38-helper-protocol.cjs');
const REFUSALS = new Set(['adapter-refused','sheet-owner','sheet-parent','sheet-replaced','child-owner','confirm-unsupported','children-unavailable','sheet-ambiguous','sheet-active','standalone-window']);
const STARTUP_DIAGNOSTIC_LIMIT = 16 * 1024;
function safeStartupDiagnostics(value) {
  const classes = ['rust-panic','tauri-startup-abort','p38-startup-refusal','unclassified-startup-abort','stderr-unavailable','diagnostic-capture-failed'];
  const states = ['starting','active','stopped','exited','failed','reconciliation-required'];
  const signals = ['SIGABRT','SIGSEGV','SIGILL','SIGBUS','SIGTRAP','SIGKILL','SIGTERM','SIGINT'];
  const manifestOutcomes = ['status-unavailable','manifest-pending','manifest-not-written','manifest-invalid-schema',
    'manifest-task-mismatch','manifest-present','manifest-error-other','manifest-unclassified'];
  if (!value || typeof value !== 'object') return undefined;
  const safe = { classification: classes.includes(value.classification) ? value.classification : 'diagnostic-capture-failed' };
  if (states.includes(value.runtimeState)) safe.runtimeState = value.runtimeState;
  if (signals.includes(value.signal)) safe.signal = value.signal;
  if (value.manifestOutcome !== undefined) safe.manifestOutcome = manifestOutcomes.includes(value.manifestOutcome) ? value.manifestOutcome : 'manifest-unclassified';
  if (Number.isSafeInteger(value.stderrBytes) && value.stderrBytes >= 0 && value.stderrBytes <= STARTUP_DIAGNOSTIC_LIMIT) safe.stderrBytes = value.stderrBytes;
  if (typeof value.stderrSha256 === 'string' && /^[a-f0-9]{64}$/.test(value.stderrSha256)) safe.stderrSha256 = value.stderrSha256;
  if (typeof value.truncated === 'boolean') safe.truncated = value.truncated;
  return safe;
}
function classifyStartupStderr(bytes) {
  const text = bytes.toString('utf8');
  if (/thread ['\"]main['\"] panicked at|panicked at /.test(text)) return 'rust-panic';
  if (text.includes('error while running tauri application')) return 'tauri-startup-abort';
  if (/P38 (?:fixture|probe|runtime|owner|checkpoint) [a-z -]+/.test(text)) return 'p38-startup-refusal';
  return bytes.length ? 'unclassified-startup-abort' : 'stderr-unavailable';
}
function classifyManifestOutcome(status) {
  if (!status) return 'status-unavailable';
  if (typeof status.appRuntimeError === 'string') {
    if (status.appRuntimeError === 'app did not write its runtime manifest') return 'manifest-not-written';
    if (status.appRuntimeError === 'app manifest is not a nemo.native-runtime/1 document') return 'manifest-invalid-schema';
    if (status.appRuntimeError.startsWith('app manifest belongs to task ')) return 'manifest-task-mismatch';
    return 'manifest-error-other';
  }
  if (status.appRuntime && typeof status.appRuntime === 'object') return 'manifest-present';
  if (['starting','active'].includes(status.state)) return 'manifest-pending';
  return 'manifest-unclassified';
}
async function captureStartupDiagnostics(task, isolation, runtime) {
  const deadline = Date.now() + 1200;
  let status = null;
  do {
    try { status = runtime.readNativeStatus(task); } catch { status = null; }
    if (status && (['stopped','exited','failed','reconciliation-required'].includes(status.state) ||
      typeof status.appRuntimeError === 'string')) break;
    await new Promise(resolve => setTimeout(resolve, 25));
  } while (Date.now() < deadline);
  let bytes = Buffer.alloc(0), truncated = false;
  try {
    const roots = isolation.taskRoots(task), root = roots.root, reports = roots.reports;
    const canonicalRoot = fs.realpathSync(root);
    for (const folder of [root, reports]) {
      const meta = fs.lstatSync(folder);
      check(meta.isDirectory() && !meta.isSymbolicLink(), 'startup-log-root-refused');
    }
    check(path.dirname(reports) === root && fs.realpathSync(reports) === path.join(canonicalRoot, 'reports'), 'startup-log-root-refused');
    const reportsIdentity = fs.lstatSync(reports), file = path.join(reports, 'native-app.stderr.log');
    const leaf = fs.lstatSync(file);
    check(leaf.isFile() && !leaf.isSymbolicLink(), 'startup-log-refused');
    const fd = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW | (fs.constants.O_NONBLOCK || 0));
    try {
      const meta = fs.fstatSync(fd), reportsAfter = fs.lstatSync(reports);
      check(meta.isFile() && meta.dev === leaf.dev && meta.ino === leaf.ino && reportsAfter.isDirectory() &&
        reportsAfter.dev === reportsIdentity.dev && reportsAfter.ino === reportsIdentity.ino &&
        fs.realpathSync(reports) === path.join(canonicalRoot, 'reports'), 'startup-log-refused');
      const length = Math.min(meta.size, STARTUP_DIAGNOSTIC_LIMIT); truncated = meta.size > length;
      bytes = Buffer.alloc(length);
      let offset = 0;
      while (offset < length) {
        const count = fs.readSync(fd, bytes, offset, length - offset, meta.size - length + offset);
        check(count > 0, 'startup-log-changed'); offset += count;
      }
    } finally { fs.closeSync(fd); }
  } catch { /* Unsafe or missing logs stay unavailable and never block cleanup. */ }
  return safeStartupDiagnostics({ classification: classifyStartupStderr(bytes),
    manifestOutcome: classifyManifestOutcome(status),
    ...(status && ['starting','active','stopped','exited','failed','reconciliation-required'].includes(status.state) ? { runtimeState: status.state } : {}),
    ...(status && typeof status.signal === 'string' ? { signal: status.signal } : {}), stderrBytes: bytes.length, stderrSha256: hash(bytes), truncated });
}
function actionTarget(action, sheet, reference = false) {
  const target = action.target || 'main'; check(['main','sheet'].includes(target), 'target-refused');
  const locationAction = ['go-to-folder','location-dismissed'].includes(action.type);
  check(reference || !locationAction && action.locationToken === undefined, 'action-refused');
  if (sheet.locationToken) check(target === 'sheet' && action.locationToken === sheet.locationToken &&
    ['inspect','press','set','location-dismissed'].includes(action.type), 'location-token-refused');
  else check(action.locationToken === undefined && action.type !== 'location-dismissed', 'location-token-refused');
  if (action.type === 'go-to-folder') {
    check(target === 'sheet' && !sheet.locationAttempted && !sheet.locationToken, 'location-state-refused');
    // No caller-supplied key/event/field data belongs to this fixed command.
    check(Object.keys(action).every(key=>['type','target','sheetToken'].includes(key)), 'action-refused');
  }
  if (target === 'sheet') check(sheet.token && action.sheetToken === sheet.token &&
    ['inspect','press','set','select',...(reference ? ['go-to-folder','location-dismissed'] : [])].includes(action.type), 'sheet-token-refused');
  else if (sheet.token) check(['pin-sheet','sheet-dismissed','capture'].includes(action.type), 'sheet-active');
  if (action.type === 'sheet-dismissed') check(sheet.token && action.sheetToken === sheet.token, 'sheet-token-refused');
  return target;
}
function sheetResult(action, sheet, result) {
  if (action.type === 'pin-sheet') {
    check(result.target === 'sheet' && typeof result.sheetToken === 'string' &&
      /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(result.sheetToken) &&
      (!sheet.token || sheet.token === result.sheetToken), 'sheet-token-refused');
    if (!sheet.token) sheet.locationAttempted = false;
    sheet.token = result.sheetToken;
  } else if (action.type === 'sheet-dismissed') { check(result.dismissed === true, 'sheet-active'); sheet.token = null; }
  else if (action.target === 'sheet') check(result.target === 'sheet' && result.sheetToken === sheet.token, 'sheet-token-refused');
  if (action.type === 'go-to-folder') {
    check(result.commandPosted === true && result.locationReady === true && typeof result.locationToken === 'string' &&
      /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(result.locationToken) &&
      result.locationToken !== sheet.token, 'location-token-refused');
    sheet.locationToken = result.locationToken;
  } else if (sheet.locationToken) {
    check(result.locationToken === sheet.locationToken, 'location-token-refused');
    if (action.type === 'location-dismissed') {
      check(result.locationDismissed === true, 'location-state-refused'); sheet.locationToken = null;
    }
  } else check(result.locationToken === undefined, 'location-token-refused');
}
async function transportReady(read, write, isTTY, nonce = crypto.randomUUID()) {
  check(isTTY === true, 'transport-pty-required');
  write({ schema:'nemo.p38-transport/1', status:'waiting', nonce, startAttempts:0, journalWrites:0 });
  const reply = await read(); check(reply && reply.transport === 'p38-ready' && reply.nonce === nonce, 'transport-handshake-refused');
  write({ schema:'nemo.p38-transport/1', status:'ready', startAttempts:0, journalWrites:0 });
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
function windowResult(info,pid,expected,allowInspectionDiagnostics=false) {
  if(info?.status==='refused')throw Object.assign(Error('owned-control-refused'),{code:REFUSALS.has(info.code)?info.code:'adapter-refused',
    ...(info.selectionDiagnostics ? {selectionDiagnostics:safeSelectionDiagnostics(info.selectionDiagnostics)} : {}),
    ...(info.actionDiagnostics ? {actionDiagnostics:safeActionDiagnostics(info.actionDiagnostics)} : {}),
    ...(allowInspectionDiagnostics && safeInspectionDiagnostics(info.inspectionDiagnostics) ?
      {inspectionDiagnostics:safeInspectionDiagnostics(info.inspectionDiagnostics)} : {})});
  check(info?.status===undefined||['ok','pinned'].includes(info.status),'owned-window-refused');
  return verifyWindow(info,pid,expected);
}
async function runOperator(config, services) { return runScopedOperator(config,services,null); }
async function runReferenceOperator(config, services) {
  return runScopedOperator(config,services,require('./reference-installed-policy.cjs'));
}
async function runScopedOperator(config, services, reference) {
  // Before this gate there is no package access, journal, process, AX or capture.
  (reference ? reference.authorize : authorize)(config);
  const receipt = { schema: reference ? 'nemo.reference-operator/1' : 'nemo.p38-operator/1', issue: reference ? 1864 : 1645, grant: config.grant.id,
    source: config.sourceHead, task: config.taskId, startAttempts: 0, appPid: null, windowId: null,
    actions: [], status: 'partial', cleanup: 'not-required', helperCleanup: 'not-required', installedAcceptance: 'not-established' };
  let controller, instance, app, timer, expire, failure = null, actionStartedAt = null, operatorWaitStartedAt = null; const sheet = { token:null }, runStarted = performance.now();
  const deadline = new Promise((_, reject) => { expire = () => reject(Object.assign(new Error('watchdog-timeout'), { code: 'watchdog-timeout' })); });
  const bounded = work => Promise.race([work, deadline]);
  const save = () => services.save(receipt);
  try {
    timer = services.setTimer(expire, config.timeoutMs); // Transport/EOF fails BEFORE journal/start.
    receipt.stage = 'transport';
    check(typeof services.transport === 'function', 'transport-required'); await bounded(services.transport());
    app = await services.package(config);
    receipt.stage = 'package';
    receipt.executableSha256 = app.executableSha256; receipt.buildProofSha256 = app.proofHash;
    if (reference) receipt.driver = app.driverProof;
    await services.journal(config); // Exclusive grant journal survives cleanup and prevents reuse.
    save();
    controller = services.controller(app, config);
    receipt.startAttempts = 1; save();
    receipt.stage = 'launch'; save();
    instance = await bounded(controller.start(config.taskId, ['desktop-input', 'gpu-reference']));
    receipt.stage = 'identity';
    const first = await bounded(controller.status(instance));
    services.validate(first.value, instance, app, config);
    receipt.appPid = verifyIdentity(first.value, instance, config); save(); // Persist before window/UI.
    receipt.windowId = windowResult(await bounded(services.window('discover', receipt.appPid)), receipt.appPid); save();
    for (;;) {
      check(receipt.actions.length < (reference ? 128 : 64), 'action-budget-exhausted');
      receipt.stage = 'awaiting-operator-input'; receipt.operatorWaitStartedElapsedMs = Math.round(performance.now() - runStarted); save();
      const waitStarted = performance.now(); operatorWaitStartedAt = waitStarted;
      const action = await bounded(services.next());
      const operatorWaitMs = Math.max(0, Math.round(performance.now() - waitStarted));
      operatorWaitStartedAt = null;
      receipt.operatorWaitMs = (receipt.operatorWaitMs || 0) + operatorWaitMs;
      check(action && ['inspect', 'press', 'set', 'select', 'raise', 'capture', 'pin-sheet', 'sheet-dismissed', 'stop', ...(reference ? ['artifacts','go-to-folder','location-dismissed'] : [])].includes(action.type), 'action-refused');
      if (action.type === 'stop') { check(!sheet.token, 'sheet-active'); receipt.stage = 'operator-stop'; break; }
      actionTarget(action, sheet, !!reference);
      if (action.type === 'go-to-folder') sheet.locationAttempted = true;
      const ordinal = receipt.actions.length + 1, actionStarted = performance.now(); actionStartedAt = actionStarted;
      receipt.inFlightAction = { type:action.type, ordinal };
      receipt.stage = 'window-'+action.type+'-'+(action.target||'main'); save();
      if (['press', 'set', 'select'].includes(action.type)) check(typeof action.element === 'string' && /^\d+(?:\.\d+)*$/.test(action.element) && action.element.length<=128, 'element-refused');
      if (action.type === 'inspect') {
        check(action.element === undefined || typeof action.element === 'string' && /^(?:\d+(?:\.\d+)*)?$/.test(action.element) && action.element.length<=128, 'element-refused');
        if (action.depth !== undefined || action.maxNodes !== undefined) check(['main','sheet'].includes(action.target || 'main') && Number.isInteger(action.depth) && action.depth >= 1 && action.depth <= 5 && Number.isInteger(action.maxNodes) && action.maxNodes >= 1 && action.maxNodes <= 128, 'inspect-budget-refused');
      }
      if (action.type === 'set') {
        if (reference) reference.verifySet(config,action);
        else check(action.target==='sheet' && typeof action.value === 'string' && action.value.length <= 4096 &&
          config.fixtures?.some(item=>action.value===item.path), 'fixture-refused');
      }
      const status = await bounded(controller.status(instance));
      services.validate(status.value, instance, app, config);
      check(verifyIdentity(status.value, instance, config) === receipt.appPid, 'native-pid-changed');
      // A sheet is a separate target under the SAME immutable main, never rediscovery.
      windowResult(await bounded(services.window('check', receipt.appPid, receipt.windowId)), receipt.appPid, receipt.windowId);
      if (action.type === 'artifacts') {
        check(!sheet.token, 'sheet-active'); receipt.artifacts=reference.artifacts(config);
        receipt.actions.push({type:'artifacts',ordinal,durationMs:Math.max(0,Math.round(performance.now()-actionStarted)),operatorWaitMs});
        delete receipt.inFlightAction; actionStartedAt=null; save(); continue;
      }
      const result = await bounded(services.window(action.type, receipt.appPid, receipt.windowId, action));
      windowResult(result, receipt.appPid, receipt.windowId, action.type==='inspect' && (action.target||'main')==='main');
      if(action.type==='set'){
        check(result.actionDiagnostics,'value-refused'); const text=safeActionDiagnostics(result.actionDiagnostics);
        check(text.stage==='axtext-confirm' && text.role==='AXTextField' && text.textFieldKind==='plain' && text.enabled===true &&
          text.valueSettableQueryError===0 && text.valueSettable===true && text.valueWriteError===0 && text.valueReadbackError===0 &&
          text.valueMatches===true && text.confirmQueryError===0 && text.confirmAdvertised===true && text.confirmError===0,'value-refused');
      }
      if(action.type==='go-to-folder' && result.actionDiagnostics)
        check(safeActionDiagnostics(result.actionDiagnostics).stage==='goto-ready','location-state-refused');
      sheetResult(action, sheet, result);
      if(action.type==='pin-sheet' || action.type==='go-to-folder'){
        check(typeof services.feedback==='function','sheet-feedback-required');
        await bounded(services.feedback({target:'sheet',ordinal:receipt.actions.length+1,token:sheet.token,
          ...(sheet.locationToken ? {locationToken:sheet.locationToken} : {})}));
      }
      if (result.captureSha256) check(/^[a-f0-9]{64}$/.test(result.captureSha256), 'capture-proof-refused');
      const completed = { type:action.type, ordinal, durationMs:Math.max(0,Math.round(performance.now()-actionStarted)), operatorWaitMs };
      for (const key of ['commandPosted','locationReady','locationDismissed']) if (typeof result[key] === 'boolean') completed[key] = result[key];
      if (result.captureSha256) completed.captureSha256 = result.captureSha256;
      for (const key of ['selected','enabled','truncated']) if (typeof result[key] === 'boolean') completed[key] = result[key];
      if (result.selectionDiagnostics && action.type === 'select') completed.selectionDiagnostics = safeSelectionDiagnostics(result.selectionDiagnostics);
      if (result.actionDiagnostics && ['press','set','go-to-folder'].includes(action.type)) completed.actionDiagnostics = safeActionDiagnostics(result.actionDiagnostics);
      receipt.actions.push(completed); delete receipt.inFlightAction; actionStartedAt = null;
      save();
    }
    receipt.status = 'operator-complete'; // No claim that tab/alert/native evidence passes.
  } catch (error) { failure = error;
    if (operatorWaitStartedAt !== null) { receipt.operatorWaitTimedOutMs = Math.max(0,Math.round(performance.now()-operatorWaitStartedAt)); receipt.operatorWaitMs = (receipt.operatorWaitMs || 0) + receipt.operatorWaitTimedOutMs; }
    receipt.reason = ['watchdog-timeout','native-identity-refused','flat-app-pid-required','owned-window-refused',
      'native-pid-changed','action-refused','element-refused','value-refused','capture-proof-refused','action-budget-exhausted',
      'source-head-refused','fresh-build-proof-required','window-helper-refused','grant-already-attempted',
      'transport-pty-required','transport-handshake-refused','transport-required','operator-input-ended','helper-input-ended','inspect-budget-refused',
      'helper-malformed','fixture-refused','sheet-token-refused','sheet-feedback-required','sheet-active','target-refused',
      'location-token-refused','location-state-refused',...REFUSALS,...(reference ? reference.errors : [])].includes(error.code) ? error.code : 'operator-failed';
    receipt.firstFailure = Object.freeze({ code: receipt.reason, stage: receipt.stage || 'preflight', elapsedMs:Math.max(0,Math.round(performance.now()-runStarted)),
      ...(safeStartupDiagnostics(error.startupDiagnostics) ? { startupDiagnostics: safeStartupDiagnostics(error.startupDiagnostics) } : {}),
      ...(error.selectionDiagnostics ? { selectionDiagnostics:safeSelectionDiagnostics(error.selectionDiagnostics) } : {}),
      ...(error.actionDiagnostics ? { actionDiagnostics:safeActionDiagnostics(error.actionDiagnostics) } : {}),
      ...(safeInspectionDiagnostics(error.inspectionDiagnostics) ? {inspectionDiagnostics:safeInspectionDiagnostics(error.inspectionDiagnostics)} : {}),
      ...(actionStartedAt !== null ? { action:{...receipt.inFlightAction,durationMs:Math.max(0,Math.round(performance.now()-actionStartedAt))} } : {}) });
    receipt.status = 'partial'; save(); // Persist the first safe refusal before cleanup can fail.
  }
  finally {
    if (timer !== undefined) services.clearTimer(timer);
    try {
      const cleanup = services.close ? await services.close() : null;
      if (cleanup?.cleanupDiagnostics) receipt.helperCleanupDiagnostics = safeCleanupDiagnostics(cleanup.cleanupDiagnostics);
      receipt.helperCleanup = 'complete';
    }
    catch (error) { failure = true; receipt.helperCleanup = 'incomplete';
      if (error.cleanupDiagnostics) receipt.helperCleanupDiagnostics = safeCleanupDiagnostics(error.cleanupDiagnostics);
      if (!receipt.firstFailure) { receipt.reason = 'helper-cleanup-incomplete';
        receipt.firstFailure = Object.freeze({ code: receipt.reason, stage: 'helper-cleanup' }); }
    }
    if (controller) {
      try { await controller.cleanup(); receipt.cleanup = 'complete'; } // Owner cleanup is independent of helper cleanup.
      catch (_) { failure = true; receipt.cleanup = 'incomplete';
        if (!receipt.firstFailure) { receipt.reason = 'cleanup-incomplete';
          receipt.firstFailure = Object.freeze({ code: receipt.reason, stage: 'owner-cleanup' }); }
      }
    }
    if (failure) receipt.status = 'partial';
    save();
  }
  return Object.freeze(receipt);
}
function verifyFixtures(config) {
    check(Array.isArray(config.fixtures) && config.fixtures.length === 2, 'fixture-refused');
    for (const label of ['A','B']) {
      const item = config.fixtures.find(item => item.label === label);
      check(item && typeof item.path === 'string' && path.basename(item.path) === label+'.json' &&
        /^[a-f0-9]{64}$/.test(item.sha256), 'fixture-refused');
      let fd;
      try {
        fd=fs.openSync(item.path,fs.constants.O_RDONLY|fs.constants.O_NOFOLLOW);
        const stat=fs.fstatSync(fd);check(stat.isFile()&&stat.size<=1048576,'fixture-refused');
        const bytes=fs.readFileSync(fd);JSON.parse(bytes);check(hash(bytes)===item.sha256,'fixture-refused');
      } catch (_) { throw Object.assign(Error('fixture-refused'),{code:'fixture-refused'}); }
      finally {if(fd!==undefined)fs.closeSync(fd);}
    }
  }

function createWindowTransport(config,directory,getGuard,helpers,helperTimeoutMs=10000) {
  const reference=config.workflow==='P37_REFERENCE' ? require('./reference-installed-policy.cjs') : null;
  if(reference)reference.authorize(config);
  check(Number.isInteger(helperTimeoutMs) && helperTimeoutMs >= 1 && helperTimeoutMs <= 10000, 'helper-timeout-refused');
  let channel=null, ordinal=0;
  async function helper(mode, pid, windowId, action = {}) {
    const reportGuard = getGuard(); reportGuard.verify();
    if (!channel) {
      check(mode === 'discover', 'owned-window-refused');
      const scopedEnv=reference ? {NEMO_REFERENCE_GRANT:config.grant.id,NEMO_REFERENCE_REPORT_ROOT:reportGuard.directory} :
        {NEMO_P38_GRANT:config.grant.id,NEMO_P38_REPORT_ROOT:reportGuard.directory};
      const environment={...process.env};
      for(const key of ['NEMO_P38_GRANT','NEMO_P38_REPORT_ROOT','NEMO_REFERENCE_GRANT','NEMO_REFERENCE_REPORT_ROOT'])delete environment[key];
      const child = spawn(config.windowHelper, [reference ? '--reference-persistent' : '--persistent', String(pid)], { detached:true, stdio:['pipe','pipe','pipe'],
        env:{...environment,...scopedEnv} });
      child.p38DetachedGroup=true;
      child.p38GroupMayContainDescendants=false;
      helpers.add(child); child.on('error', () => {}); child.stdin.on('error', () => {}); child.stderr.resume();
      child.once('close', () => {
        if(!child.p38GroupMayContainDescendants){helpers.delete(child);return;}
        try { process.kill(-child.pid,0); } catch(error) { if(error.code==='ESRCH')helpers.delete(child); }
      });
      const lines = readline.createInterface({input:child.stdout,terminal:false})[Symbol.asyncIterator]();
      channel = {child,lines,pid,windowId:null}; // Track BEFORE the first readiness read.
      const pinned = await reply(); check(pinned.status==='pinned','owned-window-refused'); verifyWindow(pinned,pid);
      channel.windowId = pinned.windowId; return pinned;
    }
    check(mode !== 'discover' && channel.pid===pid && channel.windowId===windowId, 'owned-window-refused');
    (reference ? reference.verifyInputs : verifyFixtures)(config);
    const request = { ...action, type:mode, pid, windowId };
    const name = 'capture-' + ++ordinal + '.png', output = path.join(directory,name);
    // A refused press can run the bounded CoreGraphics sampler in a child.
    // Mark the process group before writing so operator timeout/early helper
    // exit still leaves the helper tracked for group cleanup.
    if(mode==='press')channel.child.p38GroupMayContainDescendants=true;
    if(mode==='capture'){request.capture=name;channel.child.p38GroupMayContainDescendants=true;}
    if(mode==='set' && request.target==='sheet') {
      if(reference)reference.verifySet(config,request);
      else check(config.fixtures.some(f=>request.value===f.path),'fixture-refused');
    }
    channel.child.stdin.write(JSON.stringify(request)+'\n');
    const result = await reply(mode==='inspect' && (action.target||'main')==='main');
    reportGuard.verify(); verifyWindow(result,pid,windowId);
    check(result.type===mode && result.status==='ok','owned-window-refused');
    if(mode==='capture'){check(result.capture===name,'capture-proof-refused');result.captureSha256=hash(fs.readFileSync(output));}
    if(mode==='inspect')atomicJSON(reportGuard,'ax-'+ordinal+'.json',safeWindowEvidence(result,!!reference));
    return result;
  }
  async function reply(allowInspectionDiagnostics=false) {
    let timer;
    try {
      const line = await Promise.race([channel.lines.next(),new Promise((_,reject)=>{
        timer=setTimeout(()=>reject(Object.assign(Error('helper-input-ended'),{code:'helper-input-ended'})),helperTimeoutMs);})]);
      check(!line.done && line.value.length<=1024*1024,'helper-input-ended');
      let value;try{value=JSON.parse(line.value);}catch{throw Object.assign(Error('helper-malformed'),{code:'helper-malformed'});}
      check(value&&typeof value==='object'&&!Array.isArray(value),'helper-malformed');
      if(value.status==='refused')throw Object.assign(Error('owned-control-refused'),{code:REFUSALS.has(value.code)?value.code:'adapter-refused',
        ...(value.selectionDiagnostics ? {selectionDiagnostics:safeSelectionDiagnostics(value.selectionDiagnostics)} : {}),
        ...(value.actionDiagnostics ? {actionDiagnostics:safeActionDiagnostics(value.actionDiagnostics)} : {}),
        ...(allowInspectionDiagnostics && safeInspectionDiagnostics(value.inspectionDiagnostics) ?
          {inspectionDiagnostics:safeInspectionDiagnostics(value.inspectionDiagnostics)} : {})});
      return value;
    } finally {clearTimeout(timer);}
  }
  return helper;
}
function defaultServices(config) {
  const harness = require('./native-harness.cjs');
  const runtime = require('../../scripts/nemo/lib/native-runtime.cjs');
  const isolation = require('../../scripts/nemo/lib/isolation.cjs');
  const identity = require('../../scripts/nemo/lib/identity.cjs');
  const directory = path.join(ROOT, 'reports', 'p38-installed-operators', hash(config.grant.id));
  let ready = false, reportGuard; const helpers = new Set();
  const input = readline.createInterface({ input: process.stdin });
  const iterator = input[Symbol.asyncIterator]();
  const helper = createWindowTransport(config,directory,()=>reportGuard,helpers);
  async function inputJSON() {
    const next = await iterator.next(); check(!next.done && next.value.length <= 8192, 'operator-input-ended'); return JSON.parse(next.value);
  }

  return {
    transport() { return transportReady(inputJSON, value => process.stdout.write(JSON.stringify(value)+'\n'), process.stdin.isTTY); },
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
        helperProof.sourceSha256 === require('./p38-owned-window-build.cjs').verifySourceProof(helperProof) &&
        helperProof.executableSha256 === config.helperSha256, 'window-helper-refused');
      verifyFixtures(config); return { ...app, source, build: identity.buildIdentity(), proofHash: hash(fs.readFileSync(config.buildProof)) };
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
    controller(app) { return harness.createController({ root: ROOT, app: app.bundle,
      startupDiagnostics: task => captureStartupDiagnostics(task, isolation, runtime) }); },
    validate(snapshot, instance, app) { verifyFixtures(config); harness.validateSnapshot(snapshot, instance, { isolation, runtime, source: app.source, build: app.build, app }); },
    window: helper,
    next: inputJSON,
    feedback(value){process.stdout.write(JSON.stringify(value)+'\n');},
    async close() {
      return closeOwnedHelpers(()=>input.close(),[...helpers],cleanupHelper);
    },
    setTimer: setTimeout, clearTimer: clearTimeout,
  };
}
async function main() {
  if(process.argv.length===3&&process.argv[2]==='--transport-probe'){
    const input=readline.createInterface({input:process.stdin,terminal:false}),lines=input[Symbol.asyncIterator]();let timer;
    try{await Promise.race([transportReady(async()=>{const line=await lines.next();check(!line.done&&line.value.length<=8192,'operator-input-ended');return JSON.parse(line.value);},
      value=>process.stdout.write(JSON.stringify(value)+'\n'),process.stdin.isTTY),new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('transport timeout')),15000);})]);}
    finally{clearTimeout(timer);input.close();}return;
  }
  const config = readJSON(process.argv[2], 16384);
  authorize(config);
  const result = await runOperator(config, defaultServices(config));
  process.stdout.write(JSON.stringify(result) + '\n');
  process.exitCode = result.cleanup === 'incomplete' || result.status === 'partial' ? 1 : 0;
}
if (require.main === module) main().catch(() => { process.stderr.write('P38 operator refused; no installed acceptance.\n'); process.exitCode = 1; });
module.exports = { authorize, verifyIdentity, verifyWindow, runOperator, runReferenceOperator, safeDirectory, atomicJSON, safeWindowEvidence, safeSelectionDiagnostics, safeInspectionDiagnostics, safeActionDiagnostics, safeCleanupDiagnostics, safeStartupDiagnostics, classifyStartupStderr, classifyManifestOutcome, captureStartupDiagnostics, cleanupFailureDiagnostics, closeOwnedHelpers, transportReady, actionTarget, sheetResult, verifyFixtures, cleanupHelper, createWindowTransport };
