'use strict';
// #2256: the read-only native component model (query.document.object.component.read). The raw client contract is judged by the REAL
// common v2 SDK (native-application.js with the composed read contract); the native owner is a scripted double that answers from a
// small component model. Nothing here opens a context or writes: the adapter method only reads.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const contract = require('../src/js/adapters/native-object-component-read-contract.js');
const component = require('../src/js/adapters/native-object-component.js');
const core = require('../src/js/adapters/native-object-component-intent.js');
const SDK = require('../src/js/adapters/native-application.js');

const ROOT = path.resolve(__dirname, '..');
const OP = 'query.document.object.component.read';
const SNAP = (revision) => 'native-object:document-A:' + revision;
const clone = (value) => JSON.parse(JSON.stringify(value));

// The model of the double: the scene, and two components; sym_a is instanced by two scene layers.
const MODEL = {
  scene: { name: null, totalFrames: 24, layers: [['L1', 'Plain', 'drawing'], ['L2', 'Inst A', 'instance'], ['L3', 'Inst A again', 'instance']] },
  comps: [
    { id: 'sym_a', name: 'Comp A', fps: 24, totalFrames: 12, layers: [['ia', 'Inner A', 'drawing'], ['ib', null, 'widget']], instances: ['L2', 'L3'], exposed: 1 },
    { id: 'sym_b', name: null, fps: null, totalFrames: 6, layers: [['ic', 'Inner C', 'drawing']], instances: [], exposed: 0 },
  ],
};
function answer(request, state) {
  const p = request.payload, s = p.selector, head = { atRevision: p.atRevision, documentSnapshotId: p.documentSnapshotId, kind: s.kind };
  const per = state.pageSize || s.limit;
  const page = (rows, countKey, extra) => {
    const take = rows.slice(s.cursor, s.cursor + Math.min(per, s.limit)), end = s.cursor + take.length;
    return Object.assign(head, extra, { [countKey]: rows.length, cursor: s.cursor, items: take, nextCursor: end < rows.length ? end : null });
  };
  const all = [{ contextId: 'scene-root', contextKind: 'scene', name: null, totalFrames: 24, layerCount: 3 }]
    .concat(MODEL.comps.map((c) => ({ contextId: c.id, contextKind: 'component', name: c.name, totalFrames: c.totalFrames, layerCount: c.layers.length })));
  const comp = MODEL.comps.find((c) => c.id === s.contextId);
  const rows = (list) => list.map(([layerUid, name, layerKind]) => ({ layerUid, name, layerKind }));
  if (s.kind === 'contexts') return { result: page(all, 'contextCount') };
  if (s.contextId !== 'scene-root' && !comp) return { error: { code: 'not_found', message: 'no such context' } };
  if (s.kind === 'context') {
    return { result: Object.assign(head, { contextId: s.contextId, contextKind: comp ? 'component' : 'scene', name: comp ? comp.name : null, totalFrames: comp ? comp.totalFrames : 24,
      layerCount: comp ? comp.layers.length : 3, fps: comp ? comp.fps : null, instanceCount: comp ? comp.instances.length : 2, exposedPropertyCount: comp ? comp.exposed : 0, hasCameraKeys: false }) };
  }
  if (s.kind === 'layers') return { result: page(rows(comp ? comp.layers : MODEL.scene.layers), 'layerCount', { contextId: s.contextId }) };
  if (!comp) return { error: { code: 'invalid_request', message: 'the scene is not instanced' } };
  return { result: page(comp.instances.map((layerUid) => ({ layerUid })), 'instanceCount', { contextId: s.contextId }) };
}
function owner(options = {}) {
  const state = { revision: options.revision === undefined ? 7 : options.revision, sent: [], pageSize: options.pageSize, counter: 0, pins: 0 };
  const transport = { dispatch(request) {
    state.sent.push(clone(request));
    const base = { apiVersion: 2, requestId: request.requestId, instanceId: 'instance-A', documentId: 'document-A', contentRevision: state.revision };
    if (options.reply) return Promise.resolve(options.reply(request, base, state));
    const plan = answer(request, state);
    return Promise.resolve(plan.error ? Object.assign(base, { ok: false, error: plan.error }) : Object.assign(base, { ok: true, result: plan.result }));
  } };
  const sdk = SDK.createNativeApplicationAdapter('component-read-test', transport);
  const ports = {
    pin() {
      state.pins += 1;
      if (options.noSession) return null;
      if (options.moveAfter && state.pins > options.moveAfter) state.revision += 1;
      return { instanceId: 'instance-A', documentId: 'document-A', contentRevision: state.revision, documentSnapshotId: SNAP(state.revision), layers: [] };
    },
    declared: () => true,
    readDeclared: () => options.declared !== false,
    requestId(label) { state.counter += 1; return label + '-' + state.counter; },
    async submit() { throw new Error('a read never submits a command'); },
    async read(request) {
      if (options.busy) throw Object.assign(new Error('busy'), { notSent: true, reason: 'native_project_busy' });
      if (options.drop) throw new Error(options.drop);
      const out = await sdk.dispatch(request);
      state.reads = (state.reads || 0) + 1;
      if (options.moveAtRead && options.moveAtRead(state.reads)) state.revision += 1; // the project moves right after this answer
      return out;
    },
  };
  return { ports, state, sdk, intent: component.createComponentIntent(ports, {}) };
}
const request = (selector, extra) => Object.assign({ apiVersion: 2, requestId: 'r-1', instanceId: 'instance-A', documentId: 'document-A', operation: OP,
  payload: { atRevision: 7, documentSnapshotId: SNAP(7), selector } }, extra);

