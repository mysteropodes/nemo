'use strict';
// #2256 leaf 3: entering and leaving a Component on a native object project. The real pieces run: the layer-list row (its dblclick),
// the controller, the SM entry replacement, the native adapter and its contracts, the scene projection (projectView) and the real
// renderSymbolTabs of timeline.js (extracted). The native owner is a scripted double that answers the component read and the
// workspace transition from a small model; the legacy enterSymbol / exitToScene / closeSymbolTab are recorders that must NEVER run on a
// native document. The viewport is a recorder that notes WHICH context is presented when it is asked to render.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const Context = require('../src/js/adapters/native-component-context.js');
const EnterUI = require('../src/js/adapters/native-component-enter-ui.js');
const Presenter = require('../src/js/adapters/native-layer-list-presenter.js');
const Component = require('../src/js/adapters/native-object-component.js');
const Files = require('../src/js/adapters/native-object-project-file.js');
const SDK = require('../src/js/adapters/native-application.js');

const SRC = path.join(__dirname, '..', 'src', 'js');
const SNAP = (revision) => 'native-object:document-A:' + revision;
const clone = (value) => JSON.parse(JSON.stringify(value));
const settle = async () => { for (let i = 0; i < 24; i++) await new Promise((done) => setImmediate(done)); };

// The model: a 24-frame scene with an instance of sym_a (12 frames, 12 fps) and another of sym_b (6 frames).
function model() {
  return { revision: 7, comps: { sym_a: { name: 'Comp A', totalFrames: 12, fps: 12, layers: [['ia', 'Inner A', 'drawing'], ['ib', null, 'null']], instances: ['L2'] },
    sym_b: { name: 'Comp B', totalFrames: 6, fps: null, layers: [['ic', 'Inner C', 'drawing']], instances: ['L3'] } } };
}
const SCENE_LAYERS = [['L1', 'Plain', 'drawing'], ['L2', 'Inst A', 'instance'], ['L3', 'Inst B', 'instance']];
function readAnswer(m, request) {
  const p = request.payload, s = p.selector, head = { atRevision: p.atRevision, documentSnapshotId: p.documentSnapshotId, kind: s.kind };
  const page = (rows, countKey, extra) => { const take = rows.slice(s.cursor, s.cursor + s.limit), end = s.cursor + take.length;
    return Object.assign(head, extra, { [countKey]: rows.length, cursor: s.cursor, items: take, nextCursor: end < rows.length ? end : null }); };
  const all = [{ contextId: 'scene-root', contextKind: 'scene', name: null, totalFrames: 24, layerCount: 3 }]
    .concat(Object.entries(m.comps).map(([id, c]) => ({ contextId: id, contextKind: 'component', name: c.name, totalFrames: c.totalFrames, layerCount: c.layers.length })));
  if (s.kind === 'contexts') return page(all, 'contextCount');
  const comp = m.comps[s.contextId];
  const rows = (list) => list.map(([layerUid, name, layerKind]) => ({ layerUid, name, layerKind }));
  if (s.kind === 'layers') return page(rows(comp ? comp.layers : SCENE_LAYERS), 'layerCount', { contextId: s.contextId });
  return page(comp.instances.map((layerUid) => ({ layerUid })), 'instanceCount', { contextId: s.contextId });
}
// The baseline rules of enterSymbol / exitToScene / closeSymbolTab (stated independently of the Rust kernel).
function workspaceAnswer(m, request) {
  const ws = clone(request.payload.workspace), action = request.payload.action, comps = m.comps;
  const shown = (outcome) => { const a = ws.activeContextId, meta = a === null ? { totalFrames: 24, fps: 24 } : comps[a];
    return { atRevision: request.payload.atRevision, documentSnapshotId: request.payload.documentSnapshotId, outcome, contextId: a === null ? 'scene-root' : a,
      totalFrames: meta.totalFrames, fps: meta.fps || 24, workspace: Object.assign(ws, { openTabs: ws.openTabs.filter((id) => comps[id]) }) }; };
  if (ws.activeContextId !== null && !comps[ws.activeContextId]) { ws.view = ws.sceneView; ws.sceneView = null; ws.activeContextId = null; if (action.kind === 'refresh') return shown('context_gone'); }
  const leave = () => { ws.view = ws.sceneView; ws.sceneView = null; ws.activeContextId = null; };
  if (action.kind === 'enter') {
    if (ws.activeContextId === action.contextId) return shown('already_active');
    if (ws.activeContextId !== null) return shown('refused_nested');
    const meta = comps[action.contextId];
    if (!meta) return null;
    ws.sceneView = ws.view; ws.activeContextId = action.contextId; ws.view = { frame: action.fromLayerUid && m.entryFrames ? Math.min(meta.totalFrames - 1, m.entryFrames[action.fromLayerUid] || 0) : (action.entryFrame || 0), workIn: 0, workOut: meta.totalFrames - 1 };
    if (!ws.openTabs.includes(action.contextId)) ws.openTabs.push(action.contextId);
    while (ws.openTabs.length > 8) ws.openTabs.splice(ws.openTabs.findIndex((id) => id !== ws.activeContextId), 1);
    return shown('entered');
  }
  if (action.kind === 'exit') { if (ws.activeContextId === null) return shown('not_in_context'); leave(); return shown('exited'); }
  if (action.kind === 'close') { if (ws.activeContextId === action.contextId) leave(); const at = ws.openTabs.indexOf(action.contextId); if (at >= 0) ws.openTabs.splice(at, 1); return shown(at >= 0 ? 'closed' : 'close_noop'); }
  return shown('refreshed');
}

