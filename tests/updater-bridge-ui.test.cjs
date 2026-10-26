'use strict';
// CS8-D08 I40 (#2381): the original updater UI over mocked available / no-update / failure platforms. The real
// updater-bridge.js and updater-platform.js run in a vm; nothing publishes, deploys or touches the network.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const SRC = path.join(__dirname, '..', 'src', 'js');
function element(id) {
  const listeners = {};
  return { id, textContent: '', innerHTML: '', disabled: false, title: '', style: {},
    addEventListener(kind, fn) { (listeners[kind] = listeners[kind] || []).push(fn); },
    click() { for (const fn of listeners.click || []) fn({}); } };
}
function harness(updater) {
  const ids = ['app-check-update', 'app-update-status', 'app-version-txt', 'status-text'];
  const elements = Object.fromEntries(ids.map((id) => [id, element(id)]));
  const toasts = [], confirms = [], installs = [], relaunches = [], fetches = [], timers = [];
  const document = { readyState: 'complete', title: '', getElementById: (id) => elements[id] || null, addEventListener() {} };
  const tauri = updater && {
    // The real desktop ports: check() returns an Update whose downloadAndInstall(onProgress) installs it.
    updater: { check: async () => { const found = await updater.check(); return found && Object.assign({}, found, {
      downloadAndInstall: (onProgress) => { installs.push(found.version); return updater.install(onProgress); } }); } },
    app: { getVersion: async () => '1.2.3' },
    process: { relaunch: async () => { relaunches.push(true); } },
    dialog: { confirm: async (message, options) => { confirms.push({ message, options }); return updater.confirm(message); } },
    event: { listen: () => {} },
  };
  const root = { document, navigator: { platform: 'Linux', userAgent: 'node' }, console: { warn() {}, log() {} },
    __TAURI__: tauri || undefined, SM: { t: (key) => key }, showToast: (text) => toasts.push(text),
    fetch: (...args) => { fetches.push(args); return Promise.reject(new Error('network is not part of this check')); },
    setTimeout: (fn, ms) => { timers.push(ms); return 0; } };
  root.window = root; root.globalThis = root;
  vm.createContext(root);
  vm.runInContext(fs.readFileSync(path.join(SRC, 'adapters', 'updater-platform.js'), 'utf8'), root, { filename: 'updater-platform.js' });
  vm.runInContext(fs.readFileSync(path.join(SRC, 'updater-bridge.js'), 'utf8'), root, { filename: 'updater-bridge.js' });
  const settle = () => new Promise((resolve) => setImmediate(resolve));
  return { elements, toasts, confirms, installs, relaunches, fetches, timers, settle,
    async check() { elements['app-check-update'].click(); for (let i = 0; i < 6; i++) await settle(); } };
}

test('no update: the status line and toast say up to date, the button re-enables and nothing installs', async () => {
  const h = harness({ check: async () => null, confirm: () => true, install: async () => {} });
  await h.check();
  assert.equal(h.elements['app-update-status'].innerHTML, 'updaterUpToDateStatus');
  assert.deepEqual(h.toasts, ['updaterNoUpdateToast']);
  assert.equal(h.elements['app-check-update'].disabled, false);
  assert.deepEqual(h.installs, []); assert.deepEqual(h.fetches, []);
  assert.deepEqual(h.timers, [4000], 'the silent startup check is only scheduled, never run here');
});

test('available: the user is asked, a decline installs nothing, an accept installs with progress and offers a restart', async () => {
  const notes = { version: '9.9.9', body: 'Fixes' };
  const declined = harness({ check: async () => notes, confirm: () => false, install: async () => {} });
  await declined.check();
  assert.equal(declined.elements['app-update-status'].textContent, 'updaterNewVersionStatus');
  assert.equal(declined.confirms.length, 1); assert.equal(declined.confirms[0].options.title, 'updaterAvailableDialogTitle');
  assert.deepEqual(declined.installs, []);
  const progress = [];
  const accepted = harness({ check: async () => notes, confirm: (m) => m !== 'updaterRestartConfirm', install: async (on) => {
    on({ event: 'Started', data: { contentLength: 200 } }); on({ event: 'Progress', data: { chunkLength: 100 } });
    progress.push(accepted.elements['app-update-status'].textContent); on({ event: 'Finished', data: {} });
  } });
  await accepted.check();
  assert.deepEqual(accepted.installs, ['9.9.9']);
  assert.deepEqual(progress, ['updaterDownloadingPct']);
  assert.equal(accepted.elements['app-update-status'].textContent, 'updaterInstalledStatus');
  assert.deepEqual(accepted.relaunches, [], 'declining the restart keeps the session');
  assert.deepEqual(accepted.toasts, ['updaterDownloading', 'updaterInstalledToast']);
  assert.deepEqual(accepted.fetches, []);
});

test('failure: a check or install error is shown with its detail and changes nothing else', async () => {
  const failed = harness({ check: async () => { throw new Error('TargetsNotFound'); }, confirm: () => true, install: async () => {} });
  await failed.check();
  assert.equal(failed.elements['app-update-status'].textContent, 'updaterCheckFailedStatus');
  assert.deepEqual(failed.toasts, ['updaterCheckFailedToast']);
  assert.equal(failed.elements['app-check-update'].disabled, false);
  const broken = harness({ check: async () => ({ version: '9.9.9' }), confirm: () => true, install: async () => { throw new Error('signature mismatch'); } });
  await broken.check();
  assert.equal(broken.elements['app-update-status'].textContent, 'updaterInstallFailedStatus');
  assert.deepEqual(broken.toasts, ['updaterDownloading', 'updaterInstallFailedToast']);
  assert.deepEqual(broken.relaunches, []);
  const offline = harness(null);
  offline.elements['app-check-update'].click(); await offline.settle();
  assert.deepEqual(offline.toasts, ['updaterUnavailable'], 'without the native updater the original toast is the only effect');
  assert.deepEqual(offline.timers, []);
});
