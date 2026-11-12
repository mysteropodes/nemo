'use strict';
// Shared harness of the native ordinary-palette tests: the real palette controller over a fake DOM and a fake native session.
const ProjectFile = require('../../src/js/adapters/native-object-project-file.js');
const assert = require('node:assert/strict');
const fs = require('node:fs'), vm = require('node:vm');
const Contract = require('../../src/js/adapters/native-palette-contract.js');
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
  let unavailable = options.unavailable === true, publicPresentation = true;
  let presentation = { ...info, owner: 'native', status: 'presented' };
  const calls = [], prompts = [], notices = [], menus = [];
  const projected = pin => ({ ...pin, presentation });
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
    readSubscriptionGeneration: () => ticket, readPresentedObjectSession: refused => {
      if (!publicPresentation && active && typeof refused === 'function') refused('queue-pending');
      return active && !running && publicPresentation ? projected(info) : null; },
    runResourceCommand(action) {
      running++;
      const work = tail.then(() => { const origin = info;
        return action({ dispatch: q => root.NemoNativeApplicationAdapter.createNativeApplicationAdapter('ui', { dispatch }).dispatch(q),
          readPresentedObjectSession: () => active && running === 1 && info === origin ? projected(origin) : null,
          async reconcile(next) { assert.equal(next, revision); info = { ...info, contentRevision: revision, documentSnapshotId: 's' + revision };
            presentation = { ...info, owner: 'native', status: 'presented' };
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
    showToast: text => notices.push(text), prompt: () => prompts.shift(), setTimeout: (fn, ms) => ms ? setImmediate(fn) : setTimeout(fn, 0),
    showContextMenu: (x, y, items) => menus.push(items), addEventListener() {},
    localStorage: { getItem: () => null, setItem() {} }, pushUndo() { throw Error('legacy history'); } };
  if (typeof options.extend === 'function') options.extend(root);
  root.window = root; root.globalThis = root; context = vm.createContext(root);
  const browserRoot = vm.runInContext('globalThis', context);
  function project() { presentation = { ...info, owner: 'native', status: 'presented' }; ProjectFile.projectView(browserRoot, info); }
  project();
  // The ordinary panel loads first, as it does in src/index.html.
  for (const file of ['palette-panel.js', 'adapters/native-presentation-wait.js', 'adapters/native-palette-contract.js', 'adapters/native-application.js',
    'adapters/native-palette-desktop.js', 'adapters/native-palette-ui.js'])
    vm.runInContext(fs.readFileSync(require.resolve('../../src/js/' + file), 'utf8'), context, { filename: file });
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
    replace(next = { documentId: 'replacement', lifecycleGeneration: 2 }) {
      info = { ...info, ...next }; revision = info.contentRevision; controller.willProject(info); project();
    },
    resubscribe() { ticket++; }, retire() { active = false; controller.dispose(); },
    refuse(value = true) { unavailable = value; },
    renewPresentation() { presentation = { ...presentation }; },
    present(value) { publicPresentation = value; },
    rebind() { const replacement = { ...session }; root.NemoNativeObjectProject.desktop = () => replacement; },
    hold() { let entered, release; const started = new Promise(r => entered = r), wait = new Promise(r => release = r);
      block = { entered, wait }; return { started, release }; }
  };
}
module.exports = { fixture, clone, node };