// A tiny DOM: enough for the presenter row, the paint and the tab strip (events dispatched through real listeners).
function element(tag) {
  const listeners = {}, kids = [], classes = new Set();
  const el = { tagName: tag, style: {}, dataset: {}, attributes: {}, textContent: '', type: '', value: '', hidden: false, disabled: false, parentNode: null,
    classList: { add: (x) => classes.add(x), remove: (x) => classes.delete(x), contains: (x) => classes.has(x) },
    setAttribute(k, v) { this.attributes[k] = String(v); }, addEventListener(k, f) { (listeners[k] = listeners[k] || []).push(f); },
    replaceChildren(...next) { kids.splice(0).forEach((k) => { k.parentNode = null; }); next.forEach((k) => this.appendChild(k)); },
    appendChild(k) { if (k.parentNode) k.parentNode.removeChild(k); kids.push(k); k.parentNode = this; return k; },
    removeChild(k) { kids.splice(kids.indexOf(k), 1); k.parentNode = null; },
    fire(type, extra) { (listeners[type] || []).forEach((f) => f(Object.assign({ target: el, preventDefault() {}, stopPropagation() {} }, extra))); } };
  Object.defineProperty(el, 'className', { get: () => [...classes].join(' '), set(v) { classes.clear(); String(v).split(/\s+/).filter(Boolean).forEach((x) => classes.add(x)); } });
  Object.defineProperty(el, 'children', { get: () => kids.slice() });
  Object.defineProperty(el, 'innerHTML', { set() { this.replaceChildren(); }, get: () => '' });
  return el;
}
function documentFor() {
  const nodes = new Map(['layer-list', 'frame-hdr', 'frame-grid', 'tl-cf', 'tl-tf', 'info-frame', 'playhead', 'playhead-flag', 'symbol-tabs'].map((id) => [id, Object.assign(element('div'), { id })]));
  return { nodes, activeElement: null, getElementById: (id) => nodes.get(id) || null, createElement: element };
}
// The real renderSymbolTabs of timeline.js, run against the fake page (window = the host).
function realRenderSymbolTabs(host) {
  const text = fs.readFileSync(path.join(SRC, 'timeline.js'), 'utf8'), start = text.indexOf('function renderSymbolTabs(){'), end = text.indexOf('// Footage panel', start);
  assert.ok(start > 0 && end > start, 'renderSymbolTabs located');
  const context = vm.createContext({ window: host, document: host.document, state: host.state });
  vm.runInContext(text.slice(start, end), context);
  return vm.runInContext('renderSymbolTabs', context);
}

