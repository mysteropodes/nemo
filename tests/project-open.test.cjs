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

function harness({ auto = null, version = null, deferFrames = false } = {}) {
  const elements = new Map();
  const downloads = [], toasts = [], frames = [], writes = [];
  let repaints = 0, mutations = 0;
  let json = JSON.stringify({ title: 'boot' });
  function element() {
    const classes = new Set();
    const el = { style: {}, dataset: {}, value: '', files: [], children: [], classList: {
      add(name) { classes.add(name); }, remove(name) { classes.delete(name); },
      toggle(name) { if (classes.has(name)) classes.delete(name); else classes.add(name); },
      contains(name) { return classes.has(name); } },
      addEventListener(type, fn) { this.listeners ||= {}; this.listeners[type] = fn; }, appendChild(child) { this.children.push(child); },
      setAttribute(name, value) { this.attributes ||= {}; this.attributes[name] = value; },
      getAttribute(name) { return this.attributes && this.attributes[name]; },
      removeChild() {}, click() { this.clicked = true; } };
    Object.defineProperty(el, 'innerHTML', { get() { return ''; }, set() { this.children = []; } });
    return el;
  }
  const document = { readyState: 'loading', body: element(), addEventListener(type, fn) { if (type === 'DOMContentLoaded') this.ready = fn; },
    getElementById(id) { if (!elements.has(id)) elements.set(id, element()); return elements.get(id); }, createElement: element, querySelector() { return null; } };
  let rejectNextImport = false;
  const window = { NemoNativeOpacityLegacySurface: require('../src/js/adapters/native-opacity-legacy-surface.js'), addEventListener() {}, SM: { t(key) { return key; }, fitCanvas() {}, exportJSON() { return json; }, importJSON(raw) {
    if (rejectNextImport) { rejectNextImport = false; return false; }
    try { const parsed = JSON.parse(raw); if (parsed.fail) return false; json = JSON.stringify({ ...parsed, normalized: true }); return true; } catch (_) { return false; }
  } } };
  window.NemoNativeOpacityProjectEntry = require('../src/js/adapters/native-opacity-project-entry.js');
  class Reader { readAsText(file) { if (file.error) this.onerror(new Error('read failed')); else this.onload({ target: { result: file.text } }); } }
  const context = { window, SM: window.SM, document, FileReader: Reader, Blob: class { constructor(parts) { this.parts = parts; } }, URL: { createObjectURL() { return 'blob:test'; }, revokeObjectURL() {} },
    localStorage: { getItem(key) { return key === 'nemo-auto' ? auto : null; }, setItem(key, value) { writes.push([key, value]); }, removeItem() {} }, state: {}, userLayers: [], _symbolPaperLayers: {}, showToast(message) { toasts.push(message); },
    requestAnimationFrame(fn) { if (deferFrames) frames.push(fn); else fn(); }, view: { update() { repaints++; } }, saveAllLayerFrames() { mutations++; }, createUserLayer() { mutations++; }, activateUL() {}, drawStage() {}, loadFrame() {}, renderOS() {}, renderArcs() {}, updateUI() {}, renderSymbolTabs() {}, syncDocFields() {}, exitToScene() {}, setTimeout, console };
  window.requestAnimationFrame = function (fn) {
    assert.strictEqual(this, window, 'WebKit requestAnimationFrame requires a Window receiver');
    return context.requestAnimationFrame(fn);
  };
  if (version !== null) window.__TAURI__ = { fs: { readTextFile: async () => version } };
  vm.runInNewContext(documentSource, context, { filename: 'project-document.js' });
  vm.runInNewContext(entrySource, context, { filename: 'project-entry.js' });
  context.SMProjectEntry = window.SMProjectEntry;
  document.body.appendChild = node => downloads.push(node.download);
  vm.runInNewContext(source, context, { filename: 'project.js' });
  document.ready();
  return { window, input: document.getElementById('file-input'), startScreen: document.getElementById('start-screen'),
    element(id) { return document.getElementById(id); },
    downloads, toasts, writes, project: window.SMProject, elements,
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
test('production loads the native project-entry adapter before its project consumer', () => {
  const html = fs.readFileSync('src/index.html', 'utf8');
  const adapter = html.indexOf('js/adapters/native-opacity-project-entry.js');
  const project = html.indexOf('js/project.js', adapter);
  assert.ok(adapter >= 0 && project > adapter);
});
function deferred() { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }
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
          result.documentSnapshotId !== incoming.documentSnapshotId) {
        throw new Error('Native viewport is not current after reveal');
      }
      return result;
    });
  } };
  return { receipt, first, visible, get presentations() { return presentations; },
    get deferredAllowed() { return deferredAllowed; } };
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

test('deferred native first frame remains unpublished until post-reveal presentation', async () => {
  const app = harness({ deferFrames: true }), native = installNativeOpen(app);
  const first = { ...native.receipt, status: 'deferred-occluded' };
  select(app, { name: 'Occluded.json', text: '{"supported":true}' });
  native.first.resolve(first);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(app.startScreen.classList.contains('hid'), true);
  assert.equal(app.project.getCurrentLabel(), 'Untitled (not saved)');
  app.flushFrame(); app.flushFrame();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(native.presentations, 1);
  native.visible.resolve({ ...native.receipt, workId: 'visible-frame' });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(app.project.getCurrentLabel(), 'Occluded (not saved)');
  assert.equal(app.toasts.at(-1), 'Opened: Occluded');
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
  const app = harness({ auto: '{"fail":true}', version: '{"layers":[{"frames":[{"strokes":[]}]}]}' });
  const beforeResume = { label: app.project.getCurrentLabel(), json: app.json, dirty: app.project.isDirty() };
  app.elements.get('start-resume').listeners.click();
  assert.deepEqual({ label: app.project.getCurrentLabel(), json: app.json, dirty: app.project.isDirty() }, beforeResume);
  assert.match(app.toasts.at(-1), /toastCannotResumeSessionCorrupt/);
  assert.equal(app.toasts.includes('Session resumed'), false);

  select(app, { name: 'Keep.json', text: '{"title":"keep"}' });
  app.json = JSON.stringify({ title: 'keep', normalized: true, edited: true });
  const beforeRestore = { label: app.project.getCurrentLabel(), json: app.json, dirty: app.project.isDirty() };
  app.rejectNextImport();
  assert.equal(await app.project.restoreVersion('/history/rejected.json'), false);
  assert.deepEqual({ label: app.project.getCurrentLabel(), json: app.json, dirty: app.project.isDirty() }, beforeRestore);
  assert.equal(app.toasts.includes('toastVersionRestored'), false);

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
