'use strict';
const ProjectFile = require('../src/js/adapters/native-object-project-file.js');
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), vm = require('node:vm');
const Contract = require('../src/js/adapters/native-palette-contract.js');
const clone = value => JSON.parse(JSON.stringify(value));
function node(tag = 'div') {
  const listeners = new Map(); let text = '';
  const el = { tagName: tag.toUpperCase(), children: [], dataset: {}, className: '', value: '', disabled: false,
    style: { setProperty(key, value) { this[key] = value; } }, classList: { toggle() {} },
    setAttribute(key, value) { this[key] = value; },
    addEventListener(key, fn) { if (!listeners.has(key)) listeners.set(key, []); listeners.get(key).push(fn); },
    appendChild(child) { this.children.push(child); return child; },
    fire(key, extra = {}) {
      const event = { target: el, currentTarget: el, clientX: 0, clientY: 0, preventDefault() {},
        dataTransfer: {}, ...extra };
      for (const fn of listeners.get(key) || []) fn(event);
    }
  };
  Object.defineProperty(el, 'textContent', { get: () => text, set(value) { text = value; el.children = []; } });
  Object.defineProperty(el, 'innerHTML', { get: () => text, set(value) { text = value; el.children = []; } });
  return el;
}
function fixture(options = {}) {
  const names = ['palette-grid', 'palette-tabs', 'btn-palette-new', 'btn-palette-add-fill', 'btn-palette-add-stroke',
    'btn-palette-replace', 'btn-palette-swap', 'palette-replace-scope', 'color-fill', 'color-stroke'];
  const elements = Object.fromEntries(names.map(name => [name, node()]));
  elements['color-fill'].value = '#aBc'; elements['color-stroke'].value = '#010203';
  const document = Object.assign(node('document'), { readyState: 'loading', createElement: node,
    getElementById: id => elements[id] || null });
  let info = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 0, documentSnapshotId: 's0',
    canvasW: 320, canvasH: 180, fps: 24, totalFrames: 40, layers: [{ layerUid: 'drawing', visible: true }] };
  let metadata = { palettes: [
    { id: 'same', name: 'First', colors: ['#AbC', '#abc', '#00000000'] },
    { id: 'same', name: 'Second', colors: ['#DEF'] }
  ], activePaletteIdx: 0, ...options.metadata };
  for (const key of Object.keys(metadata)) if (metadata[key] === undefined) delete metadata[key];
  let revision = 0, ticket = 1, serial = 0, active = true, running = 0, tail = Promise.resolve(), block = null;
  let unavailable = options.unavailable === true;
  const calls = [], prompts = [], notices = [], menus = [];
  const projected = pin => ({ ...pin, presentation: { ...pin, owner: 'native', status: 'presented' } });
  let context, controller;
  function read(selector) {
    const list = metadata.palettes || [];
    if (selector.kind === 'settings') return { palettesPresent: Object.hasOwn(metadata, 'palettes'), paletteCount: list.length,
      ...Object.fromEntries(['activePaletteIdx'].filter(k => Object.hasOwn(metadata, k)).map(k => [k, metadata[k]])),
      effectiveActivePaletteIdx: metadata.activePaletteIdx != null && metadata.activePaletteIdx < (metadata.palettes ? list.length : 1) ? metadata.activePaletteIdx : 0 };
    const colors = selector.kind === 'colorPage', total = colors ? list[selector.ordinal - 1].colors.length : list.length;
    const all = colors ? list[selector.ordinal - 1].colors.map((color, i) => ({ index: i + 1, color })) :
      list.map((p, i) => ({ ordinal: i + 1, id: p.id, name: p.name, colorCount: p.colors.length }));
    const end = Math.min(selector.cursor + selector.limit, total);
    return { kind: colors ? 'paletteColorPage' : 'palettePage', cursor: selector.cursor,
      items: all.slice(selector.cursor, end), nextCursor: end < total ? end : null,
      ...(colors ? { paletteOrdinal: selector.ordinal, colorCount: total } :
        { palettesPresent: Object.hasOwn(metadata, 'palettes'), paletteCount: total }) };
  }
  function apply(action) {
    const list = metadata.palettes || [], i = action.ordinal - 1;
    switch (action.kind) {
      case 'create': metadata.palettes = list; list.push({ id: action.id, name: action.name, colors: [] }); metadata.activePaletteIdx = list.length - 1; break;
      case 'select': metadata.activePaletteIdx = i; break;
      case 'rename': list[i].name = action.name; break;
      case 'duplicate': list.splice(i + 1, 0, { id: action.id, name: action.name, colors: list[i].colors.slice() }); metadata.activePaletteIdx = i + 1; break;
      case 'remove': list.splice(i, 1); if (metadata.activePaletteIdx >= list.length) metadata.activePaletteIdx = list.length - 1; break;
      case 'appendColor': if (!list[i].colors.includes(action.color)) list[i].colors.push(action.color); break;
      case 'removeColor': list[i].colors.splice(action.index - 1, 1); break;
      case 'moveColor': { const [color] = list[i].colors.splice(action.from - 1, 1); list[i].colors.splice(action.to - 1, 0, color); break; }
      default: throw Error('unexpected UI intent');
    }
  }
  async function dispatch(request) {
    calls.push(clone(request));
    if (block) { const held = block; block = null; held.entered(); await held.wait; }
    const response = { apiVersion: 2, requestId: request.requestId, instanceId: info.instanceId,
      documentId: info.documentId, contentRevision: revision, ok: !unavailable };
    if (request.documentId !== info.documentId) {
      response.ok = false; response.error = { code: 'wrong_document', message: 'Document was replaced' };
    } else if (unavailable) response.error = { code: 'unavailable', message: 'Palette is unavailable' };
    else if (request.operation === Contract.EDIT) {
      assert.equal(request.expectedRevision, revision);
      const before = JSON.stringify(metadata); apply(request.payload.action);
      const applied = before !== JSON.stringify(metadata); if (applied) revision++;
      response.contentRevision = revision; response.result = { applied, historyEntriesAdded: Number(applied) };
    } else {
      assert.equal(request.operation, Contract.READ); assert.equal(request.payload.atRevision, revision);
      assert.equal(request.payload.documentSnapshotId, 's' + revision);
      response.result = { instanceId: info.instanceId, documentId: info.documentId, atRevision: revision,
        documentSnapshotId: 's' + revision, contextId: 'scene-root', result: read(request.payload.selector) };
    }
    return vm.runInContext('JSON.parse(' + JSON.stringify(JSON.stringify(response)) + ')', context);
  }
  const session = { info: () => info, isActive: () => active, requestId: () => 'palette-' + (++serial),
    readSubscriptionGeneration: () => ticket, readPresentedObjectSession: () => active && !running ? projected(info) : null,
    runResourceCommand(action) {
      running++;
      const work = tail.then(() => { const origin = info;
        return action({ dispatch: q => root.NemoNativeApplicationAdapter.createNativeApplicationAdapter('ui', { dispatch }).dispatch(q),
          readPresentedObjectSession: () => active && running === 1 && info === origin ? projected(origin) : null,
          async reconcile(next) { assert.equal(next, revision); info = { ...info, contentRevision: revision, documentSnapshotId: 's' + revision };
            controller.willProject(info); assert.equal(ProjectFile.refreshLayerTable(browserRoot, info), true);
            controller.presented(info); return info; } });
      }).finally(() => { running--; controller.presented(info); });
      tail = work.catch(() => {}); return work;
    }
  };
  const state = {};
  for (const key of ['palettes', 'activePaletteIdx']) Object.defineProperty(state, key, {
    get() { throw Error('legacy palette read'); }, set() { throw Error('legacy palette write'); }
  });
  const root = { document, state, __TAURI__: { core: {} }, SM: { t: x => x,
    setFillColor() { throw Error('legacy fill writer'); }, setStrokeColor() { throw Error('legacy stroke writer'); } },
    NemoNativeObjectProject: { desktop: () => session },
    NemoNativeObjectProjectFile: { projectionReadyForUI: (valueRoot, value) => active && !running && ProjectFile.projectionReadyForUI(valueRoot, value) },
    showToast: text => notices.push(text), prompt: () => prompts.shift(),
    showContextMenu: (x, y, items) => menus.push(items), addEventListener() {},
    localStorage: { getItem: () => null, setItem() {} }, pushUndo() { throw Error('legacy history'); } };
  root.window = root; root.globalThis = root; context = vm.createContext(root);
  const browserRoot = vm.runInContext('globalThis', context);
  function project() { ProjectFile.projectView(browserRoot, info); }
  project();
  // The ordinary panel loads first, as it does in src/index.html.
  for (const file of ['palette-panel.js', 'adapters/native-palette-contract.js', 'adapters/native-application.js',
    'adapters/native-palette-desktop.js', 'adapters/native-palette-ui.js'])
    vm.runInContext(fs.readFileSync(require.resolve('../src/js/' + file), 'utf8'), context, { filename: file });
  controller = root.NemoNativePaletteController; document.fire('DOMContentLoaded');
  async function settled() {
    for (let i = 0; i < 30; i++) { await tail; await new Promise(r => setImmediate(r)); if (!controller.isPending()) return; }
    throw Error('palette controller did not settle');
  }
  return { root, elements, controller, calls, prompts, notices, menus, settled,
    edits: () => calls.filter(q => q.operation === Contract.EDIT),
    metadata: () => clone(metadata), tabs: () => elements['palette-tabs'].children, swatches: () => elements['palette-grid'].children,
    async ready() { await controller.presented(); await settled(); },
    fullInfo: () => info, projectionReady: value => ProjectFile.projectionReadyForUI(browserRoot, value),
    drift() { root.state.canvasW++; }, repairProjection: project,
    replace() { info = { ...info, documentId: 'replacement', lifecycleGeneration: 2 }; controller.willProject(info); project(); },
    resubscribe() { ticket++; }, retire() { active = false; controller.dispose(); },
    refuse() { unavailable = true; },
    hold() { let entered, release; const started = new Promise(r => entered = r), wait = new Promise(r => release = r);
      block = { entered, wait }; return { started, release }; }
  };
}