function fixture(options) {
  options = options || {};
  const m = model(), effects = [], toasts = [], renders = [], requests = [];
  let ready = true;
  const info = () => ({ apiVersion: 2, instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 1, contentRevision: m.revision, documentSnapshotId: SNAP(m.revision),
    canvasW: 320, canvasH: 180, fps: 24, totalFrames: 24, layers: SCENE_LAYERS.map(([layerUid, name]) => ({ layerUid, name })) });
  const sdk = SDK.createNativeApplicationAdapter('component-context-test', { async dispatch(request) {
    requests.push(clone(request));
    if (options.hold && request.operation === 'query.document.object.workspace.context' && request.payload.action.kind === options.hold.kind) await options.hold.promise;
    const base = { apiVersion: 2, requestId: request.requestId, instanceId: 'instance-A', documentId: 'document-A', contentRevision: m.revision };
    if (m.failRefresh > 0 && request.operation === 'query.document.object.workspace.context' && request.payload.action.kind === 'refresh') {
      m.failRefresh -= 1; return Object.assign(base, { ok: false, error: { code: 'unavailable', message: 'the owner could not answer' } });
    }
    const result = request.operation === 'query.document.object.workspace.context' ? workspaceAnswer(m, request) : readAnswer(m, request);
    return result ? Object.assign(base, { ok: true, result }) : Object.assign(base, { ok: false, error: { code: 'not_found', message: 'no such context' } });
  } });
  const host = { document: documentFor(), userLayers: [], _layerSel: [0], selectedPaths: [], showToast: (text) => toasts.push(text),
    state: { activeLayerIdx: 0, currentFrame: 0, activeSymbolId: null, openSymbolTabs: [] },
    SM: { enterSymbol: () => effects.push('legacy-enter'), exitToScene: () => effects.push('legacy-exit'), closeSymbolTab: () => effects.push('legacy-close'), t: (key) => key },
    updateUI: () => { effects.push('updateUI'); if (host.NemoNativeComponentContext.paint(host) !== true) effects.push('scene-paint'); host.renderSymbolTabs(); },
    NemoNativeObjectProjectFile: Files, NemoNativeComponentContext: Context, NemoNativeObjectComponent: Component };
  const session = {
    isActive: () => ready, blocksLegacy: () => !options.legacyDocument, info, requestId: (label) => label + '-' + (requests.length + 1),
    async runResourceCommand(action) { return action({ dispatch: (request) => sdk.dispatch(request), reconcile: async () => info() }); },
    async renderPreview(frame) {
      renders.push({ frame, presented: Context.presented(host) });
      if (options.failPresent && Context.presented(host)) throw new Error('the viewport refused the context');
    },
  };
  host.NemoNativeObjectProject = { desktop: () => session };
  host.renderSymbolTabs = realRenderSymbolTabs(host);
  Files.projectView(host, info(), null);
  const ports = { currentSession: () => (options.legacyDocument ? null : session), projectionReady: () => true, selectLayer: () => true };
  const policy = { bindPolicyEntries: () => true, disposePolicyEntries: () => effects.push('policy-disposed') };
  const composed = EnterUI.compose(host, ports, policy);
  return { host, m, session, effects, toasts, renders, requests, composed, ports, setReady: (v) => { ready = v; },
    workspaceRequests: () => requests.filter((r) => r.operation === 'query.document.object.workspace.context'),
    // The real presenter row of a scene layer, built with the open port the native list gives it.
    row(index) {
      const context = { document: host.document, info: info(), selection: [], shyMode: false, policyOwner: null, onPick: () => false, onVisibility: () => false, onPolicy: () => false,
        onOpen: (layer) => composed.openLayer(layer.layerUid) };
      return Presenter.render(host.document.getElementById('layer-list'), context).rows[index];
    },
    tabs: () => host.document.getElementById('symbol-tabs').children.map((tab) => ({ text: tab.children.length ? tab.children[0].textContent : tab.textContent, act: tab.className.includes('act') })) };
}

