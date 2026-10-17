'use strict';
// L04-N component intents: convert, place and expose are one closed typed request each; the read model, the StoryBoard chain and
// entering a component go through injected native ports and the real workspace tab. Every request crosses the REAL common v2 SDK.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const { createHarness, createSession } = require('./native-object-component-harness.cjs');
const component = require('../src/js/adapters/native-object-component.js');
const core = require('../src/js/adapters/native-object-component-intent.js');

const ROOT = path.resolve(__dirname, '..');
const P = 'command.document.object.';
const oracle = JSON.parse(fs.readFileSync(path.join(__dirname, 'native-object-component-fixtures', 'oracle.json'), 'utf8'));
const created = (uid) => ({ applied: true, historyEntriesAdded: 1, layerUid: uid || 'native-layer:8:0' });

function make(options, hooks) {
  const h = createHarness(options);
  return Object.assign(h, { intent: component.createComponentIntent(h.ports, hooks) });
}

test('convert sends one literal request in document order, de-duplicated, with the active context', async () => {
  const h = make({ step: () => ({ applied: true }) });
  const out = await h.intent.convert(['layer-c', 'layer-a', 'layer-c']);
  assert.equal(h.state.sent.length, 1);
  assert.deepEqual(h.state.sent[0], { apiVersion: 2, requestId: 'component-convert-1', instanceId: 'instance-A', documentId: 'document-A', operation: P + 'component.convert',
    expectedRevision: 7, payload: { command: 'object.component.convert', contextId: 'scene-root', layerUids: ['layer-a', 'layer-c'] } });
  assert.equal(out.ok, true); assert.equal(out.applied, true); assert.equal(out.revision, 8); assert.equal(out.synchronized, true);
  assert.equal(out.merged, true); assert.equal(out.instanceLayerUid, 'layer-a', 'a compact receipt means the first layer became the instance in place');
});

test('a single-layer convert reports the layer itself, a merge reports the instance it minted', async () => {
  const single = make({});
  const one = await single.intent.convertLayer('layer-b');
  assert.deepEqual(single.state.sent[0].payload.layerUids, ['layer-b']);
  assert.equal(one.merged, false); assert.equal(one.instanceLayerUid, 'layer-b'); assert.equal(one.layerUid, undefined);
  const merge = make({ step: () => ({ result: created('native-layer:8:0'), layers: ['native-layer:8:0', 'layer-c'] }) });
  const two = await merge.intent.convert(['layer-a', 'layer-b']);
  assert.equal(two.merged, true); assert.equal(two.layerUid, 'native-layer:8:0'); assert.equal(two.instanceLayerUid, 'native-layer:8:0');
});

test('convert follows the active context hook and defaults to the scene root', async () => {
  const inside = make({ step: () => ({ error: { code: 'unavailable', message: 'Close the component first.' } }) }, { activeContext: () => 'native-component:6:0' });
  const refused = await inside.intent.convert(['layer-a']);
  assert.equal(inside.state.sent[0].payload.contextId, 'native-component:6:0', 'the owner decides about a nested request, the adapter does not pre-refuse');
  assert.equal(refused.ok, false); assert.equal(refused.code, 'unavailable'); assert.equal(refused.message, 'Close the component first.');
  for (const hook of [() => 7, () => { throw new Error('x'); }, undefined]) {
    const h = make({}, hook ? { activeContext: hook } : {});
    await h.intent.convert(['layer-a']);
    assert.equal(h.state.sent[0].payload.contextId, 'scene-root');
  }
});

