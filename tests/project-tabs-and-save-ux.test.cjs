'use strict';
// #2381: Cmd+S on an untitled project asks where (Save As), a titled one saves in place; opening a file that is
// already open focuses its tab; the shortcut and the File menu reach one Save path.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), vm = require('node:vm');
const Shortcut = require('../src/js/adapters/project-save-shortcut.js');
const TabsUI = require('../src/js/adapters/native-object-project-tabs-ui.js');
const InactiveTabs = require('../src/js/adapters/native-object-inactive-project-tabs.js');

function shortcutRig({ tauri = true } = {}) {
  const calls = [], listeners = {}, menu = {};
  const document = { addEventListener(type, fn, capture) { listeners[type] = { fn, capture }; } };
  const root = { document, SMProject: { save() { calls.push('save'); return Promise.resolve(); },
    saveAs() { calls.push('saveAs'); return Promise.resolve(); } } };
  if (tauri) root.__TAURI__ = { event: { listen(name, fn) { menu[name] = fn; return Promise.resolve(() => {}); } } };
  Shortcut.install(root);
  const key = (init) => { const seen = { prevented: false, stopped: false };
    listeners.keydown.fn({ key: 's', metaKey: true, preventDefault() { seen.prevented = true; },
      stopImmediatePropagation() { seen.stopped = true; }, ...init }); return seen; };
  return { calls, listeners, menu, key };
}

test('Cmd+S runs Save once in the capture phase and stops the other key handlers', () => {
  const r = shortcutRig();
  assert.equal(r.listeners.keydown.capture, true);
  const seen = r.key({});
  assert.deepEqual(r.calls, ['save']); assert.equal(seen.prevented, true); assert.equal(seen.stopped, true);
});
test('Shift+Cmd+S (key reported as S) runs Save As; Ctrl+S saves; other chords do nothing', () => {
  let r = shortcutRig(); r.key({ key: 'S', shiftKey: true }); assert.deepEqual(r.calls, ['saveAs']);
  r = shortcutRig(); r.key({ metaKey: false, ctrlKey: true }); assert.deepEqual(r.calls, ['save']);
  r = shortcutRig(); r.key({ key: 'dead', code: 'KeyS' }); assert.deepEqual(r.calls, ['save']);
  for (const init of [{ key: 'a' }, { metaKey: false }, { altKey: true }]) {
    r = shortcutRig(); assert.equal(r.key(init).prevented, false, JSON.stringify(init)); assert.deepEqual(r.calls, []);
  }
});
test('the File menu events run Save and Save As, and one press reported by menu and key saves once', () => {
  const r = shortcutRig();
  assert.equal(Shortcut.SAVE_EVENT, 'menu-save-project'); assert.equal(Shortcut.SAVE_AS_EVENT, 'menu-save-project-as');
  r.menu[Shortcut.SAVE_EVENT](); r.key({});
  assert.deepEqual(r.calls, ['save']);
  const other = shortcutRig(); other.menu[Shortcut.SAVE_AS_EVENT](); assert.deepEqual(other.calls, ['saveAs']);
});
test('a missing project or a throwing Save never throws into the page; browser preview still handles the key', () => {
  const root = { document: { addEventListener() {} }, SMProject: { save() { throw Error('x'); } } };
  assert.doesNotThrow(() => Shortcut.install(root).run('save'));
  assert.equal(Shortcut.install({ document: { addEventListener() {} } }).run('save'), false);
  const r = shortcutRig({ tauri: false }); r.key({}); assert.deepEqual(r.calls, ['save']);
});
test('Rust: the File menu carries Save and Save As items with the accelerators and the same page events', () => {
  const lib = fs.readFileSync('src-tauri/src/lib.rs', 'utf8');
  assert.match(lib, /CmdOrCtrl\+S/); assert.match(lib, /CmdOrCtrl\+Shift\+S/);
  assert.match(lib, /Some\("menu-save-project"\)/); assert.match(lib, /Some\("menu-save-project-as"\)/);
  assert.match(lib, /add_project_save_items\(app\.handle\(\), &menu\)/);
});