test('double-clicking an instance layer enters its component through ONE native workspace request and presents the component context', async () => {
  const f = fixture();
  assert.equal(f.composed.bindPolicyEntries(), true);
  f.host.state.currentFrame = 5; f.host.state.waIn = 0; f.host.state.waOut = 23;
  f.row(1).fire('dblclick'); await settle();
  assert.equal(f.workspaceRequests().length, 1, 'one native workspace transition');
  assert.deepEqual(f.workspaceRequests()[0].payload.workspace, { activeContextId: null, openTabs: [], view: { frame: 5, workIn: 0, workOut: 23 }, sceneView: null });
  assert.deepEqual(f.workspaceRequests()[0].payload.action, { kind: 'enter', contextId: 'sym_a', entryFrame: null, fromLayerUid: 'L2' }, 'the instance layer is named so the owner can resolve its frame');
  assert.equal('expectedRevision' in f.workspaceRequests()[0], false);
  assert.deepEqual(f.effects.filter((e) => e.startsWith('legacy')), [], 'the legacy enterSymbol never ran');
  // The viewport was asked once, for the component context at its first frame, after the context was entered.
  assert.deepEqual(f.renders, [{ frame: 0, presented: { contextId: 'sym_a', totalFrames: 12, fps: 12 } }]);
  const s = f.host.state;
  assert.equal(s.activeSymbolId, 'sym_a');
  assert.equal(s.totalFrames, 12); assert.equal(s.fps, 12); assert.equal(s.currentFrame, 0); assert.equal(s.waIn, 0); assert.equal(s.waOut, 11); assert.equal(s.activeLayerIdx, 0);
  assert.deepEqual(s.layers.map((l) => [l.layerUid, l.name]), [['ia', 'Inner A'], ['ib', 'Layer 2']], 'the component layers come from the native read');
  assert.deepEqual(f.host._layerSel, [0]);
  assert.deepEqual(f.tabs(), [{ text: 'Scene', act: false }, { text: 'Comp A', act: true }]);
  assert.deepEqual(f.toasts, []);
});

test('a row that instances nothing does nothing, and the component read finds the right component per instance', async () => {
  const f = fixture();
  f.composed.bindPolicyEntries();
  f.row(0).fire('dblclick'); await settle();
  assert.equal(f.workspaceRequests().length, 0, 'a plain layer is not entered');
  assert.equal(f.host.state.activeSymbolId, null);
  f.row(2).fire('dblclick'); await settle();
  assert.equal(f.host.state.activeSymbolId, 'sym_b');
  assert.equal(f.host.state.totalFrames, 6);
  assert.equal(f.host.state.fps, 24, 'a component without fps keeps the scene fps');
});