test('convert refuses stale layers and empty input before anything is sent', async () => {
  const h = make({});
  const stale = await h.intent.convert(['layer-a', 'layer-x']);
  assert.deepEqual([stale.ok, stale.code, stale.ids], [false, 'stale_selection', ['layer-x']]);
  for (const bad of [[], 'layer-a', null, [''], [7], undefined]) {
    const out = await h.intent.convert(bad);
    assert.deepEqual([out.ok, out.code], [false, 'invalid_request'], JSON.stringify(bad));
  }
  assert.equal(h.state.sent.length, 0);
  // The owner's typed refusal of a layer kind carries the baseline text through unchanged.
  const refused = make({ step: () => ({ error: { code: 'invalid_request', message: 'Impossible de convertir : un composant' } }) });
  const out = await refused.intent.convert(['layer-a']);
  assert.deepEqual([out.ok, out.code, out.message], [false, 'invalid_request', 'Impossible de convertir : un composant']);
});

test('place sends the literal request and reports the minted instance layer', async () => {
  const h = make({});
  const out = await h.intent.place('native-component:6:0', 'Hand');
  assert.deepEqual(h.state.sent[0].payload, { command: 'object.component.place', symbolId: 'native-component:6:0', name: 'Hand' });
  assert.equal(h.state.sent[0].operation, P + 'component.place');
  assert.deepEqual([out.ok, out.applied, out.layerUid], [true, true, 'native-layer:8:0']);
  const bad = await h.intent.place('', 'Hand');
  assert.deepEqual([bad.ok, bad.code], [false, 'invalid_request']);
  const unknown = make({ step: () => ({ error: { code: 'not_found', message: 'Component is absent.' } }) });
  assert.equal((await unknown.intent.place('native-component:9:9', 'X')).code, 'not_found');
});

test('expose makes the baseline key from the injected clock and random, for every baseline row', async () => {
  for (const row of oracle.expose) {
    assert.equal(component.exposedKey(() => row.clock, () => row.random), row.key, JSON.stringify([row.clock, row.random]));
  }
  const row = oracle.expose.find((r) => r.clock === 1788000000000 && r.random === 0.123456789);
  const h = make({}, { clock: () => row.clock, random: () => row.random });
  const out = await h.intent.expose('native-component:6:0', 'stroke-1', 'opacity', 'Eye', 100);
  assert.deepEqual(h.state.sent[0].payload, { command: 'object.component.expose', symbolId: 'native-component:6:0', key: row.key, targetStrokeId: 'stroke-1',
    targetField: 'opacity', label: 'Eye', default: 100 });
  assert.equal(h.state.sent[0].operation, P + 'component.expose');
  assert.deepEqual([out.ok, out.applied, out.layerUid], [true, true, undefined]);
});

test('expose keeps an absent default absent, a zero default zero, and refuses a null default locally', async () => {
  const absent = make({}, { clock: () => 1, random: () => 0.5 });
  await absent.intent.expose('s', 'stroke-1', '__visible', 'Show');
  assert.equal(Object.prototype.hasOwnProperty.call(absent.state.sent[0].payload, 'default'), false);
  assert.equal(oracle.defaults.absent, false, 'the baseline entry has no default member either');
  const zero = make({}, { clock: () => 1, random: () => 0.5 });
  await zero.intent.expose('s', 'stroke-1', 'opacity', 'Eye', 0);
  assert.equal(zero.state.sent[0].payload.default, 0);
  assert.equal(oracle.defaults.zero, 0);
  const nul = make({}, {});
  const out = await nul.intent.expose('s', 'stroke-1', 'opacity', 'Eye', null);
  assert.deepEqual([out.ok, out.code], [false, 'invalid_request']);
  assert.equal(nul.state.sent.length, 0);
});

test('expose reports an unusable clock or random source and a taken key as typed results', async () => {
  const broken = make({}, { clock: () => { throw new Error('no clock'); } });
  const out = await broken.intent.expose('s', 'stroke-1', 'opacity', 'Eye', 1);
  assert.deepEqual([out.ok, out.code, out.reason], [false, 'invalid_request', 'no clock']);
  const taken = make({ step: () => ({ error: { code: 'invalid_request', message: 'Exposed property key is taken.' } }) });
  const refused = await taken.intent.expose('s', 'stroke-1', 'opacity', 'Eye', 1);
  assert.deepEqual([refused.ok, refused.code, refused.message], [false, 'invalid_request', 'Exposed property key is taken.']);
});

