'use strict';
// #2358 B-comp-edit-rest: the native tween commands inside an ENTERED component (baseline enterSymbol swapped state.layers for the component's,
// so Tween / Regenerate / Split / Reassign work on its layers). The real adapter and the real command / contract run against a scripted session
// with a scene route and a component route: a held component sends its context id over the COMPONENT route, the scene keeps the scene route and
// no contextId, the layer comes from the entered rows, and a component that changed (or a missing route) is refused with nothing sent.
const test = require('node:test'), assert = require('node:assert/strict');
const Tween = require('../src/js/adapters/native-object-tween.js');

const OPTIONS = { resamplePoints: 50, step: 1, skipManual: true, easing: [{ x: 0, y: 0 }, { x: 1, y: 1 }] };
function session(options = {}) {
  const sent = [], routes = [];
  let revision = 4;
  const info = () => ({ instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: revision, layers: [{ layerUid: 'S1' }, { layerUid: 'S2' }] });
  const lease = () => ({
    dispatch: async (request) => { sent.push(request); revision += 1; return { apiVersion: 2, requestId: request.requestId, instanceId: 'i', documentId: 'd', ok: true, contentRevision: revision, result: { applied: true, historyEntriesAdded: 1 } }; },
    reconcile: async () => info(),
  });
  const s = { info, isActive: () => true, requestId: (label) => label + '-' + (sent.length + 1), runResourceCommand: async (action) => { routes.push('scene'); return action(lease()); } };
  if (!options.noComponentRoute) s.runComponentCommand = async (action) => { routes.push('component'); if (options.gate) await options.gate; return action(lease()); };
  return { s, sent, routes };
}
const held = (state) => () => state.at;

test('a held component sends its context id over the component route, for the entered layer, one request per command', async () => {
  const f = session(), state = { at: { contextId: 'comp-a', rows: [{ layerUid: 'in-1' }, { layerUid: 'in-2' }] } };
  const tween = Tween.create(f.s, held(state));
  assert.equal(tween.layerUid(1), 'in-2', 'the entered rows, not the scene layers');
  assert.equal(tween.layerUid(5), null);
  assert.equal(tween.declared('command.document.object.tween.generate'), true);
  assert.equal((await tween.generate('in-1', { selection: [2] }, OPTIONS)).applied, true);
  assert.equal((await tween.regenerate('in-1', { starts: [0] }, OPTIONS)).applied, true);
  assert.equal((await tween.split('in-2', 3, OPTIONS)).applied, true);
  assert.equal((await tween.reassign('in-1', { from: 0, to: 4, overrides: [] }, OPTIONS)).applied, true);
  assert.deepEqual(f.routes, ['component', 'component', 'component', 'component'], 'never the scene resource route');
  assert.deepEqual(f.sent.map((r) => [r.operation.split('.').pop(), r.payload.layerUid, r.payload.contextId]),
    [['generate', 'in-1', 'comp-a'], ['regenerate', 'in-1', 'comp-a'], ['split', 'in-2', 'comp-a'], ['reassign', 'in-1', 'comp-a']]);
});

test('at the scene root nothing changes: the scene route, the scene layers and no context id', async () => {
  const f = session(), tween = Tween.create(f.s, held({ at: null }));
  assert.equal(tween.layerUid(1), 'S2');
  await tween.generate('S1', {}, OPTIONS);
  assert.deepEqual(f.routes, ['scene']);
  assert.equal('contextId' in f.sent[0].payload, false);
  const bare = Tween.create(f.s);
  assert.equal(bare.layerUid(0), 'S1', 'an adapter built without the component port is the scene adapter');
});

test('a missing component route and a component that changed before dispatch are refused with nothing sent', async () => {
  const none = session({ noComponentRoute: true }), state = { at: { contextId: 'comp-a', rows: [{ layerUid: 'in-1' }] } };
  const tween = Tween.create(none.s, held(state));
  assert.equal(tween.declared('command.document.object.tween.generate'), false, 'no component route: not declared here');
  await assert.rejects(tween.generate('in-1', {}, OPTIONS), (error) => error.code === 'unavailable' && error.reason === 'component_route_unavailable');
  assert.deepEqual([none.routes, none.sent], [[], []]);
  let open; const gate = new Promise((resolve) => { open = resolve; });
  const f = session({ gate }), moving = { at: { contextId: 'comp-a', rows: [{ layerUid: 'in-1' }] } };
  const moved = Tween.create(f.s, held(moving));
  const pending = moved.generate('in-1', {}, OPTIONS);
  moving.at = { contextId: 'comp-b', rows: [{ layerUid: 'in-1' }] }; // the user switched component before the queued command ran
  open();
  await assert.rejects(pending, (error) => error.code === 'stale_origin');
  assert.equal(f.sent.length, 0);
});
