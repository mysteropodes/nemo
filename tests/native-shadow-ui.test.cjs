'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), vm = require('node:vm');
const Contract = require('../src/js/adapters/native-shadow-contract.js');
const clone = value => JSON.parse(JSON.stringify(value));
const defaults = [
  { id: 'sh1', color: '#ff3355' }, { id: 'sh2', color: '#ff8800' }, { id: 'sh3', color: '#ffdd00' },
  { id: 'sh4', color: '#22cc55' }, { id: 'sh5', color: '#2288ff' }, { id: 'sh6', color: '#aa33ff' }
];
function node(tag = 'div') {
  const listeners = new Map(); let text = '';
  const el = { tagName: tag.toUpperCase(), children: [], parentNode: null, className: '', disabled: false, value: '',
    offsetWidth: 160, offsetHeight: 120, style: { setProperty(key, value) { this[key] = value; } },
    setAttribute(key, value) { this[key] = value; },
    appendChild(child) { child.parentNode = this; this.children.push(child); return child; },
    remove() { if (this.parentNode) this.parentNode.children = this.parentNode.children.filter(n => n !== this); this.parentNode = null; },
    contains(other) { return this === other || this.children.some(n => n.contains(other)); },
    getBoundingClientRect() { return { left: 20, bottom: 30 }; },
    addEventListener(kind, action, options) {
      if (!listeners.has(kind)) listeners.set(kind, []);
      listeners.get(kind).push({ action, capture: options === true || !!options && options.capture === true });
    },
    removeEventListener(kind, action, options) {
      const capture = options === true || !!options && options.capture === true;
      if (listeners.has(kind)) listeners.set(kind, listeners.get(kind).filter(r => r.action !== action || r.capture !== capture));
    },
    listenerCount() { return [...listeners.values()].reduce((total, records) => total + records.length, 0); },
    fire(kind, extra = {}) {
      const event = { target: el, key: '', stopped: false, stopImmediatePropagation() { this.stopped = true; }, ...extra };
      const records = [...(listeners.get(kind) || [])].sort((a, b) => Number(b.capture) - Number(a.capture));
      for (const record of records) { record.action.call(el, event); if (event.stopped) break; }
      return event;
    },
    click() { el.fire('click'); }
  };
  Object.defineProperty(el, 'textContent', { get: () => text, set(value) { text = value; el.children.forEach(n => n.parentNode = null); el.children = []; } });
  Object.defineProperty(el, 'innerHTML', { get: () => text, set(value) { el.textContent = value; } });
  return el;
}
function filesInHtml() {
  const wanted = ['shadow-brush-bridge.js', 'adapters/native-shadow-contract.js', 'adapters/native-application.js',
    'adapters/native-shadow-desktop.js', 'adapters/native-shadow-ui.js'];
  const html = fs.readFileSync(require.resolve('../src/index.html'), 'utf8');
  const actual = [...html.matchAll(/<script\b[^>]*\bsrc="js\/([^"\n]+)"[^>]*>/g)]
    .map(m => m[1]).filter(file => wanted.includes(file));
  assert.deepEqual(actual, wanted, 'production classic-script order must capture Shadow before the SDK');
  return actual;
}
// Controlled native ports below exercise the real caller, protocol, SDK, desktop model and controller.
// They are not an installed app or native-history acceptance receipt.
function fixture(options = {}) {
  const document = Object.assign(node('document'), { body: node('body'), createElement: node,
    getElementById: id => id === 'p-shadowmode' ? checkbox : null });
  const checkbox = Object.assign(node('input'), { checked: false }), events = node('window'), anchor = node('button');
  let info = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 0, documentSnapshotId: 's0' };
  let metadata = clone(options.metadata === undefined ? { shadowPalette: [
    { id: 'duplicate', color: '#aBc' }, { id: 'duplicate', color: '#ABC' }, { id: 'sh1', color: '#00000000' }
  ], shadowActiveId: 'duplicate' } : options.metadata);
  let revision = 0, serial = 0, ticket = 1, active = true, unavailable = false, running = 0;
  let tail = Promise.resolve(), block = null, corrupt = null, controller, context;
  const calls = [], notices = [], history = [clone(metadata)]; let historyIndex = 0, renders = 0;
  const projected = pin => ({ ...pin, presentation: { ...pin, owner: 'native', status: 'presented' } });
  function read(selector) {
    const list = metadata.shadowPalette || [], present = Object.hasOwn(metadata, 'shadowPalette');
    if (selector.kind === 'settings') return { shadowPalettePresent: present, shadowColorCount: list.length,
      ...(Object.hasOwn(metadata, 'shadowActiveId') ? { shadowActiveId: metadata.shadowActiveId } : {}) };
    const end = Math.min(selector.cursor + selector.limit, list.length);
    return { kind: 'shadowPalettePage', shadowPalettePresent: present, shadowColorCount: list.length, cursor: selector.cursor,
      items: list.slice(selector.cursor, end).map((s, i) => ({ ordinal: selector.cursor + i + 1, ...s })), nextCursor: end < list.length ? end : null };
  }
  function apply(action) {
    if (!metadata.shadowPalette || !metadata.shadowPalette.length) metadata.shadowPalette = clone(defaults);
    if (action.kind === 'add') {
      metadata.shadowPalette.push({ id: action.id, color: action.color }); metadata.shadowActiveId = action.id;
    } else if (action.kind === 'select') metadata.shadowActiveId = metadata.shadowPalette[action.ordinal - 1].id;
    else throw Error('unexpected Shadow UI intent');
  }
  async function dispatch(request) {
    calls.push(clone(request));
    if (block) { const held = block; block = null; held.entered(); await held.wait; }
    const response = { apiVersion: 2, requestId: request.requestId, instanceId: info.instanceId,
      documentId: info.documentId, contentRevision: revision, ok: true };
    if (request.documentId !== info.documentId) { response.ok = false; response.error = { code: 'wrong_document', message: 'Replaced' }; }
    else if (unavailable) { response.ok = false; response.error = { code: 'unavailable', message: 'Unavailable' }; }
    else if (request.operation === Contract.EDIT) {
      assert.equal(request.expectedRevision, revision);
      const before = JSON.stringify(metadata); apply(request.payload.action); const applied = before !== JSON.stringify(metadata);
      if (applied) { revision++; history.splice(historyIndex + 1); history.push(clone(metadata)); historyIndex++; }
      response.contentRevision = revision; response.result = { applied, historyEntriesAdded: Number(applied) };
    } else {
      assert.equal(request.operation, Contract.READ); assert.equal(request.payload.atRevision, revision);
      assert.equal(request.payload.documentSnapshotId, 's' + revision);
      response.result = { instanceId: info.instanceId, documentId: info.documentId, atRevision: revision,
        documentSnapshotId: 's' + revision, contextId: 'scene-root', result: read(request.payload.selector) };
    }
    if (corrupt) { const fn = corrupt; corrupt = null; fn(response); }
    return vm.runInContext('JSON.parse(' + JSON.stringify(JSON.stringify(response)) + ')', context);
  }
  const session = { info: () => info, isActive: () => active, requestId: () => 'shadow-' + (++serial),
    readSubscriptionGeneration: () => ticket, readPresentedObjectSession: () => active && !running ? projected(info) : null,
    runResourceCommand(action) {
      running++;
      const work = tail.then(() => { const origin = info;
        return action({ dispatch: q => root.NemoNativeApplicationAdapter.createNativeApplicationAdapter('ui', { dispatch }).dispatch(q),
          readPresentedObjectSession: () => active && running === 1 && info === origin ? projected(origin) : null,
          async reconcile(next) {
            assert.equal(next, revision); info = { ...info, contentRevision: revision, documentSnapshotId: 's' + revision };
            controller.willProject(info); controller.presented(info); return info;
          } });
      }).finally(() => { running--; controller.presented(info); });
      tail = work.catch(() => {}); return work;
    }
  };
  const state = { shadowMode: false };
  for (const key of ['shadowPalette', 'shadowActiveId']) Object.defineProperty(state, key, {
    get() { throw Error('legacy Shadow read'); }, set() { throw Error('legacy Shadow write'); }
  });
  const root = { document, state, SM: { t: value => value }, __TAURI__: { core: {} },
    innerWidth: 800, innerHeight: 600, setTimeout, clearTimeout,
    addEventListener: events.addEventListener.bind(events), removeEventListener: events.removeEventListener.bind(events),
    NemoNativeObjectProject: { desktop: () => session },
    NemoNativeObjectProjectFile: { projectionReadyForUI: () => active && !running },
    showToast: text => notices.push(text), SMEngineBridge: { renderNow() { renders++; } }, renderLabsFloatPanel() {},
    pushUndo() { throw Error('legacy history'); } };
  root.globalThis = root; root.window = root; context = vm.createContext(root);
  for (const file of filesInHtml()) vm.runInContext(fs.readFileSync(require.resolve('../src/js/' + file), 'utf8'), context, { filename: file });
  controller = root.NemoNativeShadowController;
  async function settled() {
    for (let i = 0; i < 30; i++) { await tail; await new Promise(resolve => setImmediate(resolve)); if (!controller.isPending()) return; }
    throw Error('Shadow controller did not settle');
  }
  function popup() { return document.body.children.find(n => n.className === 'ctx-menu shadow-brush-pop'); }
  const swatches = () => popup() ? popup().children[1].children : [];
  return { root, controller, document, events, checkbox, calls, notices, settled, swatches, popup,
    metadata: () => clone(metadata), historyEntries: () => history.length - 1, renders: () => renders,
    edits: () => calls.filter(q => q.operation === Contract.EDIT),
    async ready() { await controller.presented(); await settled(); }, open() { root.SMShadowBrush.toggle(anchor); },
    picker: () => document.body.children.find(n => n.tagName === 'INPUT'),
    refuse() { unavailable = true; }, resubscribe() { ticket++; },
    replace(saved = metadata) {
      metadata = clone(saved); revision++; info = { ...info, documentId: 'reopened-' + revision, lifecycleGeneration: info.lifecycleGeneration + 1,
        contentRevision: revision, documentSnapshotId: 's' + revision }; controller.willProject(info); controller.presented(info);
    },
    history(delta) {
      historyIndex += delta; metadata = clone(history[historyIndex]); revision++;
      info = { ...info, contentRevision: revision, documentSnapshotId: 's' + revision }; controller.willProject(info); controller.presented(info);
    },
    corrupt(fn) { corrupt = fn; }, retire() { active = false; controller.dispose(); root.SMShadowBrush.dispose(); },
    hold() { let entered, release; const started = new Promise(r => entered = r), wait = new Promise(r => release = r);
      block = { entered, wait }; return { started, release }; }
  };
}
test('original popover reads the pinned native palette and duplicate IDs without touching legacy fields', async () => {
  const h = fixture(), before = h.metadata(); await h.ready(); h.open();
  assert.deepEqual(h.swatches().slice(0, -1).map(n => n.title), ['#aBc', '#ABC', '#00000000']);
  assert.deepEqual(h.metadata(), before); assert.equal(h.edits().length, 0);
  assert.equal(h.root.SMShadowBrush.activeSwatch().color, '#aBc');
  const calls = h.calls.length; await h.ready(); assert.equal(h.calls.length, calls);
  const stale = h.swatches()[1]; stale.fire('click'); await h.settled();
  assert.deepEqual(h.edits().at(-1).payload.action, { kind: 'select', ordinal: 2 });
  assert.equal(h.historyEntries(), 0, 'duplicate-ID selection preserves existing first-match persistence semantics');
  assert.equal(h.root.state.shadowMode, true); assert.equal(h.checkbox.checked, true); assert.equal(h.popup(), undefined);
});
test('real picker change performs one atomic add/select and only arms after confirmed presentation', async () => {
  const h = fixture(); await h.ready(); h.open(); h.swatches().at(-1).fire('click');
  const input = h.picker(), held = h.hold(); input.value = '#112233'; input.fire('change'); input.fire('change'); await held.started;
  assert.equal(h.root.state.shadowMode, false); assert.equal(h.controller.read(), null); assert.equal(h.historyEntries(), 0);
  held.release(); await h.settled();
  assert.deepEqual(h.edits().at(-1).payload.action, { kind: 'add', id: 'sh2', color: '#112233' });
  assert.equal(h.edits().length, 1); assert.equal(h.historyEntries(), 1);
  assert.equal(h.metadata().shadowActiveId, 'sh2'); assert.equal(h.root.SMShadowBrush.activeColor(), '#112233');
  assert.equal(h.root.state.shadowMode, true); assert.equal(input.listenerCount(), 0); assert.equal(h.picker(), undefined);
});
test('absent and explicit empty metadata display exactly six defaults without history until explicit selection', async () => {
  for (const metadata of [{}, { shadowPalette: [], shadowActiveId: 'missing' }]) {
    const h = fixture({ metadata }); await h.ready(); h.open();
    assert.deepEqual(h.swatches().slice(0, -1).map(n => n.title), defaults.map(s => s.color));
    assert.deepEqual(h.metadata(), metadata); assert.equal(h.historyEntries(), 0); assert.equal(h.edits().length, 0);
    h.swatches()[3].fire('click'); await h.settled();
    assert.deepEqual(h.metadata(), { shadowPalette: defaults, shadowActiveId: 'sh4' });
    assert.equal(h.historyEntries(), 1); assert.equal(h.root.state.shadowMode, true);
  }
});
test('bounded pages collect every complete swatch with exact order and spelling', async () => {
  const shadowPalette = Array.from({ length: 35 }, (_, i) => ({ id: 'same', color: '#Ab' + i }));
  const h = fixture({ metadata: { shadowPalette, shadowActiveId: 'same' } }); await h.ready(); h.open();
  assert.deepEqual(h.swatches().slice(0, -1).map(n => n.title), shadowPalette.map(s => s.color));
  assert.deepEqual(h.calls.filter(q => q.payload.selector.kind === 'palettePage').map(q => q.payload.selector.cursor), [0, 16, 32]);
  assert.equal(h.edits().length, 0);
});
test('replacement removes picker, retained click and outside/key handlers; stale callback cannot mutate', async () => {
  const h = fixture(); await h.ready(); h.open(); const retained = h.swatches()[2];
  h.swatches().at(-1).fire('click'); const input = h.picker(); input.value = '#abcdef';
  h.replace({ shadowPalette: [{ id: 'other', color: '#123' }] }); await h.settled();
  input.fire('change'); retained.fire('click'); await h.settled();
  assert.equal(h.edits().length, 0); assert.equal(input.listenerCount(), 0); assert.equal(retained.listenerCount(), 0);
  assert.equal(h.document.listenerCount(), 0); assert.equal(h.picker(), undefined); assert.equal(h.root.state.shadowMode, false);
});
test('subscription changes and refused dispatch do not arm or fall back; failure does not self-retry', async () => {
  const h = fixture(); await h.ready(); h.open(); const old = h.swatches()[2]; h.resubscribe(); old.fire('click'); await h.settled();
  assert.equal(h.edits().length, 0); assert.equal(h.root.state.shadowMode, false); await h.ready();
  h.root.SMShadowBrush.close(); h.open(); h.refuse(); h.swatches()[2].fire('click'); await h.settled();
  assert.equal(h.edits().length, 1); assert.equal(h.controller.read(), null); assert.equal(h.root.state.shadowMode, false);
  const count = h.calls.length; await new Promise(r => setImmediate(r)); assert.equal(h.calls.length, count);
});
test('stale in-flight replacement cannot arm and refreshes the new presentation once', async () => {
  const h = fixture(); await h.ready(); h.open(); const held = h.hold(); h.swatches()[2].fire('click'); await held.started;
  h.replace({}); held.release(); await h.settled();
  assert.equal(h.edits().length, 1); assert.equal(h.root.state.shadowMode, false);
  assert.match(h.controller.read().info.documentId, /^reopened-/); assert.deepEqual(h.metadata(), {});
});
test('uncertain native edit receipt never arms or repeats the original callback', async () => {
  const h = fixture(); await h.ready(); h.open(); h.corrupt(r => { r.result.historyEntriesAdded = 2; });
  h.swatches()[2].fire('click'); await h.settled();
  assert.equal(h.edits().length, 1); assert.equal(h.historyEntries(), 1);
  assert.equal(h.root.state.shadowMode, false); assert.equal(h.controller.read(), null);
});
test('controller consumes controlled history and save/reopen projections while retaining one source of metadata', async () => {
  const h = fixture({ metadata: {} }); await h.ready(); h.open(); h.swatches()[4].fire('click'); await h.settled();
  const saved = h.metadata(); h.history(-1); await h.settled(); assert.deepEqual(h.metadata(), {});
  assert.equal(h.root.state.shadowMode, false); h.history(1); await h.settled(); assert.deepEqual(h.metadata(), saved);
  h.replace(clone(saved)); await h.settled(); h.open();
  assert.deepEqual(h.swatches().slice(0, -1).map(n => n.title), defaults.map(s => s.color));
  assert.equal(h.root.SMShadowBrush.activeSwatch().id, 'sh5'); assert.equal(h.root.state.shadowMode, false);
});
test('retirement cancels read publication, DOM callbacks, picker and lifecycle listeners', async () => {
  const h = fixture(), held = h.hold(), reading = h.controller.presented(); await held.started;
  h.retire(); held.release(); await reading; await h.settled(); assert.equal(h.controller.read(), null);
  assert.equal(h.events.listenerCount(), 0); assert.equal(h.document.listenerCount(), 0); assert.equal(h.root.state.shadowMode, false);
});
test('actual tools consumer refuses stale desktop swatches before any path mutation', () => {
  const source = fs.readFileSync(require.resolve('../src/js/shadow-brush-bridge.js'), 'utf8');
  const start = source.indexOf('function applyShadowBrushTag('), end = source.length;
  assert(start >= 0 && end > start);
  const path = new Proxy({}, { get() { throw Error('path read before swatch admission'); }, set() { throw Error('path mutation before swatch admission'); } });
  const root = { __TAURI__: { core: {} }, SMShadowBrush: { activeSwatch: () => null } }; root.window = root;
  vm.createContext(root); vm.runInContext(source.slice(start, end), root);
  const result = root.applyShadowBrushTag(path, false);
  assert.equal(result.status, 'unavailable'); assert.equal(result.code, 'unavailable');
  root.SMShadowBrush = null; assert.equal(root.applyShadowBrushTag(path, false).status, 'unavailable');
});
test('browser fallback keeps original add/select behavior and listener cleanup', async () => {
  const events = node('window'), document = Object.assign(node('document'), { body: node('body'), createElement: node, getElementById: () => null });
  const root = { document, state: { shadowMode: false }, SM: { t: x => x }, innerWidth: 800, innerHeight: 600, setTimeout, clearTimeout,
    addEventListener: events.addEventListener.bind(events), removeEventListener: events.removeEventListener.bind(events) };
  root.window = root; vm.createContext(root);
  vm.runInContext(fs.readFileSync(require.resolve('../src/js/shadow-brush-bridge.js'), 'utf8'), root);
  root.SMShadowBrush.toggle(node('button')); const grid = document.body.children[0].children[1];
  assert.equal(root.state.shadowPalette.length, 6); grid.children.at(-1).fire('click');
  const input = document.body.children.find(n => n.tagName === 'INPUT'); input.value = '#abcdef'; input.fire('change'); input.fire('change');
  assert.equal(root.state.shadowPalette.length, 7); assert.equal(root.state.shadowActiveId, 'sh7'); assert.equal(root.state.shadowMode, true);
  root.SMShadowBrush.dispose(); assert.equal(input.listenerCount(), 0); assert.equal(events.listenerCount(), 0); assert.equal(document.listenerCount(), 0);
});

