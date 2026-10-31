'use strict';
// #2256 component writes (JS): the vector client builds every create/replace/delete target from the context the workspace
// controller holds, sends it ONLY through the component command route, and leaves every other family on the scene root.
const test = require('node:test'), assert = require('node:assert/strict');
const Vector = require('../src/js/adapters/native-object-vector.js');
const Contract = require('../src/js/adapters/native-object-vector-contract.js');
const Context = require('../src/js/adapters/native-component-context.js');
const Application = require('../src/js/adapters/native-application.js');

const SEG = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const DICT = () => ({ segments: [SEG(0, 0), SEG(10, 10)], closed: false, strokeColor: '#112233ff', strokeWidth: 2, strokeCap: 'round', strokeJoin: 'miter', miterLimit: 10, opacity: 1, fillColor: null });
const pin = { instanceId: 'i', documentId: 'd', contentRevision: 3, documentSnapshotId: 's', lifecycleGeneration: 1 };

function session() {
  const routes = [], sent = [];
  const run = (route) => (action) => { routes.push(route); return Promise.resolve(action({
    dispatch: async (request) => {
      sent.push(request);
      const p = request.payload, ids = [];
      if (p.command === 'object.vector.batch') p.ops.forEach((op, n) => ids.push(op.op === 'create' ? 'native-stroke:4:' + n : (op.stableTarget || op.record.target).strokeId));
      const result = p.command === 'object.vector.batch' ? { applied: true, historyEntriesAdded: 1, operation: 'batch', strokeId: ids[0], strokeIds: ids }
        : p.command === 'object.vector.createMany' ? { applied: true, historyEntriesAdded: 1, operation: 'createMany', strokeId: 'native-stroke:4:0', strokeIds: p.records.map((_, n) => 'native-stroke:4:' + n) }
        : p.command === 'object.vector.create' ? { applied: true, historyEntriesAdded: 1, operation: 'create', strokeId: 'native-stroke:4:0' }
          : p.command === 'object.vector.delete' ? { applied: true, historyEntriesAdded: 1, operation: 'delete', strokeId: p.stableTarget.strokeId }
            : { applied: true, historyEntriesAdded: 1, operation: 'replace', strokeId: p.record.target.strokeId };
      return { apiVersion: 2, requestId: request.requestId, instanceId: 'i', documentId: 'd', contentRevision: 4, ok: true, result };
    },
    reconcile: async () => pin,
  })); };
  let serial = 0;
  return { routes, sent, info: () => pin, isActive: () => true, requestId: (label) => label + '-' + (++serial), runResourceCommand: run('scene'), runComponentCommand: run('component') };
}

test('create, replace, delete and a batch inside the entered component carry its context and use the component route only', async () => {
  const s = session(), vector = Vector.create(s, () => 'comp-a');
  assert.equal((await vector.createStroke('inner', 2, DICT())).status, 'applied');
  assert.equal(s.sent.at(-1).payload.record.target.contextId, 'comp-a');
  assert.equal((await vector.deleteStroke({ layerUid: 'inner', frame: 2, strokeId: 'c-a' })).status, 'applied');
  assert.equal(s.sent.at(-1).payload.stableTarget.contextId, 'comp-a');
  assert.equal((await vector.replaceStroke({ layerUid: 'inner', frame: 2, strokeId: 'c-a' }, DICT())).status, 'applied');
  assert.equal(s.sent.at(-1).payload.record.target.contextId, 'comp-a');
  const batch = await vector.batchStrokes('inner', 2, [{ create: DICT() }, { delete: 'c-a' }]);
  assert.equal(batch.status, 'applied');
  assert.deepEqual(s.sent.at(-1).payload.ops.map((op) => (op.record ? op.record.target : op.stableTarget).contextId), ['comp-a', 'comp-a']);
  assert.equal((await vector.createStrokes('inner', 2, [DICT(), DICT()])).status, 'applied');
  assert.deepEqual(s.sent.at(-1).payload.records.map((r) => r.target.contextId), ['comp-a', 'comp-a']);
  assert.deepEqual([...new Set(s.routes)], ['component'], 'nothing went through the scene route');
});

