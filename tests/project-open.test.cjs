'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const test = require('node:test');
const vm = require('node:vm');

const source = fs.readFileSync('src/js/project.js', 'utf8');
const timelineSource = fs.readFileSync('src/js/timeline.js', 'utf8');
const documentSource = fs.readFileSync('src/js/project-document.js', 'utf8');
const entrySource = fs.readFileSync('src/js/project-entry.js', 'utf8');

function realImportJSON() {
  const start = timelineSource.indexOf('  importJSON:function(json,silent){');
  const end = timelineSource.indexOf('\n  getState:function()', start);
  assert.notEqual(start, -1, 'timeline exposes importJSON');
  assert.notEqual(end, -1, 'timeline importJSON has a stable boundary');
  const method = timelineSource.slice(start, end).replace('  importJSON:', '').replace(/},\s*$/, '}');
  const calls = [];
  const state = { layers: [{ name: 'Keep' }] };
  const engine = { clearRetainedPaths() { calls.push('engine'); } };
  const context = vm.createContext({
    window: { SMLabs: { resetAll() { calls.push('labs'); } }, SMEngineBridge: engine }, SMEngineBridge: engine,
    SM: { t(key) { return key; } }, state, userLayers: [], _symbolPaperLayers: {}, showToast(message) { calls.push(message); }
  });
  vm.runInContext(documentSource, context);
  const importJSON = vm.runInContext(`(${method})`, context, { filename: 'timeline-importJSON.js' });
  return { calls, importJSON, state };
}

function harness({ auto = null, version = null, deferFrames = false, dirtyDot = false, nativeGuard = false,
  configureWindow = () => {}, allowBootstrapFailure = false } = {}) {
  const elements = new Map();
  const downloads = [], toasts = [], frames = [], writes = [], windowEvents = {};
  let repaints = 0, mutations = 0;
  let json = JSON.stringify({ title: 'boot' });
  function element() {
    const classes = new Set();
    const el = { style: {}, dataset: {}, value: '', files: [], children: [], classList: {
      add(name) { classes.add(name); }, remove(name) { classes.delete(name); },
      toggle(name, force) { if (force === false || force === undefined && classes.has(name)) classes.delete(name); else classes.add(name); },
      contains(name) { return classes.has(name); } },
      addEventListener(type, fn) { this.listeners ||= {}; this.listeners[type] = fn; }, appendChild(child) { this.children.push(child); child.parent = this; },
      setAttribute(name, value) { this.attributes ||= {}; this.attributes[name] = value; },
      getAttribute(name) { return this.attributes && this.attributes[name]; },
      removeChild() {}, focus() {}, select() {}, replaceWith(next) { next.parent = this.parent; this.parent.children[this.parent.children.indexOf(this)] = next; },
      click() { this.clicked = true; } };
    Object.defineProperty(el, 'innerHTML', { get() { return ''; }, set() { this.children = []; } });
    return el;
  }
  const dot = element();
  const document = { readyState: 'loading', body: element(), addEventListener(type, fn) { if (type === 'DOMContentLoaded') this.ready = fn; },
    getElementById(id) { if (!elements.has(id)) elements.set(id, element()); return elements.get(id); }, createElement: element,
    querySelector(selector) {
      const name = selector.match(/^\.project-tab\[data-tab="([^"]+)"\] \.pt-name$/);
      if (name) return elements.get('project-tabs-list')?.children.find(row => row.dataset.tab === name[1])?.children[2];
      return dirtyDot && selector.startsWith('.project-tab[data-tab=') ? dot : null;
    } };
  let rejectNextImport = false;
  const window = { document, location: { protocol: 'http:', hostname: 'localhost', port: '1420' },
    NemoNativeOpacityLegacySurface: require('../src/js/adapters/native-opacity-legacy-surface.js'),
    NemoNativeObjectInactiveProjectTabs: require('../src/js/adapters/native-object-inactive-project-tabs.js'),
    NemoNativeObjectProjectTabsUI: require('../src/js/adapters/native-object-project-tabs-ui.js'),
    addEventListener(type, callback) { windowEvents[type] = callback; }, SM: { t(key) { return key; }, fitCanvas() {}, exportJSON() { return json; }, importJSON(raw) {
    if (rejectNextImport) { rejectNextImport = false; return false; }
    try { const parsed = JSON.parse(raw); if (parsed.fail) return false; json = JSON.stringify({ ...parsed, normalized: true }); return true; } catch (_) { return false; }
  } } };
  window.NemoNativeOpacityProjectEntry = require('../src/js/adapters/native-opacity-project-entry.js');
  window.NemoProjectNativeSave = require('../src/js/adapters/project-native-save.js');
  window.NemoProjectDirtyBaseline = require('../src/js/adapters/project-dirty-baseline.js');
  window.NemoProjectNativeVersionRestore = require('../src/js/adapters/project-native-version-restore.js');
  const context = { window, SM: window.SM, document, FileReader: class Reader { readAsText(file) { if (file.error) this.onerror(new Error('read failed')); else this.onload({ target: { result: file.text } }); } }, Blob: class { constructor(parts) { this.parts = parts; } }, URL: { createObjectURL() { return 'blob:test'; }, revokeObjectURL() {} },
    localStorage: { getItem(key) { return key === 'nemo-auto' ? auto : writes.findLast(([saved]) => saved === key)?.[1] || null; }, setItem(key, value) { writes.push([key, value]); }, removeItem() {} }, state: {}, userLayers: [], _symbolPaperLayers: {}, showToast(message) { toasts.push(message); },
    requestAnimationFrame(fn) { if (deferFrames) frames.push(fn); else fn(); }, view: { update() { repaints++; } }, saveAllLayerFrames() {
      if (nativeGuard) window.NemoNativeOpacityLegacySurface.requireLegacyWrite(window, 'save-all-layer-frames');
      mutations++;
    }, createUserLayer() { mutations++; }, activateUL() {}, drawStage() {}, loadFrame() {}, renderOS() {}, renderArcs() {}, updateUI() {}, renderSymbolTabs() {}, syncDocFields() {}, exitToScene() {}, setTimeout, console };
  window.requestAnimationFrame = function (fn) {
    assert.strictEqual(this, window, 'WebKit requestAnimationFrame requires a Window receiver');
    return context.requestAnimationFrame(fn);
  };
  if (version !== null) window.__TAURI__ = { core: { invoke() {} }, fs: { readTextFile: async () => version } };
  window.updateUI = context.updateUI;
  configureWindow(window);
  vm.runInNewContext(documentSource, context, { filename: 'project-document.js' });
  vm.runInNewContext(entrySource, context, { filename: 'project-entry.js' });
  context.SMProjectEntry = window.SMProjectEntry;
  document.body.appendChild = node => downloads.push(node.download);
  let bootstrapError;
  try { vm.runInNewContext(source, context, { filename: 'project.js' }); }
  catch (error) { if (!allowBootstrapFailure) throw error; bootstrapError = error; }
  if (!bootstrapError) document.ready();
  return { window, input: document.getElementById('file-input'), startScreen: document.getElementById('start-screen'),
    element(id) { return document.getElementById(id); },
    downloads, toasts, writes, project: window.SMProject, elements, dot, windowEvents, bootstrapError,
    flushFrame() { assert.ok(frames.length, 'an animation frame is queued'); frames.shift()(); },
    get pendingFrames() { return frames.length; },
    rejectNextImport() { rejectNextImport = true; }, get json() { return json; }, set json(value) { json = value; }, get repaints() { return repaints; }, get mutations() { return mutations; } };
}