test('the contract admits exactly the one read, pinned and without expectedRevision', () => {
  assert.equal(contract.OP, OP);
  assert.equal(contract.admitted(OP), true);
  assert.equal(contract.requiresRevision(OP), false);
  for (const op of [OP + '.x', 'query.document.object.component', 'command.document.object.component.convert', 'query.document.object', '', null, undefined]) assert.equal(contract.admitted(op), false, String(op));
});

test('every selector is accepted and every one-member mutation is refused', () => {
  const good = [{ kind: 'contexts', cursor: 0, limit: 16 }, { kind: 'context', contextId: 'sym_a' }, { kind: 'layers', contextId: 'scene-root', cursor: 3, limit: 1 },
    { kind: 'instances', contextId: 'sym_a', cursor: 0, limit: 16 }];
  for (const selector of good) {
    contract.payload(OP, request(selector).payload);
    for (const key of Object.keys(selector).filter((k) => k !== 'kind')) {
      const missing = clone(selector); delete missing[key];
      assert.throws(() => contract.payload(OP, request(missing).payload), TypeError, selector.kind + ' without ' + key);
    }
    assert.throws(() => contract.payload(OP, request(Object.assign({}, selector, { extra: 1 })).payload), TypeError, selector.kind + ' extra');
  }
  const bad = [{ kind: 'contexts', cursor: 0, limit: 0 }, { kind: 'contexts', cursor: 0, limit: 17 }, { kind: 'contexts', cursor: -1, limit: 1 },
    { kind: 'contexts', cursor: 0.5, limit: 1 }, { kind: 'contexts', cursor: 9007199254740992, limit: 1 }, { kind: 'context', contextId: '' },
    { kind: 'context', contextId: 'x'.repeat(129) }, { kind: 'context', contextId: 'a\u0000b' }, { kind: 'context', contextId: 7 }, { kind: 'other' }, 'contexts', null];
  for (const selector of bad) assert.throws(() => contract.payload(OP, request(selector).payload), TypeError, JSON.stringify(selector));
  for (const patch of [{ atRevision: -1 }, { atRevision: 1.5 }, { documentSnapshotId: '' }, { documentSnapshotId: 'bad id' }, { extra: 1 }]) {
    assert.throws(() => contract.payload(OP, Object.assign({}, request(good[0]).payload, patch)), TypeError, JSON.stringify(patch));
  }
  assert.throws(() => contract.payload('query.document.object', request(good[0]).payload), TypeError);
});