test('the existing Shadow checkbox is intercepted before the legacy setter and arms only after native selection', async () => {
  const h = fixture({ metadata: {} }); await h.ready(); const held = h.hold(); h.checkbox.checked = true;
  const timeline = fs.readFileSync(require.resolve('../src/js/timeline.js'), 'utf8');
  const setter = timeline.match(/setShadowMode:(function\(v\)\{state\.shadowMode=v;\})/);
  const listener = timeline.match(/document\.getElementById\('p-shadowmode'\)\.addEventListener\('change',function\(\)\{window\.SM\.setShadowMode\(this\.checked\);\}\);/);
  assert(setter && listener, 'exercise the actual original setter and callback');
  vm.runInContext('SM.setShadowMode=' + setter[1] + ';' + listener[0], h.root);
  const event = h.checkbox.fire('change'); await held.started; const stopped = event.stopped;
  assert.equal(stopped, true); assert.equal(h.root.state.shadowMode, false); assert.equal(h.checkbox.checked, false);
  held.release(); await h.settled(); assert.deepEqual(h.edits()[0].payload.action, { kind: 'select', ordinal: 1 });
  assert.equal(h.root.state.shadowMode, true); h.checkbox.checked = false; h.checkbox.fire('change');
  assert.equal(h.root.state.shadowMode, false); assert.equal(h.edits().length, 1);
  h.retire(); assert.equal(h.checkbox.listenerCount(), 1, 'only the original timeline listener remains after capture disposal');
});
test('oversized complete native response and settings/page count mismatch refuse before UI publication', async () => {
  for (const corrupt of [r => { r.result.result.shadowActiveId = '\u0001'.repeat(1024); },
    r => { r.result.result.shadowColorCount = 20; }]) {
    const h = fixture(); h.corrupt(corrupt); await h.ready();
    assert.equal(h.controller.read(), null); assert.equal(h.edits().length, 0); assert.equal(h.root.state.shadowMode, false);
    assert.equal(h.root.SMShadowBrush.palette().length, 0);
  }
});