function select(app, file) { app.input.listeners.change({ target: { files: file ? [file] : [], value: 'selected' } }); }
function newProjectForm(app) {
  app.elements.get('start-new').listeners.click();
  app.element('np-name').value = 'Keep fields';
  app.element('np-preset').value = '1280x720';
  app.element('np-fps').value = '30';
  return app.elements.get('start-newpanel');
}
function admissionMessage(form) {
  return form.children.find(child => child.getAttribute && child.getAttribute('role') === 'alert');
}
test('production loads native project-entry and inactive-tab adapters before the project consumer', () => {
  const html = fs.readFileSync('src/index.html', 'utf8');
  const adapter = html.indexOf('js/adapters/native-opacity-project-entry.js');
  const inactive = html.indexOf('js/adapters/native-object-inactive-project-tabs.js');
  const project = html.indexOf('js/project.js', adapter);
  assert.ok(adapter >= 0 && inactive > adapter && project > inactive);
  const dirty = html.indexOf('js/adapters/project-dirty-baseline.js');
  assert.ok(dirty >= 0 && dirty < project, 'dirty baseline is loaded before its real consumer');
});
function deferred() { let resolve, reject; const promise = new Promise((done, fail) => { resolve = done; reject = fail; }); return { promise, resolve, reject }; }
function installNativeOpen(app) {
  const first = deferred(), visible = deferred();
  const receipt = { owner: 'native', status: 'presented', frame: 0,
    instanceId: 'instance-a', documentId: 'document-a', contentRevision: 0,
    lifecycleGeneration: 1, documentSnapshotId: 'snapshot-a' };
  let active = false, presentations = 0, deferredAllowed = null;
  app.window.NemoNativeOpacityCutover = {
    blocksLegacy() { return active; }, isActive() { return active; },
    identity() { return { instanceId: receipt.instanceId, documentId: receipt.documentId,
      contentRevision: receipt.contentRevision }; },
    persistenceJSON() { return '{"native":true}'; },
    presentPreview(frame) { assert.equal(frame, 0); presentations++; return visible.promise; },
  };
  app.window.NemoNativeOpacityProject = { importJSON(json, silent, allowOccludedAdmission) {
    deferredAllowed = allowOccludedAdmission;
    return first.promise.then(value => {
    if (value) active = true; return value;
  }); }, finishOpenAfterReveal(incoming) {
    assert.equal(incoming.instanceId, receipt.instanceId);
    presentations++;
    return visible.promise.then(result => {
      if (result.status !== 'presented' || result.lifecycleGeneration !== incoming.lifecycleGeneration ||
          result.instanceId !== incoming.instanceId || result.documentId !== incoming.documentId ||
          result.contentRevision !== incoming.contentRevision ||
          result.documentSnapshotId !== incoming.documentSnapshotId) {
        throw new Error('Native viewport is not current after reveal');
      }
      return result;
    });
  } };
  return { receipt, first, visible, get presentations() { return presentations; },
    get deferredAllowed() { return deferredAllowed; } };
}

test('browser Resume preserves normalized imported data and the existing repaint', () => {
  const app = harness({ auto: '{"title":"autosaved"}' });
  app.elements.get('start-resume').listeners.click();
  assert.deepEqual(JSON.parse(app.json), { title: 'autosaved', normalized: true });
  assert.equal(app.startScreen.classList.contains('hid'), true);
  assert.equal(app.repaints, 1);
  assert.equal(app.toasts.at(-1), 'Session resumed');
  assert.equal(app.project.getCurrentLabel(), 'Untitled (not saved)');
});

test('native Resume waits for presented admission and final visible publication', async () => {
  const app = harness({ auto: '{"supported":true}', deferFrames: true });
  const native = installNativeOpen(app);
  app.elements.get('start-resume').listeners.click();
  assert.equal(native.deferredAllowed, true);
  assert.equal(app.startScreen.classList.contains('hid'), false);
  native.first.resolve(native.receipt);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(app.startScreen.classList.contains('hid'), true);
  assert.equal(app.toasts.includes('Session resumed'), false);
  assert.equal(app.elements.get('project-tabs-list').children.length, 0);
  app.flushFrame(); app.flushFrame();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(native.presentations, 1);
  assert.equal(app.repaints, 0);
  native.visible.resolve({ ...native.receipt, workId: 'visible-resume' });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(app.toasts.at(-1), 'Session resumed');
  assert.equal(app.elements.get('project-tabs-list').children.length, 1);
  assert.equal(app.mutations, 0);
});

for (const rejected of [false, { instanceId: 'stale-instance' }, { documentId: 'stale-document' },
  { contentRevision: 1 }, { status: 'failed' }, { status: 'deferred-occluded' }]) {
  test(`denied or stale native Resume admission leaves the start screen visible (${JSON.stringify(rejected)})`, async () => {
    const app = harness({ auto: '{"supported":true}', deferFrames: true });
    const native = installNativeOpen(app);
    app.elements.get('start-resume').listeners.click();
    native.first.resolve(rejected === false ? false : { ...native.receipt, ...rejected });
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(app.startScreen.classList.contains('hid'), false);
    assert.equal(app.toasts.includes('Session resumed'), false);
    assert.equal(app.toasts.at(-1), 'toastCannotResumeSessionCorrupt');
    assert.equal(native.presentations, 0);
    assert.equal(app.elements.get('project-tabs-list').children.length, 0);
    assert.equal(app.mutations, 0);
  });
}

test('stale final native Resume presentation restores the start screen without success', async () => {
  const app = harness({ auto: '{"supported":true}', deferFrames: true });
  const native = installNativeOpen(app);
  app.elements.get('start-resume').listeners.click();
  native.first.resolve(native.receipt);
  await new Promise(resolve => setImmediate(resolve));
  app.flushFrame(); app.flushFrame();
  await new Promise(resolve => setImmediate(resolve));
  native.visible.resolve({ ...native.receipt, contentRevision: 1 });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(app.startScreen.classList.contains('hid'), false);
  assert.equal(app.toasts.includes('Session resumed'), false);
  assert.equal(app.toasts.at(-1), 'toastCannotResumeSessionCorrupt');
  assert.equal(app.elements.get('project-tabs-list').children.length, 0);
  assert.equal(app.mutations, 0);
});

for (const storage of ['localStorage', 'IndexedDB']) {
test(`rejected final native Resume presentation restores the start screen without success (${storage})`, async () => {
  const app = harness({ auto: storage === 'localStorage' ? '{"supported":true}' : null, deferFrames: true });
  const native = installNativeOpen(app);
  if (storage === 'IndexedDB') app.window.SMIdb = { get: () => Promise.resolve('{"supported":true}') };
  app.elements.get('start-resume').listeners.click();
  await new Promise(resolve => setImmediate(resolve));
  native.first.resolve(native.receipt);
  await new Promise(resolve => setImmediate(resolve));
  app.flushFrame(); app.flushFrame();
  await new Promise(resolve => setImmediate(resolve));
  native.visible.reject(new Error('native reveal presentation rejected'));
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(app.startScreen.classList.contains('hid'), false);
  assert.equal(app.toasts.includes('Session resumed'), false);
  assert.equal(app.toasts.at(-1), 'toastCannotResumeSessionCorrupt');
  assert.equal(app.elements.get('project-tabs-list').children.length, 0);
  assert.equal(app.mutations, 0);
});
}