test('unavailability is typed: no declaration, no session, an undeclared operation, and a request above 4096 bytes', async () => {
  const noDecl = make({ declared: () => false });
  const a = await noDecl.intent.place('s', 'Hand');
  assert.deepEqual([a.ok, a.code, a.reason], [false, 'unavailable', 'native_declaration_absent']);
  const noSession = make({ noSession: true });
  const b = await noSession.intent.convert(['layer-a']);
  assert.deepEqual([b.ok, b.code, b.reason], [false, 'unavailable', 'no_presented_native_session']);
  assert.equal(noSession.state.sent.length + noDecl.state.sent.length, 0);
  const huge = make({});
  const c = await huge.intent.place('s', 'n'.repeat(5000));
  assert.deepEqual([c.ok, c.code, c.limit], [false, 'request_too_large', 4096]);
  const bound = make({});
  const room = 4096 - Buffer.byteLength(JSON.stringify({ apiVersion: 2, requestId: 'component-place-1', instanceId: 'instance-A', documentId: 'document-A', operation: P + 'component.place',
    expectedRevision: 7, payload: { command: 'object.component.place', symbolId: 's', name: '' } }));
  assert.equal((await bound.intent.place('s', 'n'.repeat(room))).ok, true, 'exactly 4096 bytes is accepted');
  assert.equal((await make({}).intent.place('s', 'n'.repeat(room + 1))).code, 'request_too_large', '4097 bytes is not');
});

test('refusals, stale revisions and transport failures are classified, never retried', async () => {
  const stale = make({ step: () => ({ error: { code: 'stale_revision', message: 'stale' } }) });
  const a = await stale.intent.place('s', 'Hand');
  assert.deepEqual([a.ok, a.code, a.stale, a.revision], [false, 'stale_revision', true, 7]);
  const session = make({ sessionStyle: true, step: () => ({ error: { code: 'not_found', message: 'Component is absent.' } }) });
  const b = await session.intent.place('s', 'Hand');
  assert.deepEqual([b.ok, b.code, b.stale], [false, 'not_found', false]);
  const dropped = make({ step: () => ({ drop: 'socket closed' }) });
  const c = await dropped.intent.place('s', 'Hand');
  assert.deepEqual([c.ok, c.code, c.indeterminate], [false, 'transport_failed', true]);
  assert.equal(dropped.state.sent.length, 1, 'the failed request is not replayed');
  const uncorrelated = make({ rawResponse: () => ({ requestId: 'other', ok: true }) });
  assert.equal((await uncorrelated.intent.place('s', 'Hand')).indeterminate, true);
  // A well-formed receipt that answers ANOTHER request is never accepted as this request's outcome.
  const foreign = make({ rawResponse: () => ({ apiVersion: 2, requestId: 'someone-else', instanceId: 'instance-A', documentId: 'document-A', contentRevision: 8, ok: true, result: created() }) });
  const stray = await foreign.intent.place('s', 'Hand');
  assert.deepEqual([stray.ok, stray.code, stray.indeterminate, stray.message], [false, 'transport_failed', true, 'uncorrelated native response']);
  const noResult = make({ rawResponse: (r) => ({ requestId: r.requestId, ok: true }) });
  assert.equal((await noResult.intent.place('s', 'Hand')).indeterminate, true);
});

