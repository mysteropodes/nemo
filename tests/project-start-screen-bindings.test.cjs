'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const test = require('node:test');
const source = fs.readFileSync(path.join(__dirname, '../src/js/project-entry.js'), 'utf8');

function view({ local = false, missing } = {}) {
  const elements = new Map(), calls = [], root = {};
  function node() {
    const classes = new Set(), listeners = new Map();
    return { style: {}, value: '', attributes: {}, children: [], listeners,
      classList: { add: key => classes.add(key), remove: key => classes.delete(key), contains: key => classes.has(key) },
      setAttribute(key, value) { this.attributes[key] = value; },
      appendChild(child) { this.children.push(child); },
      addEventListener(type, listener) { listeners.set(type, listener); },
      fire(type, event) { return listeners.get(type).call(this, event); } };
  }
  for (const id of ['start-screen', 'start-resume', 'start-cards', 'start-open', 'start-newpanel',
    'start-new', 'np-cancel', 'np-preset', 'np-create', 'np-custom-row', 'np-w', 'np-h', 'np-fps', 'np-name']) elements.set(id, node());
  elements.get('start-resume').style.display = 'none';
  elements.get('start-newpanel').style.display = 'none';
  elements.get('np-preset').value = '1280x720'; elements.get('np-fps').value = '30';
  elements.get('np-name').value = '  Project A  ';
  if (missing) elements.delete(missing);
  const context = { window: root };
  for (const name of ['localStorage', 'indexedDB', 'state']) Object.defineProperty(context, name, {
    get() { throw Error(`presentation must not access ${name}`); } });
  vm.runInNewContext(source, context);
  let publish; const publishers = [];
  const ports = {
    readResumePresence() { calls.push('presence'); return local; },
    readResumePresenceAsync(callback) { calls.push('async-presence'); publish = callback; publishers.push(callback); },
    renderRecents() { calls.push('recents'); },
    resume() { calls.push('resume'); return 'resume-result'; },
    open() { calls.push('open'); return 'open-result'; },
    createProject(config, current, failed) { calls.push({ config, current, failed }); return false; },
    showStartScreen() { calls.push('show'); }, notice(message) { calls.push(message); },
  };
  const refreshView = root.SMProjectEntry.bindStartScreen({ getElementById: id => elements.get(id) || null, createElement: node }, ports);
  return { refreshView, publishers, calls, ports, elements, publish: value => publish(value), entry: root.SMProjectEntry };
}

test('binding reads only presence, preserves startup order and never invokes document actions', () => {
  const h = view({ local: true });
  assert.deepEqual(h.calls, ['presence', 'recents']);
  assert.equal(h.elements.get('start-resume').style.display, '');
  assert.equal(h.elements.get('start-cards').classList.contains('has-resume'), true);
  assert.equal(h.elements.get('start-resume').fire('click'), 'resume-result', 'the click runs the Resume action and returns its result');
  assert.deepEqual(h.calls.slice(-1), ['resume']);
  assert.strictEqual(h.elements.get('start-open').listeners.get('click'), h.ports.open);
  assert.equal(h.elements.get('start-newpanel').style.display, 'none');
});

test('asynchronous presence can reveal Resume without transferring bytes or importing a document', () => {
  const h = view();
  assert.deepEqual(h.calls, ['presence', 'async-presence', 'recents']);
  h.publish(false);
  assert.equal(h.elements.get('start-resume').style.display, 'none');
  h.publish(true);
  assert.equal(h.elements.get('start-resume').style.display, '');
  assert.deepEqual(h.calls, ['presence', 'async-presence', 'recents']);
  assert.equal(h.elements.get('start-resume').fire('click'), 'resume-result');
  assert.equal(h.elements.get('start-open').fire('click'), 'open-result');
  assert.deepEqual(h.calls.slice(-2), ['resume', 'open']);
});

test('the Resume card is re-decided each time the start screen is shown, and hides when nothing valid remains', () => {
  const h = view();
  h.publish(true);
  assert.equal(h.elements.get('start-resume').style.display, '');
  const shown = h.refreshView;
  assert.equal(typeof shown.refresh, 'function');
  shown.refresh();
  assert.equal(h.calls.filter(c => c === 'async-presence').length, 2, 'presence is read again');
  h.publish(false);
  assert.equal(h.elements.get('start-resume').style.display, 'none');
  assert.equal(h.elements.get('start-cards').classList.contains('has-resume'), false);
  // A slower earlier answer cannot overwrite the newer one.
  const stale = h.publishers[h.publishers.length - 1];
  shown.refresh(); h.publish(true); stale(false);
  assert.equal(h.elements.get('start-resume').style.display, '');
});

test('after a click the card is re-read while the start screen is still showing', async () => {
  const h = view({ local: true });
  const before = h.calls.filter(c => c === 'presence').length;
  h.elements.get('start-resume').fire('click');
  await new Promise(setImmediate);
  assert.equal(h.calls.filter(c => c === 'presence').length, before + 1);
  h.elements.get('start-screen').classList.add('hid');
  h.elements.get('start-resume').fire('click');
  await new Promise(setImmediate);
  assert.equal(h.calls.filter(c => c === 'presence').length, before + 1, 'a hidden start screen is not refreshed');
});

test('an absent view binds nothing and never asks the controller for data or actions', () => {
  for (const missing of ['start-screen', 'start-resume', 'start-cards', 'start-open', 'start-newpanel']) {
    const h = view({ missing }); assert.deepEqual(h.calls, [], missing);
    for (const element of h.elements.values()) assert.equal(element.listeners.size, 0, missing);
  }
});

test('native Open Enter and Space stop document shortcuts without cancelling or synthesizing activation', () => {
  const h = view(), open = h.elements.get('start-open');
  let stopped = 0;
  for (const type of ['keydown', 'keyup']) for (const key of ['Enter', ' ', 'Escape', 'x']) {
    let count = 0;
    open.fire(type, { key, stopPropagation() { count++; stopped++; }, preventDefault() { throw Error('native default activation was cancelled'); } });
    assert.equal(count, ['Enter', ' '].includes(key) ? 1 : 0);
  }
  assert.equal(stopped, 4); assert.equal(h.calls.includes('open'), false);
  assert.equal(open.fire('click'), 'open-result'); assert.equal(h.calls.filter(value => value === 'open').length, 1);
});

test('Start delegates the existing New form parsing and denial instead of inventing native creation', () => {
  const h = view(), e = id => h.elements.get(id);
  e('start-new').fire('click');
  assert.equal(e('start-newpanel').style.display, 'block');
  assert.equal(e('np-create').fire('click'), false);
  const intent = h.calls.find(value => value && typeof value === 'object');
  assert.deepEqual(JSON.parse(JSON.stringify(intent.config)), { w: 1280, h: 720, fps: 30, name: 'Project A' });
  assert.equal(intent.current(), true); assert.equal(typeof intent.failed, 'function');
  assert.equal(h.calls.at(-1), 'New project unavailable — native admission was denied.');
  assert.equal(e('start-newpanel').style.display, 'block');
  assert.equal(h.calls.some(value => value === 'open' || value === 'resume' || value === 'show'), false);
});