test('browser Open keeps the file name and normalized clean baseline for later Save', async () => {
  const app = harness();
  select(app, { name: 'Story.JSON', text: '{"title":"story"}' });
  assert.equal(app.project.getCurrentLabel(), 'Story (not saved)');
  assert.equal(app.project.isDirty(), false, 'normalized import is clean');
  assert.equal(app.repaints, 1, 'browser Open uses the existing two-frame repaint adapter');
  await app.project.save();
  assert.equal(app.downloads.at(-1), 'Story.json', 'unchanged browser Save retains opened basename');
  app.json = JSON.stringify({ title: 'story', normalized: true, edited: true });
  assert.equal(app.project.isDirty(), true, 'an actual edit is dirty');
  await app.project.save();
  assert.equal(app.downloads.at(-1), 'Story.json', 'edited browser Save still retains opened basename');
});

test('desktop picker requests one awaited window exposure before native Open admission', async () => {
  const app = harness({ version: '{"supported":true}' });
  const native = installNativeOpen(app);
  const focus = deferred();
  const calls = [];
  app.window.__TAURI__.dialog = { open: async () => '/tmp/Focused.json' };
  app.window.__TAURI__.window = { getCurrentWindow() { return { setFocus() { calls.push('focus'); return focus.promise; } }; } };
  app.window.__TAURI__.fs.readTextFile = async () => { calls.push('read'); return '{"supported":true}'; };
  const opening = app.project.open();
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(calls, ['focus'], 'file admission waits for the one-time focus request');
  assert.equal(app.toasts.some(toast => toast.startsWith('Opened:')), false);
  focus.resolve();
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(calls, ['focus', 'read']);
  native.first.resolve(native.receipt);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(app.toasts.some(toast => toast.startsWith('Opened:')), false,
    'window focus cannot substitute for native presentation');
  native.visible.resolve({ ...native.receipt, workId: 'visible-focus-frame' });
  await opening;
  assert.equal(app.toasts.at(-1), 'Opened: Focused');
});

test('failed window focus never publishes an Open before native presentation', async () => {
  const app = harness({ version: '{"supported":true}' });
  const native = installNativeOpen(app);
  app.window.__TAURI__.dialog = { open: async () => '/tmp/Unfocused.json' };
  app.window.__TAURI__.window = { getCurrentWindow() { return { async setFocus() { throw new Error('occluded'); } }; } };
  const opening = app.project.open();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(native.deferredAllowed, true);
  assert.equal(app.toasts.some(toast => toast.startsWith('Opened:')), false);
  native.first.resolve(native.receipt);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(app.toasts.some(toast => toast.startsWith('Opened:')), false);
  native.visible.resolve({ ...native.receipt, workId: 'visible-after-focus-failure' });
  await opening;
  assert.equal(app.toasts.at(-1), 'Opened: Unfocused');
});

test('native browser Open publishes success only after both frame-0 presentation boundaries', async () => {
  const app = harness({ deferFrames: true }), native = installNativeOpen(app);
  select(app, { name: 'Native.json', text: '{"supported":true}' });
  assert.equal(app.startScreen.classList.contains('hid'), false);
  assert.equal(native.deferredAllowed, true, 'only explicit Open opts into the reveal handoff');
  assert.equal(app.project.getCurrentLabel(), 'Untitled (not saved)');
  native.first.resolve(native.receipt);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(app.startScreen.classList.contains('hid'), true);
  assert.equal(app.project.getCurrentLabel(), 'Untitled (not saved)', 'no early metadata publication');
  app.flushFrame(); app.flushFrame();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(app.repaints, 0, 'native reveal does not schedule legacy repaint');
  assert.equal(native.presentations, 1, 'the current viewport is presented after reveal');
  assert.equal(app.toasts.includes('Opened: Native'), false);
  native.visible.resolve({ ...native.receipt, workId: 'visible-frame' });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(app.project.getCurrentLabel(), 'Native (not saved)');
  assert.equal(app.project.isDirty(), false);
  assert.equal(app.toasts.at(-1), 'Opened: Native');
  assert.equal(app.mutations, 0, 'Paper writers never run during native Open');
});

test('deferred native first frame cannot hide the start screen or publish an Open', async () => {
  const app = harness({ deferFrames: true }), native = installNativeOpen(app);
  const first = { ...native.receipt, status: 'deferred-occluded' };
  select(app, { name: 'Occluded.json', text: '{"supported":true}' });
  native.first.resolve(first);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(app.startScreen.classList.contains('hid'), false);
  assert.equal(app.project.getCurrentLabel(), 'Untitled (not saved)');
  assert.equal(app.pendingFrames, 0);
  assert.equal(native.presentations, 0);
  assert.equal(app.toasts.includes('Opened: Occluded'), false);
  assert.equal(app.mutations, 0);
});

test('desktop Open attempts final native presentation when reveal animation frames stall', async () => {
  const app = harness({ version: '{"supported":true}', deferFrames: true });
  const native = installNativeOpen(app);
  const opening = app.project.openPath('/tmp/Native.json');
  native.first.resolve(native.receipt);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(app.startScreen.classList.contains('hid'), true);
  assert.equal(app.project.getCurrentLabel(), 'Untitled (not saved)');
  assert.equal(app.pendingFrames, 1, 'the first RAF remains suspended');
  await new Promise(resolve => setTimeout(resolve, 250));
  assert.equal(native.presentations, 1, 'the bounded fallback reaches final presentation');
  assert.equal(app.project.getCurrentLabel(), 'Untitled (not saved)', 'the timer alone cannot publish Open');
  native.visible.resolve({ ...native.receipt, workId: 'visible-frame' });
  await opening;
  assert.match(app.project.getCurrentLabel(), /^Native — \/tmp\/Native\.json$/);
  assert.equal(app.toasts.at(-1), 'Opened: Native');
  assert.equal(app.mutations, 0, 'no legacy writer runs on the fallback path');
});

test('stalled reveal RAF fallback refuses a deferred final native receipt', async () => {
  const app = harness({ version: '{"supported":true}', deferFrames: true });
  const native = installNativeOpen(app);
  const opening = app.project.openPath('/tmp/Native.json');
  native.first.resolve(native.receipt);
  await new Promise(resolve => setTimeout(resolve, 250));
  assert.equal(native.presentations, 1);
  native.visible.resolve({ ...native.receipt, status: 'deferred-occluded' });
  await opening;
  assert.equal(app.startScreen.classList.contains('hid'), false);
  assert.equal(app.project.getCurrentLabel(), 'Untitled (not saved)');
  assert.equal(app.toasts.some(toast => toast.startsWith('Opened:')), false);
  assert.equal(app.mutations, 0);
});

test('stale post-reveal native frame leaves desktop Open unpublished and returns to start screen', async () => {
  const app = harness({ version: '{"supported":true}', deferFrames: true });
  const native = installNativeOpen(app);
  const opening = app.project.openPath('/tmp/Native.json');
  native.first.resolve(native.receipt);
  await new Promise(resolve => setImmediate(resolve));
  app.flushFrame(); app.flushFrame();
  await new Promise(resolve => setImmediate(resolve));
  native.visible.resolve({ ...native.receipt, lifecycleGeneration: 2 });
  await opening;
  assert.equal(app.startScreen.classList.contains('hid'), false);
  assert.equal(app.project.getCurrentLabel(), 'Untitled (not saved)');
  assert.equal(app.toasts.some(toast => toast.startsWith('Opened:')), false);
  assert.equal(app.mutations, 0);
});

