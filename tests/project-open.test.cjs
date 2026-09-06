'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const test = require('node:test');
const vm = require('node:vm');

const source = fs.readFileSync('src/js/project.js', 'utf8');

function harness() {
  const elements = new Map();
  const downloads = [], toasts = [];
  let json = JSON.stringify({ title: 'boot' });
  function element() {
    return { style: {}, dataset: {}, value: '', files: [], children: [], classList: { add() {}, remove() {}, toggle() {} },
      addEventListener(type, fn) { this.listeners ||= {}; this.listeners[type] = fn; }, appendChild(child) { this.children.push(child); },
      removeChild() {}, click() { this.clicked = true; } };
  }
  const document = { readyState: 'loading', body: element(), addEventListener(type, fn) { if (type === 'DOMContentLoaded') this.ready = fn; },
    getElementById(id) { if (!elements.has(id)) elements.set(id, element()); return elements.get(id); }, createElement: element, querySelector() { return null; } };
  const window = { addEventListener() {}, SM: { t(key) { return key; }, exportJSON() { return json; }, importJSON(raw) {
    const parsed = JSON.parse(raw); if (parsed.fail) throw new Error('bad project'); json = JSON.stringify({ ...parsed, normalized: true });
  } } };
  class Reader { readAsText(file) { if (file.error) this.onerror(new Error('read failed')); else this.onload({ target: { result: file.text } }); } }
  const context = { window, SM: window.SM, document, FileReader: Reader, Blob: class { constructor(parts) { this.parts = parts; } }, URL: { createObjectURL() { return 'blob:test'; }, revokeObjectURL() {} },
    localStorage: { getItem() { return null; }, setItem() {}, removeItem() {} }, state: {}, userLayers: [], _symbolPaperLayers: {}, showToast(message) { toasts.push(message); },
    saveAllLayerFrames() {}, createUserLayer() {}, activateUL() {}, drawStage() {}, loadFrame() {}, renderOS() {}, renderArcs() {}, updateUI() {}, renderSymbolTabs() {}, syncDocFields() {}, exitToScene() {}, setTimeout, console };
  document.body.appendChild = node => downloads.push(node.download);
  vm.runInNewContext(source, context, { filename: 'project.js' });
  document.ready();
  return { input: document.getElementById('file-input'), downloads, toasts, project: window.SMProject, get json() { return json; }, set json(value) { json = value; } };
}

function select(app, file) { app.input.listeners.change({ target: { files: file ? [file] : [], value: 'selected' } }); }

test('browser Open keeps the file name and normalized clean baseline for later Save', async () => {
  const app = harness();
  select(app, { name: 'Story.JSON', text: '{"title":"story"}' });
  assert.equal(app.project.getCurrentLabel(), 'Story (not saved)');
  assert.equal(app.project.isDirty(), false, 'normalized import is clean');
  await app.project.save();
  assert.equal(app.downloads.at(-1), 'Story.json', 'unchanged browser Save retains opened basename');
  app.json = JSON.stringify({ title: 'story', normalized: true, edited: true });
  assert.equal(app.project.isDirty(), true, 'an actual edit is dirty');
  await app.project.save();
  assert.equal(app.downloads.at(-1), 'Story.json', 'edited browser Save still retains opened basename');
});

test('browser Open leaves the current document alone for cancellation and failed reads', () => {
  const app = harness();
  select(app, { name: 'Keep.json', text: '{"title":"keep"}' });
  const before = { label: app.project.getCurrentLabel(), json: app.json, dirty: app.project.isDirty() };
  select(app, null);
  assert.deepEqual({ label: app.project.getCurrentLabel(), json: app.json, dirty: app.project.isDirty() }, before);
  select(app, { name: 'Broken.json', error: true });
  assert.deepEqual({ label: app.project.getCurrentLabel(), json: app.json, dirty: app.project.isDirty() }, before);
  select(app, { name: 'Broken.json', text: '{"fail":true}' });
  assert.deepEqual({ label: app.project.getCurrentLabel(), json: app.json, dirty: app.project.isDirty() }, before);
  assert.match(app.toasts.at(-1), /Could not open file/);
});
