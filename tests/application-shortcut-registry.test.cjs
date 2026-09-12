'use strict';
// P30 — keyboard-shortcut binding data and persistence, extracted from
// timeline.js. No prior test touched any shortcut function (per the C06
// census: "oracle: none; zero test coverage") — these are the first.

const test = require('node:test');
const assert = require('node:assert/strict');

const registry = require('../src/js/application/shortcut-registry.js');

function memoryStorage(initial) {
  const data = new Map(Object.entries(initial || {}));
  return {
    getItem: (key) => (data.has(key) ? data.get(key) : null),
    setItem: (key, value) => { data.set(key, String(value)); },
    removeItem: (key) => { data.delete(key); },
    _raw: data,
  };
}

test('the fixed tables carry the current baseline counts and a representative sample of key defaults', () => {
  assert.equal(registry.TOOL_SHORTCUTS.length, 21);
  assert.equal(registry.COMMAND_SHORTCUTS.length, 15);
  assert.equal(registry.READONLY_SHORTCUTS.length, 14);
  assert.equal(registry.defFor('draw').key, 'b');
  assert.equal(registry.defFor('eraser').key, 'e');
  assert.equal(registry.defFor('cmdTween').key, 't');
  assert.equal(registry.defFor('cmdInsertFrame').key, 'F5');
  assert.equal(registry.defFor('not-a-real-action'), null);
});

test('a fresh preference context resolves every action to its table default', () => {
  const shortcuts = registry.create({ storage: memoryStorage() });
  assert.equal(shortcuts.keyFor('draw'), 'b');
  assert.equal(shortcuts.keyFor('cmdTween'), 't');
  assert.equal(shortcuts.keyFor('not-a-real-action'), null);
});

test('a stored override takes precedence over the table default, and only for its own action', () => {
  const shortcuts = registry.create({ storage: memoryStorage({ 'nemo-shortcuts': '{"draw":"x"}' }) });
  assert.equal(shortcuts.keyFor('draw'), 'x');
  assert.equal(shortcuts.keyFor('select'), 'v');
});

test('setKey persists the override and is visible on the same instance immediately', () => {
  const storage = memoryStorage();
  const shortcuts = registry.create({ storage });
  shortcuts.setKey('draw', 'X');
  assert.equal(shortcuts.keyFor('draw'), 'x', 'lowercased, matching the original setShortcutKey');
  assert.deepEqual(JSON.parse(storage.getItem('nemo-shortcuts')), { draw: 'x' });
});

test('a fresh context reads back a previous instance\'s persisted override', () => {
  const storage = memoryStorage();
  registry.create({ storage }).setKey('draw', 'x');
  const reopened = registry.create({ storage });
  assert.equal(reopened.keyFor('draw'), 'x');
});

test('setKey with no key clears an override back to the table default', () => {
  const storage = memoryStorage();
  const shortcuts = registry.create({ storage });
  shortcuts.setKey('draw', 'x');
  shortcuts.setKey('draw', '');
  assert.equal(shortcuts.keyFor('draw'), 'b');
  assert.deepEqual(JSON.parse(storage.getItem('nemo-shortcuts')), {});
});

test('reset clears every override and the storage entry, independent of setKey', () => {
  const storage = memoryStorage();
  const shortcuts = registry.create({ storage });
  shortcuts.setKey('draw', 'x');
  shortcuts.setKey('select', 'y');
  shortcuts.reset();
  assert.equal(shortcuts.keyFor('draw'), 'b');
  assert.equal(shortcuts.keyFor('select'), 'v');
  assert.equal(storage.getItem('nemo-shortcuts'), null);
});

test('onOverrideChanged fires once per setKey, naming the rebound action, and never from reset', () => {
  const calls = [];
  const shortcuts = registry.create({ storage: memoryStorage(), onOverrideChanged: (action) => calls.push(action) });
  shortcuts.setKey('draw', 'x');
  shortcuts.setKey('select', 'y');
  shortcuts.reset();
  assert.deepEqual(calls, ['draw', 'select'], 'reset does not call it, matching the original reset button handler');
});

