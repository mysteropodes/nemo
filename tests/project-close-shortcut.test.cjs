'use strict';
// #2373: Cmd+W (and the native "Close Project" menu item that replaces the default Close Window)
// closes the active project tab once and never closes the window.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs');
const Shortcut = require('../src/js/adapters/project-close-shortcut.js');
function rig({ tauri = true } = {}) {
  const closes = [], listeners = {}, menu = {};
  const document = { addEventListener(type, fn, capture) { listeners[type] = { fn, capture }; } };
  const root = { document, SMProject: { closeActiveTab() { closes.push('close'); return Promise.resolve(); } } };
  if (tauri) root.__TAURI__ = { event: { listen(name, fn) { menu[name] = fn; return Promise.resolve(() => {}); } } };
  const api = Shortcut.install(root);
  const key = (init) => { let prevented = false;
    listeners.keydown.fn({ key: 'w', metaKey: true, preventDefault() { prevented = true; }, ...init }); return prevented; };
  return { closes, listeners, menu, api, key };
}
test('Cmd+W keydown routes to the project-close path once and is cancelled', () => {
  const r = rig();
  assert.equal(r.listeners.keydown.capture, true);
  assert.equal(r.key({}), true);
  assert.deepEqual(r.closes, ['close']);
});
test('Ctrl+W and uppercase W also close; other chords do nothing', () => {
  const unique = () => rig();
  let r = unique(); r.key({ metaKey: false, ctrlKey: true }); assert.equal(r.closes.length, 1);
  r = unique(); r.key({ key: 'W' }); assert.equal(r.closes.length, 1);
  for (const init of [{ key: 'q' }, { metaKey: false }, { shiftKey: true }, { altKey: true }]) {
    r = unique(); assert.equal(r.key(init), false, JSON.stringify(init)); assert.deepEqual(r.closes, []);
  }
});
test('the native menu event closes once, and a menu+keydown pair for one press closes once', () => {
  const r = rig();
  assert.equal(typeof r.menu[Shortcut.MENU_EVENT], 'function');
  assert.equal(Shortcut.MENU_EVENT, 'menu-close-project');
  r.menu[Shortcut.MENU_EVENT](); r.key({});
  assert.deepEqual(r.closes, ['close']);
});
test('a missing SMProject or a throwing close never throws into the page', () => {
  const bare = rig(); bare.listeners.keydown.fn({ key: 'w', metaKey: true, preventDefault() {} });
  const root = { document: { addEventListener() {} }, SMProject: { closeActiveTab() { throw Error('x'); } } };
  assert.doesNotThrow(() => Shortcut.install(root).close());
  assert.equal(Shortcut.install({ document: { addEventListener() {} } }).close(), false);
});
test('browser preview without Tauri still handles the keydown', () => {
  const r = rig({ tauri: false }); r.key({}); assert.deepEqual(r.closes, ['close']);
});
test('Rust: the default Close Window item is replaced by a Cmd+W Close Project menu item that emits the same event', () => {
  const lib = fs.readFileSync('src-tauri/src/lib.rs', 'utf8');
  assert.match(lib, /CmdOrCtrl\+W/);
  assert.match(lib, /emit\("menu-close-project"/);
  assert.match(lib, /route_close_window_to_project\(app\.handle\(\), &menu\)/);
  assert.equal(Shortcut.MENU_EVENT, 'menu-close-project');
});
