'use strict';
// #2256 review of the component context (#2657): what happens to an ENTERED component when the project moves on under it. Unlike
// native-component-context.test.cjs (projection stubbed ready) this runs the REAL desktop session, the REAL revision subscription
// (application-mcp), the real desktop ports and the real projection readiness: an Undo or an MCP edit while a component is entered
// must be acknowledged, not fence the session. The native owner is a scripted double answering the component reads, the workspace
// transition and the preview; the preview refuses what the scheduler refuses (an undeclared context, a frame beyond its frame count).
const test = require('node:test'), assert = require('node:assert/strict');
const Project = require('../src/js/adapters/native-object-project.js');
const Underlay = require('../src/js/adapters/native-viewport-underlay.js');
const Files = require('../src/js/adapters/native-object-project-file.js');
const Application = require('../src/js/adapters/native-application.js');
const Context = require('../src/js/adapters/native-component-context.js');
const Component = require('../src/js/adapters/native-object-component.js');

const turn = () => new Promise(setImmediate);
const settle = async () => { for (let i = 0; i < 40; i++) await turn(); };
const clone = (value) => JSON.parse(JSON.stringify(value));
const SCENE_LAYERS = [{ layerUid: 'L1', visible: true }, { layerUid: 'L2', visible: true }];
const MAX_TABS = 8;

function comps() {
  return { sym_a: { name: 'Comp A', totalFrames: 12, fps: 12, layers: [['ia', 'Inner A', 'drawing'], ['ib', null, 'null']], instances: ['L2'] } };
}
function workspaceAnswer(m, request) {
  const ws = clone(request.payload.workspace), action = request.payload.action, table = m.comps;
  const clamp = (v, total) => ({ frame: Math.min(v.frame, total - 1), workIn: Math.min(v.workIn, total - 1), workOut: Math.min(v.workOut, total - 1) });
  const total = (id) => (id === null ? 24 : table[id].totalFrames);
  let gone = false;
  if (ws.activeContextId !== null && !table[ws.activeContextId]) { ws.view = ws.sceneView; ws.sceneView = null; ws.activeContextId = null; gone = true; }
  ws.view = clamp(ws.view, total(ws.activeContextId)); if (ws.sceneView) ws.sceneView = clamp(ws.sceneView, 24);
  const leave = () => { ws.view = ws.sceneView; ws.sceneView = null; ws.activeContextId = null; };
  let outcome = 'refreshed';
  if (action.kind === 'enter') {
    if (!table[action.contextId]) return null;
    ws.sceneView = ws.view; ws.activeContextId = action.contextId; ws.view = { frame: action.entryFrame || 0, workIn: 0, workOut: table[action.contextId].totalFrames - 1 };
    if (!ws.openTabs.includes(action.contextId)) ws.openTabs.push(action.contextId);
    while (ws.openTabs.length > MAX_TABS) ws.openTabs.splice(ws.openTabs.findIndex((id) => id !== ws.activeContextId), 1);
    outcome = 'entered';
  } else if (action.kind === 'exit') { if (ws.activeContextId === null) outcome = 'not_in_context'; else { leave(); outcome = 'exited'; } }
  else if (action.kind === 'close') { if (ws.activeContextId === action.contextId) leave(); const at = ws.openTabs.indexOf(action.contextId); if (at >= 0) ws.openTabs.splice(at, 1); outcome = at >= 0 ? 'closed' : 'close_noop'; }
  else if (gone) outcome = 'context_gone';
  ws.openTabs = ws.openTabs.filter((id) => table[id]);
  const a = ws.activeContextId;
  return { atRevision: request.payload.atRevision, documentSnapshotId: request.payload.documentSnapshotId, outcome, contextId: a === null ? 'scene-root' : a,
    totalFrames: total(a), fps: a === null ? 24 : table[a].fps, workspace: ws };
}
function readAnswer(m, request) {
  const p = request.payload, s = p.selector, head = { atRevision: p.atRevision, documentSnapshotId: p.documentSnapshotId, kind: s.kind };
  const page = (rows, countKey, extra) => { const take = rows.slice(s.cursor, s.cursor + s.limit), end = s.cursor + take.length;
    return Object.assign(head, extra, { [countKey]: rows.length, cursor: s.cursor, items: take, nextCursor: end < rows.length ? end : null }); };
  const all = [{ contextId: 'scene-root', contextKind: 'scene', name: null, totalFrames: 24, layerCount: 2 }]
    .concat(Object.entries(m.comps).map(([id, c]) => ({ contextId: id, contextKind: 'component', name: c.name, totalFrames: c.totalFrames, layerCount: c.layers.length })));
  if (s.kind === 'contexts') return page(all, 'contextCount');
  const comp = m.comps[s.contextId];
  const rows = (list) => list.map(([layerUid, name, layerKind]) => ({ layerUid, name, layerKind }));
  if (s.kind === 'layers') return page(rows(comp ? comp.layers : [['L1', 'Plain', 'drawing'], ['L2', 'Inst A', 'instance']]), 'layerCount', { contextId: s.contextId });
  return page(comp.instances.map((layerUid) => ({ layerUid })), 'instanceCount', { contextId: s.contextId });
}