test('results answer their own pin, selector and page, and the model invariants hold', () => {
  const run = (selector, mutate) => {
    const req = request(selector), plan = answer(req, {}), result = clone(plan.result);
    if (mutate) mutate(result);
    contract.result(OP, result, req.payload);
  };
  const contexts = { kind: 'contexts', cursor: 0, limit: 16 };
  run(contexts); run({ kind: 'context', contextId: 'sym_a' }); run({ kind: 'layers', contextId: 'sym_a', cursor: 0, limit: 16 }); run({ kind: 'instances', contextId: 'sym_a', cursor: 0, limit: 16 });
  run({ kind: 'context', contextId: 'scene-root' });
  run(contexts, (r) => { r.items[1].nameTruncated = true; }); // an oversized component name the owner cut
  const no = (selector, mutate, label) => assert.throws(() => run(selector, mutate), TypeError, label);
  no(contexts, (r) => { r.atRevision = 8; }, 'foreign revision');
  no(contexts, (r) => { r.documentSnapshotId = 'foreign'; }, 'foreign snapshot');
  no(contexts, (r) => { r.kind = 'layers'; }, 'foreign kind');
  no(contexts, (r) => { r.extra = true; }, 'extra member');
  no(contexts, (r) => { r.cursor = 1; }, 'cursor does not answer');
  no(contexts, (r) => { r.nextCursor = null; r.contextCount = 9; }, 'ended early');
  no(contexts, (r) => { r.nextCursor = 2; }, 'cursor skips');
  no(contexts, (r) => { r.items[0].contextId = 'sym_x'; r.items[0].contextKind = 'component'; }, 'first is the scene');
  no(contexts, (r) => { r.items[1].contextId = 'scene-root'; r.items[1].contextKind = 'scene'; }, 'scene only first');
  no(contexts, (r) => { r.items[0].name = 'Scene'; }, 'scene is unnamed');
  for (const flag of [false, 'true', 1, null]) no(contexts, (r) => { r.items[1].nameTruncated = flag; }, 'truncation flag is only ever true: ' + String(flag));
  no(contexts, (r) => { r.items[0].nameTruncated = true; }, 'the scene has no name to cut');
  no(contexts, (r) => { r.items[1].name = null; r.items[1].nameTruncated = true; }, 'an absent name is not truncated');
  no(contexts, (r) => { r.items.length = 3; r.items.push(...r.items.slice(0, 1)); r.contextCount = 1; }, 'beyond the total');
  no({ kind: 'contexts', cursor: 0, limit: 1 }, (r) => { r.items.push(clone(r.items[0])); r.contextCount = 5; r.nextCursor = 2; }, 'beyond the limit');
  no({ kind: 'context', contextId: 'sym_a' }, (r) => { r.contextId = 'sym_b'; }, 'other context');
  no({ kind: 'context', contextId: 'sym_a' }, (r) => { r.fps = 0; }, 'non-positive fps');
  no({ kind: 'context', contextId: 'sym_a' }, (r) => { r.hasCameraKeys = 'no'; }, 'camera flag type');
  no({ kind: 'context', contextId: 'sym_a' }, (r) => { r.instanceCount = -1; }, 'negative count');
  no({ kind: 'context', contextId: 'scene-root' }, (r) => { r.name = 'Scene'; }, 'scene summary is unnamed');
  no({ kind: 'layers', contextId: 'sym_a', cursor: 0, limit: 16 }, (r) => { r.items[0].layerKind = 'mystery'; }, 'layer kind');
  // The drawing-gate facts are optional booleans (the parent a uid or null); anything else is refused.
  for (const [key, value] of [['visible', 'yes'], ['locked', 1], ['threeD', null], ['hasMotion', 'no'], ['parentLayerUid', 7], ['parentLayerUid', '']]) {
    no({ kind: 'layers', contextId: 'sym_a', cursor: 0, limit: 16 }, (r) => { r.items[0][key] = value; }, 'layer fact ' + key + ' = ' + String(value));
  }
  run({ kind: 'layers', contextId: 'sym_a', cursor: 0, limit: 16 }, (r) => { Object.assign(r.items[0], { visible: false, locked: true, parentLayerUid: null, threeD: true, hasMotion: false }); }); // admitted
  no({ kind: 'layers', contextId: 'sym_a', cursor: 0, limit: 16 }, (r) => { r.contextId = 'sym_b'; }, 'layers of another context');
  no({ kind: 'instances', contextId: 'sym_a', cursor: 0, limit: 16 }, (r) => { r.items[0].extra = 1; }, 'closed instance item');
  assert.throws(() => contract.result('query.document.object', {}), TypeError);
});