test('a success the contract cannot correlate is indeterminate even when the route bypassed the SDK', async () => {
  const reply = (patch) => (r) => ({ apiVersion: 2, requestId: r.requestId, instanceId: 'instance-A', documentId: 'document-A', contentRevision: 8, ok: true, result: created(), ...patch });
  const cases = [
    ['a compact receipt for an operation that mints a layer', { result: { applied: true, historyEntriesAdded: 1 } }],
    ['a revision that does not advance by the applied edit', { contentRevision: 7 }],
    ['a foreign instance', { instanceId: 'instance-B' }],
    ['a foreign document', { documentId: 'document-B' }],
    ['a receipt with an extra member', { result: { ...created(), document: {} } }],
  ];
  for (const [label, patch] of cases) {
    const h = make({ rawResponse: reply(patch) });
    const out = await h.intent.place('s', 'Hand');
    assert.deepEqual([out.ok, out.code, out.indeterminate], [false, 'transport_failed', true], label);
    assert.match(out.message, /uncorrelated native receipt/, label);
  }
  const good = make({ rawResponse: reply({}) });
  assert.equal((await good.intent.place('s', 'Hand')).ok, true);
  // A refusal committed nothing: it is classified even when it names another owner.
  const refusal = make({ rawResponse: (r) => ({ apiVersion: 2, requestId: r.requestId, instanceId: 'instance-B', documentId: 'document-B', contentRevision: 7, ok: false, error: { code: 'wrong_instance', message: 'x' } }) });
  const out = await refusal.intent.place('s', 'Hand');
  assert.deepEqual([out.ok, out.code, out.indeterminate], [false, 'wrong_instance', undefined]);
});

test('intents are serialized: each reads the pin its predecessor left', async () => {
  const h = make({});
  const first = h.intent.convert(['layer-a']);
  const second = h.intent.place('s', 'Hand');
  const third = h.intent.expose('s', 'stroke-1', 'opacity', 'Eye', 1);
  const outcomes = await Promise.all([first, second, third]);
  assert.deepEqual(h.state.sent.map((r) => r.expectedRevision), [7, 8, 9]);
  assert.deepEqual(outcomes.map((o) => o.revision), [8, 9, 10]);
});

test('a queued intent is judged against what its predecessor published, not against the pin at call time', async () => {
  const h = make({ step: (request) => (request.operation === P + 'component.place' ? { layers: ['layer-a', 'layer-b', 'layer-c', 'native-layer:8:0'] } : { applied: true }) });
  const place = h.intent.place('native-component:6:0', 'Hand');
  const convert = h.intent.convert(['native-layer:8:0']);
  const [placed, converted] = await Promise.all([place, convert]);
  assert.equal(placed.layerUid, 'native-layer:8:0');
  assert.deepEqual([converted.ok, converted.instanceLayerUid], [true, 'native-layer:8:0'], 'the instance placed a moment ago is a live layer for the next intent');
  assert.deepEqual(h.state.sent.map((r) => r.expectedRevision), [7, 8]);
});

test('the runner builds exactly the declared members: unknown, missing and optional members', async () => {
  const h = createHarness({});
  const runner = core.createIntentRunner(h.ports);
  const op = P + 'component.expose';
  const full = { symbolId: 's', key: 'ep_a_b', targetStrokeId: 'stroke-1', targetField: 'opacity', label: 'Eye' };
  for (const bad of [{ ...full, extra: 1 }, { symbolId: 's', key: 'ep_a_b', targetStrokeId: 'stroke-1', targetField: 'opacity' }, null, [], 'x']) {
    const out = await runner.run(op, bad, 'component-expose');
    assert.deepEqual([out.ok, out.code], [false, 'invalid_request'], JSON.stringify(bad));
  }
  assert.equal(h.state.sent.length, 0);
  assert.equal((await runner.run(op, full, 'component-expose')).ok, true, 'the optional default may be absent');
  assert.equal((await runner.run(op, { ...full, default: 0 }, 'component-expose')).ok, true);
  assert.deepEqual(h.state.sent[0].payload, { command: 'object.component.expose', ...full });
  assert.equal((await runner.run('command.document.object.layer.create', { name: 'x' }, 'x')).reason, 'operation_not_declared');
});