function fixture() {
  let installed = false, revision = 0;
  const m = { comps: comps() }, calls = [], previews = [], toasts = [], listeners = new Map(), transports = [], log = [];
  const info = () => ({ apiVersion: 2, instanceId: 'instance', documentId: 'document', contentRevision: revision, documentSnapshotId: 'snapshot-' + revision, lifecycleGeneration: 1,
    documentDigest: 'a'.repeat(64), byteCount: 100, canvasW: 320, canvasH: 180, fps: 24, totalFrames: 24, layers: SCENE_LAYERS.map((row) => ({ ...row })), reference: null, undoDepth: 1, redoDepth: 0 });
  const document = { addEventListener() {}, removeEventListener() {}, getElementById: (id) => (id === 'drawing-canvas' ? { getBoundingClientRect: () => ({ left: 0, top: 0, width: 320, height: 180 }) } : null),
    documentElement: { clientWidth: 1200, clientHeight: 900, classList: { contains: () => false, add() {}, remove() {} }, style: { getPropertyValue: () => '', getPropertyPriority: () => '', setProperty() {}, removeProperty() {} } } };
  const root = { addEventListener() {}, removeEventListener() {}, document, stageLayer: { visible: true }, paper: { view: { update() {} } }, devicePixelRatio: 1,
    state: { currentFrame: 0, activeLayerIdx: 0 }, _layerSel: [0], selectedPaths: [],
    NemoNativeViewportUnderlay: Underlay, NemoNativeObjectProject: Project, NemoNativeObjectProjectFile: Files, NemoNativeApplicationAdapter: Application,
    NemoNativeComponentContext: Context, NemoNativeObjectComponent: Component,
    SMReference: { refreshNative() {} }, SMProject: { refreshActiveTabDirtyDot() {} },
    updateUI: () => log.push('update'), renderSymbolTabs: () => {}, showToast: (text) => toasts.push(text) };
  async function invoke(command, args) {
    const r = args && (args.request || JSON.parse(args.requestJson));
    calls.push({ command, r });
    if (command === 'nemo_native_object_client_status' || command === 'nemo_native_status') return { ...info(), available: installed };
    if (command === 'nemo_native_object_bootstrap') { installed = true; revision = 0; return { ...info(), requestId: r.requestId, resourceCount: 0, viewportAvailable: false }; }
    if (command === 'nemo_native_object_reference') { assert.equal(r.action.kind, 'inspect'); return { ...info(), requestId: r.requestId }; }
    if (command === 'nemo_native_object_preview') {
      previews.push({ contextId: r.contextId, frame: r.frame, revision: r.contentRevision });
      // The scheduler's own refusals (scheduler_object.rs): an undeclared context, a frame beyond the context's frame count.
      const frames = r.contextId === 'scene-root' ? 24 : (m.comps[r.contextId] || {}).totalFrames;
      if (!frames) throw new Error('object scheduling requires a declared context');
      if (r.frame >= frames) throw new Error('object frame is outside the retained document range');
      return { status: 'presented', workId: 'work-' + calls.length, viewGeneration: 1 };
    }
    if (command === 'nemo_native_revision_sync') {
      if (r.action === 'acknowledge') { log.push('native-ack:' + (r.requestId || r.eventRequestId || JSON.stringify(r).slice(0, 80))); return null; }
      return r.action === 'binding' ? { ...info(), subscriptionId: 'subscription-1' } : r.action === 'subscribe' ? r.binding : null;
    }
    if (command === 'nemo_native_object_dispatch') { // Undo / Redo of the document history
      assert.match(r.operation, /\.undo$/); m.onUndo(m); revision += 1;
      return { apiVersion: 2, requestId: r.requestId, instanceId: 'instance', documentId: 'document', contentRevision: revision, ok: true, result: { applied: true } };
    }
    if (command === 'nemo_native_dispatch') {
      const pin = info();
      const result = r.operation === 'query.document.object.workspace.context' ? workspaceAnswer(m, r) : r.operation === 'query.document.object.component.read' ? readAnswer(m, r) : undefined;
      if (result === undefined) throw new Error('unexpected fake operation ' + r.operation);
      return result === null ? { apiVersion: 2, requestId: r.requestId, instanceId: 'instance', documentId: 'document', contentRevision: pin.contentRevision, ok: false, error: { code: 'not_found', message: 'no such context' } }
        : { apiVersion: 2, requestId: r.requestId, instanceId: 'instance', documentId: 'document', contentRevision: pin.contentRevision, ok: true, result };
    }
    throw new Error('Unexpected native fake command ' + command);
  }
  root.__TAURI__ = { core: { invoke }, event: { listen: async (name, callback) => { listeners.set(name, callback); return () => listeners.delete(name); } } };
  root.NemoApplicationMcpTransport = { createNativeTauriTransport: (call) => { const value = require('../src/js/adapters/application-mcp.js').createNativeTauriTransport(call); transports.push(value); return value; } };
  const session = Project.desktop(root);
  return { root, session, m, calls, previews, toasts, log, info,
    async open() { await session.openPath('/fake.json', { reveal: async (i) => { Files.projectView(root, i); root.updateUI(); } }); await turn(); },
    // A revision published from elsewhere (an MCP edit, another window): the REAL subscription listener of the desktop ports.
    async emit() { const from = revision; revision += 1; return listeners.get('nemo-native-revision')({ payload: { instanceId: 'instance', documentId: 'document', lifecycleGeneration: 1, requestId: 'event-' + revision, fromRevision: from, toRevision: revision } }); },
    revision: () => revision };
}
const lastPreview = (f) => f.previews[f.previews.length - 1];