test('findOpen: the active tab by the live path, another tab by its row path, nothing otherwise', () => {
  const tabs = [{ id: 't1', path: null }, { id: 't2', path: '/B.json' }];
  const current = { activeTabId: 't1', path: '/A.json', tabs };
  assert.deepEqual(TabsUI.findOpen(current, '/A.json'), { tab: tabs[0], active: true });
  assert.deepEqual(TabsUI.findOpen(current, '/B.json'), { tab: tabs[1], active: false });
  assert.equal(TabsUI.findOpen(current, '/C.json'), null);
  assert.equal(TabsUI.findOpen(current, ''), null); assert.equal(TabsUI.findOpen(current, null), null);
  assert.equal(TabsUI.findOpen({ activeTabId: 't1', path: null, tabs }, null), null, 'untitled tabs never match');
});

// The shipped project.js with the real Save UI binding; the native file/tab sessions are modelled.
function project() {
  const notices = [], saves = [], opened = [], activated = [], elements = new Map();
  let tabPorts = null, filePorts = null, active = false;
  function element() {
    const classes = new Set();
    const el = { children: [], style: {}, dataset: {}, listeners: {}, value: '', className: '', attributes: {},
      addEventListener(type, callback) { this.listeners[type] = callback; },
      appendChild(child) { this.children.push(child); child.parent = this; return child; },
      setAttribute(name, value) { this.attributes[name] = String(value); }, getAttribute(name) { return this.attributes[name]; },
      replaceWith() {}, focus() {}, select() {}, blur() {}, removeChild() {}, click() {} };
    el.classList = { add: n => classes.add(n), remove: n => classes.delete(n), contains: n => classes.has(n),
      toggle(n, force) { if (force === false || force === undefined && classes.has(n)) classes.delete(n); else classes.add(n); } };
    Object.defineProperty(el, 'innerHTML', { get: () => '', set() { el.children = []; } });
    return el;
  }
  const document = { readyState: 'loading', body: element(), addEventListener(type, callback) { if (type === 'DOMContentLoaded') this.ready = callback; },
    getElementById(id) { if (!elements.has(id)) elements.set(id, element()); return elements.get(id); },
    createElement: element, querySelector: () => null };
  const stubTabs = { create(root, ports) { tabPorts = ports; return Object.freeze({
    // Mirrors the real adapter's contract: a focus port that answers first wins, otherwise the file is prepared.
    openPath: async (p) => { const focused = ports.focusOpen(p); if (focused) return focused; opened.push(p); return { status: 'prepared' }; },
    activateClick: async (meta) => { activated.push(meta); return { status: 'activated' }; },
    close: async () => ({ status: 'closed' }), canReplaceActiveProject: () => true, held: () => null, isBusy: () => false }); } };
  const fileAdapter = { bind(root, ports) { filePorts = ports; return Object.freeze({
    isActive: () => active, canOpen: () => true, isDirty: () => false,
    saveAs: async (...args) => { saves.push(args); return { acknowledged: true }; },
    openPath: async (path) => { filePorts.commit(path, root.SMProjectDocument.baseName(path)); filePorts.opened(); active = true; return true; },
    openRecovered: async () => null, newProject: async () => null, activateProject: async () => ({ notDispatched: true }),
    recoveryPin: () => ({}) }); } };
  const root = { document, location: { protocol: 'http:', hostname: 'localhost', port: '1420' }, addEventListener() {},
    requestAnimationFrame(cb) { cb(); }, __TAURI__: {},
    NemoNativeObjectInactiveProjectTabs: stubTabs, NemoNativeObjectProjectTabsUI: TabsUI,
    NemoNativeOpacityProjectEntry: require('../src/js/adapters/native-opacity-project-entry.js'),
    NemoNativeOpacityLegacySurface: require('../src/js/adapters/native-opacity-legacy-surface.js'),
    NemoProjectNativeSave: require('../src/js/adapters/project-native-save.js'),
    NemoProjectDirtyBaseline: require('../src/js/adapters/project-dirty-baseline.js'),
    NemoProjectNativeVersionRestore: require('../src/js/adapters/project-native-version-restore.js'),
    NemoNativeObjectProjectFile: fileAdapter,
    SM: { t: key => key, fitCanvas() {}, exportJSON: () => '{}', importJSON: () => true } };
  const noop = () => {};
  const sandbox = { window: root, document, SM: root.SM, state: {}, userLayers: [], _symbolPaperLayers: {},
    localStorage: { getItem: () => null, setItem() {}, removeItem() {} }, showToast(m) { notices.push(m); },
    saveAllLayerFrames: noop, createUserLayer: noop, activateUL: noop, drawStage: noop, loadFrame: noop, renderOS: noop,
    renderArcs: noop, updateUI: noop, renderSymbolTabs: noop, syncDocFields: noop, exitToScene: noop, view: { update: noop },
    setTimeout, console, FileReader: class {} };
  for (const f of ['project-document.js', 'project-entry.js']) vm.runInNewContext(fs.readFileSync('src/js/' + f, 'utf8'), sandbox);
  sandbox.SMProjectEntry = root.SMProjectEntry;
  vm.runInNewContext(fs.readFileSync('src/js/project.js', 'utf8'), sandbox, { filename: 'project.js' });
  document.ready();
  return { root, notices, saves, opened, activated, document, get tabPorts() { return tabPorts; }, get filePorts() { return filePorts; },
    start: () => { document.getElementById('start-screen').classList.remove('hid'); },
    startHidden: () => document.getElementById('start-screen').classList.contains('hid'),
    setActive(value) { active = value; } };
}