test('rejected project transitions preserve the active document and suppress success signals', async () => {
  const app = harness({ auto: '{"fail":true}' });
  const beforeResume = { label: app.project.getCurrentLabel(), json: app.json, dirty: app.project.isDirty() };
  app.elements.get('start-resume').listeners.click();
  assert.deepEqual({ label: app.project.getCurrentLabel(), json: app.json, dirty: app.project.isDirty() }, beforeResume);
  assert.match(app.toasts.at(-1), /toastCannotResumeSessionCorrupt/);
  assert.equal(app.toasts.includes('Session resumed'), false);

  select(app, { name: 'Keep.json', text: '{"title":"keep"}' });
  app.json = JSON.stringify({ title: 'keep', normalized: true, edited: true });
  // Desktop restore uses admitted native ports; the remaining tab assertions
  // exercise the separate ordinary-browser document, not an fs-only desktop.
  const restore = versionRestoreHarness(), nativeApp = restore.app;
  nativeApp.window.NemoNativeOpacityProject.importJSON = async () => false;
  const beforeRestore = { label: nativeApp.project.getCurrentLabel(), json: nativeApp.json, dirty: nativeApp.project.isDirty() };
  assert.equal(await nativeApp.project.restoreVersion('/history/rejected.json'), false);
  assert.deepEqual({ label: nativeApp.project.getCurrentLabel(), json: nativeApp.json, dirty: nativeApp.project.isDirty() }, beforeRestore);
  assert.equal(nativeApp.toasts.includes('toastVersionRestored'), false);
  assert.deepEqual(restore.writes, []);

  const tabs = app.elements.get('project-tabs-list');
  app.elements.get('project-tab-add').listeners.click();
  const beforeSwitch = { label: app.project.getCurrentLabel(), json: app.json, dirty: app.project.isDirty() };
  app.rejectNextImport();
  tabs.children[0].listeners.click();
  assert.deepEqual({ label: app.project.getCurrentLabel(), json: app.json, dirty: app.project.isDirty() }, beforeSwitch);
  assert.equal(tabs.children.length, 2, 'rejected switch leaves the active tab intact');

  app.rejectNextImport();
  tabs.children[1].children.at(-1).listeners.click({ stopPropagation() {} });
  assert.deepEqual({ label: app.project.getCurrentLabel(), json: app.json, dirty: app.project.isDirty() }, beforeSwitch);
  assert.equal(tabs.children.length, 2, 'rejected close keeps the active tab');
});

test('browser Open leaves the current document alone for cancellation and failed reads', () => {
  const app = harness();
  select(app, { name: 'Keep.json', text: '{"title":"keep"}' });
  app.json = JSON.stringify({ title: 'keep', normalized: true, edited: true });
  const before = { label: app.project.getCurrentLabel(), json: app.json, dirty: app.project.isDirty() };
  assert.equal(before.dirty, true, 'the prior document is dirty before a failed Open');
  select(app, null);
  assert.deepEqual({ label: app.project.getCurrentLabel(), json: app.json, dirty: app.project.isDirty() }, before);
  select(app, { name: 'Broken.json', error: true });
  assert.deepEqual({ label: app.project.getCurrentLabel(), json: app.json, dirty: app.project.isDirty() }, before);
  select(app, { name: 'Broken.json', text: '{"fail":true}' });
  assert.deepEqual({ label: app.project.getCurrentLabel(), json: app.json, dirty: app.project.isDirty() }, before);
  assert.match(app.toasts.at(-1), /Could not open file/);
});

test('real importJSON reports malformed and structurally invalid input without replacing state', () => {
  for (const raw of ['{', '{"layers":[{"frames":null}]}']) {
    const actual = realImportJSON();
    const before = JSON.stringify(actual.state);
    assert.equal(actual.importJSON(raw, true), false, 'actual importJSON reports failure');
    assert.equal(JSON.stringify(actual.state), before, 'validation fails before the current document is replaced');
    assert.equal(actual.calls.includes('engine'), false, 'rejected input preserves retained renderer paths');
    assert.equal(actual.calls.includes('labs'), false, 'rejected input preserves live lab state');
    assert.doesNotMatch(actual.calls.at(-1), /undefined|not a function/, 'the real validator must run');
  }
});

test('project validation preserves fields and the legacy frame-only migration', () => {
  const context = vm.createContext({ window: {} });
  vm.runInContext(documentSource, context);
  const parse = context.window.SMProjectDocument.parse;
  const frame = { strokes: [{ custom: 'retained' }] };
  const current = { version: 13, fps: 24, layers: [{ frames: [frame] }] };
  assert.deepEqual(JSON.parse(JSON.stringify(parse(JSON.stringify(current)))), current);
  const old = parse(JSON.stringify({ frames: [frame], fps: 12 }));
  assert.equal(old.layers[0].name, 'Layer 1');
  assert.equal(old.layers[0].frames[0].strokes[0].custom, 'retained');
  for (const raw of ['null', '{}', '{"layers":[]}', '{"layers":[{"frames":[{}]}]}']) {
    assert.throws(() => parse(raw));
  }
});


test('closing the first active tab selects a surviving document', () => {
  const app = harness();
  select(app, { name: 'First.json', text: '{"title":"first"}' });
  app.elements.get('project-tab-add').listeners.click();
  select(app, { name: 'Second.json', text: '{"title":"second"}' });
  const tabs = app.elements.get('project-tabs-list');
  tabs.children[0].listeners.click();
  assert.equal(app.project.getCurrentLabel(), 'First (not saved)');
  tabs.children[0].children.at(-1).listeners.click({ stopPropagation() {} });
  assert.equal(tabs.children.length, 1);
  assert.equal(app.project.getCurrentLabel(), 'Second (not saved)');
  assert.equal(JSON.parse(app.json).title, 'second');
  app.json = JSON.stringify({ title: 'second', edited: true });
  assert.equal(app.project.isDirty(), true);
});

test('native-owned tab add explains denial without changing the current project', async () => {
  for (const release of [() => false, () => Promise.resolve(false), () => Promise.reject(new Error('transport unavailable'))]) {
    const app = harness();
    app.project.enterEditor();
    const tabs = app.elements.get('project-tabs-list');
    app.window.NemoNativeOpacityLegacySurface = { allowProjectTransition() { return false; }, releaseProjectTransition: release };
    const before = { json: app.json, label: app.project.getCurrentLabel(), count: tabs.children.length, mutations: app.mutations };
    assert.equal(await app.elements.get('project-tab-add').listeners.click(), false);
    assert.deepEqual({ json: app.json, label: app.project.getCurrentLabel(), count: tabs.children.length, mutations: app.mutations }, before);
    const alert = app.elements.get('project-tabs-bar').children.at(-1);
    assert.equal(alert.attributes.role, 'alert');
    assert.equal(alert.style.display, 'block');
    assert.match(alert.textContent, /New project unavailable.*native admission was denied/);
  }
});