test('ordinary palette uses the actual application SDK and preserves duplicate IDs and exact colors without a JS mirror', async () => {
  const h = fixture(), before = h.metadata(); await h.ready();
  assert.deepEqual(h.tabs().map(n => n.textContent), ['First', 'Second']);
  assert.deepEqual(h.swatches().map(n => n.title), ['#AbC', '#abc', '#00000000']);
  assert.deepEqual(h.metadata(), before); assert.equal(h.edits().length, 0);
  const count = h.calls.length; await h.ready(); assert.equal(h.calls.length, count);
  assert.equal(h.elements['btn-palette-new'].disabled, false);
  assert.equal(h.elements['btn-palette-replace'].disabled, true); assert.equal(h.elements['btn-palette-swap'].disabled, true);
});

test('ordinary palette presentation collects complete summaries and active colors across bounded SDK pages', async () => {
  const palettes = Array.from({ length: 19 }, (_, i) => ({ id: 'duplicate', name: 'Palette ' + i,
    colors: i === 18 ? Array.from({ length: 35 }, (_, j) => '#' + j.toString(16).padStart(6, '0')) : [] }));
  const h = fixture({ metadata: { palettes, activePaletteIdx: 18 } }); await h.ready();
  assert.equal(h.tabs().length, 19); assert.equal(h.swatches().length, 35);
  assert.deepEqual(h.calls.filter(q => q.payload.selector.kind === 'palettePage').map(q => q.payload.selector.cursor), [0, 16]);
  assert.deepEqual(h.calls.filter(q => q.payload.selector.kind === 'colorPage').map(q => q.payload.selector.cursor), [0, 16, 32]);
  assert.equal(h.edits().length, 0); assert.deepEqual(h.metadata().palettes, palettes);
});

