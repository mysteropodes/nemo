(function () {
  'use strict';
  const key = '__NEMO_P38_FIXTURE_OPEN__';
  const config = window[key];
  const HASH = Object.freeze({
    A: 'f54c0228634f2c8cd53515d3df9d884464697e62c5837011de35704a67bb5180',
    B: 'd30d31a5524ed5b7bfa7cfbcae5b186cbde7b3ed269ba579c5125cb52806c215'
  });
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
  let activeOpen = null;
  function observePromise(value, fulfilled, rejected) {
    try {
      if (value && typeof value.then === 'function') value.then(
        result => { try { fulfilled(result); } catch (_) {} },
        error => { try { rejected(error); } catch (_) {} });
    } catch (_) { /* Observation must never replace the operation's result. */ }
  }
  function installOpenObserver() {
    const project = window.SMProject;
    const tauri = window.__TAURI__;
    const core = tauri.core;
    const fs = tauri.fs;
    const originalOpen = project.openPath;
    const originalInvoke = core.invoke;
    const originalRead = fs && fs.readTextFile;
    const wrappedOpen = function () {
      if (activeOpen) activeOpen.openCalled = true;
      return Reflect.apply(originalOpen, this, arguments);
    };
    const wrappedInvoke = function (command) {
      const trace = activeOpen;
      const kind = command === 'nemo_native_object_bootstrap' ? 'bootstrap' :
        command === 'nemo_native_object_client_status' ? 'status' : null;
      let result;
      try { result = Reflect.apply(originalInvoke, this, arguments); }
      catch (error) { if (trace && kind) trace[kind] = 'rejected'; throw error; }
      if (trace && kind) observePromise(result, value => {
        trace[kind] = kind === 'status' && value && typeof value.available === 'boolean'
          ? (value.available ? 'available' : 'empty') : 'resolved';
      }, error => {
        trace[kind] = kind === 'bootstrap' && error && error.code === 'unsupported_format'
          ? 'unsupported-format' : 'rejected';
      });
      return result;
    };
    const wrappedRead = originalRead && function () {
      const trace = activeOpen;
      let result;
      try { result = Reflect.apply(originalRead, this, arguments); }
      catch (error) { if (trace) trace.fsRead = 'rejected'; throw error; }
      if (trace) observePromise(result, () => { trace.fsRead = 'resolved'; }, () => { trace.fsRead = 'rejected'; });
      return result;
    };
    const restore = function () {
      if (project.openPath === wrappedOpen) project.openPath = originalOpen;
      if (core.invoke === wrappedInvoke) core.invoke = originalInvoke;
      if (originalRead && fs.readTextFile === wrappedRead) fs.readTextFile = originalRead;
    };
    try {
      project.openPath = wrappedOpen;
      core.invoke = wrappedInvoke;
      if (originalRead) fs.readTextFile = wrappedRead;
    } catch (error) {
      restore();
      throw error;
    }
    return restore;
  }
  function categoricalFailure(id, trace, fallback) {
    const prefix = id === 'A' ? 'a' : 'b';
    if (trace.fsRead === 'rejected') return prefix + '-fs-read-rejected';
    if (trace.fsRead === 'resolved') return prefix + '-fs-read-resolved-open-refused';
    if (trace.bootstrap === 'unsupported-format') return prefix + '-native-bootstrap-unsupported-format';
    if (trace.bootstrap === 'rejected') return prefix + '-native-bootstrap-rejected';
    if (trace.bootstrap === 'resolved') return prefix + '-native-bootstrap-resolved-open-refused';
    if (trace.status === 'rejected') return prefix + '-native-status-rejected';
    if (trace.status === 'empty') return prefix + '-native-status-empty-no-bootstrap';
    if (trace.status === 'available') return prefix + '-native-status-available-no-bootstrap';
    if (trace.status === 'resolved') return prefix + '-native-status-malformed-no-bootstrap';
    if (!trace.status && !trace.bootstrap && !trace.fsRead) return prefix + '-pre-native-refusal';
    return fallback;
  }
  const fail = async (phase, fixtureId, code) => {
    const invoke = window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke;
    if (typeof invoke !== 'function') return;
    const payload = { runId: config.runId, taskKey: config.taskKey, sourceSha: config.sourceSha,
      buildSha: config.buildSha, manifestSha256: config.manifestSha256, phase, status: 'failed',
      fixtureId, fixtureSha256: fixtureId ? HASH[fixtureId] : undefined, failureCode: code };
    await invoke('nemo_test_p38_fixture_open_probe', { payloadJson: JSON.stringify(payload) });
  };
  const checkpoint = (phase, status, fixtureId) => {
    const invoke = window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke;
    if (typeof invoke !== 'function') throw new Error('checkpoint-unavailable');
    const payload = { runId: config.runId, taskKey: config.taskKey, sourceSha: config.sourceSha,
      buildSha: config.buildSha, manifestSha256: config.manifestSha256, phase, status,
      fixtureId, fixtureSha256: fixtureId ? HASH[fixtureId] : undefined };
    return invoke('nemo_test_p38_fixture_open_probe', { payloadJson: JSON.stringify(payload) });
  };
  async function waitForOpenPath() {
    const deadline = Date.now() + 60000;
    while (Date.now() < deadline) {
      if (window.SMProject && typeof window.SMProject.openPath === 'function' &&
          window.__TAURI__ && window.__TAURI__.core && typeof window.__TAURI__.core.invoke === 'function') return;
      await new Promise(resolve => setTimeout(resolve, 50));
    }
    throw new Error('open-path-unavailable');
  }
  async function open(phase, id) {
    const trace = {};
    activeOpen = trace;
    let result;
    try { result = await window.SMProject.openPath(config.fixtures[id]); }
    catch (_) { await fail(phase, id, id === 'A' ? 'a-open-threw' : 'b-open-threw'); throw new Error('open-refused'); }
    finally { activeOpen = null; }
    const accepted = result === undefined || (result && typeof result === 'object' &&
      Object.getPrototypeOf(result) === Object.prototype &&
      Object.keys(result).length === 1 && result.status === 'prepared');
    if (!accepted) {
      const prefix = id === 'A' ? 'a' : 'b';
      const code = result === false ? categoricalFailure(id, trace, prefix + '-open-false') : prefix + '-open-unaccepted';
      await fail(phase, id, code);
      throw new Error('open-refused');
    }
    await checkpoint(phase, 'returned', id);
  }
  window.__NEMO_P38_FIXTURE_OPEN_DONE__ = (async () => {
    let restore = null;
    try {
      await waitForOpenPath();
      await checkpoint('ready', 'ready');
      restore = installOpenObserver();
      await open('a-opened', 'A');
      await open('b-opened', 'B');
      return 'complete';
    } catch (_) { return 'refused'; }
    finally { if (restore) restore(); }
  })();
})();