test('tab add denies before mutation when legacy writer admission is absent, then clears notice on allowed legacy add', async () => {
  const app = harness();
  app.project.enterEditor();
  delete app.window.NemoNativeOpacityLegacySurface;
  app.window.n20AllowLegacyWrite = () => false;
  const tabs = app.elements.get('project-tabs-list');
  const before = { json: app.json, label: app.project.getCurrentLabel(), count: tabs.children.length, mutations: app.mutations };
  assert.equal(await app.elements.get('project-tab-add').listeners.click(), false);
  assert.deepEqual({ json: app.json, label: app.project.getCurrentLabel(), count: tabs.children.length, mutations: app.mutations }, before);
  const alert = app.elements.get('project-tabs-bar').children.at(-1);
  assert.equal(alert.style.display, 'block');
  app.window.n20AllowLegacyWrite = () => true;
  await app.elements.get('project-tab-add').listeners.click();
  assert.equal(tabs.children.length, before.count + 1);
  assert.ok(app.mutations > before.mutations);
  assert.equal(alert.style.display, 'none');
});

test('late tab-add denial cannot announce failure after a newer successful tab add', async () => {
  const app = harness();
  app.project.enterEditor();
  const pending = deferred();
  app.window.NemoNativeOpacityLegacySurface = { allowProjectTransition() { return false; }, releaseProjectTransition() { return pending.promise; } };
  const first = app.elements.get('project-tab-add').listeners.click();
  delete app.window.NemoNativeOpacityLegacySurface;
  await app.elements.get('project-tab-add').listeners.click();
  pending.reject(new Error('old transport failure'));
  assert.equal(await first, false);
  assert.equal(app.elements.get('project-tabs-list').children.length, 2);
  assert.equal(app.elements.get('project-tabs-bar').children.at(-1).style.display, 'none');
});

test('actual New and blank tab callbacks deny before release or document creation for every blocked phase', async () => {
  for (const phase of ['native', 'closed', 'indeterminate']) {
    const app = harness();
    const tabs = app.elements.get('project-tabs-list');
    app.project.enterEditor();
    app.elements.get('project-tab-add').listeners.click();
    const blank = tabs.children[1];
    const exportJSON = app.window.SM.exportJSON;
    app.window.SM.exportJSON = () => null;
    tabs.children[0].listeners.click();
    app.window.SM.exportJSON = exportJSON;
    let releases = 0;
    app.window.NemoNativeOpacityCutover = { blocksLegacy: () => true };
    app.window.NemoNativeOpacityProject = { release() { releases++; return Promise.resolve({ owner: 'none', status: 'closed' }); } };
    const before = { json: app.json, label: app.project.getCurrentLabel(), count: tabs.children.length, toasts: app.toasts.length };
    assert.equal(await app.project.newProject({ w: 320, h: 180, fps: 24 }), false, phase);
    app.elements.get('project-tab-add').listeners.click();
    blank.listeners.click();
    tabs.children[0].children.at(-1).listeners.click({ stopPropagation() {} });
    await Promise.resolve();
    assert.equal(releases, 0, phase);
    assert.deepEqual({ json: app.json, label: app.project.getCurrentLabel(), count: tabs.children.length, toasts: app.toasts.length }, before, phase);
  }
});

test('missing or malformed legacy surface denies actual blank transitions with any remaining native publication', async () => {
  const surfaces = [undefined, null, {}, { allowProjectTransition() { return true; } },
    { allowProjectTransition() { return true; }, releaseProjectTransition: false },
    { allowProjectTransition() { return true; }, releaseProjectTransition() { throw new Error('broken surface'); } }];
  for (const surface of surfaces) for (const publications of ['cutover', 'project', 'both']) {
    const app = harness();
    app.project.enterEditor();
    const tabs = app.elements.get('project-tabs-list');
    app.elements.get('project-tab-add').listeners.click();
    const blank = tabs.children[1];
    const exportJSON = app.window.SM.exportJSON;
    app.window.SM.exportJSON = () => null;
    tabs.children[0].listeners.click();
    app.window.SM.exportJSON = exportJSON;
    let releases = 0;
    app.window.NemoNativeOpacityLegacySurface = surface;
    if (publications !== 'project') app.window.NemoNativeOpacityCutover = { blocksLegacy: () => true };
    if (publications !== 'cutover') app.window.NemoNativeOpacityProject = {
      release() { releases++; return Promise.resolve({ owner: 'none', status: 'closed' }); },
    };
    const observed = () => ({ json: app.json, label: app.project.getCurrentLabel(), dirty: app.project.isDirty(),
      count: tabs.children.length, toasts: app.toasts.length, repaints: app.repaints, mutations: app.mutations });
    const before = observed();
    const result = app.project.newProject({ w: 320, h: 180, fps: 24 });
    assert.equal(result, false);
    app.elements.get('project-tab-add').listeners.click();
    blank.listeners.click();
    tabs.children[0].children.at(-1).listeners.click({ stopPropagation() {} });
    assert.equal(releases, 0, publications);
    assert.deepEqual(observed(), before, publications);
    await Promise.resolve();
    assert.equal(releases, 0, publications);
    assert.deepEqual(observed(), before, publications);
  }
});

test('always-dormant absence of all native publications preserves actual New and blank tab transitions', async () => {
  const app = harness();
  delete app.window.NemoNativeOpacityLegacySurface;
  assert.equal(await app.project.newProject({ w: 320, h: 180, fps: 24, name: 'Dormant' }), undefined);
  assert.equal(app.project.getCurrentLabel(), 'Dormant (not saved)');
  const tabs = app.elements.get('project-tabs-list');
  const before = app.mutations;
  app.elements.get('project-tab-add').listeners.click();
  assert.equal(tabs.children.length, 2);
  assert.ok(app.mutations > before);
  const exportJSON = app.window.SM.exportJSON;
  app.window.SM.exportJSON = () => null;
  tabs.children[0].listeners.click();
  app.window.SM.exportJSON = exportJSON;
  tabs.children[1].listeners.click();
  assert.equal(app.project.getCurrentLabel(), 'Untitled 2 (not saved)');
  tabs.children[1].children.at(-1).listeners.click({ stopPropagation() {} });
  assert.equal(tabs.children.length, 1);
  assert.equal(app.toasts.filter(value => value === 'New project created').length, 3);
});

test('visible New Project denies sync false, promised false, and rejection without a legacy writer', async () => {
  for (const answer of [() => false, () => Promise.resolve(false), () => Promise.reject(new Error('native unavailable'))]) {
    const app = harness();
    const form = newProjectForm(app);
    const create = app.elements.get('np-create');
    const tabs = app.elements.get('project-tabs-list');
    app.window.NemoNativeOpacityLegacySurface = {
      allowProjectTransition() { return true; }, releaseProjectTransition() { return answer(); },
    };
    const before = { json: app.json, label: app.project.getCurrentLabel(), tabs: tabs.children.length,
      mutations: app.mutations, downloads: app.downloads.length, writes: app.writes.length };
    await create.listeners.click();
    assert.deepEqual({ json: app.json, label: app.project.getCurrentLabel(), tabs: tabs.children.length,
      mutations: app.mutations, downloads: app.downloads.length, writes: app.writes.length }, before);
    assert.equal(form.style.display, 'block', 'denial keeps the entered form visible');
    assert.equal(app.startScreen.classList.contains('hid'), false, 'denial never reveals an editor');
    assert.equal(app.elements.get('np-name').value, 'Keep fields');
    assert.equal(app.elements.get('np-preset').value, '1280x720');
    assert.equal(app.elements.get('np-fps').value, '30');
    assert.match(admissionMessage(form).textContent, /unavailable|denied/i);
    assert.equal(admissionMessage(form).style.display, 'block');
    assert.equal(app.toasts.includes('New project created'), false);
  }
});