test('Save on an untitled native project asks where (no pre-dialog notice); on a titled one it saves in place', async () => {
  const p = project(); p.setActive(true); p.filePorts.commit(null, 'Untitled');
  await p.root.SMProject.save();
  assert.deepEqual(p.saves, [[]], 'untitled: the same Save As picker path, no destination, no replace');
  assert.deepEqual(p.notices, []);
  p.filePorts.commit('/work/film.json', 'film');
  await p.root.SMProject.save();
  assert.deepEqual(p.saves[1], ['/work/film.json', { replace: true }], 'titled: in place');
  await p.root.SMProject.saveAs();
  assert.deepEqual(p.saves[2], [], 'Save As always asks, whatever the title');
  assert.deepEqual(p.notices, []);
});

test('Opening a file that is already open focuses its tab: active tab -> notice, inactive tab -> activation, new file -> prepare', async () => {
  const p = project();
  assert.equal(await p.root.SMProject.openPath('/work/A.json'), true);
  p.start();
  assert.deepEqual(await p.root.SMProject.openPath('/work/A.json'), { status: 'already-open' }, 'the active project is already open');
  assert.deepEqual(p.opened, []); assert.deepEqual(p.activated, []);
  const alert = p.document.getElementById('project-tabs-bar').children.find(c => c.getAttribute && c.getAttribute('role') === 'alert');
  assert.equal(alert && alert.textContent, 'Already open: A'); assert.equal(p.startHidden(), true, 'the start screen is dismissed');
  const meta = Object.freeze({ kind: 'native-object-inactive', path: '/work/B.json', name: 'B', preparationToken: 'tok-B' });
  assert.equal(p.tabPorts.commit(meta), true);
  const rows = p.document.getElementById('project-tabs-list').children;
  assert.equal(rows.length, 2);
  await p.root.SMProject.openPath('/work/B.json');
  assert.deepEqual(p.activated.map(m => m.preparationToken), ['tok-B'], 'the existing tab is activated, not duplicated');
  assert.deepEqual(p.opened, []); assert.equal(p.document.getElementById('project-tabs-list').children.length, 2);
  await p.root.SMProject.openPath('/work/C.json');
  assert.deepEqual(p.opened, ['/work/C.json'], 'a file that is not open still prepares a new tab');
});

test('the real inactive-tab adapter answers a focus request before it reads the owner or calls the host', async () => {
  const touched = [], focus = [];
  const tabs = InactiveTabs.create({}, { owner() { touched.push('owner'); }, invoke() { touched.push('invoke'); }, commit() { return true; },
    remove() { return true; }, notice() {}, baseName: x => x, focusOpen(path) { focus.push(path); return path === '/open.json' ? Promise.resolve({ status: 'focused' }) : null; } });
  assert.deepEqual(await tabs.openPath('/open.json'), { status: 'focused' });
  assert.deepEqual(touched, []); assert.deepEqual(focus, ['/open.json']);
  await tabs.openPath('/new.json');
  assert.deepEqual(focus, ['/open.json', '/new.json']);
  assert.ok(touched.includes('owner'), 'a file that is not open continues into the normal prepare path');
});