test('the Scene tab leaves the component and restores the scene projection from the native truth', async () => {
  const f = fixture();
  f.composed.bindPolicyEntries();
  f.host.state.currentFrame = 5; f.host.state.activeLayerIdx = 2; f.host._layerSel = [2]; f.host._layerSelAnchor = 2;
  f.row(1).fire('dblclick'); await settle();
  f.host.state.currentFrame = 3; // the user scrubbed inside the component
  f.host.document.getElementById('symbol-tabs').children[0].fire('click'); await settle();
  const exit = f.workspaceRequests()[1];
  assert.deepEqual(exit.payload.action, { kind: 'exit' });
  assert.deepEqual(exit.payload.workspace.view, { frame: 3, workIn: 0, workOut: 11 }, 'the live playhead of the component is sent');
  const s = f.host.state;
  assert.equal(s.activeSymbolId, null);
  assert.equal(s.totalFrames, 24); assert.equal(s.fps, 24); assert.equal(s.currentFrame, 5);
  assert.deepEqual(s.layers.map((l) => l.layerUid), ['L1', 'L2', 'L3']);
  assert.equal(s.activeLayerIdx, 2, 'the scene selection is restored');
  assert.deepEqual(f.host._layerSel, [2]);
  assert.deepEqual(f.renders.map((r) => [r.frame, r.presented && r.presented.contextId]), [[0, 'sym_a'], [5, null]], 'the scene is presented again at its own frame');
  assert.deepEqual(f.tabs(), [{ text: 'Scene', act: true }, { text: 'Comp A', act: false }], 'the tab stays open');
  assert.equal(Files.projectionReadyForUI(f.host, f.session.info()), true, 'the ordinary scene UI is ready again');
  assert.deepEqual(f.effects.filter((e) => e.startsWith('legacy')), []);
});

test('a component tab re-enters it, its x closes it (the active tab exits first), and entering from inside another component is refused', async () => {
  const f = fixture();
  f.composed.bindPolicyEntries();
  f.row(1).fire('dblclick'); await settle();
  f.host.SM.enterSymbol('sym_b'); await settle();
  assert.deepEqual(f.toasts, ['toastNestedComponentsUnsupported'], 'the baseline nested notice');
  assert.equal(f.host.state.activeSymbolId, 'sym_a', 'nothing moved');
  assert.equal(f.renders.length, 1);
  const compTab = () => f.host.document.getElementById('symbol-tabs').children[1];
  compTab().children[1].fire('click', { target: compTab().children[1] }); await settle(); // the x
  const close = f.workspaceRequests().pop();
  assert.deepEqual(close.payload.action, { kind: 'close', contextId: 'sym_a' });
  assert.equal(f.host.state.activeSymbolId, null);
  assert.deepEqual(f.tabs(), [{ text: 'Scene', act: true }]);
  f.row(1).fire('dblclick'); await settle();
  f.host.document.getElementById('symbol-tabs').children[0].fire('click'); await settle();
  assert.deepEqual(f.tabs(), [{ text: 'Scene', act: true }, { text: 'Comp A', act: false }]);
  f.host.document.getElementById('symbol-tabs').children[1].fire('click', { target: {} }); await settle(); // the tab itself
  assert.equal(f.host.state.activeSymbolId, 'sym_a');
  assert.deepEqual(f.effects.filter((e) => e.startsWith('legacy')), []);
});

test('the SM entries (Scene tab, programmatic) route to the native transition; a legacy document keeps its guarded entries', async () => {
  const f = fixture();
  const legacy = f.host.SM.enterSymbol;
  f.composed.bindPolicyEntries();
  assert.notEqual(f.host.SM.enterSymbol, legacy);
  await f.host.SM.enterSymbol('sym_a');
  assert.equal(f.host.state.activeSymbolId, 'sym_a');
  await f.host.SM.exitToScene();
  assert.equal(f.host.state.activeSymbolId, null);
  await f.host.SM.closeSymbolTab('sym_a');
  assert.deepEqual(f.effects.filter((e) => e.startsWith('legacy')), []);
  const old = fixture({ legacyDocument: true });
  old.composed.bindPolicyEntries();
  old.host.SM.enterSymbol('sym_a'); old.host.SM.exitToScene(); old.host.SM.closeSymbolTab('sym_a');
  assert.deepEqual(old.effects.filter((e) => e.startsWith('legacy')), ['legacy-enter', 'legacy-exit', 'legacy-close'], 'a legacy document keeps its (guarded) entries');
  assert.equal(old.workspaceRequests().length, 0);
  assert.equal(old.host.state.activeSymbolId, null);
});

