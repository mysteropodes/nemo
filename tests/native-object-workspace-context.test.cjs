'use strict';
// #2256 leaf 3: the pure enter/exit component-context transition (query.document.object.workspace.context). The raw client contract is
// judged by the REAL common v2 SDK (native-application.js with the composed contract); the native owner is a scripted double that
// states the BASELINE rules (enterSymbol / exitToScene / closeSymbolTab of app.js at 1065e9d8) independently of the Rust kernel, whose
// own tests run on the real admitted worlds. The adapter holds the session workspace and nothing else.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const contract = require('../src/js/adapters/native-object-workspace-context-contract.js');
const component = require('../src/js/adapters/native-object-component.js');
const SDK = require('../src/js/adapters/native-application.js');

const ROOT = path.resolve(__dirname, '..');
const OP = 'query.document.object.workspace.context';
const SNAP = (revision) => 'native-object:document-A:' + revision;
const clone = (value) => JSON.parse(JSON.stringify(value));
const COMPS = { sym_a: { totalFrames: 12, fps: 12 }, sym_b: { totalFrames: 6, fps: null } };
const SCENE = { totalFrames: 24, fps: 24 };

// The baseline rules, stated again: the owner of the contexts the snapshot declares.
function apply(workspace, action) {
  const ws = clone(workspace);
  const shown = (outcome, state) => {
    const active = state.activeContextId, meta = active === null ? SCENE : COMPS[active];
    return { outcome, contextId: active === null ? 'scene-root' : active, totalFrames: meta.totalFrames, fps: meta.fps || SCENE.fps,
      workspace: Object.assign(state, { openTabs: state.openTabs.filter((id) => COMPS[id]) }) };
  };
  if (ws.activeContextId !== null && !COMPS[ws.activeContextId]) { // the context vanished: exitToScene first
    ws.view = ws.sceneView; ws.sceneView = null; ws.activeContextId = null;
    if (action.kind === 'refresh') return shown('context_gone', ws);
  }
  if (action.kind === 'enter') {
    if (ws.activeContextId === action.contextId) return shown('already_active', ws);
    if (ws.activeContextId !== null) return shown('refused_nested', ws);
    const meta = COMPS[action.contextId];
    if (!meta) return { error: { code: 'not_found', message: 'no such context' } };
    const last = meta.totalFrames - 1, frame = action.entryFrame === null ? 0 : Math.max(0, Math.min(last, action.entryFrame));
    ws.sceneView = ws.view; ws.activeContextId = action.contextId; ws.view = { frame, workIn: 0, workOut: last };
    if (!ws.openTabs.includes(action.contextId)) ws.openTabs.push(action.contextId);
    return shown('entered', ws);
  }
  const leave = () => { ws.view = ws.sceneView; ws.sceneView = null; ws.activeContextId = null; };
  if (action.kind === 'exit') {
    if (ws.activeContextId === null) return shown('not_in_context', ws);
    leave(); return shown('exited', ws);
  }
  if (action.kind === 'close') {
    if (ws.activeContextId === action.contextId) leave();
    const at = ws.openTabs.indexOf(action.contextId);
    if (at >= 0) ws.openTabs.splice(at, 1);
    return shown(at >= 0 ? 'closed' : 'close_noop', ws);
  }
  return shown('refreshed', ws);
}
function owner(options = {}) {
  const state = { revision: options.revision === undefined ? 7 : options.revision, sent: [], counter: 0, pins: 0, documentId: 'document-A' };
  const transport = { dispatch(request) {
    state.sent.push(clone(request));
    const base = { apiVersion: 2, requestId: request.requestId, instanceId: 'instance-A', documentId: 'document-A', contentRevision: state.revision };
    if (options.reply) return Promise.resolve(options.reply(request, base, state));
    const plan = apply(request.payload.workspace, request.payload.action);
    if (plan.error) return Promise.resolve(Object.assign(base, { ok: false, error: plan.error }));
    return Promise.resolve(Object.assign(base, { ok: true, result: Object.assign({ atRevision: request.payload.atRevision, documentSnapshotId: request.payload.documentSnapshotId }, {
      outcome: plan.outcome, contextId: plan.contextId, totalFrames: plan.totalFrames, fps: plan.fps, workspace: plan.workspace }) }));
  } };
  const sdk = SDK.createNativeApplicationAdapter('workspace-context-test', transport);
  const ports = {
    pin() {
      state.pins += 1;
      if (options.noSession) return null;
      if (options.moveAfter && state.pins > options.moveAfter) state.revision += 1;
      return { instanceId: 'instance-A', documentId: state.documentId, contentRevision: state.revision, documentSnapshotId: SNAP(state.revision), layers: [] };
    },
    declared: () => true,
    readDeclared: () => options.declared !== false,
    requestId(label) { state.counter += 1; return label + '-' + state.counter; },
    async submit() { throw new Error('a view-state transition never submits a command'); },
    async read(request) {
      if (options.busy) throw Object.assign(new Error('busy'), { notSent: true, reason: 'native_project_busy' });
      return sdk.dispatch(request);
    },
  };
  return { ports, state, sdk, intent: component.createComponentIntent(ports, {}) };
}
const scene = (frame, tabs = []) => ({ activeContextId: null, openTabs: tabs, view: { frame, workIn: 0, workOut: 23 }, sceneView: null });
const view = (frame) => ({ frame, workIn: 0, workOut: 23 });
const payload = (workspace, action) => ({ atRevision: 7, documentSnapshotId: SNAP(7), workspace, action });
const enter = (contextId, entryFrame = null) => ({ kind: 'enter', contextId, entryFrame });