test('the common v2 SDK admits the read without expectedRevision and judges request and response', async () => {
  const o = owner();
  const good = await o.sdk.dispatch(request({ kind: 'contexts', cursor: 0, limit: 16 }));
  assert.equal(good.ok, true);
  assert.deepEqual(good.result.items.map((item) => item.contextId), ['scene-root', 'sym_a', 'sym_b']);
  assert.throws(() => SDK.validateRequest(request({ kind: 'contexts', cursor: 0, limit: 16 }, { expectedRevision: 7 })), TypeError);
  assert.throws(() => SDK.validateRequest(request({ kind: 'contexts', cursor: 0, limit: 17 })), TypeError);
  assert.throws(() => SDK.validateRequest(request({ kind: 'context', contextId: 'a', cursor: 0 })), TypeError);
  // A forged reply never reaches the caller: wrong request, wrong pin, an extra member.
  for (const forge of [(r) => { r.requestId = 'other'; }, (r) => { r.contentRevision = 8; }, (r) => { r.result.atRevision = 8; }, (r) => { r.result.extra = true; },
    (r) => { r.documentId = 'document-B'; }]) {
    const bad = owner({ reply: (req, base) => { const plan = answer(req, {}), reply = Object.assign(base, { ok: true, result: plan.result }); forge(reply); return reply; } });
    await assert.rejects(() => bad.sdk.dispatch(request({ kind: 'contexts', cursor: 0, limit: 16 })), TypeError);
  }
});

test('read() sends one pinned literal request, returns a frozen detached answer and writes nothing', async () => {
  const o = owner();
  const out = await o.intent.read({ kind: 'context', contextId: 'sym_a' });
  assert.equal(o.state.sent.length, 1);
  assert.deepEqual(o.state.sent[0], { apiVersion: 2, requestId: 'component-read-1', instanceId: 'instance-A', documentId: 'document-A', operation: OP,
    payload: { atRevision: 7, documentSnapshotId: SNAP(7), selector: { kind: 'context', contextId: 'sym_a' } } });
  assert.equal('expectedRevision' in o.state.sent[0], false);
  assert.deepEqual([out.ok, out.operation, out.revision], [true, 'component.read', 7]);
  assert.deepEqual([out.result.contextId, out.result.name, out.result.fps, out.result.totalFrames, out.result.layerCount, out.result.instanceCount, out.result.exposedPropertyCount, out.result.hasCameraKeys],
    ['sym_a', 'Comp A', 24, 12, 2, 2, 1, false]);
  assert.equal(Object.isFrozen(out.result), true);
  assert.equal(o.state.revision, 7, 'a read advances nothing');
  const layers = await o.intent.read({ kind: 'layers', contextId: 'sym_a', cursor: 0, limit: 16 });
  assert.deepEqual(layers.result.items.map((row) => [row.layerUid, row.name, row.layerKind]), [['ia', 'Inner A', 'drawing'], ['ib', null, 'widget']]);
  const instances = await o.intent.read({ kind: 'instances', contextId: 'sym_a', cursor: 0, limit: 16 });
  assert.deepEqual(instances.result.items.map((row) => row.layerUid), ['L2', 'L3']);
});