test('the read model is detached and frozen, and absent providers are typed unavailability', async () => {
  const none = make({});
  assert.deepEqual([(await none.intent.readModel()).ok, (await none.intent.readModel()).reason], [false, 'component_read_unavailable']);
  const source = { contexts: [{ contextId: 'scene-root' }, { contextId: 'native-component:6:0', name: 'Hand' }], storyboard: { modules: [{ id: 'sbm1' }] } };
  const h = make({}, { model: { read: async () => source } });
  const read = await h.intent.readModel();
  assert.equal(read.ok, true);
  assert.deepEqual(read.model, source);
  assert.notEqual(read.model, source);
  assert.equal(Object.isFrozen(read.model) && Object.isFrozen(read.model.contexts[1]), true);
  assert.throws(() => { read.model.contexts[1].name = 'changed'; }, TypeError);
  assert.equal(source.contexts[1].name, 'Hand', 'the provider state is never reachable through the copy');
  const board = await h.intent.storyboard();
  assert.deepEqual([board.ok, board.storyboard], [true, { modules: [{ id: 'sbm1' }] }]);
  const noBoard = make({}, { model: { read: async () => ({ contexts: [] }) } });
  assert.deepEqual([(await noBoard.intent.storyboard()).reason], ['storyboard_read_unavailable']);
  const failing = make({}, { model: { read: async () => { throw new Error('closed'); } } });
  const f = await failing.intent.readModel();
  assert.deepEqual([f.ok, f.reason, f.message], [false, 'component_read_failed', 'closed']);
  const invalid = make({}, { model: { read: async () => [1] } });
  assert.equal((await invalid.intent.readModel()).reason, 'component_read_invalid');
});

function workspace(active, extra) {
  const calls = [];
  const tabs = { active: () => active.value, enter: async (id) => { calls.push(['enter', id]); active.value = id; }, exit: async () => { calls.push(['exit']); active.value = 'scene-root'; } };
  const model = { read: async () => ({ contexts: [{ contextId: 'scene-root' }, { contextId: 'native-component:6:0' }, { contextId: 'native-component:6:1' }] }) };
  return { calls, hooks: Object.assign({ tabs, model }, extra) };
}

test('entering a component uses the real tab, once, and reports the idempotent split plan', async () => {
  const active = { value: 'scene-root' };
  const w = workspace(active, { splitPlan: async () => [] });
  const h = make({}, w.hooks);
  const out = await h.intent.enter('native-component:6:0');
  assert.deepEqual(w.calls, [['enter', 'native-component:6:0']]);
  assert.deepEqual([out.ok, out.entered, out.contextId, out.split], [true, true, 'native-component:6:0', { state: 'none', planned: 0 }]);
  const again = await h.intent.enter('native-component:6:0');
  assert.deepEqual([again.ok, again.entered, again.reason], [true, false, 'already_active']);
  assert.equal(w.calls.length, 1, 'a second entry opens no second tab');
  assert.equal(h.state.sent.length, 0, 'entering a context writes nothing to the document');
});

test('a non-empty split plan is reported as unavailable, never applied or faked', async () => {
  const w = workspace({ value: 'scene-root' }, { splitPlan: async () => [{ layerUid: 'inner-1' }, { layerUid: 'inner-2' }] });
  const h = make({}, w.hooks);
  const out = await h.intent.enter('native-component:6:0');
  assert.deepEqual(out.split, { state: 'unavailable', reason: 'component_split_transaction_absent', planned: 2 });
  assert.equal(out.entered, true);
  assert.equal(h.state.sent.length, 0);
  for (const [provider, reason] of [[undefined, 'split_plan_unavailable'], [async () => { throw new Error('x'); }, 'split_plan_failed'], [async () => ({}), 'split_plan_invalid']]) {
    const other = make({}, workspace({ value: 'scene-root' }, provider ? { splitPlan: provider } : {}).hooks);
    assert.deepEqual((await other.intent.enter('native-component:6:0')).split, { state: 'unavailable', reason });
  }
});

