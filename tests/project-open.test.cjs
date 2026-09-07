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

function harness({ auto = null, version = null } = {}) {
  const elements = new Map();
  const downloads = [], toasts = [];
  let repaints = 0;
  let json = JSON.stringify({ title: 'boot' });
  function element() {
    const el = { style: {}, dataset: {}, value: '', files: [], children: [], classList: { add() {}, remove() {}, toggle() {} },
      addEventListener(type, fn) { this.listeners ||= {}; this.listeners[type] = fn; }, appendChild(child) { this.children.push(child); },
      removeChild() {}, click() { this.clicked = true; } };
    Object.defineProperty(el, 'innerHTML', { get() { return ''; }, set() { this.children = []; } });
    return el;
  }
  const document = { readyState: 'loading', body: element(), addEventListener(type, fn) { if (type === 'DOMContentLoaded') this.ready = fn; },
    getElementById(id) { if (!elements.has(id)) elements.set(id, element()); return elements.get(id); }, createElement: element, querySelector() { return null; } };
  let rejectNextImport = false;
  const window = { addEventListener() {}, SM: { t(key) { return key; }, fitCanvas() {}, exportJSON() { return json; }, importJSON(raw) {
    if (rejectNextImport) { rejectNextImport = false; return false; }
    try { const parsed = JSON.parse(raw); if (parsed.fail) return false; json = JSON.stringify({ ...parsed, normalized: true }); return true; } catch (_) { return false; }
  } } };
  class Reader { readAsText(file) { if (file.error) this.onerror(new Error('read failed')); else this.onload({ target: { result: file.text } }); } }
  const context = { window, SM: window.SM, document, FileReader: Reader, Blob: class { constructor(parts) { this.parts = parts; } }, URL: { createObjectURL() { return 'blob:test'; }, revokeObjectURL() {} },
    localStorage: { getItem(key) { return key === 'nemo-auto' ? auto : null; }, setItem() {}, removeItem() {} }, state: {}, userLayers: [], _symbolPaperLayers: {}, showToast(message) { toasts.push(message); },
    requestAnimationFrame(fn) { fn(); }, view: { update() { repaints++; } }, saveAllLayerFrames() {}, createUserLayer() {}, activateUL() {}, drawStage() {}, loadFrame() {}, renderOS() {}, renderArcs() {}, updateUI() {}, renderSymbolTabs() {}, syncDocFields() {}, exitToScene() {}, setTimeout, console };
  if (version !== null) window.__TAURI__ = { fs: { readTextFile: async () => version } };
  vm.runInNewContext(documentSource, context, { filename: 'project-document.js' });
  vm.runInNewContext(entrySource, context, { filename: 'project-entry.js' });
  context.SMProjectEntry = window.SMProjectEntry;
  document.body.appendChild = node => downloads.push(node.download);
  vm.runInNewContext(source, context, { filename: 'project.js' });
  document.ready();
  return { input: document.getElementById('file-input'), downloads, toasts, project: window.SMProject, elements, rejectNextImport() { rejectNextImport = true; }, get json() { return json; }, set json(value) { json = value; }, get repaints() { return repaints; } };
}

function select(app, file) { app.input.listeners.change({ target: { files: file ? [file] : [], value: 'selected' } }); }

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