test('ordinary create, rename, duplicate, select and delete submit field-specific native intents', async () => {
  const h = fixture(); await h.ready(); h.prompts.push('New'); h.elements['btn-palette-new'].fire('click'); await h.settled();
  assert.equal(h.edits().at(-1).payload.action.kind, 'create'); assert.equal(h.controller.read().palette.active.summary.name, 'New');
  h.prompts.push('Renamed'); h.tabs()[2].fire('click'); h.tabs()[2].fire('dblclick'); await h.settled();
  assert.deepEqual(h.edits().at(-1).payload.action, { kind: 'rename', ordinal: 3, name: 'Renamed' });
  h.tabs()[2].fire('contextmenu'); h.menus.at(-1).find(x => x.label === 'Dupliquer').action(); await h.settled();
  assert.equal(h.edits().at(-1).payload.action.kind, 'duplicate'); assert.equal(h.metadata().palettes.length, 4);
  h.tabs()[0].fire('click'); await h.settled(); assert.deepEqual(h.edits().at(-1).payload.action, { kind: 'select', ordinal: 1 });
  h.tabs()[3].fire('contextmenu'); h.menus.at(-1).find(x => x.label === 'Supprimer').action(); await h.settled();
  assert.deepEqual(h.edits().at(-1).payload.action, { kind: 'remove', ordinal: 4 }); assert.equal(h.metadata().palettes.length, 3);
});