test('a neutral browser transition still denies before the first legacy document mutation', async () => {
  const app = harness();
  const form = newProjectForm(app);
  app.window.n20AllowLegacyWrite = () => false;
  const before = { json: app.json, label: app.project.getCurrentLabel(), mutations: app.mutations,
    writes: app.writes.length, tabs: app.elements.get('project-tabs-list').children.length };
  await app.elements.get('np-create').listeners.click();
  assert.deepEqual({ json: app.json, label: app.project.getCurrentLabel(), mutations: app.mutations,
    writes: app.writes.length, tabs: app.elements.get('project-tabs-list').children.length }, before);
  assert.equal(form.style.display, 'block');
  assert.equal(admissionMessage(form).style.display, 'block');
  assert.equal(app.toasts.includes('New project created'), false);
});

test('visible New Project admits one legacy creation and ignores a stale delayed completion', async () => {
  const app = harness();
  const form = newProjectForm(app);
  const create = app.elements.get('np-create');
  let resolveFirst;
  let releases = 0;
  app.window.NemoNativeOpacityLegacySurface = {
    allowProjectTransition() { return true; },
    releaseProjectTransition() { releases++; return releases === 1 ? new Promise(resolve => { resolveFirst = resolve; }) : null; },
  };
  const pending = create.listeners.click();
  create.listeners.click();
  assert.equal(releases, 1, 'pending click is single-flight');
  assert.equal(app.mutations, 0);
  assert.equal(form.style.display, 'block');
  await app.project.newProject({ w: 320, h: 180, fps: 24, name: 'Replacement' });
  assert.equal(app.project.getCurrentLabel(), 'Replacement (not saved)');
  const afterReplacement = app.mutations;
  resolveFirst(null);
  await pending;
  assert.equal(app.mutations, afterReplacement, 'late release cannot replace the newer document');
  assert.equal(app.project.getCurrentLabel(), 'Replacement (not saved)');
  assert.equal(app.toasts.filter(value => value === 'New project created').length, 1);

  const allowed = harness();
  const allowedForm = newProjectForm(allowed);
  allowed.window.n20AllowLegacyWrite = () => true;
  await allowed.elements.get('np-create').listeners.click();
  assert.equal(allowed.project.getCurrentLabel(), 'Keep fields (not saved)');
  assert.equal(allowed.startScreen.classList.contains('hid'), true);
  assert.equal(allowedForm.style.display, 'none');
  assert.equal(allowed.toasts.filter(value => value === 'New project created').length, 1);
});

test('cancelling a pending Create prevents its later release from creating a project', async () => {
  const app = harness();
  const form = newProjectForm(app);
  let resolveAdmission;
  app.window.NemoNativeOpacityLegacySurface = {
    allowProjectTransition() { return true; },
    releaseProjectTransition() { return new Promise(resolve => { resolveAdmission = resolve; }); },
  };
  const pending = app.elements.get('np-create').listeners.click();
  app.elements.get('np-cancel').listeners.click();
  resolveAdmission(null);
  await pending;
  assert.equal(form.style.display, 'none');
  assert.equal(app.mutations, 0);
  assert.equal(app.writes.length, 0);
  assert.equal(app.project.getCurrentLabel(), 'Untitled (not saved)');
  assert.equal(app.toasts.includes('New project created'), false);
});

test('a stale rejection cannot unlock a newer pending Create', async () => {
  const app = harness();
  const form = newProjectForm(app);
  const create = app.elements.get('np-create');
  let rejectFirst, resolveSecond, releases = 0;
  app.window.NemoNativeOpacityLegacySurface = {
    allowProjectTransition() { return true; },
    releaseProjectTransition() {
      releases++;
      if (releases === 1) return new Promise((resolve, reject) => { rejectFirst = reject; });
      if (releases === 2) return new Promise(resolve => { resolveSecond = resolve; });
      return false;
    },
  };
  const first = create.listeners.click();
  app.elements.get('np-cancel').listeners.click();
  app.elements.get('start-new').listeners.click();
  const second = create.listeners.click();
  rejectFirst(new Error('old admission rejected'));
  await first;
  create.listeners.click();
  assert.equal(releases, 2, 'old rejection must not unlock a third Create');
  assert.equal(form.style.display, 'block');
  assert.equal(app.toasts.includes('New project created'), false);
  resolveSecond(null);
  await second;
  assert.equal(app.project.getCurrentLabel(), 'Keep fields (not saved)');
  assert.equal(app.toasts.filter(value => value === 'New project created').length, 1);
});

test('post-admission creation failure reports possible partial state instead of admission denial', async () => {
  const app = harness();
  const form = newProjectForm(app);
  app.window.n20AllowLegacyWrite = () => true;
  app.window.SM.fitCanvas = () => { throw new Error('viewport failed after layer creation'); };
  await app.elements.get('np-create').listeners.click();
  assert.ok(app.mutations > 0, 'the failure occurs after the first document mutation');
  assert.equal(form.style.display, 'block');
  assert.match(admissionMessage(form).textContent, /creation failed.*may have changed/i);
  assert.doesNotMatch(admissionMessage(form).textContent, /admission.*denied/i);
  assert.equal(app.toasts.includes('New project created'), false);
});

test('real project dirty consumers preserve exact bytes and reuse supplied autosave bytes', () => {
  const app = harness({ dirtyDot: true });
  const saved = '{"label":"☃\\r\\n","opacity":[20,80]}\r\n';
  app.json = saved;
  app.project.markSaved();
  assert.equal(app.project.isDirty(), false);
  app.json = saved + ' ';
  assert.equal(app.project.isDirty(), true);
  app.json = saved;
  assert.equal(app.project.isDirty(), false, 'undo to saved bytes is clean');
  app.window.SM.exportJSON = () => { throw Error('native persistence unavailable'); };
  assert.equal(app.project.isDirty(), true, 'failed read must guard close');
  app.project.refreshActiveTabDirtyDot(saved);
  assert.equal(app.dot.classList.contains('dirty'), false);
  app.project.refreshActiveTabDirtyDot(saved + ' ');
  assert.equal(app.dot.classList.contains('dirty'), true);
});

test('native save failure preserves dirty baseline and skips all legacy frame writes', async () => {
  const app = harness();
  app.json = '{"saved":true}';
  app.project.markSaved();
  const edited = '{"saved":false,"opacity":[20,80]}';
  const writes = [];
  app.window.NemoNativeOpacityCutover = { blocksLegacy: () => true, isActive: () => true,
    identity: () => ({ instanceId: 'native-i', documentId: 'native-d', contentRevision: 7 }),
    getNativeIdentity: () => ({ documentId: 'native-d', generation: 1 }),
    persistenceJSON: () => edited };
  app.window.SM.exportJSON = () => { throw Error('legacy exporter forbidden'); };
  app.window.__TAURI__ = {
    core: { invoke() {} },
    dialog: { save: async () => '/disposable/edited.json' },
    fs: { writeTextFile: async (path, bytes) => { writes.push([path, bytes]); throw Error('disk unavailable'); },
      rename: async () => { throw Error('must not rename'); }, remove: async () => {} },
  };
  const before = app.mutations;
  await assert.rejects(app.project.saveAs(), /disk unavailable/);
  assert.equal(app.project.isDirty(), true);
  assert.equal(app.project.getCurrentLabel(), 'Untitled (not saved)');
  assert.equal(app.mutations, before);
  assert.deepEqual(writes, [['/disposable/edited.json.saving', edited], ['/disposable/edited.json', edited]]);
  assert.ok(app.toasts.some(message => message.includes('disk unavailable')));
});