test('entering is refused for an unknown context, the scene root, a nested open and an absent tab or read model', async () => {
  const w = workspace({ value: 'native-component:6:1' });
  const h = make({}, w.hooks);
  const nested = await h.intent.enter('native-component:6:0');
  assert.deepEqual([nested.ok, nested.code, nested.reason, nested.message], [false, 'unavailable', 'nested_context', 'Close the component first.']);
  assert.equal(w.calls.length, 0);
  const root = workspace({ value: 'scene-root' });
  const r = make({}, root.hooks);
  for (const bad of ['native-component:9:9', 'scene-root']) {
    const out = await r.intent.enter(bad);
    assert.deepEqual([out.ok, out.code, out.reason], [false, 'not_found', 'unknown_context'], bad);
  }
  for (const bad of ['', 7, null, undefined]) assert.equal((await r.intent.enter(bad)).code, 'invalid_request');
  assert.equal(root.calls.length, 0);
  const noTabs = make({}, { model: root.hooks.model });
  assert.deepEqual([(await noTabs.intent.enter('native-component:6:0')).reason], ['workspace_tab_unavailable']);
  const noModel = make({}, { tabs: root.hooks.tabs });
  assert.deepEqual([(await noModel.intent.enter('native-component:6:0')).reason], ['component_read_unavailable']);
  const noContexts = make({}, { tabs: root.hooks.tabs, model: { read: async () => ({}) } });
  assert.deepEqual([(await noContexts.intent.enter('native-component:6:0')).reason], ['component_contexts_unavailable']);
  const failing = workspace({ value: 'scene-root' });
  failing.hooks.tabs.enter = async () => { throw new Error('tab strip closed'); };
  const failed = await make({}, failing.hooks).intent.enter('native-component:6:0');
  assert.deepEqual([failed.ok, failed.code, failed.message], [false, 'workspace_failed', 'tab strip closed']);
});

test('exit leaves a component through the same tab strip and is a no-op at the scene root', async () => {
  const active = { value: 'native-component:6:0' };
  const w = workspace(active);
  const h = make({}, w.hooks);
  const out = await h.intent.exit();
  assert.deepEqual([out.ok, out.exited, out.contextId], [true, true, 'scene-root']);
  assert.deepEqual(w.calls, [['exit']]);
  const again = await h.intent.exit();
  assert.deepEqual([again.ok, again.exited, again.reason], [true, false, 'already_scene']);
  assert.equal(w.calls.length, 1);
  assert.equal((await make({}, {}).intent.exit()).reason, 'workspace_tab_unavailable');
});

test('the session mapping uses the common resource route and reconciles with the receipt revision', async () => {
  for (const route of ['runResourceCommand', 'runFontResource']) {
    const s = createSession({ route });
    const intent = component.createComponentIntent(core.portsFromSession(s.session));
    const out = await intent.place('native-component:6:0', 'Hand');
    assert.deepEqual([out.ok, out.requestId], [true, 'project-component-place-1'], route);
    assert.equal(s.log.dispatched.length, 1);
    assert.deepEqual(s.log.reconciled, [8], 'the session is synchronized to the revision the receipt names');
  }
  const refused = createSession({ reply: (r) => ({ apiVersion: 2, requestId: r.requestId, instanceId: 'instance-A', documentId: 'document-A', contentRevision: 7, ok: false, error: { code: 'not_found', message: 'x' } }) });
  const out = await component.createComponentIntent(core.portsFromSession(refused.session)).place('s', 'Hand');
  assert.deepEqual([out.ok, out.code], [false, 'not_found']);
  assert.deepEqual(refused.log.reconciled, [null], 'a refusal reconciles without a revision pin');
  const dropped = createSession({ dispatchThrows: 'socket closed' });
  const lost = await component.createComponentIntent(core.portsFromSession(dropped.session)).place('s', 'Hand');
  assert.deepEqual([lost.ok, lost.code, lost.indeterminate], [false, 'transport_failed', true]);
  assert.equal(dropped.log.reconciled.length, 1, 'a transport error resynchronizes before it is surfaced');
});