test('the contract admits exactly the one op, pinned and without expectedRevision', () => {
  assert.equal(contract.OP, OP);
  assert.equal(contract.admitted(OP), true);
  assert.equal(contract.requiresRevision(OP), false);
  for (const op of [OP + '.x', 'query.document.object.component.read', 'command.document.object.component.convert', '', null, undefined]) assert.equal(contract.admitted(op), false, String(op));
});

test('every action is accepted and malformed workspaces and actions are refused', () => {
  for (const action of [enter('sym_a'), enter('sym_a', 4294967295), { kind: 'exit' }, { kind: 'close', contextId: 'sym_a' }, { kind: 'refresh' }]) contract.payload(OP, payload(scene(3), action));
  const entered = { activeContextId: 'sym_a', openTabs: ['sym_a'], view: { frame: 0, workIn: 0, workOut: 11 }, sceneView: view(3) };
  contract.payload(OP, payload(entered, { kind: 'exit' }));
  const no = (workspace, action, label) => assert.throws(() => contract.payload(OP, payload(workspace, action)), TypeError, label);
  const mut = (patch) => Object.assign(clone(scene(3)), patch);
  no(scene(3), { kind: 'enter', contextId: 'sym_a' }, 'entryFrame required');
  no(scene(3), { kind: 'enter', contextId: '', entryFrame: null }, 'empty id');
  no(scene(3), { kind: 'enter', contextId: 'sym_a', entryFrame: -1 }, 'negative entry');
  no(scene(3), { kind: 'exit', contextId: 'sym_a' }, 'exit carries nothing');
  no(scene(3), { kind: 'teleport' }, 'unknown action');
  no(mut({ openTabs: Array.from({ length: 9 }, (_, i) => 't' + i) }), enter('sym_a'), 'nine tabs');
  no(mut({ openTabs: ['a', 'a'] }), enter('sym_a'), 'duplicate tabs');
  no(mut({ openTabs: [''] }), enter('sym_a'), 'empty tab');
  no(mut({ sceneView: view(1) }), enter('sym_a'), 'the scene saves no scene view');
  no(Object.assign(clone(entered), { sceneView: null }), { kind: 'exit' }, 'an entered context saves the scene view');
  no(Object.assign(clone(entered), { openTabs: [] }), { kind: 'exit' }, 'the entered context is an open tab');
  no(mut({ view: { frame: 1, workIn: 9, workOut: 3 } }), enter('sym_a'), 'reversed work area');
  no(mut({ extra: 1 }), enter('sym_a'), 'extra member');
  for (const patch of [{ atRevision: -1 }, { atRevision: 1.5 }, { documentSnapshotId: '' }, { extra: 1 }]) assert.throws(() => contract.payload(OP, Object.assign(payload(scene(3), enter('sym_a')), patch)), TypeError, JSON.stringify(patch));
});

test('results answer their own pin and present the entered context, else the scene', () => {
  const ok = (request, result) => contract.result(OP, result, request);
  const request = payload(scene(3), enter('sym_a'));
  const good = apply(scene(3), enter('sym_a'));
  const result = Object.assign({ atRevision: 7, documentSnapshotId: SNAP(7) }, clone(good));
  ok(request, result);
  const no = (mutate, label) => { const bad = clone(result); mutate(bad); assert.throws(() => ok(request, bad), TypeError, label); };
  no((r) => { r.atRevision = 8; }, 'foreign revision');
  no((r) => { r.documentSnapshotId = 'foreign'; }, 'foreign snapshot');
  no((r) => { r.outcome = 'teleported'; }, 'unknown outcome');
  no((r) => { r.contextId = 'scene-root'; }, 'presents the entered context');
  no((r) => { r.totalFrames = 0; }, 'at least one frame');
  no((r) => { r.fps = 0; }, 'positive fps');
  no((r) => { r.fps = '24'; }, 'numeric fps');
  no((r) => { r.extra = true; }, 'closed result');
  no((r) => { r.workspace.sceneView = null; }, 'saved scene view');
  no((r) => { r.workspace.openTabs = []; }, 'tab of the entered context');
});

