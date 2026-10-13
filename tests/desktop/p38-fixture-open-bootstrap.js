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
    let result;
    try { result = await window.SMProject.openPath(config.fixtures[id]); }
    catch (_) { await fail(phase, id, id === 'A' ? 'a-open-threw' : 'b-open-threw'); throw new Error('open-refused'); }
    const accepted = result === undefined || (result && typeof result === 'object' &&
      Object.getPrototypeOf(result) === Object.prototype &&
      Object.keys(result).length === 1 && result.status === 'prepared');
    if (!accepted) {
      const prefix = id === 'A' ? 'a' : 'b';
      const code = result === false ? prefix + '-open-false' : prefix + '-open-unaccepted';
      await fail(phase, id, code);
      throw new Error('open-refused');
    }
    await checkpoint(phase, 'returned', id);
  }
  window.__NEMO_P38_FIXTURE_OPEN_DONE__ = (async () => {
    try {
      await waitForOpenPath();
      await checkpoint('ready', 'ready');
      await open('a-opened', 'A');
      await open('b-opened', 'B');
      return 'complete';
    } catch (_) { return 'refused'; }
  })();
})();