test('ordinary add, remove and drag reorder colors use one-based native positions and confirmed readback', async () => {
  const h = fixture(); await h.ready(); h.elements['btn-palette-add-fill'].fire('click'); await h.settled();
  assert.deepEqual(h.edits().at(-1).payload.action, { kind: 'appendColor', ordinal: 1, color: '#aBc' });
  h.swatches()[1].fire('contextmenu'); h.menus.at(-1)[0].action(); await h.settled();
  assert.deepEqual(h.edits().at(-1).payload.action, { kind: 'removeColor', ordinal: 1, index: 2 });
  const before = h.metadata(); h.swatches()[0].fire('dragstart');
  assert.deepEqual(h.metadata(), before); const count = h.edits().length;
  h.swatches()[2].fire('drop'); await h.settled(); assert.equal(h.edits().length, count + 1);
  assert.deepEqual(h.edits().at(-1).payload.action, { kind: 'moveColor', ordinal: 1, from: 1, to: 3 });
  assert.deepEqual(h.swatches().map(n => n.title), ['#00000000', '#aBc', '#AbC']);
});

test('retained menu, prompt and drag cannot target the same ordinal after document replacement', async () => {
  for (const kind of ['menu', 'prompt', 'drag']) {
    const h = fixture(); await h.ready(); let action;
    if (kind === 'menu') { h.tabs()[0].fire('contextmenu'); action = h.menus.at(-1).find(x => x.label === 'Supprimer').action; }
    if (kind === 'drag') { h.swatches()[0].fire('dragstart'); const target = h.swatches()[1]; action = () => target.fire('drop'); }
    if (kind === 'prompt') { h.root.prompt = () => { h.replace(); return 'Stale'; }; h.tabs()[0].fire('dblclick'); }
    else { h.replace(); await h.ready(); action(); }
    await h.settled(); assert.equal(h.edits().length, 0); assert(h.notices.length > 0);
  }
});

test('subscription change, unavailable admission and retirement refuse ordinary writes without browser fallback', async () => {
  const h = fixture(); await h.ready(); const old = h.tabs()[1]; h.resubscribe(); old.fire('click'); await h.settled();
  assert.equal(h.edits().length, 0); await h.ready(); h.refuse(); h.elements['btn-palette-add-stroke'].fire('click'); await h.settled();
  assert.equal(h.edits().length, 1); assert.equal(h.controller.read(), null); assert(h.notices.length > 0);
  h.retire(); h.elements['btn-palette-new'].fire('click'); await h.settled(); assert.equal(h.edits().length, 1);
});

test('pending edit retires gestures and never publishes an optimistic palette', async () => {
  const h = fixture(); await h.ready(); const held = h.hold(), prior = h.metadata(), retained = h.tabs()[1];
  h.elements['btn-palette-add-fill'].fire('click'); await held.started;
  assert.equal(h.controller.read(), null); assert.equal(h.elements['btn-palette-new'].disabled, true);
  retained.fire('click'); assert.equal(h.edits().length, 1); assert.deepEqual(h.metadata(), prior);
  held.release(); await h.settled(); assert.equal(h.edits().length, 1); assert.equal(h.swatches().length, 4);
});

test('raw invalid active selection and missing palettes do not materialize defaults during ordinary presentation', async () => {
  for (const metadata of [{ palettes: undefined }, { activePaletteIdx: -1 }, { activePaletteIdx: 0.5 }, { palettes: [] }]) {
    const h = fixture({ metadata });
    // JSON import represents absence rather than an explicit undefined field.
    await h.ready(); assert.equal(h.controller.read().palette.active, null); assert.equal(h.edits().length, 0);
    assert.equal(h.elements['btn-palette-new'].disabled, false); assert.equal(h.elements['btn-palette-add-fill'].disabled, true);
  }
});

test('retirement during paged palette read cannot publish stale controls', async () => {
  const h = fixture(), held = h.hold(), reading = h.controller.presented(); await held.started;
  h.retire(); held.release(); assert.equal(await reading, null); await h.settled();
  assert.equal(h.controller.read(), null); assert.equal(h.elements['btn-palette-new'].disabled, true);
});