test('actual tools consumer retains exact current swatch spelling for the existing browser path', () => {
  const source = fs.readFileSync(require.resolve('../src/js/shadow-brush-bridge.js'), 'utf8');
  const start = source.indexOf('function applyShadowBrushTag('), end = source.length;
  const root = { SMShadowBrush: { activeSwatch: () => { assert.equal(path.data.channelTag, 'shadow'); return { id: 'duplicate', color: '#aBc' }; } }, state: { brushSize: 3 } }; root.window = root;
  vm.createContext(root); vm.runInContext(source.slice(start, end), root);
  const path = {}; root.applyShadowBrushTag(path, false);
  assert.equal(path.data.channelTag, 'shadow'); assert.equal(path.data.shadowSwatchId, 'duplicate');
  assert.equal(path.strokeColor, '#aBc'); assert.equal(path.fillColor, null); assert.equal(path.strokeWidth, 3);
});

test('a later explicit popup off or checkbox off revokes pending arming without repeating the native edit', async () => {
  for (const route of ['popup', 'checkbox']) {
    const h = fixture(); await h.ready(); h.open(); const held = h.hold();
    h.swatches()[2].fire('click'); await held.started;
    assert.equal(h.root.state.shadowMode, false); assert.equal(h.checkbox.checked, false);
    if (route === 'popup') {
      assert(h.popup(), 'the actual existing off button remains available during the pending command');
      h.popup().children.at(-1).fire('click');
    } else {
      h.checkbox.checked = false; h.checkbox.fire('change');
    }
    held.release(); await h.settled();
    assert.equal(h.edits().length, 1); assert.equal(h.historyEntries(), 1);
    assert.equal(h.metadata().shadowActiveId, 'sh1', 'already-dispatched metadata command still settles');
    assert.equal(h.root.state.shadowMode, false, route + ' must win over the late successful command');
    assert.equal(h.checkbox.checked, false); assert.equal(h.controller.read().shadow.settings.shadowActiveId, 'sh1');
  }
});

test('the original global Shadow path consumer loads before every later bridge without a new HTML tag', () => {
  const html = fs.readFileSync(require.resolve('../src/index.html'), 'utf8');
  const scripts = [...html.matchAll(/<script\b[^>]*\bsrc="([^"\n]+)"[^>]*>/g)].map(m => m[1]);
  const owner = scripts.indexOf('js/shadow-brush-bridge.js'); assert(owner >= 0);
  assert.equal(scripts.filter(s => s === 'js/shadow-brush-bridge.js').length, 1);
  assert(scripts.indexOf('js/tools.js') < owner);
  for (const caller of ['js/draw-bridge.js', 'js/shape-bridge.js', 'js/pen-bridge.js']) assert(owner < scripts.indexOf(caller));
  const tools = fs.readFileSync(require.resolve('../src/js/tools.js'), 'utf8');
  assert(!tools.includes('function applyShadowBrushTag('));
  assert.match(tools, /var _pen=\{path:null,/);
  const timeline = fs.readFileSync(require.resolve('../src/js/timeline.js'), 'utf8');
  assert.match(timeline, /if\(t!=='pen'&&_pen\.path\)finalizePen\(\)/);
});
