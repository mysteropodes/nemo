(function () {
  'use strict';
  const key = '__NEMO_P38_FIXTURE_OPEN__';
  const config = window[key];
  const HASH = Object.freeze({
    A: 'e1bc63b0e284f521e170d8236296392bdcf19d4b1f13b4e04683727844fa71a0',
    B: 'a438bda3b5c25ec3e8797d1c95fb8ade1da7b0bda675c2279499c659ece46bf6'
  });
  const ID = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;
  const NATIVE_ROUTE = 'nemo_native_object_dispatch';
  function valid() {
    return config && Object.keys(config).sort().join(',') ===
      'buildSha,fixtures,manifestSha256,pid,runId,schema,sourceSha,taskKey' &&
      config.schema === 'nemo.p38-fixture-open-bootstrap/1' &&
      /^[a-f0-9-]{36}$/.test(config.runId) && /^[a-f0-9]{64}$/.test(config.taskKey) &&
      /^[a-f0-9]{40}$/.test(config.sourceSha) && config.buildSha === config.sourceSha &&
      /^[a-f0-9]{64}$/.test(config.manifestSha256) && Number.isSafeInteger(config.pid) && config.pid > 0 &&
      config.fixtures && Object.keys(config.fixtures).sort().join(',') === 'A,B' &&
      typeof config.fixtures.A === 'string' && config.fixtures.A.endsWith('/A.json') &&
      typeof config.fixtures.B === 'string' && config.fixtures.B.endsWith('/B.json');
  }
  if (!valid() || window.__NEMO_P38_FIXTURE_OPEN_STARTED__) return;
  Object.freeze(config.fixtures);
  Object.freeze(config);
  window.__NEMO_P38_FIXTURE_OPEN_STARTED__ = true;
  const readinessFailure = Object.freeze({
    openPathUnavailable: Symbol('open-path-unavailable'),
    invokeUnavailable: Symbol('tauri-invoke-unavailable'),
  });
  function invoke(command, args) {
    const core = window.__TAURI__ && window.__TAURI__.core;
    const original = core && core.invoke;
    if (typeof original !== 'function') throw new Error('invoke-unavailable');
    return arguments.length > 1
      ? Reflect.apply(original, core, [command, args])
      : Reflect.apply(original, core, [command]);
  }
  let activeOpen = null;
  const OPEN_STAGES = ['receipt','installed-status','metadata','first-preview','first-owner','reveal',
    'second-preview','second-owner','ui-commit','publication','subscriber'].concat(['first-preview','second-preview'].flatMap(prefix=>
      ['frame-range','mapping','viewport','owner','receipt','status','work-id','view-generation','receipt-access',
        'status-stale-discarded','status-deferred-timeout','status-deferred-occluded','status-failed-validation',
        'status-failed-device-lost','status-failed-retry-lost','status-failed-retry-outdated','status-failed-recreate','status-failed-reconfigure']
        .map(detail=>prefix+'-'+detail)));
  function postBootstrapCode(code) {
    if(typeof code!=='string')return false;
    const parts=code.split(':');
    return parts.length===4&&OPEN_STAGES.some(stage=>parts[0]==='post-bootstrap-'+stage)&&
      ['invoke-rejected','local-refusal'].includes(parts[1])&&
      ['invalid_request','unavailable','unsupported_format','replacement_refused','stale_revision','stale_owner','cancelled','wrong_instance','wrong_document','cancelled_before_dispatch','unknown'].includes(parts[2])&&
      ['notDispatched','other'].includes(parts[3]);
  }
  function diagnostic(event, code) {
    if (!activeOpen) return;
    if (event === 'allow-open-refused') activeOpen.gateRefused = true;
    else if (event === 'bootstrap-dispatched') activeOpen.dispatches++;
    else if (event === 'bootstrap-resolved') activeOpen.resolved++;
    else if (event === 'bootstrap-rejected' && (code === 'unsupported-format' || code === 'other'))
      activeOpen.rejected = code;
    else if(event==='post-bootstrap-failed'&&!activeOpen.postBootstrap&&postBootstrapCode(code))activeOpen.postBootstrap=code;
  }
  window.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__ = diagnostic;
  function openFailureCode(state, fixtureId, threw) {
    const prefix = fixtureId.toLowerCase();
    if (state && state.dispatches === 1 && state.rejected === 'unsupported-format')
      return prefix + '-native-bootstrap-unsupported-format';
    if (state && state.dispatches === 1 && state.rejected === 'other')
      return prefix + '-native-bootstrap-rejected';
    if (state && state.dispatches === 1 && state.resolved === 1)
      return state.postBootstrap?prefix+'-'+state.postBootstrap:prefix + '-native-bootstrap-resolved-open-refused';
    if (state && state.dispatches === 0)
      return state.gateRefused ? prefix + '-pre-native-refusal' :
        threw ? prefix + '-open-threw' : prefix + '-native-status-empty-no-bootstrap';
    return prefix + (threw ? '-open-threw' : '-open-false');
  }
  async function fail(phase, fixtureId, code) {
    const core = window.__TAURI__ && window.__TAURI__.core;
    if (!core || typeof core.invoke !== 'function') return;
    const payload = {runId:config.runId,taskKey:config.taskKey,sourceSha:config.sourceSha,
      buildSha:config.buildSha,manifestSha256:config.manifestSha256,phase,status:'failed',
      fixtureId,fixtureSha256:fixtureId?HASH[fixtureId]:undefined,failureCode:code};
    await invoke('nemo_test_p38_fixture_open_probe', {payloadJson:JSON.stringify(payload)});
  }
  function checkpoint(phase, status, fixtureId) {
    const payload = {runId:config.runId,taskKey:config.taskKey,sourceSha:config.sourceSha,
      buildSha:config.buildSha,manifestSha256:config.manifestSha256,phase,status,
      fixtureId,fixtureSha256:fixtureId?HASH[fixtureId]:undefined};
    return invoke('nemo_test_p38_fixture_open_probe', {payloadJson:JSON.stringify(payload)});
  }
  async function readNativeStatus() {
    return await invoke('nemo_native_object_client_status');
  }
  function validStatusBase(value) {
    try {
      return !!value && typeof value === 'object' && value.apiVersion === 2 &&
        typeof value.instanceId === 'string' && ID.test(value.instanceId) &&
        value.route === NATIVE_ROUTE && typeof value.available === 'boolean';
    } catch (_) { return false; }
  }
  function validEmptyStatus(value) {
    try {
      return validStatusBase(value) && value.available === false && value.documentId === null &&
        value.contentRevision === null && value.lifecycleGeneration === null;
    } catch (_) { return false; }
  }
  function validActiveStatus(value, instanceId, documentId, generation) {
    try {
      return validStatusBase(value) && value.available === true && value.instanceId === instanceId &&
        typeof value.documentId === 'string' && ID.test(value.documentId) && (!documentId || value.documentId === documentId) &&
        value.contentRevision === 0 && Number.isSafeInteger(value.lifecycleGeneration) &&
        value.lifecycleGeneration >= 1 && (!generation || value.lifecycleGeneration === generation);
    } catch (_) { return false; }
  }
  function sameActiveStatus(left, right) {
    try {
      return validActiveStatus(left, left.instanceId, left.documentId, left.lifecycleGeneration) &&
        validActiveStatus(right, left.instanceId, left.documentId, left.lifecycleGeneration);
    } catch (_) { return false; }
  }
  async function waitForOpenPath() {
    const deadline = Date.now() + 60000;
    while (Date.now() < deadline) {
      const core = window.__TAURI__ && window.__TAURI__.core;
      if (window.SMProject && typeof window.SMProject.openPath === 'function' &&
          core && typeof core.invoke === 'function') return;
      await new Promise(resolve => setTimeout(resolve, 50));
    }
    throw typeof (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke) === 'function'
      ? readinessFailure.openPathUnavailable : readinessFailure.invokeUnavailable;
  }
  async function preflightARead() {
    let fsBridge, read;
    try {
      fsBridge = window.__TAURI__ && window.__TAURI__.fs;
      read = fsBridge && fsBridge.readTextFile;
    } catch (_) { /* An unreadable API is a fixed pre-native refusal. */ }
    if (typeof read !== 'function') {
      await fail('a-opened', 'A', 'a-pre-native-refusal');
      return false;
    }
    let text;
    try { text = await Reflect.apply(read, fsBridge, [config.fixtures.A]); }
    catch (_) {
      await fail('a-opened', 'A', 'a-fs-read-rejected');
      return false;
    }
    if (typeof text !== 'string' || text.length === 0 || text.length > 1048576) {
      await fail('a-opened', 'A', 'a-fs-read-rejected');
      return false;
    }
    return true;
  }
  async function openA() {
    let before;
    try { before = await readNativeStatus(); }
    catch (_) {
      await fail('a-opened', 'A', 'a-pre-native-refusal');
      throw new Error('open-refused');
    }
    if (!validEmptyStatus(before)) {
      await fail('a-opened', 'A', 'a-pre-native-refusal');
      throw new Error('open-refused');
    }
    let result, openState = {gateRefused:false,dispatches:0,resolved:0,rejected:null};
    activeOpen = openState;
    try { result = await window.SMProject.openPath(config.fixtures.A); }
    catch (_) { activeOpen = null; await fail('a-opened', 'A', openFailureCode(openState,'A',true)); throw new Error('open-refused'); }
    activeOpen = null;
    if (result !== true) {
      const failure = openState.dispatches > 0 || result === false ? openFailureCode(openState,'A',false) : 'a-open-unaccepted';
      await fail('a-opened', 'A', failure);
      throw new Error('open-refused');
    }
    let after;
    try { after = await readNativeStatus(); }
    catch (_) { after = null; }
    if (!validActiveStatus(after, before.instanceId, null, 1)) {
      await fail('a-opened', 'A', 'a-open-unaccepted');
      throw new Error('open-refused');
    }
    return after;
  }
  function validId(value) { return typeof value === 'string' && ID.test(value); }
  const PIN_FIELDS = ['apiVersion','instanceId','documentId','contentRevision','documentSnapshotId',
    'lifecycleGeneration','documentDigest','undoDepth','redoDepth'];
  let serial = 0;
  function rows() {
    const bar = window.document.getElementById('project-tabs-bar');
    if (!bar) throw new Error('tabs-unavailable');
    const all = Array.from(bar.querySelectorAll('.project-tab'));
    const named = name => all.filter(row => row.querySelector('.pt-name')?.textContent === name);
    return {bar,all,A:named('A'),B:named('B')};
  }
  function validRows(value, withB, aId, bId) {
    return value.all.length === (withB ? 2 : 1) && value.A.length === 1 &&
      value.B.length === (withB ? 1 : 0) && value.A[0].classList.contains('act') &&
      value.A[0].dataset.tab === aId && (!withB || value.B[0].dataset.tab === bId &&
      bId !== aId && !value.B[0].classList.contains('act'));
  }
  async function aBaseline(status) {
    const session = window.NemoNativeObjectProject.desktop(window);
    const info = session.info(), presented = session.readPresentedObjectSession(), current = await readNativeStatus();
    if (!session.isActive() || !presented || !sameActiveStatus(status,current) ||
        !Array.isArray(current.operations) || !['fill.set','undo','redo'].every(action =>
          current.operations.includes('command.document.object.' + action))) throw new Error('A-unavailable');
    const request = {apiVersion:2,requestId:'p38-inspect-' + (++serial),instanceId:status.instanceId,
      documentId:status.documentId,action:{kind:'inspect'}};
    const native = await invoke('nemo_native_object_reference', {requestJson:JSON.stringify(request)});
    if (!native || native.requestId !== request.requestId || !PIN_FIELDS.every(field => native[field] === info[field]) ||
        info.instanceId !== status.instanceId || info.documentId !== status.documentId ||
        info.contentRevision !== status.contentRevision || info.lifecycleGeneration !== status.lifecycleGeneration ||
        !validId(info.documentSnapshotId) || !/^[a-f0-9]{64}$/.test(info.documentDigest) ||
        !Number.isSafeInteger(info.undoDepth) || info.undoDepth < 0 ||
        !Number.isSafeInteger(info.redoDepth) || info.redoDepth < 0 ||
        presented.presentation?.status !== 'presented' || presented.presentation.frame !== window.state.currentFrame ||
        !PIN_FIELDS.slice(1,6).every(field => presented[field] === info[field] && presented.presentation[field] === info[field])) throw new Error('A-unaccepted');
    const pins = Object.fromEntries(PIN_FIELDS.map(field => [field,info[field]]));
    const label = window.SMProject.getCurrentLabel(), dirty = window.SMProject.isDirty();
    if (typeof label !== 'string' || !label.endsWith(' — ' + config.fixtures.A) || typeof dirty !== 'boolean')
      throw new Error('A-ui-unavailable');
    return JSON.stringify({pins,label,dirty,frame:window.state.currentFrame});
  }
  async function sameA(previous, baseline) {
    await Promise.resolve();
    try { return await aBaseline(previous) === baseline; }
    catch (_) { return false; }
  }
  async function inactive(previous, token) {
    const info = window.NemoNativeObjectProject.desktop(window).info();
    const request = {apiVersion:2,requestId:'p38-inactive-' + (++serial),instanceId:previous.instanceId,
      documentId:previous.documentId,expectedRevision:previous.contentRevision,
      documentSnapshotId:info.documentSnapshotId,lifecycleGeneration:previous.lifecycleGeneration,
      action:token ? {kind:'inspect',preparationToken:token} : {kind:'inspect'}};
    const value = await invoke('nemo_native_object_inactive_project', {requestJson:JSON.stringify(request)});
    if (!value || value.apiVersion !== 2 || value.requestId !== request.requestId ||
        value.instanceId !== request.instanceId || value.documentId !== request.documentId ||
        value.contentRevision !== request.expectedRevision || value.documentSnapshotId !== request.documentSnapshotId ||
        value.lifecycleGeneration !== request.lifecycleGeneration || value.retryExecution !== false)
      throw new Error('inactive-uncorrelated');
    return value;
  }
  async function waitUntil(check) {
    const deadline = Date.now() + 5000;
    do {
      try { if (await check()) return true; } catch (_) { /* Refuse an uncorrelated observation. */ }
      await new Promise(resolve => setTimeout(resolve,25));
    } while (Date.now() < deadline);
    return false;
  }
  function switchAlert(value) {
    return Array.from(value.bar.querySelectorAll('[role="alert"]')).some(node =>
      node.textContent === 'Switching project tabs is unavailable in this build.' &&
      window.getComputedStyle(node).display !== 'none' && window.getComputedStyle(node).visibility !== 'hidden' &&
      node.getClientRects().length > 0);
  }
  async function rejectB(phase, code) {
    await fail(phase,'B',code);
    throw new Error('B-refused');
  }
  async function openB(previous, baseline, aId) {
    if (!await sameA(previous,baseline) || !validRows(rows(),false,aId))
      return rejectB('b-opened','b-pre-native-refusal');
    let result;
    try { result = await window.SMProject.openPath(config.fixtures.B); }
    catch (_) { return rejectB('b-opened','b-open-threw'); }
    const meta = result && result.meta, info = window.NemoNativeObjectProject.desktop(window).info();
    if (result?.status !== 'prepared' || !meta || meta.kind !== 'native-object-inactive' ||
        meta.path !== config.fixtures.B || meta.name !== 'B' || !validId(meta.preparationToken) ||
        !validId(meta.preparationRequestId) || meta.instanceId !== previous.instanceId ||
        meta.ownerDocumentId !== previous.documentId || meta.lifecycleGeneration !== previous.lifecycleGeneration ||
        meta.originSnapshotId !== info.documentSnapshotId || meta.originRevision !== previous.contentRevision ||
        !validId(meta.documentSnapshotId) || meta.documentSnapshotId === info.documentSnapshotId ||
        meta.contentRevision !== 0 || !/^[a-f0-9]{64}$/.test(meta.digest))
      return rejectB('b-opened',result === false ? 'b-open-false' : 'b-open-unaccepted');
    const preparedRows = rows(), bId = preparedRows.B[0]?.dataset.tab;
    if (!bId || !validRows(preparedRows,true,aId,bId)) return rejectB('b-opened','b-open-unaccepted');
    try { preparedRows.B[0].click(); }
    catch (_) { return rejectB('b-opened','b-switch-unaccepted'); }
    const retained = await waitUntil(async () => {
      const value = await inactive(previous,meta.preparationToken);
      return value.state === 'inactive-retained' && value.preparationToken === meta.preparationToken &&
        value.project?.preparationToken === meta.preparationToken &&
        value.project.preparationRequestId === meta.preparationRequestId &&
        value.project.documentSnapshotId === meta.documentSnapshotId && value.project.documentDigest === meta.digest &&
        value.project.origin?.documentId === previous.documentId &&
        value.project.origin.documentSnapshotId === info.documentSnapshotId &&
        value.project.origin.contentRevision === previous.contentRevision &&
        validRows(rows(),true,aId,bId) && switchAlert(rows());
    });
    if (!await sameA(previous,baseline)) return rejectB('b-opened','b-a-state-changed');
    if (!retained) return rejectB('b-opened','b-switch-unaccepted');
    await checkpoint('b-opened','returned','B');
    // The accepted owner ACK is the only admission to the real close control.
    if (!await sameA(previous,baseline) || !validRows(rows(),true,aId,bId) || !switchAlert(rows()))
      return rejectB('b-closed','b-close-precondition-refused');
    const close = rows().B[0].querySelector('.pt-close');
    if (!close) return rejectB('b-closed','b-close-precondition-refused');
    try { close.click(); }
    catch (_) { return rejectB('b-closed','b-close-threw'); }
    const closed = await waitUntil(async () => {
      const value = await inactive(previous,null);
      return value.state === 'inactive-absent' && value.preparationToken === null && value.project === null &&
        validRows(rows(),false,aId);
    });
    if (!closed) return rejectB('b-closed','b-close-unaccepted');
    if (!await sameA(previous,baseline)) return rejectB('b-closed','b-close-a-state-changed');
    await checkpoint('b-closed','returned','B');
  }
  window.__NEMO_P38_FIXTURE_OPEN_DONE__ = (async () => {
    try {
      try { await waitForOpenPath(); }
      catch (cause) {
        // Cursor zero is the only place a pre-ready refusal can be recorded.
        // If invoke itself is absent there is no trusted channel to persist it.
        const core = window.__TAURI__ && window.__TAURI__.core;
        const failureCode = cause === readinessFailure.openPathUnavailable ? 'ready-open-path-unavailable' : null;
        if (failureCode && core && typeof core.invoke === 'function') {
          try { await fail('ready', undefined, failureCode); } catch (_) {}
        }
        return 'refused';
      }
      const ready = await checkpoint('ready', 'ready');
      if (!ready || typeof ready !== 'object' || Object.keys(ready).join(',') !== 'ownerAcknowledged' ||
          ready.ownerAcknowledged !== true) {
        await fail('a-opened', 'A', 'a-pre-native-refusal');
        return 'refused';
      }
      if (!await preflightARead()) return 'refused';
      const stateA = await openA();
      let baseline, aId;
      try {
        baseline = await aBaseline(stateA);
        aId = rows().A[0]?.dataset.tab;
        if (!aId || !validRows(rows(),false,aId)) throw new Error('A-row-unavailable');
      } catch (_) { await fail('a-opened','A','a-open-unaccepted'); return 'refused'; }
      await checkpoint('a-opened','returned','A');
      await openB(stateA,baseline,aId);
      return 'complete';
    } catch (_) { return 'refused'; }
  })();
})();
