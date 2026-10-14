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
    let result;
    try { result = await window.SMProject.openPath(config.fixtures.A); }
    catch (_) {
      await fail('a-opened', 'A', 'a-open-threw');
      throw new Error('open-refused');
    }
    if (result !== true) {
      await fail('a-opened', 'A', result === false ? 'a-open-false' : 'a-open-unaccepted');
      throw new Error('open-refused');
    }
    let after;
    try { after = await readNativeStatus(); }
    catch (_) { after = null; }
    if (!validActiveStatus(after, before.instanceId, null, 1)) {
      await fail('a-opened', 'A', 'a-open-unaccepted');
      throw new Error('open-refused');
    }
    await checkpoint('a-opened', 'returned', 'A');
    return after;
  }
  async function openB(previous) {
    let before;
    try { before = await readNativeStatus(); }
    catch (_) { before = null; }
    if (!sameActiveStatus(previous, before)) {
      await fail('b-opened', 'B', 'b-pre-native-refusal');
      throw new Error('open-refused');
    }
    let result;
    try { result = await window.SMProject.openPath(config.fixtures.B); }
    catch (_) {
      await fail('b-opened', 'B', 'b-open-threw');
      throw new Error('open-refused');
    }
    if (result !== true) {
      await fail('b-opened', 'B', result === false ? 'b-open-false' : 'b-open-unaccepted');
      throw new Error('open-refused');
    }
    let after;
    try { after = await readNativeStatus(); }
    catch (_) { after = null; }
    if (!validActiveStatus(after, previous.instanceId, null, previous.lifecycleGeneration + 1) ||
        after.documentId === previous.documentId) {
      await fail('b-opened', 'B', 'b-open-unaccepted');
      throw new Error('open-refused');
    }
    await checkpoint('b-opened', 'returned', 'B');
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
      await checkpoint('ready', 'ready');
      const stateA = await openA();
      await openB(stateA);
      return 'complete';
    } catch (_) { return 'refused'; }
  })();
})();
