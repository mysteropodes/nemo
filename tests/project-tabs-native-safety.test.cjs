'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const test = require('node:test');
const vm = require('node:vm');
const source = fs.readFileSync('src/js/project.js', 'utf8');
const A = '{"project":"A","paint":[20,80]}', B = '{"project":"B","paint":[40,60]}';
const EDITED_A = '{"project":"A","paint":[25,85],"edited":true}';

// Runs the complete shipped project entry and its real tab DOM listeners.
// Native modes are introduced only after disposable browser A/B setup;
// this is a source fixture, never a production native-tab setup mechanism.
function fixture({ populated = true } = {}) {
  const effects = [], notices = [], elements = new Map();
  let bytes = A;
  function element() {
    const classes = new Set();
    const el = { children: [], style: {}, dataset: {}, listeners: {}, value: '', className: '', attributes: {},
      addEventListener(type, callback) { this.listeners[type] = callback; },
      appendChild(child) { this.children.push(child); }, setAttribute(name, value) { this.attributes[name] = value; },
      getAttribute(name) { return this.attributes[name]; }, removeChild() {}, click() { this.listeners.click?.(); } };
    el.classList = { add: name => classes.add(name), remove: name => classes.delete(name),
      contains: name => classes.has(name) || el.className.split(' ').includes(name),
      toggle(name, force) { if (force === false || force === undefined && classes.has(name)) classes.delete(name); else classes.add(name); } };
    Object.defineProperty(el, 'innerHTML', { get: () => '', set() { el.children = []; } });
    return el;
  }
  const document = { readyState: 'loading', body: element(),
    addEventListener(type, callback) { if (type === 'DOMContentLoaded') this.ready = callback; },
    getElementById(id) { if (!elements.has(id)) elements.set(id, element()); return elements.get(id); },
    createElement: element, querySelector(selector) {
      const match = selector.match(/data-tab="([^"]+)"/);
      return match ? elements.get('project-tabs-list')?.children.find(row => row.dataset.tab === match[1]) : null;
    } };
  const root = { location: { protocol: 'http:', hostname: 'localhost', port: '1420' },
    addEventListener() {}, requestAnimationFrame(callback) { callback(); },
    NemoNativeOpacityProjectEntry: require('../src/js/adapters/native-opacity-project-entry.js'),
    NemoNativeOpacityLegacySurface: require('../src/js/adapters/native-opacity-legacy-surface.js'),
    NemoProjectNativeSave: require('../src/js/adapters/project-native-save.js'),
    NemoProjectDirtyBaseline: require('../src/js/adapters/project-dirty-baseline.js'),
    NemoProjectNativeVersionRestore: require('../src/js/adapters/project-native-version-restore.js'),
    SM: { t: key => key, fitCanvas() {}, exportJSON() { effects.push('Paper export'); return bytes; },
      importJSON(json) { effects.push('browser import'); bytes = JSON.stringify({ ...JSON.parse(json), normalized: true }); return true; } } };
  const noop = () => {};
  const sandbox = { window: root, document, SM: root.SM, state: {}, userLayers: [], _symbolPaperLayers: {},
    localStorage: { getItem: () => null, setItem() { effects.push('storage'); }, removeItem() {} },
    showToast(message) { notices.push(message); }, saveAllLayerFrames() { effects.push('Paper frame save'); },
    createUserLayer() { effects.push('create layer'); }, activateUL: noop, drawStage: noop, loadFrame: noop,
    renderOS: noop, renderArcs: noop, updateUI: noop, renderSymbolTabs: noop, syncDocFields: noop, exitToScene: noop,
    view: { update: noop }, setTimeout, console,
    FileReader: class { readAsText(file) { this.onload({ target: { result: file.text } }); } } };
  vm.runInNewContext(fs.readFileSync('src/js/project-document.js', 'utf8'), sandbox);
  vm.runInNewContext(fs.readFileSync('src/js/project-entry.js', 'utf8'), sandbox);
  sandbox.SMProjectEntry = root.SMProjectEntry;
  vm.runInNewContext(source, sandbox, { filename: 'project.js' });
  document.ready();
  root.SMProject.newProject({ name: 'A', w: 320, h: 180, fps: 12 });
  const list = () => document.getElementById('project-tabs-list').children;
  const a = list()[0].dataset.tab;
  document.getElementById('project-tab-add').listeners.click();
  const b = list()[1].dataset.tab;
  if (populated) {
    bytes = B; root.SMProject.markSaved();
    row(a).listeners.click();
    bytes = EDITED_A;
    root.SMProject.refreshActiveTabDirtyDot(EDITED_A);
  }
  function row(id) { return list().find(node => node.dataset.tab === id); }
  function state() {
    return { bytes, label: root.SMProject.getCurrentLabel(), open: root.SMProject.getOpenTabs(),
      rows: list().map(node => ({ id: node.dataset.tab, active: node.classList.contains('act'),
        name: node.children[2].textContent, dirty: node.classList.contains('dirty') })) };
  }
  effects.length = 0; notices.length = 0;
  return { root, effects, notices, document, sandbox, a, b, row, state,
    close(id) { row(id).children[3].listeners.click({ stopPropagation() {} }); },
    switch(id) { row(id).listeners.click(); }, setBytes(value) { bytes = value; } };
}
function native(f) {
  const owner = { instanceId: 'native-i', documentId: 'native-A', generation: 3, revision: 11,
    snapshot: 'snapshot-A', frame: 7, undo: ['saved-A'], redo: [] };
  f.root.__TAURI__ = { core: { invoke() { f.effects.push('native invoke'); } } };
  f.root.NemoNativeOpacityCutover = { blocksLegacy: () => true, isActive: () => true,
    identity: () => ({ instanceId: owner.instanceId, documentId: owner.documentId, contentRevision: owner.revision }),
    persistenceJSON() { f.effects.push('native serialization'); return EDITED_A; } };
  f.root.NemoNativeOpacityProject = {
    async importJSON() { f.effects.push('native import'); owner.documentId = 'native-B'; owner.revision = 0; return { owner: 'native', status: 'presented' }; },
    finishOpenAfterReveal() { f.effects.push('native finalizer'); },
    release() { f.effects.push('native release'); return Promise.resolve({ owner: 'none', status: 'closed' }); } };
  return owner;
}
const modes = [
  ['active native', () => {}],
  ...['closed', 'replacing', 'release-requested', 'releasing', 'indeterminate'].map(name =>
    [name, f => { f.root.NemoNativeOpacityCutover.isActive = () => false; }]),
  ['missing cutover', f => { delete f.root.NemoNativeOpacityCutover; }],
  ['broken cutover', f => { f.root.NemoNativeOpacityCutover = {}; }],
  ['null cutover', f => { f.root.NemoNativeOpacityCutover = null; }],
  ['throwing cutover', f => { Object.defineProperty(f.root, 'NemoNativeOpacityCutover', { get() { throw Error('cutover inaccessible'); } }); }],
  ['desktop marker without native globals', f => { delete f.root.NemoNativeOpacityCutover; delete f.root.NemoNativeOpacityProject; }],
  ['malformed desktop API', f => { f.root.__TAURI__ = {}; delete f.root.NemoNativeOpacityProject; }],
  ['explicit undefined desktop marker', f => { f.root.__TAURI__ = undefined; delete f.root.NemoNativeOpacityProject; }],
  ['packaged origin without API/globals', f => {
    delete f.root.__TAURI__; delete f.root.NemoNativeOpacityCutover; delete f.root.NemoNativeOpacityProject;
    f.root.location = { protocol: 'tauri:', hostname: 'localhost', port: '' };
  }],
  ['browser surface with published native owner', f => { delete f.root.__TAURI__; }],
  ['browser surface with only native cutover', f => { delete f.root.__TAURI__; delete f.root.NemoNativeOpacityProject; }],
  ['browser surface with broken cutover', f => {
    delete f.root.__TAURI__; delete f.root.NemoNativeOpacityProject; f.root.NemoNativeOpacityCutover = {};
  }],
  ['browser surface with throwing cutover phase', f => {
    delete f.root.__TAURI__; delete f.root.NemoNativeOpacityProject;
    f.root.NemoNativeOpacityCutover.blocksLegacy = () => { throw Error('phase unavailable'); };
  }],
  ['malformed browser location', f => {
    delete f.root.__TAURI__; delete f.root.NemoNativeOpacityCutover; delete f.root.NemoNativeOpacityProject;
    f.root.location.hostname = 123;
  }],
  ['unknown location', f => {
    delete f.root.__TAURI__; delete f.root.NemoNativeOpacityCutover; delete f.root.NemoNativeOpacityProject; delete f.root.location;
  }],
  ['inaccessible location', f => {
    delete f.root.__TAURI__; delete f.root.NemoNativeOpacityCutover; delete f.root.NemoNativeOpacityProject;
    Object.defineProperty(f.root, 'location', { get() { throw Error('location inaccessible'); } });
  }],
];
for (const [name, change] of modes) {
  for (const action of ['switch', 'active close']) {
    test(`real ${action} refuses ${name} before every document/tab effect`, async () => {
      const f = fixture(), owner = native(f); change(f);
      const before = JSON.stringify({ owner, state: f.state() });
      if (action === 'switch') f.switch(f.b); else f.close(f.a);
      const alert = f.document.getElementById('project-tabs-bar').children.find(node => node.getAttribute('role') === 'alert');
      assert.ok(alert, 'real tab bar exposes the unavailable result');
      assert.match(alert.textContent, /unavailable/i);
      assert.equal(alert.style.display, 'block');
      assert.deepEqual(f.notices, [], 'availability alert does not add a success or duplicate toast');
      assert.deepEqual(f.effects, []);
      assert.equal(JSON.stringify({ owner, state: f.state() }), before);
      await Promise.resolve(); await new Promise(resolve => setImmediate(resolve));
      assert.deepEqual(f.effects, []);
      assert.equal(JSON.stringify({ owner, state: f.state() }), before);
    });
  }
}
for (const released of [false, true]) {
  test(`ordinary browser ${released ? 'legacy-phase' : 'without native marker'} preserves normalized switch/active close`, () => {
    const f = fixture();
    if (released) f.root.NemoNativeOpacityCutover = { blocksLegacy: () => false };
    f.switch(f.b);
    assert.equal(f.effects.filter(x => x === 'browser import').length, 1);
    assert.equal(f.state().rows.find(x => x.active).id, f.b);
    assert.equal(JSON.parse(f.state().bytes).project, 'B');
    assert.equal(JSON.parse(f.state().bytes).normalized, true);
    assert.equal(f.notices.length, 0);
    f.effects.length = 0;
    f.close(f.b);
    assert.equal(f.effects.filter(x => x === 'browser import').length, 1);
    assert.equal(f.state().rows.length, 1);
    assert.equal(f.state().rows[0].id, f.a);
    assert.equal(JSON.parse(f.state().bytes).project, 'A');
    assert.equal(f.root.SMProject.isDirty(), true, 'edited outgoing A remains dirty after normalized browser import');
  });
}
test('native same-tab click and inactive close retain their existing metadata-only behavior', () => {
  const f = fixture(), owner = native(f), before = JSON.stringify(owner);
  f.switch(f.a);
  assert.deepEqual(f.effects, []); assert.deepEqual(f.notices, []);
  f.close(f.b);
  assert.equal(f.state().rows.length, 1); assert.equal(f.state().rows[0].id, f.a);
  assert.equal(JSON.stringify(owner), before);
  assert.equal(f.effects.some(x => /import|release|frame save/.test(x)), false);
  f.effects.length = 0; f.close(f.a);
  assert.deepEqual(f.effects, []);
  assert.equal(f.notices.at(-1), 'hsCannotCloseLastTab');
});
test('ordinary browser new-tab callback retains existing creation behavior', () => {
  const f = fixture({ populated: false });
  f.document.getElementById('project-tab-add').listeners.click();
  assert.equal(f.state().rows.length, 3);
  assert.equal(f.state().rows.find(x => x.active).name, 'Untitled 3');
  assert.equal(f.effects.filter(x => x === 'create layer').length, 1);
  assert.equal(f.effects.filter(x => x === 'browser import').length, 0);
});