test('typed refusals: unknown context, the scene has no instances, stale pin, invalid selector sends nothing', async () => {
  const o = owner();
  const unknown = await o.intent.read({ kind: 'context', contextId: 'sym_nope' });
  assert.deepEqual([unknown.ok, unknown.code], [false, 'not_found']);
  const scene = await o.intent.read({ kind: 'instances', contextId: 'scene-root', cursor: 0, limit: 4 });
  assert.deepEqual([scene.ok, scene.code], [false, 'invalid_request']);
  const stale = owner({ reply: (req, base) => Object.assign(base, { ok: false, error: { code: 'stale_revision', message: 'pin' } }) });
  const old = await stale.intent.read({ kind: 'contexts', cursor: 0, limit: 4 });
  assert.deepEqual([old.ok, old.code, old.stale], [false, 'stale_revision', true]);
  const before = o.state.sent.length;
  for (const selector of [{ kind: 'contexts', cursor: 0, limit: 99 }, { kind: 'nope' }, null, undefined]) {
    const bad = await o.intent.read(selector);
    assert.deepEqual([bad.ok, bad.code], [false, 'invalid_request'], JSON.stringify(selector));
  }
  assert.equal(o.state.sent.length, before, 'a selector the contract refuses is never sent');
});

test('an answer is discarded when the project moved on, and transport trouble is typed', async () => {
  const moved = owner({ moveAfter: 1 });
  const gone = await moved.intent.read({ kind: 'contexts', cursor: 0, limit: 4 });
  assert.deepEqual([gone.ok, gone.code, gone.stale], [false, 'stale_revision', true]);
  const dropped = await owner({ drop: 'socket closed' }).intent.read({ kind: 'contexts', cursor: 0, limit: 4 });
  assert.deepEqual([dropped.ok, dropped.code], [false, 'transport_failed']);
  const busy = owner({ busy: true });
  const b = await busy.intent.read({ kind: 'contexts', cursor: 0, limit: 4 });
  assert.deepEqual([b.ok, b.code, b.reason], [false, 'unavailable', 'native_project_busy']);
  assert.equal(busy.state.sent.length, 0);
  const forged = owner({ reply: (req, base) => Object.assign(base, { requestId: 'other', ok: true, result: {} }) });
  const f = await forged.intent.read({ kind: 'contexts', cursor: 0, limit: 4 });
  assert.deepEqual([f.ok, f.code], [false, 'transport_failed']);
  const lying = owner({ reply: (req, base) => { const plan = answer(req, {}); plan.result.extra = 1; return Object.assign(base, { ok: true, result: plan.result }); } });
  assert.equal((await lying.intent.read({ kind: 'contexts', cursor: 0, limit: 4 })).code, 'transport_failed');
});

test('absent ports, absent declaration and absent session are typed unavailability, never a legacy read', async () => {
  const none = component.createComponentIntent({ pin: () => null, declared: () => true, submit: async () => ({}), requestId: () => 'x-1' }, {});
  assert.deepEqual([(await none.read({ kind: 'contexts', cursor: 0, limit: 1 })).code, (await none.read({ kind: 'contexts', cursor: 0, limit: 1 })).reason], ['unavailable', 'component_read_unavailable']);
  const undeclared = await owner({ declared: false }).intent.read({ kind: 'contexts', cursor: 0, limit: 1 });
  assert.deepEqual([undeclared.code, undeclared.reason], ['unavailable', 'native_declaration_absent']);
  const nosession = await owner({ noSession: true }).intent.read({ kind: 'contexts', cursor: 0, limit: 1 });
  assert.deepEqual([nosession.code, nosession.reason], ['unavailable', 'no_presented_native_session']);
});

test('listContexts follows nextCursor across owner-cut pages and keeps the scene first', async () => {
  const o = owner({ pageSize: 1 });
  const out = await o.intent.listContexts();
  assert.equal(out.ok, true);
  assert.deepEqual(out.contexts.map((item) => item.contextId), ['scene-root', 'sym_a', 'sym_b']);
  assert.equal(o.state.sent.length, 3);
  assert.deepEqual(o.state.sent.map((r) => r.payload.selector.cursor), [0, 1, 2]);
  const endless = owner({ reply: (req, base) => { const plan = answer(req, {}); plan.result.nextCursor = plan.result.cursor + plan.result.items.length; plan.result.contextCount = 1e6; return Object.assign(base, { ok: true, result: plan.result }); } });
  assert.notEqual((await endless.intent.listContexts()).ok, true, 'a malformed owner cannot loop the client');
  const refused = await owner({ declared: false }).intent.listContexts();
  assert.deepEqual([refused.ok, refused.code], [false, 'unavailable']);
});