test('the scene root is unchanged: scene target, scene route', async () => {
  const s = session(), vector = Vector.create(s, () => 'scene-root'), plain = Vector.create(s);
  await vector.createStroke('L1', 0, DICT());
  await plain.createStroke('L1', 0, DICT());
  assert.deepEqual(s.sent.map((r) => r.payload.record.target.contextId), ['scene-root', 'scene-root']);
  assert.deepEqual(s.routes, ['scene', 'scene']);
});

test('a session without the component route is unavailable inside a component, never silently the scene route', async () => {
  const s = session(); delete s.runComponentCommand;
  const result = await Vector.create(s, () => 'comp-a').createStroke('inner', 0, DICT());
  assert.deepEqual([result.status, result.reason], ['unavailable', 'component_route_unavailable']);
  assert.deepEqual([s.sent.length, s.routes.length], [0, 0]);
});

test('families that are not made context-aware keep the scene root and the scene route inside a component', async () => {
  const s = session(), vector = Vector.create(s, () => 'comp-a');
  await vector.setParametric({ layerUid: 'inner', frame: 0, strokeId: 'c-a' }, { kind: 'rect', tl: 0, tr: 0, br: 0, bl: 0, box: { x1: 0, y1: 0, x2: 4, y2: 4 } }).catch(() => null);
  await vector.editMask({ layerUid: 'inner', frame: 0, strokeId: 'c-a' }, { kind: 'unset' }).catch(() => null);
  assert.ok(s.sent.length >= 1);
  s.sent.forEach((request) => assert.equal((request.payload.stableTarget || {}).contextId, 'scene-root'));
  assert.ok(!s.routes.includes('component'));
});

test('the vector contract admits a context id, never free text, and a batch stays on one context', () => {
  const target = (contextId) => ({ contextId, frameScope: { kind: 'authored', frame: 0 }, layerUid: 'inner', strokeId: 'c-a' });
  const del = (contextId) => ({ command: 'object.vector.delete', stableTarget: target(contextId) });
  Contract.payload(Contract.VECTOR, del('scene-root'));
  Contract.payload(Contract.VECTOR, del('comp-a'));
  Contract.payload(Contract.VECTOR, del('component:c_1.v2/x'));
  for (const bad of ['', 'two words', 'a;b', 'x'.repeat(129), 7, null]) assert.throws(() => Contract.payload(Contract.VECTOR, del(bad)), /contextId/, String(bad));
  const batch = (a, b) => ({ command: 'object.vector.batch', ops: [{ op: 'delete', stableTarget: target(a) }, { op: 'delete', stableTarget: target(b) }] });
  Contract.payload(Contract.VECTOR, batch('comp-a', 'comp-a'));
  assert.throws(() => Contract.payload(Contract.VECTOR, batch('comp-a', 'scene-root')), /one context/);
});

test('the application client admits an object target in a component context but not free text', () => {
  const request = (contextId) => ({ apiVersion: 2, requestId: 'r', instanceId: 'i', documentId: 'd', operation: 'command.document.object.vector.edit', expectedRevision: 1,
    payload: { command: 'object.vector.delete', stableTarget: { contextId, frameScope: { kind: 'authored', frame: 0 }, layerUid: 'inner', strokeId: 'c-a' } } });
  Application.validateRequest(request('comp-a'));
  assert.throws(() => Application.validateRequest(request('not an id')));
});

test('the workspace controller holds the context: the scene root until a component is entered, and its own rows inside', () => {
  const host = { state: {} };
  assert.equal(Context.contextId(host), 'scene-root');
  assert.equal(Context.layerRows(host), null);
  assert.equal(Context.frameBound(host, { totalFrames: 24 }), 24);
});
