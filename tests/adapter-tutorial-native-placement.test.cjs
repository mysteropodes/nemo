'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const crypto = require('node:crypto');
const { create, plan, matches, positionTooltip } = require('../src/js/adapters/tutorial-native-placement.js');

function deferred() { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; }
function rect(left, top, width, height) { return { left, top, width, height, right: left + width, bottom: top + height }; }
function intersects(a, b) { return a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top; }
function style(initial = {}) {
  const values = new Map(Object.entries(initial));
  const css = key => String(key).replace(/[A-Z]/g, x => '-' + x.toLowerCase());
  return new Proxy({
    getPropertyValue: key => values.get(key) || '', getPropertyPriority: () => '',
    setProperty: (key, value) => values.set(key, value), removeProperty: key => values.delete(key),
    values: () => Object.fromEntries(values),
  }, { get: (obj, key) => key in obj ? obj[key] : values.get(css(key)) || '', set: (obj, key, value) => { values.set(css(key), value); return true; } });
}
function classes(initial = []) {
  const set = new Set(initial);
  return { add: (...xs) => xs.forEach(x => set.add(x)), remove: (...xs) => xs.forEach(x => set.delete(x)),
    contains: x => set.has(x), toggle: (x, on) => on ? set.add(x) : set.delete(x) };
}
function harness({ width = 1400, collapsed = false, native = true } = {}) {
  const nodes = new Map(), events = new Map(), frames = [], notices = [];
  let windowWidth = width, nextFrame, doc = 'A', revision = 2, history = false;
  const inspectorWidth = collapsed ? 36 : 280;
  function node(id) {
    const n = { id, style: style(), classList: classes(), setAttribute() {}, remove() { nodes.delete(this.id); },
      appendChild(child) { nodes.set(child.id, child); } };
    nodes.set(id, n); return n;
  }
  const top = node('top-area'), canvas = node('canvas-area'), drawing = node('drawing-canvas'), props = node('props-panel');
  props.style.width = '280px'; if (collapsed) props.classList.add('collapsed');
  const dockWidth = () => parseFloat(nodes.get('tutorial-native-space')?.style.width) || 0;
  const canvasWidth = () => windowWidth - 66 - inspectorWidth - dockWidth();
  top.getBoundingClientRect = () => rect(66, 70, windowWidth - 66, 500);
  canvas.getBoundingClientRect = drawing.getBoundingClientRect = () => rect(66, 102, canvasWidth(), 468);
  props.getBoundingClientRect = () => rect(66 + canvasWidth(), 70, inspectorWidth, 500);
  const buttons = [{ disabled: false }, { disabled: false }, { disabled: true }];
  const card = node('tut-tooltip'); card.style.left = '17px'; card.style.top = '19px';
  card.style.width = '280px'; card.classList.add('on'); card.offsetWidth = 280; card.offsetHeight = 250;
  card.querySelectorAll = () => buttons;
  buttons.forEach((button, i) => { button.getBoundingClientRect = () => rect(parseFloat(card.style.left) + 10,
    parseFloat(card.style.top) + 20 + i * 60, 80, 24); });
  const root = {
    get innerWidth() { return windowWidth; }, innerHeight: 900, state: { currentFrame: 0 },
    document: { getElementById: id => id === 'history-column-space' && history ? {} : nodes.get(id),
      createElement() { const n = node('new'); nodes.delete('new'); n.getBoundingClientRect = () => rect(windowWidth - dockWidth(), 70, dockWidth(), 500); return n; } },
    addEventListener: (name, fn) => events.set(name, fn), removeEventListener: (name, fn) => { if (events.get(name) === fn) events.delete(name); },
    dispatchEvent: e => events.get(e.type)?.(), Event: class { constructor(type) { this.type = type; } },
    requestAnimationFrame: fn => queueMicrotask(fn), setTimeout, clearTimeout,
    showToast: message => notices.push(message),
  };
  const identity = () => ({ instanceId: 'instance', documentId: doc, contentRevision: revision });
  const view = () => ({ documentId: doc, generation: 3 });
  const receipt = () => ({ ...identity(), owner: 'native', status: 'presented', frame: root.state.currentFrame,
    lifecycleGeneration: 3, viewGeneration: frames.length + 1, workId: 'work-' + (frames.length + 1) });
  root.NemoNativeOpacityProject = native ? {} : null;
  root.NemoNativeOpacityCutover = { isActive: () => true, identity, getNativeIdentity: view,
    presentPreview: () => { frames.push(drawing.getBoundingClientRect()); const p = nextFrame; nextFrame = null; return p || Promise.resolve(receipt()); } };
  let failures = 0; const controller = create(root, () => { failures++; });
  return { root, controller, card, props, buttons, frames, notices, nodes, events, identity, view, receipt,
    failures: () => failures, setWidth: n => { windowWidth = n; }, setHistory: () => { history = true; },
    drift: () => { doc = 'B'; revision++; }, next: promise => { nextFrame = promise; }, nativeBounds: drawing.getBoundingClientRect,
    cardBounds: () => rect(parseFloat(card.style.left), parseFloat(card.style.top), parseFloat(card.style.width), 250) };
}