test('the common v2 SDK admits the op without expectedRevision and judges request and response', async () => {
  const o = owner();
  const envelope = (extra) => Object.assign({ apiVersion: 2, requestId: 'r-1', instanceId: 'instance-A', documentId: 'document-A', operation: OP, payload: payload(scene(3), enter('sym_a')) }, extra);
  const good = await o.sdk.dispatch(envelope());
  assert.equal(good.ok, true);
  assert.equal(good.result.outcome, 'entered');
  assert.throws(() => o.sdk.dispatch(envelope({ expectedRevision: 7 })), TypeError, 'a pure view-state op carries no expectedRevision');
  for (const [name, reply] of [
    ['foreign owner', (r, b) => Object.assign(b, { instanceId: 'foreign', ok: true, result: {} })],
    ['forged presented context', (r, b) => Object.assign(b, { ok: true, result: Object.assign({ atRevision: 7, documentSnapshotId: SNAP(7) }, apply(r.payload.workspace, r.payload.action), { contextId: 'scene-root' }) })],
    ['foreign pin', (r, b) => Object.assign(b, { ok: true, result: Object.assign({ atRevision: 8, documentSnapshotId: SNAP(8) }, apply(r.payload.workspace, r.payload.action)) })],
  ]) {
    const forged = owner({ reply });
    await assert.rejects(() => forged.sdk.dispatch(envelope()), name);
  }
});

test('workspaceTransition sends the held workspace literally, keeps the answer and nothing else', async () => {
  const o = owner();
  assert.equal(o.intent.workspaceNow(), null, 'nothing is held before the first transition');
  const entered = await o.intent.workspaceTransition(enter('sym_a'), view(5));
  assert.equal(entered.ok, true);
  assert.equal(entered.outcome, 'entered');
  assert.equal(entered.contextId, 'sym_a');
  assert.equal(entered.totalFrames, 12);
  assert.equal(entered.fps, 12);
  assert.deepEqual(o.state.sent[0], { apiVersion: 2, requestId: 'workspace-context-1', instanceId: 'instance-A', documentId: 'document-A', operation: OP,
    payload: { atRevision: 7, documentSnapshotId: SNAP(7), workspace: scene(5), action: enter('sym_a') } });
  assert.equal('expectedRevision' in o.state.sent[0], false);
  assert.deepEqual(o.intent.workspaceNow(), { activeContextId: 'sym_a', openTabs: ['sym_a'], view: { frame: 0, workIn: 0, workOut: 11 }, sceneView: view(5) });
  assert.equal(Object.isFrozen(entered), true);
  // Inside, the playhead moved: the held workspace is sent with the LIVE view of the presented context.
  const exited = await o.intent.workspaceTransition({ kind: 'exit' }, { frame: 9, workIn: 0, workOut: 11 });
  assert.deepEqual(o.state.sent[1].payload.workspace, { activeContextId: 'sym_a', openTabs: ['sym_a'], view: { frame: 9, workIn: 0, workOut: 11 }, sceneView: view(5) });
  assert.equal(exited.outcome, 'exited');
  assert.equal(exited.contextId, 'scene-root');
  assert.deepEqual(exited.workspace.view, view(5), 'the scene playhead and work area are restored exactly');
  assert.deepEqual(o.intent.workspaceNow().openTabs, ['sym_a'], 'the tab stays open after exit');
  assert.equal(o.state.sent.length, 2);
});

test('nested entry is refused, the active entry is a no-op, an unknown context is not_found and none of them moves the held state', async () => {
  const o = owner();
  await o.intent.workspaceTransition(enter('sym_a'), view(2));
  const held = o.intent.workspaceNow();
  const live = { frame: 4, workIn: 0, workOut: 11 };
  assert.equal((await o.intent.workspaceTransition(enter('sym_a'), live)).outcome, 'already_active');
  assert.equal((await o.intent.workspaceTransition(enter('sym_b'), live)).outcome, 'refused_nested');
  assert.deepEqual(o.intent.workspaceNow().activeContextId, 'sym_a');
  const gone = await o.intent.workspaceTransition({ kind: 'exit' }, live);
  assert.equal(gone.ok, true);
  const unknown = await o.intent.workspaceTransition(enter('sym_nope'), view(2));
  assert.equal(unknown.ok, false);
  assert.equal(unknown.code, 'not_found');
  assert.equal(o.intent.workspaceNow().activeContextId, null, 'a refusal keeps the held state');
  assert.deepEqual(held.sceneView, view(2));
});