test('the session mapping reads through the single resource route and reconciles nothing', async () => {
  const log = { dispatched: [], reconciled: 0, routed: 0 };
  const info = { instanceId: 'instance-A', documentId: 'document-A', contentRevision: 7, documentSnapshotId: SNAP(7), layers: [] };
  const sdk = SDK.createNativeApplicationAdapter('component-session', { dispatch: (req) => {
    const plan = answer(req, {});
    return Promise.resolve({ apiVersion: 2, requestId: req.requestId, instanceId: 'instance-A', documentId: 'document-A', contentRevision: 7, ok: true, result: plan.result });
  } });
  const session = { isActive: () => true, info: () => info, requestId: (label) => 'project-' + label + '-1', blocksLegacy: () => true,
    runResourceCommand: (action) => { log.routed += 1; return Promise.resolve().then(() => action({ dispatch: (r) => { log.dispatched.push(clone(r)); return sdk.dispatch(r); }, reconcile: () => { log.reconciled += 1; return Promise.resolve(info); } })); } };
  const intent = component.createComponentIntent(core.portsFromSession(session), {});
  const out = await intent.read({ kind: 'context', contextId: 'sym_b' });
  assert.deepEqual([out.ok, out.result.contextId, out.result.name, out.result.fps], [true, 'sym_b', null, null]);
  assert.deepEqual([log.routed, log.dispatched.length, log.reconciled], [1, 1, 0]);
  const base = Object.assign({}, session); delete base.runResourceCommand;
  const none = await component.createComponentIntent(core.portsFromSession(base), {}).read({ kind: 'contexts', cursor: 0, limit: 1 });
  assert.deepEqual([none.ok, none.code, none.reason], [false, 'unavailable', 'native_declaration_absent']);
  const busy = Object.assign({}, session, { runResourceCommand: () => Promise.reject(new Error('Native font owner busy or unavailable')) });
  const b = await component.createComponentIntent(core.portsFromSession(busy), {}).read({ kind: 'contexts', cursor: 0, limit: 1 });
  assert.deepEqual([b.ok, b.code, b.reason], [false, 'unavailable', 'native_project_busy']);
});

test('the declaration, schema, composition and load order agree', () => {
  const descriptor = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/capabilities-v2/native-object-component-read.json'), 'utf8'));
  assert.deepEqual(descriptor.operations, [OP]);
  assert.equal(descriptor.availability.state, 'available');
  assert.equal(descriptor.resourceBoundary.maxReadResponseBytes, 4096);
  const schema = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/native-transport-v2.schema.json'), 'utf8'));
  const branch = schema.$defs.Request.oneOf.find((b) => b.allOf && b.allOf[1].properties.operation.const === OP);
  assert.deepEqual(branch.allOf[0], { $ref: '#/$defs/ForbiddenRevisionRequest' });
  assert.equal(branch.allOf[1].properties.payload.$ref, '#/$defs/ObjectComponentReadPayload');
  assert.ok(schema.$defs.ResponseBase.properties.result.oneOf.some((b) => b.$ref === '#/$defs/ObjectComponentReadResult'));
  const kinds = schema.$defs.ObjectComponentReadResult.oneOf.map((b) => b.properties.kind.const);
  assert.deepEqual(kinds, ['contexts', 'context', 'layers', 'instances', 'duplicator', 'instance']);
  assert.deepEqual(schema.$defs.ObjectComponentReadPayload.properties.selector.oneOf.map((b) => b.properties.kind.const), kinds);
  assert.deepEqual(descriptor.examples[0].request.payload.selector, { kind: 'contexts', cursor: 0, limit: 16 });
  contract.payload(OP, descriptor.examples[0].request.payload);
  contract.result(OP, descriptor.examples[0].response.result, descriptor.examples[0].request.payload);
  const manifest = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/script-manifest.json'), 'utf8')).entries.map((e) => e.src);
  const at = (name) => manifest.indexOf('js/adapters/' + name);
  assert.ok(at('native-object-component-read-contract.js') > 0 && at('native-object-component-read-contract.js') < at('native-application.js'));
  assert.ok(at('native-application.js') < at('native-object-component.js'));
  assert.equal(OP in {}, false);
  assert.equal(component.OPERATIONS.includes(OP), false, 'the read is not a command of the component adapter');
});