test('independent minimum-width oracle refuses nonfinite or insufficient canvas space', () => {
  assert.equal(plan(395, 500), null); assert.equal(plan(1000, 199), null); assert.equal(plan(Infinity, 500), null);
  assert.deepEqual(plan(396, 200), { width: 220 }); assert.deepEqual(plan(1000, 500), { width: 340 });
});
for (const [width, collapsed] of [[1400, false], [1100, false], [520, true]]) {
  for (const [kind, target, corner] of [['no-target', () => null], ['canvas', h => h.nativeBounds()],
    ['missing-target', () => null], ['corner', () => null, 'top-right'], ['inspector', h => h.props.getBoundingClientRect()]]) {
    test(`native card avoids viewport and retains usable inspector: ${kind}/${width}/${collapsed}`, async () => {
      const h = harness({ width, collapsed }), original = h.card.style.values(), pin = h.identity();
      const preferences = h.props.style.values();
      assert.equal(await h.controller.place(h.card, () => target(h), corner), 'ready');
      assert.equal(intersects(h.cardBounds(), h.nativeBounds()), false);
      assert.equal(intersects(h.cardBounds(), h.props.getBoundingClientRect()), false);
      assert.ok(h.nativeBounds().width >= 160); assert.equal(h.props.classList.contains('collapsed'), collapsed);
      assert.deepEqual(h.props.style.values(), preferences); assert.equal(h.props.style.display, '');
      assert.ok(h.props.getBoundingClientRect().width > 0, 'real inspector target region still exists');
      assert.deepEqual(h.buttons.map(b => b.disabled), [false, false, true]);
      await h.controller.close(); assert.deepEqual(h.card.style.values(), original);
      assert.deepEqual(h.props.style.values(), preferences); assert.deepEqual(h.identity(), pin);
      assert.equal(h.nodes.has('tutorial-native-space'), false); assert.equal(h.events.size, 0);
      assert.equal(h.frames.at(-1).width, width - 66 - (collapsed ? 36 : 280));
    });
  }
}
test('controls and step advancement wait for the final matching native receipt', async () => {
  const h = harness(), d = deferred(); h.next(d.promise);
  const p = h.controller.place(h.card, () => null);
  assert.equal(h.controller.ready(), false); assert.ok(h.buttons.every(b => b.disabled));
  await new Promise(setImmediate); assert.equal(h.controller.ready(), false);
  d.resolve(h.receipt()); assert.equal(await p, 'ready'); assert.equal(h.controller.ready(), true);
  await h.controller.close();
});
for (const change of [r => ({ ...r, status: 'deferred-occluded' }), r => ({ ...r, documentId: 'other' }),
  r => ({ ...r, lifecycleGeneration: 99 }), r => ({ ...r, workId: '' }), r => ({ ...r, frame: 10 })]) {
  test('unmatched/deferred native receipt unwinds and stops the active tutorial', async () => {
    const h = harness(), before = h.card.style.values(); h.next(Promise.resolve(change(h.receipt())));
    assert.equal(await h.controller.place(h.card, () => null), 'failed');
    assert.equal(h.controller.ready(), false); assert.equal(h.failures(), 1);
    assert.equal(h.nodes.has('tutorial-native-space'), false); assert.deepEqual(h.card.style.values(), before);
    assert.equal(h.card.classList.contains('on'), false); assert.equal(h.notices.length, 1);
  });
}
test('document replacement while presenting refuses rather than publishing stale controls', async () => {
  const h = harness(), d = deferred(), old = h.receipt(); h.next(d.promise);
  const p = h.controller.place(h.card, () => null); await new Promise(setImmediate); h.drift(); d.resolve(old);
  assert.equal(await p, 'failed'); assert.equal(h.failures(), 1);
});
test('close during presentation cancels publication and restores layout exactly once', async () => {
  const h = harness(), d = deferred(); h.next(d.promise); const before = h.card.style.values();
  const p = h.controller.place(h.card, () => null); await new Promise(setImmediate); await h.controller.close();
  d.resolve(h.receipt()); assert.equal(await p, 'stale'); assert.equal(h.controller.ready(), false);
  assert.deepEqual(h.card.style.values(), before); await h.controller.close(); assert.equal(h.frames.length, 2);
});
test('layout changing during presentation refuses and preserves inspector preferences', async () => {
  const h = harness(), d = deferred(); h.next(d.promise);
  const p = h.controller.place(h.card, () => null); await new Promise(setImmediate); h.setWidth(1300); d.resolve(h.receipt());
  assert.equal(await p, 'failed'); assert.equal(h.props.style.width, '280px');
});
test('too-narrow window or occupied History column refuses without hiding inspector', async () => {
  for (const occupied of [false, true]) {
    const h = harness({ width: occupied ? 1400 : 500 }); if (occupied) h.setHistory();
    assert.equal(await h.controller.place(h.card, () => null), 'failed');
    assert.equal(h.props.style.display, ''); assert.equal(h.nodes.has('tutorial-native-space'), false);
  }
});
test('a card with an off-column control cannot enable an invisible Skip', async () => {
  const h = harness(); h.buttons[0].getBoundingClientRect = () => rect(700, 400, 80, 24);
  assert.equal(await h.controller.place(h.card, () => null), 'failed');
  assert.equal(h.controller.ready(), false); assert.equal(h.failures(), 1);
});
test('browser placement retains prior center/corner behavior without a native dock', async () => {
  const h = harness({ native: false }); assert.equal(await h.controller.place(h.card, () => null), 'ready');
  assert.equal(h.card.style.left, '560px'); assert.equal(h.card.style.top, '325px'); assert.equal(h.frames.length, 0);
  assert.deepEqual(positionTooltip(280, 250, null, 'top-right', 1400, 900), { left: 1104, top: 16 });
});
test('all tutorial module definitions, translations and measurement oracles stay byte-identical', () => {
  const current = fs.readFileSync('src/js/tutorial.js', 'utf8');
  // Frozen pre-repair definitions/translation/measurement bytes, usable in source archives.
  const prefix = current.slice(0, current.indexOf('  // ---- Progress persistence'));
  assert.equal(crypto.createHash('sha256').update(prefix).digest('hex'), 'e8d414b4425f05dec927249990d26bea15f780d007b92cf51e4f5b8051773676');
});
test('real tutorial Escape/Skip/finish retain separate seen and completed flags', () => {
  const handlers = new Map(), store = new Map(), nodes = new Map(); let stopCount = 0;
  const n = id => {
    if (!nodes.has(id)) nodes.set(id, { id, classList: classes(id === 'start-screen' ? ['hid'] : []), style: {}, addEventListener() {},
      getBoundingClientRect: () => rect(1120, 70, 280, 500),
      innerHTML: '', offsetWidth: 280, offsetHeight: 250, querySelector: selector => ({ addEventListener: (name, fn) => handlers.set(selector + name, fn) }) });
    return nodes.get(id);
  };
  const document = { getElementById: n, querySelector: selector => selector === '#tut-spotlight' && !nodes.has('tut-spotlight') ? null : n(selector.slice(1)),
    createElement: () => ({ classList: classes(), style: {}, querySelector: selector => ({ addEventListener: (name, fn) => handlers.set(selector + name, fn) }) }),
    body: { appendChild: el => nodes.set(el.id, el) }, addEventListener: (name, fn) => handlers.set(name, fn),
    removeEventListener: (name, fn) => { if (handlers.get(name) === fn) handlers.delete(name); } };
  const window = { document, state: { language: 'fr' }, NemoTutorialNativePlacement: { create: () => ({ ready: () => true, place: () => Promise.resolve('ready'), close: () => { stopCount++; } }) } };
  const context = { window, document, localStorage: { getItem: k => store.get(k), setItem: (k, v) => store.set(k, v) },
    setInterval: fn => { handlers.set('timer', fn); return 1; }, clearInterval() {} };
  vm.runInNewContext(fs.readFileSync('src/js/tutorial.js', 'utf8'), context);
  handlers.get('DOMContentLoaded')(); handlers.get('timer')();
  assert.equal(store.get('nemo-tut-feedback-shown'), '1'); assert.equal(store.has('nemo-tutorial-done'), false);
  handlers.get('keydown')({ key: 'Escape' }); assert.equal(stopCount, 1); assert.equal(store.has('nemo-tutorial-done'), false);
  window.SMTutorial.start('feedback-welcome'); handlers.get('.tut-skipclick')();
  assert.equal(stopCount, 2); assert.equal(store.has('nemo-tutorial-done'), false);
  window.SMTutorial.start('overview');
  handlers.get('.tut-nextclick')();
  const click = () => handlers.get('click')({ target: { closest: () => ({}) } });
  click(); click(); handlers.get('.tut-nextclick')(); click(); handlers.get('.tut-nextclick')();
  assert.equal(stopCount, 3); assert.deepEqual(JSON.parse(store.get('nemo-tutorial-done')), ['overview']);
  assert.equal(store.get('nemo-tut-feedback-shown'), '1'); assert.equal(handlers.has('click'), false);
  window.SMTutorial.start('reference'); handlers.get('.tut-nextclick')(); handlers.get('timer')();
  let selector;
  handlers.get('click')({ target: { closest: s => { selector = s; return {}; } } });
  assert.equal(selector, '#p-ref-menu', 'real tutorial delegation still reaches an inspector control');
  window.SMTutorial.stop(); assert.equal(stopCount, 4);
});