test('close drops a tab (the active one exits first) and refresh leaves a vanished context', async () => {
  const o = owner();
  await o.intent.workspaceTransition(enter('sym_a'), view(1));
  await o.intent.workspaceTransition({ kind: 'exit' }, { frame: 3, workIn: 0, workOut: 11 });
  await o.intent.workspaceTransition(enter('sym_b'), view(1));
  const closed = await o.intent.workspaceTransition({ kind: 'close', contextId: 'sym_b' }, { frame: 2, workIn: 0, workOut: 5 });
  assert.equal(closed.outcome, 'closed');
  assert.equal(closed.contextId, 'scene-root');
  assert.deepEqual(closed.workspace.openTabs, ['sym_a']);
  assert.equal((await o.intent.workspaceTransition({ kind: 'close', contextId: 'sym_b' }, view(1))).outcome, 'close_noop');
  const doc = owner();
  await doc.intent.workspaceTransition(enter('sym_a'), view(1));
  const saved = COMPS.sym_a; delete COMPS.sym_a;
  try {
    const refreshed = await doc.intent.workspaceTransition({ kind: 'refresh' }, { frame: 2, workIn: 0, workOut: 11 });
    assert.equal(refreshed.outcome, 'context_gone');
    assert.equal(refreshed.contextId, 'scene-root');
    assert.deepEqual(refreshed.workspace.openTabs, [], 'only tabs whose context still exists');
  } finally { COMPS.sym_a = saved; }
});

test('another project starts from the scene, a stale answer is discarded and trouble is typed', async () => {
  const o = owner();
  await o.intent.workspaceTransition(enter('sym_a'), view(1));
  o.state.documentId = 'document-B';
  assert.equal(o.intent.workspaceNow(), null, 'the held workspace belongs to its project');
  const stale = owner({ moveAfter: 1 });
  const lost = await stale.intent.workspaceTransition(enter('sym_a'), view(1));
  assert.equal(lost.ok, false);
  assert.equal(lost.code, 'stale_revision');
  assert.equal(stale.intent.workspaceNow(), null, 'a discarded answer holds nothing');
  const busy = await owner({ busy: true }).intent.workspaceTransition(enter('sym_a'), view(1));
  assert.deepEqual([busy.ok, busy.code, busy.reason], [false, 'unavailable', 'native_project_busy']);
  const none = await owner({ noSession: true }).intent.workspaceTransition(enter('sym_a'), view(1));
  assert.deepEqual([none.ok, none.code, none.reason], [false, 'unavailable', 'no_presented_native_session']);
  const undeclared = await owner({ declared: false }).intent.workspaceTransition(enter('sym_a'), view(1));
  assert.deepEqual([undeclared.ok, undeclared.reason], [false, 'native_declaration_absent']);
  const bad = await owner().intent.workspaceTransition(enter(''), view(1));
  assert.deepEqual([bad.ok, bad.code], [false, 'invalid_request']);
  const forged = owner({ reply: (r, b) => Object.assign(b, { ok: true, result: { nonsense: true } }) });
  assert.equal((await forged.intent.workspaceTransition(enter('sym_a'), view(1))).code, 'transport_failed');
  assert.equal(forged.intent.workspaceNow(), null);
});

test('the declaration, schema, composition and load order agree', () => {
  const descriptor = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/capabilities-v2/native-object-workspace-context.json'), 'utf8'));
  assert.deepEqual(descriptor.operations, [OP]);
  const example = descriptor.examples[0];
  contract.payload(OP, example.request.payload);
  contract.correlate(example.request, example.response);
  const schema = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/native-transport-v2.schema.json'), 'utf8'));
  const branches = schema.$defs.Request.oneOf.filter((b) => b.allOf && b.allOf[1].properties.operation && b.allOf[1].properties.operation.const === OP);
  assert.equal(branches.length, 1);
  assert.deepEqual(branches[0].allOf[0], { $ref: '#/$defs/ForbiddenRevisionRequest' });
  assert.ok(schema.$defs.ResponseBase.properties.result.oneOf.some((b) => b.$ref === '#/$defs/ObjectWorkspaceContextResult'));
  const html = fs.readFileSync(path.join(ROOT, 'src/index.html'), 'utf8');
  const at = (name) => html.indexOf('js/adapters/' + name);
  assert.ok(at('native-object-workspace-context-contract.js') > 0 && at('native-object-workspace-context-contract.js') < at('native-application.js'), 'the contract loads before the SDK');
  assert.ok(at('native-component-context.js') > 0 && at('native-component-context.js') < at('native-component-enter-ui.js'), 'the controller loads before its entries');
});