test('real tab switch and active close retain clean versus dirty normalized baselines', () => {
  for (const dirty of [false, true]) {
    const app = harness();
    app.project.enterEditor();
    app.json = '{"title":"first"}';
    app.project.markSaved();
    if (dirty) app.json = '{"title":"first","edited":true}';
    const firstId = app.elements.get('project-tabs-list').children[0].dataset.tab;
    app.elements.get('project-tab-add').listeners.click();
    app.json = '{"title":"second"}';
    app.project.markSaved();
    app.elements.get('project-tabs-list').children.find(row => row.dataset.tab === firstId).listeners.click();
    assert.equal(JSON.parse(app.json).normalized, true);
    assert.equal(app.project.isDirty(), dirty, 'normalized import preserves first-tab dirty flag');
    // Switching back captures first-tab status; closing second restores first
    // through the separate closeTab baseline assignment.
    app.elements.get('project-tabs-list').children.find(row => row.dataset.tab !== firstId).listeners.click();
    const activeSecond = app.elements.get('project-tabs-list').children.find(row => row.dataset.tab !== firstId);
    activeSecond.children[3].listeners.click({ stopPropagation() {} });
    assert.equal(app.elements.get('project-tabs-list').children.length, 1);
    assert.equal(app.project.isDirty(), dirty, 'active close preserves target-tab dirty flag');
  }
});

function versionRestoreHarness() {
  const shell = JSON.parse(fs.readFileSync('tests/animation/fixtures/curve-workflow.json', 'utf8'));
  shell.layers[0].motionStatic = { opacity: [60] };
  const previous = JSON.stringify(shell);
  shell.layers[0].motionStatic.opacity = [40];
  const version = JSON.stringify(shell);
  const app = harness({ version, deferFrames: true, nativeGuard: true });
  app.json = previous;
  let current = previous, identity = { instanceId: 'native-instance', documentId: 'A', contentRevision: 2 };
  const writes = [], visible = deferred(), first = { owner: 'native', status: 'presented', frame: 0,
    instanceId: 'native-instance', documentId: 'B', contentRevision: 0,
    lifecycleGeneration: 2, documentSnapshotId: 'snapshot-B' };
  app.window.NemoNativeOpacityLegacyAdmission = { allow: () => false };
  app.window.NemoNativeOpacityCutover = { blocksLegacy: () => true, isActive: () => true,
    identity: () => identity, persistenceJSON: () => current };
  app.project.markSaved();
  app.window.SM.exportJSON = () => { throw Error('legacy export forbidden'); };
  Object.assign(app.window.__TAURI__, { path: { appDataDir: async () => '/owned-app-data' } });
  Object.assign(app.window.__TAURI__.fs, { mkdir: async () => {},
    writeTextFile: async (path, bytes) => writes.push([path, bytes]), readDir: async () => [] });
  app.window.NemoNativeOpacityProject = {
    importJSON: async (bytes, silent, allowOccluded) => {
      assert.equal(bytes, version); assert.equal(silent, true); assert.equal(allowOccluded, true);
      current = version; identity = { ...identity, documentId: 'B', contentRevision: 0 }; return first;
    },
    finishOpenAfterReveal: receipt => {
      assert.equal(receipt, first);
      return visible.promise.then(final => {
        if (final.status !== 'presented' || final.documentId !== first.documentId ||
            final.contentRevision !== first.contentRevision || final.lifecycleGeneration !== first.lifecycleGeneration)
          throw Error('native final presentation failed');
        return final;
      });
    },
  };
  app.element('history-modal').style.display = 'flex';
  return { app, previous, version, writes, visible, first, get identity() { return identity; } };
}

test('native Version Restore never reaches legacy writers and waits for final presentation before backup/success', async () => {
  const h = versionRestoreHarness();
  const pending = h.app.project.restoreVersion('/owned-history/version.json');
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(h.app.mutations, 0);
  assert.equal(h.writes.length, 0);
  assert.equal(h.app.toasts.includes('toastVersionRestored'), false);
  assert.equal(h.app.pendingFrames, 1);
  h.app.flushFrame(); h.app.flushFrame();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(h.writes.length, 0);
  h.visible.resolve({ ...h.first, status: 'presented' });
  assert.equal(await pending, true);
  assert.equal(h.writes.length, 1);
  assert.equal(h.writes[0][1], h.previous, 'backup is previous native pin, never a legacy export or restored version');
  assert.match(h.writes[0][0], /^\/owned-app-data\/history\/untitled-autosave\/\d+\.json$/);
  assert.equal(h.app.mutations, 0);
  assert.equal(h.app.project.isDirty(), true, 'version restoration does not mark the new content saved');
  assert.equal(h.app.project.getCurrentLabel(), 'Untitled (not saved)');
  assert.equal(h.app.element('history-modal').style.display, 'none');
});

test('native Version Restore final failure restores modal visibility without backup or false success', async () => {
  const h = versionRestoreHarness();
  const pending = h.app.project.restoreVersion('/owned-history/version.json');
  await new Promise(resolve => setImmediate(resolve));
  h.app.flushFrame(); h.app.flushFrame();
  h.visible.resolve({ ...h.first, status: 'deferred-occluded' });
  assert.equal(await pending, false);
  assert.equal(h.writes.length, 0);
  assert.equal(h.app.mutations, 0);
  assert.equal(h.app.toasts.includes('toastVersionRestored'), false);
  assert.equal(h.app.element('history-modal').style.display, 'flex');
  assert.equal(h.identity.documentId, 'B', 'test does not manufacture rollback to A after native replacement');
});

test('desktop Version Restore with no published native globals denies before every legacy or file effect', async () => {
  const version = fs.readFileSync('tests/animation/fixtures/curve-workflow.json', 'utf8');
  const app = harness({ version });
  let reads = 0, exports = 0;
  app.window.__TAURI__.core = { invoke: async () => { throw Error('must not invoke'); } };
  app.window.__TAURI__.fs.readTextFile = async () => { reads++; return version; };
  app.window.SM.exportJSON = () => { exports++; return '{"keep":true}'; };
  const before = app.json;
  assert.equal(await app.project.restoreVersion('/owned-history/version.json'), false);
  assert.equal(reads, 0, 'missing native pin denies before file read');
  assert.equal(exports, 0);
  assert.equal(app.mutations, 0);
  assert.equal(app.writes.length, 0);
  assert.equal(app.json, before);
  assert.equal(app.element('project-tabs-list').children.length, 0);
  assert.equal(app.toasts.includes('toastVersionRestored'), false);
});

test('native Version Restore rejected import preserves the existing document, baseline and modal', async () => {
  const h = versionRestoreHarness();
  const before = { identity: { ...h.identity }, label: h.app.project.getCurrentLabel(), dirty: h.app.project.isDirty() };
  h.app.window.NemoNativeOpacityProject.importJSON = async () => false;
  assert.equal(await h.app.project.restoreVersion('/owned-history/version.json'), false);
  assert.deepEqual(h.identity, before.identity);
  assert.equal(h.app.project.getCurrentLabel(), before.label);
  assert.equal(h.app.project.isDirty(), before.dirty);
  assert.equal(h.app.mutations, 0);
  assert.equal(h.writes.length, 0);
  assert.equal(h.app.pendingFrames, 0);
  assert.equal(h.app.element('history-modal').style.display, 'flex');
  assert.equal(h.app.toasts.includes('toastVersionRestored'), false);
});