test('listContexts reads every page from ONE revision: a move between pages restarts the listing, never mixes two', async () => {
  const o = owner({ pageSize: 1, moveAtRead: (n) => n === 1 });
  const out = await o.intent.listContexts();
  assert.equal(out.ok, true);
  assert.deepEqual(out.contexts.map((item) => item.contextId), ['scene-root', 'sym_a', 'sym_b'], 'no context skipped or duplicated');
  assert.equal(out.revision, 8, 'reported under the revision the whole list was read at');
  const sent = o.state.sent.map((r) => [r.payload.selector.cursor, r.payload.atRevision]);
  assert.deepEqual(sent, [[0, 7], [0, 8], [1, 8], [2, 8]], 'the stale page 2 was never sent; the restart is whole and uniformly pinned');
  assert.ok(o.state.sent.slice(1).every((r) => r.payload.documentSnapshotId === SNAP(8)));
});

test('listContexts gives up after bounded restarts with a typed stale refusal, and sends no unpinned page', async () => {
  const o = owner({ pageSize: 1, moveAtRead: () => true });
  const out = await o.intent.listContexts();
  assert.deepEqual([out.ok, out.code, out.stale, out.reason], [false, 'stale_revision', true, 'component_list_unstable']);
  assert.equal(o.state.sent.length, 3, 'one first page per attempt, three attempts');
  assert.ok(o.state.sent.every((r) => r.payload.selector.cursor === 0));
  // A refusal that is not staleness is not retried.
  const refused = owner({ pageSize: 1, reply: (req, base) => Object.assign(base, { ok: false, error: { code: 'not_found', message: 'gone' } }) });
  const gone = await refused.intent.listContexts();
  assert.deepEqual([gone.ok, gone.code, gone.stale === true], [false, 'not_found', false]);
  assert.equal(refused.state.sent.length, 1);
});

test('layers and instances listings are pinned the same way', async () => {
  const o = owner({ pageSize: 1, moveAtRead: (n) => n === 1 });
  const layers = await o.intent.listLayers('sym_a');
  assert.equal(layers.ok, true);
  assert.deepEqual(layers.items.map((item) => item.layerUid), ['ia', 'ib']);
  const instances = await owner({ pageSize: 1 }).intent.listInstances('sym_a');
  assert.deepEqual(instances.items.map((item) => item.layerUid), ['L2', 'L3']);
  assert.equal((await owner({ pageSize: 1, moveAtRead: () => true }).intent.listLayers('sym_a')).code, 'stale_revision');
});

test('a context whose name the owner had to cut is listed, flagged, and paging continues past it', async () => {
  const cut = (req, base, state) => {
    const plan = answer(req, state);
    plan.result.items.forEach((item) => { if (item.contextId === 'sym_a') { item.name = 'Comp'; item.nameTruncated = true; } });
    return Object.assign(base, { ok: true, result: plan.result });
  };
  const o = owner({ pageSize: 1, reply: cut });
  const out = await o.intent.listContexts();
  assert.equal(out.ok, true);
  assert.deepEqual(out.contexts.map((item) => item.contextId), ['scene-root', 'sym_a', 'sym_b']);
  assert.deepEqual(out.contexts.map((item) => item.nameTruncated === true), [false, true, false]);
  assert.equal(out.contexts[1].name, 'Comp');
  // A forged flag is refused at the edge like any other unknown member.
  const forged = owner({ reply: (req, base, state) => { const plan = answer(req, state); plan.result.items[0].nameTruncated = true; return Object.assign(base, { ok: true, result: plan.result }); } });
  assert.equal((await forged.intent.listContexts()).code, 'transport_failed');
});
