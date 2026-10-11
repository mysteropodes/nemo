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
  let inspectorWidth = collapsed ? 36 : 280, observerCallback;
  const observed = new Set();
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
    ResizeObserver: class {
      constructor(fn) { observerCallback = fn; }
      observe(element) { observed.add(element); }
      disconnect() { observed.clear(); }
    },
    showToast: message => notices.push(message),
  };
  const identity = () => ({ instanceId: 'instance', documentId: doc, contentRevision: revision });
  const view = () => ({ documentId: doc, generation: 3 });
  const receipt = () => ({ ...identity(), owner: 'native', status: 'presented', frame: root.state.currentFrame,
    lifecycleGeneration: 3, viewGeneration: frames.length + 1, workId: 'work-' + (frames.length + 1) });
  root.NemoNativeOpacityProject = native ? {} : null;
  root.NemoNativeOpacityCutover = { isActive: () => true, identity, getNativeIdentity: view,
    presentPreview: () => { frames.push(drawing.getBoundingClientRect()); const p = nextFrame; nextFrame = null; return p || Promise.resolve(receipt()); } };
  if (!native) root.NemoNativeOpacityCutover = null;
  let failures = 0; const controller = create(root, () => { failures++; });
  return { root, controller, card, props, buttons, frames, notices, nodes, events, identity, view, receipt,
    failures: () => failures, setWidth: n => { windowWidth = n; }, setHistory: (value = true) => { history = value; },
    fireChild: id => { const element = nodes.get(id); assert.ok(observed.has(element), id + ' must be observed'); observerCallback(); },
    setInspectorWidth: width => { inspectorWidth = width; props.style.width = width + 'px'; },
    collapseInspector: () => { inspectorWidth = 36; props.style.width = ''; props.classList.add('collapsed'); },
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
test('child collapse re-presents before enabling controls and coalesces observer feedback', async () => {
  const h = harness(); await h.controller.place(h.card, () => h.props.getBoundingClientRect());
  const d = deferred(); h.next(d.promise); h.collapseInspector(); h.fireChild('props-panel'); h.fireChild('drawing-canvas');
  assert.equal(h.controller.ready(), false); assert.ok(h.buttons.every(b => b.disabled));
  await new Promise(setImmediate); assert.equal(h.frames.length, 2, 'one native presentation for the changed layout');
  assert.equal(h.controller.ready(), false); d.resolve(h.receipt()); await new Promise(setImmediate);
  assert.equal(h.controller.ready(), true); assert.equal(intersects(h.cardBounds(), h.nativeBounds()), false);
  h.fireChild('canvas-area'); await new Promise(setImmediate); assert.equal(h.frames.length, 2, 'unchanged geometry cannot form an observer loop');
  await h.controller.close(); assert.equal(h.props.classList.contains('collapsed'), true); assert.equal(h.props.style.width, '');
});
test('in-session inspector width that leaves insufficient native space refuses and unwinds', async () => {
  const h = harness({ width: 1100 }); await h.controller.place(h.card, () => h.props.getBoundingClientRect());
  h.setInspectorWidth(700); h.fireChild('props-panel'); await new Promise(setImmediate);
  assert.equal(h.controller.ready(), false); assert.equal(h.failures(), 1);
  assert.equal(h.nodes.has('tutorial-native-space'), false); assert.equal(h.card.classList.contains('on'), false);
  assert.equal(h.props.style.width, '700px', 'the user width change is preserved'); assert.equal(h.props.style.display, '');
  assert.ok(h.nativeBounds().width >= 160, 'unwind restores a positive viewport');
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
function launcher(h) {
  const controls = new Map(); let scroll = 0;
  const box = () => h.cardBounds();
  for (const [i, name] of ['#tut-launcher-close', '#tut-launcher-search', '#tut-ask-input', '#tut-ask-btn'].entries()) {
    const control = { disabled: false, getBoundingClientRect: () => rect(box().left + 10, box().top + 10 + i * 50, 70, 24) };
    controls.set(name, control); h.buttons.push(control);
  }
  controls.set('.modal-bdy', { getBoundingClientRect: () => rect(box().left + 10, box().top + 70, box().width - 20, 60) });
  const back = { disabled: false, getBoundingClientRect: () => rect(0, 0, 0, 0) };
  const row = { disabled: false, getBoundingClientRect: () => rect(box().left + 10, box().top + 850 - scroll, 100, 40) };
  h.buttons.push(back, row); h.card.querySelector = s => controls.get(s) || null;
  return { controls, row, back, scroll: n => { scroll = n; } };
}
test('launcher fixed controls and list viewport wait for native presentation; clipped rows and hidden Back are legitimate', async () => {
  const h = harness(), menu = launcher(h), d = deferred(); h.next(d.promise);
  const p = h.controller.place(h.card, () => null, null, 'launcher');
  assert.ok(h.buttons.every(b => b.disabled)); assert.equal(h.controller.ready(), false);
  await new Promise(setImmediate); d.resolve(h.receipt()); assert.equal(await p, 'ready');
  for (const control of menu.controls.values()) assert.equal(intersects(control.getBoundingClientRect(), h.nativeBounds()), false);
  assert.equal(menu.row.disabled, false); assert.equal(menu.back.disabled, false);
  assert.ok(menu.row.getBoundingClientRect().top > 900, 'offscreen row does not pretend to be pointer-visible');
  menu.scroll(760); const rowRect = menu.row.getBoundingClientRect(), listRect = menu.controls.get('.modal-bdy').getBoundingClientRect();
  assert.ok(rowRect.top >= listRect.top && rowRect.bottom <= listRect.bottom, 'normal scrolling brings row inside the uncovered list');
  await h.controller.close();
});
for (const selector of ['#tut-launcher-close', '#tut-launcher-search', '#tut-ask-input', '#tut-ask-btn', '.modal-bdy']) {
  test('launcher missing or covered required control refuses: ' + selector, async () => {
    for (const missing of [true, false]) {
      const h = harness(), menu = launcher(h);
      if (missing) menu.controls.delete(selector);
      else menu.controls.get(selector).getBoundingClientRect = () => rect(700, 400, 100, 40);
      assert.equal(await h.controller.place(h.card, () => null, null, 'launcher'), 'failed');
      assert.equal(h.controller.ready(), false); assert.equal(h.failures(), 1); assert.equal(h.nodes.has('tutorial-native-space'), false);
    }
  });
}
test('launcher mode does not weaken the default tooltip visible-control oracle', async () => {
  const h = harness(); launcher(h);
  assert.equal(await h.controller.place(h.card, () => null), 'failed');
});
for (const kind of [undefined, 'launcher']) {
  for (const reason of ['narrow', 'history']) {
    test('no-space refusal restores controls and permits reopen: ' + kind + '/' + reason, async () => {
      const h = harness({ width: reason === 'narrow' ? 500 : 1400 });
      if (kind) launcher(h); if (reason === 'history') h.setHistory();
      const before = h.buttons.map(b => b.disabled);
      assert.equal(await h.controller.place(h.card, () => null, null, kind), 'failed');
      assert.equal(h.nodes.has('tutorial-native-space'), false);
      assert.deepEqual(h.buttons.map(b => b.disabled), before);
      assert.equal(h.card.classList.contains('tutorial-native-pending'), false);
      h.setWidth(1400); h.setHistory(false); h.card.classList.add('on');
      assert.equal(await h.controller.place(h.card, () => null, null, kind), 'ready');
      assert.deepEqual(h.buttons.map(b => b.disabled), before);
      await h.controller.close();
    });
  }
}
test('initial/start-screen native lifecycle without a document retains centered launcher', async () => {
  const h = harness(); launcher(h);
  h.root.NemoNativeOpacityCutover = { isActive: () => false, identity: () => null, getNativeIdentity: () => null, blocksLegacy: () => false };
  assert.equal(await h.controller.place(h.card, () => null, null, 'launcher'), 'ready');
  assert.equal(h.nodes.has('tutorial-native-space'), false); assert.equal(h.frames.length, 0);
  assert.equal(h.card.style.left, '560px'); assert.equal(h.card.style.top, '325px');
});
test('inactive or indeterminate native ownership never falls back to centered controls', async () => {
  for (const unknown of [false, true]) {
    const h = harness(); launcher(h); h.root.NemoNativeOpacityCutover.isActive = () => false;
    if (unknown) h.root.NemoNativeOpacityCutover.identity = () => { throw new Error('indeterminate'); };
    assert.equal(await h.controller.place(h.card, () => null, null, 'launcher'), 'failed');
    assert.equal(h.controller.ready(), false); assert.equal(h.card.classList.contains('on'), false);
    assert.equal(h.nodes.has('tutorial-native-space'), false);
  }
});
test('an active native cutover cannot be centered merely because its project marker is absent', async () => {
  const h = harness(); launcher(h); h.root.NemoNativeOpacityProject = null;
  assert.equal(await h.controller.place(h.card, () => null, null, 'launcher'), 'ready');
  assert.equal(h.nodes.has('tutorial-native-space'), true); assert.equal(intersects(h.cardBounds(), h.nativeBounds()), false);
  await h.controller.close();
});
test('launcher child resize retains fixed-control semantics and disabled inputs until a new receipt', async () => {
  const h = harness(); launcher(h); await h.controller.place(h.card, () => null, null, 'launcher');
  const d = deferred(); h.next(d.promise); h.collapseInspector(); h.fireChild('props-panel');
  assert.equal(h.controller.ready(), false); assert.ok(h.buttons.every(b => b.disabled));
  await new Promise(setImmediate); d.resolve(h.receipt()); await new Promise(setImmediate); assert.equal(h.controller.ready(), true);
  await h.controller.close();
});
test('a new surface cannot steal an existing native dock without a release', async () => {
  const h = harness(); await h.controller.place(h.card, () => null);
  assert.equal(await h.controller.place({}, () => null, null, 'launcher'), 'failed');
  assert.equal(h.nodes.has('tutorial-native-space'), false); assert.equal(h.controller.ready(), false);
});
test('tutorial definitions outside the admitted Reference notice stay byte-identical', () => {
  const current = fs.readFileSync('src/js/tutorial.js', 'utf8');
  // P37B frozen bytes, excluding exactly the admitted P37C lesson and new translations.
  const prefix = current.slice(0, current.indexOf('  // ---- Progress persistence'))
    .replace(/    \/\/ P37C availability translations:start\n[\s\S]*?    \/\/ P37C availability translations:end\n/, '')
    .replace(/    \{\n      id: 'reference',[\s\S]*?(?=    \{\n      id: 'squash-stretch',)/, '    /* P37C Reference availability notice */\n');
  assert.equal(crypto.createHash('sha256').update(prefix).digest('hex'), '4d0dc1d9cde02c893f04387870f2a2b522ee263e8e07d0bad0ce4f37206ffd84');
});
function tutorialFixture() {
  const handlers = new Map(), store = new Map(), nodes = new Map(); let stopCount = 0;
  function element(id) {
    const el = { id, classList: classes(id === 'start-screen' ? ['hid'] : []), style: {}, children: [], listeners: {},
      addEventListener(name, fn) { this.listeners[name] = fn; handlers.set(this.id + name, fn); }, appendChild(child) { this.children.push(child); },
      getBoundingClientRect: () => rect(1120, 70, 280, 500), offsetWidth: 280, offsetHeight: 250,
      querySelector: selector => n(selector.replace(/^#/, '')) };
    let markup = '';
    Object.defineProperty(el, 'innerHTML', { get: () => markup, set: value => { markup = value; el.children = []; } });
    return el;
  }
  const n = id => {
    if (!nodes.has(id)) nodes.set(id, element(id));
    return nodes.get(id);
  };
  const document = { getElementById: n, querySelector: selector => ['#tut-spotlight', '#tut-launcher'].includes(selector) && !nodes.has(selector.slice(1)) ? null : n(selector.replace(/^#/, '')),
    createElement: () => element(''),
    body: { appendChild: el => nodes.set(el.id, el) }, addEventListener: (name, fn) => handlers.set(name, fn),
    removeEventListener: (name, fn) => { if (handlers.get(name) === fn) handlers.delete(name); } };
  let live = false; const places = [];
  const placement = { ready: () => true,
    place: (...args) => { live = true; places.push(args); return Promise.resolve('ready'); },
    close: () => { if (live) stopCount++; live = false; return Promise.resolve(true); } };
  const window = { document, state: { language: 'fr' }, NemoTutorialNativePlacement: { create: () => placement } };
  const context = { window, document, localStorage: { getItem: k => store.get(k), setItem: (k, v) => store.set(k, v) },
    SM: { t: () => '%s completed' },
    setInterval: fn => { handlers.set('timer', fn); return 1; }, clearInterval() {} };
  vm.runInNewContext(fs.readFileSync('src/js/tutorial.js', 'utf8'), context);
  handlers.get('DOMContentLoaded')();
  return { handlers, store, nodes, window, placement, places, stopCount: () => stopCount };
}
test('ordinary category/search/module handlers lead to the explicit Reference availability notice', async () => {
  const h = tutorialFixture(); await h.window.SMTutorial.open();
  assert.equal(h.places[0][3], 'launcher');
  const list = h.nodes.get('tut-mod-list');
  const media = list.children.find(el => el.className === 'tut-cat-hdr' && el.innerHTML.includes('Médias'));
  assert.ok(media); media.listeners.click(); assert.equal(h.nodes.get('tut-launcher-title').textContent, 'Médias');
  assert.equal(h.nodes.get('tut-launcher-back').style.display, 'flex');
  h.nodes.get('tut-launcher-search').listeners.input({ target: { value: 'Référence' } });
  const reference = list.children.find(el => el.className === 'tut-mod' && el.innerHTML.includes('roto'));
  assert.ok(reference); reference.listeners.click(); await new Promise(setImmediate);
  assert.equal(h.places.length, 2); assert.equal(h.places[1][3], undefined);
  assert.equal(h.nodes.get('tut-launcher').style.display, 'none');
  assert.ok(h.nodes.get('tut-tooltip').innerHTML.includes('Référence de rotoscopie indisponible'));
  assert.equal(h.store.has('nemo-tutorial-done'), false);
});
test('real tutorial Escape/Skip/finish retain separate seen and completed flags', async () => {
  const h = tutorialFixture(), { handlers, store, window } = h; handlers.get('timer')();
  await new Promise(setImmediate);
  assert.equal(store.get('nemo-tut-feedback-shown'), '1'); assert.equal(store.has('nemo-tutorial-done'), false);
  handlers.get('keydown')({ key: 'Escape' }); assert.equal(h.stopCount(), 1); assert.equal(store.has('nemo-tutorial-done'), false);
  await window.SMTutorial.start('feedback-welcome'); handlers.get('.tut-skipclick')();
  assert.equal(h.stopCount(), 2); assert.equal(store.has('nemo-tutorial-done'), false);
  await window.SMTutorial.start('overview');
  handlers.get('.tut-nextclick')();
  const click = () => handlers.get('click')({ target: { closest: () => ({}) } });
  click(); click(); handlers.get('.tut-nextclick')(); click(); handlers.get('.tut-nextclick')();
  assert.equal(h.stopCount(), 3); assert.deepEqual(JSON.parse(store.get('nemo-tutorial-done')), ['overview']);
  assert.equal(store.get('nemo-tut-feedback-shown'), '1'); assert.equal(handlers.has('click'), false);
  await window.SMTutorial.start('palette'); handlers.get('.tut-nextclick')();
  window.state.fillColor = '#123456'; handlers.get('timer')();
  let selector;
  handlers.get('click')({ target: { closest: s => { selector = s; return {}; } } });
  assert.equal(selector, '#btn-palette-swap', 'real tutorial delegation still reaches a supported inspector control');
  await window.SMTutorial.stop(); assert.equal(h.stopCount(), 4);
});
test('native close refusal is a false handoff result, not a swallowed success', async () => {
  const h = harness(); await h.controller.place(h.card, () => null);
  h.next(Promise.resolve({ ...h.receipt(), status: 'deferred-occluded' }));
  assert.equal(await h.controller.close(), false); assert.equal(h.notices.length, 1);
  assert.equal(h.nodes.has('tutorial-native-space'), false);
});
test('closing the unavailable Reference notice never records completion or a success toast', async () => {
  const h = tutorialFixture(), notices = [];
  h.window.showToast = text => notices.push(text);
  await h.window.SMTutorial.start('reference');
  const card = h.nodes.get('tut-tooltip');
  assert.ok(card.innerHTML.includes('>Fermer</button>'));
  assert.equal(h.handlers.has('click'), false);
  assert.equal(h.places[0][1](), null, 'no hidden inspector target or state probe');
  h.handlers.get('.tut-nextclick')(); await new Promise(setImmediate);
  assert.equal(h.stopCount(), 1);
  assert.equal(card.classList.contains('on'), false);
  assert.equal(h.store.has('nemo-tutorial-done'), false);
  assert.deepEqual(notices, []);
});
test('historical Reference completion cannot show a done badge or inflate available lesson counts', async () => {
  const h = tutorialFixture(); h.store.set('nemo-tutorial-done', JSON.stringify(['reference', 'media']));
  await h.window.SMTutorial.open();
  const list = h.nodes.get('tut-mod-list');
  const media = list.children.find(el => el.className === 'tut-cat-hdr' && el.innerHTML.includes('Médias'));
  assert.ok(media.innerHTML.includes('1/2'), 'only Media and Export count as available lessons');
  media.listeners.click();
  const row = list.children.find(el => el.innerHTML.includes('roto'));
  assert.equal(row.className, 'tut-mod'); assert.ok(row.innerHTML.includes('Indisponible'));
  assert.doesNotMatch(row.innerHTML, /✓|1 min/);
  assert.deepEqual(JSON.parse(h.store.get('nemo-tutorial-done')), ['reference', 'media'], 'preserve old stored progress');
});
test('Reference Q&A returns availability copy and opens that same notice', async () => {
  const h = tutorialFixture(); await h.window.SMTutorial.open();
  h.nodes.get('tut-ask-input').value = 'Référence rotoscopie';
  h.handlers.get('tut-ask-btnclick')();
  const answer = h.nodes.get('tut-ask-answer').innerHTML;
  assert.ok(answer.includes('pas accessible dans cette version'));
  assert.doesNotMatch(answer, /p-ref|Ouvre la section|Clique/);
  h.handlers.get('tut-ask-open-btnclick')(); await new Promise(setImmediate);
  assert.ok(h.nodes.get('tut-tooltip').innerHTML.includes('Référence de rotoscopie indisponible'));
});
for (const [language, title, close] of [
  ['en', 'Rotoscopy reference unavailable', 'Close'],
  ['es', 'Referencia de rotoscopia no disponible', 'Cerrar'],
  ['ja', 'ロトスコープ参照は利用できません', '閉じる'],
]) {
  test('Reference availability notice uses existing translation route: ' + language, async () => {
    const h = tutorialFixture(); h.window.state.language = language;
    await h.window.SMTutorial.start('reference');
    const markup = h.nodes.get('tut-tooltip').innerHTML;
    assert.ok(markup.includes(title)); assert.ok(markup.includes('>' + close + '</button>'));
    await h.window.SMTutorial.stop(); assert.equal(h.stopCount(), 1);
  });
}
test('superseded project creation cannot hide or close a newer ordinary launcher', async () => {
  const h = tutorialFixture(), project = deferred();
  h.nodes.get('start-screen').classList.remove('hid');
  h.window.SMProject = { newProject: () => project.promise };
  const old = h.window.SMTutorial.start('reference'); await new Promise(setImmediate);
  await h.window.SMTutorial.open();
  assert.equal(h.nodes.get('tut-launcher').style.display, 'flex'); assert.equal(h.places.length, 1);
  project.resolve(); await old;
  assert.equal(h.nodes.get('tut-launcher').style.display, 'flex'); assert.equal(h.places.length, 1);
  assert.equal(h.stopCount(), 0, 'stale creation cannot release the newer surface');
});
for (const operation of ['start', 'open']) {
  test('real tutorial ' + operation + ' waits for successful prior-surface release', async () => {
    const h = tutorialFixture(); h.handlers.get('timer')(); await new Promise(setImmediate);
    const d = deferred(); h.placement.close = () => d.promise;
    const pending = operation === 'start' ? h.window.SMTutorial.start('reference') : h.window.SMTutorial.open();
    await new Promise(setImmediate); assert.equal(h.places.length, 1);
    d.resolve(true); await pending; assert.equal(h.places.length, 2);
    assert.equal(h.places[1][3], operation === 'open' ? 'launcher' : undefined);
    assert.equal(h.store.has('nemo-tutorial-done'), false, 'handoff is never lesson completion');
  });
  test('real tutorial ' + operation + ' refuses a failed handoff', async () => {
    const h = tutorialFixture(); h.handlers.get('timer')(); await new Promise(setImmediate);
    h.placement.close = () => Promise.resolve(false);
    await (operation === 'start' ? h.window.SMTutorial.start('reference') : h.window.SMTutorial.open());
    assert.equal(h.places.length, 1); assert.equal(h.store.has('nemo-tutorial-done'), false);
  });
  test('Escape cancels a pending ' + operation + ' so a stale completion cannot reopen UI', async () => {
    const h = tutorialFixture(); h.handlers.get('timer')(); await new Promise(setImmediate);
    const d = deferred(); h.placement.close = () => d.promise;
    const pending = operation === 'start' ? h.window.SMTutorial.start('reference') : h.window.SMTutorial.open();
    await new Promise(setImmediate); h.handlers.get('keydown')({ key: 'Escape' });
    d.resolve(true); await pending; assert.equal(h.places.length, 1);
    assert.equal(h.store.get('nemo-tut-feedback-shown'), '1'); assert.equal(h.store.has('nemo-tutorial-done'), false);
  });
}