const BASELINE_BYTES = '{"native":"saved-content"}', EDITED_BYTES = '{"native":"edited-content"}';
function baselineHarness(change = () => {}, options = {}) {
  const calls = { paper: 0, baselines: [], destroys: 0, asks: 0 };
  const current = { bytes: BASELINE_BYTES, revision: 0 };
  const app = harness({ ...options, configureWindow(root) {
    root.SM.exportJSON = () => { calls.paper++; return '{"paper":"unrelated"}'; };
    const original = root.NemoProjectDirtyBaseline;
    root.NemoProjectDirtyBaseline = { create() {
      const real = original.create();
      return { ...real, markSaved(bytes) { calls.baselines.push(bytes); real.markSaved(bytes); } };
    } };
    root.__TAURI__ = { core: { invoke() {} }, dialog: { ask: async () => { calls.asks++; return false; } },
      window: { getCurrentWindow: () => ({ onCloseRequested(callback) { calls.close = callback; },
        async destroy() { calls.destroys++; } }) } };
    root.NemoNativeOpacityCutover = { blocksLegacy: () => true, isActive: () => true,
      identity: () => ({ instanceId: 'instance-baseline', documentId: 'document-baseline', contentRevision: current.revision }),
      persistenceJSON: () => current.bytes };
    change(root, current, calls);
  } });
  return { app, calls, current };
}
for (const [name, change, bootstrapFailure] of [
  ['object/no opacity owner', root => { delete root.NemoNativeOpacityCutover; }],
  ['malformed API', root => { root.__TAURI__ = {}; }],
  ['packaged API absent', root => { delete root.__TAURI__; root.location = { protocol: 'tauri:', hostname: 'localhost', port: '' }; }],
  ['throwing API evidence', root => { Object.defineProperty(root, '__TAURI__', { get() { throw Error('API unavailable'); } }); }, true],
  ...['closed', 'replacing', 'indeterminate', 'synchronizing'].map(name => [name, root => { root.NemoNativeOpacityCutover.isActive = () => false; }]),
  ['unreadable persistence', root => { root.NemoNativeOpacityCutover.persistenceJSON = () => { throw Error('native read refused'); }; }],
  ['unavailable close dialog', (root, current, calls) => {
    delete root.NemoNativeOpacityCutover;
    root.__TAURI__.dialog.ask = async () => { calls.asks++; throw Error('dialog unavailable'); };
  }],
  ['changed revision', (root, current) => { root.NemoNativeOpacityCutover.persistenceJSON = () => { current.revision++; return current.bytes; }; }],
]) {
  test(`actual boot and exposed markSaved refuse ${name} before Paper/baseline effects`, async () => {
    const h = baselineHarness(change, { allowBootstrapFailure: !!bootstrapFailure });
    assert.equal(!!h.app.bootstrapError, !!bootstrapFailure, 'bootstrap errors remain explicit');
    assert.equal(h.calls.paper, 0);
    assert.deepEqual(h.calls.baselines, []);
    assert.equal(h.app.project.isDirty(), true, 'uninitialized unavailable baseline cannot be clean');
    h.app.project.markSaved();
    assert.equal(h.calls.paper, 0);
    assert.deepEqual(h.calls.baselines, []);
    assert.equal(h.app.project.isDirty(), true);
    if (h.calls.close) {
      let prevented = 0;
      await h.calls.close({ preventDefault() { prevented++; } });
      assert.equal(prevented, 1); assert.equal(h.calls.asks, 1); assert.equal(h.calls.destroys, 0);
    }
    assert.deepEqual(h.app.writes, []);
  });
}
test('actual native boot/exposed baseline follows native content through edit/undo/redo', () => {
  const h = baselineHarness();
  assert.deepEqual(h.calls.baselines, [BASELINE_BYTES]);
  assert.equal(h.app.project.isDirty(), false);
  h.current.bytes = EDITED_BYTES; h.current.revision++;
  assert.equal(h.app.project.isDirty(), true);
  h.current.bytes = BASELINE_BYTES; h.current.revision++;
  assert.equal(h.app.project.isDirty(), false, 'new revision with saved content is clean');
  h.current.bytes = EDITED_BYTES; h.current.revision++;
  assert.equal(h.app.project.isDirty(), true);
  h.app.project.markSaved();
  assert.equal(h.app.project.isDirty(), false);
  assert.deepEqual(h.calls.baselines, [BASELINE_BYTES, EDITED_BYTES]);
  assert.equal(h.calls.paper, 0);
  h.app.project.enterEditor();
  const currentTab = h.app.element('project-tabs-list').children[0];
  const label = h.app.project.getCurrentLabel();
  h.current.bytes = BASELINE_BYTES; h.current.revision++;
  h.app.window.NemoNativeOpacityCutover.isActive = () => false;
  h.app.project.markSaved();
  assert.deepEqual(h.calls.baselines, [BASELINE_BYTES, EDITED_BYTES], 'refusal preserves the prior saved baseline');
  assert.equal(h.app.project.getCurrentLabel(), label);
  assert.equal(h.app.element('project-tabs-list').children[0], currentTab, 'refusal leaves the active tab UI untouched');
  assert.equal(h.app.project.isDirty(), true);
  h.app.window.NemoNativeOpacityCutover.isActive = () => true;
  assert.equal(h.app.project.isDirty(), true, 'old content stays dirty against the last accepted baseline');
});
test('desktop New without the native object adapter refuses before legacy construction or persistence', async () => {
  const h = baselineHarness(root => {
    delete root.NemoNativeOpacityCutover;
    // A permissive legacy surface cannot bypass the ordinary native caller.
    root.NemoNativeOpacityLegacySurface = { allowProjectTransition: () => true, releaseProjectTransition: () => null };
    root.n20AllowLegacyWrite = () => true;
  });
  const form = newProjectForm(h.app);
  await h.app.elements.get('np-create').listeners.click();
  assert.equal(h.app.mutations, 0, 'native New cannot construct a legacy document');
  assert.equal(h.calls.paper, 0); assert.deepEqual(h.calls.baselines, []);
  assert.deepEqual(h.app.writes, []);
  assert.equal(h.app.project.isDirty(), true);
  assert.equal(h.app.toasts.includes('New project created'), false);
  assert.match(admissionMessage(form).textContent, /unavailable.*admission was denied/i);
});
test('ordinary browser New stamps current browser bytes and keeps autosave ordering', async () => {
  const app = harness();
  app.json = '{"browser":"new"}';
  const form = newProjectForm(app);
  await app.elements.get('np-create').listeners.click();
  assert.equal(app.project.isDirty(), false);
  assert.equal(app.toasts.includes('New project created'), true);
  assert.equal(form.style.display, 'none');
  assert.deepEqual(app.writes, [['nemo-auto', '{"browser":"new"}']]);
});
test('browser post-creation serialization failure cannot retain an old clean baseline', async () => {
  const app = harness();
  const form = newProjectForm(app);
  app.window.SM.exportJSON = () => { throw Error('browser serialization failed'); };
  await app.elements.get('np-create').listeners.click();
  assert.ok(app.mutations > 0);
  assert.equal(app.project.isDirty(), true);
  assert.deepEqual(app.writes, []);
  assert.equal(app.toasts.includes('New project created'), false);
  assert.match(admissionMessage(form).textContent, /creation failed.*may have changed/i);
});