test('an MCP edit while a component is entered is acknowledged by the entered context, not fenced; the view re-reads its layers', async () => {
  const f = fixture(); await f.open();
  assert.equal(await Context.enter(f.root, 'sym_a', null), true);
  assert.equal(f.root.state.activeSymbolId, 'sym_a');
  assert.equal(Files.projectionReadyForUI(f.root, f.session.info()), true, 'the real readiness is judged against the entered context');
  f.m.comps.sym_a.layers.push(['ic', 'Inner C', 'drawing']);
  await f.emit(); await settle();
  assert.equal(f.session.isIndeterminate(), false, 'the revision did not fence the session');
  assert.equal(f.session.isActive(), true);
  assert.ok(f.log.some((x) => x.startsWith('native-ack')), 'the revision was acknowledged to the native owner');
  assert.equal(f.root.state.activeSymbolId, 'sym_a');
  assert.deepEqual(f.root.state.layers.map((l) => l.layerUid), ['ia', 'ib', 'ic'], 'the entered context re-read its layers');
  assert.deepEqual([lastPreview(f).contextId, lastPreview(f).revision], ['sym_a', 1], 'the component is presented at the new revision');
  assert.deepEqual(f.toasts, []);
});

test('Undo (the native history) while a component is entered does not fence either, and the entered view follows the undone document', async () => {
  const f = fixture(); await f.open();
  await Context.enter(f.root, 'sym_a', null);
  f.m.onUndo = (m) => { m.comps.sym_a.layers.pop(); };
  await f.session.historyFromUi('undo'); await settle();
  assert.equal(f.session.isIndeterminate(), false);
  assert.equal(f.root.state.activeSymbolId, 'sym_a');
  assert.deepEqual(f.root.state.layers.map((l) => l.layerUid), ['ia'], 'the component rows follow Undo');
  assert.equal(lastPreview(f).contextId, 'sym_a');
});

test('a component that SHRANK under the playhead is clamped, presented at a frame it still has, and can still be left', async () => {
  const f = fixture(); await f.open();
  await Context.enter(f.root, 'sym_a', null);
  f.root.state.currentFrame = 9; f.root.state.waOut = 11;
  f.m.comps.sym_a.totalFrames = 4;
  await f.emit(); await settle();
  assert.equal(f.session.isIndeterminate(), false, 'no preview of a frame beyond the new frame count was asked for');
  assert.equal(f.root.state.totalFrames, 4); assert.equal(f.root.state.currentFrame, 3); assert.equal(f.root.state.waOut, 3);
  assert.deepEqual([lastPreview(f).contextId, lastPreview(f).frame], ['sym_a', 3]);
  assert.equal(await Context.exit(f.root), true, 'exit works after the shrink');
  assert.equal(f.root.state.activeSymbolId, null);
  assert.equal(lastPreview(f).contextId, 'scene-root');
});