test('a new presented document during a held read is refreshed once after the stale read settles', async () => {
  const h = fixture(), held = h.hold(), reading = h.controller.presented(); await held.started;
  h.replace(); h.controller.presented(); held.release(); await reading; await h.settled();
  assert.equal(h.controller.read().info.documentId, 'replacement');
  assert.equal(h.elements['btn-palette-new'].disabled, false);
  const settings = h.calls.filter(q => q.payload.selector && q.payload.selector.kind === 'settings');
  assert.equal(settings.length, 2); assert.deepEqual(settings.map(q => q.documentId), ['d', 'replacement']);
});

test('unavailable read does not retry itself and a new presentation after pending failure remains consumable', async () => {
  const unavailable = fixture({ unavailable: true }); await unavailable.ready();
  assert.equal(unavailable.calls.length, 1); assert.equal(unavailable.controller.read(), null);
  const h = fixture(); await h.ready(); const before = h.metadata(), held = h.hold(); h.elements['btn-palette-add-fill'].fire('click'); await held.started;
  h.replace(); h.controller.presented(); held.release(); await h.settled();
  assert.equal(h.controller.read().info.documentId, 'replacement');
  assert.equal(h.edits().length, 1); assert.equal(h.elements['btn-palette-new'].disabled, false);
  assert.deepEqual(h.metadata(), before);
});

test('palette UI installation leaves browser palette DOM and button state unchanged', async () => {
  const elements = Object.fromEntries(['palette-grid', 'palette-tabs', 'btn-palette-new'].map(id => [id, node()]));
  elements['palette-grid'].textContent = 'browser palette'; elements['palette-tabs'].textContent = 'browser tabs';
  const root = { document: { getElementById: id => elements[id] || null }, addEventListener() {} };
  const UI = require('../src/js/adapters/native-palette-ui.js'), controller = UI.install(root);
  controller.render(); await controller.presented(); controller.dispose();
  assert.equal(elements['palette-grid'].textContent, 'browser palette');
  assert.equal(elements['palette-tabs'].textContent, 'browser tabs'); assert.equal(elements['btn-palette-new'].disabled, false);
  assert.strictEqual(UI.install(root), controller);
});

test('actual HTML loads the palette protocol before SDK capture and the model before ordinary UI', () => {
  const wanted = ['native-palette-contract.js', 'native-application.js', 'native-palette-desktop.js', 'native-palette-ui.js'];
  const html = fs.readFileSync(require.resolve('../src/index.html'), 'utf8');
  const actual = [...html.matchAll(/<script\b[^>]*\bsrc="js\/adapters\/([^"\n]+)"[^>]*>/g)]
    .map(match => match[1]).filter(file => wanted.includes(file));
  assert.deepEqual(actual, wanted);
  function load(files) {
    const root = { document: { getElementById: () => null }, addEventListener() {} };
    root.globalThis = root; const context = vm.createContext(root);
    for (const file of files) vm.runInContext(fs.readFileSync(require.resolve('../src/js/adapters/' + file), 'utf8'), context);
    return () => vm.runInContext(`NemoNativeApplicationAdapter.validateRequest({apiVersion:2,requestId:'html-palette',
      instanceId:'i',documentId:'d',operation:'query.document.object.project.palette.read',payload:{atRevision:0,
      documentSnapshotId:'s0',contextId:'scene-root',selector:{kind:'settings'}}})`, context);
  }
  assert.doesNotThrow(load(actual));
  assert.throws(load([wanted[1], wanted[0], wanted[2], wanted[3]]), /^TypeError: operation is not admitted$/);
});

test('palette uses full project readiness and retains narrow immutable authority pins', async () => {
  const f = fixture(); await f.ready();
  assert.equal(f.projectionReady(f.fullInfo()), true);
  const origin = f.controller.read(); assert.ok(origin);
  assert.deepEqual(Object.keys(origin.info).sort(),
    ['contentRevision', 'documentId', 'documentSnapshotId', 'instanceId', 'lifecycleGeneration']);
  assert.equal(Object.isFrozen(origin.info), true);
  assert.equal(f.projectionReady(origin.info), false, 'identity pins cannot stand in for project metadata');
  const calls = f.calls.length;
  f.drift(); assert.equal(f.controller.read(), null);
  assert.equal(await f.controller.presented(), null); await f.settled();
  assert.equal(f.controller.read(), null);
  assert.equal(f.calls.length, calls, 'invalid full projection refuses before dispatch');
  assert.equal(f.edits().length, 0);
  f.repairProjection(); await f.controller.presented(); await f.settled();
  assert.ok(f.controller.read());
  assert.equal(f.edits().length, 0, 'restoring readiness does not replay a gesture');
});
