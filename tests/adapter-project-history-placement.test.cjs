'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { plan, create, bind } = require('../src/js/adapters/project-history-placement.js');
function deferred() { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }
function harness() {
  const events = []; let available = 900, context = 'document-A/revision-2';
  const ports = {
    measure: () => ({ available, height: 600 }),
    reserve: width => { events.push(['reserve', width]); return true; },
    release: () => events.push('release'), show: () => events.push('show'), hide: () => events.push('hide'),
    loading: () => events.push('loading'), enable: value => events.push(['enable', value]),
    present: async () => { events.push('presented'); }, context: () => context,
    list: async () => [{ path: 'version40.json', ts: 100 }],
    render: rows => events.push(['rows', rows]),
    restore: async () => false, notice: message => events.push(['notice', message]),
  };
  return { ports, events, controller: create(ports), set available(v) { available = v; }, set context(v) { context = v; } };
}
test('column allocation preserves at least 160px native width and supports a bounded narrow layout', () => {
  assert.deepEqual(plan({ available: 900, height: 600 }), { width: 340 });
  assert.deepEqual(plan({ available: 410, height: 250 }), { width: 234 });
  for (const bounds of [{ available: 395, height: 600 }, { available: 900, height: 199 }, { available: NaN, height: 600 }]) assert.equal(plan(bounds), null);
});
test('controls and version list wait for presentation; close releases temporary layout', async () => {
  const h = harness(), frame = deferred(); h.ports.present = () => frame.promise;
  const pending = h.controller.open();
  assert.equal(h.events.some(e => Array.isArray(e) && e[0] === 'rows'), false);
  assert.equal(await h.controller.restore('version40.json'), false);
  frame.resolve(); assert.equal(await pending, true);
  assert.ok(h.events.find(e => Array.isArray(e) && e[0] === 'rows'));
  assert.equal(await h.controller.close(), true);
  assert.ok(h.events.includes('release')); assert.ok(h.events.includes('hide'));
});
test('closing while listing cannot republish stale rows', async () => {
  const h = harness(), rows = deferred(); h.ports.list = () => rows.promise;
  const pending = h.controller.open(); await new Promise(done => setImmediate(done));
  await h.controller.close(); rows.resolve([{ path: 'late.json' }]);
  assert.equal(await pending, false);
  assert.equal(h.events.some(e => Array.isArray(e) && e[0] === 'rows'), false);
});
test('reopening ignores the older generation even when its listing finishes last', async () => {
  const h = harness(), old = deferred(); let count = 0;
  h.ports.list = () => ++count === 1 ? old.promise : Promise.resolve([{ path: 'new.json' }]);
  const pending = h.controller.open(); await new Promise(done => setImmediate(done));
  assert.equal(await h.controller.open(), true); old.resolve([{ path: 'old.json' }]);
  assert.equal(await pending, false);
  assert.deepEqual(h.events.filter(e => Array.isArray(e) && e[0] === 'rows'), [['rows', [{ path: 'new.json' }]]]);
});
for (const failure of ['present', 'list', 'reserve']) test(failure + ' failure rolls back layout and never enables Restore', async () => {
  const h = harness(); h.ports[failure] = () => { throw Error('failure'); };
  assert.equal(await h.controller.open(), false);
  assert.ok(h.events.includes('release')); assert.ok(h.events.includes('hide'));
  assert.equal(h.events.some(e => Array.isArray(e) && e[0] === 'enable' && e[1]), false);
});
test('unsupported resize releases layout; supported resize waits for a new presentation', async () => {
  const h = harness(); await h.controller.open(); h.available = 410;
  assert.equal(await h.controller.refresh(), true);
  assert.ok(h.events.some(e => Array.isArray(e) && e[0] === 'reserve' && e[1] === 234));
  h.available = 300; assert.equal(await h.controller.refresh(), false);
  assert.ok(h.events.includes('release')); assert.equal(await h.controller.restore('version40.json'), false);
});
test('document/revision drift refuses stale History before dispatch', async () => {
  const h = harness(); let calls = 0; h.ports.restore = async () => { calls++; return true; };
  await h.controller.open(); h.context = 'document-B/revision-0';
  assert.equal(await h.controller.restore('version40.json'), false); assert.equal(calls, 0);
  assert.ok(h.events.includes('release'));
});
test('Restore holds layout and prevents close/double dispatch until publication settles', async () => {
  const h = harness(), restored = deferred(); h.ports.restore = () => restored.promise;
  await h.controller.open(); const before = h.events.length, pending = h.controller.restore('version40.json');
  assert.equal(await h.controller.close(), false); assert.equal(await h.controller.restore('another.json'), false);
  assert.equal(h.events.slice(before).includes('release'), false);
  restored.resolve(false); assert.equal(await pending, false);
  assert.equal(h.events.slice(before).includes('release'), false, 'failure keeps the visible controls and prior layout lease');
  h.ports.restore = async () => true; assert.equal(await h.controller.restore('version40.json'), true);
  assert.ok(h.events.slice(before).includes('release'), 'success restores the original layout');
});
test('production registers the lazy History adapter before DOM-ready and reserves space', () => {
  const html = fs.readFileSync('src/index.html', 'utf8'), css = fs.readFileSync('src/css/style.css', 'utf8');
  const adapter = html.indexOf('js/adapters/project-history-placement.js');
  assert.ok(adapter > html.indexOf('js/project.js') && adapter < html.indexOf('</body>'));
  assert.match(css, /history-column-open[^}]+#props-panel/);
  assert.match(css, /#history-column-space/);
});

function domHarness(width, collapsed, windowWidth = 1000) {
  const notices = [], elements = new Map(), listeners = new Map();
  function element(id) {
    const classes = new Set(), style = {
      getPropertyValue(k) { return this[k.replace(/-([a-z])/g, (_, c) => c.toUpperCase())] || ''; },
      getPropertyPriority() { return ''; },
      setProperty(k, v) { this[k.replace(/-([a-z])/g, (_, c) => c.toUpperCase())] = v; },
      removeProperty(k) { delete this[k.replace(/-([a-z])/g, (_, c) => c.toUpperCase())]; },
    };
    const node = { id, style, children: [], listeners: {}, classList: {
      add(v) { classes.add(v); }, remove(v) { classes.delete(v); }, contains(v) { return classes.has(v); },
    }, setAttribute() {}, addEventListener(k, f) { this.listeners[k] = f; },
    appendChild(child) { this.children.push(child); child.parent = this; },
    remove() { this.parent.children = this.parent.children.filter(v => v !== this); },
    querySelector() { return elements.get('box'); },
    getBoundingClientRect() {
      const space = elements.get('top-area').children.find(v => v.id === 'history-column-space');
      const panelWidth = space ? parseFloat(space.style.width) : collapsed ? 36 : width;
      const actualWidth = id === 'canvas-area' ? windowWidth - 66 - panelWidth : id === 'top-area' ? windowWidth : panelWidth;
      const left = id === 'canvas-area' ? 50 : id === 'top-area' ? 0 : windowWidth - panelWidth;
      return { left, top: 70, width: actualWidth, height: 400, right: left + actualWidth };
    } };
    Object.defineProperty(node, 'textContent', { get() { return this.text || ''; }, set(v) { this.text = v; this.children = []; } });
    return node;
  }
  for (const id of ['top-area', 'canvas-area', 'props-panel', 'history-modal', 'history-close', 'history-list', 'box']) elements.set(id, element(id));
  const props = elements.get('props-panel'); props.style.width = width + 'px'; if (collapsed) props.classList.add('collapsed');
  elements.get('box').style.width = '340px';
  const identity = { instanceId: 'i', documentId: 'A', contentRevision: 2 };
  const root = { innerWidth: windowWidth, document: { getElementById: id => elements.get(id), createElement: () => element('new') },
    __TAURI__: {}, state: { currentFrame: 10 }, NemoNativeOpacityProject: {},
    NemoNativeOpacityCutover: { isActive: () => true, identity: () => identity,
      presentPreview: async frame => ({ ...identity, status: 'presented', frame }) },
    Event: class { constructor(type) { this.type = type; } },
    addEventListener(type, handler) { listeners.set(type, handler); }, dispatchEvent(event) { listeners.get(event.type)?.(); },
    setTimeout, clearTimeout, requestAnimationFrame: f => f(), confirm: () => true,
  };
  const ui = { context: () => ({ path: 'A.json' }), notice: message => notices.push(message),
    list: async () => [{ ts: 100, path: 'version40.json' }], restore: async () => false, relTime: () => '30s' };
  return { root, props, elements, notices, controller: bind(root, ui) };
}
for (const [width, collapsed, viewport] of [[280, false, 1000], [520, false, 1000], [520, true, 1000], [280, false, 500]]) {
  test(`real DOM binding preserves panel preferences and positive viewport (${width}/${collapsed}/${viewport})`, async () => {
    const h = domHarness(width, collapsed, viewport), before = h.props.style.width;
    assert.equal(await h.controller.open(), true);
    assert.ok(h.elements.get('canvas-area').getBoundingClientRect().width >= 160);
    const box = h.elements.get('box'), canvas = h.elements.get('canvas-area').getBoundingClientRect();
    assert.ok(parseFloat(box.style.left) > canvas.right, 'History controls lie outside the native canvas rectangle');
    assert.equal(h.props.style.width, before); assert.equal(h.props.classList.contains('collapsed'), collapsed);
    assert.equal(await h.controller.close(), true);
    assert.equal(h.props.style.width, before); assert.equal(h.props.classList.contains('collapsed'), collapsed);
    assert.equal(h.elements.get('top-area').children.length, 0);
    assert.equal(box.style.width, '340px'); assert.equal(box.style.left, undefined);
  });
}
for (const patch of [{ status: 'deferred-occluded' }, { documentId: 'B' }, { contentRevision: 3 }, { frame: 0 }]) {
  test('DOM binding rolls layout back on an unpresented or mismatched native receipt ' + JSON.stringify(patch), async () => {
    const h = domHarness(280, false);
    h.root.NemoNativeOpacityCutover.presentPreview = async frame => ({ ...h.root.NemoNativeOpacityCutover.identity(), status: 'presented', frame, ...patch });
    assert.equal(await h.controller.open(), false);
    assert.equal(h.elements.get('top-area').children.length, 0);
    assert.equal(h.elements.get('history-modal').style.display, 'none');
    assert.equal(h.elements.get('history-list').children.length, 0);
    assert.ok(h.notices.length);
  });
}