test('an unknown component, a missing session and a viewport that cannot present it leave the scene untouched', async () => {
  const f = fixture();
  f.composed.bindPolicyEntries();
  await f.host.SM.enterSymbol('sym_nope');
  assert.equal(f.host.state.activeSymbolId, null);
  assert.deepEqual(f.toasts.length, 0, 'the baseline ignores an unknown symbol silently');
  f.setReady(false);
  await f.host.SM.enterSymbol('sym_a');
  assert.deepEqual(f.toasts, ['Native component context is unavailable.']);
  assert.equal(f.workspaceRequests().length, 1, 'only the unknown-context request was sent');
  f.setReady(true);
  const bad = fixture({ failPresent: true });
  bad.composed.bindPolicyEntries();
  await bad.host.SM.enterSymbol('sym_a');
  assert.match(bad.toasts.join('|'), /the viewport refused the context/);
  assert.equal(bad.host.state.activeSymbolId, null, 'the half-entered view is rolled back');
  assert.equal(bad.host.state.totalFrames, 24);
  assert.equal(Context.presented(bad.host), null);
  assert.deepEqual(bad.workspaceRequests().map((r) => r.payload.action.kind), ['enter', 'exit']);
  assert.deepEqual(bad.renders.map((r) => r.presented && r.presented.contextId), ['sym_a', null], 'the scene is presented again');
});

test('a component that vanished (Undo) is left on the next revision, the others re-read their layers', async () => {
  const f = fixture();
  f.composed.bindPolicyEntries();
  f.row(1).fire('dblclick'); await settle();
  f.m.revision = 8; f.m.comps.sym_a.layers.push(['id', 'Inner D', 'drawing']);
  Context.revision(f.host, f.session.info()); await settle();
  assert.equal(f.host.state.activeSymbolId, 'sym_a');
  assert.deepEqual(f.host.state.layers.map((l) => l.layerUid), ['ia', 'ib', 'id']);
  f.m.revision = 9; delete f.m.comps.sym_a;
  Context.revision(f.host, f.session.info()); await settle();
  assert.equal(f.host.state.activeSymbolId, null);
  assert.deepEqual(f.host.state.layers.map((l) => l.layerUid), ['L1', 'L2', 'L3']);
  assert.match(f.toasts.join('|'), /no longer exists/);
  assert.deepEqual(f.tabs(), [{ text: 'Scene', act: true }], 'the tab of a context that no longer exists is not listed');
});

test('an entered component paints read-only: its layers, no scene paint, nothing to click', async () => {
  const f = fixture();
  f.composed.bindPolicyEntries();
  f.row(1).fire('dblclick'); await settle();
  const list = f.host.document.getElementById('layer-list');
  assert.deepEqual(list.children.map((r) => r.dataset.layerUid), ['ia', 'ib']);
  assert.ok(list.children.every((r) => r.children[0].disabled === true), 'component rows cannot be picked');
  assert.match(f.host.document.getElementById('frame-hdr').children[0].textContent, /^Comp A — Frame 1 \/ 12$/);
  assert.match(f.host.document.getElementById('frame-grid').children[0].textContent, /read-only/);
  assert.equal(f.host.document.getElementById('tl-tf').textContent, 12);
  assert.equal(f.effects.includes('scene-paint'), false, 'the scene paint is skipped inside a component');
  f.host.state.currentFrame = 4; f.host.updateUI();
  assert.match(f.host.document.getElementById('frame-hdr').children[0].textContent, /Frame 5 \/ 12/);
});