test('a session without the resource route, an inactive session and a busy route are typed unavailability', async () => {
  const none = createSession({ route: null });
  const a = await component.createComponentIntent(core.portsFromSession(none.session)).place('s', 'Hand');
  assert.deepEqual([a.ok, a.code, a.reason], [false, 'unavailable', 'native_declaration_absent']);
  const off = createSession({ active: false });
  const b = await component.createComponentIntent(core.portsFromSession(off.session)).place('s', 'Hand');
  assert.deepEqual([b.ok, b.code, b.reason], [false, 'unavailable', 'native_declaration_absent']);
  const busy = createSession({ busy: true });
  const c = await component.createComponentIntent(core.portsFromSession(busy.session)).place('s', 'Hand');
  assert.deepEqual([c.ok, c.code, c.reason], [false, 'unavailable', 'native_project_busy']);
  assert.equal(busy.log.dispatched.length, 0, 'a route that never started the action sent nothing');
});

test('desktop() binds once per host and only while the native Project session owns the document', () => {
  const s = createSession({});
  const host = { NemoNativeObjectProject: { desktop: () => s.session } };
  const first = component.desktop(host, {});
  assert.equal(typeof first.convert, 'function');
  assert.equal(component.desktop(host, {}), first);
  const idle = createSession({ blocks: false });
  assert.equal(component.desktop({ NemoNativeObjectProject: { desktop: () => idle.session } }, {}), null);
  assert.equal(component.desktop({}, {}), null);
  assert.equal(component.desktop(null, {}), null);
});

test('no legacy writer or legacy state is reachable: spies on every baseline writer stay silent and the sources never name them', async () => {
  const names = ['convertLayerToComponent', 'convertLayersToComponent', 'placeComponentInstance', 'exposeSymbolProperty', 'enterSymbol', 'exitToScene',
    'enterComponentLayer', 'splitLayerIntoElements', 'splitLayerIntoElementsCore', 'maybeAutoConvertToComponent', 'toggleLayerDuplicator',
    'convertSelectionToObjectDuplicator', 'createComponentFromForeignProject'];
  const hits = [];
  names.forEach((name) => { globalThis[name] = () => { hits.push(name); }; });
  globalThis.SMStoryboard = new Proxy({}, { get: (_, key) => { hits.push('SMStoryboard.' + String(key)); return () => {}; } });
  try {
    const w = workspace({ value: 'scene-root' }, { splitPlan: async () => [{}] });
    const h = make({ step: () => ({ result: created('native-layer:8:0') }) }, w.hooks);
    await h.intent.convert(['layer-a', 'layer-b']); await h.intent.place('s', 'Hand'); await h.intent.expose('s', 'stroke-1', 'opacity', 'Eye', 1);
    await h.intent.enter('native-component:6:0'); await h.intent.exit(); await h.intent.storyboard();
  } finally { names.forEach((name) => { delete globalThis[name]; }); delete globalThis.SMStoryboard; }
  assert.deepEqual(hits, []);
  for (const file of ['native-object-component.js', 'native-object-component-intent.js', 'native-object-component-contract.js']) {
    const text = fs.readFileSync(path.join(ROOT, 'src/js/adapters', file), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '');
    for (const name of names.concat(['SMStoryboard', 'state.symbols', 'state.layers', 'storyboard.js', 'Paper', 'paper.', 'activeSymbolId'])) {
      assert.equal(text.includes(name), false, file + ' must not name ' + name);
    }
  }
});

test('animating a flat layer never converts it: nothing but the explicit request names a conversion', () => {
  for (const file of ['native-object-component.js', 'native-object-duplicator.js']) {
    const text = fs.readFileSync(path.join(ROOT, 'src/js/adapters', file), 'utf8');
    assert.equal(/Motion|toggleAnimated|setValue|keyframe/.test(text.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '')), false, file + ' has no Motion write path');
  }
});