test('a component DELETED while it is entered leaves the context during the revision: the scene is presented, nothing fences', async () => {
  const f = fixture(); await f.open();
  await Context.enter(f.root, 'sym_a', null);
  delete f.m.comps.sym_a;
  await f.emit(); await settle();
  assert.equal(f.session.isIndeterminate(), false);
  assert.equal(f.root.state.activeSymbolId, null);
  assert.deepEqual(f.root.state.layers.map((l) => l.layerUid), ['L1', 'L2'], 'the scene rows are back');
  assert.equal(lastPreview(f).contextId, 'scene-root', 'the scene is what the revision presented');
  assert.match(f.toasts.join('|'), /no longer exists/);
  assert.equal(Files.projectionReadyForUI(f.root, f.session.info()), true);
  assert.equal(Context.presented(f.root), null);
});

test('nothing is edited while a component is presented: every write path refuses through the one scene-root guard', async () => {
  const f = fixture(); await f.open();
  const writers = (s) => ({ createLayer: s.createLayer, editProjectSettings: s.editProjectSettings, importBatch: s.importBatch, editLayerPolicy: s.editLayerPolicy,
    editFrames: s.editFrames, toggleLayerVisibility: s.toggleLayerVisibility, editProperty: s.editProperty, editPropertyTrack: s.editPropertyTrack });
  const before = writers(f.session);
  const reachable = Object.keys(before).filter((k) => typeof before[k] === 'function');
  assert.ok(reachable.length >= 1, 'the scene offers editors (' + reachable.join(',') + ')');
  await Context.enter(f.root, 'sym_a', null);
  Object.entries(writers(f.session)).forEach(([name, editor]) => assert.equal(editor, undefined, name + ' is not offered inside a component'));
  const writes = () => f.calls.filter((c) => c.command === 'nemo_native_object_reference' && c.r.action.kind !== 'inspect' || c.command === 'nemo_native_object_dispatch' ||
    c.command === 'nemo_native_dispatch' && /^command\./.test(c.r.operation)).length;
  const base = writes();
  // An editor captured on the scene (a stale UI binding) refuses through the same guard when it is called inside a component.
  const args = { createLayer: ['Layer', f.session.info()], editProjectSettings: [{ kind: 'noop' }, f.session.info()], importBatch: [{ command: 'object.import.batch' }, f.session.info()],
    editLayerPolicy: [{ command: 'object.layer.policy.edit', layerUid: 'L1', flag: 'locked', value: true }, f.session.info()],
    editFrames: ['command.document.object.frames.edit', {}, f.session.info()], toggleLayerVisibility: ['L1'] };
  for (const name of reachable.filter((k) => args[k])) await assert.rejects(Promise.resolve().then(() => before[name].apply(null, args[name])), /inside a component/, name);
  await assert.rejects(f.session.setLayerVisibility('L1', false), /inside a component/);
  await assert.rejects(f.session.reference({ kind: 'remove' }), /inside a component/);
  await assert.rejects(f.session.reference(() => ({ kind: 'attach', id: 'x', sourcePath: '/p.png' })), /inside a component/);
  assert.equal(writes(), base, 'no write command reached the native owner');
  await Context.exit(f.root);
  assert.deepEqual(Object.keys(writers(f.session)).filter((k) => typeof writers(f.session)[k] === 'function'), reachable, 'the same editors are offered again on the scene');
});

test('Undo and Redo stay available inside a component, and the document-wide history is the only write', async () => {
  const f = fixture(); await f.open();
  await Context.enter(f.root, 'sym_a', null);
  f.m.onUndo = () => {};
  await f.session.historyFromUi('undo'); await settle();
  assert.equal(f.calls.filter((c) => c.command === 'nemo_native_object_dispatch').length, 1);
  assert.equal(f.session.isIndeterminate(), false);
});

test('export is unavailable while a component is entered (its range and fps are not the scene\'s) and works again on the scene', async () => {
  const f = fixture(); await f.open();
  await Context.enter(f.root, 'sym_a', null);
  const exports = () => f.calls.filter((c) => c.command === 'nemo_native_object_export').length;
  await assert.rejects(f.session.exportPng('/dest', [0, 1, 2]), /not available inside a component/);
  await assert.rejects(f.session.exportSvg('/dest', [0]), /not available inside a component/);
  assert.equal(exports(), 0, 'nothing was exported with the component\'s range against the scene');
  await Context.exit(f.root);
  await assert.rejects(f.session.exportPng('/dest', [0]), (error) => !/inside a component/.test(error.message), 'on the scene the ordinary export path decides');
});