test('malformed stored JSON falls back to no overrides rather than throwing', () => {
  const shortcuts = registry.create({ storage: memoryStorage({ 'nemo-shortcuts': '{not json' }) });
  assert.equal(shortcuts.keyFor('draw'), 'b');
});

test('a stored non-object JSON value falls back to no overrides rather than throwing', () => {
  const shortcuts = registry.create({ storage: memoryStorage({ 'nemo-shortcuts': '[1,2,3]' }) });
  assert.equal(shortcuts.keyFor('draw'), 'b');
});

test('a storage.getItem that throws is treated the same as no stored overrides', () => {
  const storage = memoryStorage();
  storage.getItem = () => { throw new Error('quota/private-mode failure'); };
  const shortcuts = registry.create({ storage });
  assert.equal(shortcuts.keyFor('draw'), 'b');
});

test('a storage.setItem that throws still applies the override in memory (silent persistence failure, same as before)', () => {
  const storage = memoryStorage();
  storage.setItem = () => { throw new Error('quota exceeded'); };
  const shortcuts = registry.create({ storage });
  assert.doesNotThrow(() => shortcuts.setKey('draw', 'x'));
  assert.equal(shortcuts.keyFor('draw'), 'x', 'in-memory override still wins even though it could not persist');
});

test('a storage.removeItem that throws during reset still clears in-memory overrides', () => {
  const storage = memoryStorage();
  const shortcuts = registry.create({ storage });
  shortcuts.setKey('draw', 'x');
  storage.removeItem = () => { throw new Error('failure'); };
  assert.doesNotThrow(() => shortcuts.reset());
  assert.equal(shortcuts.keyFor('draw'), 'b');
});

test('clashFor finds another action already bound to a key, tools and commands together', () => {
  const shortcuts = registry.create({ storage: memoryStorage() });
  const clash = shortcuts.clashFor('select', 'b'); // 'b' is draw's default
  assert.equal(clash && clash.action, 'draw');
  assert.equal(shortcuts.clashFor('draw', 'b'), null, 'an action never clashes with its own current key');
  assert.equal(shortcuts.clashFor('draw', ''), null, 'an empty key never clashes');
});

test('clashFor honors overrides, not just table defaults, on both sides of the comparison', () => {
  const shortcuts = registry.create({ storage: memoryStorage() });
  shortcuts.setKey('select', 'z'); // was 'v', zoom's default — now collides with zoom
  const clash = shortcuts.clashFor('draw', 'z');
  assert.equal(clash && clash.action, 'select', 'the rebound action is what now holds the key, not its old default');
});

test('clashFor does not consider READONLY_SHORTCUTS, preserving the original (documented, not widened) behavior', () => {
  const shortcuts = registry.create({ storage: memoryStorage() });
  // Space is READONLY_SHORTCUTS' play/pause binding; the original clash set
  // was TOOL_SHORTCUTS.concat(COMMAND_SHORTCUTS) only, so this must be free.
  assert.equal(shortcuts.clashFor('draw', ' '), null);
});

test('each create() call is an independent preference context; one instance\'s writes do not leak into another\'s storage-less instance', () => {
  const a = registry.create({ storage: memoryStorage() });
  const b = registry.create({ storage: memoryStorage() });
  a.setKey('draw', 'x');
  assert.equal(a.keyFor('draw'), 'x');
  assert.equal(b.keyFor('draw'), 'b');
});

test('the read accessors expose the same table instances the registry resolves against', () => {
  const shortcuts = registry.create({ storage: memoryStorage() });
  assert.equal(shortcuts.toolShortcuts(), registry.TOOL_SHORTCUTS);
  assert.equal(shortcuts.commandShortcuts(), registry.COMMAND_SHORTCUTS);
  assert.equal(shortcuts.readonlyShortcuts(), registry.READONLY_SHORTCUTS);
  assert.equal(shortcuts.categories(), registry.SHORTCUT_CATS);
});