test('a revision that arrives while another transition is in flight is queued, not dropped: the context is re-read once it is done', async () => {
  let release; const hold = { kind: 'close', promise: new Promise((resolve) => { release = resolve; }) };
  const f = fixture({ hold }); f.composed.bindPolicyEntries();
  await f.host.SM.enterSymbol('sym_a'); await f.host.SM.exitToScene(); await f.host.SM.enterSymbol('sym_b');
  const closing = f.host.SM.closeSymbolTab('sym_a'); await settle(); // held in flight
  f.m.revision = 8; f.m.comps.sym_b.layers.push(['id', 'Inner D', 'drawing']);
  Context.revision(f.host, f.session.info()); await settle();
  assert.deepEqual(f.host.state.layers.map((l) => l.layerUid), ['ic'], 'nothing could be re-read while the transition is in flight');
  release(); await closing; await settle();
  assert.deepEqual(f.host.state.layers.map((l) => l.layerUid), ['ic', 'id'], 'the queued revision re-read the layers afterwards');
  assert.equal(f.host.state.activeSymbolId, 'sym_b');
});

test('leaving a component that vanished under the user still shows the scene (the owner answers not_in_context)', async () => {
  const f = fixture(); f.composed.bindPolicyEntries();
  await f.host.SM.enterSymbol('sym_a');
  delete f.m.comps.sym_a; f.m.revision = 8; // deleted by Undo / an agent; no revision event reached the view yet
  assert.equal(await f.host.SM.exitToScene(), true);
  assert.equal(f.host.state.activeSymbolId, null);
  assert.deepEqual(f.host.state.layers.map((l) => l.layerUid), ['L1', 'L2', 'L3']);
  assert.equal(Context.presented(f.host), null);
  assert.equal(f.renders[f.renders.length - 1].presented, null, 'the scene is presented');
  assert.deepEqual(f.toasts, []);
});

test('a ninth distinct component keeps the strip within the eight tabs the wire allows', async () => {
  const f = fixture(); f.composed.bindPolicyEntries();
  for (let n = 0; n < 7; n += 1) f.m.comps['sym_x' + n] = { name: 'X' + n, totalFrames: 4, fps: null, layers: [['x' + n, 'X', 'drawing']], instances: [] };
  const order = ['sym_a', 'sym_b'].concat([0, 1, 2, 3, 4, 5, 6].map((n) => 'sym_x' + n));
  for (const id of order) { assert.equal(await f.host.SM.enterSymbol(id), true, id); await f.host.SM.exitToScene(); }
  assert.deepEqual(f.toasts, [], 'no opaque contract error');
  assert.equal(f.host.state.openSymbolTabs.length, 8);
  assert.equal(f.host.state.openSymbolTabs.includes('sym_a'), false, 'the oldest tab was evicted');
  assert.equal(await f.host.SM.enterSymbol('sym_a'), true, 'an evicted component can still be entered');
  assert.equal(f.host.state.activeSymbolId, 'sym_a');
  assert.equal(f.host.state.openSymbolTabs.length, 8);
});

test('the layer-row double-click enters at the frame the instance shows at the playhead (resolveSymbolFrameIdx); tabs and SM.enterSymbol start at the first frame', async () => {
  const f = fixture(); f.composed.bindPolicyEntries();
  f.m.entryFrames = { L2: 7, L3: 99 }; // what the owner resolves from the instance's placement, speed, play mode and component frame keys
  f.host.state.currentFrame = 15;
  f.row(1).fire('dblclick'); await settle();
  const enter = f.workspaceRequests()[0].payload;
  assert.deepEqual(enter.action, { kind: 'enter', contextId: 'sym_a', entryFrame: null, fromLayerUid: 'L2' });
  assert.deepEqual(enter.workspace.view, { frame: 15, workIn: 0, workOut: 23 }, 'the scene playhead is what the instance is resolved at');
  assert.equal(f.host.state.currentFrame, 7, 'the playhead is on the resolved component frame');
  assert.deepEqual([f.host.state.waIn, f.host.state.waOut], [0, 11]);
  assert.deepEqual(f.renders.map((r) => r.frame), [7], 'the component is presented at that frame');
  await f.host.SM.exitToScene();
  assert.equal(f.host.state.currentFrame, 15, 'the scene playhead is restored');
  await f.host.SM.enterSymbol('sym_a');
  assert.equal(f.host.state.currentFrame, 0, 'a tab / programmatic entry has no instance and starts at the first frame');
  assert.equal('fromLayerUid' in f.workspaceRequests().pop().payload.action, false);
  await f.host.SM.exitToScene();
  f.row(2).fire('dblclick'); await settle();
  assert.equal(f.host.state.currentFrame, 5, 'an out-of-range resolution is clamped into the component (6 frames)');
});

// #2256 review of #2660: a refresh that failed must not be acknowledged. The revision is recorded only once the context was re-read; until
// then projectionReady() is false (the rows, tabs and frame count are stale), the refresh is retried a bounded number of times, and when
// the retries run out the scene is shown with a notice instead of a stale entered view.
async function enteredAt8(f) {
  f.composed.bindPolicyEntries();
  f.row(1).fire('dblclick'); await settle();
  f.m.revision = 8; f.m.comps.sym_a.layers.push(['id', 'Inner D', 'drawing']);
  return f;
}
const refreshes = (f) => f.workspaceRequests().filter((r) => r.payload.action.kind === 'refresh').length;

test('a failed refresh does not acknowledge the revision: it is retried, and only the successful retry makes the projection ready', async () => {
  const f = fixture(), timers = [];
  f.host.setTimeout = (fn, ms) => { timers.push({ fn, ms }); };
  await enteredAt8(f); f.m.failRefresh = 1;
  const info = f.session.info();
  Context.revision(f.host, info); await settle();
  assert.equal(refreshes(f), 1, 'the first refresh failed');
  assert.equal(Context.projectionReady(f.host, info), false, 'the stale rows are not a ready projection');
  assert.deepEqual(f.host.state.layers.map((l) => l.layerUid), ['ia', 'ib'], 'nothing was applied from the failed answer');
  assert.equal(timers.length, 1, 'one backoff pause is pending');
  timers.shift().fn(); await settle();
  assert.equal(refreshes(f), 2);
  assert.equal(Context.projectionReady(f.host, info), true, 'acknowledged once the context was re-read');
  assert.deepEqual(f.host.state.layers.map((l) => l.layerUid), ['ia', 'ib', 'id']);
  assert.equal(f.host.state.activeSymbolId, 'sym_a', 'the component stays entered');
});

test('when every retry fails the scene is shown with a notice, after a bounded number of asks', async () => {
  const f = fixture(), pauses = [];
  f.host.setTimeout = (fn, ms) => { pauses.push(ms); fn(); };
  await enteredAt8(f); f.m.failRefresh = Infinity;
  const info = f.session.info();
  Context.revision(f.host, info); await settle();
  assert.equal(refreshes(f), 4, 'the first ask and three retries, never more');
  assert.deepEqual(pauses, [100, 300, 900], 'with a growing pause');
  assert.equal(f.host.state.activeSymbolId, null, 'back on the scene');
  assert.deepEqual(f.host.state.layers.map((l) => l.layerUid), ['L1', 'L2', 'L3']);
  assert.match(f.toasts.join('|'), /could not be refreshed/);
  assert.equal(Context.projectionReady(f.host, info), false, 'no entered context is claimed ready');
});

test('a failed refresh inside a publication rides the same retry and exit (the lease is the publication the queue runs)', async () => {
  const f = fixture();
  f.host.setTimeout = (fn) => { fn(); };
  await enteredAt8(f); f.m.failRefresh = Infinity;
  await f.session.runResourceCommand((lease) => Context.revision(f.host, f.session.info(), lease));
  assert.equal(refreshes(f), 4);
  assert.equal(f.host.state.activeSymbolId, null);
  assert.match(f.toasts.join('|'), /could not be refreshed/);
});
